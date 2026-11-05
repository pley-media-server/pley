package app.pley.android.model.format

import java.util.Locale

/*
 * Sizes and rates, as apps/web/src/lib/format.ts writes them: decimals
 * like JavaScript's toFixed, with the decimal separator of the words'
 * language ("1.5 KB", "1,5 Ko").
 */

/** 1536 → "1.5 KB", 150 MiB → "150 MB": three significant digits, in units of 1024, up to TB. */
fun formatBytes(bytes: Long?, words: FormatWords = FormatWords.English): String {
    if (bytes == null) return ""
    var v = bytes.toDouble()
    var i = 0
    while (v >= 1024 && i < BYTE_POWERS - 1) {
        v /= 1024
        i++
    }
    val digits = if (v >= 100 || i == 0) 0 else 1
    return words.size(fixed(v, digits, words), i)
}

/** Bits per second → "8.2 Mbps", "12 Mbps", "320 kbps"; empty without a rate. */
fun formatBitrate(bps: Long?, words: FormatWords = FormatWords.English): String {
    if (bps == null || bps == 0L) return ""
    if (bps >= 1_000_000) return words.megabits(fixed(bps / 1_000_000.0, if (bps >= 10_000_000) 0 else 1, words))
    return words.kilobits(Math.round(bps / 1000.0).toString())
}

/** Bytes, KB, MB, GB and TB. */
private const val BYTE_POWERS = 5

/** [value] with [digits] decimals and a point, rounding half up as toFixed does for these values. */
internal fun fixed(value: Double, digits: Int): String = String.format(Locale.ROOT, "%.${digits}f", value)

/** [fixed] with [words]' decimal separator. */
internal fun fixed(value: Double, digits: Int, words: FormatWords): String =
    fixed(value, digits).let { if (digits > 0) it.replace('.', words.decimalSeparator) else it }
