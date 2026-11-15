package app.pley.android.ui.phone.livetv

import androidx.activity.ComponentActivity
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.ViewModelStore
import androidx.lifecycle.viewmodel.CreationExtras
import app.pley.android.common.TestTags
import app.pley.android.network.ApiException
import app.pley.android.player.live.LivePhase
import app.pley.android.player.live.LivePlayback
import app.pley.android.screens.livetv.ChannelHandoff
import app.pley.android.screens.livetv.LivePlayerDeps
import app.pley.android.screens.livetv.LivePlayerViewModel
import app.pley.android.testing.data.FakeLiveTv
import app.pley.android.testing.player.engine.FakePlayerEngine
import app.pley.android.ui.phone.R
import app.pley.android.ui.phone.theme.PhoneTheme
import kotlin.reflect.KClass
import kotlinx.coroutines.flow.flowOf
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

/** The phone's live player and picture in picture: its button, and what the small window shows. */
@RunWith(RobolectricTestRunner::class)
@Config(qualifiers = "w800dp-h360dp-xxhdpi")
class LivePlayerScreenTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    /** Holds the view models made, cleared after each test: their playbacks end with them. */
    private val store = ViewModelStore()
    private val live = FakeLiveTv()
    private val engines = mutableListOf<FakePlayerEngine>()

    @After
    fun clear() {
        store.clear()
        // What a test gave and no player took.
        ChannelHandoff.take(0)
    }

    /** The player on [channelId] of every channel, its view model held by [store]. */
    private fun player(channelId: Long = 1): LivePlayerViewModel {
        val deps = LivePlayerDeps(
            live = live,
            playback = { scope -> LivePlayback(live, { FakePlayerEngine().also { engines += it } }, { "android-1" }, scope) },
            origin = flowOf("http://nas:7539"),
        )
        val factory = object : ViewModelProvider.Factory {
            @Suppress("UNCHECKED_CAST")
            override fun <V : ViewModel> create(modelClass: KClass<V>, extras: CreationExtras): V =
                LivePlayerViewModel(deps, SavedStateHandle(), channelId, "all") as V
        }
        return ViewModelProvider.create(store, factory)[LivePlayerViewModel::class]
    }

    private fun show(vm: LivePlayerViewModel, inPip: Boolean = false, onPip: (() -> Unit)? = null) {
        compose.setContent {
            PhoneTheme {
                LivePlayerScreen(vm, onBack = {}, inPip = inPip, onPip = onPip)
            }
        }
        compose.waitForIdle()
    }

    /** The channel's picture is out. */
    private fun playing(vm: LivePlayerViewModel) {
        engines.last().player.ready()
        compose.waitForIdle()
        assertEquals(LivePhase.Playing, vm.state.value.phase)
    }

    @Test
    fun `the controls shrink the channel into picture in picture where it is allowed`() {
        var asked = 0
        show(player(), onPip = { asked++ })
        compose.onNodeWithTag(LIVE_PLAYER_PIP).performClick()
        assertEquals(1, asked)
    }

    @Test
    fun `without picture in picture the controls have no button for it`() {
        show(player())
        compose.onNodeWithTag(LIVE_PLAYER_CHANNELS).assertExists()
        compose.onNodeWithTag(LIVE_PLAYER_PIP).assertDoesNotExist()
    }

    @Test
    fun `picture in picture shows the picture alone`() {
        val vm = player()
        show(vm, inPip = true, onPip = {})
        playing(vm)
        compose.onNodeWithTag(TestTags.PLAYER_SURFACE).assertExists()
        compose.onNodeWithTag(LIVE_PLAYER_CHANNELS).assertDoesNotExist()
        compose.onNodeWithTag(LIVE_PLAYER_PIP).assertDoesNotExist()
        compose.onNodeWithTag(LIVE_PLAYER_LIVE).assertDoesNotExist()
    }

    @Test
    fun `picture in picture says why a channel doesn't play, with nothing to press`() {
        live.watchFails = ApiException(409, "conflict", "The one connection of Test Provider is in use.")
        show(player(), inPip = true, onPip = {})
        compose.onNodeWithTag(LIVE_PLAYER_TROUBLE).assertExists()
        compose.onNodeWithText(compose.activity.getString(R.string.live_player_busy)).assertExists()
        compose.onNodeWithText(compose.activity.getString(R.string.live_player_other_channels)).assertDoesNotExist()
        compose.onNodeWithText(compose.activity.getString(R.string.live_player_back_to_guide)).assertDoesNotExist()
    }
}
