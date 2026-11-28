package app.pley.android.ui.phone.livetv

import androidx.activity.ComponentActivity
import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.ViewModelStore
import androidx.lifecycle.viewmodel.CreationExtras
import app.pley.android.player.live.LivePlayback
import app.pley.android.screens.livetv.ChannelHandoff
import app.pley.android.screens.livetv.LivePlayerDeps
import app.pley.android.screens.livetv.LivePlayerViewModel
import app.pley.android.testing.data.FakeLiveTv
import app.pley.android.testing.player.engine.FakePlayerEngine
import app.pley.android.ui.phone.player.CarScreenChecks
import app.pley.android.ui.phone.player.CarSized
import app.pley.android.ui.phone.theme.PhoneTheme
import kotlin.reflect.KClass
import kotlinx.coroutines.flow.flowOf
import org.junit.After
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

/** Live TV's player on each of the screens Android Auto runs on ([CarScreenChecks]), where a parked car plays channels. */
@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class CarLivePlayerSizesTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    /** Holds the view model made, cleared after each test: its playback ends with it. */
    private val store = ViewModelStore()
    private val live = FakeLiveTv()

    @After
    fun clear() {
        store.clear()
        ChannelHandoff.take(0)
    }

    private fun check(screen: String) {
        val deps = LivePlayerDeps(
            live = live,
            playback = { scope -> LivePlayback(live, { FakePlayerEngine() }, { "android-1" }, scope) },
            origin = flowOf("http://nas:7539"),
        )
        val factory = object : ViewModelProvider.Factory {
            @Suppress("UNCHECKED_CAST")
            override fun <V : ViewModel> create(modelClass: KClass<V>, extras: CreationExtras): V =
                LivePlayerViewModel(deps, SavedStateHandle(), 1, "all") as V
        }
        val vm = ViewModelProvider.create(store, factory)[LivePlayerViewModel::class]
        compose.setContent {
            PhoneTheme {
                CarSized(car = true) { LivePlayerScreen(vm, onBack = {}, inPip = false, onPip = null) }
            }
        }
        compose.waitForIdle()
        CarScreenChecks.save(compose, "live-$screen")
        val controls = listOf("Back", favorite(), "Channels", "Playback details", "Channel down", "Channel up")
        CarScreenChecks.controls(compose, "Live TV, $screen", controls)
        CarScreenChecks.apart(compose, "Live TV, $screen", controls)
    }

    /** The channel's star, said by what a press does. */
    private fun favorite(): String =
        FAVORITE.firstOrNull { compose.onAllNodes(hasContentDescription(it)).fetchSemanticsNodes().isNotEmpty() } ?: FAVORITE.first()

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

    private companion object {
        val FAVORITE = listOf("Add to favorites", "Remove from favorites")
    }
}
