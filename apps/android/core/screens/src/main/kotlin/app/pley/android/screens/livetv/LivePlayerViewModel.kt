package app.pley.android.screens.livetv

import android.app.Application
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
import app.pley.android.model.LiveGroups
import app.pley.android.player.PlayerEngine
import app.pley.android.player.live.LivePhase
import app.pley.android.player.live.LivePlayback
import app.pley.android.player.live.LiveState
import app.pley.android.screens.screensGraph
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch

/** What the live player reads and drives. */
class LivePlayerDeps(
    val live: LiveTvRepository,
    /** The playback, made in the ViewModel's scope (the main thread's). */
    val playback: (CoroutineScope) -> LivePlayback,
    val origin: Flow<String?>,
    /** Unix ms. */
    val clock: () -> Long = System::currentTimeMillis,
    val work: CoroutineScope = LiveWork,
    /** Where the channel watched is kept for the profile, for the guide to open at after a restart. */
    val views: LiveTvViews = LiveTvViews.None,
)

/** A channel number being typed on a remote or keypad, or one that matched nothing. */
data class ChannelDigits(val typed: String = "", val notFound: String? = null)

/**
 * The live player (apps/web/src/player/live/LivePlayerPage.tsx), shared
 * by the phone's and the TV's: the channel playing, with what's on now and
 * next; channel up and down within the category it was opened from (the
 * channels either side of it in the server's pages, round the ends; in
 * Recently watched, the order it had as the player opened); a channel
 * number typed, found among every channel; the category's channels a page
 * at a time for the channel list, which can show another category's (a
 * channel tuned from there makes channel up and down go through that one,
 * as the web's does); watched as it airs, without pausing or going back; a
 * favourite; the profile's remote changes channel as channel up and down
 * do. The channel and its category are kept with the screen's saved
 * state, and the channel for the profile too: the guide opens back at it.
 * The playback ends with the ViewModel.
 */
@OptIn(ExperimentalCoroutinesApi::class)
class LivePlayerViewModel(
    private val deps: LivePlayerDeps,
    private val saved: SavedStateHandle,
    channelId: Long,
    categoryKey: String,
    /** What the page that opened the player knew of [channelId] ([ChannelHandoff]): shown at once. */
    seen: LiveChannel? = ChannelHandoff.take(channelId),
) : ViewModel() {
    // The remote's next and previous are channel up and down, as here.
    private val playback = deps.playback(viewModelScope).also { it.zap = { by -> step(by) } }
    private val mutableDigits = MutableStateFlow(ChannelDigits())
    private var digitsJob: Job? = null
    private var zapping: Job? = null
    private val current = MutableStateFlow(saved[CHANNEL] ?: channelId)

    /** What the lists knew of channels tuned from them: shown at once, before the server's own answer. */
    private val known = HashMap<Long, LiveChannel>().apply { if (seen != null) put(seen.id, seen) }

    /** Recently watched's order as the player opened: watching changes it, zapping through it shouldn't. */
    private var recentOrder: List<LiveChannel>? = null

    private val zapCategory = MutableStateFlow(LiveCategory.parse(saved[CATEGORY] ?: categoryKey))
    private val shown = MutableStateFlow(zapCategory.value)

    /** The category channel up and down go through. */
    val category: StateFlow<LiveCategory> = zapCategory.asStateFlow()

    /** The category the channel list shows: the player's, unless another was chosen in it. */
    val listCategory: StateFlow<LiveCategory> = shown.asStateFlow()

    private val mutableListQuery = MutableStateFlow("")

    /** What the channel list is searched for: among every channel, whatever category it shows. */
    val listQuery: StateFlow<String> = mutableListQuery.asStateFlow()

    /** What the channel list's channels are: [listCategory], or every channel while searching. */
    val listed: StateFlow<LiveCategory> = combine(shown, mutableListQuery) { c, q -> if (q.isBlank()) c else LiveCategory.All }
        .stateIn(viewModelScope, SharingStarted.Eagerly, shown.value)

    /** The groups and the counts, for choosing another in the channel list; `null` while unknown. */
    val groups: StateFlow<LiveGroups?> = deps.live.groups().map { it.valueOrNull }
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(LiveTvViewModel.STOP_AFTER_MS), null)

    val state: StateFlow<LiveState> = playback.state

    /** The engine, for the video surface and the media session. */
    val engine: StateFlow<PlayerEngine?> = playback.engine

    /** The channel playing, as the server has it now (a favourite set shows at once). */
    val channel: StateFlow<LiveChannel?> = current.flatMapLatest { id ->
        deps.live.channel(id).map { it.valueOrNull ?: known[id] }
    }.stateIn(viewModelScope, SharingStarted.Eagerly, known[current.value])

    /** The server's origin, for logos. */
    val origin: StateFlow<String> = deps.origin.map { it.orEmpty() }.stateIn(viewModelScope, SharingStarted.Eagerly, "")

    /** Unix seconds, every 5 s: what's on and how far it is. */
    val now: StateFlow<Long> = flow {
        while (true) {
            emit(deps.clock() / 1000)
            delay(TICK_MS)
        }
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(LiveTvViewModel.STOP_AFTER_MS), deps.clock() / 1000)

    /** A channel number being typed. */
    val digits: StateFlow<ChannelDigits> = mutableDigits.asStateFlow()

    private val panel = ChannelPaging(deps.live, listed, mutableListQuery, viewModelScope, deps.clock)

    /** [listed]'s channels for the channel list (those its search finds), as far as they are loaded. */
    val list: StateFlow<ChannelPages> = panel.pages

    /** Where the channel playing is in the channel list; `null` while unknown, or when it isn't in it. */
    val listIndex: StateFlow<Int?> = combine(current, shown, mutableListQuery.map { it.isNotBlank() }.distinctUntilChanged(), ::Triple).flatMapLatest { (id, c, searching) ->
        if (searching) return@flatMapLatest flowOf(null)
        deps.live.around(c, id).map { load ->
            load.valueOrNull?.let { page -> page.channels.indexOfFirst { it.id == id }.takeIf { it >= 0 }?.plus(page.offset) }
        }
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(LiveTvViewModel.STOP_AFTER_MS), null)

    init {
        tuneNow(current.value)
        // What's on moves on while watching: asked again when the programme playing ends.
        viewModelScope.launch {
            combine(now, channel) { t, c -> c?.now?.let { it.stop <= t } == true }.distinctUntilChanged().collect { ended ->
                if (ended) deps.live.refresh()
            }
        }
    }

    /**
     * Watches [channelId] in place of what plays; [seen] is what a list knew
     * of it. Chosen in another category's list ([from]), channel up and down
     * go through that one from then on.
     */
    fun tune(channelId: Long, seen: LiveChannel? = null, from: LiveCategory? = null) {
        seen?.let { known[it.id] = it }
        if (from != null && from != zapCategory.value) {
            zapCategory.value = from
            recentOrder = null
            saved[CATEGORY] = from.key
        }
        if (channelId == state.value.channelId && state.value.phase !is LivePhase.Failed) return
        tuneNow(channelId)
    }

    private fun tuneNow(channelId: Long) {
        saved[CHANNEL] = channelId
        saved[CATEGORY] = zapCategory.value.key
        current.value = channelId
        LastWatched.channel.value = channelId
        deps.work.launch { deps.views.keep(channel = channelId) }
        playback.tune(channelId)
    }

    /** Channel up ([by] 1, the next in the category) or down (-1), round its ends. */
    fun step(by: Int) {
        zapping?.cancel()
        zapping = viewModelScope.launch {
            neighbour(current.value, by)?.let { tune(it.id, it) }
        }
    }

    /** The channel [by] places from [id] in the category, round its ends; `null` when it has no other. */
    private suspend fun neighbour(id: Long, by: Int): LiveChannel? {
        val category = zapCategory.value
        if (category == LiveCategory.Recent) {
            val list = recentOrder ?: ready(deps.live.page(category, "", 0))?.channels?.also { recentOrder = it } ?: return null
            if (list.size < 2) return null
            val at = list.indexOfFirst { it.id == id }.coerceAtLeast(0)
            return list[Math.floorMod(at + by, list.size)]
        }
        val page = ready(deps.live.around(category, id)) ?: return null
        if (page.channels.isEmpty()) return null
        val at = page.channels.indexOfFirst { it.id == id }
        // Not in this category (tuned by number, say): its first channel.
        if (at < 0) return page.channels.first()
        val j = at + by
        if (j in page.channels.indices) return page.channels[j]
        val pages = (page.total + LiveTvRepository.PAGE - 1) / LiveTvRepository.PAGE
        if (pages <= 1) return page.channels[Math.floorMod(j, page.channels.size)]
        val other = Math.floorMod(page.offset / LiveTvRepository.PAGE + by, pages)
        val next = ready(deps.live.page(category, "", other))?.channels?.takeIf { it.isNotEmpty() } ?: return null
        return if (by < 0) next.last() else next.first()
    }

    /** [flow]'s first answer: its value, or `null` when it failed. */
    private suspend fun <T> ready(flow: Flow<Loadable<T>>): T? = flow.first { it !is Loadable.Loading }.valueOrNull

    /** Rows [first] to [last] of the channel list show: their pages are loaded. */
    fun showingListRows(first: Int, last: Int) = panel.showing(first, last)

    /** The channel list opens: on the category channel up and down go through, unsearched. */
    fun openList() {
        shown.value = zapCategory.value
        mutableListQuery.value = ""
    }

    /** The channel list shows [category]'s channels; channel up and down keep theirs until one is tuned. */
    fun showList(category: LiveCategory) {
        shown.value = category
        mutableListQuery.value = ""
    }

    /** Searches the channel list (every channel); "" lists [listCategory]'s again. */
    fun searchList(query: String) {
        mutableListQuery.value = query
    }

    /**
     * A digit of a channel number: the channel numbered so among every
     * channel is tuned once no digit came for [DIGITS_WAIT_MS], or "No
     * channel 123" shows for a while.
     */
    fun digit(d: Char) {
        if (!d.isDigit()) return
        val typed = (mutableDigits.value.typed + d).take(MAX_DIGITS)
        mutableDigits.value = ChannelDigits(typed)
        digitsJob?.cancel()
        digitsJob = viewModelScope.launch {
            delay(DIGITS_WAIT_MS)
            val match = try {
                deps.live.numbered(typed)
            } catch (e: CancellationException) {
                throw e
            } catch (_: Exception) {
                null
            }
            if (match != null) {
                mutableDigits.value = ChannelDigits()
                tune(match.id, match)
            } else {
                mutableDigits.value = ChannelDigits(notFound = typed)
                delay(NOT_FOUND_MS)
                mutableDigits.value = ChannelDigits()
            }
        }
    }

    fun retry() = playback.retry()

    /** Every connection being in use: watches the channel playing on [channelId]'s connection instead (one the server said may be taken). */
    fun takeOver(channelId: Long) = playback.retry(takeOver = channelId)

    /** The screen went to the background: the connection and the decoders are let go. */
    fun stop() = playback.stop()

    /** Back in front: the channel plays again. */
    fun resume() = playback.resume()

    fun toggleFavorite() {
        val c = channel.value ?: return
        deps.work.launch {
            try {
                deps.live.setFavorite(c.id, !c.favorite)
            } catch (e: CancellationException) {
                throw e
            } catch (_: Exception) {
                // Put back by the repository: the star shows it.
            }
        }
    }

    override fun onCleared() {
        playback.close()
    }

    companion object {
        const val TICK_MS = 5_000L

        /** A typed number waits this long for another digit (the web's 1.4 s). */
        const val DIGITS_WAIT_MS = 1_400L
        const val NOT_FOUND_MS = 2_000L
        const val MAX_DIGITS = 5

        private const val CHANNEL = "live.channel"
        private const val CATEGORY = "live.category"

        /** The player on [channelId] of [category] (the one opened; the saved state's after a restart). */
        fun factory(channelId: Long, category: String): ViewModelProvider.Factory = viewModelFactory {
            initializer {
                val graph = screensGraph()
                val app = this[ViewModelProvider.AndroidViewModelFactory.APPLICATION_KEY] as Application
                val deps = LivePlayerDeps(
                    live = graph.data.liveTv,
                    playback = { scope ->
                        LivePlayback.create(
                            app,
                            graph.data.liveTv,
                            graph.player.engines,
                            graph.device,
                            scope,
                            graph.data.liveUpdates.events,
                            graph.data.liveUpdates::playing,
                            graph.data.remote::here,
                        ) { graph.caps.capabilities.snapshot.value }
                    },
                    origin = graph.data.servers.active,
                    views = graph.data.liveTvViews,
                )
                LivePlayerViewModel(deps, createSavedStateHandle(), channelId, category)
            }
        }
    }
}
