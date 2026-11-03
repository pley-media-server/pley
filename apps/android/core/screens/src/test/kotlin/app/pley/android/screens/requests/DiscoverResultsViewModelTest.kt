package app.pley.android.screens.requests

import app.pley.android.data.RequestChange
import app.pley.android.model.DiscoverList
import app.pley.android.model.DiscoverPage
import app.pley.android.model.RequestList
import app.pley.android.model.RequestScope
import app.pley.android.model.RequestSummary
import app.pley.android.model.TitleKind
import app.pley.android.model.TitleStatus
import app.pley.android.network.ApiException
import app.pley.android.testing.Fixtures
import app.pley.android.testing.data.FakeRequests
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

/** The See all of Search's Discover row: every title TMDB finds for the words, page by page, badges following events, and why when it can't show. */
@OptIn(ExperimentalCoroutinesApi::class)
class DiscoverResultsViewModelTest {
    private val dispatcher = StandardTestDispatcher()
    private val trending: DiscoverPage = Fixtures.decode("discover/trending.json")
    private val summary: RequestSummary = Fixtures.decode("requests/summary.json")

    @Before
    fun main() = Dispatchers.setMain(dispatcher)

    @After
    fun reset() = Dispatchers.resetMain()

    private fun TestScope.viewModel(requests: FakeRequests, query: String = "robot"): DiscoverResultsViewModel {
        val vm = DiscoverResultsViewModel(query, RequestsDeps(requests, MutableStateFlow("http://pley"), MutableStateFlow(true), dispatcher))
        backgroundScope.launch { vm.state.collect {} }
        return vm
    }

    /** A TV's grid of [source]: [screenful] titles wanted first, five pages at most at a time. */
    private fun TestScope.tvGrid(requests: FakeRequests, source: DiscoverSource, screenful: Int = 28): DiscoverResultsViewModel {
        val deps = RequestsDeps(requests, MutableStateFlow("http://pley"), MutableStateFlow(true), dispatcher)
        val vm = DiscoverResultsViewModel(source, deps, screenful, DiscoverResultsViewModel.TV_FILL)
        backgroundScope.launch { vm.state.collect {} }
        return vm
    }

    /** Page [page] of 30: six titles of its own from page [from] on, none before. */
    private fun page(page: Int, from: Int = 1) =
        DiscoverPage(page, 30, if (page < from) emptyList() else trending.results.map { it.copy(tmdbId = it.tmdbId * 100 + page) })

    private fun DiscoverResultsViewModel.ready() = state.value.search as DiscoverSearch.Ready

    @Test
    fun everyTitleTmdbFindsIsReadUntilAScreenfulThenAsTheGridScrolls() = runTest(dispatcher) {
        val requests = FakeRequests()
        val asked = mutableListOf<Triple<String, Int, Boolean>>()
        requests.searches = { q, page, missing ->
            asked += Triple(q, page, missing)
            DiscoverPage(page, 30, trending.results.map { it.copy(tmdbId = it.tmdbId * 100 + page) })
        }
        val vm = viewModel(requests)
        advanceUntilIdle()
        // Six titles a page: four pages for a screenful, those in a library too.
        assertEquals((1..4).map { Triple("robot", it, false) }, asked)
        assertEquals(24, vm.ready().pages.titles.size)
        assertEquals("http://pley", vm.state.value.origin)
        vm.more()
        advanceUntilIdle()
        assertEquals(5, asked.last().second)
        assertEquals(30, vm.ready().pages.titles.size)
    }

    @Test
    fun aDiscoverRowIsShownWholeFromItsList() = runTest(dispatcher) {
        val requests = FakeRequests()
        val asked = mutableListOf<Pair<DiscoverList, Int>>()
        requests.pages = { list, n ->
            asked += list to n
            page(n)
        }
        val vm = tvGrid(requests, DiscoverSource.Row(DiscoverList.Movies), screenful = 12)
        advanceUntilIdle()
        assertEquals(listOf(DiscoverList.Movies to 1, DiscoverList.Movies to 2), asked)
        assertEquals(DiscoverList.Movies, vm.state.value.list)
        assertEquals("", vm.state.value.query)
        assertEquals(12, vm.ready().pages.titles.size)
        assertTrue(requests.api.calls("discoverSearch").isEmpty())

        // Focus near the grid's end wants three rows more: read until they're in.
        vm.want(24)
        advanceUntilIdle()
        assertEquals((1..4).map { DiscoverList.Movies to it }, asked)
        assertEquals(24, vm.ready().pages.titles.size)
    }

    @Test
    fun aTvGridStopsShortAfterFivePagesAndShowMoreReadsFiveMore() = runTest(dispatcher) {
        val requests = FakeRequests()
        val asked = mutableListOf<Int>()
        // An age limit left nothing on the first seven pages.
        requests.searches = { _, n, _ ->
            asked += n
            page(n, from = 8)
        }
        val vm = tvGrid(requests, DiscoverSource.Words("robot"))
        advanceUntilIdle()
        assertEquals((1..5).toList(), asked)
        val stopped = vm.ready().pages
        assertTrue(stopped.stopped && stopped.settled && stopped.titles.isEmpty())

        // Focus can't come near an end without titles; and while stopped, wanting more reads nothing.
        vm.want(40)
        advanceUntilIdle()
        assertEquals((1..5).toList(), asked)

        vm.showMore()
        advanceUntilIdle()
        assertEquals((1..10).toList(), asked)
        assertEquals(18, vm.ready().pages.titles.size)
        assertTrue(vm.ready().pages.stopped)

        vm.showMore()
        advanceUntilIdle()
        // Up to the 40 wanted, then it waits for focus to come near the end again.
        assertEquals((1..14).toList(), asked)
        assertEquals(42, vm.ready().pages.titles.size)
        assertTrue(!vm.ready().pages.stopped)
    }

    @Test
    fun aPageThatFailsAfterTheFirstKeepsTheTitlesAndShowMoreGoesOnFromIt() = runTest(dispatcher) {
        val requests = FakeRequests()
        var down = false
        val asked = mutableListOf<Int>()
        requests.searches = { _, n, _ ->
            asked += n
            if (down) throw ApiException(503, "tmdb_down", "TMDB isn't answering") else page(n)
        }
        val vm = tvGrid(requests, DiscoverSource.Words("robot"), screenful = 12)
        advanceUntilIdle()
        down = true
        vm.want(24)
        advanceUntilIdle()
        assertEquals(RequestProblem.TmdbDown, vm.ready().pages.problem)
        assertEquals(12, vm.ready().pages.titles.size)
        down = false
        vm.showMore()
        advanceUntilIdle()
        assertEquals(listOf(1, 2, 3, 3, 4), asked)
        assertEquals(null, vm.ready().pages.problem)
        assertEquals(24, vm.ready().pages.titles.size)
    }

    @Test
    fun wordsPastTheServersLimitAreCutToIt() = runTest(dispatcher) {
        val requests = FakeRequests()
        val asked = mutableListOf<String>()
        requests.searches = { q, page, _ ->
            asked += q
            trending.copy(page = page)
        }
        val vm = viewModel(requests, query = " " + "dune ".repeat(50))
        advanceUntilIdle()
        val cut = "dune ".repeat(40).trim()
        assertEquals(cut, vm.state.value.query)
        assertEquals(listOf(cut), asked.distinct())
    }

    @Test
    fun badgesFollowARequestInTheResults() = runTest(dispatcher) {
        val requests = FakeRequests()
        val vm = viewModel(requests)
        advanceUntilIdle()
        val pending = Fixtures.decode<RequestList>("requests/list.json").requests.first { it.id == 44L }
        requests.api.answer("request", pending.copy(id = 61, kind = TitleKind.Movie, tmdbId = 1184918))
        val searches = requests.api.calls("discoverSearch").size
        requests.change(RequestChange(61, TitleKind.Movie, 1184918))
        advanceUntilIdle()
        assertEquals(TitleStatus.Requested, vm.ready().pages.titles.first { it.tmdbId == 1184918L }.status)
        // The results aren't read again.
        assertEquals(searches, requests.api.calls("discoverSearch").size)
    }

    @Test
    fun nothingOnTmdbIsEmpty() = runTest(dispatcher) {
        val requests = FakeRequests()
        requests.searches = { _, page, _ -> DiscoverPage(page, 1, emptyList()) }
        val vm = viewModel(requests, query = "zzz")
        advanceUntilIdle()
        assertEquals(DiscoverSearch.Empty("zzz"), vm.state.value.search)
    }

    @Test
    fun tmdbAwaySaysSoAndRetrySearchesAgain() = runTest(dispatcher) {
        val requests = FakeRequests()
        var down = true
        requests.searches = { _, page, _ -> if (down) throw ApiException(503, "tmdb_down", "TMDB isn't answering") else trending.copy(page = page) }
        val vm = viewModel(requests)
        advanceUntilIdle()
        assertEquals(DiscoverSearch.Failed("robot", RequestProblem.TmdbDown), vm.state.value.search)
        down = false
        vm.retry()
        advanceUntilIdle()
        assertEquals(trending.results.size, vm.ready().pages.titles.size)
    }

    @Test
    fun whereAskingDoesNotWorkTheGateSaysWhyAndTmdbIsNotAsked() = runTest(dispatcher) {
        val before = FakeRequests(summary.copy(signInAgain = RequestScope.Requests, own = 0, unseen = 0))
        val vm = viewModel(before)
        advanceUntilIdle()
        assertEquals(RequestProblem.SignInAgain(RequestScope.Requests), vm.state.value.problem)
        assertTrue(before.api.calls("discoverSearch").isEmpty())

        val off = FakeRequests(summary.copy(enabled = false))
        val other = viewModel(off)
        advanceUntilIdle()
        assertEquals(RequestProblem.Off, other.state.value.problem)
        assertTrue(off.api.calls("discoverSearch").isEmpty())
    }
}
