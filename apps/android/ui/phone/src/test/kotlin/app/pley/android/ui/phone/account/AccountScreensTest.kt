package app.pley.android.ui.phone.account

import android.app.Application
import android.content.ComponentName
import android.content.Intent
import android.content.IntentFilter
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.remember
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.assertIsEnabled
import androidx.compose.ui.test.assertIsOff
import androidx.compose.ui.test.assertIsOn
import androidx.compose.ui.test.hasClickAction
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.isHeading
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onAllNodesWithContentDescription
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onLast
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollToNode
import androidx.compose.ui.test.getBoundsInRoot
import androidx.compose.ui.test.onRoot
import androidx.compose.ui.test.hasScrollAction
import androidx.test.core.app.ApplicationProvider
import app.pley.android.common.perf.FrameRate
import app.pley.android.data.AdminAccess
import app.pley.android.data.TrackDefaultsUnsupported
import app.pley.android.model.SubtitleMode
import app.pley.android.network.ApiException
import app.pley.android.player.NowPlaying
import app.pley.android.player.MusicRepeat
import app.pley.android.screens.Navigator
import app.pley.android.screens.PleyKey
import app.pley.android.testing.data.FakeServerAdmin
import app.pley.android.ui.phone.admin.ManageServerScreen
import app.pley.android.ui.phone.diagnostics.DIAGNOSTICS_FRAME_RATE
import app.pley.android.ui.phone.diagnostics.DiagnosticsScreen
import app.pley.android.ui.phone.kit.LocalOffline
import app.pley.android.ui.phone.music.AlbumScreen
import app.pley.android.ui.phone.music.NowPlayingScreen
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
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

/**
 * The phone's Me, history, album page and mini player over a graph of
 * fakes: track defaults saved and shown read-only when the server refuses,
 * history paging and removal, playing an album from a row, the mini
 * player's controls, Diagnostics' frame-rate switch, and Manage server
 * for a phone that must sign in again to manage the server, with a browser
 * for it or without one.
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

    /** A control renamed by work done off the main thread (Repeat's next press) reads [description] in a moment. */
    private fun waitForDescription(description: String) {
        compose.waitUntil(5_000) { compose.onAllNodesWithContentDescription(description).fetchSemanticsNodes().isNotEmpty() }
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
    fun manageServerOpensItsPageWhenItNeedsANewSignIn() {
        // The page says why and what to tick, with its button: the row doesn't sign in on its own.
        graph.admin = FakeServerAdmin(AdminAccess.SignInAgain)
        show { MeScreen() }
        waitFor("Anna")
        compose.onNode(hasScrollAction()).performScrollToNode(hasText("Manage server"))
        compose.onNodeWithText("Sign in again to manage it from this phone").assertExists()
        compose.onNodeWithText("Manage server").performClick()
        assertEquals(listOf(PleyKey.ManageServer), opened)
    }

    @Test
    fun manageServerSaysWhatToTickBeforeSigningInAgain() {
        installBrowser()
        graph.admin = FakeServerAdmin(AdminAccess.SignInAgain)
        show { ManageServerScreen() }
        waitFor("Sign in again to manage the server")
        compose.onNodeWithText("Sign in again in the browser, not with a code, and tick Manage the server.", substring = true).assertExists()
        compose.onNode(hasText("Sign in again") and hasClickAction()).assertExists()
        compose.onNodeWithText("Dashboard").assertDoesNotExist()
    }

    @Test
    fun manageServerSaysABrowserIsNeededOnAPhoneWithoutOne() {
        // Signing in there takes a code, which never allows managing the server: no sign-in that can't give it.
        graph.admin = FakeServerAdmin(AdminAccess.SignInAgain)
        show { ManageServerScreen() }
        waitFor("Managing the server needs a browser")
        compose.onNodeWithText("Only a sign-in through a browser on this phone can let it manage the server", substring = true).assertExists()
        compose.onNode(hasText("Sign in again") and hasClickAction()).assertDoesNotExist()
        compose.onNodeWithText("Sign in again to manage the server").assertDoesNotExist()
    }

    /** A browser, for the server's sign-in page to open in. */
    private fun installBrowser() {
        val app = ApplicationProvider.getApplicationContext<Application>()
        val activity = ComponentName("com.example.browser", "com.example.browser.Main")
        shadowOf(app.packageManager).run {
            addActivityIfNotPresent(activity)
            addIntentFilterForActivity(
                activity,
                IntentFilter(Intent.ACTION_VIEW).apply {
                    addCategory(Intent.CATEGORY_DEFAULT)
                    addCategory(Intent.CATEGORY_BROWSABLE)
                    addDataScheme("http")
                    addDataScheme("https")
                },
            )
        }
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

    @Test
    fun nowPlayingRepeatsTheQueueThenTheTrackThenNothingAndShowsTheNextTrackAsItChanges() {
        val now = NowPlaying(503, "Song 3", "Harbour Lights", playing = true, album = "Deep Water", count = 12, index = 11, durationMs = 180_000)
        graph.music.now.value = now
        show { NowPlayingScreen() }
        waitFor("Song 3")
        // The last track of a queue that doesn't repeat has no next one.
        compose.onNodeWithContentDescription("Next").assertIsNotEnabled()
        compose.onNodeWithContentDescription("Repeat").performClick()
        assertEquals(listOf(MusicRepeat.All), graph.music.repeats)

        graph.music.now.value = now.copy(repeat = MusicRepeat.All)
        waitForDescription("Repeat this track")
        compose.onNodeWithContentDescription("Repeat this track").performClick()
        compose.onNodeWithContentDescription("Next").assertIsEnabled()
        graph.music.now.value = now.copy(repeat = MusicRepeat.One)
        waitForDescription("Stop repeating")
        compose.onNodeWithContentDescription("Stop repeating").performClick()
        assertEquals(listOf(MusicRepeat.All, MusicRepeat.One, MusicRepeat.Off), graph.music.repeats)

        // The next track: its words take the place of the last one's, which leave.
        graph.music.now.value = now.copy(itemId = 504, title = "Song 4", index = 0, playing = false, loading = true, wantsPlay = true)
        waitFor("Song 4")
        compose.waitForIdle()
        compose.onNodeWithText("Song 3").assertDoesNotExist()
        // Still starting, it will play: Pause, and no "Loading…" for a track that starts at once.
        compose.onNodeWithContentDescription("Pause").assertExists()
        compose.onNodeWithText("Loading…").assertDoesNotExist()
    }

    @Test
    @Config(qualifiers = "w800dp-h360dp-land")
    fun nowPlayingInAShortWideWindowPutsTheArtBesideTheTitleAndShowsEveryControl() {
        val now = NowPlaying(503, "Song 3", "Harbour Lights", playing = true, album = "Deep Water", count = 12, index = 2, durationMs = 180_000)
        graph.music.now.value = now
        show { NowPlayingScreen() }
        waitFor("Song 3")
        val window = compose.onRoot().getBoundsInRoot()
        val title = compose.onNodeWithText("Song 3").getBoundsInRoot()
        // Beside the art, not under it: the title starts a fifth of the way across, in the top half.
        assertTrue("title: $title", title.left > window.right / 5 && title.bottom < window.bottom / 2)
        // A phone's portrait layout put the scrubber and the controls below the window's bottom.
        for (control in listOf("Position in the track", "Shuffle", "Previous", "Pause", "Next", "Repeat", "Queue")) {
            val bounds = compose.onAllNodesWithContentDescription(control)[0].getBoundsInRoot()
            assertTrue("$control: $bounds in $window", bounds.bottom <= window.bottom && bounds.top >= window.top)
        }
    }

    @Test
    @Config(qualifiers = "w800dp-h360dp-land")
    @GraphicsMode(GraphicsMode.Mode.NATIVE)
    fun anAlbumInAShortWideWindowPutsItsCoverBesideItsTitleWithPlayInView() {
        show { AlbumScreen(AccountTestGraph.ALBUM, trackId = null) }
        waitFor("Deep Water")
        val window = compose.onRoot().getBoundsInRoot()
        val title = compose.onNode(hasText("Deep Water") and isHeading()).getBoundsInRoot()
        // Beside the cover, as an item's poster: a phone's layout stacked them, Play under the window's bottom.
        assertTrue("title: $title", title.left > window.right / 5)
        for (button in listOf("Play", "Shuffle")) {
            val bounds = compose.onNode(hasText(button) and hasClickAction()).getBoundsInRoot()
            assertTrue("$button: $bounds in $window", bounds.bottom <= window.bottom)
        }
        // Side by side, as tall as each other.
        val play = compose.onNode(hasText("Play") and hasClickAction()).getBoundsInRoot()
        val shuffle = compose.onNode(hasText("Shuffle") and hasClickAction()).getBoundsInRoot()
        assertEquals((play.bottom - play.top).value, (shuffle.bottom - shuffle.top).value, 0.5f)
    }
}
