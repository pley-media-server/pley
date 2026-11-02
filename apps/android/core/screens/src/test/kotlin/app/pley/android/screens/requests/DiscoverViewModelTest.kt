package app.pley.android.screens.requests

import app.pley.android.data.RequestChange
import app.pley.android.model.DiscoverDetail
import app.pley.android.model.DiscoverList
import app.pley.android.model.DiscoverPage
import app.pley.android.model.MediaRequest
import app.pley.android.model.NewRequest
import app.pley.android.model.RequestList
import app.pley.android.model.RequestScope
import app.pley.android.model.RequestSummary
import app.pley.android.model.TitleKind
import app.pley.android.model.TitleStatus
import app.pley.android.network.ApiException
import app.pley.android.screens.PleyKey
import app.pley.android.testing.Fixtures
import app.pley.android.testing.data.FakeRequests
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

/** Discover: its hero, its rows read page by page, badges following events, the person's requests. */
@OptIn(ExperimentalCoroutinesApi::class)
class DiscoverViewModelTest {
    private val dispatcher = StandardTestDispatcher()
    private val trending: DiscoverPage = Fixtures.decode("discover/trending.json")
    private val summary: RequestSummary = Fixtures.decode("requests/summary.json")

    @Before
    fun main() = Dispatchers.setMain(dispatcher)

    @After
    fun reset() = Dispatchers.resetMain()

    /** "Your requests" are read every 10 s while one downloads, without end: these tests step the clock, never run it out. */
    private fun TestScope.settle() {
        advanceTimeBy(1_000)
        runCurrent()
    }

    private fun TestScope.viewModel(requests: FakeRequests): DiscoverViewModel {
        val vm = DiscoverViewModel(RequestsDeps(requests, MutableStateFlow("http://pley"), MutableStateFlow(true), dispatcher))
        backgroundScope.launch { vm.state.collect {} }
        return vm
    }

    private fun DiscoverViewModel.row(list: DiscoverList) = state.value.rows.first { it.list == list }.pages

    @Test
    fun theHeroIsTheFirstTrendingTitleWithItsMainAction() = runTest(dispatcher) {
        val vm = viewModel(FakeRequests())
        settle()
        val hero = vm.state.value.hero!!
        assertEquals("The Wild Robot", hero.title.title)
        // Its page was read for the exact action.
        assertEquals(MainAction.Request(Ask.Yes), hero.action)
        assertEquals(null, vm.state.value.problem)
    }

    @Test
    fun aShortPageIsNotTheEndAndEachTitleShowsOnce() = runTest(dispatcher) {
        val requests = FakeRequests()
        val asked = mutableListOf<Int>()
        requests.pages = { _, page ->
            asked += page
            when (page) {
                // Titles above an age limit were left out: none on page 1, yet more follow.
                1 -> DiscoverPage(1, 3, emptyList())
                2 -> DiscoverPage(2, 3, trending.results.take(2))
                else -> DiscoverPage(page, 3, trending.results.drop(1))
            }
        }
        val vm = viewModel(requests)
        settle()
        val row = vm.row(DiscoverList.Trending)
        assertEquals(listOf(1, 2, 3), asked)
        assertEquals(trending.results.map { it.tmdbId }, row.titles.map { it.tmdbId })
        assertFalse(row.more)
    }

    @Test
    fun withNothingTrendingTheHeroIsTheNextRowsFirstTitleAndEveryRowIsRead() = runTest(dispatcher) {
        val requests = FakeRequests()
        val lists = mutableListOf<DiscoverList>()
        requests.pages = { list, page ->
            lists += list
            // Nothing trending this account may ask for; the movies come later than the shows.
            when (list) {
                DiscoverList.Trending -> DiscoverPage(page, 1, emptyList())
                DiscoverList.Shows -> DiscoverPage(page, 1, trending.results.filter { it.kind == TitleKind.Show })
                else -> DiscoverPage(page, 1, emptyList())
            }
        }
        val vm = viewModel(requests)
        settle()
        assertEquals(DiscoverList.entries.toSet(), lists.toSet())
        val hero = vm.state.value.hero!!
        assertEquals(trending.results.first { it.kind == TitleKind.Show }.tmdbId, hero.title.tmdbId)
        assertFalse(vm.state.value.nothing)
    }

    @Test
    fun withNothingInAnyRowThePageSaysSo() = runTest(dispatcher) {
        val requests = FakeRequests()
        requests.pages = { _, page -> DiscoverPage(page, 1, emptyList()) }
        val vm = viewModel(requests)
        settle()
        assertEquals(null, vm.state.value.hero)
        assertTrue(vm.state.value.nothing)
        assertEquals(null, vm.state.value.problem)
    }

    @Test
    fun aRowStillShortAfterItsPagesWaitsForAScroll() = runTest(dispatcher) {
        val requests = FakeRequests()
        val asked = mutableListOf<Int>()
        requests.pages = { _, page ->
            asked += page
            DiscoverPage(page, 30, if (page == 2) trending.results.take(1) else emptyList())
        }
        val vm = viewModel(requests)
        settle()
        val row = vm.row(DiscoverList.Trending)
        assertEquals((1..5).toList(), asked)
        assertTrue(row.stopped && row.settled)
        assertEquals(1, row.titles.size)
        vm.more(DiscoverList.Trending)
        settle()
        assertEquals(6, asked.last())
        assertFalse(vm.row(DiscoverList.Trending).stopped)
    }

    @Test
    fun rowsAreReadAsTheyComeIntoView() = runTest(dispatcher) {
        val requests = FakeRequests()
        val lists = mutableListOf<DiscoverList>()
        requests.pages = { list, page ->
            lists += list
            DiscoverPage(page, 1, trending.results)
        }
        val vm = viewModel(requests)
        settle()
        assertEquals(listOf(DiscoverList.Trending), lists.distinct())
        vm.showRow(DiscoverList.Anime)
        settle()
        assertEquals(listOf(DiscoverList.Trending, DiscoverList.Anime), lists.distinct())
        assertEquals(6, vm.row(DiscoverList.Anime).titles.size)
    }

    @Test
    fun badgesFollowARequestInRowsAlreadyShown() = runTest(dispatcher) {
        val requests = FakeRequests()
        val vm = viewModel(requests)
        settle()
        val pending = Fixtures.decode<RequestList>("requests/list.json").requests.first { it.id == 44L }
        requests.api.answer("request", pending.copy(id = 61, kind = TitleKind.Movie, tmdbId = 1184918))
        val pagesBefore = requests.api.calls.size
        requests.change(RequestChange(61, TitleKind.Movie, 1184918))
        settle()
        val robot = vm.row(DiscoverList.Trending).titles.first { it.tmdbId == 1184918L }
        assertEquals(TitleStatus.Requested, robot.status)
        assertEquals(TitleBadge.Requested, badgeOf(robot))
        // One request read, the rows not read again.
        assertEquals(listOf("request"), requests.api.calls.drop(pagesBefore).map { it.method }.filter { it != "requests" })
    }

    @Test
    fun aRequestNoLongerReadableLeavesTheTitlesStatusToItsPage() = runTest(dispatcher) {
        val requests = FakeRequests()
        val vm = viewModel(requests)
        settle()
        // The Substance (933260): Requested by someone this account stopped following.
        requests.api.fail("request", ApiException(404, "not_found", "no such request"))
        val page: DiscoverDetail = Fixtures.decode("discover/movie.json")
        requests.api.answer("discoverTitle", page.copy(tmdbId = 933260, status = TitleStatus.Requested, requestId = null))
        requests.change(RequestChange(44, TitleKind.Movie, 933260))
        settle()
        val substance = vm.row(DiscoverList.Trending).titles.first { it.tmdbId == 933260L }
        assertEquals(TitleStatus.Requested, substance.status)
        assertEquals(null, substance.requestId)
        assertEquals(listOf<Any?>(TitleKind.Movie, 933260L), requests.api.calls("discoverTitle").last().args)

        // Its page can't be read either: the status stays, the request let go of.
        requests.api.fail("discoverTitle", ApiException(503, "tmdb_down", "TMDB isn't answering"))
        requests.change(RequestChange(44, TitleKind.Movie, 933260))
        settle()
        assertEquals(TitleStatus.Requested, vm.row(DiscoverList.Trending).titles.first { it.tmdbId == 933260L }.status)
    }

    @Test
    fun signedInBeforeRequestsShowsOnlySigningInAgain() = runTest(dispatcher) {
        val requests = FakeRequests(summary.copy(signInAgain = RequestScope.Requests, own = 0, unseen = 0))
        val read = mutableListOf<Int>()
        requests.pages = { _, page -> read += page; trending }
        val vm = viewModel(requests)
        settle()
        assertEquals(RequestProblem.SignInAgain(RequestScope.Requests), vm.state.value.problem)
        assertEquals(emptyList<Int>(), read)
    }

    @Test
    fun tmdbAwayShowsItsStateAndRetryReadsAgain() = runTest(dispatcher) {
        val requests = FakeRequests()
        var down = true
        requests.pages = { _, page -> if (down) throw ApiException(503, "tmdb_down", "TMDB isn't answering") else trending.copy(page = page) }
        val vm = viewModel(requests)
        settle()
        assertEquals(RequestProblem.TmdbDown, vm.state.value.problem)
        down = false
        vm.retry()
        settle()
        assertEquals(null, vm.state.value.problem)
        assertEquals("The Wild Robot", vm.state.value.hero?.title?.title)
    }

    @Test
    fun theHeroAsksForAMovieAndOpensAShowsPicker() = runTest(dispatcher) {
        val requests = FakeRequests()
        val vm = viewModel(requests)
        val events = mutableListOf<RequestEvent>()
        // Unconfined: an event is taken as it's sent, before the clock's step ends.
        backgroundScope.launch(UnconfinedTestDispatcher(testScheduler)) { vm.events.collect { events += it } }
        settle()
        vm.heroAction()
        settle()
        assertEquals(NewRequest(TitleKind.Movie, 1184918), requests.api.calls("ask").single().args.single())
        assertTrue(events.single() is RequestEvent.Asked)

        // A show first: its picker, on its page.
        val severance = trending.results.first { it.tmdbId == 95396L }.copy(status = TitleStatus.None, itemId = null)
        requests.pages = { _, page -> DiscoverPage(page, 1, listOf(severance)) }
        val other = viewModel(requests)
        val opened = mutableListOf<RequestEvent>()
        backgroundScope.launch(UnconfinedTestDispatcher(testScheduler)) { other.events.collect { opened += it } }
        settle()
        other.heroAction()
        settle()
        assertEquals(RequestEvent.Open(PleyKey.DiscoverTitle(TitleKind.Show, 95396, pick = true)), opened.single())

        // Partly in a library: Open goes to what the library has.
        val partly = trending.results.first { it.tmdbId == 95396L }
        requests.pages = { _, page -> DiscoverPage(page, 1, listOf(partly)) }
        val third = viewModel(requests)
        val went = mutableListOf<RequestEvent>()
        backgroundScope.launch(UnconfinedTestDispatcher(testScheduler)) { third.events.collect { went += it } }
        settle()
        third.heroAction()
        settle()
        assertEquals(RequestEvent.Open(PleyKey.Item(201)), went.single())
    }

    @Test
    fun theHerosAskWaitsForTheOneOnItsWay() = runTest(dispatcher) {
        val requests = FakeRequests()
        val vm = viewModel(requests)
        val events = mutableListOf<RequestEvent>()
        backgroundScope.launch(UnconfinedTestDispatcher(testScheduler)) { vm.events.collect { events += it } }
        settle()
        requests.api.hold()
        vm.heroAction()
        vm.heroAction()
        runCurrent()
        assertTrue(vm.state.value.asking)
        requests.api.release()
        settle()
        assertEquals(1, requests.api.calls("ask").size)
        assertEquals(1, events.count { it is RequestEvent.Asked })
        assertFalse(vm.state.value.asking)
    }

    @Test
    fun yourRequestsShowForThoseWhoHaveAny() = runTest(dispatcher) {
        val requests = FakeRequests()
        val vm = viewModel(requests)
        settle()
        assertTrue(vm.state.value.yours.isNotEmpty())
        val none = FakeRequests(summary.copy(own = 0, unseen = 0))
        val other = viewModel(none)
        settle()
        assertEquals(emptyList<MediaRequest>(), other.state.value.yours)
        assertTrue(none.api.calls("requests").isEmpty())
    }

    @Test
    fun yourRequestsAreReadAgainEvery10sWhileOneDownloads() = runTest(dispatcher) {
        val requests = FakeRequests()
        val vm = viewModel(requests)
        settle()
        // Breaking Bad (42) downloads: its progress sends no events.
        assertTrue(vm.state.value.yours.any { it.id == 42L })
        val reads = requests.api.calls("requests").size
        advanceTimeBy(10_000)
        runCurrent()
        assertEquals(reads + 1, requests.api.calls("requests").size)

        // Once nothing downloads, they're read only as requests change.
        requests.api.answer("requests", RequestList(total = 1, requests = listOf(Fixtures.decode<RequestList>("requests/list.json").requests.first { it.id == 44L })))
        advanceTimeBy(10_000)
        runCurrent()
        val quiet = requests.api.calls("requests").size
        advanceTimeBy(60_000)
        runCurrent()
        assertEquals(quiet, requests.api.calls("requests").size)
    }
}
