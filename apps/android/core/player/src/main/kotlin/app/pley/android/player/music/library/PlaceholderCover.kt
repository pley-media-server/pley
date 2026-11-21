package app.pley.android.player.music.library

import android.content.Context
import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.LinearGradient
import android.graphics.Paint
import android.graphics.Shader
import android.util.LruCache
import androidx.core.content.ContextCompat
import androidx.core.graphics.ColorUtils
import app.pley.android.player.R
import java.io.ByteArrayOutputStream

/**
 * The cover a car shows for music without one: the title's gradient, as the
 * app draws its placeholder (the design system's Placeholder, the web's
 * `placeholderGradient`), with Pley's mark on it. Drawn here, as a PNG
 * [MusicArtProvider] hands the car; what it is drawn from is the hue alone,
 * so no title goes into an address.
 */
internal object PlaceholderCover {
    /** The web's hue for [seed]: `h = h * 31 + code` over UTF-16 units, unsigned 32-bit, mod 360. */
    fun hue(seed: String): Int {
        var h = 0L
        for (ch in seed) h = (h * 31 + ch.code) and 0xFFFF_FFFFL
        return (h % 360).toInt()
    }

    /** Covers drawn, by hue: an album's tracks, and the rows of a car's list, ask for the same few. */
    private val drawn = LruCache<Int, ByteArray>(KEPT)

    /** [hue]'s cover, a PNG [SIZE] px square. */
    fun png(context: Context, hue: Int): ByteArray = drawn.get(hue) ?: draw(context, hue).also { drawn.put(hue, it) }

    private fun draw(context: Context, hue: Int): ByteArray {
        val bitmap = Bitmap.createBitmap(SIZE, SIZE, Bitmap.Config.ARGB_8888)
        val canvas = Canvas(bitmap)
        val side = SIZE.toFloat()
        // A 135° gradient over a square: from its top left corner to its bottom right.
        val from = ColorUtils.HSLToColor(floatArrayOf(hue.toFloat(), 0.30f, 0.22f))
        val to = ColorUtils.HSLToColor(floatArrayOf(((hue + 40) % 360).toFloat(), 0.35f, 0.12f))
        canvas.drawRect(0f, 0f, side, side, Paint().apply { shader = LinearGradient(0f, 0f, side, side, from, to, Shader.TileMode.CLAMP) })
        ContextCompat.getDrawable(context, R.drawable.music_art_mark)?.let { mark ->
            val size = (SIZE * MARK_SHARE).toInt()
            val at = (SIZE - size) / 2
            mark.setBounds(at, at, at + size, at + size)
            mark.draw(canvas)
        }
        val out = ByteArrayOutputStream()
        bitmap.compress(Bitmap.CompressFormat.PNG, 100, out)
        bitmap.recycle()
        return out.toByteArray()
    }

    /** As large as the server's covers for cars. */
    const val SIZE = 512

    /** How much of the cover's side Pley's mark takes. */
    private const val MARK_SHARE = 0.42f

    private const val KEPT = 24
}
