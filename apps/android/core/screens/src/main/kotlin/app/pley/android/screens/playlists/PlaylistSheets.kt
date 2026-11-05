@file:OptIn(ExperimentalCoroutinesApi::class)

package app.pley.android.screens.playlists

import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.viewModelScope
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import app.pley.android.common.AppLanguage
import app.pley.android.common.UiText
import app.pley.android.data.ItemRepository
import app.pley.android.data.Loadable
import app.pley.android.data.map
import app.pley.android.data.valueOrNull
import app.pley.android.model.ItemSummary
import app.pley.android.model.PlaylistKind
import app.pley.android.model.format.FormatWords
import app.pley.android.model.format.fullTitle
import app.pley.android.screens.R
import app.pley.android.screens.library.SmartFilter
import app.pley.android.screens.screensGraph
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.flowOn
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.receiveAsFlow
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch

/** The Add to playlist sheet: what is being added, and the playlists it can go in. */
data class AddToPlaylistUiState(
    /** "The Office — S2 · E3 — Office Olympics", "Heat (1995)", "3 items"; `null` while it loads. */
    val title: UiText?,
    /** What a playlist must play to take it; `null` while that isn't known. */
    val kind: PlaylistKind?,
    /** The profile's playlists of that kind, smart ones left out (they fill themselves). */
    val playlists: Loadable<List<PlaylistCard>>,
    /** A change is on its way: the sheet's buttons wait. */
    val busy: Boolean,
    /** The first item, whose backdrop the TV shows behind its panel; `null` while it loads. */
    val lead: ItemSummary? = null,
    /** The server's address, for the pictures. */
    val origin: String = "",
)

/** The outcome of adding, for the toast the sheet leaves behind. */
sealed interface AddEvent {
    /**
     * Added to [playlist]: [entries] entries, when known. 0 is "Nothing to
     * add", 1 or unknown "Added to X", more "Added N items to X".
     */
    data class Added(val playlist: String, val entries: Int?) : AddEvent

    data class Failed(val error: Throwable) : AddEvent
}

/**
 * Add to playlist (apps/web/src/components/lists/AddToPlaylistModal.tsx),
 * a bottom sheet on phones and a side panel on TVs: the full title of
 * what is added, a new playlist named on the spot (video or music, as
 * the items are), and the profile's playlists that take it. Shows,
 * seasons, albums and artists add all their episodes or tracks.
 */
class AddToPlaylistViewModel(
    val itemIds: List<Long>,
    private val deps: PlaylistDeps,
    private val items: ItemRepository,
) : ViewModel() {
    private val eventChannel = Channel<AddEvent>(Channel.BUFFERED)
    private val working = MutableStateFlow(false)

    /** Added (close and toast), or a failure (toast, and the sheet stays). */
    val events: Flow<AddEvent> = eventChannel.receiveAsFlow()

    /** The items as far as they are known; `null` until each has loaded (or failed). */
    private val known: Flow<List<ItemSummary>?> =
        if (itemIds.isEmpty()) {
            flowOf(emptyList())
        } else {
            combine(itemIds.map { id -> items.detail(id) }) { loads ->
                if (loads.any { it == Loadable.Loading }) null else loads.mapNotNull { it.valueOrNull?.toSummary() }
            }
        }

    @Volatile
    private var summaries: List<ItemSummary> = emptyList()

    val state: StateFlow<AddToPlaylistUiState> = combine(
        known,
        deps.playlists.playlists(),
        deps.origin.map { it.orEmpty() }.distinctUntilChanged(),
        working,
        // Made again when the language shown changes: what is added and the cards' durations are written in it.
        AppLanguage.shown,
    ) { known, playlists, origin, busy, _ ->
        if (known != null) summaries = known
        val words = AppLanguage.formatWords()
        val kind = known?.let { list -> list.firstOrNull()?.let { Playlists.kindOf(it.kind) } ?: PlaylistKind.Video }
        AddToPlaylistUiState(
            title = known?.let { titleOf(it, words) },
            kind = kind,
            playlists = playlists.map { list ->
                list.filter { it.smartFilter.isNullOrEmpty() && (kind == null || it.kind == kind) }.map { Playlists.card(it, origin, words) }
            },
            busy = busy,
            lead = known?.firstOrNull(),
            origin = origin,
        )
    }
        .flowOn(deps.compute)
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(PlaylistsViewModel.STOP_AFTER_MS), AddToPlaylistUiState(null, null, Loadable.Loading, false))

    /** Adds the items to [playlist]. */
    fun add(playlist: PlaylistCard) {
        if (!working.compareAndSet(false, true)) return
        val entries = Playlists.entriesOf(summaries)
        deps.work.launch {
            change { deps.playlists.add(playlist.id, itemIds) }?.let { eventChannel.trySend(AddEvent.Added(playlist.title, entries)) }
        }
    }

    /** Makes a playlist called [title] holding the items; `false` (and nothing sent) when the title won't do. */
    fun create(title: String): Boolean {
        if (Playlists.check(title) != null) return false
        if (!working.compareAndSet(false, true)) return false
        val name = title.trim()
        val kind = state.value.kind
        val entries = Playlists.entriesOf(summaries)
        deps.work.launch {
            change { deps.playlists.create(name, kind, itemIds) }?.let { eventChannel.trySend(AddEvent.Added(name, entries)) }
        }
        return true
    }

    private suspend fun <R> change(block: suspend () -> R): R? = try {
        block()
    } catch (e: CancellationException) {
        throw e
    } catch (e: Exception) {
        eventChannel.trySend(AddEvent.Failed(e))
        null
    } finally {
        working.value = false
    }

    companion object {
        /** What is added, in words: one item's full title (in [words]), else how many ("3 items"). */
        fun titleOf(items: List<ItemSummary>, words: FormatWords): UiText = when (items.size) {
            0 -> UiText.raw("")
            1 -> UiText.raw(fullTitle(items.single(), words))
            else -> UiText.plural(R.plurals.playlists_count_items, items.size)
        }

        fun factory(itemIds: List<Long>): ViewModelProvider.Factory = viewModelFactory {
            initializer {
                val graph = screensGraph()
                AddToPlaylistViewModel(itemIds, PlaylistDeps.of(graph), graph.data.items)
            }
        }
    }
}

/** What a new playlist starts from. */
sealed interface NewPlaylistMode {
    /** A playlist of [itemIds] (their kind decides what it plays), or an empty one of the kind chosen. */
    data class Items(val itemIds: List<Long>) : NewPlaylistMode

    /** A smart playlist of a library's filters ([query], the web's filter string). */
    data class Smart(val libraryId: Long, val query: String) : NewPlaylistMode
}

/**
 * New playlist (the web's PlaylistFormModal) and Save these filters (its
 * SaveFiltersModal, as a smart playlist: apps don't make collections): a
 * title of at most 200 characters, Movies & TV or Music for an empty one.
 * [CreateEvent.Created] opens the playlist made.
 */
class NewPlaylistViewModel(
    val mode: NewPlaylistMode,
    private val deps: PlaylistDeps,
) : ViewModel() {
    private val eventChannel = Channel<CreateEvent>(Channel.BUFFERED)
    private val working = MutableStateFlow(false)

    val events: Flow<CreateEvent> = eventChannel.receiveAsFlow()

    /** Whether it is on its way to the server. */
    val busy: StateFlow<Boolean> = working

    /** Whether the sheet asks what the playlist plays: only an empty, ordinary one. */
    val asksKind: Boolean = mode is NewPlaylistMode.Items && mode.itemIds.isEmpty()

    /** A smart playlist's filters in words: "Genre: Comedy", "Unwatched". */
    val filterTexts: List<UiText> = (mode as? NewPlaylistMode.Smart)?.let { SmartFilter.words(it.query) }.orEmpty()

    /** Makes it; `false` (and nothing sent) when [title] won't do or it is already being made. */
    fun create(title: String, kind: PlaylistKind = PlaylistKind.Video): Boolean {
        if (Playlists.check(title) != null || !working.compareAndSet(false, true)) return false
        val name = title.trim()
        deps.work.launch {
            try {
                val id = when (val m = mode) {
                    is NewPlaylistMode.Items -> deps.playlists.create(name, if (m.itemIds.isEmpty()) kind else null, m.itemIds)
                    is NewPlaylistMode.Smart -> deps.playlists.createSmart(name, m.libraryId, m.query)
                }
                eventChannel.trySend(CreateEvent.Created(id, name))
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                eventChannel.trySend(CreateEvent.Failed(e))
            } finally {
                working.value = false
            }
        }
        return true
    }

    companion object {
        fun factory(mode: NewPlaylistMode): ViewModelProvider.Factory = viewModelFactory {
            initializer { NewPlaylistViewModel(mode, PlaylistDeps.of(screensGraph())) }
        }
    }
}
