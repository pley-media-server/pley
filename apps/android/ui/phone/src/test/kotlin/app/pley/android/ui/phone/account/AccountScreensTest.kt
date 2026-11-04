package app.pley.android.ui.phone.account

import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.remember
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.assertIsOff
import androidx.compose.ui.test.assertIsOn
import androidx.compose.ui.test.hasClickAction
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onAllNodesWithContentDescription
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onLast
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollToNode
import androidx.compose.ui.test.hasScrollAction
import androidx.test.core.app.ApplicationProvider
import app.pley.android.common.perf.FrameRate
import app.pley.android.data.TrackDefaultsUnsupported
import app.pley.android.model.SubtitleMode
import app.pley.android.network.ApiException
import app.pley.android.player.NowPlaying
import app.pley.android.screens.Navigator
import app.pley.android.screens.PleyKey
import app.pley.android.ui.phone.diagnostics.DIAGNOSTICS_FRAME_RATE
import app.pley.android.ui.phone.diagnostics.DiagnosticsScreen
import app.pley.android.ui.phone.kit.LocalOffline
import app.pley.android.ui.phone.music.AlbumScreen
import app.pley.android.ui.phone.shell.LocalPhoneNavigator
import app.pley.android.ui.phone.shell.LocalPhoneShell
import app.pley.android.ui.phone.shell.MiniPlayerSlot
import app.pley.android.ui.phone.shell.PhoneShellState
import app.pley.android.ui.phone.shell.rememberPhoneNavigation
import app.pley.android.ui.phone.theme.PhoneTheme
import org.junit.Assert.assertEquals
import org.junit.After
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

/**
 * The phone's Me, history, album page and mini player over a graph of
 * fakes: track defaults saved and shown read-only when the server refuses,
 * history paging and removal, playing an album from a row, the mini
 * player's controls, Diagnostics' frame-rate switch.
 */
@RunWith(RobolectricTestRunner::class)
@Config(qualifiers = "w412dp-h915dp-xxhdpi", application = AccountTestApplication::class)
class AccountScreensTest {
    @get:Rule
    val compose = createComposeRule()

    private val graph = AccountTestGraph()
    private val opened = mutableListOf<PleyKey>()
    private val navigator = object : Navigator {
        override fun open(key: PleyKey) {
            opened += key
        }

        override fun back() = Unit

        override fun restart(key: PleyKey) {
            opened += key
        }
    }

    @Before
    fun graph() {
        AccountTestApplication.graph = graph
    }

    @After
    fun frameRateOff() {
        FrameRate.show(ApplicationProvider.getApplicationContext(), false)
    }

    private fun show(content: @Composable () -> Unit) {
        compose.setContent {
            PhoneTheme {
                val navigation = rememberPhoneNavigation(PleyKey.Home) {}
                val resources = LocalResources.current
                val shell = remember { PhoneShellState(navigation, resources, AccountTestGraph.SERVER) }
                CompositionLocalProvider(
                    LocalPhoneNavigator provides navigator,
                    LocalPhoneShell provides shell,
                    LocalOffline provides shell.offlineState,
                ) { content() }
            }
        }
        compose.waitForIdle()
    }

    /** Work done off the main thread shows [text] in a moment. */
    private fun waitFor(text: String) {
        compose.waitUntil(5_000) { compose.onAllNodesWithText(text, substring = true).fetchSemanticsNodes().isNotEmpty() }
    }

    @Test
    fun meShowsTheProfileAndItsTrackDefaults() {
        show { MeScreen() }
        waitFor("Anna")
        // The profile's line: the server alone (the Switch server button names it too).
        compose.onNode(hasText("192.168.1.20:7539") and !hasClickAction()).assertExists()
        compose.onNodeWithText("English").assertExists()
        compose.onNodeWithText("Only for foreign dialogue").assertExists()
        // Forced subtitles follow the audio: no subtitle language to choose.
        compose.onNodeWithText("Subtitle language").assertDoesNotExist()
        compose.onNodeWithText("Switch profile").assertExists()
    }

    @Test
    fun choosingSubtitlesSavesThemOnTheServer() {
        show { MeScreen() }
        waitFor("Only for foreign dialogue")
        compose.onNode(hasText("Subtitles") and hasClickAction()).performClick()
        compose.onNodeWithText("Always").performClick()
        compose.waitUntil(5_000) { graph.saved.isNotEmpty() }
        assertEquals(SubtitleMode.Always, graph.saved.single().subtitleMode)
        // Full subtitles need a language to read.
        waitFor("Subtitle language")
        compose.onNodeWithText("Same as the audio language").assertExists()
    }

    @Test
    fun aServerThatRefusesShowsTheDefaultsReadOnlyToChangeOnTheWeb() {
        graph.refuse = TrackDefaultsUnsupported(ApiException(403, "insufficient_scope", "Not allowed"))
        show { MeScreen() }
        waitFor("Only for foreign dialogue")
        compose.onNode(hasText("Subtitles") and hasClickAction()).performClick()
        compose.onNodeWithText("Always").performClick()
        waitFor("Change on the web")
        compose.onNode(hasText("Audio language") and hasClickAction()).assertIsNotEnabled()
        assertTrue(graph.saved.isEmpty())
    }

    @Test
    fun signingOutAsksFirstInOneShortLine() {
        show { MeScreen() }
        waitFor("Anna")
        compose.onNode(hasScrollAction()).performScrollToNode(hasText("Sign out") and hasClickAction())
        compose.onNode(hasText("Sign out") and hasClickAction()).performClick()
        compose.onNodeWithText("Sign out?").assertExists()
        compose.onNodeWithText("You'll be signed out of 192.168.1.20:7539.").assertExists()
        compose.onAllNodes(hasText("Sign out") and hasClickAction()).onLast().performClick()
        compose.waitUntil(5_000) { graph.sessionFlow.value == null }
    }

    @Test
    fun meOpensHistorySettingsAndDiagnostics() {
        show { MeScreen() }
        waitFor("Watch history")
        compose.onNodeWithText("Watch history").performClick()
        compose.onNode(hasScrollAction()).performScrollToNode(hasText("Diagnostics"))
        compose.onNodeWithText("Diagnostics").performClick()
        compose.onNodeWithText("Settings").performClick()
        assertEquals(listOf(PleyKey.History, PleyKey.Diagnostics, PleyKey.Settings), opened)
    }

    @Test
    fun diagnosticsSwitchesTheFrameRateOnAndOff() {
        show { DiagnosticsScreen() }
        compose.onNodeWithTag(DIAGNOSTICS_FRAME_RATE).assertIsOff()
        compose.onNodeWithText("Show frame rate").assertExists()
        compose.onNodeWithTag(DIAGNOSTICS_FRAME_RATE).performClick()
        compose.waitUntil(5_000) { FrameRate.on.value }
        compose.onNodeWithTag(DIAGNOSTICS_FRAME_RATE).assertIsOn()
        compose.onNodeWithTag(DIAGNOSTICS_FRAME_RATE).performClick()
        compose.waitUntil(5_000) { !FrameRate.on.value }
        compose.onNodeWithTag(DIAGNOSTICS_FRAME_RATE).assertIsOff()
    }

    @Test
    fun historyPagesBy15AndRemovesARowAtOnce() {
        show { HistoryScreen() }
        waitFor("Film 1")
        compose.onNode(hasScrollAction()).performScrollToNode(hasText("Show more"))
        compose.onNodeWithText("Show more").performClick()
        compose.onNode(hasScrollAction()).performScrollToNode(hasText("Film 20"))
        compose.onNodeWithText("Film 20").assertExists()

        compose.onNode(hasScrollAction()).performScrollToNode(hasText("Film 2"))
        compose.onAllNodesWithContentDescription("Remove Film 2 from history")[0].performClick()
        compose.waitUntil(5_000) { graph.removed.isNotEmpty() }
        assertEquals(listOf(2L), graph.removed)
        compose.waitUntil(5_000) { compose.onAllNodesWithText("Film 2").fetchSemanticsNodes().isEmpty() }
    }

    @Test
    fun anAlbumPlaysFromTheRowTappedWithDiscHeaders() {
        show { AlbumScreen(AccountTestGraph.ALBUM, trackId = null) }
        waitFor("Deep Water")
        compose.onNode(hasScrollAction()).performScrollToNode(hasText("Disc 2"))
        compose.onNodeWithText("Disc 2").assertExists()
        compose.onNode(hasScrollAction()).performScrollToNode(hasText("Song 3"))
        compose.onNodeWithText("Song 3").performClick()
        assertEquals(AccountTestGraph.TRACKS.map { it.id } to 2, graph.music.played.single())
    }

    @Test
    fun theMiniPlayerShowsTheTrackAndItsControls() {
        val now = NowPlaying(503, "Song 3", "Harbour Lights", playing = true, album = "Deep Water", count = 12, index = 2)
        graph.music.now.value = now
        show { MiniPlayerSlot(now) }
        compose.onNodeWithText("Song 3").assertExists()
        compose.onNodeWithText("Harbour Lights · Deep Water").assertExists()
        compose.onAllNodesWithContentDescription("Pause")[0].performClick()
        compose.onAllNodesWithContentDescription("Next")[0].performClick()
        assertEquals(1, graph.music.toggles)
        assertEquals(1, graph.music.nexts)
    }
}
