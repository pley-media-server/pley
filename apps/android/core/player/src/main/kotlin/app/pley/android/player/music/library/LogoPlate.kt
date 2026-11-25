package app.pley.android.player.music.library

import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Paint
import android.graphics.RectF
import java.io.ByteArrayOutputStream

/**
 * A channel's logo as a car's row shows it: fitted on a square plate, as
 * the phone's ChannelLogo fits it in its well. Providers' logos come in
 * every shape, many of them wide, white or black on nothing: shown as they
 * are, a car stretched them to the row's square or lost them on its gray.
 * The plate is light under a dark logo and dark under a light one, from
 * the logo's own colors, each pixel counted as much as it is opaque.
 */
internal object LogoPlate {
    /** [logo] (the server's PNG, JPEG or WebP) on its plate, a PNG [SIZE] px square; `null` for one that can't be read. */
    fun png(logo: ByteArray): ByteArray? {
        val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
        BitmapFactory.decodeByteArray(logo, 0, logo.size, bounds)
        if (bounds.outWidth <= 0 || bounds.outHeight <= 0) return null
        // Read no larger than it is drawn: a provider's logo can be thousands of pixels wide.
        var sample = 1
        while (maxOf(bounds.outWidth, bounds.outHeight) / (sample * 2) >= SIZE) sample *= 2
        val image = BitmapFactory.decodeByteArray(logo, 0, logo.size, BitmapFactory.Options().apply { inSampleSize = sample }) ?: return null
        val plate = Bitmap.createBitmap(SIZE, SIZE, Bitmap.Config.ARGB_8888)
        val canvas = Canvas(plate)
        val side = SIZE.toFloat()
        val paint = Paint(Paint.ANTI_ALIAS_FLAG)
        paint.color = if (isDark(image)) LIGHT else DARK
        canvas.drawRoundRect(RectF(0f, 0f, side, side), side * CORNER, side * CORNER, paint)
        val box = side * LOGO_SHARE
        val scale = minOf(box / image.width, box / image.height)
        val w = image.width * scale
        val h = image.height * scale
        val at = RectF((side - w) / 2, (side - h) / 2, (side + w) / 2, (side + h) / 2)
        canvas.drawBitmap(image, null, at, Paint(Paint.ANTI_ALIAS_FLAG or Paint.FILTER_BITMAP_FLAG))
        image.recycle()
        val out = ByteArrayOutputStream()
        plate.compress(Bitmap.CompressFormat.PNG, 100, out)
        plate.recycle()
        return out.toByteArray()
    }

    /**
     * Whether [image] reads as dark: its pixels' mean luminance, each weighed by its opacity, under half. A logo
     * with nothing opaque in it reads as light, and goes on the dark plate as the car's other rows are.
     */
    internal fun isDark(image: Bitmap): Boolean {
        val step = maxOf(1, maxOf(image.width, image.height) / SAMPLES)
        var weight = 0.0
        var light = 0.0
        for (y in 0 until image.height step step) {
            for (x in 0 until image.width step step) {
                val pixel = image.getPixel(x, y)
                val alpha = Color.alpha(pixel) / 255.0
                if (alpha == 0.0) continue
                val luminance = (0.2126 * Color.red(pixel) + 0.7152 * Color.green(pixel) + 0.0722 * Color.blue(pixel)) / 255
                weight += alpha
                light += alpha * luminance
            }
        }
        return weight > 0 && light / weight < 0.5
    }

    /** As large as a car draws a row's picture, and then some: the plate is read for every channel of a list. */
    const val SIZE = 256

    /** How much of the plate's side the logo takes at most, as the phone insets it in its well. */
    private const val LOGO_SHARE = 0.75f

    /** The plate's corners, of its side. */
    private const val CORNER = 0.12f

    /** The pixels read across a logo to weigh how dark it is. */
    private const val SAMPLES = 64

    /** The plates: the app's foreground color under a dark logo, its raised surface under a light one. */
    private const val LIGHT = 0xFFE9EBEF.toInt()
    private const val DARK = 0xFF222731.toInt()
}
