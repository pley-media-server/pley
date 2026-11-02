package app.pley.android.screens.livetv

import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.createSavedStateHandle
import androidx.lifecycle.viewModelScope
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import app.pley.android.data.LiveTvRepository
import app.pley.android.data.LiveTvViews
import app.pley.android.data.Loadable
import app.pley.android.data.valueOrNull
import app.pley.android.model.LiveCategory
import app.pley.android.model.LiveChannel
import app.pley.android.model.LiveGroup
import app.pley.android.model.LiveGroups
import app.pley.android.model.LiveProgram
import app.pley.android.screens.PlayerLauncher
import app.pley.android.screens.ScreensGraph
import app.pley.android.screens.screensGraph
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.flowOn
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.receiveAsFlow
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch

/** What Live TV's pages read and change. */
class LiveTvDeps(
    val live: LiveTvRepository,
    val launcher: PlayerLauncher,
    val origin: Flow<String?>,
    /** Unix ms. */
    val clock: () -> Long = System::currentTimeMillis,
    val compute: CoroutineDispatcher = Dispatchers.Default,
    /** Where changes run: they outlive the page that asked. */
    val work: CoroutineScope = LiveWork,
    /** The category, the view and the channel Live TV was left at, for the profile. */
    val views: LiveTvViews = LiveTvViews.None,
) {
    companion object {
        fun of(graph: ScreensGraph): LiveTvDeps =
            LiveTvDeps(graph.data.liveTv, graph.playerLauncher, graph.data.servers.active, views = graph.data.liveTvViews)
    }
}

/** The Live TV page, as its screens draw it, the channels aside ([LiveTvViewModel.channels]). */
data class LivePage(
    /** Live TV is open to this profile (true until the server says). */
    val allowed: Boolean = true,
    /** The groups and the counts; `null` while unknown. */
    val groups: LiveGroups? = null,
    val category: LiveCategory = LiveCategory.All,
    val query: String = "",
    val view: LiveView = LiveView.Guide,
    /** The server's origin: logos and programme pictures are on it. */
    val origin: String = "",
    /** The groups a search names, as quick links to them. */
    val found: List<LiveGroup> = emptyList(),
) {
    val searching: Boolean get() = query.isNotBlank()

    /** What the channels are: the category, or every channel while searching (search looks everywhere, as the web's). */
    val listed: LiveCategory get() = if (searching) LiveCategory.All else category
}

/** Where a list opens: at [index] of [category] searched for [query] (the channel last watched). */
data class Landing(val category: LiveCategory, val query: String, val index: Int?)

/** The guide's programmes by channel, and whether those in view are in. */
data class GuidePrograms(val byChannel: Map<Long, List<LiveProgram>> = emptyMap(), val loaded: Boolean = false)

/** Something to tell: a favourite the server refused. */
sealed interface LiveTvEvent {
    data class Failed(val error: Throwable) : LiveTvEvent
}

/**
 * Live TV (apps/web/src/pages/livetv/LiveTvPage.tsx): the
 * categories first (Favourites, Recently watched, All channels, then the
 * provider's groups, those of a region gathered under it), then the
 * chosen one's channels, a page at a time as they scroll, as a guide that
 * scrolls through time (asking for the programmes of the rows in view
 * only) or as cards; searched by name, number or what's on. A channel
 * opens the live player, which changes channel within the category. The
 * list opens back at the channel last watched.
 *
 * The category and the view are kept with the page's saved state and for
 * the profile ([LiveTvViews], as the web keeps them in the browser), with
 * the channel last watched: Live TV opens as it was left, after a restart
 * too.
 */
@OptIn(ExperimentalCoroutinesApi::class)
class LiveTvViewModel(private val deps: LiveTvDeps, private val saved: SavedStateHandle = SavedStateHandle()) : ViewModel() {
    /** `null` until what was kept for the profile is read: nothing is asked for before. */
    private val chosen = MutableStateFlow(saved.get<String>(CATEGORY)?.let(LiveCategory::parse))
    private val category: Flow<LiveCategory> = chosen.filterNotNull()
    private val query = MutableStateFlow("")
    private val view = MutableStateFlow(viewOf(saved[VIEW]) ?: LiveView.Guide)
    private val eventChannel = Channel<LiveTvEvent>(Channel.BUFFERED)

    /** A favourite refused, to tell. */
    val events: Flow<LiveTvEvent> = eventChannel.receiveAsFlow()

    /** Unix seconds, every [TICK_MS]: progress bars, the guide's now line. */
    val now: StateFlow<Long> = flow {
        while (true) {
            emit(deps.clock() / 1000)
            delay(TICK_MS)
        }
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(STOP_AFTER_MS), deps.clock() / 1000)

    /** Where the guide starts: the half hour before the one it opened in, fixed while the page is open. */
    val guideStart: Long = LiveTv.guideStart(deps.clock() / 1000)

    val state: StateFlow<LivePage> = combine(
        combine(deps.live.summary(), deps.live.groups(), deps.origin.map { it.orEmpty() }.distinctUntilChanged(), ::Triple),
        category,
        query,
        view,
    ) { (summary, groups, origin), c, q, v ->
        LivePage(
            allowed = summary.valueOrNull?.allowed ?: true,
            groups = groups.valueOrNull,
            category = c,
            query = q,
            view = v,
            origin = origin,
            found = LiveTv.foundGroups(groups.valueOrNull?.groups.orEmpty(), q),
        )
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(STOP_AFTER_MS), LivePage(category = chosen.value ?: LiveCategory.All, view = view.value))

    /** The category listed: every channel while searching. */
    private val listed: Flow<LiveCategory> = combine(category, query) { c, q -> if (q.isBlank()) c else LiveCategory.All }.distinctUntilChanged()

    private val paging = ChannelPaging(deps.live, listed, query, viewModelScope, deps.clock)

    /** The chosen category's channels, as far as they are loaded. */
    val channels: StateFlow<ChannelPages> = paging.pages

    /**
     * Where the list opens: at the channel last watched, when it is in the
     * category (unsearched); at the top otherwise. Once per category,
     * search and return from the player.
     */
    val landing: StateFlow<Landing?> = combine(category, query.map { it.trim() }.distinctUntilChanged(), LastWatched.channel, ::Triple)
        .flatMapLatest { (c, q, id) ->
            if (id == null || q.isNotEmpty()) {
                flowOf(Landing(c, q, null))
            } else {
                deps.live.around(c, id).map { load ->
                    when (load) {
                        is Loadable.Ready -> Landing(c, q, load.value.channels.indexOfFirst { it.id == id }.takeIf { it >= 0 }?.plus(load.value.offset))
                        is Loadable.Failed -> Landing(c, q, null)
                        Loadable.Loading -> null
                    }
                }
            }
        }
        .distinctUntilChanged()
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(STOP_AFTER_MS), null)

    private val window = MutableStateFlow<Pair<Long, Long>?>(null)

    /** The guide's programmes, for the pages of channels in view and the time in view, either side too. */
    val guide: StateFlow<GuidePrograms> = combine(
        window.map { span -> span?.let { (from, to) -> LiveTv.chunks(guideStart, from, to) }.orEmpty() }.distinctUntilChanged(),
        channels.map { pages -> pages.pages.values.map { page -> page.map { it.id } }.filter { it.isNotEmpty() } }.distinctUntilChanged(),
        ::Pair,
    ).flatMapLatest { (chunks, idPages) ->
        val asked = chunks.flatMap { chunk -> idPages.map { ids -> deps.live.guide(chunk, ids) } }
        if (asked.isEmpty()) {
            flowOf(GuidePrograms())
        } else {
            combine(asked) { loads ->
                GuidePrograms(LiveTv.merge(loads.mapNotNull { it.valueOrNull }), loaded = loads.none { it is Loadable.Loading })
            }
        }
    }
        .flowOn(deps.compute)
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(STOP_AFTER_MS), GuidePrograms())

    init {
        // As it was left for the profile, unless the saved state says (the page restored).
        viewModelScope.launch {
            val kept = deps.views.of()
            if (LastWatched.channel.value == null) kept.channel?.let { LastWatched.channel.compareAndSet(null, it) }
            if (saved.get<String>(VIEW) == null) viewOf(kept.view)?.let { view.value = it }
            if (chosen.value == null) chosen.value = kept.category?.let(LiveCategory::parse) ?: LiveCategory.All
        }
        // What's on moves on: a programme listed as now that ended asks for the pages again.
        viewModelScope.launch {
            var asked = 0L
            combine(now, channels) { t, pages -> pages.pages.values.any { page -> page.any { c -> c.now?.let { it.stop <= t } == true } } }
                .collect { ended ->
                    if (ended && deps.clock() - asked > REFRESH_FLOOR_MS) {
                        asked = deps.clock()
                        deps.live.refresh()
                    }
                }
        }
    }

    fun setView(value: LiveView) {
        view.value = value
        saved[VIEW] = value.name
        deps.work.launch { deps.views.keep(view = value.name) }
    }

    /** Shows [value]'s channels; a search ends, as the web's does. */
    fun setCategory(value: LiveCategory) {
        query.value = ""
        chosen.value = value
        saved[CATEGORY] = value.key
        deps.work.launch { deps.views.keep(category = value.key) }
    }

    fun setQuery(value: String) {
        query.value = value
    }

    /** Asks again for what failed (and what's on). */
    fun retry() = deps.live.refresh()

    /** Rows [first] to [last] of the list show: their pages are loaded. Cheap: call it as it scrolls. */
    fun showingRows(first: Int, last: Int) = paging.showing(first, last)

    /** The guide shows [from] to [to] (unix seconds): their chunks are loaded. Cheap: call it as it scrolls. */
    fun showingTimes(from: Long, to: Long) {
        val span = Math.floorDiv(from, HALF_CHUNK) to Math.floorDiv(to, HALF_CHUNK)
        val now = window.value?.let { (f, t) -> Math.floorDiv(f, HALF_CHUNK) to Math.floorDiv(t, HALF_CHUNK) }
        if (span != now) window.value = from to to
    }

    /** Opens the live player on [channel], which changes channel within the category it was chosen in (every channel, from a search). */
    fun watch(channel: LiveChannel) {
        val from = if (query.value.isBlank()) chosen.value ?: LiveCategory.All else LiveCategory.All
        ChannelHandoff.give(channel)
        deps.launcher.watchLive(channel.id, from.key)
    }

    fun toggleFavorite(channel: LiveChannel) {
        deps.work.launch {
            try {
                deps.live.setFavorite(channel.id, !channel.favorite)
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                eventChannel.trySend(LiveTvEvent.Failed(e))
            }
        }
    }

    companion object {
        const val STOP_AFTER_MS = 5_000L

        /** The progress bars and the now line move this often. */
        const val TICK_MS = 15_000L

        /** What's on is asked for again at most this often. */
        const val REFRESH_FLOOR_MS = 60_000L

        private const val HALF_CHUNK = LiveTvRepository.GUIDE_CHUNK_S / 2
        private const val VIEW = "live.view"
        private const val CATEGORY = "live.category"

        private fun viewOf(name: String?): LiveView? = name?.let { runCatching { LiveView.valueOf(it) }.getOrNull() }

        val Factory: ViewModelProvider.Factory = viewModelFactory {
            initializer { LiveTvViewModel(LiveTvDeps.of(screensGraph()), createSavedStateHandle()) }
        }
    }
}

/**
 * Whether Live TV has a way in (the phone's Libraries tab, the TV's
 * rail): open to this profile and with channels, as the web's sidebar
 * shows it. `null` while unknown, so nothing pops in and out.
 */
class LiveTvEntryViewModel(live: LiveTvRepository) : ViewModel() {
    /** The channels there are when Live TV shows; 0 when it doesn't. */
    val channels: StateFlow<Int?> = live.summary()
        .map { load ->
            when (load) {
                is Loadable.Ready -> load.value.channels.takeIf { load.value.available } ?: 0
                is Loadable.Failed -> load.stale?.let { if (it.available) it.channels else 0 } ?: 0
                Loadable.Loading -> null
            }
        }
        .distinctUntilChanged()
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(LiveTvViewModel.STOP_AFTER_MS), null)

    companion object {
        val Factory: ViewModelProvider.Factory = viewModelFactory {
            initializer { LiveTvEntryViewModel(screensGraph().data.liveTv) }
        }
    }
}
