package app.pley.android.screens.item

import android.os.Build
import android.os.Trace
import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.viewModelScope
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import app.pley.android.common.AppLanguage
import app.pley.android.common.TraceNames
import app.pley.android.data.DataGraph
import app.pley.android.data.Loadable
import app.pley.android.data.PlayTarget
import app.pley.android.data.map
import app.pley.android.data.mayAsk
import app.pley.android.data.summary
import app.pley.android.data.valueOrNull
import app.pley.android.model.ItemDetail
import app.pley.android.model.ItemKind
import app.pley.android.model.ItemRequest
import app.pley.android.model.ItemSummary
import app.pley.android.network.ApiException
import app.pley.android.player.PlayRequest
import app.pley.android.screens.PlayerLauncher
import app.pley.android.screens.PleyKey
import app.pley.android.screens.requests.ItemRequestState
import app.pley.android.screens.requests.RequestsViewModel
import app.pley.android.screens.requests.moving
import app.pley.android.screens.screensGraph
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.FlowPreview
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.debounce
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.filter
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.flowOn
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.merge
import kotlinx.coroutines.flow.receiveAsFlow
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.flow.transformLatest
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

/**
 * Something the page tells the person once: nothing to play, or an action
 * that failed (the shell's toast says why).
 */
sealed interface ItemEvent {
    data object NothingToPlay : ItemEvent

    data class Failed(val error: Throwable) : ItemEvent
}

/**
 * An item page (movie, show, season, episode), for the phone and the TV
 * alike (apps/web/src/pages/item): the item from what was saved first, the
 * server's answer after; its seasons and episodes, the next episode,
 * More Like This and its collections beside it; and what can be done there.
 *
 * Playing goes through [PlayItem][app.pley.android.data.PlayItem] (movies and
 * episodes resume, shows play their next episode) and the [PlayerLauncher],
 * with the version and tracks chosen here; an audio track only when picked
 * here, else the server picks the one the device plays best. Choosing an
 * audio or subtitle track is saved for the item (PUT /api/items/{id}/streams),
 * as on the web.
 * Watched marks, ratings and My List show at once and are undone if the server
 * refuses. The page is made again when the language shown changes: its
 * dates and numbers are written in it.
 */
@OptIn(ExperimentalCoroutinesApi::class, FlowPreview::class)
class ItemViewModel(
    private val data: DataGraph,
    private val launcher: PlayerLauncher,
    val itemId: Long,
    compute: CoroutineDispatcher = Dispatchers.Default,
) : ViewModel() {
    private val pick = MutableStateFlow(StreamPick())
    private val chosenSeason = MutableStateFlow<Long?>(null)
    private val retries = MutableStateFlow(0)
    private val eventChannel = Channel<ItemEvent>(Channel.BUFFERED)

    /** What the page says once: shown by the interface as a toast. */
    val events: Flow<ItemEvent> = eventChannel.receiveAsFlow()

    /** The detail last shown, for actions. */
    @Volatile
    private var shown: ItemDetail? = null

    private var traced = false

    init {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            Trace.beginAsyncSection(TraceNames.NAV_ITEM, itemId.toInt())
            traced = true
        }
    }

    val state: StateFlow<ItemUiState> = retries
        .flatMapLatest { pageStates() }
        .flowOn(compute)
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(STOP_MS), heldPage())

    /**
     * The page as what is held in memory makes it, there for its first
     * frame: an item seen before, or one asked for ahead, opens whole,
     * with no outline and no empty frame first. Loading when its details aren't held; its
     * seasons, episodes and next episode are in it when they are held
     * too, as the first page made from the server's answers has them.
     * The one page made on the calling thread: nothing is waited for.
     */
    private fun heldPage(): ItemUiState {
        val d = data.items.held(itemId) ?: return ItemUiState.Loading
        val container = d.kind == ItemKind.Show || d.kind == ItemKind.Season
        if (!container && d.kind != ItemKind.Movie && d.kind != ItemKind.Episode) return ItemUiState.Loading
        val next = if (container) data.items.heldOnDeck(itemId) else null
        val seasons = when (d.kind) {
            ItemKind.Show -> data.items.heldChildren(itemId)
            ItemKind.Season -> d.parentId?.let(data.items::heldChildren)
            else -> null
        }?.let(::seasonOrder)
        val selected = when (d.kind) {
            // As the page made later picks it: not before the next episode is known.
            ItemKind.Show -> if (seasons != null && next != null) defaultSeason(seasons, next.valueOrNull) else null
            ItemKind.Season -> itemId
            else -> d.parentId
        }
        shown = d
        val page = itemPage(
            PageInput(
                detail = d,
                origin = data.servers.active.value.orEmpty(),
                onDeck = next ?: Loadable.Loading.takeIf { container },
                seasons = seasons,
                selectedSeason = selected,
                episodes = selected?.let(data.items::heldChildren),
            ),
        )
        return ItemUiState.Ready(page, Loadable.Ready(Unit, fresh = false))
    }

    private fun pageStates(): Flow<ItemUiState> {
        val detail = data.items.detail(itemId)
        val shape = detail.map { load -> load.valueOrNull?.let { Shape(it.kind, it.parentId) } }.distinctUntilChanged()
        return shape.flatMapLatest { s ->
            when {
                s == null -> detail.map(::withoutDetail)
                s.kind == ItemKind.Artist -> flowOf(ItemUiState.Elsewhere(PleyKey.Artist(itemId)))
                s.kind == ItemKind.Album -> flowOf(ItemUiState.Elsewhere(PleyKey.Album(itemId)))
                s.kind == ItemKind.Track -> flowOf(if (s.parentId != null) ItemUiState.Elsewhere(PleyKey.Album(s.parentId)) else ItemUiState.NotFound)
                s.kind == ItemKind.Extra -> flowOf(if (s.parentId != null) ItemUiState.Elsewhere(PleyKey.Item(s.parentId)) else ItemUiState.NotFound)
                s.kind == ItemKind.Unknown -> flowOf(ItemUiState.NotFound)
                else -> pages(detail, s)
            }
        }
    }

    /** The page's state while no detail is known: loading, not found, or failed. */
    private fun withoutDetail(load: Loadable<ItemDetail>): ItemUiState = when (load) {
        is Loadable.Failed -> if ((load.error as? ApiException)?.status == 404) ItemUiState.NotFound else ItemUiState.Failed(load.error)
        else -> ItemUiState.Loading
    }

    private fun pages(detail: Flow<Loadable<ItemDetail>>, shape: Shape): Flow<ItemUiState> {
        val origin = data.servers.active.map { it.orEmpty() }.distinctUntilChanged()
        val related: Flow<List<ItemSummary>> = if (shape.kind == ItemKind.Movie || shape.kind == ItemKind.Show) {
            data.items.related(itemId).map { it.valueOrNull.orEmpty() }
        } else {
            flowOf(emptyList())
        }
        val partOf = if (shape.kind == ItemKind.Movie || shape.kind == ItemKind.Show) {
            data.items.collectionsOf(itemId).map { it.valueOrNull.orEmpty() }
        } else {
            flowOf(emptyList())
        }
        val onDeck: Flow<Loadable<ItemSummary?>?> = when (shape.kind) {
            ItemKind.Show, ItemKind.Season -> data.items.onDeck(itemId)
            else -> flowOf(null)
        }
        // The tabs: a show's seasons, or a season's and its siblings, in the order they are listed.
        val seasons: Flow<List<ItemSummary>?> = when (shape.kind) {
            ItemKind.Show -> data.items.children(itemId)
            ItemKind.Season -> shape.parentId?.let { show -> data.items.children(show) }
            else -> null
        }?.map { load -> load.valueOrNull?.let(::seasonOrder) } ?: flowOf(null)
        val selected: Flow<Long?> = when (shape.kind) {
            ItemKind.Show -> combine(chosenSeason, seasons, onDeck) { chosen, list, next ->
                chosen ?: when {
                    list == null -> null
                    // Wait for the next episode before picking a season, so the page doesn't jump to it.
                    next == null || next is Loadable.Loading -> null
                    else -> defaultSeason(list, next.valueOrNull)
                }
            }
            ItemKind.Season -> chosenSeason.map { it ?: itemId }
            ItemKind.Episode -> flowOf(shape.parentId)
            else -> flowOf(null)
        }.distinctUntilChanged()
        val shownEpisodes: Flow<Pair<Long?, List<ItemSummary>?>> = selected.flatMapLatest { season ->
            if (season == null) flowOf(null to null) else data.items.children(season).map { season to it.valueOrNull }
        }
        // The seasons beside the one shown have their episodes asked for ahead, while the page is
        // shown: moving along the seasons, each has its episodes when focus rests on it.
        val warming = flow<Nothing> {
            combine(selected, seasons) { season, list -> season to list }.collectLatest { (season, list) -> warmBeside(season, list) }
        }
        val episodes = merge(shownEpisodes, warming)
        val beside = combine(onDeck, seasons, episodes, related, partOf) { next, list, (season, eps), rel, part ->
            Beside(next, list, season, eps, rel, part)
        }
        return combine(detail, origin, pick, beside, AppLanguage.shown) { load, server, picked, b, _ ->
            val d = load.valueOrNull ?: return@combine withoutDetail(load)
            shown = d
            val page = itemPage(
                PageInput(
                    detail = d,
                    origin = server,
                    pick = picked,
                    onDeck = b.onDeck,
                    seasons = b.seasons,
                    selectedSeason = b.season,
                    episodes = b.episodes,
                    related = b.related,
                    partOf = b.partOf,
                    locale = AppLanguage.locale,
                    words = AppLanguage.formatWords(),
                ),
            )
            ItemUiState.Ready(page, load.map { })
        }
    }

    /**
     * The page first drew its content: the end of [TraceNames.NAV_ITEM],
     * and what requests say of it may be asked now.
     */
    fun onShown() {
        endTrace()
        drawn.value = true
    }

    private fun endTrace() {
        if (traced) {
            traced = false
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) Trace.endAsyncSection(TraceNames.NAV_ITEM, itemId.toInt())
        }
    }

    /** The page has drawn ([onShown]): until then nothing is asked of requests. */
    private val drawn = MutableStateFlow(false)

    /**
     * What requests say of this movie or show: asked for
     * once the page has drawn, never before, and only when the person may
     * ask or has requests; read again every 3 s, 4 times at most, while it
     * fails or TMDB's details haven't come (Request more seasons waits for
     * them), then when one of its requests changes, and every 10 s while
     * its request downloads or goes into the library (download progress
     * sends no events). `null`: nothing to show (not a movie or show, no
     * TMDB match, or requests don't apply).
     */
    val request: StateFlow<ItemRequestState?> = combine(
        drawn,
        data.requests.state,
        data.items.detail(itemId).map { it.valueOrNull?.kind }.distinctUntilChanged(),
    ) { shown, gate, kind ->
        val wanted = gate.mayAsk || gate.summary?.let { it.own > 0 && it.yourRequests } == true
        if (shown && wanted && (kind == ItemKind.Movie || kind == ItemKind.Show)) RequestGate(gate.mayAsk, gate.summary?.manage == true) else null
    }
        .distinctUntilChanged()
        .flatMapLatest { gate -> if (gate == null) flowOf(null) else itemRequests(gate) }
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(STOP_MS), null)

    private data class RequestGate(val mayAsk: Boolean, val manage: Boolean)

    private fun itemRequests(gate: RequestGate): Flow<ItemRequestState?> = flow {
        // The server waits for TMDB 2 s at most: a later read has what this one lacked.
        var answer: ItemRequest? = null
        for (attempt in 0..AGAIN_READS) {
            if (attempt > 0) delay(AGAIN_MS)
            val read = readItemRequest()
            // No TMDB match: nothing to show, ever.
            if (read.isSuccess && read.getOrNull() == null) return@flow emit(null)
            read.getOrNull()?.let {
                answer = it
                emit(ItemRequestState.of(it, gate.mayAsk, gate.manage))
            }
            if (answer?.askable != null) break
        }
        var shown = answer ?: return@flow emit(null)
        val downloading = MutableStateFlow(shown.request?.moving == true)
        val changes = data.requests.changes
            .filter { it.about(shown.kind, shown.tmdbId) || (it.requestId != null && it.requestId == shown.request?.id) }
            .debounce(REQUEST_CHANGES_MS)
            .map { }
        val ticks = downloading.transformLatest { on ->
            while (on) {
                delay(RequestsViewModel.MOVING_MS)
                emit(Unit)
            }
        }
        merge(changes, ticks).collect {
            // A failure keeps what shows: an extra, the page shows without it.
            val again = readItemRequest().getOrNull() ?: return@collect
            shown = again
            downloading.value = again.request?.moving == true
            emit(ItemRequestState.of(again, gate.mayAsk, gate.manage))
        }
    }

    /** What requests say of this item: `null` without a TMDB match, or why it couldn't be read. */
    private suspend fun readItemRequest(): Result<ItemRequest?> = try {
        Result.success(data.requests.itemRequest(itemId))
    } catch (e: CancellationException) {
        throw e
    } catch (e: Exception) {
        Result.failure(e)
    }

    /** Loads the page again after it failed. */
    fun retry() {
        retries.update { it + 1 }
    }

    /**
     * Plays the item: a movie or episode from where it was left (or from
     * the start), with the version and tracks chosen here; a show or season
     * its next episode.
     */
    fun play(fromStart: Boolean = false) {
        val d = shown ?: return
        viewModelScope.launch {
            val target = attempt { data.playItem.resolve(d.toSummary(), fromStart) } ?: return@launch
            when (target) {
                is PlayTarget.Video -> launcher.play(request(d, target, fromStart))
                else -> eventChannel.send(ItemEvent.NothingToPlay)
            }
        }
    }

    /**
     * The request for [target]: this item's own choices when it is the item
     * played. The audio only when picked here: otherwise the server picks
     * the track this device plays best.
     */
    private fun request(d: ItemDetail, target: PlayTarget.Video, fromStart: Boolean): PlayRequest {
        if (target.itemId != d.id) return PlayRequest(itemId = target.itemId, startMs = target.startMs, fromStart = fromStart)
        val playing = playing(d, pick.value)
        return PlayRequest(
            itemId = d.id,
            fileId = playing.file?.id?.takeIf { d.files.size > 1 },
            startMs = target.startMs,
            fromStart = fromStart,
            audioIndex = playing.pickedAudio?.toLong(),
            subtitleIndex = playing.subtitleIndex?.toLong(),
        )
    }

    /** Plays [item] (an episode of the list, an extra), from where it was left unless [fromStart]. */
    fun play(item: ItemSummary, fromStart: Boolean = false) {
        if (item.id == shown?.id) return play(fromStart)
        viewModelScope.launch { playItem(data, launcher, item, fromStart)?.let { eventChannel.send(it) } }
    }

    /** Marks the item watched, or unwatched when it is. */
    fun toggleWatched() {
        val page = (state.value as? ItemUiState.Ready)?.page ?: return
        setWatched(page.id, !page.watched)
    }

    /** Adds the item to My List, or takes it off when it is in it. */
    fun toggleMyList() {
        val page = (state.value as? ItemUiState.Ready)?.page ?: return
        val inList = page.myList ?: return
        viewModelScope.launch { attempt { if (inList) data.watchlist.remove(page.id) else data.watchlist.add(page.id) } }
    }

    /** Marks [id] (this item, or one of its episodes) watched or not. */
    fun setWatched(id: Long, watched: Boolean) {
        viewModelScope.launch { attempt { data.watchState.setWatched(id, watched) } }
    }

    /** The profile's rating, 1 to 10 (a half star each), or none. */
    fun rate(rating: Int?) {
        viewModelScope.launch { attempt { data.watchState.rate(itemId, rating?.takeIf { it > 0 }?.toDouble()) } }
    }

    /** Plays another version: its tracks start as the server would pick them. Not saved, as on the web. */
    fun chooseVersion(fileId: Long) {
        pick.update { it.copy(fileId = fileId) }
    }

    /**
     * Plays audio track [index], and keeps it for the item (and the rest of
     * a show). `null` is Automatic: the pick is forgotten, and each device
     * plays its best track of the language.
     */
    fun chooseAudio(index: Int?) {
        val d = shown ?: return
        val file = playing(d, pick.value).file ?: return
        pick.update { it.copy(audioFile = file.id, audio = index) }
        remember(d, file.id, index, playing(d, pick.value).subtitleIndex, automaticAudio = index == null)
    }

    /**
     * Shows subtitle track [index] (`null`: off), and keeps it for the item,
     * with the audio picked here, if any: else the audio chosen before stays.
     */
    fun chooseSubtitle(index: Int?) {
        val d = shown ?: return
        val playing = playing(d, pick.value)
        val file = playing.file ?: return
        pick.update { it.copy(subtitleFile = file.id, subtitle = index) }
        remember(d, file.id, playing.pickedAudio, index)
    }

    private fun remember(d: ItemDetail, fileId: Long, audio: Int?, subtitle: Int?, automaticAudio: Boolean = false) {
        viewModelScope.launch { attempt { data.watchState.chooseStreams(d.id, fileId, audio, subtitle, automaticAudio) } }
    }

    /** [block]'s value; `null` once a failure was told as an event. */
    private suspend fun <T> attempt(block: suspend () -> T): T? = try {
        block()
    } catch (e: CancellationException) {
        throw e
    } catch (e: Exception) {
        eventChannel.send(ItemEvent.Failed(e))
        null
    }

    /** Shows [seasonId]'s episodes. */
    fun selectSeason(seasonId: Long) {
        chosenSeason.value = seasonId
    }

    /**
     * Asks for the episodes of the seasons beside [season] ahead, the
     * nearest first, one at a time. Only those: each is an answer kept, and
     * the next ones are asked for when focus gets to this one's neighbour.
     */
    private suspend fun warmBeside(season: Long?, seasons: List<ItemSummary>?) {
        val at = seasons?.indexOfFirst { it.id == season }?.takeIf { it >= 0 } ?: return
        for (by in WARM_BESIDE) {
            val beside = seasons.getOrNull(at + by) ?: continue
            try {
                data.items.warmChildren(beside.id)
            } catch (e: CancellationException) {
                throw e
            } catch (_: Exception) {
                // Asked again when shown.
            }
        }
    }

    override fun onCleared() {
        endTrace()
    }

    private data class Shape(val kind: ItemKind, val parentId: Long?)

    private data class Beside(
        val onDeck: Loadable<ItemSummary?>?,
        val seasons: List<ItemSummary>?,
        val season: Long?,
        val episodes: List<ItemSummary>?,
        val related: List<ItemSummary>,
        val partOf: List<app.pley.android.model.CollectionSummary>,
    )

    companion object {
        /** Keeps loading this long after the page stops showing (rotation, a quick look elsewhere). */
        const val STOP_MS = 5_000L

        /** The seasons whose episodes are asked for ahead, from the one shown: the next ones first. */
        private val WARM_BESIDE = intArrayOf(1, -1, 2, -2)

        /** Request events gathered before the item's request is read again. */
        const val REQUEST_CHANGES_MS = 500L

        /** How long after a failed first read, or one without TMDB's details, the item's request is read again. */
        const val AGAIN_MS = 3_000L

        /** How many times it is: the server has the details once TMDB answers. */
        const val AGAIN_READS = 4

        /** The page of [itemId], made with the app's graph. */
        fun factory(itemId: Long): ViewModelProvider.Factory = viewModelFactory {
            initializer {
                val graph = screensGraph()
                ItemViewModel(graph.data, graph.playerLauncher, itemId)
            }
        }
    }
}

/**
 * Plays [item] as pressing play does: a video from where it was left (or
 * from the start), a show or season its next episode. What to tell the
 * person when it can't play, or `null`.
 */
internal suspend fun playItem(data: DataGraph, launcher: PlayerLauncher, item: ItemSummary, fromStart: Boolean): ItemEvent? {
    val target = try {
        data.playItem.resolve(item, fromStart)
    } catch (e: CancellationException) {
        throw e
    } catch (e: Exception) {
        return ItemEvent.Failed(e)
    }
    return when (target) {
        is PlayTarget.Video -> {
            launcher.play(PlayRequest(itemId = target.itemId, startMs = target.startMs, fromStart = fromStart))
            null
        }
        else -> ItemEvent.NothingToPlay
    }
}
