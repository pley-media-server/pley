package app.pley.android.player.music.library

import android.content.Context
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.BitmapShader
import android.graphics.Canvas
import android.graphics.Matrix
import android.graphics.Paint
import android.graphics.Shader
import android.util.LruCache
import java.io.ByteArrayOutputStream

/**
 * An artist's picture as a car's row shows it: cut to a circle, its
 * corners transparent, as the phone and the web show artists, so a list
 * of artists reads apart from the square covers of albums. Drawn by
 * [MusicArtProvider] from the server's picture, or the artist's
 * placeholder when there is none, under addresses whose letters name this
 * drawing ([MusicArt.artistUri]): they change whenever it is drawn
 * otherwise, as Android Auto keeps each picture by its address.
 */
internal object RoundPicture {
    /** [picture] (the server's PNG, JPEG or WebP), its middle square cut to a circle, a PNG [SIZE] px across; `null` for one that can't be read. */
    fun png(picture: ByteArray): ByteArray? {
        val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
        BitmapFactory.decodeByteArray(picture, 0, picture.size, bounds)
        if (bounds.outWidth <= 0 || bounds.outHeight <= 0) return null
        // Read no larger than it is drawn.
        var sample = 1
        while (minOf(bounds.outWidth, bounds.outHeight) / (sample * 2) >= SIZE) sample *= 2
        val image = BitmapFactory.decodeByteArray(picture, 0, picture.size, BitmapFactory.Options().apply { inSampleSize = sample }) ?: return null
        return round(image).also { image.recycle() }
    }

    /** Placeholders cut to a circle, by hue: a list of artists without pictures asks for the same few. */
    private val drawn = LruCache<Int, ByteArray>(KEPT)

    /** [hue]'s placeholder ([PlaceholderCover]), with a person on it, cut to a circle. */
    fun placeholder(context: Context, hue: Int): ByteArray = drawn.get(hue) ?: run {
        val png = PlaceholderCover.png(context, hue, PlaceholderCover.Glyph.Artist)
        val image = BitmapFactory.decodeByteArray(png, 0, png.size)
        round(image).also {
            image.recycle()
            drawn.put(hue, it)
        }
    }

    private fun round(image: Bitmap): ByteArray {
        val side = minOf(image.width, image.height).toFloat()
        val scale = SIZE / side
        // The picture's middle square, scaled to the circle's.
        val matrix = Matrix().apply {
            setTranslate(-(image.width - side) / 2, -(image.height - side) / 2)
            postScale(scale, scale)
        }
        val paint = Paint(Paint.ANTI_ALIAS_FLAG or Paint.FILTER_BITMAP_FLAG).apply {
            shader = BitmapShader(image, Shader.TileMode.CLAMP, Shader.TileMode.CLAMP).apply { setLocalMatrix(matrix) }
        }
        val circle = Bitmap.createBitmap(SIZE, SIZE, Bitmap.Config.ARGB_8888)
        val radius = SIZE / 2f
        Canvas(circle).drawCircle(radius, radius, radius, paint)
        val out = ByteArrayOutputStream()
        circle.compress(Bitmap.CompressFormat.PNG, 100, out)
        circle.recycle()
        return out.toByteArray()
    }

    /** As large as a car draws a row's picture, and then some. */
    const val SIZE = 256

    private const val KEPT = 24
}
