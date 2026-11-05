package app.pley.android.player.session

import app.pley.android.model.MediaFile
import app.pley.android.model.MediaStream
import app.pley.android.model.PlaybackInfo
import app.pley.android.model.PlaybackMethod
import app.pley.android.model.StreamAction
import app.pley.android.model.StreamKind
import app.pley.android.model.TranscodeInfo
import app.pley.android.model.format.codecLabel
import app.pley.android.model.format.dynamicRangeLabel
import app.pley.android.model.format.formatBitrate
import app.pley.android.model.format.resolutionLabel
import app.pley.android.player.DetailRow
import app.pley.android.player.PlaybackDetailsUi
import app.pley.android.player.SubtitleDelivery

/**
 * How a video plays, for the top of the player's menu
 * (apps/web/src/player/PlaybackDetails.tsx): the way, why in a sentence,
 * then what goes in and what comes out, with a transcode's live numbers.
 */
object PlaybackDetails {
    private val ENCODERS = mapOf(
        "libx264" to "x264", "libx265" to "x265",
        "h264_nvenc" to "NVIDIA NVENC", "hevc_nvenc" to "NVIDIA NVENC",
        "h264_qsv" to "Intel Quick Sync", "hevc_qsv" to "Intel Quick Sync",
        "h264_amf" to "AMD AMF", "hevc_amf" to "AMD AMF",
        "h264_vaapi" to "VA-API", "hevc_vaapi" to "VA-API",
        "h264_videotoolbox" to "VideoToolbox", "hevc_videotoolbox" to "VideoToolbox",
        "h264_vulkan" to "Vulkan", "hevc_vulkan" to "Vulkan",
        "h264_v4l2m2m" to "V4L2",
    )

    /** A video stream as "HEVC 4K Dolby Vision P8.1", "HEVC 4K Dolby Vision P7 FEL". */
    fun videoLabel(v: MediaStream?): String {
        if (v == null) return ""
        val range = dynamicRangeLabel(v).orEmpty()
        return listOf(codecLabel(v.codec), resolutionLabel(v.width, v.height).orEmpty(), range).filter { it.isNotEmpty() }.joinToString(" ")
    }

    /**
     * What the screen gets of HDR video: the live transcode's tone mapping
     * when there is one, else what the server said ("HDR10 (Dolby Vision
     * base layer)"), else `null` (SDR video, or an older server).
     */
    fun shownAs(texts: PlayerTexts, info: PlaybackInfo, live: TranscodeInfo?): String? =
        live?.tonemap?.let { texts.toneMapped(it) } ?: info.videoRange

    /** An audio stream as "TrueHD 7.1". */
    fun audioLabel(a: MediaStream?): String {
        if (a == null) return ""
        val channels = a.channels?.let { CHANNELS[it] ?: "${it}ch" }
        return listOfNotNull(codecLabel(a.codec).takeIf { it.isNotEmpty() }, channels).joinToString(" ")
    }

    private val CHANNELS = mapOf(1 to "1.0", 2 to "2.0", 3 to "2.1", 6 to "5.1", 7 to "6.1", 8 to "7.1")

    /** The transcoder's "H.264 1920×1080" as "H.264 1080p". */
    fun simplified(text: String): String = SIZE.replace(text) { m ->
        resolutionLabel(m.groupValues[1].toIntOrNull(), m.groupValues[2].toIntOrNull()) ?: m.value
    }

    private val SIZE = Regex("""(\d+)×(\d+)""")

    /** The encoder and where it runs: "NVIDIA NVENC (GPU)", "x264 veryfast (CPU)". */
    fun encoderLabel(texts: PlayerTexts, t: TranscodeInfo): String {
        val name = t.encoder?.let { ENCODERS[it] ?: it }
        val where = t.pipeline?.takeIf { it != "copy" }?.let { "(${texts.pipeline(it)})" }
        return listOfNotNull(name, t.preset, where).joinToString(" ")
    }

    /**
     * The details of [info] playing [file] with audio [audioIndex]. [note]
     * says why it plays this way when the player knows better than the
     * server (a fallback); [live] is GET /api/playback/{session}'s
     * transcode; [engine] the engine's own lines; [subtitle] the subtitles
     * shown, if any.
     */
    fun build(
        texts: PlayerTexts,
        info: PlaybackInfo,
        file: MediaFile?,
        audioIndex: Int?,
        note: String?,
        positionMs: Long,
        live: TranscodeInfo?,
        engine: List<String>,
        subtitle: MediaStream? = null,
    ): PlaybackDetailsUi {
        val transcode = if (info.method == PlaybackMethod.Direct) null else live
        val streams = file?.streams.orEmpty()
        val video = streams.firstOrNull { it.kind == StreamKind.Video }
        val audios = streams.filter { it.kind == StreamKind.Audio }
        val audio = audios.firstOrNull { it.index == (audioIndex ?: info.audioIndex) } ?: audios.firstOrNull { it.default } ?: audios.firstOrNull()
        // The file as it is needs no reason; a fallback's says why it plays so. Nothing more specific known: the method says it.
        val why = if (info.method == PlaybackMethod.Direct) {
            note
        } else {
            note ?: info.reasons.joinToString(" ").ifEmpty { null } ?: info.reason
        }
        val pending = "…"
        val videoIn = transcode?.videoIn?.let(::simplified) ?: videoLabel(video)
        val audioIn = transcode?.audioIn ?: audioLabel(audio)
        val converting = info.method != PlaybackMethod.Direct
        val videoRow = if (converting && transcode?.video != StreamAction.Copy) {
            "$videoIn → " + (transcode?.videoOut?.let { simplified(it) + if (transcode.tonemap != null) " SDR" else "" } ?: pending)
        } else {
            videoIn
        }
        val audioRow = if (converting && transcode?.audio != StreamAction.Copy) "$audioIn → ${transcode?.audioOut ?: pending}" else audioIn
        val bitrate = formatBitrate(file?.bitrate)
        val bitrateRow = if (transcode?.bitrateOut != null && bitrate.isNotEmpty()) "$bitrate → ${formatBitrate(transcode.bitrateOut)}" else bitrate
        val ahead = transcode?.let { ((it.transcodedMs - positionMs) / 1000).toInt().coerceAtLeast(0) } ?: 0

        val rows = buildList {
            if (video != null) add(DetailRow(texts.rowVideo, videoRow))
            if (audio != null) add(DetailRow(texts.rowAudio, audioRow))
            if (bitrateRow.isNotEmpty()) add(DetailRow(texts.rowBitrate, bitrateRow))
            if (transcode?.video == StreamAction.Transcode && transcode.encoder != null) add(DetailRow(texts.rowEncoder, encoderLabel(texts, transcode)))
            if (video != null) shownAs(texts, info, transcode)?.let { add(DetailRow(texts.rowHdr, it)) }
            if (transcode?.subtitleBurn == true) add(DetailRow(texts.rowSubtitles, texts.burnedIn))
            transcode?.speed?.takeIf { it > 0 }?.let { add(DetailRow(texts.rowSpeed, texts.speed(it, ahead))) }
        }
        return PlaybackDetailsUi(
            method = texts.method(info),
            why = why,
            pipeline = transcode?.pipeline?.takeIf { transcode.video == StreamAction.Transcode }?.let(texts::pipeline),
            rows = rows,
            engine = engine,
            playbackMethod = info.method,
            file = file,
            video = video,
            audio = audio,
            subtitle = subtitle,
            subtitleDelivery = subtitle?.let { deliveryOf(info, it) },
            transcode = transcode,
        )
    }

    /**
     * How [subtitle] reaches the screen in [info]: burned in, the server's
     * .mks of pictures or its WebVTT, or read from the file by the player.
     */
    fun deliveryOf(info: PlaybackInfo, subtitle: MediaStream): SubtitleDelivery = when {
        info.burnSubtitles && info.subtitleIndex == subtitle.index -> SubtitleDelivery.BurnedIn
        info.hls && PlaybackPlan.isImage(subtitle) && !subtitle.external -> SubtitleDelivery.ServerPictures
        info.hls || subtitle.external -> SubtitleDelivery.Server
        else -> SubtitleDelivery.File
    }
}
