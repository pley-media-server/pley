@file:OptIn(ExperimentalCoroutinesApi::class)

package app.pley.android.screens.home

import app.pley.android.data.Loadable
import app.pley.android.data.PlayTarget
import app.pley.android.data.ScanState
import app.pley.android.model.Hub
import app.pley.android.model.Hubs
import app.pley.android.model.ItemDetail
import app.pley.android.model.ItemKind
import app.pley.android.model.ItemSummary
import app.pley.android.network.OfflineException
import app.pley.android.player.PlayRequest
import app.pley.android.screens.PleyKey
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

class HomeViewModelTest {
    private val dispatcher = UnconfinedTestDispatcher()
    private val sources = FakeHomeSources()

    @Before
    fun main() = Dispatchers.setMain(dispatcher)

    @After
    fun reset() = Dispatchers.resetMain()

    private fun TestScope.home(): HomeViewModel = HomeViewModel(sources, dispatcher).also { vm ->
        // Something on screen keeps the state flowing.
        backgroundScope.launch { vm.state.collect { } }
        backgroundScope.launch { vm.status.collect { } }
        backgroundScope.launch { vm.info.collect { } }
    }

    @Test
    fun homeIsDrawnFromTheSnapshotWithoutTheNetwork() = runTest(dispatcher) {
        val vm = home()
        assertSame(HomeState.Loading, vm.state.value)
        assertNull(vm.status.value)
        sources.home.value = Loadable.Ready(HOME, fresh = false)
        val saved = vm.state.value as HomeState.Ready
        assertEquals(Loadable.Ready(Unit, fresh = false), vm.status.value)
        assertEquals(listOf("continue_watching", "recently_added:1", "recently_added:2", "recently_added:3"), saved.rows.map { it.key })
        assertEquals(GLASSHOUSE, saved.hero?.item)
        assertEquals(FakeHomeSources.ORIGIN, saved.origin)

        // The server can't be reached: what was saved stays, marked as such.
        val offline = OfflineException(java.net.ConnectException("refused"))
        sources.home.value = Loadable.Failed(offline, HOME)
        val stale = vm.state.value as HomeState.Ready
        assertEquals(Loadable.Failed(offline, Unit), vm.status.value)
        assertEquals(4, stale.rows.size)
    }

    @Test
    fun theServerSayingWhatWasSavedRedrawsNothing() = runTest(dispatcher) {
        val vm = home()
        var shapings = 0
        val shaped = vm.present { _: HomeState -> ++shapings }
        backgroundScope.launch { shaped.collect { } }
        sources.home.value = Loadable.Ready(HOME, fresh = false)
        val saved = vm.state.value
        val shaped0 = shapings
        // Coming back to Home, the server is asked again and says the same: only the status moves.
        sources.home.value = Loadable.Ready(HOME, fresh = true)
        assertSame(saved, vm.state.value)
        assertEquals(shaped0, shapings)
        assertEquals(Loadable.Ready(Unit, fresh = true), vm.status.value)
    }

    @Test
    fun theHeroGetsItsDetailsAndKeepsThemThroughNewRows() = runTest(dispatcher) {
        val vm = home()
        sources.home.value = Loadable.Ready(HOME, fresh = false)
        val detail = ItemDetail(
            id = GLASSHOUSE.id, libraryId = 2, kind = ItemKind.Episode, title = "Glasshouse",
            summary = "The lamp goes out.", contentRating = "TV-14",
        )
        sources.detail(GLASSHOUSE.id).value = Loadable.Ready(detail, fresh = true)
        assertEquals("TV-14", (vm.state.value as HomeState.Ready).hero?.meta?.contentRating)
        // A fresh answer for the rows: the hero is never drawn without what was known of it.
        sources.home.value = Loadable.Ready(HOME.copy(hubs = HOME.hubs.reversed()), fresh = true)
        val ready = vm.state.value as HomeState.Ready
        assertEquals(Loadable.Ready(Unit, fresh = true), vm.status.value)
        assertEquals("TV-14", ready.hero?.meta?.contentRating)
    }

    @Test
    fun theFirstThreeContinueWatchingDetailsAreAskedForOnce() = runTest(dispatcher) {
        home()
        sources.home.value = Loadable.Ready(HOME, fresh = false)
        sources.home.value = Loadable.Ready(HOME, fresh = true)
        assertEquals(listOf(GLASSHOUSE.id, ASHFALL.id, SALTWIND.id), sources.prefetched)
    }

    @Test
    fun aShowPlaysItsOnDeckEpisode() = runTest(dispatcher) {
        val vm = home()
        val navigator = RecordingNavigator()
        sources.resolve = { item, fromStart ->
            if (item.kind == ItemKind.Show) PlayTarget.Video(GLASSHOUSE.id, if (fromStart) 0 else GLASSHOUSE.userState.positionMs) else PlayTarget.NothingToPlay
        }
        vm.play(OVERGROWTH, fromStart = false, navigator)
        advanceUntilIdle()
        assertEquals(listOf(PlayRequest(itemId = GLASSHOUSE.id, startMs = 32 * 60_000L, fromStart = false)), sources.played)
        vm.play(OVERGROWTH, fromStart = true, navigator)
        advanceUntilIdle()
        assertEquals(PlayRequest(itemId = GLASSHOUSE.id, startMs = 0, fromStart = true), sources.played.last())
        assertTrue(navigator.opened.isEmpty())
    }

    @Test
    fun aMovieResumesAndMusicOpensItsPage() = runTest(dispatcher) {
        val vm = home()
        val navigator = RecordingNavigator()
        vm.play(ASHFALL, fromStart = false, navigator)
        advanceUntilIdle()
        assertEquals(PlayRequest(itemId = ASHFALL.id, startMs = 50 * 60_000L, fromStart = false), sources.played.single())

        sources.resolve = { item, _ -> PlayTarget.Music(listOf(1, 2), 0) }
        vm.play(ALBUM, fromStart = false, navigator)
        advanceUntilIdle()
        assertEquals(listOf<PleyKey>(PleyKey.Album(ALBUM.id)), navigator.opened)
        assertEquals(1, sources.played.size)
    }

    @Test
    fun nothingToPlaySaysSo() = runTest(dispatcher) {
        val vm = home()
        sources.resolve = { _, _ -> PlayTarget.NothingToPlay }
        vm.play(OVERGROWTH, fromStart = false, RecordingNavigator())
        assertSame(HomeMessage.NothingToPlay, vm.messages.first())
        assertTrue(sources.played.isEmpty())
    }

    @Test
    fun aFailedPlaySaysWhy() = runTest(dispatcher) {
        val vm = home()
        val error = IllegalStateException("the server said no")
        sources.resolve = { _, _ -> throw error }
        vm.play(OVERGROWTH, fromStart = false, RecordingNavigator())
        // (Coroutines may hand back a copy of the exception, with its stack trace recovered.)
        val message = vm.messages.first() as HomeMessage.Failed
        assertEquals(error.message, message.error.message)
    }

    @Test
    fun watchedGoesToTheRepository() = runTest(dispatcher) {
        val vm = home()
        vm.setWatched(ASHFALL, true)
        advanceUntilIdle()
        assertEquals(listOf(ASHFALL.id to true), sources.watched)
    }

    @Test
    fun theInfoBlockFollowsFocusAndAsksAhead() = runTest(dispatcher) {
        val vm = home()
        assertNull(vm.info.value)
        vm.focus(ASHFALL, ahead = listOf(SALTWIND, PARALLEL))
        assertEquals(ASHFALL, vm.info.value?.item)
        assertFalse(vm.info.value!!.complete)
        assertEquals(listOf(SALTWIND.id, PARALLEL.id), sources.prefetched)
        sources.detail(ASHFALL.id).value = Loadable.Ready(
            ItemDetail(id = ASHFALL.id, libraryId = 1, kind = ItemKind.Movie, title = "Ashfall", summary = "Ash.", contentRating = "R"),
            fresh = true,
        )
        assertTrue(vm.info.value!!.complete)
        assertEquals("Ash.", vm.info.value!!.summary)
    }

    @Test
    fun aScanThatEndsAsksForTheRowsAgain() = runTest(dispatcher) {
        home()
        sources.scanList.value = listOf(ScanState(1, 0.2f))
        assertEquals(0, sources.hubRefreshes)
        sources.scanList.value = emptyList()
        assertEquals(1, sources.hubRefreshes)
    }

    @Test
    fun refreshAsksForRowsAndLibrariesOnce() = runTest(dispatcher) {
        val vm = home()
        sources.home.value = Loadable.Ready(HOME, fresh = true)
        vm.refresh()
        advanceUntilIdle()
        assertEquals(1, sources.hubRefreshes)
        assertEquals(1, sources.libraryRefreshes)
        assertFalse(vm.refreshing.value)
    }

    @Test
    fun presentShapesTheStateOnceAndKeepsIt() = runTest(dispatcher) {
        val vm = home()
        val shape: (HomeState) -> String = { state -> state.javaClass.simpleName }
        val shaped = vm.present(shape)
        assertSame(shaped, vm.present(shape))
        backgroundScope.launch { shaped.collect { } }
        assertEquals("Loading", shaped.value)
        sources.home.value = Loadable.Ready(HOME, fresh = true)
        assertEquals("Ready", shaped.value)
    }

    @Test
    fun theProfileComesFromTheSession() = runTest(dispatcher) {
        val vm = home()
        backgroundScope.launch { vm.profile.collect { } }
        assertNull(vm.profile.value)
        val me = app.pley.android.model.Me(id = 2, username = "anna", name = "Anna", avatarUrl = "/api/users/2/avatar?v=1", avatarColor = "#e07a5f")
        sources.sessionFlow.value = app.pley.android.data.Session(FakeHomeSources.ORIGIN, me, emptySet())
        val profile = vm.profile.value!!
        assertEquals("Anna", profile.name)
        assertEquals("${FakeHomeSources.ORIGIN}/api/users/2/avatar?v=1", profile.pictureUrl)
    }

    @Test
    fun theBannerComesWithItsDetailsAndPutsTitlesOnMyList() = runTest(dispatcher) {
        val vm = home()
        val film = ItemSummary(id = 501, libraryId = 1, kind = ItemKind.Movie, title = "Lantern", backdropUrl = "/api/items/501/images/backdrop")
        sources.libraryList.value = Loadable.Ready(listOf(MOVIES), fresh = true)
        sources.home.value = Loadable.Ready(Hubs(listOf(Hub("recently_added:1", "Recently Added in Movies", 1, listOf(film)))), fresh = true)
        val before = (vm.state.value as HomeState.Ready).featured.single().info
        assertEquals("Lantern", before.title)
        assertFalse(before.complete)

        sources.detail(501).value = Loadable.Ready(
            ItemDetail(id = 501, libraryId = 1, kind = ItemKind.Movie, title = "Lantern", summary = "A light that never goes out.", genres = listOf("Drama")),
            fresh = true,
        )
        val told = (vm.state.value as HomeState.Ready).featured.single().info
        assertEquals("A light that never goes out.", told.summary)
        assertEquals(listOf("Drama"), told.genres)
        assertFalse(told.inList)

        vm.setInList(told.item, true)
        assertEquals(listOf(501L to true), sources.listed)
        assertTrue((vm.state.value as HomeState.Ready).featured.single().info.inList)
    }

    @Test
    fun withTheBannerOffNothingIsFeaturedOrAskedForIt() = runTest(dispatcher) {
        sources.bannerOn.value = false
        val vm = home()
        val film = ItemSummary(id = 501, libraryId = 1, kind = ItemKind.Movie, title = "Lantern", backdropUrl = "/api/items/501/images/backdrop")
        sources.libraryList.value = Loadable.Ready(listOf(MOVIES), fresh = true)
        sources.home.value = Loadable.Ready(Hubs(listOf(Hub("recently_added:1", "Recently Added in Movies", 1, listOf(film)))), fresh = true)
        val off = vm.state.value as HomeState.Ready
        assertFalse(off.banner)
        assertNull(off.hero)
        assertTrue(off.featured.isEmpty())
        assertEquals(listOf("recently_added:1"), off.rows.map { it.key })
        // Neither the libraries' Top Rated rows nor the banner's details are asked for.
        assertTrue(sources.libraryHubs.isEmpty())
        assertTrue(sources.details.isEmpty())

        // Turned back on, the banner comes back without leaving Home.
        sources.bannerOn.value = true
        val on = vm.state.value as HomeState.Ready
        assertTrue(on.banner)
        assertEquals("Lantern", on.featured.single().info.title)
        assertEquals(setOf(MOVIES.id), sources.libraryHubs.keys)
    }
}
