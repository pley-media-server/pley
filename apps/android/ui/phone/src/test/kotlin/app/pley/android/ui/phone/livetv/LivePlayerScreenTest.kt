package app.pley.android.ui.phone.livetv

import androidx.activity.ComponentActivity
import androidx.compose.ui.test.assertContentDescriptionEquals
import androidx.compose.ui.test.assertCountEquals
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.ViewModelStore
import androidx.lifecycle.viewmodel.CreationExtras
import app.pley.android.common.TestTags
import app.pley.android.model.LivePlan
import app.pley.android.network.ApiException
import app.pley.android.player.live.LivePhase
import app.pley.android.player.live.LivePlayback
import app.pley.android.screens.livetv.ChannelHandoff
import app.pley.android.screens.livetv.LivePlayerDeps
import app.pley.android.screens.livetv.LivePlayerViewModel
import app.pley.android.testing.data.FakeLiveTv
import app.pley.android.testing.player.engine.FakePlayerEngine
import app.pley.android.ui.phone.R
import app.pley.android.ui.phone.player.TAG_READOUT
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

/** The phone's live player: Playback details in their sheet and over the picture, and picture in picture (its button, what the small window shows). */
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

    private fun show(vm: LivePlayerViewModel, inPip: Boolean = false, onPip: (() -> Unit)? = null, showDetails: Boolean = false) {
        compose.setContent {
            PhoneTheme {
                LivePlayerScreen(vm, onBack = {}, inPip = inPip, onPip = onPip, showDetails = showDetails)
            }
        }
        compose.waitForIdle()
    }

    /** A tap on the picture: it hides the controls, or brings them back. */
    private fun tapPicture() {
        compose.onNodeWithTag(LIVE_PLAYER).performClick()
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
    fun `the controls badge how the channel plays, as the video player's, and the badge opens Playback details`() {
        live.plan = LivePlan(
            videoCodec = "hevc", width = 1920, height = 1080, frameRate = 50.0, bitDepth = 10,
            audioCodec = "aac", audioChannels = 2, video = "copy", audio = "copy", kbps = 6_500,
        )
        val vm = player()
        show(vm)
        // Nothing to badge while the channel tunes.
        compose.onNodeWithTag(LIVE_PLAYER_DETAILS).assertDoesNotExist()
        playing(vm)
        compose.onNodeWithTag(LIVE_PLAYER_DETAILS_SHEET, useUnmergedTree = true).assertDoesNotExist()
        compose.onNodeWithTag(LIVE_PLAYER_DETAILS).assertContentDescriptionEquals(compose.activity.getString(R.string.player_method_hint, "Direct play"))
        compose.onNodeWithTag(LIVE_PLAYER_DETAILS).performClick()
        compose.waitForIdle()
        // Read unmerged: the sheet takes the touches inside it, which makes one node of its words.
        compose.onNodeWithTag(LIVE_PLAYER_DETAILS_SHEET, useUnmergedTree = true).assertExists()
        // The video player's words and rows.
        compose.onNodeWithText(compose.activity.getString(R.string.player_details), useUnmergedTree = true).assertExists()
        // In the badge and in the sheet.
        compose.onAllNodesWithText("Direct play", useUnmergedTree = true).assertCountEquals(2)
        compose.onNodeWithText("HEVC 10-bit · 1920×1080 · 50 fps", useUnmergedTree = true).assertExists()
        compose.onNodeWithText("AAC 2.0", useUnmergedTree = true).assertExists()
        compose.onNodeWithText("6.5 Mbps", useUnmergedTree = true).assertExists()
        // Asked of the session watched, besides the tune's own asking.
        assertEquals(listOf("live-1", "live-1"), live.plansAsked)

        compose.onNodeWithContentDescription(compose.activity.getString(R.string.player_close)).performClick()
        compose.waitForIdle()
        compose.onNodeWithTag(LIVE_PLAYER_DETAILS_SHEET, useUnmergedTree = true).assertDoesNotExist()
    }

    @Test
    fun `Playback details show over the channel while the controls are hidden, as over a video, asking the server`() {
        live.plan = LivePlan(
            videoCodec = "hevc", width = 1920, height = 1080, frameRate = 50.0, bitDepth = 10,
            audioCodec = "aac", audioChannels = 2, video = "copy", audio = "copy", kbps = 6_500,
        )
        val vm = player()
        show(vm, showDetails = true)
        playing(vm)
        // The controls show as the player opens: the badge says it, and nothing more is asked.
        compose.onNodeWithTag(TAG_READOUT, useUnmergedTree = true).assertDoesNotExist()
        assertEquals(listOf("live-1"), live.plansAsked)
        tapPicture()
        compose.onNodeWithTag(LIVE_PLAYER_DETAILS).assertDoesNotExist()
        // The video player's readout: the way, then the rows of the channel's details.
        compose.onNodeWithTag(TAG_READOUT, useUnmergedTree = true).assertExists()
        compose.onNodeWithText("Direct play", useUnmergedTree = true).assertExists()
        compose.onNodeWithText("HEVC 10-bit · 1920×1080 · 50 fps", useUnmergedTree = true).assertExists()
        compose.onNodeWithText("AAC 2.0", useUnmergedTree = true).assertExists()
        compose.onNodeWithText("6.5 Mbps", useUnmergedTree = true).assertExists()
        assertEquals(listOf("live-1", "live-1"), live.plansAsked)
        // The controls take its place.
        tapPicture()
        compose.onNodeWithTag(LIVE_PLAYER_DETAILS).assertExists()
        compose.onNodeWithTag(TAG_READOUT, useUnmergedTree = true).assertDoesNotExist()
    }

    @Test
    fun `without the setting nothing shows over the channel, and the server isn't asked`() {
        live.plan = LivePlan(videoCodec = "h264", width = 1280, height = 720, audioCodec = "aac", audioChannels = 2, video = "copy", audio = "copy")
        val vm = player()
        show(vm)
        playing(vm)
        tapPicture()
        compose.onNodeWithTag(LIVE_PLAYER_DETAILS).assertDoesNotExist()
        compose.onNodeWithTag(TAG_READOUT, useUnmergedTree = true).assertDoesNotExist()
        // The tune's own asking, alone.
        assertEquals(listOf("live-1"), live.plansAsked)
    }

    @Test
    fun `picture in picture shows no Playback details over the picture`() {
        live.plan = LivePlan(videoCodec = "h264", width = 1280, height = 720, audioCodec = "aac", audioChannels = 2, video = "copy", audio = "copy")
        val vm = player()
        show(vm, inPip = true, onPip = {}, showDetails = true)
        playing(vm)
        compose.onNodeWithTag(TAG_READOUT, useUnmergedTree = true).assertDoesNotExist()
        assertEquals(listOf("live-1"), live.plansAsked)
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
