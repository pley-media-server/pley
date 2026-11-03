package app.pley.android.designsystem.image

import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import kotlin.math.abs
import kotlin.math.cos
import kotlin.math.sin

/**
 * The picture shown while artwork loads and when there is none, as the
 * web's `placeholderGradient` (apps/web/src/lib/images.ts): a 135° gradient
 * from hsl(h, 30 %, 22 %) to hsl(h + 40, 35 %, 12 %), where h comes from a
 * hash of the title. It is drawn, never decoded.
 */
object Placeholder {
    /** The web's hue for [seed]: `h = h * 31 + code` over UTF-16 units, unsigned 32-bit, mod 360. */
    fun hue(seed: String): Int {
        var h = 0L
        for (ch in seed) h = (h * 31 + ch.code) and 0xFFFF_FFFFL
        return (h % 360).toInt()
    }

    /** The gradient's two ends for [seed]. */
    fun colors(seed: String): Pair<Color, Color> {
        val hue = hue(seed)
        return hsl(hue.toFloat(), 0.30f, 0.22f) to hsl(((hue + 40) % 360).toFloat(), 0.35f, 0.12f)
    }

    /**
     * The gradient over a box of [size], as CSS draws `linear-gradient(135deg, …)`:
     * along the diagonal from the top left to the bottom right, its ends
     * exactly at those corners.
     */
    fun brush(seed: String, size: Size): Brush {
        val (from, to) = colors(seed)
        val angle = Math.toRadians(135.0)
        val dx = sin(angle).toFloat()
        val dy = -cos(angle).toFloat()
        val half = (abs(size.width * dx) + abs(size.height * dy)) / 2f
        val centre = Offset(size.width / 2f, size.height / 2f)
        return Brush.linearGradient(
            colors = listOf(from, to),
            start = centre - Offset(dx * half, dy * half),
            end = centre + Offset(dx * half, dy * half),
        )
    }

    /** Every hue's picture once made: 360 of 16 × 16 pixels, under 400 KB. */
    private val pictures = android.util.LruCache<Int, androidx.compose.ui.graphics.ImageBitmap>(360)

    /**
     * The gradient for [seed] as a small picture, to draw stretched
     * ([drawStretched]): a gradient shader over a tile or a backdrop costs far
     * more on a TV's GPU. Kept per hue, all of them: a grid of a library's
     * titles goes through most hues, each tile that comes in needing its own.
     */
    fun picture(seed: String): androidx.compose.ui.graphics.ImageBitmap {
        val hue = hue(seed)
        pictures.get(hue)?.let { return it }
        val (from, to) = colors(seed)
        return GradientBitmaps.diagonal(from, to).also { pictures.put(hue, it) }
    }

    /** CSS's hsl(), hue in degrees, saturation and lightness 0 to 1. */
    fun hsl(hue: Float, saturation: Float, lightness: Float): Color {
        val c = (1f - abs(2f * lightness - 1f)) * saturation
        val hp = (hue % 360f) / 60f
        val x = c * (1f - abs(hp % 2f - 1f))
        val (r, g, b) = when {
            hp < 1f -> Triple(c, x, 0f)
            hp < 2f -> Triple(x, c, 0f)
            hp < 3f -> Triple(0f, c, x)
            hp < 4f -> Triple(0f, x, c)
            hp < 5f -> Triple(x, 0f, c)
            else -> Triple(c, 0f, x)
        }
        val m = lightness - c / 2f
        return Color(r + m, g + m, b + m)
    }
}
