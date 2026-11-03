package app.pley.android.ui.tv.livetv

import androidx.activity.ComponentActivity
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.assertIsFocused
import androidx.compose.ui.test.assertIsNotFocused
import androidx.compose.ui.test.hasSetTextAction
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.onRoot
import androidx.compose.ui.test.performKeyInput
import androidx.compose.ui.test.performTextReplacement
import androidx.compose.ui.test.pressKey
import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.ViewModelStore
import androidx.lifecycle.viewmodel.CreationExtras
import app.pley.android.player.PlayRequest
import app.pley.android.screens.PlayerLauncher
import app.pley.android.screens.livetv.LiveTvDeps
import app.pley.android.screens.livetv.LiveTvViewModel
import app.pley.android.screens.livetv.LiveView
import app.pley.android.testing.data.FakeLiveTv
import app.pley.android.ui.tv.R
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

/** Live TV's page on TV with the remote: Back a step at a time, a search left. */
@RunWith(RobolectricTestRunner::class)
@Config(qualifiers = "w960dp-h540dp-xhdpi")
@OptIn(ExperimentalTestApi::class)
class TvLiveTvScreenTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    private val store = ViewModelStore()
    private val live = FakeLiveTv()
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
                TvLiveTvScreen(vm)
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
    fun `Back leaves a search for the category before`() {
        show()
        compose.onNode(hasSetTextAction()).performTextReplacement("sport")
        compose.waitForIdle()
        assertEquals("sport", vm.state.value.query)
        back()
        assertEquals("", vm.state.value.query)
    }
}
