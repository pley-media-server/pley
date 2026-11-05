package app.pley.android.model.format

import app.pley.android.model.DynamicRange
import app.pley.android.model.DynamicRangeFilter
import app.pley.android.model.ItemDetail
import app.pley.android.model.MediaFile
import app.pley.android.model.MediaStream
import app.pley.android.model.MediaSummary
import app.pley.android.model.StreamAction
import app.pley.android.model.StreamKind
import app.pley.android.model.TranscodeInfo

/*
 * What files and streams are like, in words: the web's labels
 * (apps/web/src/lib/format.ts, pages/item/shared.tsx, VideoItemView.tsx),
 * and the short badges of the design spec (§2.6) for tiles and metadata
 * lines, where the long forms don't fit. Codecs, formats and badges are
 * named the same in every language; the labels with words take them
 * last, English when given none.
 */

private val CODECS: Map<String, String> = mapOf(
    "h264" to "H.264", "hevc" to "HEVC", "h265" to "HEVC", "av1" to "AV1", "vp9" to "VP9", "vp8" to "VP8",
    "mpeg4" to "MPEG-4", "mpeg2video" to "MPEG-2", "mpeg1video" to "MPEG-1", "vc1" to "VC-1", "wmv3" to "WMV",
    "msmpeg4v3" to "DivX", "theora" to "Theora", "prores" to "ProRes", "aac" to "AAC", "ac3" to "AC3",
    "eac3" to "EAC3", "dts" to "DTS", "truehd" to "TrueHD", "mlp" to "MLP", "flac" to "FLAC", "alac" to "ALAC",
    "mp3" to "MP3", "mp2" to "MP2", "opus" to "Opus", "vorbis" to "Vorbis", "wmav2" to "WMA",
    "pcm_s16le" to "PCM", "pcm_s24le" to "PCM", "pcm_s32le" to "PCM", "pcm_f32le" to "PCM", "subrip" to "SRT",
    "srt" to "SRT", "ass" to "ASS", "ssa" to "SSA", "webvtt" to "WebVTT", "mov_text" to "MOV Text",
    "hdmv_pgs_subtitle" to "PGS", "pgssub" to "PGS", "dvd_subtitle" to "VobSub", "dvdsub" to "VobSub",
    "dvb_subtitle" to "DVB", "dvb_teletext" to "Teletext", "eia_608" to "CEA-608",
)

/** ffprobe's codec name as people know it: "h264" → "H.264", "hdmv_pgs_subtitle" → "PGS"; others upper-cased. */
fun codecLabel(codec: String?): String {
    if (codec.isNullOrEmpty()) return ""
    return CODECS[codec.lowercase()] ?: codec.uppercase()
}

/** 2 → "Stereo", 6 → "5.1", 8 → "7.1", 5 → "5ch"; the layout's name without a count. */
fun channelsLabel(channels: Int?, layout: String? = null, words: FormatWords = FormatWords.English): String {
    if (channels == null || channels == 0) return layout.orEmpty()
    return when (channels) {
        1 -> "Mono"
        2 -> words.stereo
        3 -> "2.1"
        6 -> "5.1"
        7 -> "6.1"
        8 -> "7.1"
        else -> words.channels(channels)
    }
}

/** "4K", "1440p", "1080p", "720p", "576p", "480p" or "SD", cropped pictures included; `null` without a size. */
fun resolutionLabel(width: Int?, height: Int?): String? {
    val w = width ?: 0
    val h = height ?: 0
    if (w == 0 && h == 0) return null
    return when {
        w >= 3800 || h >= 2100 -> "4K"
        w >= 2500 || h >= 1400 -> "1440p"
        w >= 1800 || h >= 1000 -> "1080p"
        w >= 1200 || h >= 700 -> "720p"
        h >= 540 -> "576p"
        h >= 470 -> "480p"
        else -> "SD"
    }
}

/** The four steps badges use: "4K", "1080p", "720p", "SD"; `null` without a size. */
fun resolutionBadge(width: Int?, height: Int?): String? {
    val w = width ?: 0
    val h = height ?: 0
    return when {
        w >= 3800 || h >= 2100 -> "4K"
        w >= 1800 || h >= 1000 -> "1080p"
        w >= 1200 || h >= 700 -> "720p"
        h > 0 -> "SD"
        else -> null
    }
}

/**
 * "Dolby Vision P8.1" (the profile, and its base layer's compatibility from
 * profile 8 on: profile 7's is always Blu-ray HDR10, never "P7.6"), "Dolby
 * Vision P7 FEL" (or MEL) with [dvEl], "+ HDR10+" with [hdr10plus], "HDR10+",
 * "HDR10", "HLG", "HDR" (an older server's flag), or `null` for SDR: the
 * server's names (docs/API.md, MediaStream), as the web and the TV apps say.
 */
fun dynamicRangeLabel(
    range: DynamicRange?,
    dvProfile: Int? = null,
    dvBlCompat: Int? = null,
    hdr: Boolean? = null,
    dvEl: String? = null,
    hdr10plus: Boolean? = null,
): String? =
    when (range) {
        DynamicRange.DolbyVision ->
            "Dolby Vision" + (dolbyVisionProfile(dvProfile, dvBlCompat, dvEl)?.let { " $it" } ?: "") +
                (if (hdr10plus == true) " + HDR10+" else "")
        DynamicRange.Hdr10Plus -> "HDR10+"
        DynamicRange.Hdr10 -> "HDR10"
        DynamicRange.Hlg -> "HLG"
        DynamicRange.Sdr -> null
        else -> if (hdr == true) "HDR" else null
    }

/** "P8.1", "P8.4", "P5", "P7 FEL": a Dolby Vision profile as the server names it. */
private fun dolbyVisionProfile(dvProfile: Int?, dvBlCompat: Int?, dvEl: String?): String? {
    if (dvProfile == null) return null
    val compat = dvBlCompat?.takeIf { dvProfile >= 8 && it in setOf(1, 2, 4) }?.let { ".$it" } ?: ""
    val layer = dvEl?.takeIf { dvProfile == 7 && it.isNotEmpty() }?.let { " ${it.uppercase()}" } ?: ""
    return "P$dvProfile$compat$layer"
}

/** [dynamicRangeLabel] of a video stream: "Dolby Vision P8.1 + HDR10+" for a hybrid. */
fun dynamicRangeLabel(stream: MediaStream?): String? =
    stream?.let { dynamicRangeLabel(it.dynamicRange, it.dvProfile, it.dvBlCompat, it.hdr, it.dvEl, it.hdr10plus) }

/**
 * The short form for badges, tiles and metadata lines: "DV P8.1", "DV P7
 * FEL", "DV", "HDR10+", "HDR10", "HLG", "HDR", or `null`. HDR10+ with Dolby
 * Vision is a badge of its own ([dynamicRangeBadges]).
 */
fun dynamicRangeShort(
    range: DynamicRange?,
    dvProfile: Int? = null,
    dvBlCompat: Int? = null,
    hdr: Boolean? = null,
    dvEl: String? = null,
): String? =
    dynamicRangeLabel(range, dvProfile, dvBlCompat, hdr, dvEl)?.replace("Dolby Vision", "DV")

/** A video stream's dynamic range badges: "DV P8.1", and "HDR10+" too for Dolby Vision + HDR10+. */
fun dynamicRangeBadges(stream: MediaStream?): List<String> {
    if (stream == null) return emptyList()
    val badge = dynamicRangeShort(stream.dynamicRange, stream.dvProfile, stream.dvBlCompat, stream.hdr, stream.dvEl)
        ?: return emptyList()
    val hybrid = stream.dynamicRange == DynamicRange.DolbyVision && stream.hdr10plus == true
    return if (hybrid) listOf(badge, "HDR10+") else listOf(badge)
}

/** Dolby Atmos, as ffprobe names it in the stream's profile ("Dolby TrueHD + Dolby Atmos") or a title says. */
fun isAtmos(stream: MediaStream?): Boolean {
    if (stream == null || stream.kind != StreamKind.Audio) return false
    return stream.profile.orEmpty().contains("atmos", ignoreCase = true) ||
        stream.title.orEmpty().contains("atmos", ignoreCase = true)
}

/** An audio stream's format: "TrueHD Atmos 7.1", "EAC3 5.1", "AAC Stereo". */
fun audioFormatLabel(stream: MediaStream, words: FormatWords = FormatWords.English): String = listOf(
    codecLabel(stream.codec),
    if (isAtmos(stream)) "Atmos" else "",
    channelsLabel(stream.channels, stream.channelLayout, words),
).filter { it.isNotEmpty() }.joinToString(" ")

/**
 * A track as the pickers list it. Audio: "English (EAC3 5.1)", "English –
 * Commentary (AAC Stereo)", "English (TrueHD Atmos 7.1)". Subtitles:
 * "French – SDH (SRT, Forced)", "Portuguese (BR) (PGS)". Video: "4K HEVC
 * Dolby Vision P8.1 10-bit".
 */
fun streamLabel(s: MediaStream, words: FormatWords = FormatWords.English): String {
    val lang = languageName(s.language, words)
    val title = s.title?.takeIf { it.isNotEmpty() }
    // Files' titles mostly name the language in English ("English SDH"), whatever the language shown.
    val named = title != null && listOfNotNull(lang, languageName(s.language, FormatWords.English)).any { title.lowercase().contains(it.lowercase()) }
    val extra = if (title != null && lang != null && !named) " – $title" else ""
    return when (s.kind) {
        StreamKind.Audio -> {
            val details = audioFormatLabel(s, words)
            val base = if (title != null && lang == null) title else lang ?: title ?: words.unknown
            base + extra + (if (details.isNotEmpty()) " ($details)" else "")
        }
        StreamKind.Subtitle -> {
            val base = lang ?: title ?: words.unknown
            val flags = listOfNotNull(
                codecLabel(s.codec).takeIf { it.isNotEmpty() },
                words.forced.takeIf { isForced(s) },
                words.external.takeIf { s.external },
            ).joinToString(", ")
            base + extra + (if (flags.isNotEmpty()) " ($flags)" else "")
        }
        else -> listOfNotNull(
            resolutionLabel(s.width, s.height),
            codecLabel(s.codec).takeIf { it.isNotEmpty() },
            dynamicRangeLabel(s),
            s.bitDepth?.takeIf { it > 8 }?.let { words.bitDepth(it) },
        ).joinToString(" ")
    }
}

/**
 * Forced subtitles: flagged so, or only named so ("English (Forced)",
 * "Signs & Songs"), as the server counts them when it chooses. Without it
 * a file's "English" and "English (Forced)" both read "English (SRT)".
 */
fun isForced(s: MediaStream): Boolean =
    s.forced || s.title.orEmpty().lowercase().split(Regex("[^\\p{L}\\p{N}]+")).any { it == "forced" || it == "signs" }

/**
 * The web's poster badges for a listing's [MediaSummary]: ["4K", "DV"], only 4K of the resolutions and no codec.
 * HDR10 and HDR10+ read as plain "HDR" and "HDR+" there; a title's page names them exactly.
 */
fun mediaBadges(media: MediaSummary?): List<String> {
    if (media == null) return emptyList()
    val range = when (media.dynamicRange) {
        DynamicRange.DolbyVision -> "DV"
        DynamicRange.Hdr10Plus -> "HDR+"
        DynamicRange.Hdr10 -> "HDR"
        DynamicRange.Hlg -> "HLG"
        else -> null
    }
    return listOfNotNull(resolutionLabel(media.width, media.height).takeIf { it == "4K" }, range)
}

/** "42 Mbps · 23.4 GB", "Bitrate unknown" when neither is known, "" without a summary. */
fun mediaLine(media: MediaSummary?, words: FormatWords = FormatWords.English): String {
    if (media == null) return ""
    return listOf(formatBitrate(media.bitrate, words), formatBytes(media.size, words))
        .filter { it.isNotEmpty() }
        .joinToString(" · ")
        .ifEmpty { words.bitrateUnknown }
}

/** How the library filters name [filter]: "Any HDR", "Dolby Vision", "HDR+"… ([DynamicRangeFilter.label] in [words]). */
fun dynamicRangeFilterLabel(filter: DynamicRangeFilter, words: FormatWords = FormatWords.English): String =
    if (filter == DynamicRangeFilter.Hdr) words.anyHdr else filter.label

/** The audio track a file starts with: the profile's choice, else the default, else the first. */
fun startingAudio(file: MediaFile): MediaStream? {
    val audio = file.streams.filter { it.kind == StreamKind.Audio }
    return audio.firstOrNull { it.index == file.selectedAudio } ?: audio.firstOrNull { it.default } ?: audio.firstOrNull()
}

/**
 * The short quality badges of a file, in the web's order: resolution
 * ("4K", "1080p", "720p", "SD"), dynamic range ("DV P8.1", "HDR10+",
 * "HDR10", "HLG", "HDR"), then "Atmos", else "7.1" or "5.1".
 */
fun qualityBadges(file: MediaFile?): List<String> {
    if (file == null) return emptyList()
    val video = file.streams.firstOrNull { it.kind == StreamKind.Video }
    val audio = file.streams.firstOrNull { it.kind == StreamKind.Audio && it.default }
        ?: file.streams.firstOrNull { it.kind == StreamKind.Audio }
    val channels = audio?.channels ?: 0
    return listOfNotNull(resolutionBadge(file.width ?: video?.width, file.height ?: video?.height)) +
        dynamicRangeBadges(video) +
        listOfNotNull(
            when {
                isAtmos(audio) -> "Atmos"
                channels >= 8 -> "7.1"
                channels >= 6 -> "5.1"
                else -> null
            },
        )
}

/** [qualityBadges] of an item's first file. */
fun qualityBadges(item: ItemDetail): List<String> = qualityBadges(item.files.firstOrNull())

/** Badges a listing can give (it has no streams): resolution and dynamic range, "DV" without its profile. */
fun qualityBadges(media: MediaSummary?): List<String> {
    if (media == null) return emptyList()
    return listOfNotNull(resolutionBadge(media.width, media.height), dynamicRangeShort(media.dynamicRange))
}

/**
 * A version in the web's picker: "Final Cut · 4K · HEVC · MKV · 58.2 GB",
 * else "Version 2". A size of 0 means unknown here (the web never gets 0).
 */
fun versionLabel(file: MediaFile, position: Int, words: FormatWords = FormatWords.English): String = listOf(
    file.edition.orEmpty(),
    resolutionLabel(file.width, file.height).orEmpty(),
    codecLabel(file.videoCodec),
    file.container?.uppercase().orEmpty(),
    if (file.size > 0) formatBytes(file.size, words) else "",
).filter { it.isNotEmpty() }.joinToString(" · ").ifEmpty { words.version(position + 1) }

/** A version in an item's Details: "4K · HEVC · Dolby Vision P8.1 · TrueHD Atmos 7.1 · 62.0 GB". */
fun versionDescription(file: MediaFile, words: FormatWords = FormatWords.English): String {
    val video = file.streams.firstOrNull { it.kind == StreamKind.Video }
    return listOf(
        file.edition.orEmpty(),
        resolutionLabel(file.width ?: video?.width, file.height ?: video?.height).orEmpty(),
        codecLabel(file.videoCodec ?: video?.codec),
        dynamicRangeLabel(video).orEmpty(),
        startingAudio(file)?.let { audioFormatLabel(it, words) }.orEmpty(),
        if (file.size > 0) formatBytes(file.size, words) else "",
    ).filter { it.isNotEmpty() }.joinToString(" · ")
}

/**
 * What the transcoder is doing, in one line: "HEVC 3840×2160 Dolby Vision
 * P8.1 → H.264 1920×1080 · EAC3 5.1 (copy) · h264_nvenc · cuda decode ·
 * libplacebo · 3.2×".
 */
fun transcodeSummary(t: TranscodeInfo, words: FormatWords = FormatWords.English): String {
    val parts = mutableListOf<String>()
    when (t.video) {
        StreamAction.Transcode -> parts += listOfNotNull(t.videoIn, t.videoOut).filter { it.isNotEmpty() }
            .joinToString(" → ").ifEmpty { words.videoTranscode }
        StreamAction.Copy -> parts += words.copied(t.videoIn?.takeIf { it.isNotEmpty() } ?: words.video)
        else -> Unit
    }
    when (t.audio) {
        StreamAction.Transcode -> listOfNotNull(t.audioIn, t.audioOut).filter { it.isNotEmpty() }
            .joinToString(" → ").takeIf { it.isNotEmpty() }?.let { parts += it }
        StreamAction.Copy -> t.audioIn?.takeIf { it.isNotEmpty() }?.let { parts += words.copied(it) }
        else -> Unit
    }
    val transcoding = t.video == StreamAction.Transcode
    if (transcoding) t.encoder?.takeIf { it.isNotEmpty() }?.let { parts += it }
    if (transcoding) t.decoder?.takeIf { it.isNotEmpty() && it != "cpu" }?.let { parts += words.decoded(it) }
    t.tonemap?.takeIf { it.isNotEmpty() }?.let { parts += it }
    if (t.subtitleBurn) parts += words.burnIn
    t.speed?.takeIf { it > 0 }?.let { parts += "${fixed(it, 1, words)}×" }
    return parts.joinToString(" · ")
}
