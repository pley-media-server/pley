package app.pley.android.ui.tv.livetv

import android.view.KeyEvent as AndroidKeyEvent
import androidx.activity.ComponentActivity
import androidx.compose.foundation.focusable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.input.key.KeyEvent
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.test.assertIsFocused
import androidx.compose.ui.test.assertIsNotFocused
import androidx.compose.ui.test.getUnclippedBoundsInRoot
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onRoot
import androidx.compose.ui.test.performKeyPress
import androidx.compose.ui.unit.dp
import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.ViewModelStore
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.CreationExtras
import app.pley.android.data.LiveTvRepository
import app.pley.android.data.Loadable
import app.pley.android.model.LiveCategory
import app.pley.android.model.LiveGuide
import app.pley.android.model.LiveProgram
import app.pley.android.player.PlayRequest
import app.pley.android.screens.PlayerLauncher
import app.pley.android.screens.livetv.LiveTvDeps
import app.pley.android.screens.livetv.LiveTvViewModel
import app.pley.android.testing.data.FakeLiveTv
import app.pley.android.ui.tv.kit.KeyRepeatGate
import app.pley.android.ui.tv.kit.LocalKeyRepeatGate
import app.pley.android.ui.tv.kit.keyRepeatGate
import app.pley.android.ui.tv.theme.TvTheme
import kotlin.reflect.KClass
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.flowOf
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

/** The TV guide with the remote's keys: its edges, under a held key and pressed. */
@RunWith(RobolectricTestRunner::class)
@Config(qualifiers = "w960dp-h540dp-xhdpi")
class TvGuideTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    private val store = ViewModelStore()
    private val live = FakeLiveTv()

    /** The keys' uptime, which the gate reads too. */
    private var uptime = 100_000L
    private val gate = KeyRepeatGate { uptime }
    private var leaves = 0
    private lateinit var vm: LiveTvViewModel

    @After
    fun clear() = store.clear()

    private fun show() {
        val deps = LiveTvDeps(
            live = live,
            launcher = object : PlayerLauncher {
                override fun play(request: PlayRequest) = Unit
            },
            origin = flowOf("http://nas:7539"),
            compute = Dispatchers.Main,
        )
        val factory = object : ViewModelProvider.Factory {
            @Suppress("UNCHECKED_CAST")
            override fun <V : ViewModel> create(modelClass: KClass<V>, extras: CreationExtras): V = LiveTvViewModel(deps, SavedStateHandle()) as V
        }
        vm = ViewModelProvider.create(store, factory)[LiveTvViewModel::class]
        compose.setContent {
            TvTheme {
                CompositionLocalProvider(LocalKeyRepeatGate provides gate) {
                    Column(Modifier.fillMaxSize().keyRepeatGate(gate)) {
                        // Where Up from the guide's first row goes: the tabs, on the page.
                        Box(Modifier.size(40.dp).testTag(ABOVE).focusable())
                        val pages by vm.channels.collectAsStateWithLifecycle()
                        val now by vm.now.collectAsStateWithLifecycle()
                        if (pages.total != null) {
                            TvGuide(vm, pages, LiveCategory.All, "", now, onWatch = {}, onDetails = { _, _ -> }, onLeave = { leaves++ })
                        }
                    }
                }
            }
        }
        compose.waitForIdle()
    }

    /** [code] down ([repeat] of a held key) at [at] ms, then up unless held. */
    private fun key(code: Int, at: Long, repeat: Int = 0, up: Boolean = true) {
        uptime = at
        compose.onRoot().performKeyPress(KeyEvent(AndroidKeyEvent(at, at, AndroidKeyEvent.ACTION_DOWN, code, repeat)))
        compose.waitForIdle()
        if (up) {
            compose.onRoot().performKeyPress(KeyEvent(AndroidKeyEvent(at, at, AndroidKeyEvent.ACTION_UP, code, 0)))
            compose.waitForIdle()
        }
    }

    @Test
    fun `Left leaves for the categories on a press of its own, not straight after moving`() {
        show()
        // Back to the guide's first half hour (the channels have no programmes).
        key(AndroidKeyEvent.KEYCODE_DPAD_LEFT, at = 100_000)
        assertEquals(0, leaves)
        // At the start, a tap straight after: it stays.
        key(AndroidKeyEvent.KEYCODE_DPAD_LEFT, at = 100_000 + EDGE_PAUSE_MS / 2)
        assertEquals(0, leaves)
        key(AndroidKeyEvent.KEYCODE_DPAD_LEFT, at = 100_000 + EDGE_PAUSE_MS + 1)
        assertEquals(1, leaves)
    }

    @Test
    fun `a held Left stops at the guide's start`() {
        show()
        key(AndroidKeyEvent.KEYCODE_DPAD_LEFT, at = 100_000, up = false)
        for (i in 1..10) key(AndroidKeyEvent.KEYCODE_DPAD_LEFT, at = 100_400 + i * 100L, repeat = i, up = false)
        assertEquals(0, leaves)
        compose.onRoot().performKeyPress(KeyEvent(AndroidKeyEvent(uptime, uptime, AndroidKeyEvent.ACTION_UP, AndroidKeyEvent.KEYCODE_DPAD_LEFT, 0)))
        compose.waitForIdle()
        // Let go, a press of its own leaves.
        key(AndroidKeyEvent.KEYCODE_DPAD_LEFT, at = uptime + EDGE_PAUSE_MS + 1)
        assertEquals(1, leaves)
    }

    @Test
    fun `a held Up stops at the first row, a press goes on above`() {
        show()
        key(AndroidKeyEvent.KEYCODE_DPAD_DOWN, at = 100_000)
        key(AndroidKeyEvent.KEYCODE_DPAD_UP, at = 100_200, up = false)
        for (i in 1..5) key(AndroidKeyEvent.KEYCODE_DPAD_UP, at = 100_600 + i * 100L, repeat = i, up = false)
        compose.onNodeWithTag(ABOVE).assertIsNotFocused()
        compose.onRoot().performKeyPress(KeyEvent(AndroidKeyEvent(uptime, uptime, AndroidKeyEvent.ACTION_UP, AndroidKeyEvent.KEYCODE_DPAD_UP, 0)))
        compose.waitForIdle()
        key(AndroidKeyEvent.KEYCODE_DPAD_UP, at = uptime + 1_000)
        compose.onNodeWithTag(ABOVE).assertIsFocused()
    }

    @Test
    fun `programmes sit at their time on the grid, and slide with it`() {
        show()
        val start = vm.guideStart
        // Now is in the guide's second half hour: the first programme is on, the second next.
        val on = LiveProgram(1, start, start + 90 * 60, "Morning Briefing")
        val next = LiveProgram(1, start + 90 * 60, start + 150 * 60, "World at One")
        for (chunk in live.guides.keys) {
            live.guideChunk(chunk).value = Loadable.Ready(LiveGuide(chunk, chunk + LiveTvRepository.GUIDE_CHUNK_S, mapOf(1L to listOf(on, next))), fresh = true)
        }
        compose.waitForIdle()
        val area = compose.onNodeWithTag(TV_LIVE_GUIDE).getUnclippedBoundsInRoot().left + CHANNEL
        fun leftOf(title: String) = compose.onNodeWithContentDescription(title, substring = true).getUnclippedBoundsInRoot().left - area
        // 3.6 dp a minute, each cell inset 2 dp.
        assertEquals(2f, leftOf("Morning Briefing").value, 0.5f)
        assertEquals(90 * 3.6f + 2f, leftOf("World at One").value, 0.5f)
        // Right to the next programme, near the grid's edge: the grid moves on an hour.
        key(AndroidKeyEvent.KEYCODE_DPAD_RIGHT, at = 100_000)
        assertEquals(30 * 3.6f + 2f, leftOf("World at One").value, 0.5f)
        assertEquals(-60 * 3.6f + 2f, leftOf("Morning Briefing").value, 0.5f)
    }

    private companion object {
        const val ABOVE = "above"

        /** The channels' column, left of the programmes. */
        val CHANNEL = 160.dp
    }
}
