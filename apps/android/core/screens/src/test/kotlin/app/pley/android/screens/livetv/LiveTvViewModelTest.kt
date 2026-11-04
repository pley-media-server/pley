package app.pley.android.screens.livetv

import android.os.Looper
import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.ViewModelStore
import androidx.lifecycle.viewmodel.CreationExtras
import app.pley.android.data.Loadable
import app.pley.android.model.LiveCategory
import app.pley.android.model.LiveChannel
import app.pley.android.model.LiveGuide
import app.pley.android.model.LiveProgram
import app.pley.android.network.ApiException
import app.pley.android.player.PlayRequest
import app.pley.android.player.live.LivePlayback
import app.pley.android.screens.PlayerLauncher
import app.pley.android.testing.MainDispatcherRule
import app.pley.android.data.LiveTvView
import app.pley.android.testing.data.FakeLiveTv
import app.pley.android.testing.data.FakeLiveTvViews
import app.pley.android.testing.player.engine.FakePlayerEngine
import kotlin.reflect.KClass
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf

/** The Live TV page and the live player's view models. */
@OptIn(ExperimentalCoroutinesApi::class)
@RunWith(RobolectricTestRunner::class)
class LiveTvViewModelTest {
    @get:Rule
    val main = MainDispatcherRule(StandardTestDispatcher())

    /** Holds the view models made, cleared after each test: their tickers end with them. */
    private val store = ViewModelStore()

    @After
    fun clear() {
        store.clear()
        LastWatched.channel.value = null
        // What a test watched and no player took.
        ChannelHandoff.take(0)
    }

    /**
     * A test on the main dispatcher, its view models cleared before it ends:
     * runTest runs what is due until idle, and their tickers never are.
     */
    private fun liveTest(block: suspend TestScope.() -> Unit) = runTest(main.dispatcher) {
        try {
            block()
        } finally {
            store.clear()
        }
    }

    private fun <T : ViewModel> held(make: () -> T): T {
        val factory = object : ViewModelProvider.Factory {
            @Suppress("UNCHECKED_CAST")
            override fun <V : ViewModel> create(modelClass: KClass<V>, extras: CreationExtras): V = make() as V
        }
        return ViewModelProvider.create(store, factory)["vm${store.keys().size}", ViewModel::class] as T
    }

    /** The fixtures' three channels, then 200 more: three pages. */
    private val live = FakeLiveTv().apply {
        lineup.value = lineup.value + (1..200).map { LiveChannel(1000L + it, name = "Filler $it", number = "${1000 + it}", group = "Filler") }
    }
    private val views = FakeLiveTvViews()
    private val watched = mutableListOf<Pair<Long, String>>()
    private val launcher = object : PlayerLauncher {
        override fun play(request: PlayRequest) = Unit

        override fun watchLive(channelId: Long, category: String, castFrom: app.pley.android.player.CastFrom?) {
            watched += channelId to category
        }
    }

    /** 15:40 UTC, while the fixtures' briefing is on. */
    private var clock = 1_761_752_400_000L

    private fun TestScope.page(saved: SavedStateHandle = SavedStateHandle()): LiveTvViewModel {
        val deps = LiveTvDeps(live, launcher, flowOf("http://nas:7539"), clock = { clock }, compute = main.dispatcher, work = backgroundScope, views = views)
        val vm = held { LiveTvViewModel(deps, saved) }
        backgroundScope.launch(main.dispatcher) { vm.state.collect { } }
        backgroundScope.launch(main.dispatcher) { vm.channels.collect { } }
        backgroundScope.launch(main.dispatcher) { vm.guide.collect { } }
        backgroundScope.launch(main.dispatcher) { vm.landing.collect { } }
        runCurrent()
        return vm
    }

    @Test
    fun `the page lists a category's channels a page at a time`() = liveTest {
        val vm = page()
        assertEquals(5, vm.state.value.groups?.groups?.size)
        // The first page says how many there are.
        assertEquals(203, vm.channels.value.total)
        assertEquals(100, vm.channels.value.loaded.size)
        assertNull(vm.channels.value[150])
        vm.showingRows(150, 160)
        runCurrent()
        assertEquals(1148L, vm.channels.value[150]?.id)

        vm.setCategory(LiveCategory.Group("News"))
        vm.setQuery("weather")
        runCurrent()
        assertEquals(listOf(5L), vm.channels.value.loaded.map { it.id })
        assertEquals(1, vm.channels.value.total)
    }

    @Test
    fun `a page that failed is asked for again once 10 s have passed, as the list moves`() = liveTest {
        live.failingPages[1] = ApiException(503, "unavailable", "Busy.")
        val vm = page()
        vm.showingRows(150, 160)
        runCurrent()
        assertNull(vm.channels.value[150])
        val asked = live.pagesAsked.count { it == 1 }
        live.failingPages.remove(1)
        // Moving on within 10 s: not yet.
        clock += ChannelPaging.RETRY_MS / 2
        vm.showingRows(151, 161)
        runCurrent()
        assertEquals(asked, live.pagesAsked.count { it == 1 })
        assertNull(vm.channels.value[150])
        // Once they have passed, as the list moves: asked for again, and in.
        clock += ChannelPaging.RETRY_MS / 2
        vm.showingRows(152, 162)
        runCurrent()
        assertEquals(asked + 1, live.pagesAsked.count { it == 1 })
        assertEquals(1148L, vm.channels.value[150]?.id)
    }

    @Test
    fun `a search looks among every channel, names the groups it matches, and ends with a category chosen`() = liveTest {
        val vm = page()
        vm.setCategory(LiveCategory.Group("News"))
        vm.setQuery("sport")
        runCurrent()
        // Sport 1080i isn't in News: found all the same.
        assertEquals(listOf(2L), vm.channels.value.loaded.map { it.id })
        assertEquals(LiveCategory.All, vm.state.value.listed)
        assertEquals(listOf("Sport", "UK| Sport"), vm.state.value.found.map { it.name })

        vm.watch(vm.channels.value[0]!!)
        assertEquals(listOf(2L to "all"), watched)

        vm.setCategory(LiveCategory.Group("UK| Sport"))
        runCurrent()
        assertEquals("", vm.state.value.query)
        assertEquals(emptyList<String>(), vm.state.value.found.map { it.name })
    }

    @Test
    fun `the view and the category are kept with the page`() = liveTest {
        val saved = SavedStateHandle()
        val vm = page(saved)
        vm.setView(LiveView.Channels)
        vm.setCategory(LiveCategory.Favorites)
        runCurrent()
        val again = page(saved)
        assertEquals(LiveView.Channels, again.state.value.view)
        assertEquals(LiveCategory.Favorites, again.state.value.category)
    }

    @Test
    fun `the category, the view and the channel last watched are kept for the next launch`() = liveTest {
        val vm = page()
        vm.setView(LiveView.Channels)
        vm.setCategory(LiveCategory.Group("News"))
        runCurrent()
        assertEquals(LiveTvView("group:News", "Channels", null), views.kept)

        // A cold start: no saved state, nothing watched in this process.
        views.kept = views.kept.copy(channel = 5)
        val again = page()
        assertEquals(LiveView.Channels, again.state.value.view)
        assertEquals(LiveCategory.Group("News"), again.state.value.category)
        assertEquals(5L, LastWatched.channel.value)
        assertEquals(Landing(LiveCategory.Group("News"), "", 1), again.landing.value)
        assertEquals(listOf(1L, 5L), again.channels.value.loaded.map { it.id })
    }

    @Test
    fun `the page's saved state wins over what was kept`() = liveTest {
        views.kept = LiveTvView("favorites", "Channels", null)
        val vm = page(SavedStateHandle(mapOf("live.category" to "recent", "live.view" to "Guide")))
        assertEquals(LiveCategory.Recent, vm.state.value.category)
        assertEquals(LiveView.Guide, vm.state.value.view)
    }

    @Test
    fun `the list opens at the channel last watched, when it is in the category`() = liveTest {
        LastWatched.channel.value = 5
        val vm = page()
        assertEquals(Landing(LiveCategory.All, "", 2), vm.landing.value)
        vm.setCategory(LiveCategory.Favorites)
        runCurrent()
        assertEquals(Landing(LiveCategory.Favorites, "", null), vm.landing.value)
    }

    @Test
    fun `a channel opens the live player in its category, and a refused favourite is told`() = liveTest {
        val vm = page()
        vm.setCategory(LiveCategory.Group("News"))
        runCurrent()
        vm.watch(vm.channels.value[0]!!)
        assertEquals(listOf(1L to "group:News"), watched)

        val events = mutableListOf<LiveTvEvent>()
        backgroundScope.launch(main.dispatcher) { vm.events.collect { events += it } }
        live.favoriteFails = ApiException(500, "internal", "no")
        vm.toggleFavorite(vm.channels.value[0]!!)
        runCurrent()
        assertEquals(listOf("favorite 1 true"), live.sent.filter { it.startsWith("favorite") })
        assertTrue(events.single() is LiveTvEvent.Failed)
    }

    @Test
    fun `the guide asks for the programmes of the pages in view, chunk by chunk`() = liveTest {
        val vm = page()
        vm.showingTimes(vm.guideStart, vm.guideStart + 3600)
        runCurrent()
        val asked = live.sent.filter { it.startsWith("guide") }
        // The first page's hundred channels, for each chunk asked.
        assertTrue(asked.isNotEmpty() && asked.all { it.substringAfter("[").split(", ").size == 100 })
        assertFalse(vm.guide.value.loaded)
        for (c in live.guides.keys) {
            live.guideChunk(c).value = Loadable.Ready(LiveGuide(c, c + 1, mapOf(1L to listOf(LiveProgram(1, c, c + 1800, "At $c")))), fresh = true)
        }
        runCurrent()
        assertTrue(vm.guide.value.loaded)
        assertEquals(live.guides.keys.sorted().map { "At $it" }, vm.guide.value.byChannel.getValue(1).map { it.title })
    }

    // ---------------------------------------------------------------- the live player

    private val engines = mutableListOf<FakePlayerEngine>()

    private fun TestScope.player(channelId: Long, category: LiveCategory = LiveCategory.All, saved: SavedStateHandle = SavedStateHandle()): LivePlayerViewModel {
        val deps = LivePlayerDeps(
            live = live,
            playback = { scope: CoroutineScope -> LivePlayback(live, { FakePlayerEngine().also { engines += it } }, { "android-1" }, scope) },
            origin = flowOf("http://nas:7539"),
            clock = { clock },
            work = backgroundScope,
            views = views,
        )
        val vm = held { LivePlayerViewModel(deps, saved, channelId, category.key) }
        settle()
        return vm
    }

    private fun TestScope.settle() {
        runCurrent()
        shadowOf(Looper.getMainLooper()).idle()
        runCurrent()
    }

    private fun lastWatch() = live.sent.last { it.startsWith("watch") }

    @Test
    fun `channel up and down go through the category's pages, round its ends`() = liveTest {
        val vm = player(1)
        assertEquals("watch 1 android-1", lastWatch())
        assertEquals("Pley News HD", vm.channel.value?.name)
        assertEquals(1L, LastWatched.channel.value)
        vm.step(1)
        settle()
        assertEquals("watch 2 android-1", lastWatch())
        // Round the start: the last page's last.
        vm.step(-1)
        settle()
        vm.step(-1)
        settle()
        assertEquals("watch 1200 android-1", lastWatch())
        // Round the end: the first page's first.
        vm.step(1)
        settle()
        assertEquals("watch 1 android-1", lastWatch())
        // From the first page's last to the second's first.
        vm.tune(1097)
        settle()
        vm.step(1)
        settle()
        assertEquals("watch 1098 android-1", lastWatch())
    }

    @Test
    fun `in a group, channel up stays in it`() = liveTest {
        player(1, LiveCategory.Group("News")).step(1)
        settle()
        assertEquals("watch 5 android-1", lastWatch())
    }

    @Test
    fun `Recently watched keeps the order it had as the player opened`() = liveTest {
        val vm = player(2, LiveCategory.Recent)
        vm.step(1)
        settle()
        assertEquals("watch 1 android-1", lastWatch())
        // Watching changed the server's order; zapping keeps the first.
        live.recent.value = listOf(1, 2)
        vm.step(1)
        settle()
        assertEquals("watch 2 android-1", lastWatch())
    }

    @Test
    fun `a typed number tunes once the digits stop, or says there is no such channel`() = liveTest {
        val vm = player(1, LiveCategory.Group("Sport"))
        vm.digit('1')
        vm.digit('0')
        vm.digit('5')
        assertEquals("105", vm.digits.value.typed)
        advanceTimeBy(LivePlayerViewModel.DIGITS_WAIT_MS + 1)
        settle()
        assertEquals("watch 5 android-1", lastWatch())

        vm.digit('9')
        advanceTimeBy(LivePlayerViewModel.DIGITS_WAIT_MS + 1)
        settle()
        assertEquals("9", vm.digits.value.notFound)
        advanceTimeBy(LivePlayerViewModel.NOT_FOUND_MS + 1)
        assertEquals(ChannelDigits(), vm.digits.value)
    }

    @Test
    fun `the player comes back on the channel it was on, in its category`() = liveTest {
        val saved = SavedStateHandle()
        val vm = player(1, LiveCategory.Group("News"), saved)
        vm.tune(5)
        settle()
        val again = player(1, LiveCategory.All, saved)
        assertEquals("watch 5 android-1", lastWatch())
        assertEquals(LiveCategory.Group("News"), again.category.value)
        assertNull(again.digits.value.notFound)
    }

    @Test
    fun `the channel list shows another group, and a channel tuned from it changes what channel up goes through`() = liveTest {
        val vm = player(2, LiveCategory.Group("Sport"))
        backgroundScope.launch(main.dispatcher) { vm.list.collect { } }
        backgroundScope.launch(main.dispatcher) { vm.listIndex.collect { } }
        settle()
        assertEquals(listOf(2L), vm.list.value.loaded.map(LiveChannel::id))

        vm.showList(LiveCategory.Group("News"))
        settle()
        assertEquals(listOf(1L, 5L), vm.list.value.loaded.map(LiveChannel::id))
        assertNull("the channel playing isn't in it", vm.listIndex.value)
        // Only looking: channel up stays in Sport.
        assertEquals(LiveCategory.Group("Sport"), vm.category.value)

        vm.tune(1, vm.list.value[0], from = vm.listCategory.value)
        settle()
        assertEquals(LiveCategory.Group("News"), vm.category.value)
        vm.step(1)
        settle()
        assertEquals("watch 5 android-1", lastWatch())

        // Opened again, the list starts on the category zapped through.
        vm.showList(LiveCategory.Favorites)
        vm.openList()
        assertEquals(LiveCategory.Group("News"), vm.listCategory.value)
    }

    @Test
    fun `the channel list's search looks among every channel`() = liveTest {
        val vm = player(1, LiveCategory.Group("News"))
        backgroundScope.launch(main.dispatcher) { vm.list.collect { } }
        vm.searchList("sport")
        settle()
        assertEquals(LiveCategory.All, vm.listed.value)
        assertEquals(listOf(2L), vm.list.value.loaded.map(LiveChannel::id))
        vm.tune(2, vm.list.value[0], from = vm.listed.value)
        settle()
        assertEquals(LiveCategory.All, vm.category.value)
        // Opened again, unsearched.
        vm.openList()
        assertEquals("", vm.listQuery.value)
    }

    @Test
    fun `the player shows the channel as the page had it, before the server says`() = liveTest {
        val vm = page()
        // A channel the server doesn't answer for yet.
        val seen = LiveChannel(77, name = "Seventy-seven", number = "77")
        vm.watch(seen)
        assertEquals(listOf(77L to "all"), watched)
        assertEquals(seen, player(77).channel.value)
        // Taken once: a player opened on it again waits for the server.
        assertNull(player(77).channel.value)
        // A player of another channel doesn't take it for its own, and it goes.
        vm.watch(seen)
        assertEquals(1L, player(1).channel.value?.id)
        assertNull(ChannelHandoff.take(77))
    }

    @Test
    fun `the channel watched is kept for the guide after a restart`() = liveTest {
        val vm = player(1)
        vm.step(1)
        settle()
        assertEquals(2L, views.kept.channel)
    }

    @Test
    fun `the channel list opens on the channel playing`() = liveTest {
        val vm = player(5)
        backgroundScope.launch(main.dispatcher) { vm.listIndex.collect { } }
        backgroundScope.launch(main.dispatcher) { vm.list.collect { } }
        settle()
        assertEquals(2, vm.listIndex.value)
        vm.showingListRows(0, 3)
        settle()
        assertEquals(listOf(1L, 2L, 5L), vm.list.value.loaded.take(3).map(LiveChannel::id))
    }
}
