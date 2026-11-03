package app.pley.android.ui.tv.shell

import androidx.compose.foundation.background
import androidx.compose.foundation.focusGroup
import androidx.compose.foundation.focusable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.size
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.toPixelMap
import androidx.compose.ui.input.InputMode
import androidx.compose.ui.input.InputModeManager
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.platform.LocalInputModeManager
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.assertIsFocused
import androidx.compose.ui.test.captureToImage
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onRoot
import androidx.compose.ui.test.performKeyInput
import androidx.compose.ui.test.pressKey
import androidx.compose.ui.unit.dp
import app.pley.android.common.TestTags
import app.pley.android.designsystem.PleyColors
import app.pley.android.ui.tv.kit.press
import app.pley.android.ui.tv.kit.railOnLeftStopOnRight
import app.pley.android.ui.tv.theme.TvTheme
import kotlin.math.abs
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

/**
 * The rail (design spec §2.1): Left from the page opens it over 200 ms, in
 * place with reduced motion, and only Left goes into it.
 */
@OptIn(ExperimentalTestApi::class)
@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(qualifiers = "w960dp-h540dp-xhdpi")
class TvRailTest {
    @get:Rule
    val compose = createComposeRule()

    private val content = FocusRequester()
    private val rail = RailState(content).apply {
        shown = true
        current = "home"
    }
    private lateinit var input: InputModeManager

    /** The rail over a page, as the shell has them, with [FAR] on the page. Focus starts on the page. */
    private fun shell(reduced: Boolean) {
        compose.setContent {
            input = LocalInputModeManager.current
            TvTheme(reducedMotion = reduced) {
                Box(Modifier.fillMaxSize().background(Color.Black)) {
                    Box(
                        Modifier
                            .fillMaxSize()
                            .focusRequester(content)
                            .railOnLeftStopOnRight(rail, stopRight = false)
                            .focusGroup(),
                    ) {
                        Box(Modifier.offset(600.dp, 300.dp).size(40.dp).testTag(FAR).focusable())
                    }
                    TvRail(railItems(emptyList()), RailProfile("Anna"), rail, onSelect = {}, onProfile = {})
                }
            }
        }
        compose.waitForIdle()
        // A TV isn't in touch mode, where the rail's entries (clickable, as tiles are) take no focus.
        compose.runOnUiThread {
            input.requestInputMode(InputMode.Keyboard)
            content.requestFocus()
        }
        compose.waitForIdle()
    }

    /** The colour at [xDp] across the rail's panel, halfway down, below its entries. */
    private fun panelAt(xDp: Int): Color {
        val image = compose.onRoot().captureToImage().toPixelMap()
        return image[xDp * 2, image.height / 2]
    }

    private fun isPanel(color: Color): Boolean {
        val panel = PleyColors.Surface
        return abs(color.red - panel.red) < 0.05f && abs(color.green - panel.green) < 0.05f && abs(color.blue - panel.blue) < 0.05f
    }

    private fun pressNow(key: Key) = compose.onRoot().performKeyInput { pressKey(key) }

    @Test
    fun leftOpensTheRailOverItsTime() {
        shell(reduced = false)
        compose.onNodeWithTag(FAR).assertIsFocused()
        compose.mainClock.autoAdvance = false
        pressNow(Key.DirectionLeft)
        compose.mainClock.advanceTimeByFrame()
        compose.mainClock.advanceTimeByFrame()
        // Two frames in, the panel is still narrow: 250 dp across is the page.
        assertFalse(isPanel(panelAt(250)))
        compose.mainClock.advanceTimeBy(300)
        assertTrue(isPanel(panelAt(250)))
        compose.onNodeWithTag("${TestTags.TV_RAIL}.home").assertIsFocused()
    }

    @Test
    fun withReducedMotionTheRailOpensAndClosesInPlace() {
        shell(reduced = true)
        compose.mainClock.autoAdvance = false
        pressNow(Key.DirectionLeft)
        compose.mainClock.advanceTimeByFrame()
        compose.mainClock.advanceTimeByFrame()
        assertTrue(isPanel(panelAt(250)))
        compose.onNodeWithTag("${TestTags.TV_RAIL}.home").assertIsFocused()
        // Right goes back to the page, and the panel is gone with it.
        pressNow(Key.DirectionRight)
        compose.mainClock.advanceTimeByFrame()
        compose.mainClock.advanceTimeByFrame()
        assertFalse(isPanel(panelAt(250)))
        compose.onNodeWithTag(FAR).assertIsFocused()
    }

    @Test
    fun upFromThePageStaysThere() {
        shell(reduced = true)
        compose.onNodeWithTag(FAR).assertIsFocused()
        compose.press(Key.DirectionUp)
        compose.onNodeWithTag(FAR).assertIsFocused()
    }

    private companion object {
        const val FAR = "far"
    }
}
