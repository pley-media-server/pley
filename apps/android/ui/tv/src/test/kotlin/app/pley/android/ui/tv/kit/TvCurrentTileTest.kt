package app.pley.android.ui.tv.kit

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.toPixelMap
import androidx.compose.ui.test.assert
import androidx.compose.ui.test.captureToImage
import androidx.compose.ui.test.getBoundsInRoot
import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.isSelected
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onRoot
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.tile.TileKind
import app.pley.android.ui.tv.theme.TvTheme
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

/**
 * The tile a page is about (an episode's page, in its season's row) is
 * marked while focus is elsewhere: a ring in the text's colour at the
 * well's edge, which no other tile has, and screen readers hear it as
 * selected. Drawn, so the ring is looked for in the picture.
 */
@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(qualifiers = "w960dp-h540dp-xhdpi")
class TvCurrentTileTest {
    @get:Rule
    val compose = createComposeRule()

    /** The colour 1 dp outside the left edge of the tile named [title], halfway down it. */
    private fun besideTile(title: String): Color {
        val tile = compose.onNode(hasContentDescription(title)).getBoundsInRoot()
        val image = compose.onRoot().captureToImage().toPixelMap()
        return image[((tile.left.value - 1f) * 2).toInt(), (tile.top.value + tile.bottom.value).toInt()]
    }

    private fun isLight(color: Color) = color.red > 0.6f && color.green > 0.6f && color.blue > 0.6f

    @Test
    fun theCurrentTileHasARingAtRestThatNoOtherHas() {
        compose.setContent {
            TvTheme {
                Row(Modifier.fillMaxSize().background(PleyColors.Bg)) {
                    TvRow(
                        rowKey = "r",
                        title = "Row",
                        tiles = tiles("T", 6),
                        kind = TileKind.Episode,
                        onOpen = { },
                        state = rememberLazyListState(),
                        current = 3,
                    )
                }
            }
        }
        compose.waitForIdle()
        compose.onNode(isSelected()).assert(hasContentDescription("T 3"))
        assertTrue(isLight(besideTile("T 3")))
        assertFalse(isLight(besideTile("T 2")))
    }
}
