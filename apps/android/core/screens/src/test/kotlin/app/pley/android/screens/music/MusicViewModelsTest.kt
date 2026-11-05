@file:OptIn(ExperimentalCoroutinesApi::class)

package app.pley.android.screens.music

import app.pley.android.common.UiText
import app.pley.android.data.Loadable
import app.pley.android.data.PlayItem
import app.pley.android.data.PlayTarget
import app.pley.android.model.ItemKind
import app.pley.android.network.ApiException
import app.pley.android.player.NowPlaying
import app.pley.android.screens.R
import java.util.Locale
import kotlin.random.Random
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.flowOf
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
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

class MusicViewModelsTest {
    private val dispatcher = UnconfinedTestDispatcher()
    private val items = FakeItems()
    private val music = FakeMusic()
    private val watch = FakeWatchState()

    @Before
    fun main() = Dispatchers.setMain(dispatcher)

    @After
    fun reset() = Dispatchers.resetMain()

    private fun TestScope.albumModel(highlight: Long? = null) =
        AlbumViewModel(items, flowOf(ORIGIN), music, watch, 50, highlight, dispatcher, Random(2)).also { vm ->
            backgroundScope.launch { vm.state.collect { } }
        }

    @Test
    fun anAlbumShowsItsTracksWithDiscHeadersOnlyWhenThereAreSeveralDiscs() {
        val one = albumRows(listOf(track(1, disc = 1), track(2, disc = 1)), "Harbour Lights")
        assertTrue(one.none { it is AlbumRow.Disc })

        val two = albumRows(listOf(track(1, disc = 1), track(2, disc = 2), track(3, disc = 2)), "Harbour Lights")
        assertEquals(listOf("disc-1", "track-1", "disc-2", "track-2", "track-3"), two.map { it.key })
        val second = two[3] as AlbumRow.Track
        assertEquals(1, second.position)
        assertEquals("3:05", second.duration)
        // The album's own artist isn't repeated on every row; a guest is.
        assertNull(second.artist)
        val guest = albumRows(listOf(track(9, artist = "Nadia Fen")), "Harbour Lights").single() as AlbumRow.Track
        assertEquals("Nadia Fen", guest.artist)
    }

    @Test
    fun theAlbumPageSaysYearTracksAndLength() = runTest(dispatcher) {
        val vm = albumModel()
        items.detailOf(50).value = Loadable.Ready(ALBUM_DETAIL, fresh = false)
        val loading = (vm.state.value as MusicPageState.Ready).page
        assertTrue(loading.tracksLoading)
        items.kidsOf(50).value = Loadable.Ready((1..10L).map { track(it) }, fresh = true)
        val page = (vm.state.value as MusicPageState.Ready).page
        assertEquals(listOf(UiText.plural(R.plurals.music_count_tracks, 10), UiText.raw("31m")), page.metaTexts)
        assertEquals(2019, page.year)
        assertEquals("Harbour Lights", page.artist)
        assertEquals(40L, page.artistId)
    }

    @Test
    fun playShuffleAndARowPlayTheAlbumAndTheTrackPlayingToggles() = runTest(dispatcher) {
        val vm = albumModel()
        items.detailOf(50).value = Loadable.Ready(ALBUM_DETAIL, fresh = true)
        items.kidsOf(50).value = Loadable.Ready((1..5L).map { track(it) }, fresh = true)
        val events = mutableListOf<MusicEvent>()
        backgroundScope.launch { vm.events.collect { events += it } }

        vm.play()
        assertEquals(Triple((1..5L).toList(), 0, false), music.played.last())
        vm.shuffle()
        assertTrue(music.played.last().third)
        vm.playOrToggle(3)
        assertEquals(3, music.played.last().second)
        assertEquals(listOf(MusicEvent.Played, MusicEvent.Played, MusicEvent.Played), events)

        // Track 4 plays now: its row pauses it instead of starting over.
        music.now.value = NowPlaying(4, "Track 4", "Harbour Lights", playing = true)
        val ready = vm.state.value as MusicPageState.Ready
        assertEquals(4L, ready.current)
        assertTrue(ready.playing)
        vm.playOrToggle(3)
        assertEquals(1, music.toggles)
        assertEquals(3, music.played.size)
    }

    @Test
    fun musicFromAnotherAlbumMarksNoRow() = runTest(dispatcher) {
        val vm = albumModel()
        items.detailOf(50).value = Loadable.Ready(ALBUM_DETAIL, fresh = true)
        items.kidsOf(50).value = Loadable.Ready((1..5L).map { track(it) }, fresh = true)
        music.now.value = NowPlaying(999, "Elsewhere", null, playing = true)
        val ready = vm.state.value as MusicPageState.Ready
        assertNull(ready.current)
        assertFalse(ready.playing)
    }

    @Test
    fun anAlbumWithoutTracksHasNothingToPlay() = runTest(dispatcher) {
        val vm = albumModel()
        items.detailOf(50).value = Loadable.Ready(ALBUM_DETAIL, fresh = true)
        items.kidsOf(50).value = Loadable.Ready(emptyList(), fresh = true)
        val events = mutableListOf<MusicEvent>()
        backgroundScope.launch { vm.events.collect { events += it } }
        vm.play()
        assertEquals(listOf<MusicEvent>(MusicEvent.NothingToPlay), events)
        assertTrue(music.played.isEmpty())
    }

    @Test
    fun theRatingGoesToTheServer() = runTest(dispatcher) {
        val vm = albumModel()
        vm.rate(7)
        vm.rate(null)
        assertEquals(listOf(50L to 7.0, 50L to null), watch.ratings)
    }

    @Test
    fun aMissingAlbumIsNotFound() = runTest(dispatcher) {
        val vm = albumModel()
        items.detailOf(50).value = Loadable.Failed(ApiException(404, "not_found", "Item not found"))
        assertSame(MusicPageState.NotFound, vm.state.value)
    }

    @Test
    fun anArtistsAlbumsAreNewestFirstAndPlayGathersEveryTrack() = runTest(dispatcher) {
        val all = (1..12L).map { track(it) }
        val resolve = PlayItem { item, _ ->
            assertEquals(ItemKind.Artist, item.kind)
            PlayTarget.Music(all.map { it.id }, 0, all)
        }
        val vm = ArtistViewModel(items, flowOf(ORIGIN), resolve, music, 40, dispatcher, Random(5), Locale.US)
        backgroundScope.launch { vm.state.collect { } }
        items.detailOf(40).value = Loadable.Ready(ARTIST_DETAIL, fresh = true)
        items.kidsOf(40).value = Loadable.Ready(listOf(album(1, 2001), album(2, 2019), album(3, null), album(4, 2019, "Aaa")), fresh = true)
        val page = (vm.state.value as MusicPageState.Ready).page
        assertEquals(listOf(4L, 2L, 1L, 3L), page.albums.map { it.id })
        assertEquals(listOf(UiText.plural(R.plurals.music_count_albums, 3), UiText.plural(R.plurals.music_count_tracks, 30)), page.countTexts)

        vm.play(shuffle = false)
        vm.state.first { it is MusicPageState.Ready && !it.busy }
        assertEquals(Triple(all.map { it.id }, 0, false), music.played.last())

        vm.play(shuffle = true)
        vm.state.first { it is MusicPageState.Ready && !it.busy }
        assertTrue(music.played.last().third)
    }
}
