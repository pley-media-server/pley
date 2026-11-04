package app.pley.android.designsystem.image

import android.graphics.BitmapShader
import android.graphics.Matrix
import android.util.LruCache
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ColorFilter
import androidx.compose.ui.graphics.ColorMatrix
import androidx.compose.ui.graphics.Outline
import androidx.compose.ui.graphics.Shader
import androidx.compose.ui.graphics.ShaderBrush
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.compose.ui.graphics.drawOutline
import androidx.compose.ui.graphics.drawscope.DrawScope
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
    fun colors(seed: String): Pair<Color, Color> = colorsOf(hue(seed))

    private fun colorsOf(hue: Int): Pair<Color, Color> =
        hsl(hue.toFloat(), 0.30f, 0.22f) to hsl(((hue + 40) % 360).toFloat(), 0.35f, 0.12f)

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

    /**
     * The gradient's shape, the same for every title ([GradientBitmaps.diagonalRamp]):
     * [drawGradient] stretches it over the box, its black and white taken to
     * the title's two colours: one draw, painting each pixel once. A
     * gradient shader over a tile or a backdrop costs far more on a TV's
     * GPU, and a picture per title was a texture to make for each tile a
     * held key brought in (on a Google TV Streamer, up to 10 ms of the frame
     * each).
     */
    private val ramp by lazy { GradientBitmaps.diagonalRamp() }

    /** [seed]'s gradient: the colour matrix the ramp is drawn through. */
    class Look internal constructor(internal val filter: ColorFilter)

    /** Looks made, by hue (all a look depends on): each tile a held key brings in asks for one. */
    private val looks = LruCache<Int, Look>(48)

    /**
     * What [drawGradient] draws for [seed], to keep with the box: a matrix
     * taking the ramp's grey g (0 to 255) to from + (to - from) × g / 255
     * in each channel, the sRGB blend between the title's two colours.
     */
    fun look(seed: String): Look {
        val hue = hue(seed)
        looks.get(hue)?.let { return it }
        val (from, to) = colorsOf(hue)
        fun row(channel: Int, a: Float, b: Float) = FloatArray(5).also {
            it[channel] = b - a
            it[4] = a * 255f
        }
        val matrix = row(0, from.red, to.red) + row(1, from.green, to.green) + row(2, from.blue, to.blue) + floatArrayOf(0f, 0f, 0f, 1f, 0f)
        return Look(ColorFilter.colorMatrix(ColorMatrix(matrix))).also { looks.put(hue, it) }
    }

    /**
     * Draws [look]'s gradient over the box, or filling [outline] (a tile's
     * rounded shape) when given: shaped as it's drawn, not clipped, as the
     * native Samsung app draws its tiles. A clip is a pass of its own for a
     * TV's GPU, for each tile.
     */
    fun DrawScope.drawGradient(look: Look, outline: Outline? = null) {
        if (outline == null) {
            drawStretched(ramp, Offset.Zero, size, colorFilter = look.filter)
        } else {
            drawOutline(outline, rampBrush(size), colorFilter = look.filter)
        }
    }

    /**
     * A brush for each of the last few sizes of box the ramp filled (drawn
     * on the main thread alone). A ShaderBrush keeps the shader for the last
     * size it filled and makes another for any other: one brush for every
     * tile made a shader for each tile of another size than the one before,
     * as on a page with rows of posters and of landscape tiles.
     */
    private val rampBrushes = ArrayList<RampBrush>(RAMP_SIZES)

    private fun rampBrush(size: Size): RampBrush {
        for (i in rampBrushes.indices) {
            val brush = rampBrushes[i]
            if (brush.size == size) return brush
        }
        if (rampBrushes.size == RAMP_SIZES) rampBrushes.removeAt(0)
        return RampBrush(size).also { rampBrushes += it }
    }

    /** The ramp stretched over a box of [size], as [drawStretched] stretches it. */
    private class RampBrush(val size: Size) : ShaderBrush() {
        override fun createShader(size: Size): Shader {
            val bitmap = ramp.asAndroidBitmap()
            return BitmapShader(bitmap, android.graphics.Shader.TileMode.CLAMP, android.graphics.Shader.TileMode.CLAMP).apply {
                setLocalMatrix(Matrix().apply { setScale(size.width / bitmap.width, size.height / bitmap.height) })
            }
        }
    }

    /** The sizes [rampBrushes] keeps a brush for: a page's kinds of tile. */
    private const val RAMP_SIZES = 8

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
