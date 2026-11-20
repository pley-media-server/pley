package app.pley.android.fixtures

import java.io.ByteArrayOutputStream
import java.util.zip.CRC32
import java.util.zip.Deflater

/**
 * The FixtureServer's pictures: PNGs drawn on request (no AWT, so they are
 * drawn on Android too), sized like the server's: widths rounded up to its
 * buckets (apps/server/src/images.rs), the height from the picture's shape.
 * A picture is a gradient in a color from its seed, so tiles look apart.
 * Recent ones are kept.
 */
internal class Pictures {
    /** A picture's shape: width over height. */
    enum class Shape(val ratio: Double, val defaultWidth: Int) {
        Poster(2.0 / 3.0, 640),
        Wide(16.0 / 9.0, 1280),
        Square(1.0, 256),
    }

    private val cache = object : LinkedHashMap<String, ByteArray>(64, 0.75f, true) {
        override fun removeEldestEntry(eldest: MutableMap.MutableEntry<String, ByteArray>?): Boolean = size > 256
    }

    /** A picture of [shape] for [seed], [width] pixels wide (bucketed), or its default width. */
    fun artwork(seed: Long, shape: Shape, width: Int?): ByteArray {
        val w = width?.let(::bucket) ?: shape.defaultWidth
        val h = (w / shape.ratio).toInt().coerceAtLeast(1)
        // A handful of colors: enough to tell tiles apart, few enough to keep.
        val hue = Math.floorMod(seed * 37, 24L).toInt()
        return cached("art:$hue:$w:$h") { gradient(w, h, hue) }
    }

    /**
     * A sheet of seek previews: [columns] × [rows] thumbnails of [width] ×
     * [height], each a shade of its own, outlined.
     */
    fun trickplaySheet(sheet: Int, width: Int, height: Int, columns: Int, rows: Int): ByteArray =
        cached("sheet:${sheet % 4}:$width:$height:$columns:$rows") {
            val first = (sheet % 4) * columns * rows
            Png.encode(width * columns, height * rows) { x, y ->
                val cell = first + (y / height) * columns + x / width
                val edge = x % width == 0 || y % height == 0
                if (edge) 0xFFFFFF else hsv((cell * 15) % 360, 0.5, 0.3 + 0.5 * ((x % width).toDouble() / width))
            }
        }

    private fun cached(key: String, make: () -> ByteArray): ByteArray {
        synchronized(cache) { cache[key]?.let { return it } }
        val bytes = make()
        synchronized(cache) { cache[key] = bytes }
        return bytes
    }

    private fun gradient(w: Int, h: Int, hue: Int): ByteArray {
        val top = hsv(hue * 15, 0.55, 0.85)
        val bottom = hsv((hue * 15 + 40) % 360, 0.7, 0.25)
        val rows = IntArray(h) { y -> mix(top, bottom, y.toDouble() / h) }
        // A lighter band across, so it isn't only a gradient.
        return Png.encode(w, h) { x, y ->
            val band = ((x + y * 2) % (w / 2 + 1)) < w / 12
            if (band) mix(rows[y], 0xFFFFFF, 0.15) else rows[y]
        }
    }

    companion object {
        /** The server's widths (apps/server/src/images.rs). */
        val BUCKETS = listOf(160, 240, 320, 480, 640, 960, 1280, 1920)

        /** [width] rounded up to a bucket; the largest above it. */
        fun bucket(width: Int): Int = BUCKETS.firstOrNull { it >= width } ?: BUCKETS.last()

        private fun mix(a: Int, b: Int, t: Double): Int {
            fun channel(shift: Int): Int {
                val x = (a shr shift) and 0xFF
                val y = (b shr shift) and 0xFF
                return (x + (y - x) * t).toInt().coerceIn(0, 255)
            }
            return (channel(16) shl 16) or (channel(8) shl 8) or channel(0)
        }

        private fun hsv(hue: Int, saturation: Double, value: Double): Int {
            val c = value * saturation
            val x = c * (1 - Math.abs((hue / 60.0) % 2 - 1))
            val m = value - c
            val (r, g, b) = when (hue / 60) {
                0 -> Triple(c, x, 0.0)
                1 -> Triple(x, c, 0.0)
                2 -> Triple(0.0, c, x)
                3 -> Triple(0.0, x, c)
                4 -> Triple(x, 0.0, c)
                else -> Triple(c, 0.0, x)
            }
            fun byte(v: Double) = ((v + m) * 255).toInt().coerceIn(0, 255)
            return (byte(r) shl 16) or (byte(g) shl 8) or byte(b)
        }
    }
}

/** The smallest PNG writer: 8-bit RGB, no interlacing, every row unfiltered. */
internal object Png {
    private val SIGNATURE = byteArrayOf(0x89.toByte(), 'P'.code.toByte(), 'N'.code.toByte(), 'G'.code.toByte(), 13, 10, 26, 10)

    /** A [width] × [height] picture whose pixel at (x, y) is [pixel]'s 0xRRGGBB. */
    fun encode(width: Int, height: Int, pixel: (x: Int, y: Int) -> Int): ByteArray {
        val raw = ByteArray((width * 3 + 1) * height)
        var i = 0
        for (y in 0 until height) {
            raw[i++] = 0
            for (x in 0 until width) {
                val rgb = pixel(x, y)
                raw[i++] = (rgb shr 16).toByte()
                raw[i++] = (rgb shr 8).toByte()
                raw[i++] = rgb.toByte()
            }
        }
        val deflater = Deflater(Deflater.BEST_SPEED)
        val compressed = ByteArrayOutputStream()
        try {
            deflater.setInput(raw)
            deflater.finish()
            val buffer = ByteArray(64 * 1024)
            while (!deflater.finished()) {
                val n = deflater.deflate(buffer)
                compressed.write(buffer, 0, n)
            }
        } finally {
            deflater.end()
        }
        val out = ByteArrayOutputStream()
        out.write(SIGNATURE)
        chunk(out, "IHDR", header(width, height))
        chunk(out, "IDAT", compressed.toByteArray())
        chunk(out, "IEND", ByteArray(0))
        return out.toByteArray()
    }

    private fun header(width: Int, height: Int): ByteArray {
        val b = ByteArrayOutputStream()
        b.write(int(width))
        b.write(int(height))
        // 8 bits per channel, color type 2 (RGB), deflate, no filter method, no interlace.
        b.write(byteArrayOf(8, 2, 0, 0, 0))
        return b.toByteArray()
    }

    private fun chunk(out: ByteArrayOutputStream, type: String, data: ByteArray) {
        val typeBytes = type.toByteArray(Charsets.US_ASCII)
        out.write(int(data.size))
        out.write(typeBytes)
        out.write(data)
        val crc = CRC32()
        crc.update(typeBytes)
        crc.update(data)
        out.write(int(crc.value.toInt()))
    }

    private fun int(v: Int): ByteArray =
        byteArrayOf((v ushr 24).toByte(), (v ushr 16).toByte(), (v ushr 8).toByte(), v.toByte())
}
