package app.pley.android.screens.search

import app.pley.android.data.Loadable
import app.pley.android.data.RequestChange
import app.pley.android.data.RequestsRepository
import app.pley.android.data.SearchRepository
import app.pley.android.model.DiscoverPage
import app.pley.android.model.DiscoverTitle
import app.pley.android.model.ItemKind
import app.pley.android.model.ItemSummary
import app.pley.android.model.LiveChannel
import app.pley.android.model.MediaRequest
import app.pley.android.model.PersonSummary
import app.pley.android.model.RequestAccess
import app.pley.android.model.RequestStatus
import app.pley.android.model.RequestSummary
import app.pley.android.model.SearchResults
import app.pley.android.player.PlayRequest
import app.pley.android.screens.PlayerLauncher
import app.pley.android.model.TitleKind
import app.pley.android.model.TitleStatus
import app.pley.android.network.ApiException
import app.pley.android.screens.PleyKey
import app.pley.android.screens.library.FakeTile
import app.pley.android.screens.library.FakeTiles
import app.pley.android.screens.library.TileLook
import app.pley.android.screens.livetv.ChannelHandoff
import app.pley.android.screens.requests.RequestProblem
import app.pley.android.testing.Fixtures
import app.pley.android.testing.data.FakeRequests
import java.io.IOException
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

/** Search's 250 ms pause, the latest query only, the groups and the recent searches, on the test's clock. */
@OptIn(ExperimentalCoroutinesApi::class)
class SearchViewModelTest {
    private val dispatcher = StandardTestDispatcher()
    private val search = FakeSearch()

    @Before
    fun main() = Dispatchers.setMain(dispatcher)

    @After
    fun reset() = Dispatchers.resetMain()

    /** Answers the test gives per query; a query without one waits. */
    private class FakeSearch : SearchRepository {
        val asked = mutableListOf<String>()
        val cancelled = mutableListOf<String>()
        val answers = HashMap<String, CompletableDeferred<SearchResults>>()
        override val recent = MutableStateFlow(listOf("halcyon"))

        fun answer(q: String): CompletableDeferred<SearchResults> = answers.getOrPut(q) { CompletableDeferred() }

        override fun search(query: String): Flow<Loadable<SearchResults>> = flow {
            asked += query
            emit(Loadable.Loading)
            try {
                emit(Loadable.Ready(answer(query).await(), fresh = true))
            } catch (e: kotlinx.coroutines.CancellationException) {
                cancelled += query
                throw e
            } catch (e: IOException) {
                emit(Loadable.Failed(e))
            }
        }

        override suspend fun remember(query: String) {
            recent.value = (listOf(query) + recent.value.filterNot { it == query }).take(10)
        }

        override suspend fun clearRecent() {
            recent.value = emptyList()
        }
    }

    private val watched = mutableListOf<Pair<Long, String>>()
    private val launcher = object : PlayerLauncher {
        override fun play(request: PlayRequest) = Unit

        override fun watchLive(channelId: Long, category: String, castFrom: app.pley.android.player.CastFrom?) {
            watched += channelId to category
        }
    }

    private fun TestScope.viewModel(
        initial: String = "",
        requests: RequestsRepository = RequestsRepository.None,
        tmdbShown: Int = SearchViewModel.TMDB_SHOWN,
    ): SearchViewModel<FakeTile> {
        val tiles = SearchTiles.of(FakeTiles) { person, origin -> FakeTile(person.id, person.name, TileLook.Poster, "person $origin") }
        val vm = SearchViewModel(initial, SearchDeps(search, MutableStateFlow("http://pley"), dispatcher, launcher, requests, tmdbShown), tiles)
        backgroundScope.launch { vm.state.collect {} }
        backgroundScope.launch { vm.tmdb.collect {} }
        return vm
    }

    private fun SearchViewModel<FakeTile>.results() = state.value.results

    private fun SearchViewModel<FakeTile>.type(text: String) = onQueryChange(text)

    private fun item(id: Long, kind: ItemKind, title: String = "Item $id", parentId: Long? = null) =
        ItemSummary(id = id, libraryId = 1, kind = kind, title = title, parentId = parentId)

    @Test
    fun typingWaitsFor250msOfQuietBeforeAsking() = runTest(dispatcher) {
        val vm = viewModel()
        runCurrent()
        vm.type("n")
        advanceTimeBy(100)
        vm.type("no")
        advanceTimeBy(100)
        vm.type("nor")
        advanceTimeBy(249)
        assertEquals(emptyList<String>(), search.asked)
        advanceTimeBy(2)
        assertEquals(listOf("nor"), search.asked)
        assertEquals(SearchResultsState.Loading("nor"), vm.results())
    }

    @Test
    fun onlyTheLatestQuerysAnswerCounts() = runTest(dispatcher) {
        val vm = viewModel()
        vm.type("north")
        advanceTimeBy(300)
        vm.type("south")
        advanceTimeBy(300)
        assertEquals(listOf("north", "south"), search.asked)
        // The older request was given up.
        assertEquals(listOf("north"), search.cancelled)
        search.answer("north").complete(SearchResults(items = listOf(item(1, ItemKind.Movie, "Northern Lights"))))
        runCurrent()
        assertEquals(SearchResultsState.Loading("south"), vm.results())
        search.answer("south").complete(SearchResults(items = listOf(item(2, ItemKind.Movie, "South Wind"))))
        advanceUntilIdle()
        val ready = vm.results() as SearchResultsState.Ready
        assertEquals("south", ready.query)
        assertEquals(listOf("South Wind"), ready.groups.single().tiles.map { it.title })
    }

    @Test
    fun resultsStayWhileTheNextOnesLoad() = runTest(dispatcher) {
        val vm = viewModel()
        vm.type("north")
        search.answer("north").complete(SearchResults(items = listOf(item(1, ItemKind.Movie))))
        advanceUntilIdle()
        assertEquals(false, (vm.results() as SearchResultsState.Ready).loading)
        vm.type("northe")
        advanceTimeBy(300)
        val kept = vm.results() as SearchResultsState.Ready
        assertEquals("north", kept.query)
        assertTrue(kept.loading)
        search.answer("northe").complete(SearchResults())
        advanceUntilIdle()
        assertEquals(SearchResultsState.Empty("northe"), vm.results())
    }

    @Test
    fun resultsComeInTheWebsGroupsAndOrderWithCounts() = runTest(dispatcher) {
        val vm = viewModel("lights")
        search.answer("lights").complete(
            SearchResults(
                items = listOf(
                    item(1, ItemKind.Track, "Lights", parentId = 40),
                    item(2, ItemKind.Episode),
                    item(3, ItemKind.Movie),
                    item(4, ItemKind.Album),
                    item(5, ItemKind.Show),
                    item(6, ItemKind.Movie),
                    item(7, ItemKind.Extra),
                    item(8, ItemKind.Season),
                    item(9, ItemKind.Artist),
                ),
                people = listOf(PersonSummary(70, "Lena Okafor")),
            ),
        )
        advanceUntilIdle()
        val groups = (vm.results() as SearchResultsState.Ready).groups
        assertEquals(
            listOf(
                SearchGroupKind.Movies, SearchGroupKind.Shows, SearchGroupKind.Seasons, SearchGroupKind.Episodes,
                SearchGroupKind.Artists, SearchGroupKind.Albums, SearchGroupKind.Tracks, SearchGroupKind.People,
            ),
            groups.map { it.kind },
        )
        val movies = groups.first { it.kind == SearchGroupKind.Movies }
        assertEquals(2, movies.count)
        // The server's order of relevance holds inside a group.
        assertEquals(listOf(3L, 6L), movies.ids)
        assertEquals(TileLook.Landscape, groups.first { it.kind == SearchGroupKind.Episodes }.tiles.single().look)
        assertEquals(TileLook.Square, groups.first { it.kind == SearchGroupKind.Albums }.tiles.single().look)
        assertEquals("person http://pley", groups.last().tiles.single().subtitle)
        // Opening: a person's page, a track's album, an album's page, the rest their item page.
        assertEquals(PleyKey.Person(70), vm.open(SearchGroupKind.People, 70))
        assertEquals(PleyKey.Album(40), vm.open(SearchGroupKind.Tracks, 1))
        assertEquals(PleyKey.Album(4), vm.open(SearchGroupKind.Albums, 4))
        assertEquals(PleyKey.Artist(9), vm.open(SearchGroupKind.Artists, 9))
        assertEquals(PleyKey.Item(3), vm.open(SearchGroupKind.Movies, 3))
    }

    @Test
    fun eachGroupHoldsEveryResultTheServerGives() = runTest(dispatcher) {
        val vm = viewModel("the")
        search.answer("the").complete(
            SearchResults(
                items = (1L..150L).map { item(it, ItemKind.Movie) } + (151L..230L).map { item(it, ItemKind.Episode) },
                people = (1L..60L).map { PersonSummary(1_000 + it, "Person $it") },
            ),
        )
        advanceUntilIdle()
        val groups = (vm.results() as SearchResultsState.Ready).groups
        // Nothing cut at 12, nor anywhere else: one row each, as long as the server's answer.
        assertEquals(listOf(150, 80, 60), groups.map { it.count })
        assertEquals((1L..150L).toList(), groups.first().ids)
    }

    @Test
    fun aQueryIsRememberedWhenSubmittedOrOneOfItsResultsIsOpened() = runTest(dispatcher) {
        val vm = viewModel()
        advanceUntilIdle()
        assertEquals(listOf("halcyon"), vm.state.value.recent)
        assertEquals(SearchResultsState.Idle, vm.results())
        vm.type(" north ")
        vm.submit()
        advanceUntilIdle()
        assertEquals(listOf("north", "halcyon"), vm.state.value.recent)
        vm.type("orbit")
        search.answer("orbit").complete(SearchResults(items = listOf(item(1, ItemKind.Movie))))
        advanceUntilIdle()
        vm.open(SearchGroupKind.Movies, 1)
        advanceUntilIdle()
        assertEquals(listOf("orbit", "north", "halcyon"), vm.state.value.recent)
        // A recent search searches again.
        vm.searchAgain("halcyon")
        advanceUntilIdle()
        assertEquals("halcyon", vm.state.value.query)
        assertEquals("halcyon", search.asked.last())
        vm.clearRecent()
        advanceUntilIdle()
        assertEquals(emptyList<String>(), vm.state.value.recent)
    }

    @Test
    fun clearingTheFieldGoesBackToRecentSearchesAtOnce() = runTest(dispatcher) {
        val vm = viewModel("north")
        search.answer("north").complete(SearchResults(items = listOf(item(1, ItemKind.Movie))))
        advanceUntilIdle()
        vm.type("  ")
        runCurrent()
        assertEquals(SearchResultsState.Idle, vm.results())
    }

    @Test
    fun aFailureSaysWhy() = runTest(dispatcher) {
        val vm = viewModel("north")
        search.answer("north").completeExceptionally(IOException("Can't reach the server"))
        advanceUntilIdle()
        val failed = vm.results() as SearchResultsState.Failed
        assertEquals("north", failed.query)
        assertEquals("Can't reach the server", failed.error.message)
    }

    @Test
    fun retryAsksForTheSameQueryAgain() = runTest(dispatcher) {
        val vm = viewModel("north")
        search.answer("north").completeExceptionally(IOException("Can't reach the server"))
        advanceUntilIdle()
        assertTrue(vm.results() is SearchResultsState.Failed)
        search.answers.remove("north")
        vm.retry()
        search.answer("north").complete(SearchResults(items = listOf(item(1, ItemKind.Movie, "Northern Lights"))))
        advanceUntilIdle()
        assertEquals(listOf("north", "north"), search.asked)
        assertTrue(vm.results() is SearchResultsState.Ready)
    }

    @Test
    fun channelsFoundComeWithTheResultsAndPlayAmongEveryChannel() = runTest(dispatcher) {
        val vm = viewModel("sport")
        val channel = LiveChannel(id = 7, name = "Sport 1", number = "101", logo = "/logo.png")
        search.answer("sport").complete(SearchResults(channels = listOf(channel)))
        advanceUntilIdle()
        // Channels alone are results: not "No results".
        val ready = vm.results() as SearchResultsState.Ready
        assertEquals(emptyList<SearchGroup<FakeTile>>(), ready.groups)
        assertEquals(listOf(channel), ready.channels)
        assertEquals("http://pley", ready.origin)
        vm.watch(channel)
        advanceUntilIdle()
        assertEquals(listOf(7L to "all"), watched)
        // The player shows it at once, as search had it.
        assertEquals(channel, ChannelHandoff.take(7))
        assertEquals("sport", vm.state.value.recent.first())
    }

    // ------------------------------------------------------------ Discover row

    private val summary: RequestSummary = Fixtures.decode("requests/summary.json")
    private val trending: DiscoverPage = Fixtures.decode("discover/trending.json")

    @Test
    fun onTmdbAsksForWhatNoLibraryHasBesideTheResultsWithoutHoldingThemUp() = runTest(dispatcher) {
        val requests = FakeRequests(summary)
        val tmdb = CompletableDeferred<DiscoverPage>()
        val asked = mutableListOf<Triple<String, Int, Boolean>>()
        requests.searches = { q, page, missing ->
            asked += Triple(q, page, missing)
            tmdb.await()
        }
        val vm = viewModel(requests = requests)
        runCurrent()
        assertEquals(TmdbSection.Hidden, vm.tmdb.value)
        vm.type("robot")
        advanceTimeBy(251)
        assertEquals(listOf(Triple("robot", 1, true)), asked)
        assertEquals(TmdbSection.Loading("robot"), vm.tmdb.value)
        // The libraries' results show while TMDB is still asked.
        search.answer("robot").complete(SearchResults(items = listOf(item(1, ItemKind.Movie, "Robots"))))
        runCurrent()
        assertTrue(vm.results() is SearchResultsState.Ready)
        tmdb.complete(trending)
        runCurrent()
        val ready = vm.tmdb.value as TmdbSection.Ready
        assertEquals("robot", ready.query)
        assertEquals(trending.results.map { it.tmdbId }, ready.titles.map { it.tmdbId })
        assertEquals("http://pley", ready.origin)
    }

    @Test
    fun onTmdbReadsOnPastAPageWhoseTitlesAreAllInALibrary() = runTest(dispatcher) {
        val requests = FakeRequests(summary)
        val asked = mutableListOf<Int>()
        requests.searches = { _, page, _ ->
            asked += page
            // missing=1 left out every title of the first page: it isn't the end.
            if (page == 1) DiscoverPage(1, 2, emptyList()) else trending.copy(page = 2, totalPages = 2)
        }
        val vm = viewModel("robot", requests)
        advanceUntilIdle()
        assertEquals(listOf(1, 2), asked)
        assertEquals(trending.results.map { it.tmdbId }, (vm.tmdb.value as TmdbSection.Ready).titles.map { it.tmdbId })
    }

    @Test
    fun onTmdbShowsAsManyTitlesAsItsRowHolds() = runTest(dispatcher) {
        val requests = FakeRequests(summary)
        val asked = mutableListOf<Int>()
        requests.searches = { _, page, _ ->
            asked += page
            DiscoverPage(page, 10, (1..8).map { DiscoverTitle(TitleKind.Movie, page * 100L + it, "Robot $page.$it") })
        }
        // A phone's twelve; a TV's twenty, from three pages at most.
        val phone = viewModel("robot", requests)
        advanceUntilIdle()
        assertEquals(12, (phone.tmdb.value as TmdbSection.Ready).titles.size)
        assertEquals(listOf(1, 2), asked)
        asked.clear()
        val tv = viewModel("robot", requests, tmdbShown = 20)
        advanceUntilIdle()
        assertEquals(20, (tv.tmdb.value as TmdbSection.Ready).titles.size)
        assertEquals(listOf(1, 2, 3), asked)
    }

    @Test
    fun onTmdbBadgesFollowRequests() = runTest(dispatcher) {
        val requests = FakeRequests(summary)
        val vm = viewModel("sub", requests)
        advanceUntilIdle()
        val substance = (vm.tmdb.value as TmdbSection.Ready).titles.first { it.tmdbId == 933260L }
        assertEquals(TitleStatus.Requested, substance.status)
        requests.api.answer(
            "request",
            MediaRequest(id = 44, kind = TitleKind.Movie, tmdbId = 933260, title = "The Substance", status = RequestStatus.Declined),
        )
        requests.change(RequestChange(44, TitleKind.Movie, 933260))
        advanceUntilIdle()
        assertEquals(TitleStatus.Declined, (vm.tmdb.value as TmdbSection.Ready).titles.first { it.tmdbId == 933260L }.status)
    }

    @Test
    fun onTmdbIsHiddenWhenAskingDoesNotWorkOrTmdbHasNothing() = runTest(dispatcher) {
        // May not ask: TMDB isn't asked at all.
        val off = FakeRequests(summary.copy(access = RequestAccess.Off))
        val vm = viewModel("robot", off)
        advanceUntilIdle()
        assertEquals(TmdbSection.Hidden, vm.tmdb.value)
        assertTrue(off.api.calls("discoverSearch").isEmpty())

        // Nothing there: the libraries' results alone.
        val empty = FakeRequests(summary)
        empty.searches = { _, page, _ -> DiscoverPage(page, 1, emptyList()) }
        val nothing = viewModel("robot", empty)
        advanceUntilIdle()
        assertEquals(TmdbSection.Hidden, nothing.tmdb.value)
    }

    @Test
    fun onTmdbSaysWhyTmdbCantBeSearchedAndRetryAsksAgain() = runTest(dispatcher) {
        val requests = FakeRequests(summary)
        var down = true
        requests.searches = { _, page, _ -> if (down) throw ApiException(503, "tmdb_down", "TMDB isn't answering") else trending.copy(page = page) }
        val vm = viewModel("robot", requests)
        advanceUntilIdle()
        assertEquals(TmdbSection.Failed("robot", RequestProblem.TmdbDown, summary), vm.tmdb.value)
        down = false
        vm.retryTmdb()
        advanceUntilIdle()
        assertEquals(trending.results.map { it.tmdbId }, (vm.tmdb.value as TmdbSection.Ready).titles.map { it.tmdbId })

        // A key TMDB refused: said too, an administrator's to fix.
        val refused = FakeRequests(summary)
        refused.searches = { _, _, _ -> throw ApiException(503, "tmdb_key", "TMDB refused the key") }
        val other = viewModel("robot", refused)
        advanceUntilIdle()
        assertEquals(RequestProblem.TmdbKey, (other.tmdb.value as TmdbSection.Failed).problem)
    }

    @Test
    fun onTmdbAsksForAtMostTheServers200Characters() = runTest(dispatcher) {
        val requests = FakeRequests(summary)
        val asked = mutableListOf<String>()
        requests.searches = { q, page, _ ->
            asked += q
            trending.copy(page = page)
        }
        val vm = viewModel("x".repeat(250), requests)
        advanceUntilIdle()
        assertEquals(listOf("x".repeat(200)), asked.distinct())
        assertEquals("x".repeat(200), (vm.tmdb.value as TmdbSection.Ready).query)
    }
}
