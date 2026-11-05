package app.pley.android.screens.playlists

import app.pley.android.common.UiText
import app.pley.android.data.Loadable
import app.pley.android.data.valueOrNull
import app.pley.android.model.ItemDetail
import app.pley.android.model.ItemKind
import app.pley.android.model.PlaylistKind
import app.pley.android.model.PlaylistSummary
import app.pley.android.model.format.FormatWords
import app.pley.android.screens.R
import app.pley.android.screens.item.FakeData
import java.io.IOException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
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
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

/** The sheets: Add to playlist, New playlist and Save these filters. */
@OptIn(ExperimentalCoroutinesApi::class)
class PlaylistSheetsTest {
    private val dispatcher = StandardTestDispatcher()
    private val repo = FakePlaylists()
    private val data = FakeData()

    @Before
    fun main() = Dispatchers.setMain(dispatcher)

    /**
     * Where the view models' changes and the test's collectors run: work
     * the test's clock drives (backgroundScope's only runs alongside other
     * work), cancelled after each test.
     */
    private val work = CoroutineScope(SupervisorJob() + dispatcher)

    @After
    fun reset() {
        work.cancel()
        Dispatchers.resetMain()
    }

    private val lists = listOf(
        PlaylistSummary(1, "Road trip", PlaylistKind.Video, itemCount = 12, durationMs = 11_520_000, artwork = listOf("/a", "/b")),
        PlaylistSummary(2, "Comedies", PlaylistKind.Video, smartFilter = "genre=Comedy", libraryId = 1),
        PlaylistSummary(3, "Mix", PlaylistKind.Audio, itemCount = 1),
    )

    private fun TestScope.deps() = PlaylistDeps(repo, MutableStateFlow("http://pley"), dispatcher, work = work)

    private fun TestScope.sheet(vararg ids: Long): Pair<AddToPlaylistViewModel, MutableList<AddEvent>> {
        val vm = AddToPlaylistViewModel(ids.toList(), deps(), data.items)
        val events = mutableListOf<AddEvent>()
        work.launch { vm.state.collect {} }
        work.launch { vm.events.collect { events += it } }
        return vm to events
    }

    private fun show(id: Long, title: String, episodes: Int?) =
        ItemDetail(id = id, libraryId = 2, kind = ItemKind.Show, title = title, year = 2022, leafCount = episodes)

    private fun episode(id: Long) = ItemDetail(
        id = id, libraryId = 2, kind = ItemKind.Episode, title = "Half Loop", index = 2, parentIndex = 1, grandparentTitle = "Severance",
    )

    @Test
    fun itSaysWhatIsAddedAndOffersOnlyPlaylistsThatTakeIt() = runTest(dispatcher) {
        repo.list.value = Loadable.Ready(lists, fresh = true)
        data.set(episode(7))
        val (vm, _) = sheet(7)
        advanceUntilIdle()
        val state = vm.state.value
        assertEquals(UiText.raw("Severance — S1 · E2 — Half Loop"), state.title)
        assertEquals(PlaylistKind.Video, state.kind)
        // Smart playlists fill themselves; music ones don't take episodes.
        val cards = state.playlists.valueOrNull!!
        assertEquals(listOf("Road trip"), cards.map { it.title })
        assertEquals(listOf(UiText.plural(R.plurals.playlists_count_items, 12), UiText.raw("3h 12m")), cards.single().metaTexts)
    }

    @Test
    fun whileTheItemLoadsNothingIsKnown() = runTest(dispatcher) {
        repo.list.value = Loadable.Ready(lists, fresh = true)
        val (vm, _) = sheet(7)
        advanceUntilIdle()
        assertNull(vm.state.value.title)
        assertNull(vm.state.value.kind)
    }

    @Test
    fun addingSaysHowManyEntriesWentIn() = runTest(dispatcher) {
        repo.list.value = Loadable.Ready(lists, fresh = true)
        data.set(show(8, "Severance", episodes = 19))
        val (vm, events) = sheet(8)
        advanceUntilIdle()
        vm.add(vm.state.value.playlists.valueOrNull!!.single())
        advanceUntilIdle()
        assertEquals(listOf("add 1 [8]"), repo.sent)
        assertEquals(AddEvent.Added("Road trip", 19), events.single())
        assertFalse(vm.state.value.busy)
    }

    @Test
    fun theToastsCountsAreTheWebs() {
        val one = ItemDetail(id = 1, libraryId = 1, kind = ItemKind.Movie, title = "Heat").toSummary()
        val empty = show(2, "Nothing yet", episodes = 0).toSummary()
        val unknown = show(3, "Unknown", episodes = null).toSummary()
        assertEquals(1, Playlists.entriesOf(listOf(one)))
        assertEquals(0, Playlists.entriesOf(listOf(empty)))
        assertEquals(null, Playlists.entriesOf(listOf(one, unknown)))
        assertEquals(UiText.plural(R.plurals.playlists_count_items, 3), AddToPlaylistViewModel.titleOf(listOf(one, empty, unknown), FormatWords.English))
        assertEquals(UiText.raw("Heat"), AddToPlaylistViewModel.titleOf(listOf(one), FormatWords.English))
    }

    @Test
    fun aNewPlaylistIsMadeOnTheSpotWithTheItems() = runTest(dispatcher) {
        repo.list.value = Loadable.Ready(lists, fresh = true)
        data.set(episode(7))
        val (vm, events) = sheet(7)
        advanceUntilIdle()
        assertFalse(vm.create("   "))
        assertTrue(vm.create(" Sunday "))
        advanceUntilIdle()
        assertEquals(listOf("create Sunday Video [7]"), repo.sent)
        assertEquals(AddEvent.Added("Sunday", 1), events.single())
    }

    @Test
    fun aRefusalIsToldAndTheSheetStays() = runTest(dispatcher) {
        repo.list.value = Loadable.Ready(lists, fresh = true)
        data.set(episode(7))
        val (vm, events) = sheet(7)
        advanceUntilIdle()
        repo.fail = IOException("refused")
        vm.add(vm.state.value.playlists.valueOrNull!!.single())
        advanceUntilIdle()
        assertEquals("refused", (events.single() as AddEvent.Failed).error.message)
        assertFalse(vm.state.value.busy)
    }

    @Test
    fun newPlaylistAsksWhatItPlaysOnlyWhenEmpty() = runTest(dispatcher) {
        val empty = NewPlaylistViewModel(NewPlaylistMode.Items(emptyList()), deps())
        val withItems = NewPlaylistViewModel(NewPlaylistMode.Items(listOf(5)), deps())
        val events = mutableListOf<CreateEvent>()
        work.launch { empty.events.collect { events += it } }
        assertTrue(empty.asksKind)
        assertFalse(withItems.asksKind)
        assertFalse(empty.create(""))
        assertFalse(empty.create("x".repeat(201)))
        assertTrue(empty.create("Chill", PlaylistKind.Audio))
        advanceUntilIdle()
        assertTrue(withItems.create("Picks"))
        advanceUntilIdle()
        assertEquals(listOf("create Chill Audio []", "create Picks null [5]"), repo.sent)
        assertEquals(CreateEvent.Created(100, "Chill"), events.single())
    }

    @Test
    fun savingFiltersMakesASmartPlaylist() = runTest(dispatcher) {
        val vm = NewPlaylistViewModel(NewPlaylistMode.Smart(1, "genre=Comedy&unwatched=1"), deps())
        assertEquals(listOf(UiText.res(R.string.library_filter_genre, "Comedy"), UiText.res(R.string.library_filter_unwatched)), vm.filterTexts)
        assertFalse(vm.asksKind)
        assertTrue(vm.create("Comedies"))
        advanceUntilIdle()
        assertEquals(listOf("smart Comedies 1 genre=Comedy&unwatched=1"), repo.sent)
    }

    @Test
    fun thePlaylistsPageMakesOneAndOpensIt() = runTest(dispatcher) {
        repo.list.value = Loadable.Ready(lists, fresh = true)
        val vm = PlaylistsViewModel(deps())
        val events = mutableListOf<CreateEvent>()
        work.launch { vm.state.collect {} }
        work.launch { vm.events.collect { events += it } }
        advanceUntilIdle()
        val cards = vm.state.value.valueOrNull!!
        assertEquals(listOf(false, true, false), cards.map { it.smart })
        assertFalse(cards[2].video)
        assertTrue(vm.create("Weekend", PlaylistKind.Video))
        advanceUntilIdle()
        assertEquals(CreateEvent.Created(100, "Weekend"), events.single())
    }
}
