@file:OptIn(ExperimentalCoroutinesApi::class)

package app.pley.android.screens.home

import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.viewModelScope
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import app.pley.android.common.AppLanguage
import app.pley.android.data.HubsRepository
import app.pley.android.data.ItemRepository
import app.pley.android.data.LibrariesRepository
import app.pley.android.data.Loadable
import app.pley.android.data.PlayItem
import app.pley.android.data.PlayTarget
import app.pley.android.data.ScanState
import app.pley.android.data.Session
import app.pley.android.data.WatchStateRepository
import app.pley.android.data.WatchlistRepository
import app.pley.android.data.valueOrNull
import app.pley.android.model.Hubs
import app.pley.android.model.ItemDetail
import app.pley.android.model.ItemSummary
import app.pley.android.model.Library
import app.pley.android.model.LibraryKind
import app.pley.android.player.PlayRequest
import app.pley.android.screens.Navigator
import app.pley.android.screens.PlayerLauncher
import app.pley.android.screens.ScreensGraph
import app.pley.android.screens.screensGraph
import java.time.LocalDate
import kotlin.coroutines.cancellation.CancellationException
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.catch
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.emitAll
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.flowOn
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.mapNotNull
import kotlinx.coroutines.flow.onEach
import kotlinx.coroutines.flow.onStart
import kotlinx.coroutines.flow.receiveAsFlow
import kotlinx.coroutines.flow.shareIn
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/**
 * What Home reads and does: the rows, the libraries (for the empty
 * states and the banner's words), the scans, item details (the banner,
 * the hero, the TV's info block, items pressed ahead of their page),
 * playing, marking watched and My List.
 */
interface HomeSources {
    val hubs: HubsRepository
    val libraries: LibrariesRepository
    val items: ItemRepository
    val session: Flow<Session?>
    val scans: StateFlow<List<ScanState>>
    val playItem: PlayItem
    val watchState: WatchStateRepository
    val watchlist: WatchlistRepository
    val launcher: PlayerLauncher

    companion object {
        /** The app's, each part asked for only when first used (off the main thread). */
        fun of(graph: ScreensGraph): HomeSources = object : HomeSources {
            override val hubs get() = graph.data.hubs
            override val libraries get() = graph.data.libraries
            override val items get() = graph.data.items
            override val session get() = graph.data.session.session
            override val scans get() = graph.data.liveUpdates.scans
            override val playItem get() = graph.data.playItem
            override val watchState get() = graph.data.watchState
            override val watchlist get() = graph.data.watchlist
            override val launcher get() = graph.playerLauncher
        }
    }
}

/** Something to tell the person, once. */
sealed interface HomeMessage {
    /** A show with no episode to play, an album without tracks: "Nothing to play". */
    data object NothingToPlay : HomeMessage

    /** Something they did failed: [error]'s sentence (the shell handles scopes and being offline). */
    data class Failed(val error: Throwable) : HomeMessage
}

/** The profile in the phone's top bar. */
data class HomeProfile(
    val name: String,
    /** "#e07a5f", or `null` for pley's own. */
    val color: String?,
    /** The picture's full address, or `null`. */
    val pictureUrl: String?,
)

/**
 * Home, for the phone and the TV (P4-HOME, design spec §4.1, §8).
 *
 * - [state] is drawn from what was saved first, then from the server: GET
 *   /api/hubs/home through the hubs repository. A skeleton or an error
 *   shows only when nothing was saved. Rows, the hero and every label are
 *   worked out on [compute], never on the main thread; screens shape them
 *   further for their own tiles with [present]. It changes only when what
 *   Home shows does: whether that is the server's answer is [status], and
 *   a refresh under way [refreshing].
 * - The banner's titles ([HomeState.Ready.featured]) and the hero come
 *   with their details (plot, genres, content rating, My List), all asked
 *   for at once, so turning to a title shows it whole.
 * - After the first content, the details of the first three Continue
 *   Watching items are asked for ahead: that warms the server's keyframe
 *   index, so playing from Home can remux instead of transcoding
 *   (apps/transcoder/src/plan.rs).
 * - Live updates refresh the rows (the data module asks again on library
 *   changes; Home asks once more when a scan ends), and [refresh] asks
 *   again on purpose (pull to refresh, Retry).
 * - [info] is what the TV's info block says about the tile focus rests on
 *   ([focus]), its details fetched as it rests.
 * - [play] plays as the web's usePlayItem: a show its on-deck episode.
 */
class HomeViewModel(
    private val sources: HomeSources,
    private val compute: CoroutineDispatcher = Dispatchers.Default,
    /** The day it is, as days since 1970: which top rated titles the banner features changes with it. */
    private val day: () -> Long = { LocalDate.now().toEpochDay() },
) : ViewModel() {
    private val pulled = MutableStateFlow(false)
    private val focused = MutableStateFlow<ItemSummary?>(null)
    private val messageChannel = Channel<HomeMessage>(Channel.BUFFERED)
    private var prefetched = false
    private var wasScanning = false

    /** Messages to show once (toasts). */
    val messages: Flow<HomeMessage> = messageChannel.receiveAsFlow()

    /**
     * The movie and show libraries' Top Rated rows, for the banner: each
     * library's Recommended rows, which its page shows too, so they are
     * kept. Nothing until they come, and nothing from a library that
     * doesn't answer: Home never waits for them.
     */
    @Volatile
    private var lastTopRated: List<ItemSummary> = emptyList()

    private val topRated: Flow<List<ItemSummary>> = flow { emitAll(sources.libraries.libraries()) }
        .map { load -> load.valueOrNull.orEmpty().filter { it.kind == LibraryKind.Movies || it.kind == LibraryKind.Shows }.map { it.id }.take(TOP_RATED_LIBRARIES) }
        .distinctUntilChanged()
        .flatMapLatest { ids ->
            if (ids.isEmpty()) return@flatMapLatest flowOf(emptyList())
            val rows = ids.map { id ->
                flow { emitAll(sources.hubs.library(id)) }
                    .map { load -> load.valueOrNull?.hubs?.firstOrNull { it.key == TOP_RATED_PREFIX + id }?.items.orEmpty() }
                    .catch { emit(emptyList()) }
                    .onStart { emit(emptyList()) }
            }
            combine(rows) { lists -> lists.flatMap { it } }
        }
        // What came last time stands until this time's answer: Home collected again (the app back in
        // front) doesn't show the newest alone for a moment and then the mix again.
        .onEach { lastTopRated = it }
        .onStart { emit(lastTopRated) }
        .distinctUntilChanged()

    /** What the repositories hold, worked out once for [state] and [status]. */
    private val loaded: SharedFlow<HomeLoad> = combine(
        combine(
            flow { emitAll(sources.hubs.home()) },
            flow { emitAll(sources.libraries.libraries()) },
            flow { emitAll(sources.scans) }.map { it.isNotEmpty() }.distinctUntilChanged().onEach(::scanning),
            flow { emitAll(sources.session) }.map { it?.issuer.orEmpty() }.distinctUntilChanged(),
            // Made again when the language shown changes: the hero's runtime and episode numbers are written in it.
            AppLanguage.shown,
        ) { hubs, libraries, scanning, origin, _ -> HomeHeld(hubs, libraries, scanning, origin) },
        topRated,
    ) { held, rated ->
        val state = homeState(held.hubs, held.libraries, held.scanning, held.origin, rated, day())
        HomeLoad(state, homeStatus(held.hubs, state))
    }
        .flowOn(compute)
        .shareIn(viewModelScope, SharingStarted.WhileSubscribed(), replay = 1)

    val state: StateFlow<HomeState> = loaded
        .map { it.state }
        // An answer the same as the one before (the server asked again on coming back) goes no further.
        .distinctUntilChanged()
        .withDetails()
        .distinctUntilChanged()
        .onEach(::prefetchContinueWatching)
        .flowOn(compute)
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(STOP_AFTER_MS), HomeState.Loading)

    /** Whether what [state] shows is the server's answer ([homeStatus]), for the offline notice. */
    val status: StateFlow<Loadable<Unit>?> = loaded
        .map { it.status }
        .distinctUntilChanged()
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(STOP_AFTER_MS), null)

    /** A refresh someone asked for ([refresh]: pull to refresh, Retry) is running. */
    val refreshing: StateFlow<Boolean> = pulled.asStateFlow()

    /** What the TV's info block shows: the item focus rests on, with its details once they are in. */
    val info: StateFlow<HomeInfo?> = combine(focused, AppLanguage.shown) { item, _ -> item }
        .flatMapLatest { item ->
            if (item == null) {
                flowOf(null)
            } else {
                flow { emitAll(sources.items.detail(item.id)) }
                    .map { homeInfo(item, it.valueOrNull) }
                    .onStart { emit(homeInfo(item, null)) }
            }
        }
        .distinctUntilChanged()
        .flowOn(compute)
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(STOP_AFTER_MS), null)

    /** Who is watching, for the phone's top bar. */
    val profile: StateFlow<HomeProfile?> = flow { emitAll(sources.session) }
        .map { session ->
            val me = session?.me ?: return@map null
            HomeProfile(me.name, me.avatarColor, me.avatarUrl?.takeIf { it.isNotEmpty() }?.let { absolute(it, session.issuer) })
        }
        .distinctUntilChanged()
        .flowOn(compute)
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(STOP_AFTER_MS), null)

    private val presented = HashMap<Class<*>, StateFlow<*>>()

    /**
     * [state] shaped by [transform] (a screen's own tile models) on the
     * compute dispatcher, kept with the screen: coming back to Home finds
     * it made already. `null` until the first state is shaped. One flow
     * per transform (by its class), so pass a function reference or a
     * lambda written once.
     */
    @Suppress("UNCHECKED_CAST")
    fun <T : Any> present(transform: (HomeState) -> T): StateFlow<T?> = synchronized(presented) {
        presented.getOrPut(transform.javaClass) {
            state.map { transform(it) }
                .flowOn(compute)
                .stateIn(viewModelScope, SharingStarted.WhileSubscribed(STOP_AFTER_MS), null)
        } as StateFlow<T?>
    }

    /** Asks the server again for the rows and the libraries. */
    fun refresh() {
        if (pulled.value) return
        pulled.value = true
        viewModelScope.launch(compute) {
            try {
                coroutineScope {
                    launch { sources.hubs.refresh() }
                    launch { sources.libraries.refresh() }
                }
            } finally {
                pulled.value = false
            }
        }
    }

    /**
     * Focus rests on [item] (TV): the info block follows it, and the
     * details of [ahead] (the next tiles of its row) are asked for ahead.
     */
    fun focus(item: ItemSummary?, ahead: List<ItemSummary> = emptyList()) {
        focused.value = item
        ahead.forEach { prefetch(it) }
    }

    /** [item]'s page is likely next (a press going down): its details are asked for ahead. */
    fun prefetch(item: ItemSummary) {
        sources.items.prefetch(item.id)
    }

    /**
     * Plays [item] as the web does (apps/web/src/lib/usePlayItem.ts): a
     * movie or episode where it was left (or [fromStart]), a show or season
     * its next episode. Music opens its page until the music player comes
     * (P5); nothing to play says so.
     */
    fun play(item: ItemSummary, fromStart: Boolean, navigator: Navigator) {
        viewModelScope.launch {
            val target = try {
                withContext(compute) { sources.playItem.resolve(item, fromStart) }
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                messageChannel.send(HomeMessage.Failed(e))
                return@launch
            }
            when (target) {
                is PlayTarget.Video -> sources.launcher.play(
                    PlayRequest(itemId = target.itemId, startMs = target.startMs, fromStart = fromStart || target.startMs == 0L),
                )
                is PlayTarget.Music -> navigator.open(pageOf(item))
                PlayTarget.NothingToPlay -> messageChannel.send(HomeMessage.NothingToPlay)
            }
        }
    }

    /** Marks [item] watched or not; the rows follow (the data module changes what it holds). */
    fun setWatched(item: ItemSummary, watched: Boolean) {
        viewModelScope.launch(compute) {
            try {
                sources.watchState.setWatched(item.id, watched)
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                messageChannel.send(HomeMessage.Failed(e))
            }
        }
    }

    /** Puts [item] on My List, or takes it off; the banner, the rows and the item's page follow at once. */
    fun setInList(item: ItemSummary, inList: Boolean) {
        viewModelScope.launch(compute) {
            try {
                if (inList) sources.watchlist.add(item.id) else sources.watchlist.remove(item.id)
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                messageChannel.send(HomeMessage.Failed(e))
            }
        }
    }

    /**
     * The hero's and the banner's details as they come in, each title's
     * on its own. Details already seen are kept, so a new answer for the
     * rows never draws them without for a moment.
     */
    private fun Flow<HomeState>.withDetails(): Flow<HomeState> {
        val known = HashMap<Long, ItemDetail>()
        return flatMapLatest { state ->
            if (state !is HomeState.Ready) return@flatMapLatest flowOf(state)
            val ids = (listOfNotNull(state.hero?.item?.id) + state.featured.map { it.info.item.id }).distinct()
            if (ids.isEmpty()) return@flatMapLatest flowOf(state)
            val details = ids.map { id ->
                flow { emitAll(sources.items.detail(id)) }
                    .mapNotNull { it.valueOrNull }
                    .onEach { known[id] = it }
                    .map<ItemDetail, ItemDetail?> { it }
                    .onStart { emit(known[id]) }
            }
            combine(details) { found -> state.withDetails(found.filterNotNull().associateBy { it.id }) }
        }
    }

    /** A scan that ends has changed what Home shows: ask once more, whatever the events said. */
    private fun scanning(running: Boolean) {
        if (wasScanning && !running) viewModelScope.launch(compute) { sources.hubs.refresh() }
        wasScanning = running
    }

    private fun prefetchContinueWatching(state: HomeState) {
        if (prefetched || state !is HomeState.Ready) return
        prefetched = true
        state.rows.firstOrNull { it.continueWatching }?.items?.take(PREFETCHED)?.forEach { prefetch(it) }
    }

    companion object {
        /** Home's ViewModel, from the app's graph. */
        val Factory: ViewModelProvider.Factory = viewModelFactory {
            initializer { HomeViewModel(HomeSources.of(screensGraph())) }
        }

        /** How long a state is kept flowing after the screen stops looking (rotation, a quick trip away). */
        private const val STOP_AFTER_MS = 5_000L

        /** Continue Watching items whose details are asked for once Home shows. */
        const val PREFETCHED = 3

        /** The libraries whose Top Rated rows the banner draws on: the first few, a request each. */
        private const val TOP_RATED_LIBRARIES = 6

        /** A library's Top Rated row's key, before the library's id. */
        private const val TOP_RATED_PREFIX = "top_rated:"
    }
}

/**
 * Home from what the repositories hold: the rows when there are some, an
 * empty state when there are none, a skeleton or an error only when
 * nothing was saved. [topRated] and [day] are the banner's ([featuredOf]).
 */
fun homeState(
    hubs: Loadable<Hubs>,
    libraries: Loadable<List<Library>>,
    scanning: Boolean,
    origin: String,
    topRated: List<ItemSummary> = emptyList(),
    day: Long = 0,
): HomeState {
    val libs = libraries.valueOrNull
    val value = hubs.valueOrNull
    if (value == null) {
        if (libs != null && libs.isEmpty()) return HomeState.NoLibraries
        return if (hubs is Loadable.Failed) HomeState.Failed(hubs.error) else HomeState.Loading
    }
    val rows = homeRows(value.hubs, libs)
    if (rows.isEmpty()) {
        if (libs != null && libs.isEmpty()) return HomeState.NoLibraries
        return HomeState.Empty(scanning || libs.orEmpty().any { it.scanning })
    }
    return HomeState.Ready(
        origin = origin,
        rows = rows,
        hero = heroOf(rows)?.let { homeInfo(it, null) },
        featured = featuredOf(rows, topRated, day).map { Featured(homeInfo(it.item, null), featuredLabel(it, libs)) },
    )
}

/**
 * Whether [state], drawn from [hubs], is the server's answer, as the
 * offline notice and banner want it: ready, from the server now or from
 * what was saved; failed, the server not answering. `null` while there is
 * nothing to tell: nothing shows yet, or no rows do.
 */
fun homeStatus(hubs: Loadable<Hubs>, state: HomeState): Loadable<Unit>? {
    if (state !is HomeState.Ready && state !is HomeState.Failed) return null
    return when (hubs) {
        is Loadable.Ready -> Loadable.Ready(Unit, hubs.fresh)
        is Loadable.Failed -> Loadable.Failed(hubs.error, Unit)
        Loadable.Loading -> null
    }
}

/** Home as the repositories hold it: what it shows, and whether that is the server's answer. */
private data class HomeLoad(val state: HomeState, val status: Loadable<Unit>?)

/** What Home's own answers hold, before the banner's top rated titles join them. */
private data class HomeHeld(val hubs: Loadable<Hubs>, val libraries: Loadable<List<Library>>, val scanning: Boolean, val origin: String)

/** [url] as the API gave it ("/api/users/2/avatar?v=1") on the server at [origin]. */
internal fun absolute(url: String, origin: String): String = when {
    url.startsWith("http://") || url.startsWith("https://") || origin.isEmpty() -> url
    url.startsWith("/") -> origin.trimEnd('/') + url
    else -> origin.trimEnd('/') + "/" + url
}
