package app.pley.android.ui.phone.player

import androidx.compose.ui.semantics.SemanticsActions
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.hasClickAction
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.text.TextLayoutResult
import app.pley.android.ui.phone.kit.ConfirmDialog
import app.pley.android.ui.phone.theme.PhoneTheme
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

/**
 * A dialog a page opens on a car's screen is car-sized as the page is,
 * though its window starts again from the screen's own density: 64 dp
 * buttons and no text under 24 sp, in the screen's own units.
 */
@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(qualifiers = CarScreenChecks.SMALL)
class CarWindowsTest {
    @get:Rule
    val compose = createComposeRule()

    private fun ask(car: Boolean) {
        compose.setContent {
            PhoneTheme {
                CarSized(car) {
                    ConfirmDialog(TITLE, BODY, CONFIRM, onConfirm = {}, onDismiss = {}, destructive = true)
                }
            }
        }
        compose.waitForIdle()
    }

    @Test
    fun aDialogOnACarsScreenIsCarSized() = fitsTheCar("small")

    /** The dialog on a car's [screen]: its buttons on screen and 64 dp tall, none of its words under 24 sp. */
    private fun fitsTheCar(screen: String) {
        ask(car = true)
        for (button in listOf(CONFIRM, "Cancel")) {
            compose.onNode(hasText(button) and hasClickAction()).assertIsDisplayed()
            val height = buttonHeight(button)
            assertTrue("$screen: $button is $height dp tall", height >= 64f - 0.5f)
        }
        for (words in listOf(TITLE, BODY, CONFIRM, "Cancel")) {
            val size = drawnSp(words)
            assertTrue("$screen: $words at $size sp", size >= 24f - 0.1f)
        }
    }

    @Test
    @Config(qualifiers = CarScreenChecks.WIDE)
    fun onTheWideStripOfACarScreenADialogIsCarSizedAndInView() = fitsTheCar("wide")

    @Test
    @Config(qualifiers = CarScreenChecks.TALL_SPLIT)
    fun onACarScreenSplitInTwoADialogIsCarSizedAndInView() = fitsTheCar("tall-split")

    @Test
    @Config(qualifiers = CarScreenChecks.PORTRAIT)
    fun onAPortraitCarScreenADialogIsCarSizedAndInView() = fitsTheCar("portrait")

    @Test
    @Config(qualifiers = CarScreenChecks.TALL_PORTRAIT)
    fun onATallPortraitCarScreenADialogIsCarSizedAndInView() = fitsTheCar("tall-portrait")

    @Test
    @Config(qualifiers = CarScreenChecks.NARROW)
    fun onANarrowCarScreenADialogIsCarSizedAndInView() = fitsTheCar("narrow")

    @Test
    fun onAPhoneTheDialogIsAsItWas() {
        ask(car = false)
        assertEquals(48f, buttonHeight("Cancel"), 0.5f)
        assertTrue(drawnSp(BODY) < 24f)
    }

    /** The button reading [label]: the height it takes, its touch target, in the screen's dp. */
    private fun buttonHeight(label: String): Float =
        compose.onNode(hasText(label) and hasClickAction()).fetchSemanticsNode().layoutInfo.height / compose.density.density

    /** The size [words] are drawn at, in the screen's sp: the density they were laid out in counts. */
    private fun drawnSp(words: String): Float {
        val node = compose.onNode(hasText(words), useUnmergedTree = true).fetchSemanticsNode()
        val layouts = mutableListOf<TextLayoutResult>()
        node.config[SemanticsActions.GetTextLayoutResult].action?.invoke(layouts)
        val input = layouts.single().layoutInput
        return input.style.fontSize.value * input.density.fontScale * input.density.density / compose.density.density
    }

    private companion object {
        const val TITLE = "Sign out?"
        const val BODY = "You'll be signed out of the server."
        const val CONFIRM = "Sign out"
    }
}
