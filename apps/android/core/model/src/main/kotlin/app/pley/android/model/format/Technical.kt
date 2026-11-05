package app.pley.android.model.format

import app.pley.android.model.DynamicRange
import app.pley.android.model.MediaStream
import java.util.Locale

/*
 * A stream's technical details in words, for media info and the player's
 * readout: codec levels, frame rates, colour, Dolby Vision. ffprobe's names
 * come in; what people read in MediaInfo goes out. The standards' names
 * (BT.2020, PQ, 4:2:0) are the same in every language; the labels with
 * words take them last, English when given none.
 */

/**
 * ffprobe's level as written: HEVC counts thirtieths (153 is 5.1), H.264
 * and VP9 tenths (41 is 4.1), AV1 its seq_level_idx (13 is 5.1); `null`
 * when unknown (ffprobe's -99) or for other codecs.
 */
fun levelLabel(codec: String?, level: Int?): String? {
    if (level == null || level <= 0) return null
    return when (codec?.lowercase(Locale.ROOT)) {
        "hevc", "h265" -> trimmedNumber(level / 30.0, 1)
        "h264", "vp9" -> trimmedNumber(level / 10.0, 1)
        "av1" -> "${2 + (level shr 2)}.${level and 3}"
        else -> null
    }
}

/** "Main 10 · Level 5.1", "High · Level 4", "LC"; empty when neither is known. */
fun profileLevelLabel(s: MediaStream, words: FormatWords = FormatWords.English): String = listOfNotNull(
    s.profile?.takeIf { it.isNotBlank() },
    levelLabel(s.codec, s.level)?.let { words.level(it) },
).joinToString(" · ")

/** 23.976 → "23.976 fps", 24.0 → "24 fps", 29.97 → "29.97 fps"; `null` when unknown. */
fun frameRateLabel(fps: Double?, words: FormatWords = FormatWords.English): String? =
    fps?.takeIf { it > 0 }?.let { words.frameRate(trimmedNumber(it, 3, words.decimalSeparator)) }

/** 48000 → "48 kHz", 44100 → "44.1 kHz"; `null` when unknown. */
fun sampleRateLabel(hz: Int?, words: FormatWords = FormatWords.English): String? =
    hz?.takeIf { it > 0 }?.let { trimmedNumber(it / 1000.0, 1, words.decimalSeparator) + " kHz" }

/** "yuv420p10le" → "4:2:0"; `null` for formats that aren't YUV. */
fun chromaLabel(pixFmt: String?): String? {
    val f = pixFmt?.lowercase(Locale.ROOT) ?: return null
    return when {
        f.startsWith("yuv420") || f.startsWith("yuvj420") || f.startsWith("nv12") || f.startsWith("p010") -> "4:2:0"
        f.startsWith("yuv422") || f.startsWith("yuvj422") -> "4:2:2"
        f.startsWith("yuv444") || f.startsWith("yuvj444") -> "4:4:4"
        f.startsWith("gray") -> "4:0:0"
        else -> null
    }
}

/** "10-bit · 4:2:0", from the stream's bit depth and pixel format; empty when neither is known. */
fun bitDepthLabel(s: MediaStream, words: FormatWords = FormatWords.English): String = listOfNotNull(
    s.bitDepth?.takeIf { it > 0 }?.let { words.bitDepth(it) },
    chromaLabel(s.pixFmt),
).joinToString(" · ")

private val PRIMARIES = mapOf(
    "bt709" to "BT.709", "bt2020" to "BT.2020", "smpte170m" to "BT.601 NTSC", "bt470bg" to "BT.601 PAL",
    "bt470m" to "BT.470 M", "smpte240m" to "SMPTE 240M", "smpte431" to "DCI-P3", "smpte432" to "Display P3",
    "film" to "Film",
)

private val TRANSFERS = mapOf(
    "smpte2084" to "PQ (SMPTE ST 2084)", "arib-std-b67" to "HLG (ARIB STD-B67)", "bt709" to "BT.709",
    "smpte170m" to "BT.601", "bt470bg" to "BT.470 BG", "bt2020-10" to "BT.2020 (10-bit)", "bt2020-12" to "BT.2020 (12-bit)",
    "iec61966-2-1" to "sRGB", "linear" to "Linear", "smpte240m" to "SMPTE 240M",
)

private val MATRICES = mapOf(
    "bt709" to "BT.709", "bt2020nc" to "BT.2020 non-constant", "bt2020c" to "BT.2020 constant",
    "smpte170m" to "BT.601", "bt470bg" to "BT.601", "ictcp" to "ICtCp", "gbr" to "RGB", "ycgco" to "YCgCo",
    "smpte240m" to "SMPTE 240M", "fcc" to "FCC",
)

/** ffprobe's colour primaries as people know them: "bt2020" → "BT.2020"; others as they are; `null` when unknown. */
fun colorPrimariesLabel(name: String?): String? = colorName(name, PRIMARIES)

/** ffprobe's transfer: "smpte2084" → "PQ (SMPTE ST 2084)", "arib-std-b67" → "HLG (ARIB STD-B67)". */
fun colorTransferLabel(name: String?): String? = colorName(name, TRANSFERS)

/** ffprobe's colour space (the matrix): "bt2020nc" → "BT.2020 non-constant". */
fun colorMatrixLabel(name: String?): String? = colorName(name, MATRICES)

private fun colorName(name: String?, names: Map<String, String>): String? {
    val n = name?.trim()?.lowercase(Locale.ROOT)?.takeIf { it.isNotEmpty() && it != "unknown" && it != "reserved" } ?: return null
    return names[n] ?: n
}

/** A Dolby Vision base layer's compatibility: 1 "HDR10", 2 "SDR", 4 "HLG", 6 "Blu-ray HDR10"; `null` for none or unknown. */
fun dvCompatLabel(blCompat: Int?): String? = when (blCompat) {
    1 -> "HDR10"
    2 -> "SDR"
    4 -> "HLG"
    6 -> "Blu-ray HDR10"
    else -> null
}

/**
 * Dolby Vision in full: "Profile 8.1 · Level 6 · HDR10-compatible base
 * layer", "Profile 5 · Level 6 · no base layer", "Profile 8 · Level 6 ·
 * Blu-ray HDR10-compatible base layer" (named as [dynamicRangeLabel] names
 * it: 8.1, 8.2 and 8.4 only). `null` for other streams.
 */
fun dolbyVisionDetails(s: MediaStream, words: FormatWords = FormatWords.English): String? {
    if (s.dynamicRange != DynamicRange.DolbyVision) return null
    val compat = s.dvBlCompat
    val profile = s.dvProfile?.let { p ->
        words.dvProfile("$p" + (compat?.takeIf { p >= 8 && it in setOf(1, 2, 4) }?.let { ".$it" } ?: ""))
    }
    val base = when {
        compat == null -> null
        compat == 0 -> words.dvNoBaseLayer
        else -> dvCompatLabel(compat)?.let { words.dvBaseLayer(it) }
    }
    val layer = when (s.dvEl?.takeIf { s.dvProfile == 7 }) {
        "fel" -> words.dvFullLayer
        "mel" -> words.dvMinimalLayer
        else -> null
    }
    return listOfNotNull(profile, s.dvLevel?.takeIf { it > 0 }?.let { words.level("$it") }, base, layer)
        .joinToString(" · ").ifEmpty { null }
}

/**
 * The HDR format of a video stream: "Dolby Vision P8.1 + HDR10+", "Dolby
 * Vision P7 FEL", "HDR10+", "HDR10", "HLG", or "SDR".
 */
fun hdrFormatLabel(s: MediaStream): String = dynamicRangeLabel(s) ?: "SDR"

/** "3840×2160 (4K)"; `null` without a size. */
fun dimensionsLabel(width: Int?, height: Int?): String? {
    val w = width ?: 0
    val h = height ?: 0
    if (w <= 0 || h <= 0) return null
    return "$w×$h" + (resolutionLabel(w, h)?.let { " ($it)" } ?: "")
}

/** [value] with up to [digits] decimals, trailing zeros dropped: 24.0 → "24", 23.976 → "23.976" ("23,976" with [separator] a comma). */
private fun trimmedNumber(value: Double, digits: Int, separator: Char = '.'): String =
    String.format(Locale.ROOT, "%.${digits}f", value).trimEnd('0').trimEnd('.').replace('.', separator)
