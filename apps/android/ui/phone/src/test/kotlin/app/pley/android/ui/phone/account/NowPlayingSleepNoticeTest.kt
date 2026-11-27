package app.pley.android.ui.phone.account

import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.remember
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.test.getBoundsInRoot
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import app.pley.android.player.NowPlaying
import app.pley.android.player.sleep.SleepChoice
import app.pley.android.player.sleep.SleepState
import app.pley.android.screens.Navigator
import app.pley.android.screens.PleyKey
import app.pley.android.ui.phone.kit.LocalOffline
import app.pley.android.ui.phone.music.NowPlayingScreen
import app.pley.android.ui.phone.music.TAG_MUSIC_SLEEP_NOTICE
import app.pley.android.ui.phone.shell.LocalPhoneNavigator
import app.pley.android.ui.phone.shell.LocalPhoneShell
import app.pley.android.ui.phone.shell.PhoneShellState
import app.pley.android.ui.phone.shell.rememberPhoneNavigation
import app.pley.android.ui.phone.theme.PhoneTheme
import org.junit.Assert.assertEquals
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

/** Now Playing's notice of the sleep timer's last 30 s, and its Keep listening. */
@RunWith(RobolectricTestRunner::class)
@Config(application = AccountTestApplication::class, qualifiers = "w411dp-h891dp-port-xxhdpi")
class NowPlayingSleepNoticeTest {
    @get:Rule
    val compose = createComposeRule()

    private val graph = AccountTestGraph()
    private val navigator = object : Navigator {
        override fun open(key: PleyKey) = Unit

        override fun back() = Unit

        override fun restart(key: PleyKey) = Unit
    }

    @Before
    fun graph() {
        AccountTestApplication.graph = graph
        graph.music.now.value = NowPlaying(503, "Song 3", "Harbour Lights", playing = true, album = "Deep Water", count = 12, index = 2, durationMs = 180_000)
    }

    @Test
    fun theNoticeKeepsTheWidthOfItsThirtySecondsAsTheCountGoesDown() {
        graph.music.sleepState.value = SleepState(SleepChoice.Minutes(30), secondsLeft = 30, minutesLeft = 1)
        compose.setContent {
            PhoneTheme {
                val navigation = rememberPhoneNavigation(PleyKey.Home) {}
                val resources = LocalResources.current
                val shell = remember { PhoneShellState(navigation, resources, AccountTestGraph.SERVER) }
                CompositionLocalProvider(
                    LocalPhoneNavigator provides navigator,
                    LocalPhoneShell provides shell,
                    LocalOffline provides shell.offlineState,
                ) { NowPlayingScreen(car = false) }
            }
        }
        compose.onNodeWithText("Pausing in 30 s").assertExists()
        val wide = compose.onNodeWithTag(TAG_MUSIC_SLEEP_NOTICE).getBoundsInRoot()

        graph.music.sleepState.value = SleepState(SleepChoice.Minutes(30), secondsLeft = 9, minutesLeft = 1)
        compose.waitForIdle()
        compose.onNodeWithText("Pausing in 9 s").assertExists()
        assertEquals(wide, compose.onNodeWithTag(TAG_MUSIC_SLEEP_NOTICE).getBoundsInRoot())

        compose.onNodeWithText("Keep listening").performClick()
        assertEquals(1, graph.music.keeps)
    }
}
