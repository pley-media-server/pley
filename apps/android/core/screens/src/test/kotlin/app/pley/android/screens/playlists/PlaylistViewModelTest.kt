package app.pley.android.screens.playlists

import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.ViewModelStore
import androidx.lifecycle.viewmodel.CreationExtras
import app.pley.android.common.UiText
import app.pley.android.data.Loadable
import app.pley.android.data.valueOrNull
import app.pley.android.model.PlaylistKind
import app.pley.android.player.PlayQueue
import app.pley.android.player.PlayRequest
import app.pley.android.screens.PlayerLauncher
import app.pley.android.screens.PleyKey
import app.pley.android.screens.R
import app.pley.android.screens.library.FakeTile
import app.pley.android.screens.library.FakeTiles
import app.pley.android.screens.library.TileLook
import java.io.IOException
import kotlin.random.Random
import kotlin.reflect.KClass
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.MutableStateFlow
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
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class PlaylistViewModelTest {
    private val dispatcher = StandardTestDispatcher()
    private val repo = FakePlaylists()
    private val played = mutableListOf<PlayRequest>()
    private val player = object : PlayerLauncher {
        override fun play(request: PlayRequest) {
            played += request
        }
    }

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

    private val roadTrip = playlist(
        1,
        "Road trip",
        film(10, "Northern Lights"),
        episode(20, "Half Loop", "Severance", 1, 2, durationMs = 3_000_000, positionMs = 600_000),
        film(30, "Low Orbit", durationMs = 3_000_000),
    )

    private fun TestScope.viewModel(random: Random = Random(1)): PlaylistViewModel<FakeTile> {
        val deps = PlaylistDeps(repo, MutableStateFlow("http://pley"), dispatcher, work = work)
        val vm = PlaylistViewModel(1, deps, player, FakeTiles, random)
        work.launch { vm.state.collect {} }
        work.launch { vm.events.collect { events += it } }
        return vm
    }

    private val events = mutableListOf<PlaylistEvent>()

    private fun PlaylistViewModel<FakeTile>.page() = state.value.valueOrNull!!

    private fun PlaylistViewModel<FakeTile>.order() = page().rows.map { it.entryId }

    @Test
    fun entriesAreNumberedAndWordedAsOnTheWeb() = runTest(dispatcher) {
        repo.set(roadTrip)
        val vm = viewModel()
        advanceUntilIdle()
        val page = vm.page()
        assertEquals(listOf(UiText.plural(R.plurals.playlists_count_items, 3), UiText.raw("3h 20m")), page.metaTexts)
        assertTrue(page.video)
        assertTrue(page.editable)
        assertEquals(listOf(1, 2, 3), page.rows.map { it.number })
        val episode = page.rows[1]
        assertEquals("Half Loop", episode.title)
        assertEquals("Severance · S1 · E2", episode.subtitle)
        assertEquals("50m", episode.duration)
        // Video entries are 16:9 stills.
        assertEquals(TileLook.Landscape, episode.tile.look)
    }

    @Test
    fun aSmartPlaylistIsReadOnlyAndSaysItsFilter() = runTest(dispatcher) {
        repo.set(playlist(1, "Comedies", film(10, "A"), film(11, "B"), smart = "genre=Comedy&unwatched=1"))
        val vm = viewModel()
        advanceUntilIdle()
        val page = vm.page()
        assertTrue(page.smart)
        assertFalse(page.editable)
        assertEquals(listOf(UiText.res(R.string.library_filter_genre, "Comedy"), UiText.res(R.string.library_filter_unwatched)), page.noteTexts)
        vm.move(0, 1)
        vm.remove(1, undoable = false)
        advanceUntilIdle()
        assertEquals(listOf(1L, 2L), vm.order())
        assertEquals(emptyList<String>(), repo.sent)
    }

    @Test
    fun theOrderMathsIsTheWebs() {
        val ids = listOf("a", "b", "c", "d")
        assertEquals(listOf("b", "c", "a", "d"), Playlists.moved(ids, 0, 2))
        assertEquals(listOf("d", "a", "b", "c"), Playlists.moved(ids, 3, 0))
        assertEquals(listOf("a", "c", "b", "d"), Playlists.moved(ids, 2, 1))
        assertEquals(ids, Playlists.moved(ids, 4, 0))
        assertEquals(ids, Playlists.moved(ids, -1, 2))
        // Past the end goes last.
        assertEquals(listOf("b", "c", "d", "a"), Playlists.moved(ids, 0, 9))
    }

    @Test
    fun aMoveShowsAtOnceAndGoesToTheServer() = runTest(dispatcher) {
        repo.set(roadTrip)
        val vm = viewModel()
        advanceUntilIdle()
        val gate = CompletableDeferred<Unit>().also { repo.gate = it }
        vm.move(2, 0)
        runCurrent()
        // Shown before the server answers.
        assertEquals(listOf(3L, 1L, 2L), vm.order())
        assertEquals(listOf(1, 2, 3), vm.page().rows.map { it.number })
        gate.complete(Unit)
        advanceUntilIdle()
        assertEquals(listOf("reorder 1 [3, 1, 2]"), repo.sent)
        assertEquals(listOf(3L, 1L, 2L), vm.order())
    }

    @Test
    fun aRefusedMovePutsTheOrderBackAndSaysWhy() = runTest(dispatcher) {
        repo.set(roadTrip)
        val vm = viewModel()
        advanceUntilIdle()
        val gate = CompletableDeferred<Unit>().also { repo.gate = it }
        repo.fail = IOException("the server refused")
        vm.move(0, 2)
        runCurrent()
        assertEquals(listOf(2L, 3L, 1L), vm.order())
        gate.complete(Unit)
        advanceUntilIdle()
        assertEquals(listOf(1L, 2L, 3L), vm.order())
        assertEquals("the server refused", (events.single() as PlaylistEvent.Failed).error.message)
    }

    @Test
    fun movesMadeWhileOneIsOnItsWaySendTheLatestOrderOnly() = runTest(dispatcher) {
        repo.set(roadTrip)
        val vm = viewModel()
        advanceUntilIdle()
        val gate = CompletableDeferred<Unit>().also { repo.gate = it }
        vm.move(0, 1)
        runCurrent()
        vm.move(1, 2)
        runCurrent()
        vm.move(0, 1)
        runCurrent()
        assertEquals(listOf(3L, 2L, 1L), vm.order())
        gate.complete(Unit)
        advanceUntilIdle()
        assertEquals(listOf("reorder 1 [2, 1, 3]", "reorder 1 [3, 2, 1]"), repo.sent)
        assertEquals(listOf(3L, 2L, 1L), vm.order())
    }

    @Test
    fun aTvMoveModeSendsOnceOrPutsTheOrderBack() = runTest(dispatcher) {
        repo.set(roadTrip)
        val vm = viewModel()
        advanceUntilIdle()
        vm.move(0, 1, send = false)
        vm.move(1, 2, send = false)
        advanceUntilIdle()
        assertEquals(listOf(2L, 3L, 1L), vm.order())
        assertEquals(emptyList<String>(), repo.sent)
        vm.cancelMoves()
        advanceUntilIdle()
        assertEquals(listOf(1L, 2L, 3L), vm.order())
        vm.move(2, 0, send = false)
        vm.sendOrder()
        advanceUntilIdle()
        assertEquals(listOf("reorder 1 [3, 1, 2]"), repo.sent)
    }

    @Test
    fun aRemovalCanBeUndoneUntilItIsSent() = runTest(dispatcher) {
        repo.set(roadTrip)
        val vm = viewModel()
        advanceUntilIdle()
        vm.remove(2, undoable = true)
        runCurrent()
        assertEquals(listOf(1L, 3L), vm.order())
        assertEquals(listOf(UiText.plural(R.plurals.playlists_count_items, 2), UiText.raw("2h 30m")), vm.page().metaTexts)
        assertEquals(PlaylistEvent.Removed(2, "Half Loop"), events.single())
        advanceTimeBy(PlaylistViewModel.UNDO_MS - 1)
        vm.undo(2)
        advanceUntilIdle()
        assertEquals(listOf(1L, 2L, 3L), vm.order())
        assertEquals(emptyList<String>(), repo.sent)
        // Not undone: sent once the undo has had its time.
        vm.remove(2, undoable = true)
        advanceTimeBy(PlaylistViewModel.UNDO_MS + 1)
        advanceUntilIdle()
        assertEquals(listOf("remove 1 [2]"), repo.sent)
        assertEquals(listOf(1L, 3L), vm.order())
    }

    @Test
    fun aRefusedRemovalBringsTheEntryBack() = runTest(dispatcher) {
        repo.set(roadTrip)
        val vm = viewModel()
        advanceUntilIdle()
        repo.fail = IOException("no")
        vm.remove(3, undoable = false)
        advanceUntilIdle()
        assertEquals(listOf(1L, 2L, 3L), vm.order())
        assertTrue(events.last() is PlaylistEvent.Failed)
    }

    @Test
    fun leavingThePageSendsRemovalsStillWaiting() = runTest(dispatcher) {
        repo.set(roadTrip)
        val deps = PlaylistDeps(repo, MutableStateFlow("http://pley"), dispatcher, work = work)
        val store = ViewModelStore()
        val made = PlaylistViewModel(1, deps, player, FakeTiles)
        val vm = ViewModelProvider.create(store, object : ViewModelProvider.Factory {
            @Suppress("UNCHECKED_CAST")
            override fun <T : ViewModel> create(modelClass: KClass<T>, extras: CreationExtras): T = made as T
        })[PlaylistViewModel::class]
        val job = launch { vm.state.collect {} }
        advanceUntilIdle()
        vm.remove(1, undoable = true)
        runCurrent()
        job.cancel()
        store.clear()
        advanceUntilIdle()
        assertEquals(listOf("remove 1 [1]"), repo.sent)
    }

    @Test
    fun renamingChecksTheTitle() = runTest(dispatcher) {
        repo.set(roadTrip)
        val vm = viewModel()
        advanceUntilIdle()
        assertFalse(vm.rename("   "))
        assertFalse(vm.rename("x".repeat(Playlists.TITLE_MAX + 1)))
        assertTrue(vm.rename("  Long drive  "))
        advanceUntilIdle()
        assertEquals(listOf("rename 1 Long drive"), repo.sent)
        assertEquals("Long drive", vm.page().title)
        assertEquals(TitleProblem.Empty, Playlists.check(""))
        assertEquals(TitleProblem.TooLong, Playlists.check("x".repeat(201)))
        assertEquals(null, Playlists.check("x".repeat(200)))
    }

    @Test
    fun videoPlaysAsAQueueOfThePlaylist() = runTest(dispatcher) {
        repo.set(roadTrip)
        val vm = viewModel(Random(3))
        advanceUntilIdle()
        vm.play()
        assertEquals(PlayRequest(itemId = 10, queue = PlayQueue.Playlist(1)), played.last())
        vm.play(from = 1)
        assertEquals(PlayRequest(itemId = 20, queue = PlayQueue.Playlist(1)), played.last())
        vm.play(shuffle = true)
        val start = roadTrip.entries[Random(3).nextInt(3)].item.id
        assertEquals(PlayRequest(itemId = start, queue = PlayQueue.Playlist(1)), played.last())
    }

    @Test
    fun musicOpensTheTracksAlbum() = runTest(dispatcher) {
        repo.set(playlist(1, "Mix", track(70, "Tidal", album = 50), kind = PlaylistKind.Audio))
        val vm = viewModel()
        advanceUntilIdle()
        assertFalse(vm.page().video)
        assertEquals(TileLook.Square, vm.page().rows.single().tile.look)
        vm.play()
        advanceUntilIdle()
        assertEquals(emptyList<PlayRequest>(), played)
        assertEquals(PlaylistEvent.Open(PleyKey.Album(50)), events.single())
    }

    @Test
    fun deletingSaysWhenItIsGone() = runTest(dispatcher) {
        repo.set(roadTrip)
        val vm = viewModel()
        advanceUntilIdle()
        vm.delete()
        advanceUntilIdle()
        assertEquals(listOf("delete 1"), repo.sent)
        assertEquals(PlaylistEvent.Deleted, events.single())
    }

    @Test
    fun aMissingPlaylistFails() = runTest(dispatcher) {
        repo.detail(1).value = Loadable.Failed(IOException("gone"))
        val vm = viewModel()
        advanceUntilIdle()
        assertTrue(vm.state.value is Loadable.Failed)
    }
}
