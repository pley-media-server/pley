package app.pley.android.ui.phone.player

import android.graphics.Bitmap
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.toPixelMap
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.test.captureToImage
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.unit.dp
import kotlin.math.abs
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.GraphicsMode

/** The scrub preview's tile, drawn: letterboxed as the video is, never cropped to its box. */
@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class PreviewTileTest {
    @get:Rule
    val compose = createComposeRule()

    private fun near(expected: Color, actual: Color) =
        abs(expected.red - actual.red) < 0.05f && abs(expected.green - actual.green) < 0.05f && abs(expected.blue - actual.blue) < 0.05f

    @Test
    fun aWideFilmsPreviewKeepsItsShapeWithBlackAboveAndBelow() {
        // A 2.39:1 film's previews are 320 × 134.
        val frame = Bitmap.createBitmap(320, 134, Bitmap.Config.ARGB_8888).apply { eraseColor(android.graphics.Color.RED) }
        compose.setContent { PreviewTile(frame, 160.dp, 90.dp, Modifier.testTag("tile")) }
        compose.waitForIdle()
        val tile = compose.onNodeWithTag("tile").captureToImage().toPixelMap()
        // The picture fills the tile's width, about three quarters of its height, centred.
        val middle = tile.width / 2
        assertTrue("the picture in the middle", near(Color.Red, tile[middle, tile.height / 2]))
        assertTrue("black above it", near(Color.Black, tile[middle, tile.height * 7 / 90]))
        assertTrue("black below it", near(Color.Black, tile[middle, tile.height * 83 / 90]))
    }
}
