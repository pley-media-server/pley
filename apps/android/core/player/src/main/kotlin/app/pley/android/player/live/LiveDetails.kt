package app.pley.android.player.live

import android.content.res.Resources
import app.pley.android.common.formatWords
import app.pley.android.model.LivePlan
import app.pley.android.model.PlaybackMethod
import app.pley.android.model.format.FormatWords
import app.pley.android.model.format.channelsLabel
import app.pley.android.model.format.codecLabel
import app.pley.android.model.format.formatBitrate
import app.pley.android.model.format.frameRateLabel
import app.pley.android.player.DetailRow
import app.pley.android.player.PlaybackDetailsUi
import app.pley.android.player.R
import app.pley.android.player.session.PlaybackDetails

/** Why a channel isn't played as its provider sends it ([LivePlayback.notAsSent]). */
enum class NotAsSent {
    /** Its video is interlaced, and the server makes it progressive. */
    Interlaced,

    /** No decoder here takes its video. */
    Video,

    /** Nothing here decodes its sound or sends it to a receiver as it is. */
    Audio,

    /**
     * Nothing to tell: the device isn't probed yet, the plan doesn't name a
     * codec, or the device decodes it but the player doesn't read it from
     * MPEG-TS.
     */
    Unknown,
}

/**
 * How a channel plays now ([LivePlayback.details]): from `stream.ts` as
 * its provider sends it ([asSent]) or the server's HLS, what the server
 * says of it ([plan]; `null`: not known), and [why] it isn't played as
 * sent, when the plan says.
 */
data class LivePlaying(val plan: LivePlan?, val asSent: Boolean, val why: NotAsSent? = null)

/**
 * A channel's Playback details, in the words and rows of a video's
 * ([PlaybackDetails]): the way it plays (Direct play from `stream.ts`,
 * Direct stream for HLS with its video copied, "· Audio transcoding" after
 * it when its sound is converted, else Transcoding), why it isn't played
 * as sent, then the provider's video, sound and bitrate, with
 * what the server makes of them when it converts them. A row whose value
 * isn't known is left out.
 */
object LiveDetails {
    /**
     * [playing] in the language of [res]; `null` for a channel on HLS whose
     * plan isn't known, of which nothing can be said.
     */
    fun build(res: Resources, playing: LivePlaying): PlaybackDetailsUi? {
        val plan = playing.plan
        val method = when {
            playing.asSent -> PlaybackMethod.Direct
            plan == null -> return null
            plan.video in AS_SENT -> PlaybackMethod.Remux
            else -> PlaybackMethod.Transcode
        }
        val words = formatWords(res)
        // What the server makes of it reaches the player only on HLS.
        val transcoded = method == PlaybackMethod.Transcode
        val soundConverted = !playing.asSent && plan?.audio == "aac"
        val rows = buildList {
            if (plan == null) return@buildList
            video(res, plan, transcoded, words)?.let { add(DetailRow(res.getString(R.string.session_row_video), it)) }
            audio(plan, soundConverted, words)?.let { add(DetailRow(res.getString(R.string.session_row_audio), it)) }
            bitrate(plan, transcoded, words)?.let { add(DetailRow(res.getString(R.string.session_row_bitrate), it)) }
            if (transcoded) add(DetailRow(res.getString(R.string.session_row_encoder), PlaybackDetails.encoderName(plan.video)))
        }
        return PlaybackDetailsUi(
            method = res.getString(
                when (method) {
                    PlaybackMethod.Direct -> R.string.session_method_direct
                    // As a video's (PlayerTexts.method): a direct stream says when its sound is converted.
                    PlaybackMethod.Remux -> if (soundConverted) R.string.session_method_remux_audio else R.string.session_method_remux
                    else -> R.string.session_method_transcode
                },
            ),
            why = why(res, playing),
            pipeline = null,
            rows = rows,
            engine = emptyList(),
            playbackMethod = method,
        )
    }

    /** [LivePlaying.why] in a sentence; `null` when there is nothing to tell. */
    private fun why(res: Resources, playing: LivePlaying): String? = when (playing.why) {
        NotAsSent.Interlaced -> res.getString(R.string.live_why_interlaced)
        NotAsSent.Video -> res.getString(R.string.live_why_video, codecLabel(playing.plan?.videoCodec))
        NotAsSent.Audio -> res.getString(R.string.live_why_audio, codecLabel(playing.plan?.audioCodec))
        NotAsSent.Unknown, null -> null
    }

    /** "HEVC 10-bit · 3840×2160 · 50 fps", " · interlaced", and " → H.264 1080p" when [transcoded]; `null` without video. */
    private fun video(res: Resources, plan: LivePlan, transcoded: Boolean, words: FormatWords): String? {
        if (plan.video == "none") return null
        val codec = listOfNotNull(
            codecLabel(plan.videoCodec).ifEmpty { null },
            plan.bitDepth?.takeIf { it > 0 }?.let(words::bitDepth),
        ).joinToString(" ")
        val width = plan.width ?: 0
        val height = plan.height ?: 0
        val source = listOfNotNull(
            codec.ifEmpty { null },
            if (width > 0 && height > 0) "$width×$height" else null,
            frameRateLabel(plan.frameRate, words),
            if (plan.interlaced) res.getString(R.string.live_details_interlaced) else null,
        ).joinToString(" · ")
        if (!transcoded) return source.ifEmpty { null }
        val lines = (plan.outHeight ?: plan.height)?.takeIf { it > 0 }?.let { "${it}p" }
        val made = listOfNotNull(codecLabel(encodedCodec(plan.video)), lines).joinToString(" ")
        return if (source.isEmpty()) made else "$source → $made"
    }

    /** "EAC3 5.1", and " → AAC Stereo" when [converted]; `null` without sound. */
    private fun audio(plan: LivePlan, converted: Boolean, words: FormatWords): String? {
        if (plan.audio == "none") return null
        val source = listOfNotNull(
            codecLabel(plan.audioCodec).ifEmpty { null },
            channelsLabel(plan.audioChannels, words = words).ifEmpty { null },
        ).joinToString(" ")
        // The server makes AAC stereo of sound it converts (docs/API.md, "The stream").
        val made = codecLabel("aac") + " " + channelsLabel(2, words = words)
        return when {
            !converted -> source.ifEmpty { null }
            source.isEmpty() -> made
            else -> "$source → $made"
        }
    }

    /** What the provider sends now, "12 Mbps", and " → 8 Mbps" for the video of a transcode; `null` until measured. */
    private fun bitrate(plan: LivePlan, transcoded: Boolean, words: FormatWords): String? {
        val provider = formatBitrate(plan.kbps * 1000, words).ifEmpty { return null }
        val made = if (transcoded) formatBitrate(plan.videoKbps?.times(1000), words) else ""
        return if (made.isEmpty()) provider else "$provider → $made"
    }

    /** The codec an encoder makes, in the server's names: HEVC for `hevc_nvenc` and `libx265`, else H.264, which is what the server makes of live video. */
    private fun encodedCodec(encoder: String): String = if (encoder.startsWith("hevc") || encoder == "libx265") "hevc" else "h264"

    /** A plan's video as the provider sends it, or none of it. */
    private val AS_SENT = setOf("copy", "none")
}
