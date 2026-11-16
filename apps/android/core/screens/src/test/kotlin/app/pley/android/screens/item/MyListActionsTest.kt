package app.pley.android.screens.item

import app.pley.android.model.ItemDetail
import app.pley.android.model.ItemKind
import app.pley.android.network.ApiException
import app.pley.android.testing.caps.FakeCapsGraph
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

/** My List on the item page and in the actions, and leaving Continue Watching. */
@OptIn(ExperimentalCoroutinesApi::class)
class MyListActionsTest {
    private val dispatcher = UnconfinedTestDispatcher()
    private val data = FakeData()
    private val launcher = RecordingLauncher()

    @Before
    fun setUp() = Dispatchers.setMain(dispatcher)

    @After
    fun tearDown() = Dispatchers.resetMain()

    private fun TestScope.page(id: Long): ItemViewModel {
        val model = ItemViewModel(data, launcher, FakeCapsGraph(), id, compute = dispatcher)
        backgroundScope.launch(dispatcher) { model.state.collect { } }
        return model
    }

    private fun TestScope.actions(id: Long, continueWatching: Boolean = false): ItemActionsViewModel {
        val model = ItemActionsViewModel(data, launcher, id, continueWatching, compute = dispatcher, work = CoroutineScope(dispatcher))
        backgroundScope.launch(dispatcher) { model.state.collect { } }
        return model
    }

    private val ItemViewModel.page: ItemPage get() = (state.value as ItemUiState.Ready).page

    private val ItemActionsViewModel.actions: List<ItemAction> get() = (state.value as ItemActionsUiState.Ready).actions

    private fun episode() = ItemDetail(
        id = 41, libraryId = 2, kind = ItemKind.Episode, title = "Half Loop", parentId = 31, grandparentId = 10,
        userState = app.pley.android.model.UserState(positionMs = 5_000), durationMs = 3_480_000,
    )

    @Test
    fun `a movie's page says whether it is in My List, and the button changes it`() = runTest(dispatcher) {
        val model = page(1)
        data.set(movie())
        assertEquals(false, model.page.myList)

        model.toggleMyList()
        assertEquals(listOf(1L to true), data.listed)
        assertEquals(true, model.page.myList)

        model.toggleMyList()
        assertEquals(1L to false, data.listed.last())
        assertEquals(false, model.page.myList)
    }

    @Test
    fun `episodes and seasons have no My List`() = runTest(dispatcher) {
        val model = page(41)
        data.set(episode())
        assertNull(model.page.myList)
        model.toggleMyList()
        assertTrue(data.listed.isEmpty())
    }

    @Test
    fun `a refused change is told and the page shows what it was`() = runTest(dispatcher) {
        val model = page(1)
        val events = mutableListOf<ItemEvent>()
        backgroundScope.launch(dispatcher) { model.events.collect { events += it } }
        data.set(movie())
        data.listFailure = ApiException(500, "internal", "the server failed")

        model.toggleMyList()

        assertEquals(false, model.page.myList)
        assertTrue(events.single() is ItemEvent.Failed)
    }

    @Test
    fun `the actions offer My List for movies and shows, by what the page knows`() {
        val movie = summary(1, ItemKind.Movie)
        assertEquals(
            listOf(ItemAction.Play, ItemAction.MarkWatched, ItemAction.AddToMyList, ItemAction.AddToPlaylist),
            itemActions(movie, inWatchlist = false),
        )
        assertTrue(ItemAction.RemoveFromMyList in itemActions(summary(10, ItemKind.Show), inWatchlist = true))
        // Unknown (a tile's summary), or not a movie or show: not offered.
        assertFalse(itemActions(movie).any { it == ItemAction.AddToMyList || it == ItemAction.RemoveFromMyList })
        assertFalse(itemActions(summary(41, ItemKind.Episode), inWatchlist = false).contains(ItemAction.AddToMyList))
    }

    @Test
    fun `opened from Continue Watching, the actions take the item out of it`() = runTest(dispatcher) {
        data.set(episode())
        val fromRow = actions(41, continueWatching = true)
        val elsewhere = actions(41)
        assertEquals(
            listOf(ItemAction.Resume, ItemAction.FromStart, ItemAction.MarkWatched, ItemAction.RemoveFromContinueWatching),
            fromRow.actions.take(4),
        )
        assertFalse(ItemAction.RemoveFromContinueWatching in elsewhere.actions)

        assertTrue(fromRow.perform(ItemAction.RemoveFromContinueWatching))
        assertEquals(listOf(41L), data.hidden)
    }

    @Test
    fun `the actions add to My List and take off it`() = runTest(dispatcher) {
        data.set(movie())
        val model = actions(1)
        assertTrue(ItemAction.AddToMyList in model.actions)

        assertTrue(model.perform(ItemAction.AddToMyList))
        assertEquals(listOf(1L to true), data.listed)
        assertTrue(ItemAction.RemoveFromMyList in model.actions)

        assertTrue(model.perform(ItemAction.RemoveFromMyList))
        assertEquals(1L to false, data.listed.last())
    }
}
