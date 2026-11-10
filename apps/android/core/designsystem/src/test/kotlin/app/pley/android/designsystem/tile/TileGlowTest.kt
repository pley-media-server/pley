package app.pley.android.designsystem.tile

import androidx.compose.ui.graphics.asAndroidBitmap
import app.pley.android.designsystem.PleyFocus
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.GraphicsMode

@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class TileGlowTest {
    @Test
    fun aGlowIsAQuarterResolutionAlphaBitmapFadingOut() {
        val glow = TileGlow.render(TileKind.Poster, density = 2f).asAndroidBitmap()
        // (110 + 2 × 20) by (165 + 2 × 20) dp, at 2 px per dp, a quarter of it, rounded up.
        assertEquals(75, glow.width)
        assertEquals(103, glow.height)
        assertEquals(android.graphics.Bitmap.Config.ALPHA_8, glow.config)
        assertTrue(glow.byteCount in 5_000..40_000)
        val edge = android.graphics.Color.alpha(glow.getPixel(10, glow.height / 2))
        val far = android.graphics.Color.alpha(glow.getPixel(0, 0))
        assertTrue("alpha at the tile's edge was $edge", edge in (PleyFocus.GLOW_ALPHA * 255).toInt() - 12..(PleyFocus.GLOW_ALPHA * 255).toInt() + 1)
        assertEquals(0, far)
        val halfway = android.graphics.Color.alpha(glow.getPixel(5, glow.height / 2))
        assertTrue("halfway $halfway", halfway in 1 until edge)
    }

    @Test
    fun everyKindHasOne() {
        for (kind in TileKind.entries) {
            val bytes = TileGlow.render(kind, density = 2f).asAndroidBitmap().byteCount
            assertTrue("$kind: $bytes", bytes in 3_000..40_000)
        }
    }
}
