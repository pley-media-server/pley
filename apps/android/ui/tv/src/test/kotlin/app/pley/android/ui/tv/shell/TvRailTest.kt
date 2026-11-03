package app.pley.android.ui.tv.shell

import androidx.compose.foundation.background
import androidx.compose.foundation.focusGroup
import androidx.compose.foundation.focusable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusProperties
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.toPixelMap
import androidx.compose.ui.input.InputMode
import androidx.compose.ui.input.InputModeManager
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.platform.LocalInputModeManager
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.assertCountEquals
import androidx.compose.ui.test.assertIsFocused
import androidx.compose.ui.test.captureToImage
import androidx.compose.ui.test.isFocused
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.getBoundsInRoot
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
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
import org.junit.Assert.assertEquals
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
 * place with reduced motion. Only Left goes into it, and a key pressed
 * with nothing focused brings focus back to the page.
 */
@OptIn(ExperimentalTestApi::class)
@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(qualifiers = "w960dp-h540dp-xhdpi")
class TvRailTest {
    @get:Rule
    val compose = createComposeRule()

    private val content = FocusRequester()
    private val nearFocus = FocusRequester()
    private val rail = RailState(content).apply {
        shown = true
        current = "home"
    }
    private lateinit var input: InputModeManager

    /**
     * The rail over a page, as the shell has them: [FAR] on the page (while
     * [far] can take focus), and [NEAR] above and left of it while [near].
     * Focus starts on the page.
     */
    private fun shell(reduced: Boolean, near: () -> Boolean = { false }, far: () -> Boolean = { true }) {
        compose.setContent {
            input = LocalInputModeManager.current
            TvTheme(reducedMotion = reduced) {
                Box(Modifier.fillMaxSize().background(Color.Black)) {
                    Box(
                        Modifier
                            .fillMaxSize()
                            .focusRequester(content)
                            .onFocusChanged { rail.contentFocused = it.hasFocus }
                            .railOnLeftStopOnRight(rail, stopRight = false)
                            .focusGroup(),
                    ) {
                        if (near()) Box(Modifier.offset(300.dp, 100.dp).size(40.dp).testTag(NEAR).focusRequester(nearFocus).focusable())
                        Box(Modifier.offset(600.dp, 300.dp).size(40.dp).testTag(FAR).focusProperties { canFocus = far() }.focusable())
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
    fun labelsSlideInAsTheRailOpens() {
        shell(reduced = false)
        compose.mainClock.autoAdvance = false
        pressNow(Key.DirectionLeft)
        compose.mainClock.advanceTimeByFrame()
        compose.mainClock.advanceTimeByFrame()
        // Search's, as Home's is focused and grows.
        val search = compose.onNodeWithText("Search", useUnmergedTree = true)
        val sliding = search.getBoundsInRoot().left
        compose.mainClock.advanceTimeBy(300)
        // In place 56 dp into the pill, 8 dp into the rail; on its way, short of it.
        val rest = search.getBoundsInRoot().left
        assertEquals(64f, rest.value, 0.5f)
        assertTrue("$sliding", sliding.value < rest.value - 1f)
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
    fun aKeyWithNothingFocusedBringsFocusBackToThePage() {
        var near by mutableStateOf(true)
        var far by mutableStateOf(true)
        shell(reduced = true, near = { near }, far = { far })
        // Right and Down from nowhere would find the rail's entries first, at the top left.
        for (key in listOf(Key.DirectionRight, Key.DirectionDown)) {
            near = true
            compose.waitForIdle()
            compose.runOnUiThread { nearFocus.requestFocus() }
            compose.waitForIdle()
            compose.onNodeWithTag(NEAR).assertIsFocused()
            // The focused element goes with a redraw of its page while nothing else there can take
            // focus, which then comes back: nothing has focus.
            far = false
            near = false
            compose.waitForIdle()
            far = true
            compose.waitForIdle()
            compose.onAllNodes(isFocused()).assertCountEquals(0)
            // The next key doesn't open the rail, nor is it lost: focus is back on the page.
            compose.press(key)
            compose.onNodeWithTag(FAR).assertIsFocused()
        }
    }

    @Test
    fun upFromThePageStaysThere() {
        shell(reduced = true)
        compose.onNodeWithTag(FAR).assertIsFocused()
        compose.press(Key.DirectionUp)
        compose.onNodeWithTag(FAR).assertIsFocused()
    }

    private companion object {
        const val NEAR = "near"
        const val FAR = "far"
    }
}
