package app.pley.android.player.music.library

import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.Canvas
import android.graphics.Paint
import android.graphics.Rect
import java.io.ByteArrayOutputStream

/**
 * A playlist's cover in the car: the covers of its first entries in a
 * 2×2 mosaic, as a playlist is told apart from an album at a glance.
 * Drawn by [MusicArtProvider] from the server's covers ([MusicArt.mosaicUri]).
 */
internal object MosaicCover {
    /**
     * [covers] (the server's PNG, JPEG or WebP; those that can't be read left out) in a PNG [SIZE] px square: four in
     * their quarters; two or three, the first again in the last quarters, so each shows; one alone, as it is; none,
     * `null`.
     */
    fun png(covers: List<ByteArray>): ByteArray? {
        val read = covers.mapNotNull { bytes -> decode(bytes)?.let { bytes to it } }
        if (read.isEmpty()) return null
        val images = read.map { it.second }
        if (images.size == 1) return read[0].first.also { images[0].recycle() }
        val order = when (images.size) {
            2 -> listOf(0, 1, 1, 0)
            3 -> listOf(0, 1, 2, 0)
            else -> listOf(0, 1, 2, 3)
        }
        val mosaic = Bitmap.createBitmap(SIZE, SIZE, Bitmap.Config.ARGB_8888)
        val canvas = Canvas(mosaic)
        val half = SIZE / 2
        val paint = Paint(Paint.ANTI_ALIAS_FLAG or Paint.FILTER_BITMAP_FLAG)
        order.forEachIndexed { quarter, index ->
            val image = images[index]
            val side = minOf(image.width, image.height)
            // Each cover's middle square, in its quarter.
            val from = Rect((image.width - side) / 2, (image.height - side) / 2, (image.width + side) / 2, (image.height + side) / 2)
            val x = (quarter % 2) * half
            val y = (quarter / 2) * half
            canvas.drawBitmap(image, from, Rect(x, y, x + half, y + half), paint)
        }
        images.forEach(Bitmap::recycle)
        val out = ByteArrayOutputStream()
        mosaic.compress(Bitmap.CompressFormat.PNG, 100, out)
        mosaic.recycle()
        return out.toByteArray()
    }

    /** [bytes] read no larger than a quarter is drawn. */
    private fun decode(bytes: ByteArray): Bitmap? {
        val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
        BitmapFactory.decodeByteArray(bytes, 0, bytes.size, bounds)
        if (bounds.outWidth <= 0 || bounds.outHeight <= 0) return null
        var sample = 1
        while (minOf(bounds.outWidth, bounds.outHeight) / (sample * 2) >= SIZE / 2) sample *= 2
        return BitmapFactory.decodeByteArray(bytes, 0, bytes.size, BitmapFactory.Options().apply { inSampleSize = sample })
    }

    /** As large as the server's covers for cars. */
    const val SIZE = 512

    /** Which drawing this is, named in every mosaic's address: one higher whenever how it is drawn changes. */
    const val DRAWING = 1

    /** The most covers a mosaic has, as many as the server names for a playlist. */
    const val MOST = 4
}
