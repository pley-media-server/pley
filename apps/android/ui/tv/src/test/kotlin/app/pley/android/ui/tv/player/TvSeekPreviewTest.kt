package app.pley.android.ui.tv.player

import android.graphics.Bitmap
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.collectAsState
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.toPixelMap
import androidx.compose.ui.test.captureToImage
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import app.pley.android.player.PlayerPhase
import app.pley.android.player.PlayerUiState
import app.pley.android.testing.player.session.FakePlaybackController
import app.pley.android.ui.tv.theme.TvTheme
import kotlin.math.abs
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

/** The seek preview's picture, drawn: letterboxed as the video is, never cropped to its box. */
@RunWith(RobolectricTestRunner::class)
@Config(qualifiers = "w960dp-h540dp-xhdpi")
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class TvSeekPreviewTest {
    @get:Rule
    val compose = createComposeRule()

    private fun near(expected: Color, actual: Color) =
        abs(expected.red - actual.red) < 0.05f && abs(expected.green - actual.green) < 0.05f && abs(expected.blue - actual.blue) < 0.05f

    @Test
    fun `a wide film's preview keeps its shape, black above and below it`() {
        val controller = FakePlaybackController(PlayerUiState(phase = PlayerPhase.Playing, positionMs = 600_000, durationMs = 3_600_000, hasThumbnails = true))
        // A 2.39:1 film's previews are 320 × 134.
        controller.thumbnails = { Bitmap.createBitmap(320, 134, Bitmap.Config.ARGB_8888).apply { eraseColor(android.graphics.Color.RED) } }
        val seeker = TvSeeker({ 600_000 }, { 3_600_000 }, {})
        seeker.press(1)
        compose.setContent {
            TvTheme {
                val state = controller.state.collectAsState()
                Box(Modifier.fillMaxSize()) { TvSeekPreview(state, seeker, controller) }
            }
        }
        compose.waitForIdle()
        val box = compose.onNodeWithTag(TAG_PREVIEW_PICTURE).captureToImage().toPixelMap()
        // 240 × 135 dp is 480 × 270 here; the picture fills its width, 201 px tall, centred.
        val middle = box.width / 2
        assertTrue("the picture in the middle", near(Color.Red, box[middle, box.height / 2]))
        assertTrue("black above it", near(Color.Black, box[middle, 15]))
        assertTrue("black below it", near(Color.Black, box[middle, box.height - 15]))
        assertTrue("the picture's top edge, past the bar", near(Color.Red, box[middle, 40]))
    }
}
