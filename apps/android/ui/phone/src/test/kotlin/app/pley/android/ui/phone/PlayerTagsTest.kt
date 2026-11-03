package app.pley.android.ui.phone

import androidx.activity.ComponentActivity
import androidx.compose.ui.semantics.SemanticsPropertiesAndroid
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.assert
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.ViewModelStore
import androidx.lifecycle.viewmodel.CreationExtras
import app.pley.android.player.PlayerPhase
import app.pley.android.player.PlayerUiState
import app.pley.android.player.live.LivePlayback
import app.pley.android.player.subtitles.CaptionLook
import app.pley.android.screens.livetv.LivePlayerDeps
import app.pley.android.screens.livetv.LivePlayerViewModel
import app.pley.android.testing.data.FakeLiveTv
import app.pley.android.testing.player.engine.FakePlayerEngine
import app.pley.android.testing.player.session.FakePlaybackController
import app.pley.android.ui.phone.livetv.LIVE_PLAYER
import app.pley.android.ui.phone.livetv.LivePlayerScreen
import app.pley.android.ui.phone.player.PhonePlayerScreen
import app.pley.android.ui.phone.player.PictureState
import app.pley.android.ui.phone.player.PlayerHost
import app.pley.android.ui.phone.player.SubtitleChoice
import app.pley.android.ui.phone.player.TAG_PLAYER
import app.pley.android.ui.phone.theme.PhoneTheme
import kotlin.reflect.KClass
import kotlinx.coroutines.flow.flowOf
import org.junit.After
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

/**
 * The players run in windows of their own, outside the shell's root: they
 * say themselves that their tags are resource ids, which UI Automator (the
 * benchmarks' journeys) finds them by.
 */
@RunWith(RobolectricTestRunner::class)
@Config(qualifiers = "w915dp-h412dp-land")
class PlayerTagsTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    private val store = ViewModelStore()
    private val asResourceIds = SemanticsMatcher.expectValue(SemanticsPropertiesAndroid.TestTagsAsResourceId, true)

    @After
    fun clear() = store.clear()

    private object Host : PlayerHost {
        override fun back() = Unit

        override val pipSupported = false

        override fun enterPip() = Unit

        override fun volume() = 0.5f

        override fun setVolume(level: Float) = Unit

        override fun brightness() = 0.5f

        override fun setBrightness(level: Float) = Unit
    }

    @Test
    fun `the player's tags are resource ids`() {
        val controller = FakePlaybackController(PlayerUiState(phase = PlayerPhase.Playing, itemId = 1, title = "Fixture Film", durationMs = 600_000))
        compose.setContent {
            PhoneTheme {
                PhonePlayerScreen(controller, Host, PictureState(), CaptionLook(), SubtitleChoice(), {}, inPip = false)
            }
        }
        compose.waitForIdle()
        compose.onNodeWithTag(TAG_PLAYER).assert(asResourceIds)
    }

    @Test
    fun `the live player's tags are resource ids`() {
        val live = FakeLiveTv()
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
        compose.setContent { PhoneTheme { LivePlayerScreen(vm, onBack = {}) } }
        compose.waitForIdle()
        compose.onNodeWithTag(LIVE_PLAYER).assert(asResourceIds)
    }
}
