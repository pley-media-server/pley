package app.pley.android.ui.tv.livetv

import android.view.KeyEvent as AndroidKeyEvent
import androidx.activity.ComponentActivity
import androidx.compose.ui.input.key.KeyEvent
import androidx.compose.ui.test.assertIsFocused
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.onRoot
import androidx.compose.ui.test.performKeyPress
import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.ViewModelStore
import androidx.lifecycle.viewmodel.CreationExtras
import app.pley.android.model.LiveChannel
import app.pley.android.model.LivePlan
import app.pley.android.player.live.LivePhase
import app.pley.android.player.live.LivePlayback
import app.pley.android.screens.livetv.ChannelHandoff
import app.pley.android.screens.livetv.LivePlayerDeps
import app.pley.android.screens.livetv.LivePlayerViewModel
import app.pley.android.testing.data.FakeLiveTv
import app.pley.android.testing.player.engine.FakePlayerEngine
import app.pley.android.ui.tv.R
import app.pley.android.ui.tv.player.TAG_DETAILS
import app.pley.android.ui.tv.theme.TvTheme
import kotlin.reflect.KClass
import kotlinx.coroutines.flow.flowOf
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

/** The TV's live player with the remote's keys: OK and OK held, what's over the picture, Playback details in their panel and over the picture. */
@RunWith(RobolectricTestRunner::class)
@Config(qualifiers = "w960dp-h540dp-xhdpi")
class TvLivePlayerScreenTest {
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

    private fun show(vm: LivePlayerViewModel, showDetails: Boolean = false) {
        compose.setContent {
            TvTheme {
                TvLivePlayerScreen(vm, onExit = {}, showDetails = showDetails)
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

    /** Sends one native key event, as a remote's held key does (repeats included). */
    private fun send(action: Int, code: Int, repeat: Int = 0) {
        compose.onRoot().performKeyPress(KeyEvent(AndroidKeyEvent(0L, 0L, action, code, repeat)))
        compose.waitForIdle()
    }

    /** The controls' Channels button has focus. */
    private fun channelsFocused() = compose.onNodeWithContentDescription(compose.activity.getString(R.string.tv_live_player_channels)).assertIsFocused()

    private fun ok() {
        send(AndroidKeyEvent.ACTION_DOWN, AndroidKeyEvent.KEYCODE_DPAD_CENTER)
        send(AndroidKeyEvent.ACTION_UP, AndroidKeyEvent.KEYCODE_DPAD_CENTER)
    }

    @Test
    fun `OK opens the channel list`() {
        val vm = player()
        show(vm)
        playing(vm)
        ok()
        compose.onNodeWithTag(TV_LIVE_PLAYER_CHANNELS).assertExists()
        compose.onNodeWithTag(TV_LIVE_PLAYER_CHANNEL_LIST).assertDoesNotExist()
    }

    @Test
    fun `OK held shows the controls, and the rest of that press doesn't press them`() {
        val vm = player()
        show(vm)
        playing(vm)
        send(AndroidKeyEvent.ACTION_DOWN, AndroidKeyEvent.KEYCODE_DPAD_CENTER)
        send(AndroidKeyEvent.ACTION_DOWN, AndroidKeyEvent.KEYCODE_DPAD_CENTER, repeat = 1)
        channelsFocused()
        send(AndroidKeyEvent.ACTION_DOWN, AndroidKeyEvent.KEYCODE_DPAD_CENTER, repeat = 2)
        send(AndroidKeyEvent.ACTION_UP, AndroidKeyEvent.KEYCODE_DPAD_CENTER)
        // Let go, the controls stay, Channels focused, the list not opened.
        channelsFocused()
        compose.onNodeWithTag(TV_LIVE_PLAYER_CHANNELS).assertDoesNotExist()
        // A press of its own presses Channels.
        ok()
        compose.onNodeWithTag(TV_LIVE_PLAYER_CHANNELS).assertExists()
    }

    @Test
    fun `Info, last in the controls, opens Playback details, which Up and Down scroll and Back closes`() {
        live.plan = LivePlan(
            videoCodec = "hevc", width = 1920, height = 1080, frameRate = 50.0, bitDepth = 10,
            audioCodec = "aac", audioChannels = 2, video = "copy", audio = "copy", kbps = 6_500,
        )
        val vm = player()
        show(vm)
        playing(vm)
        send(AndroidKeyEvent.ACTION_DOWN, AndroidKeyEvent.KEYCODE_DPAD_CENTER)
        send(AndroidKeyEvent.ACTION_DOWN, AndroidKeyEvent.KEYCODE_DPAD_CENTER, repeat = 1)
        send(AndroidKeyEvent.ACTION_UP, AndroidKeyEvent.KEYCODE_DPAD_CENTER)
        channelsFocused()
        // How it plays, in the video player's badge.
        compose.onNodeWithText("Direct play").assertExists()
        // Right twice: past the favorite, to Info.
        repeat(2) {
            send(AndroidKeyEvent.ACTION_DOWN, AndroidKeyEvent.KEYCODE_DPAD_RIGHT)
            send(AndroidKeyEvent.ACTION_UP, AndroidKeyEvent.KEYCODE_DPAD_RIGHT)
        }
        compose.onNodeWithContentDescription(compose.activity.getString(R.string.tv_player_info)).assertIsFocused()
        compose.onNodeWithTag(TV_LIVE_PLAYER_DETAILS).assertDoesNotExist()
        ok()
        compose.onNodeWithTag(TV_LIVE_PLAYER_DETAILS).assertIsFocused()
        // The video player's words and rows, under its title.
        compose.onNodeWithText(compose.activity.getString(R.string.tv_player_info_title)).assertExists()
        compose.onNodeWithText("Direct play").assertExists()
        compose.onNodeWithText("HEVC 10-bit · 1920×1080 · 50 fps").assertExists()
        compose.onNodeWithText("6.5 Mbps").assertExists()
        compose.onNodeWithTag(TV_LIVE_PLAYER_INFO).assertDoesNotExist()
        assertEquals(listOf("live-1", "live-1"), live.plansAsked)

        // Up and Down are the panel's: the channel stays.
        send(AndroidKeyEvent.ACTION_DOWN, AndroidKeyEvent.KEYCODE_DPAD_DOWN)
        send(AndroidKeyEvent.ACTION_UP, AndroidKeyEvent.KEYCODE_DPAD_DOWN)
        send(AndroidKeyEvent.ACTION_DOWN, AndroidKeyEvent.KEYCODE_DPAD_UP)
        send(AndroidKeyEvent.ACTION_UP, AndroidKeyEvent.KEYCODE_DPAD_UP)
        assertEquals("watch 1 android-1", live.sent.last { it.startsWith("watch") })
        compose.onNodeWithTag(TV_LIVE_PLAYER_DETAILS).assertExists()

        compose.runOnUiThread { compose.activity.onBackPressedDispatcher.onBackPressed() }
        compose.waitForIdle()
        compose.onNodeWithTag(TV_LIVE_PLAYER_DETAILS).assertDoesNotExist()
    }

    @Test
    fun `Playback details show over the channel while nothing else does, as over a video, asking the server`() {
        live.plan = LivePlan(
            videoCodec = "hevc", width = 1920, height = 1080, frameRate = 50.0, bitDepth = 10,
            audioCodec = "aac", audioChannels = 2, video = "copy", audio = "copy", kbps = 6_500,
        )
        val vm = player()
        show(vm, showDetails = true)
        // Nothing to say of a channel that tunes.
        compose.onNodeWithTag(TAG_DETAILS).assertDoesNotExist()
        playing(vm)
        // The video player's readout: the way, then the rows of the channel's details.
        compose.onNodeWithTag(TAG_DETAILS).assertExists()
        compose.onNodeWithText("Direct play").assertExists()
        compose.onNodeWithText("HEVC 10-bit · 1920×1080 · 50 fps").assertExists()
        compose.onNodeWithText("AAC 2.0").assertExists()
        compose.onNodeWithText("6.5 Mbps").assertExists()
        // Asked of the session watched, besides the tune's own asking.
        assertEquals(listOf("live-1", "live-1"), live.plansAsked)

        // The controls take its place, and it comes back when they go.
        send(AndroidKeyEvent.ACTION_DOWN, AndroidKeyEvent.KEYCODE_DPAD_CENTER)
        send(AndroidKeyEvent.ACTION_DOWN, AndroidKeyEvent.KEYCODE_DPAD_CENTER, repeat = 1)
        send(AndroidKeyEvent.ACTION_UP, AndroidKeyEvent.KEYCODE_DPAD_CENTER)
        channelsFocused()
        compose.onNodeWithTag(TAG_DETAILS).assertDoesNotExist()
        compose.runOnUiThread { compose.activity.onBackPressedDispatcher.onBackPressed() }
        compose.waitForIdle()
        compose.onNodeWithTag(TAG_DETAILS).assertExists()
        // The channel list does too.
        ok()
        compose.onNodeWithTag(TV_LIVE_PLAYER_CHANNELS).assertExists()
        compose.onNodeWithTag(TAG_DETAILS).assertDoesNotExist()
    }

    @Test
    fun `without the setting nothing shows over the channel, and the server isn't asked`() {
        live.plan = LivePlan(videoCodec = "h264", width = 1280, height = 720, audioCodec = "aac", audioChannels = 2, video = "copy", audio = "copy")
        val vm = player()
        show(vm)
        playing(vm)
        compose.onNodeWithTag(TAG_DETAILS).assertDoesNotExist()
        // The tune's own asking, alone.
        assertEquals(listOf("live-1"), live.plansAsked)
    }

    @Test
    fun `the tuning card shows the channel as the page had it, at once`() {
        // A channel the server doesn't answer for yet.
        ChannelHandoff.give(LiveChannel(77, name = "Seventy-seven", number = "77"))
        show(player(77))
        compose.onNodeWithText("77  Seventy-seven").assertExists()
        compose.onNodeWithText(compose.activity.getString(R.string.tv_live_player_tuning)).assertExists()
    }

    @Test
    fun `Next and Previous change channel, as channel up and down do`() {
        show(player(1))
        send(AndroidKeyEvent.ACTION_DOWN, AndroidKeyEvent.KEYCODE_MEDIA_NEXT)
        send(AndroidKeyEvent.ACTION_UP, AndroidKeyEvent.KEYCODE_MEDIA_NEXT)
        assertEquals("watch 2 android-1", live.sent.last { it.startsWith("watch") })
        send(AndroidKeyEvent.ACTION_DOWN, AndroidKeyEvent.KEYCODE_MEDIA_PREVIOUS)
        send(AndroidKeyEvent.ACTION_UP, AndroidKeyEvent.KEYCODE_MEDIA_PREVIOUS)
        send(AndroidKeyEvent.ACTION_DOWN, AndroidKeyEvent.KEYCODE_MEDIA_PREVIOUS)
        send(AndroidKeyEvent.ACTION_UP, AndroidKeyEvent.KEYCODE_MEDIA_PREVIOUS)
        // Round the start of every channel: the last.
        assertEquals("watch 5 android-1", live.sent.last { it.startsWith("watch") })
    }
}
