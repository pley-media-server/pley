package app.pley.android.ui.tv.player

import android.content.res.Resources
import android.os.Build
import android.view.Display
import android.view.View
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.runtime.Composable
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.State
import androidx.compose.runtime.getValue
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.TvSafeArea
import app.pley.android.model.PlaybackMethod
import app.pley.android.model.StreamAction
import app.pley.android.model.format.audioFormatLabel
import app.pley.android.model.format.bitDepthLabel
import app.pley.android.model.format.codecLabel
import app.pley.android.model.format.frameRateLabel
import app.pley.android.model.format.formatBitrate
import app.pley.android.model.format.hdrFormatLabel
import app.pley.android.model.format.levelLabel
import app.pley.android.model.format.sampleRateLabel
import app.pley.android.player.PlaybackDetailsUi
import app.pley.android.player.PlayerUiState
import app.pley.android.player.SubtitleDelivery
import app.pley.android.player.engine.EngineState
import app.pley.android.player.engine.PlaybackStats
import app.pley.android.ui.tv.R
import app.pley.android.ui.tv.item.FactLine
import app.pley.android.ui.tv.item.MediaFact
import app.pley.android.ui.tv.item.MediaFactText
import app.pley.android.ui.tv.item.MediaFacts
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.withTimeoutOrNull
import java.util.Locale

/** The display's mode now, and the HDR types it takes in it (Display.HdrCapabilities.HDR_TYPE_*). */
@Immutable
internal data class DisplayNow(val width: Int, val height: Int, val refreshRate: Float, val hdrTypes: List<Int>)

/** What the readout is made of, read at most once a second. */
@Immutable
internal data class ReadoutInput(
    val details: PlaybackDetailsUi? = null,
    val stats: PlaybackStats? = null,
    val bufferedMs: Long = 0,
    /** "Original", "Automatic · 8 Mbps 1080p". */
    val quality: String = "",
    val display: DisplayNow? = null,
)

/**
 * The playback's details, the engine's stats, the buffer and the display,
 * sampled once a second while shown: the readout never recomposes faster.
 * Until the details come (they are asked for as it opens), it samples
 * again as soon as they do, so "Asking the server…" shows only while a
 * stream's live numbers are on their way, not for a second whatever comes.
 */
@Composable
internal fun rememberReadout(state: State<PlayerUiState>, engine: State<EngineState?>): State<ReadoutInput> {
    val view = LocalView.current
    return produceState(ReadoutInput(), state, engine, view) {
        while (true) {
            val s = state.value
            value = ReadoutInput(
                details = s.details,
                stats = engine.value?.stats,
                bufferedMs = (s.bufferedMs - s.positionMs).coerceAtLeast(0),
                quality = s.quality,
                display = displayNow(view),
            )
            if (s.details == null) {
                withTimeoutOrNull(READOUT_MS) { snapshotFlow { state.value.details }.first { it != null } }
            } else {
                delay(READOUT_MS)
            }
        }
    }
}

/** The window's display: its mode and the HDR types that mode takes. */
private fun displayNow(view: View): DisplayNow? {
    val display = view.display ?: return null
    val mode = display.mode
    val types = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) {
        mode.supportedHdrTypes.toList()
    } else {
        @Suppress("DEPRECATION")
        display.hdrCapabilities?.supportedHdrTypes?.toList().orEmpty()
    }
    return DisplayNow(mode.physicalWidth, mode.physicalHeight, mode.refreshRate, types)
}

/**
 * The stats for nerds, in words: how the server sends it and why, the
 * file, each stream in play in full, and what this TV does with them
 * (decoders, the display's mode and HDR, frames, buffer, network).
 */
internal object TvReadout {
    /** The Info panel's sections; empty until the server answered. */
    fun sections(r: Resources, input: ReadoutInput): List<MediaFacts> {
        val d = input.details
        val stats = input.stats
        return buildList {
            if (d != null) {
                add(MediaFacts(r.getString(R.string.tv_player_info_how), playback(r, d, input.quality)))
                d.file?.let { add(MediaFacts(r.getString(R.string.tv_readout_file), MediaFactText.file(r, it))) }
                d.video?.let { v ->
                    val facts = MediaFactText.video(r, v).toMutableList()
                    currentBitrate(r, d, stats)?.let { facts += MediaFact(r.getString(R.string.tv_readout_current_bitrate), it) }
                    add(MediaFacts(r.getString(R.string.tv_readout_video), facts))
                }
                d.audio?.let { a ->
                    val facts = MediaFactText.audio(r, a).toMutableList()
                    stats?.let { audioOutput(r, it) }?.let { facts += MediaFact(r.getString(R.string.tv_readout_output), it) }
                    add(MediaFacts(r.getString(R.string.tv_readout_audio), facts))
                }
                add(MediaFacts(r.getString(R.string.tv_readout_subtitles), subtitles(r, d)))
            }
            if (stats != null) {
                add(MediaFacts(r.getString(R.string.tv_player_info_device), device(r, stats, input)))
            } else {
                // No engine's stats (another engine): its own lines, "Label: value".
                val lines = d?.engine.orEmpty().map { line ->
                    val parts = line.split(": ", limit = 2)
                    if (parts.size == 2) MediaFact(parts[0], parts[1]) else MediaFact("", line)
                }
                if (lines.isNotEmpty()) add(MediaFacts(r.getString(R.string.tv_player_info_device), lines))
            }
        }
    }

    /** Playback details over the video: one line each for the way, the streams, and this TV. */
    fun compact(r: Resources, input: ReadoutInput): List<MediaFact> {
        val d = input.details
        val stats = input.stats
        return buildList {
            if (d != null) {
                add(MediaFact(r.getString(R.string.tv_readout_playback), d.method + (d.pipeline?.let { " · $it" } ?: "")))
                d.video?.let { v ->
                    val line = listOfNotNull(
                        codecLabel(v.codec) + (v.profile?.let { " $it" } ?: "") + (levelLabel(v.codec, v.level)?.let { " L$it" } ?: ""),
                        if ((v.width ?: 0) > 0 && (v.height ?: 0) > 0) "${v.width}×${v.height}" else null,
                        frameRateLabel(v.frameRate),
                        bitDepthLabel(v).ifEmpty { null },
                        hdrFormatLabel(v),
                    )
                    add(MediaFact(r.getString(R.string.tv_readout_video), line.joinToString(" · ")))
                }
                transcodeTarget(d)?.let { add(MediaFact(r.getString(R.string.tv_readout_target), it)) }
                d.audio?.let { a ->
                    val line = listOfNotNull(
                        audioFormatLabel(a).ifEmpty { null },
                        sampleRateLabel(a.sampleRate),
                        MediaFactText.language(a),
                        stats?.let { audioOutput(r, it) },
                    )
                    add(MediaFact(r.getString(R.string.tv_readout_audio), line.joinToString(" · ")))
                }
                val sub = d.subtitle
                add(
                    MediaFact(
                        r.getString(R.string.tv_readout_subtitles),
                        if (sub == null) r.getString(R.string.tv_readout_off) else listOfNotNull(MediaFactText.language(sub), MediaFactText.format(r, sub)).joinToString(" · "),
                    ),
                )
                val bitrate = listOf(formatBitrate(d.file?.bitrate), currentBitrate(r, d, stats).orEmpty()).filter { it.isNotEmpty() }.distinct()
                if (bitrate.isNotEmpty()) add(MediaFact(r.getString(R.string.tv_readout_bitrate), bitrate.joinToString(" → ")))
            }
            if (stats != null) {
                stats.videoDecoder?.let { add(MediaFact(r.getString(R.string.tv_readout_decoder), it + hardware(r, stats.videoDecoderHardware))) }
                input.display?.let { add(MediaFact(r.getString(R.string.tv_player_info_display), displayMode(r, it) + " · " + (hdrShown(stats, it, carriesHdr10Plus(input)) ?: "SDR"))) }
                add(MediaFact(r.getString(R.string.tv_readout_dropped_short), r.getString(R.string.tv_player_info_dropped_value, stats.droppedFrames, stats.renderedFrames + stats.droppedFrames)))
                val buffer = r.getString(R.string.tv_readout_buffer_compact, (input.bufferedMs / 1000).toInt())
                add(MediaFact(r.getString(R.string.tv_readout_buffer_short), buffer + (stats.bandwidth?.let { " · " + formatBitrate(it) } ?: "")))
            }
        }
    }

    private fun playback(r: Resources, d: PlaybackDetailsUi, quality: String): List<MediaFact> = buildList {
        add(MediaFact(r.getString(R.string.tv_readout_playback), d.method + (d.pipeline?.let { " · $it" } ?: "")))
        d.why?.takeIf { it.isNotBlank() }?.let { add(MediaFact(r.getString(R.string.tv_readout_why), it)) }
        d.rows.forEach { add(MediaFact(it.label, it.value)) }
        if (quality.isNotBlank()) add(MediaFact(r.getString(R.string.tv_readout_quality), quality))
    }

    private fun subtitles(r: Resources, d: PlaybackDetailsUi): List<MediaFact> {
        val sub = d.subtitle ?: return listOf(MediaFact(r.getString(R.string.tv_readout_track), r.getString(R.string.tv_readout_off)))
        val shown = when (d.subtitleDelivery) {
            SubtitleDelivery.File -> r.getString(R.string.tv_readout_delivery_file)
            SubtitleDelivery.Server -> r.getString(R.string.tv_readout_delivery_server)
            SubtitleDelivery.ServerPictures -> r.getString(R.string.tv_readout_delivery_pictures)
            SubtitleDelivery.BurnedIn -> r.getString(R.string.tv_readout_delivery_burned)
            null -> null
        }
        return MediaFactText.subtitle(r, sub) + listOfNotNull(shown?.let { MediaFact(r.getString(R.string.tv_readout_shown), it) })
    }

    private fun device(r: Resources, stats: PlaybackStats, input: ReadoutInput): List<MediaFact> = buildList {
        stats.videoDecoder?.let { add(MediaFact(r.getString(R.string.tv_player_info_video_decoder), it + hardware(r, stats.videoDecoderHardware))) }
        val picture = listOfNotNull(
            if (stats.width != null && stats.height != null) "${stats.width}×${stats.height}" else null,
            stats.frameRate?.let { frameRateLabel(it.toDouble()) },
            stats.bitDepth?.let { "$it-bit" },
            stats.dynamicRange,
            // The decoder's codec: HEVC for Dolby Vision played as its base layer.
            (stats.videoDecoderMime ?: stats.videoCodec?.substringBefore(' '))?.let(::codecName),
        )
        if (picture.isNotEmpty()) add(MediaFact(r.getString(R.string.tv_readout_decoded), picture.joinToString(" · ")))
        // What arrives (the file's stream, or the server's conversion): "hvc1.2.4.L153.B0", "dvhe.08.06".
        stats.videoCodec?.let(::codecString)?.let { add(MediaFact(r.getString(R.string.tv_readout_codec_string), it)) }
        stats.colorInfo?.let { add(MediaFact(r.getString(R.string.tv_player_info_picture), it)) }
        input.display?.let { display ->
            val mode = displayMode(r, display) + " · " + r.getString(if (stats.displayModeId != null) R.string.tv_player_info_display_switched else R.string.tv_player_info_display_kept)
            add(MediaFact(r.getString(R.string.tv_player_info_display), mode))
            add(MediaFact(r.getString(R.string.tv_readout_hdr_output), hdrOutput(r, stats, display, carriesHdr10Plus(input))))
        }
        stats.audioDecoder?.takeIf { !stats.passthrough }?.let { add(MediaFact(r.getString(R.string.tv_player_info_audio_decoder), it)) }
        add(MediaFact(r.getString(R.string.tv_player_info_dropped), r.getString(R.string.tv_player_info_dropped_value, stats.droppedFrames, stats.renderedFrames + stats.droppedFrames)))
        add(MediaFact(r.getString(R.string.tv_player_info_buffer), r.getString(R.string.tv_player_info_buffer_value, (input.bufferedMs / 1000).toInt())))
        stats.bandwidth?.let { add(MediaFact(r.getString(R.string.tv_readout_network), formatBitrate(it))) }
        add(MediaFact(r.getString(R.string.tv_player_info_rebuffers), stats.rebuffers.toString()))
        stats.firstFrameMs?.let { add(MediaFact(r.getString(R.string.tv_player_info_first_frame), r.getString(R.string.tv_player_info_first_frame_value, it.toInt()))) }
    }

    /** What a remux or transcode makes: "H.264 1080p · AAC 2.0", from the transcoder's numbers; `null` when nothing is converted. */
    fun transcodeTarget(d: PlaybackDetailsUi): String? {
        val t = d.transcode ?: return null
        if (d.playbackMethod == PlaybackMethod.Direct) return null
        return listOfNotNull(
            t.videoOut?.takeIf { t.video == StreamAction.Transcode }?.let { app.pley.android.player.session.PlaybackDetails.simplified(it) },
            t.audioOut?.takeIf { t.audio == StreamAction.Transcode },
        ).joinToString(" · ").ifEmpty { null }
    }

    /** The bitrate arriving now: an HLS variant's or the transcode's aim; in direct play the file's own. */
    private fun currentBitrate(r: Resources, d: PlaybackDetailsUi, stats: PlaybackStats?): String? {
        stats?.videoBitrate?.let { return formatBitrate(it.toLong() + (stats.audioBitrate ?: 0)) }
        d.transcode?.bitrateOut?.let { return formatBitrate(it) }
        return if (d.playbackMethod == PlaybackMethod.Direct) r.getString(R.string.tv_readout_as_file) else null
    }

    /** "Passed through as E-AC-3" or "Decoded to PCM". */
    private fun audioOutput(r: Resources, stats: PlaybackStats): String? {
        if (stats.audioEncoding == null && stats.audioCodec == null) return null
        return if (stats.passthrough) {
            r.getString(R.string.tv_readout_passthrough, stats.audioCodec?.let { codecName(it.substringBefore(' ')) } ?: "")
        } else {
            r.getString(R.string.tv_readout_decoded_pcm)
        }
    }

    /** "3840×2160 at 23.976 Hz". */
    private fun displayMode(r: Resources, d: DisplayNow): String =
        r.getString(R.string.tv_readout_display_mode, d.width, d.height, String.format(Locale.ROOT, "%.3f", d.refreshRate).trimEnd('0').trimEnd('.'))

    /**
     * Whether the display takes the video's HDR in its mode: "On (HDR10)",
     * "Off, Dolby Vision shown as SDR", "Off, the video is SDR". Dolby
     * Vision with a base layer falls back to HDR10 where the display takes that.
     */
    fun hdrOutput(r: Resources, stats: PlaybackStats, d: DisplayNow, hdr10plus: Boolean = false): String {
        val range = stats.dynamicRange?.takeIf { it in HDR_RANGES } ?: return r.getString(R.string.tv_readout_hdr_sdr)
        val shown = hdrShown(stats, d, hdr10plus)
        return if (shown != null) r.getString(R.string.tv_readout_hdr_on, shown) else r.getString(R.string.tv_readout_hdr_unsupported, range)
    }

    /**
     * The HDR the display shows the video in: its own, HDR10 for Dolby
     * Vision's base layer, HDR10+ for PQ video that carries it ([hdr10plus]:
     * the decoder says HDR10 either way) on a display that takes HDR10+, or
     * `null` (SDR).
     */
    private fun hdrShown(stats: PlaybackStats, d: DisplayNow, hdr10plus: Boolean = false): String? = when (val range = stats.dynamicRange) {
        "Dolby Vision" -> when {
            Display.HdrCapabilities.HDR_TYPE_DOLBY_VISION in d.hdrTypes -> range
            Display.HdrCapabilities.HDR_TYPE_HDR10 in d.hdrTypes -> "HDR10"
            else -> null
        }
        "HDR10" -> when {
            hdr10plus && Display.HdrCapabilities.HDR_TYPE_HDR10_PLUS in d.hdrTypes -> "HDR10+"
            Display.HdrCapabilities.HDR_TYPE_HDR10 in d.hdrTypes || Display.HdrCapabilities.HDR_TYPE_HDR10_PLUS in d.hdrTypes -> range
            else -> null
        }
        "HLG" -> range.takeIf { Display.HdrCapabilities.HDR_TYPE_HLG in d.hdrTypes }
        else -> null
    }

    /** The stream playing carries HDR10+ (alone, or with Dolby Vision). */
    private fun carriesHdr10Plus(input: ReadoutInput): Boolean = input.details?.video?.hdr10plus == true

    private val HDR_RANGES = setOf("Dolby Vision", "HDR10", "HLG")

    private fun hardware(r: Resources, hardware: Boolean?): String = when (hardware) {
        true -> " (" + r.getString(R.string.tv_player_info_hardware) + ")"
        false -> " (" + r.getString(R.string.tv_player_info_software) + ")"
        null -> ""
    }

    /** The codec string after the MIME type: "hvc1.2.4.L153.B0", "dvhe.08.06"; `null` without one. */
    private fun codecString(videoCodec: String): String? = videoCodec.substringAfter(' ', "").takeIf { it.isNotBlank() }
}

/** How often the readout reads its numbers. */
private const val READOUT_MS = 1_000L

/**
 * Playback details over the video (Settings → Playback details): the
 * compact readout at the top left, inside the safe area, read once a
 * second. The screen hides it while the controls or a panel show.
 */
@Composable
internal fun BoxScope.TvPlaybackDetails(state: State<PlayerUiState>, engine: State<EngineState?>) {
    val input by rememberReadout(state, engine)
    val resources = LocalResources.current
    val lines = remember(input, resources) { TvReadout.compact(resources, input) }
    if (lines.isEmpty()) return
    Column(
        Modifier
            .align(Alignment.TopStart)
            .padding(start = TvSafeArea.Horizontal, top = TvSafeArea.Vertical)
            .widthIn(max = DETAILS_WIDTH)
            .background(PleyColors.Bg.copy(alpha = 0.72f), PleyShape.Card)
            .padding(horizontal = 12.dp, vertical = 8.dp)
            .testTag(TAG_DETAILS),
    ) {
        lines.forEach { FactLine(it, labelWidth = DETAILS_LABEL_WIDTH) }
    }
}

private val DETAILS_WIDTH = 480.dp
private val DETAILS_LABEL_WIDTH = 96.dp

internal const val TAG_DETAILS = "pley.tv.player.details"
