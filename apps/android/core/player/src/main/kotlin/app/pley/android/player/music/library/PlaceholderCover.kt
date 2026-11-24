package app.pley.android.player.music.library

import android.content.Context
import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.LinearGradient
import android.graphics.Paint
import android.graphics.Shader
import android.util.LruCache
import androidx.annotation.DrawableRes
import androidx.core.content.ContextCompat
import androidx.core.graphics.ColorUtils
import app.pley.android.player.R
import java.io.ByteArrayOutputStream

/**
 * The cover a car shows for music without one: the title's gradient, of the
 * hue the app draws its placeholder in (the design system's Placeholder,
 * the web's `placeholderGradient`), with Pley's mark on it. Drawn here, as
 * a PNG [MusicArtProvider] hands the car and the system's media controls;
 * what it is drawn from is the hue alone, so no title goes into an address.
 *
 * It is lighter and more colorful than the app's, its mark brighter: the
 * app's dark gradient sits on Pley's near-black pages, and on a car's dark
 * gray lists it was a muddy square, the mark hardly seen.
 */
internal object PlaceholderCover {
    /** The web's hue for [seed]: `h = h * 31 + code` over UTF-16 units, unsigned 32-bit, mod 360. */
    fun hue(seed: String): Int {
        var h = 0L
        for (ch in seed) h = (h * 31 + ch.code) and 0xFFFF_FFFFL
        return (h % 360).toInt()
    }

    /** What a cover shows on its gradient: Pley's mark, or the glyph of what it stands for, at [share] of its side. */
    enum class Glyph(@DrawableRes val drawable: Int, val share: Float) {
        Mark(R.drawable.music_art_mark, MARK_SHARE),

        /** An artist's Shuffle, as the first tile of their page: the glyph is drawn at half its box. */
        Shuffle(R.drawable.music_library_shuffle, 2 * MARK_SHARE),
    }

    /** Covers drawn, by glyph and hue: an album's tracks, and the rows of a car's list, ask for the same few. */
    private val drawn = LruCache<Int, ByteArray>(KEPT)

    /** [hue]'s cover with [glyph] on it, a PNG [SIZE] px square. */
    fun png(context: Context, hue: Int, glyph: Glyph = Glyph.Mark): ByteArray {
        val key = glyph.ordinal * 360 + hue
        return drawn.get(key) ?: draw(context, hue, glyph).also { drawn.put(key, it) }
    }

    private fun draw(context: Context, hue: Int, glyph: Glyph): ByteArray {
        val bitmap = Bitmap.createBitmap(SIZE, SIZE, Bitmap.Config.ARGB_8888)
        val canvas = Canvas(bitmap)
        val side = SIZE.toFloat()
        // A 135° gradient over a square: from its top left corner to its bottom right.
        val from = ColorUtils.HSLToColor(floatArrayOf(hue.toFloat(), SATURATION, LIGHT_FROM))
        val to = ColorUtils.HSLToColor(floatArrayOf(((hue + 40) % 360).toFloat(), SATURATION, LIGHT_TO))
        canvas.drawRect(0f, 0f, side, side, Paint().apply { shader = LinearGradient(0f, 0f, side, side, from, to, Shader.TileMode.CLAMP) })
        ContextCompat.getDrawable(context, glyph.drawable)?.let { mark ->
            val size = (SIZE * glyph.share).toInt()
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

    /**
     * Which drawing this is, named in every placeholder's address
     * ([MusicArt.placeholderUri]): one higher whenever how it is drawn
     * changes. Android Auto keeps each cover it has read by its address, so
     * under the same address a car went on showing the drawing before (2 is
     * the lighter one, its mark brighter).
     */
    const val DRAWING = 2

    /** The gradient's saturation, and its lightness at either end: the app's is 0.30 to 0.35, 0.22 to 0.12. */
    private const val SATURATION = 0.50f
    private const val LIGHT_FROM = 0.42f
    private const val LIGHT_TO = 0.28f

    /** How much of the cover's side Pley's mark takes. */
    private const val MARK_SHARE = 0.42f

    private const val KEPT = 24
}
