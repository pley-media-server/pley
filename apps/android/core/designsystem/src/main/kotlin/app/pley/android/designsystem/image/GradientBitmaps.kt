package app.pley.android.designsystem.image

import android.graphics.Bitmap
import android.util.LruCache
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.FilterQuality
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.lerp
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.IntSize
import kotlin.math.roundToInt

/**
 * Gradients as small pictures, stretched when drawn. On the GPUs of TV
 * sticks and boxes (PowerVR, Mali) a gradient shader costs per pixel, and a
 * full-screen one took about 13 ms a frame on a Google TV Streamer; a
 * stretched picture is one filtered texture read, a few times cheaper.
 * Pictures are built once on the CPU and kept.
 */
object GradientBitmaps {
    private const val STEPS = 256
    private val strips = LruCache<Key, ImageBitmap>(32)

    private data class Key(val horizontal: Boolean, val stops: List<Pair<Float, Color>>)

    /** A strip 256 pixels wide and 1 high, running through [stops] (positions 0 to 1) from left to right. */
    fun horizontal(vararg stops: Pair<Float, Color>): ImageBitmap = strip(true, stops.toList())

    /** A strip 1 pixel wide and 256 high, running through [stops] from top to bottom. */
    fun vertical(vararg stops: Pair<Float, Color>): ImageBitmap = strip(false, stops.toList())

    private fun strip(horizontal: Boolean, stops: List<Pair<Float, Color>>): ImageBitmap {
        val key = Key(horizontal, stops)
        strips.get(key)?.let { return it }
        val pixels = IntArray(STEPS) { i -> colorAt(stops, i / (STEPS - 1f)).toArgb() }
        val bitmap = if (horizontal) Bitmap.createBitmap(pixels, STEPS, 1, Bitmap.Config.ARGB_8888)
        else Bitmap.createBitmap(pixels, 1, STEPS, Bitmap.Config.ARGB_8888)
        return bitmap.asImageBitmap().also { strips.put(key, it) }
    }

    /**
     * A square of [side] pixels from [from] at the top left to [to] at the
     * bottom right, along its diagonal (CSS's `linear-gradient(135deg, …)`
     * on a square; stretched to other boxes, close to it). Blended in sRGB,
     * as CSS blends the web's `hsl()` colours. It changes only along x + y,
     * so one ramp of 2 × [side] - 1 colours fills it; and a linear gradient
     * stays one when stretched (the filter blends between pixels linearly),
     * so a small square serves any box. Tiles build these while drawing, as
     * a held key brings them in: a few microseconds each.
     */
    fun diagonal(from: Color, to: Color, side: Int = 16): ImageBitmap {
        val a = from.toArgb()
        val b = to.toArgb()
        val last = (2 * (side - 1)).coerceAtLeast(1)
        val ramp = IntArray(last + 1) { i -> lerpArgb(a, b, i.toFloat() / last) }
        val pixels = IntArray(side * side)
        for (y in 0 until side) for (x in 0 until side) pixels[y * side + x] = ramp[x + y]
        return Bitmap.createBitmap(pixels, side, side, Bitmap.Config.ARGB_8888).asImageBitmap()
    }

    /** [a] to [b] at [t] (0 to 1), channel by channel in sRGB, alpha included. */
    internal fun lerpArgb(a: Int, b: Int, t: Float): Int {
        fun channel(shift: Int): Int {
            val from = (a ushr shift) and 0xFF
            val to = (b ushr shift) and 0xFF
            return (from + (to - from) * t).roundToInt().coerceIn(0, 255) shl shift
        }
        return channel(24) or channel(16) or channel(8) or channel(0)
    }

    private fun colorAt(stops: List<Pair<Float, Color>>, t: Float): Color {
        if (t <= stops.first().first) return stops.first().second
        for (i in 1 until stops.size) {
            val (p1, c1) = stops[i]
            if (t <= p1) {
                val (p0, c0) = stops[i - 1]
                val f = if (p1 > p0) (t - p0) / (p1 - p0) else 1f
                return lerp(c0, c1, f)
            }
        }
        return stops.last().second
    }
}

/** Draws [image] stretched over the box at [topLeft] of [size], filtered, at [alpha]. */
fun DrawScope.drawStretched(image: ImageBitmap, topLeft: Offset, size: Size, alpha: Float = 1f) {
    if (size.width <= 0f || size.height <= 0f || alpha <= 0f) return
    drawImage(
        image = image,
        srcOffset = IntOffset.Zero,
        srcSize = IntSize(image.width, image.height),
        dstOffset = IntOffset(topLeft.x.roundToInt(), topLeft.y.roundToInt()),
        dstSize = IntSize(size.width.roundToInt(), size.height.roundToInt()),
        alpha = alpha,
        filterQuality = FilterQuality.Low,
    )
}
