package app.pley.android.fixtures

import java.io.ByteArrayOutputStream

/**
 * A Matroska file holding one PGS subtitle track, made here rather than
 * committed: what GET /api/files/{id}/subtitles/{index}.mks answers for
 * subtitles shown as pictures. Each cue is a display set drawing its text
 * (a small pixel font) white on a dark box, centered low on a 1080p plane,
 * and another clearing it at its end. Timestamps are the file's own.
 */
object PgsMks {
    /** The plane the pictures are placed on. */
    const val WIDTH = 1920
    const val HEIGHT = 1080

    /** The file for [cues] (start ms, end ms, text), [durationMs] long. */
    fun build(cues: List<Triple<Long, Long, String>>, durationMs: Long, language: String?): ByteArray {
        val clusters = ByteArrayOutputStream()
        cues.forEachIndexed { n, (start, end, text) ->
            clusters.write(cluster(start, show(n, text)))
            clusters.write(cluster(end, clear(n)))
        }
        val info = element(
            0x1549A966,
            uint(0x2AD7B1, 1_000_000) + float(0x4489, durationMs.toDouble()) + string(0x4D80, "pley fixtures") + string(0x5741, "pley fixtures"),
        )
        val entry = uint(0xD7, 1) + uint(0x73C5, 1) + uint(0x83, 0x11) + uint(0x9C, 0) + string(0x86, "S_HDMV/PGS") + string(0x22B59C, language ?: "und")
        val tracks = element(0x1654AE6B, element(0xAE, entry))
        val header = element(
            0x1A45DFA3,
            uint(0x4286, 1) + uint(0x42F7, 1) + uint(0x42F2, 4) + uint(0x42F3, 8) + string(0x4282, "matroska") + uint(0x4287, 4) + uint(0x4285, 2),
        )
        return header + element(0x18538067, info + tracks + clusters.toByteArray())
    }

    // ---------------------------------------------------------------- PGS (segments without the "PG" header, as Matroska keeps them)

    private fun show(n: Int, text: String): ByteArray {
        val bitmap = render(text)
        val w = bitmap[0].size
        val h = bitmap.size
        val x = (WIDTH - w) / 2
        val y = HEIGHT - h - BOTTOM_MARGIN
        val pcs = u16(WIDTH) + u16(HEIGHT) + byteArrayOf(0x10) + u16(n * 2) + byteArrayOf(0x80.toByte(), 0, 0, 1) +
            u16(0) + byteArrayOf(0, 0) + u16(x) + u16(y)
        val wds = byteArrayOf(1, 0) + u16(x) + u16(y) + u16(w) + u16(h)
        // YCrCb + alpha: 1 white, 2 black, 3 a dark box.
        val pds = byteArrayOf(0, 0) + entry(1, 235, 255) + entry(2, 16, 255) + entry(3, 16, 176)
        val rle = rle(bitmap)
        val ods = u16(0) + byteArrayOf(0, 0xC0.toByte()) + u24(rle.size + 4) + u16(w) + u16(h) + rle
        return segment(0x16, pcs) + segment(0x17, wds) + segment(0x14, pds) + segment(0x15, ods) + segment(0x80, ByteArray(0))
    }

    /** A display set with no object: the screen clears. */
    private fun clear(n: Int): ByteArray {
        val pcs = u16(WIDTH) + u16(HEIGHT) + byteArrayOf(0x10) + u16(n * 2 + 1) + byteArrayOf(0, 0, 0, 0)
        return segment(0x16, pcs) + segment(0x80, ByteArray(0))
    }

    private fun entry(id: Int, y: Int, alpha: Int) = byteArrayOf(id.toByte(), y.toByte(), 128.toByte(), 128.toByte(), alpha.toByte())

    private fun segment(type: Int, payload: ByteArray): ByteArray = byteArrayOf(type.toByte()) + u16(payload.size) + payload

    /** PGS run lengths, line by line. */
    private fun rle(bitmap: Array<IntArray>): ByteArray {
        val out = ByteArrayOutputStream()
        for (line in bitmap) {
            var i = 0
            while (i < line.size) {
                val color = line[i]
                var run = 1
                while (i + run < line.size && line[i + run] == color && run < 16_383) run++
                when {
                    color != 0 && run < 3 -> repeat(run) { out.write(color) }
                    color == 0 && run < 64 -> out.write(byteArrayOf(0, run.toByte()))
                    color == 0 -> out.write(byteArrayOf(0, (0x40 or (run shr 8)).toByte(), run.toByte()))
                    run < 64 -> out.write(byteArrayOf(0, (0x80 or run).toByte(), color.toByte()))
                    else -> out.write(byteArrayOf(0, (0xC0 or (run shr 8)).toByte(), run.toByte(), color.toByte()))
                }
                i += run
            }
            out.write(byteArrayOf(0, 0))
        }
        return out.toByteArray()
    }

    /** [text] in the pixel font, [SCALE] times, white with a black edge on the box. */
    private fun render(text: String): Array<IntArray> {
        val glyphs = text.uppercase().map { FONT[it] ?: FONT.getValue(' ') }
        val cell = (GLYPH_W + 1) * SCALE
        val w = glyphs.size * cell - SCALE + 2 * PAD
        val h = GLYPH_H * SCALE + 2 * PAD
        val bitmap = Array(h) { IntArray(w) { 3 } }
        glyphs.forEachIndexed { g, rows ->
            for (row in 0 until GLYPH_H) {
                for (col in 0 until GLYPH_W) {
                    if (rows[row][col] != '1') continue
                    for (dy in 0 until SCALE) for (dx in 0 until SCALE) {
                        bitmap[PAD + row * SCALE + dy][PAD + g * cell + col * SCALE + dx] = 1
                    }
                }
            }
        }
        // A black edge around the strokes, as subtitles have.
        val edged = Array(h) { y -> bitmap[y].copyOf() }
        for (y in 0 until h) for (x in 0 until w) {
            if (bitmap[y][x] != 3) continue
            val near = (-2..2).any { dy -> (-2..2).any { dx -> bitmap.getOrNull(y + dy)?.getOrNull(x + dx) == 1 } }
            if (near) edged[y][x] = 2
        }
        return edged
    }

    // ---------------------------------------------------------------- EBML

    private fun cluster(timeMs: Long, block: ByteArray): ByteArray =
        element(0x1F43B675, uint(0xE7, timeMs) + element(0xA3, byteArrayOf(0x81.toByte(), 0, 0, 0x80.toByte()) + block))

    private fun element(id: Long, payload: ByteArray): ByteArray = id(id) + size(payload.size.toLong()) + payload

    private fun element(id: Int, payload: ByteArray): ByteArray = element(id.toLong(), payload)

    private fun uint(id: Int, value: Long): ByteArray {
        var bytes = 1
        while (bytes < 8 && value ushr (8 * bytes) != 0L) bytes++
        return element(id, ByteArray(bytes) { i -> (value ushr (8 * (bytes - 1 - i))).toByte() })
    }

    private fun uint(id: Int, value: Int): ByteArray = uint(id, value.toLong())

    private fun float(id: Int, value: Double): ByteArray {
        val bits = value.toRawBits()
        return element(id, ByteArray(8) { i -> (bits ushr (8 * (7 - i))).toByte() })
    }

    private fun string(id: Int, value: String): ByteArray = element(id, value.toByteArray(Charsets.US_ASCII))

    private fun id(id: Long): ByteArray {
        var bytes = 1
        while (bytes < 4 && id ushr (8 * bytes) != 0L) bytes++
        return ByteArray(bytes) { i -> (id ushr (8 * (bytes - 1 - i))).toByte() }
    }

    /** An EBML size: the fewest bytes that hold it. */
    private fun size(value: Long): ByteArray {
        var bytes = 1
        while (bytes < 8 && value >= (1L shl (7 * bytes)) - 1) bytes++
        val marked = value or (1L shl (7 * bytes))
        return ByteArray(bytes) { i -> (marked ushr (8 * (bytes - 1 - i))).toByte() }
    }

    private fun u16(v: Int) = byteArrayOf((v shr 8).toByte(), v.toByte())

    private fun u24(v: Int) = byteArrayOf((v shr 16).toByte(), (v shr 8).toByte(), v.toByte())

    private const val SCALE = 5
    private const val PAD = 14
    private const val GLYPH_W = 5
    private const val GLYPH_H = 7
    private const val BOTTOM_MARGIN = 90

    /** A 5 × 7 font: what the cues say ("PGS 12", "VOBSUB 3"). */
    private val FONT: Map<Char, List<String>> = mapOf(
        ' ' to listOf("00000", "00000", "00000", "00000", "00000", "00000", "00000"),
        '0' to listOf("01110", "10001", "10011", "10101", "11001", "10001", "01110"),
        '1' to listOf("00100", "01100", "00100", "00100", "00100", "00100", "01110"),
        '2' to listOf("01110", "10001", "00001", "00010", "00100", "01000", "11111"),
        '3' to listOf("11111", "00010", "00100", "00010", "00001", "10001", "01110"),
        '4' to listOf("00010", "00110", "01010", "10010", "11111", "00010", "00010"),
        '5' to listOf("11111", "10000", "11110", "00001", "00001", "10001", "01110"),
        '6' to listOf("00110", "01000", "10000", "11110", "10001", "10001", "01110"),
        '7' to listOf("11111", "00001", "00010", "00100", "01000", "01000", "01000"),
        '8' to listOf("01110", "10001", "10001", "01110", "10001", "10001", "01110"),
        '9' to listOf("01110", "10001", "10001", "01111", "00001", "00010", "01100"),
        'P' to listOf("11110", "10001", "10001", "11110", "10000", "10000", "10000"),
        'G' to listOf("01110", "10001", "10000", "10111", "10001", "10001", "01111"),
        'S' to listOf("01111", "10000", "10000", "01110", "00001", "00001", "11110"),
        'E' to listOf("11111", "10000", "10000", "11110", "10000", "10000", "11111"),
        'N' to listOf("10001", "11001", "10101", "10011", "10001", "10001", "10001"),
        'F' to listOf("11111", "10000", "10000", "11110", "10000", "10000", "10000"),
        'R' to listOf("11110", "10001", "10001", "11110", "10100", "10010", "10001"),
        'U' to listOf("10001", "10001", "10001", "10001", "10001", "10001", "01110"),
        'D' to listOf("11110", "10001", "10001", "10001", "10001", "10001", "11110"),
        '-' to listOf("00000", "00000", "00000", "11111", "00000", "00000", "00000"),
    )
}
