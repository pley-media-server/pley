package app.pley.android.ui.phone.account

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.test.junit4.createComposeRule
import app.pley.android.player.NowPlaying
import app.pley.android.screens.Navigator
import app.pley.android.screens.PleyKey
import app.pley.android.ui.phone.kit.LocalOffline
import app.pley.android.ui.phone.music.NowPlayingScreen
import app.pley.android.ui.phone.player.CarScreenChecks
import app.pley.android.ui.phone.shell.LocalPhoneNavigator
import app.pley.android.ui.phone.shell.LocalPhoneShell
import app.pley.android.ui.phone.shell.MiniPlayerSlot
import app.pley.android.ui.phone.shell.PhoneShellState
import app.pley.android.ui.phone.shell.rememberPhoneNavigation
import app.pley.android.ui.phone.theme.PhoneTheme
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

/** Music's Now Playing and its bar, car-sized, on each of the screens Android Auto runs on ([CarScreenChecks]). */
@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(application = AccountTestApplication::class)
class CarMusicSizesTest {
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
        graph.music.now.value = NOW
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
    }

    private fun check(screen: String) {
        show { NowPlayingScreen(car = true) }
        CarScreenChecks.save(compose, "now-playing-$screen")
        val controls = listOf("Close Now Playing", "Queue", "Sleep timer", "Position in the track", "Shuffle", "Previous", "Pause", "Next", "Repeat")
        CarScreenChecks.controls(compose, "Now Playing, $screen", controls)
        CarScreenChecks.apart(compose, "Now Playing, $screen", controls)
    }

    private fun checkBar(screen: String) {
        show { Box(Modifier.fillMaxSize()) { MiniPlayerSlot(NOW, Modifier.align(Alignment.BottomCenter), car = true) } }
        CarScreenChecks.save(compose, "bar-$screen")
        CarScreenChecks.controls(compose, "the bar, $screen", listOf("Pause", "Next"))
        CarScreenChecks.apart(compose, "the bar, $screen", listOf("Pause", "Next"))
    }

    @Test
    @Config(qualifiers = CarScreenChecks.SMALL)
    fun smallLandscape() = check("small")

    @Test
    @Config(qualifiers = CarScreenChecks.WIDE)
    fun wideLandscape() = check("wide")

    @Test
    @Config(qualifiers = CarScreenChecks.PORTRAIT)
    fun portrait() = check("portrait")

    @Test
    @Config(qualifiers = CarScreenChecks.DENSE)
    fun dense() = check("dense")

    @Test
    @Config(qualifiers = CarScreenChecks.TALL)
    fun tall() = check("tall")

    @Test
    @Config(qualifiers = CarScreenChecks.TALL_SPLIT)
    fun tallSplit() = check("tall-split")

    @Test
    @Config(qualifiers = CarScreenChecks.ULTRAWIDE)
    fun ultrawide() = check("ultrawide")

    @Test
    @Config(qualifiers = CarScreenChecks.FULL_HD)
    fun fullHd() = check("full-hd")

    @Test
    @Config(qualifiers = CarScreenChecks.TALL_PORTRAIT)
    fun tallPortrait() = check("tall-portrait")

    @Test
    @Config(qualifiers = CarScreenChecks.NARROW)
    fun narrow() = check("narrow")

    @Test
    @Config(qualifiers = CarScreenChecks.SMALL)
    fun barSmallLandscape() = checkBar("small")

    @Test
    @Config(qualifiers = CarScreenChecks.WIDE)
    fun barWideLandscape() = checkBar("wide")

    @Test
    @Config(qualifiers = CarScreenChecks.PORTRAIT)
    fun barPortrait() = checkBar("portrait")

    @Test
    @Config(qualifiers = CarScreenChecks.DENSE)
    fun barDense() = checkBar("dense")

    @Test
    @Config(qualifiers = CarScreenChecks.TALL)
    fun barTall() = checkBar("tall")

    @Test
    @Config(qualifiers = CarScreenChecks.TALL_SPLIT)
    fun barTallSplit() = checkBar("tall-split")

    @Test
    @Config(qualifiers = CarScreenChecks.ULTRAWIDE)
    fun barUltrawide() = checkBar("ultrawide")

    @Test
    @Config(qualifiers = CarScreenChecks.FULL_HD)
    fun barFullHd() = checkBar("full-hd")

    @Test
    @Config(qualifiers = CarScreenChecks.TALL_PORTRAIT)
    fun barTallPortrait() = checkBar("tall-portrait")

    @Test
    @Config(qualifiers = CarScreenChecks.NARROW)
    fun barNarrow() = checkBar("narrow")

    private companion object {
        val NOW = NowPlaying(503, "Song 3", "Harbour Lights", playing = true, album = "Deep Water", count = 12, index = 2, durationMs = 180_000)
    }
}
