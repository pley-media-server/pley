package app.pley.android.ui.tv.livetv

import androidx.activity.ComponentActivity
import androidx.compose.foundation.focusable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Modifier
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.assert
import androidx.compose.ui.test.assertIsFocused
import androidx.compose.ui.test.assertIsNotFocused
import androidx.compose.ui.test.getUnclippedBoundsInRoot
import androidx.compose.ui.test.hasAnyAncestor
import androidx.compose.ui.test.hasSetTextAction
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.isFocused
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.onRoot
import androidx.compose.ui.test.performKeyInput
import androidx.compose.ui.test.performTextReplacement
import androidx.compose.ui.test.pressKey
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.height
import androidx.compose.ui.unit.width
import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.ViewModelStore
import androidx.lifecycle.viewmodel.CreationExtras
import app.pley.android.designsystem.TvSafeArea
import app.pley.android.model.LiveChannel
import app.pley.android.model.LiveProgram
import app.pley.android.player.PlayRequest
import app.pley.android.screens.PlayerLauncher
import app.pley.android.screens.livetv.LiveTvDeps
import app.pley.android.screens.livetv.LiveTvViewModel
import app.pley.android.screens.livetv.LiveView
import app.pley.android.testing.data.FakeLiveTv
import app.pley.android.ui.tv.R
import app.pley.android.ui.tv.kit.KeyRepeatGate
import app.pley.android.ui.tv.kit.LocalKeyRepeatGate
import app.pley.android.ui.tv.kit.hold
import app.pley.android.ui.tv.kit.keyRepeatGate
import app.pley.android.ui.tv.theme.TvTheme
import kotlin.reflect.KClass
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.flowOf
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

/** Live TV's page on TV with the remote: Back a step at a time, a search left, a programme's details, a held key at a row's end. */
@RunWith(RobolectricTestRunner::class)
@Config(qualifiers = "w960dp-h540dp-xhdpi")
@OptIn(ExperimentalTestApi::class)
class TvLiveTvScreenTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    private val store = ViewModelStore()
    private val live = FakeLiveTv()
    private lateinit var vm: LiveTvViewModel

    /** On the test's clock. The test sends no repeats, as the system would: none is waited for. */
    private val gate = KeyRepeatGate(firstRepeatMs = 60_000) { compose.mainClock.currentTime }

    @After
    fun clear() = store.clear()

    private fun show(clock: () -> Long = System::currentTimeMillis, beside: Boolean = false) {
        val deps = LiveTvDeps(
            live = live,
            launcher = object : PlayerLauncher {
                override fun play(request: PlayRequest) = Unit
            },
            origin = flowOf("http://nas:7539"),
            clock = clock,
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
                    Row(Modifier.fillMaxSize().keyRepeatGate(gate)) {
                        Box(Modifier.weight(1f)) { TvLiveTvScreen(vm) }
                        // Something to go on to beside the page, when asked for.
                        if (beside) Box(Modifier.size(40.dp).testTag(BESIDE).focusable())
                    }
                }
            }
        }
        compose.waitForIdle()
    }

    private fun press(key: Key) {
        compose.onRoot().performKeyInput { pressKey(key) }
        compose.waitForIdle()
    }

    private fun back() {
        compose.runOnUiThread { compose.activity.onBackPressedDispatcher.onBackPressed() }
        compose.waitForIdle()
    }

    private fun string(id: Int) = compose.activity.getString(id)

    @Test
    fun `Back from the guide brings it back to now first, then goes to the category shown`() {
        show()
        compose.onNodeWithTag(TV_LIVE_GUIDE).assertExists()
        // Away from now: on a half hour later (the channels have no guide).
        press(Key.DirectionRight)
        back()
        // Back to now, still in the guide: the categories don't have focus yet.
        compose.onNodeWithText(string(R.string.tv_livetv_category_all)).assertIsNotFocused()
        back()
        compose.onNodeWithText(string(R.string.tv_livetv_category_all)).assertIsFocused()
    }

    @Test
    fun `Info on a channel's tile opens its details`() {
        show()
        vm.setView(LiveView.Channels)
        compose.waitForIdle()
        compose.onNodeWithTag(TV_LIVE_TILES).assertExists()
        press(Key.Info)
        compose.onNodeWithTag(TV_LIVE_PROGRAM).assertExists()
    }

    @Test
    fun `a held Right stops on a row's last channel, and only a press goes on beside`() {
        show(beside = true)
        vm.setView(LiveView.Channels)
        compose.waitForIdle()
        // Three channels, one row: the press moves to the second; held from 400 ms, Right goes to the third (105) and
        // no further.
        compose.hold(Key.DirectionRight)
        compose.onNode(isFocused() and hasAnyAncestor(hasTestTag(TV_LIVE_TILES))).assert(hasText("105"))
        compose.onNodeWithTag(BESIDE).assertIsNotFocused()
        press(Key.DirectionRight)
        compose.onNodeWithTag(BESIDE).assertIsFocused()
    }

    @Test
    fun `a programme's details keep Watch and the star side by side under all the text`() {
        val start = 1_800_000_000L
        live.lineup.value = listOf(
            LiveChannel(
                id = 7,
                name = "Pley Cinema",
                number = "107",
                hasGuide = true,
                now = LiveProgram(
                    channelId = 7,
                    start = start,
                    stop = start + 7_200,
                    title = "A film whose title runs on well past the two lines the panel gives it",
                    subtitle = "The director's cut",
                    description = "What the guide says of it, at length. ".repeat(40),
                    image = "/api/live/channels/7/programs/$start/image",
                    season = 2,
                    episode = 14,
                    isNew = true,
                    isLive = true,
                    rating = "PG-13",
                ),
            ),
        )
        show(clock = { (start + 600) * 1000 })
        vm.setView(LiveView.Channels)
        compose.waitForIdle()
        press(Key.Info)
        val watch = compose.onNodeWithText(string(R.string.tv_livetv_watch_now)).getUnclippedBoundsInRoot()
        val star = compose.onNodeWithContentDescription(string(R.string.tv_livetv_favorite_add)).getUnclippedBoundsInRoot()
        val text = compose.onNodeWithTag(TV_LIVE_PROGRAM_TEXT).getUnclippedBoundsInRoot()
        val root = compose.onRoot().getUnclippedBoundsInRoot()
        assertEquals(40f, watch.height.value, 0.5f)
        assertEquals(40f, star.width.value, 0.5f)
        assertEquals(40f, star.height.value, 0.5f)
        assertEquals("the star beside Watch", watch.top.value, star.top.value, 0.5f)
        assertTrue("the star after Watch", star.left >= watch.right)
        assertTrue("the text above the buttons", text.bottom <= watch.top)
        assertTrue("in the panel", star.right <= root.right - TvSafeArea.Horizontal && star.bottom <= root.bottom - TvSafeArea.Vertical)
        // The star alone, focused too: its words are only heard.
        press(Key.DirectionRight)
        compose.onNodeWithContentDescription(string(R.string.tv_livetv_favorite_add)).assertIsFocused()
        compose.onNodeWithText(string(R.string.tv_livetv_favorite_add)).assertDoesNotExist()
    }

    @Test
    fun `Back leaves a search for the category before`() {
        show()
        compose.onNode(hasSetTextAction()).performTextReplacement("sport")
        compose.waitForIdle()
        assertEquals("sport", vm.state.value.query)
        back()
        assertEquals("", vm.state.value.query)
    }

    private companion object {
        const val BESIDE = "beside"
    }
}
