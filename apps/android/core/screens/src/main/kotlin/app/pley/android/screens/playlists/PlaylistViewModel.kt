@file:OptIn(ExperimentalCoroutinesApi::class)

package app.pley.android.screens.playlists

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import app.pley.android.common.UiText
import app.pley.android.data.Loadable
import app.pley.android.data.map
import app.pley.android.data.valueOrNull
import app.pley.android.model.ItemSummary
import app.pley.android.model.PlaylistDetail
import app.pley.android.model.PlaylistKind
import app.pley.android.model.format.formatDuration
import app.pley.android.player.PlayQueue
import app.pley.android.player.PlayRequest
import app.pley.android.screens.PlayerLauncher
import app.pley.android.screens.PleyKey
import app.pley.android.screens.library.SmartFilter
import app.pley.android.screens.library.TileFactory
import app.pley.android.screens.library.TileLook
import app.pley.android.screens.search.SearchViewModel
import java.util.IdentityHashMap
import kotlin.random.Random
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.Job
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOn
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.receiveAsFlow
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

/**
 * A playlist as its page shows it (apps/web/src/pages/playlists/PlaylistPage.tsx;
 * design spec §4.9): what it plays, its Smart badge, its title, its count
 * and length with its filter in words, then its entries.
 */
data class PlaylistPage<T : Any>(
    val id: Long,
    val title: String,
    /** Plays movies and episodes; else music. */
    val video: Boolean,
    /** Filled by its filter: read-only, its entries never moved or removed. */
    val smart: Boolean,
    /** [metaTexts] in English, joined: the TV's, until it shows them. */
    val meta: String,
    /** [noteTexts] in English: the TV's, until it shows them. */
    val notes: List<String>,
    /** The server's addresses of its first entries' pictures, for the mosaic. */
    val artwork: List<String>,
    val origin: String,
    /** The first entry, whose backdrop the TV page shows. */
    val lead: ItemSummary?,
    val rows: List<PlaylistRow<T>>,
    /** "12 items", "3h 12m" ([meta] as it is, by default). */
    val metaTexts: List<UiText> = listOfNotNull(meta.takeIf { it.isNotEmpty() }?.let(UiText::raw)),
    /** A smart playlist's filter in words: "Genre: Comedy", "Newest first" ([notes] as they are, by default). */
    val noteTexts: List<UiText> = notes.map(UiText::raw),
) {
    val editable: Boolean get() = !smart
    val canPlay: Boolean get() = rows.isNotEmpty()
    val canShuffle: Boolean get() = rows.size >= 2
}

/**
 * An entry: its position from 1, its two lines (the web's), its length,
 * and its thumbnail ([tile]: a 16:9 still for video, the square cover for
 * music, with how much was played).
 */
data class PlaylistRow<T : Any>(
    val entryId: Long,
    val item: ItemSummary,
    val number: Int,
    val title: String,
    val subtitle: String,
    val duration: String,
    val tile: T,
)

/** What happened on the page, for a toast or a move. */
sealed interface PlaylistEvent {
    /** [title]'s entry left the playlist; [PlaylistViewModel.undo] puts it back for a few seconds. */
    data class Removed(val entryId: Long, val title: String) : PlaylistEvent

    /** The playlist is gone: back to the playlists. */
    data object Deleted : PlaylistEvent

    /** A music entry opens its album, which plays it. */
    data class Open(val key: PleyKey) : PlaylistEvent

    data class Failed(val error: Throwable) : PlaylistEvent
}

/**
 * One playlist's page, shared by phones and TVs.
 *
 * - **Play and Shuffle.** Video plays as a queue of the playlist
 *   ([PlayQueue.Playlist]) from its first entry, or from a random one;
 *   the player comes back here when it closes. Music opens the entry's
 *   album, which plays it.
 * - **Rename and Delete** (with the web's confirmation, asked by the
 *   page).
 * - **Moving entries** up, down, or anywhere (a drag on phones, a move
 *   mode on TVs): the new order shows at once and goes to the server; when
 *   the server refuses, the order it had comes back and [PlaylistEvent.Failed]
 *   says why. Moves made while one is on its way are sent after it, the
 *   latest order only.
 * - **Removing entries:** the entry goes at once; with [remove]'s undo it
 *   is sent once the undo has had its time ([undoMs]), else straight away.
 *   A refusal brings it back.
 *
 * Smart playlists are read-only: their entries come from their filter.
 * Changes run in [PlaylistDeps.work], so they finish after the page
 * closes.
 */
class PlaylistViewModel<T : Any>(
    val playlistId: Long,
    private val deps: PlaylistDeps,
    private val player: PlayerLauncher,
    private val tiles: TileFactory<T>,
    private val random: Random = Random.Default,
    private val undoMs: Long = UNDO_MS,
) : ViewModel() {
    private val retries = MutableStateFlow(0)
    private val eventChannel = Channel<PlaylistEvent>(Channel.BUFFERED)

    /** Entries' order as moved here, until the server has it; `null` for the server's. */
    private val order = MutableStateFlow<List<Long>?>(null)

    /** Entries removed here whose removal hasn't been sent or answered yet. */
    private val hidden = MutableStateFlow<Set<Long>>(emptySet())

    /** Removals waiting for their undo to run out, by entry. */
    private val pending = HashMap<Long, Job>()

    /** Orders to send, the latest only. */
    private val orders = Channel<List<Long>>(Channel.CONFLATED)

    /** Toasts and moves: removed (with Undo), deleted, a failure, an album to open. */
    val events: Flow<PlaylistEvent> = eventChannel.receiveAsFlow()

    val state: StateFlow<Loadable<PlaylistPage<T>>> = retries
        .flatMapLatest {
            combine(
                deps.playlists.playlist(playlistId),
                deps.origin.map { it.orEmpty() }.distinctUntilChanged(),
                order,
                hidden,
            ) { load, origin, order, hidden -> load.map { page(it, origin, order, hidden) } }
        }
        .flowOn(deps.compute)
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(PlaylistsViewModel.STOP_AFTER_MS), Loadable.Loading)

    init {
        deps.work.launch {
            for (ids in orders) send(ids)
        }
    }

    private val page: PlaylistPage<T>? get() = state.value.valueOrNull

    fun retry() = retries.update { it + 1 }

    /** Plays from the entry at [from], or from a random one when [shuffle]. */
    fun play(from: Int = 0, shuffle: Boolean = false) {
        val page = page ?: return
        val rows = page.rows
        if (rows.isEmpty()) return
        val start = if (shuffle) rows[random.nextInt(rows.size)] else rows.getOrNull(from) ?: return
        if (page.video) {
            player.play(PlayRequest(itemId = start.item.id, queue = PlayQueue.Playlist(playlistId)))
        } else {
            eventChannel.trySend(PlaylistEvent.Open(SearchViewModel.opens(start.item)))
        }
    }

    /** Where the entry's title goes: its item's page (a track's album). */
    fun opens(row: PlaylistRow<T>): PleyKey = SearchViewModel.opens(row.item)

    /**
     * Moves the entry at [from] to [to]: shown at once, and sent unless
     * [send] is false (a TV's move mode sends once, when it ends).
     */
    fun move(from: Int, to: Int, send: Boolean = true) {
        val ids = shownOrder() ?: return
        if (from !in ids.indices || to !in ids.indices || from == to) return
        val next = Playlists.moved(ids, from, to)
        order.value = next
        if (send) orders.trySend(next)
    }

    /** Moves [entryId] [by] places (-1 up, 1 down), from where it is now, moves not yet drawn included. */
    fun moveBy(entryId: Long, by: Int, send: Boolean = true) {
        val from = shownOrder()?.indexOf(entryId) ?: return
        if (from >= 0) move(from, from + by, send)
    }

    /** The entries' order shown, including moves the page hasn't drawn yet; `null` when they can't move. */
    private fun shownOrder(): List<Long>? {
        val page = page ?: return null
        if (!page.editable) return null
        val gone = hidden.value
        return (order.value ?: page.rows.map { it.entryId }).filterNot { it in gone }
    }

    /** Sends the order shown now (after moves made with `send = false`). */
    fun sendOrder() {
        order.value?.let { orders.trySend(it) }
    }

    /** Forgets moves not sent, back to the order the server has. */
    fun cancelMoves() {
        order.value = null
    }

    /**
     * Removes [entryId]. With [undoable], it is sent after [undoMs], unless
     * [undo] comes first; [PlaylistEvent.Removed] offers that.
     */
    fun remove(entryId: Long, undoable: Boolean) {
        val page = page ?: return
        if (!page.editable || entryId in pending) return
        val row = page.rows.firstOrNull { it.entryId == entryId } ?: return
        hidden.update { it + entryId }
        if (undoable) {
            pending[entryId] = deps.work.launch {
                delay(undoMs)
                sendRemoval(entryId)
            }
            eventChannel.trySend(PlaylistEvent.Removed(entryId, row.title))
        } else {
            deps.work.launch { sendRemoval(entryId) }
        }
    }

    /** Puts back an entry [remove]d with an undo, while it hasn't been sent. */
    fun undo(entryId: Long) {
        val job = pending.remove(entryId) ?: return
        job.cancel()
        hidden.update { it - entryId }
    }

    /** Renames the playlist; `false` (and nothing sent) when [title] won't do. */
    fun rename(title: String): Boolean {
        if (Playlists.check(title) != null) return false
        val name = title.trim()
        if (name == page?.title) return true
        deps.work.launch { attempt { deps.playlists.rename(playlistId, name) } }
        return true
    }

    /** Deletes the playlist (the page has asked first); [PlaylistEvent.Deleted] once it is gone. */
    fun delete() {
        deps.work.launch {
            if (attempt { deps.playlists.delete(playlistId) }) eventChannel.trySend(PlaylistEvent.Deleted)
        }
    }

    override fun onCleared() {
        // Removals still waiting for their undo go now.
        val waiting = pending.keys.toList()
        pending.values.forEach { it.cancel() }
        pending.clear()
        waiting.forEach { id -> deps.work.launch { sendRemoval(id) } }
        orders.close()
    }

    private suspend fun send(ids: List<Long>) {
        if (attempt { deps.playlists.reorder(playlistId, ids) }) {
            // The server's answer holds this order; a newer move keeps its own.
            order.compareAndSet(ids, null)
        } else {
            order.value = null
        }
    }

    private suspend fun sendRemoval(entryId: Long) {
        pending.remove(entryId)
        try {
            attempt { deps.playlists.remove(playlistId, listOf(entryId)) }
        } finally {
            hidden.update { it - entryId }
        }
    }

    /** Runs [change]; `false` after telling of its failure. */
    private suspend fun attempt(change: suspend () -> Unit): Boolean = try {
        change()
        true
    } catch (e: CancellationException) {
        throw e
    } catch (e: Exception) {
        eventChannel.trySend(PlaylistEvent.Failed(e))
        false
    }

    /** Thumbnails made, by the item they show, so a new answer only makes what is new. */
    private var made = IdentityHashMap<ItemSummary, T>()
    private var madeFor = ""

    private fun page(detail: PlaylistDetail, origin: String, order: List<Long>?, hidden: Set<Long>): PlaylistPage<T> {
        if (madeFor != origin) {
            made = IdentityHashMap()
            madeFor = origin
        }
        val video = detail.kind != PlaylistKind.Audio
        val look = if (video) TileLook.Landscape else TileLook.Square
        val gone = detail.entries.filter { it.entryId in hidden }
        var entries = detail.entries.filterNot { it.entryId in hidden }
        if (order != null) {
            val at = order.withIndex().associate { (i, id) -> id to i }
            entries = entries.sortedBy { at[it.entryId] ?: Int.MAX_VALUE }
        }
        val next = IdentityHashMap<ItemSummary, T>()
        val rows = entries.mapIndexed { i, entry ->
            val item = entry.item
            val tile = made[item] ?: tiles.item(item, look, origin)
            next[item] = tile
            val (title, subtitle) = Playlists.rowTitles(item)
            PlaylistRow(entry.entryId, item, i + 1, title, subtitle, formatDuration(item.durationMs), tile)
        }
        made = next
        val smart = !detail.smartFilter.isNullOrEmpty()
        val count = (detail.itemCount - gone.size).coerceAtLeast(0)
        val duration = (detail.durationMs - gone.sumOf { it.item.durationMs ?: 0 }).coerceAtLeast(0)
        return PlaylistPage(
            id = detail.id,
            title = detail.title,
            video = video,
            smart = smart,
            meta = Playlists.meta(count, duration),
            notes = if (smart) SmartFilter.describe(detail.smartFilter.orEmpty()) else emptyList(),
            artwork = detail.artwork,
            origin = origin,
            lead = entries.firstOrNull()?.item ?: detail.entries.firstOrNull()?.item,
            rows = rows,
            metaTexts = Playlists.metaTexts(count, duration),
            noteTexts = if (smart) SmartFilter.words(detail.smartFilter.orEmpty()) else emptyList(),
        )
    }

    companion object {
        /** How long a removed entry can be put back: the toast's 3.5 s and a little. */
        const val UNDO_MS = 4_000L
    }
}
