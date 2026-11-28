package app.pley.android.designsystem.text

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.width
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.toPixelMap
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.test.captureToImage
import androidx.compose.ui.test.getUnclippedBoundsInRoot
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

/**
 * A word wider than its slot shrinks to fit on its line rather than
 * breaking, even at the system's largest font, where a floor in sp drew
 * too big for "Playlist" in a fifth of a small phone.
 */
@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class WholeWordsTextTest {
    @get:Rule
    val compose = createComposeRule()

    private val style = TextStyle(fontSize = 12.sp, lineHeight = 16.sp)

    private fun height(tag: String): Dp {
        val bounds = compose.onNodeWithTag(tag).getUnclippedBoundsInRoot()
        return bounds.bottom - bounds.top
    }

    @Test
    @Config(qualifiers = "w320dp-h640dp", fontScale = 2.0f)
    fun aWordTooWideAtTheLargestFontShrinksOntoOneLine() {
        compose.setContent {
            Column {
                Box(Modifier.width(300.dp)) { WholeWordsText("Playlist", style, Modifier.testTag("wide")) }
                Box(Modifier.width(56.dp)) { WholeWordsText("Playlist", style, Modifier.testTag("narrow")) }
            }
        }
        val wide = height("wide")
        val narrow = height("narrow")
        // Broken inside the word it would take two lines, taller than the one it has room for.
        assertTrue("$narrow tall in 56 dp, $wide on one line", narrow <= wide)
    }

    @Test
    fun aNewColorAloneDrawsItAgain() {
        // A theme picked changes the words' color and nothing else: the label is drawn again in it.
        var color by mutableStateOf(Color.Red)
        compose.setContent {
            Box(Modifier.width(300.dp).background(Color.White)) {
                WholeWordsText("Playlist", style.copy(fontSize = 32.sp, lineHeight = 40.sp, color = color), Modifier.testTag("label"))
            }
        }
        fun drawn(c: Color): Boolean {
            val pixels = compose.onNodeWithTag("label").captureToImage().toPixelMap()
            return (0 until pixels.width).any { x -> (0 until pixels.height).any { y -> pixels[x, y] == c } }
        }
        assertTrue(drawn(Color.Red))
        color = Color.Blue
        compose.waitForIdle()
        assertTrue("drawn again in its new color", drawn(Color.Blue))
        assertFalse(drawn(Color.Red))
    }
}
