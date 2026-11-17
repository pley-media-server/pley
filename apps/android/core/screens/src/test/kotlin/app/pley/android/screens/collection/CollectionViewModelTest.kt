package app.pley.android.screens.collection

import app.pley.android.common.UiText
import app.pley.android.data.Loadable
import app.pley.android.data.PlayItem
import app.pley.android.data.PlayTarget
import app.pley.android.data.valueOrNull
import app.pley.android.model.CategoryRef
import app.pley.android.model.CollectionDetail
import app.pley.android.model.CollectionSort
import app.pley.android.model.CollectionSummary
import app.pley.android.model.ItemKind
import app.pley.android.model.Library
import app.pley.android.model.LibraryKind
import app.pley.android.model.MergedCollection
import app.pley.android.model.MergedPart
import app.pley.android.player.PlayQueue
import app.pley.android.player.PlayRequest
import app.pley.android.screens.PlayerLauncher
import app.pley.android.screens.R
import app.pley.android.screens.library.FakeCollections
import app.pley.android.screens.library.FakeHubs
import app.pley.android.screens.library.FakeLibraries
import app.pley.android.screens.library.FakeLibraryItems
import app.pley.android.screens.library.FakeSettings
import app.pley.android.screens.library.FakeTiles
import app.pley.android.screens.library.LibraryDeps
import app.pley.android.screens.library.movie
import kotlin.random.Random
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class CollectionViewModelTest {
    private val dispatcher = StandardTestDispatcher()
    private val libraries = FakeLibraries(Library(1, "Movies", LibraryKind.Movies), Library(2, "Shows", LibraryKind.Shows), Library(3, "Music", LibraryKind.Music))
    private val collections = FakeCollections()
    private val played = mutableListOf<PlayRequest>()
    private val player = object : PlayerLauncher {
        override fun play(request: PlayRequest) {
            played += request
        }
    }
    private val resolved = mutableListOf<Long>()
    private val playItem = PlayItem { item, _ ->
        resolved += item.id
        if (item.id == 99L) PlayTarget.NothingToPlay else PlayTarget.Video(item.id * 100, 0)
    }

    @Before
    fun main() = Dispatchers.setMain(dispatcher)

    @After
    fun reset() = Dispatchers.resetMain()

    private fun viewModel(id: Long, random: Random = Random(1)) = CollectionViewModel(
        collectionId = id,
        deps = CollectionDeps(collections, libraries, playItem, player, MutableStateFlow("http://pley"), dispatcher, random),
        tiles = FakeTiles,
    )

    @Test
    fun aCollectionSaysWhatItHoldsInWords() = runTest(dispatcher) {
        collections.details.value = mapOf(
            5L to CollectionDetail(
                5, 1, "Alien Collection", automatic = true, sort = CollectionSort.Release,
                category = CategoryRef(2, "franchises", "Franchises"), itemCount = 2, items = listOf(movie(1), movie(2)),
            ),
            6L to CollectionDetail(
                6, 1, "Nineties comedies", smartFilter = "genre=Comedy&decade=1990&sort=year",
                category = CategoryRef(3, "other", "Other"), itemCount = 0,
            ),
        )
        val tmdb = viewModel(5)
        val smart = viewModel(6)
        val job = launch { tmdb.state.collect {} }
        val job2 = launch { smart.state.collect {} }
        advanceUntilIdle()
        val page = tmdb.state.value.valueOrNull!!
        assertTrue(page.automatic)
        assertFalse(page.smart)
        assertEquals("Movies", page.libraryName)
        assertEquals("Franchises", page.category)
        assertEquals(listOf(UiText.res(R.string.collection_order_release)), page.noteTexts)
        assertTrue(page.movies)
        assertTrue(page.canShuffle)
        val filtered = smart.state.value.valueOrNull!!
        assertTrue(filtered.smart)
        // Other only says it has none: the page names no category.
        assertEquals(null, filtered.category)
        assertEquals(
            listOf(UiText.res(R.string.library_filter_genre, "Comedy"), UiText.res(R.string.library_filter_decade, "1990"), UiText.res(R.string.library_filter_year_asc)),
            filtered.noteTexts,
        )
        assertFalse(filtered.canPlay)
        job.cancel()
        job2.cancel()
    }

    @Test
    fun aMergedPageShowsItsLibrariesAndPlaysThroughTheRow() = runTest(dispatcher) {
        val ids = listOf(5L, 8L)
        collections.mergedPages.value = mapOf(
            ids to MergedCollection(
                "Award Winners",
                listOf(MergedPart(5, 1), MergedPart(8, 2), MergedPart(9, 42)),
                listOf(movie(1), movie(10, kind = ItemKind.Show, libraryId = 2), movie(2)),
            ),
        )
        val vm = MergedCollectionViewModel(ids, CollectionDeps(collections, libraries, playItem, player, MutableStateFlow("http://pley"), dispatcher, Random(1)), FakeTiles)
        val job = launch { vm.state.collect {} }
        advanceUntilIdle()
        val page = vm.state.value.valueOrNull!!
        assertEquals("Award Winners", page.title)
        assertEquals(2, page.movies)
        assertEquals(1, page.shows)
        // A library this profile doesn't know is left out.
        assertEquals(listOf(MergedSource(5, "Movies"), MergedSource(8, "Shows")), page.sources)
        vm.play(shuffle = false)
        assertEquals(PlayRequest(itemId = 1, queue = PlayQueue.MergedCollection(ids)), played.last())
        job.cancel()
    }

    @Test
    fun moviesPlayInOrderAsAQueue() = runTest(dispatcher) {
        collections.details.value = mapOf(5L to CollectionDetail(5, 1, "Trilogy", items = listOf(movie(1), movie(2), movie(3))))
        val vm = viewModel(5, Random(7))
        val job = launch { vm.state.collect {} }
        advanceUntilIdle()
        vm.play(shuffle = false)
        assertEquals(PlayRequest(itemId = 1, queue = PlayQueue.Collection(5)), played.last())
        vm.play(shuffle = true)
        val start = Random(7).nextInt(3) + 1L
        assertEquals(PlayRequest(itemId = start, queue = PlayQueue.Collection(5)), played.last())
        job.cancel()
    }

    @Test
    fun showsPlayTheirNextEpisode() = runTest(dispatcher) {
        val shows = listOf(movie(10, kind = ItemKind.Show, libraryId = 2), movie(99, kind = ItemKind.Show, libraryId = 2))
        collections.details.value = mapOf(7L to CollectionDetail(7, 2, "Shows", items = shows))
        val vm = viewModel(7)
        val job = launch { vm.state.collect {} }
        val events = mutableListOf<CollectionEvent>()
        val listen = launch { vm.events.collect { events += it } }
        advanceUntilIdle()
        assertFalse(vm.state.value.valueOrNull!!.movies)
        vm.play(shuffle = false)
        advanceUntilIdle()
        assertEquals(listOf(10L), resolved)
        assertEquals(PlayRequest(itemId = 1_000, startMs = 0), played.last())
        // A show with nothing to play says so.
        collections.details.value = mapOf(7L to CollectionDetail(7, 2, "Shows", items = shows.reversed()))
        advanceUntilIdle()
        vm.play(shuffle = false)
        advanceUntilIdle()
        assertEquals(listOf<CollectionEvent>(CollectionEvent.NothingToPlay), events)
        job.cancel()
        listen.cancel()
    }

    @Test
    fun theCollectionsPageHasAShelfPerVideoLibrary() = runTest(dispatcher) {
        collections.lists.value = mapOf(
            1L to listOf(CollectionSummary(5, 1, "Alien Collection", itemCount = 4)),
            2L to listOf(CollectionSummary(7, 2, "Sitcoms", itemCount = 3)),
        )
        val deps = LibraryDeps(libraries, FakeLibraryItems(), FakeHubs(), collections, FakeSettings(), MutableStateFlow("http://pley"), dispatcher)
        val all = CollectionsViewModel(only = null, deps = deps, tiles = FakeTiles)
        val one = CollectionsViewModel(only = 2, deps = deps, tiles = FakeTiles)
        val job = launch { all.state.collect {} }
        val job2 = launch { one.state.collect {} }
        advanceUntilIdle()
        val shelves = all.state.value.valueOrNull!!
        assertEquals(listOf("Movies", "Shows"), shelves.map { it.libraryName })
        assertEquals("3 shows", shelves[1].collections.valueOrNull!!.single().tile.subtitle)
        assertEquals(listOf(2L), one.state.value.valueOrNull!!.map { it.libraryId })
        assertTrue(one.state.value is Loadable.Ready)
        job.cancel()
        job2.cancel()
    }
}
