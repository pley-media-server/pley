package app.pley.android.ui.tv.music

import androidx.activity.ComponentActivity
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.test.assert
import androidx.compose.ui.test.assertIsFocused
import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onNodeWithTag
import androidx.navigation3.runtime.NavKey
import app.pley.android.common.Tier
import app.pley.android.player.MusicQueueState
import app.pley.android.player.NowPlaying
import app.pley.android.player.sleep.SleepChoice
import app.pley.android.player.sleep.SleepState
import app.pley.android.screens.PleyKey
import app.pley.android.ui.tv.item.LocalTvTileActions
import app.pley.android.ui.tv.item.TvTileActions
import app.pley.android.ui.tv.kit.focused
import app.pley.android.ui.tv.kit.press
import app.pley.android.ui.tv.settings.TvAccountTestApplication
import app.pley.android.ui.tv.settings.TvAccountTestGraph
import app.pley.android.ui.tv.shell.LocalTvNavigator
import app.pley.android.ui.tv.shell.RailState
import app.pley.android.ui.tv.shell.TvNavigator
import app.pley.android.ui.tv.theme.TvTheme
import org.junit.Assert.assertEquals
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

/**
 * Music on TV with the D-pad: an album starts on Play and plays from a
 * track chosen on the right, opening Now Playing; an album opened from a
 * track starts on it; Now Playing starts on Play/Pause, the queue under it
 * jumps to a track, and Down from the controls lands on the track playing.
 * An artist's album opens its actions beside the page.
 */
@RunWith(RobolectricTestRunner::class)
@Config(qualifiers = "w960dp-h540dp-xhdpi", application = TvAccountTestApplication::class)
class TvMusicFocusTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    private val graph = TvAccountTestGraph()
    private val stack = mutableListOf<NavKey>(PleyKey.Home)

    @Before
    fun graph() {
        TvAccountTestApplication.graph = graph
    }

    private fun show(content: @androidx.compose.runtime.Composable () -> Unit) {
        val navigator = TvNavigator(stack, RailState()) {}
        compose.setContent {
            TvTheme(Tier.T2, reducedMotion = true) {
                CompositionLocalProvider(LocalTvNavigator provides navigator) { content() }
            }
        }
    }

    private fun waitFor(text: String) {
        compose.waitUntil(5_000) { compose.onAllNodesWithText(text).fetchSemanticsNodes().isNotEmpty() }
        compose.waitForIdle()
    }

    @Test
    fun anAlbumStartsOnPlayAndPlaysFromTheTrackChosen() {
        show { TvAlbumScreen(TvAccountTestGraph.ALBUM, trackId = null) }
        waitFor("Song 1")
        compose.focused().assert(hasText("Play"))
        compose.press(Key.DirectionRight)
        compose.press(Key.DirectionRight)
        compose.focused().assert(hasText("Song 1"))
        compose.press(Key.DirectionDown, times = 2)
        compose.press(Key.DirectionCenter)
        assertEquals(TvAccountTestGraph.TRACKS.map { it.id } to 2, graph.music.played.single())
        assertEquals(PleyKey.NowPlaying, stack.last())
    }

    @Test
    fun anAlbumOpenedFromATrackStartsOnIt() {
        show { TvAlbumScreen(TvAccountTestGraph.ALBUM, trackId = 510) }
        waitFor("Song 10")
        compose.focused().assert(hasText("Song 10"))
    }

    @Test
    fun anAlbumsActionsOpenOverTheArtistsPage() {
        val asked = mutableListOf<Long>()
        show {
            CompositionLocalProvider(LocalTvTileActions provides TvTileActions { id, _ -> asked += id }) {
                TvArtistScreen(TvAccountTestGraph.ARTIST)
            }
        }
        waitFor("Harbour Lights")
        compose.waitUntil(5_000) { compose.onAllNodes(hasContentDescription("Shallows", substring = true)).fetchSemanticsNodes().isNotEmpty() }
        compose.press(Key.DirectionDown)
        compose.focused().assert(hasContentDescription("Shallows", substring = true))
        compose.press(Key.Menu)
        // The shell's panel, not a page of their own.
        assertEquals(1, asked.size)
        assertEquals(listOf<NavKey>(PleyKey.Home), stack)
    }

    @Test
    fun nowPlayingStartsOnPlayPauseAndTheQueueJumps() {
        graph.music.now.value = NowPlaying(501, "Song 1", "Harbour Lights", playing = true, album = "Deep Water", count = 12, index = 0)
        show { TvNowPlayingScreen() }
        waitFor("Harbour Lights · Deep Water")
        compose.onNodeWithTag(TV_PLAY_PAUSE).assertIsFocused()
        compose.press(Key.DirectionCenter)
        assertEquals(1, graph.music.toggles)
        compose.press(Key.DirectionDown)
        compose.press(Key.DirectionDown, times = 2)
        compose.press(Key.DirectionCenter)
        assertEquals(1, graph.music.jumps.size)
    }

    @Test
    fun downFromTheControlsLandsOnTheTrackPlaying() {
        // The last track: the queue can't bring it to its top, so it isn't the nearest line.
        graph.music.queueState.value = MusicQueueState(TvAccountTestGraph.TRACKS, 11)
        graph.music.now.value = NowPlaying(512, "Song 12", "Harbour Lights", playing = true, album = "Deep Water", count = 12, index = 11)
        show { TvNowPlayingScreen() }
        waitFor("Harbour Lights · Deep Water")
        compose.onNodeWithTag(TV_PLAY_PAUSE).assertIsFocused()
        compose.press(Key.DirectionDown)
        compose.focused().assert(hasText("Song 12"))
    }

    @Test
    fun theSleepTimerStepsThroughItsChoicesAndKeepsListeningDuringItsNotice() {
        graph.music.now.value = NowPlaying(501, "Song 1", "Harbour Lights", playing = true, album = "Deep Water", count = 12, index = 0)
        show { TvNowPlayingScreen() }
        waitFor("Harbour Lights · Deep Water")
        // Play/Pause, Next, then the timer.
        compose.press(Key.DirectionRight, times = 2)
        compose.press(Key.DirectionCenter)
        assertEquals(0 to 0, graph.music.toggles to graph.music.nexts)
        assertEquals(SleepChoice.Minutes(15), graph.music.sleepState.value.choice)
        compose.press(Key.DirectionCenter)
        assertEquals(SleepChoice.Minutes(30), graph.music.sleepState.value.choice)

        graph.music.sleepState.value = SleepState(SleepChoice.Minutes(30), secondsLeft = 20)
        waitFor("Pausing in 20 s")
        compose.press(Key.DirectionCenter)
        assertEquals(1, graph.music.keeps)
        assertEquals(SleepChoice.Minutes(30), graph.music.sleepState.value.choice)
    }
}
