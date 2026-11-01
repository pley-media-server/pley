package app.pley.android.player.engine

import androidx.annotation.OptIn
import androidx.media3.common.C
import androidx.media3.common.ColorInfo
import androidx.media3.common.Format
import androidx.media3.common.MimeTypes
import androidx.media3.common.Player
import androidx.media3.common.util.UnstableApi
import androidx.media3.common.util.Util
import androidx.media3.exoplayer.DecoderCounters
import androidx.media3.exoplayer.DecoderReuseEvaluation
import androidx.media3.exoplayer.analytics.AnalyticsListener
import androidx.media3.exoplayer.audio.AudioSink
import androidx.tracing.Trace
import app.pley.android.common.TraceNames

/**
 * Tells a decoder that can't keep up: over 1 % of frames dropped in the
 * last 10 s (of at least 100 frames), or 10 dropped in a row. Fed the
 * decoder's running counts once a second while playing.
 */
class StruggleDetector(
    private val windowMs: Long = WINDOW_MS,
    private val ratio: Double = RATIO,
    private val inARow: Int = IN_A_ROW,
) {
    private data class Sample(val atMs: Long, val rendered: Int, val dropped: Int)

    private val samples = ArrayDeque<Sample>()

    /** Adds the counts at [atMs]; `true` when the decoder struggles. */
    fun add(atMs: Long, rendered: Int, dropped: Int, maxConsecutiveDropped: Int): Boolean {
        samples.addLast(Sample(atMs, rendered, dropped))
        while (samples.size > 1 && atMs - samples[1].atMs >= windowMs) samples.removeFirst()
        if (maxConsecutiveDropped >= inARow) return true
        val first = samples.first()
        val droppedNow = dropped - first.dropped
        val frames = (rendered - first.rendered) + droppedNow
        return frames >= MIN_FRAMES && droppedNow > frames * ratio
    }

    /** Starts over (a new item, a seek: counts restart). */
    fun reset() = samples.clear()

    companion object {
        const val WINDOW_MS = 10_000L
        const val RATIO = 0.01
        const val IN_A_ROW = 10
        const val MIN_FRAMES = 100
    }
}

/**
 * What a playback did, for Playback details, Diagnostics and benchmarks:
 * the first frame (which ends the trace section pley.play.tapToFirstFrame),
 * the decoders and whether the video one is hardware, the formats with
 * their colour and frame rate, the audio encoding and passthrough, dropped
 * frames and stalls.
 */
@OptIn(markerClass = [UnstableApi::class])
class PlaybackAnalytics(
    private val now: () -> Long,
    /** Whether a decoder (by name) is hardware; `null` when unknown. */
    private val isHardware: (String) -> Boolean?,
    /** The MIME type a decoder (by name) decodes; `null` when unknown. */
    private val decoderMime: (String) -> String? = { null },
    private val onChange: (PlaybackStats) -> Unit,
) : AnalyticsListener {
    var stats = PlaybackStats()
        private set

    private var preparedAt = 0L
    private var seeking = false
    private var hasBeenReady = false

    /** The video's format as it arrives, judged again once its decoder is known. */
    private var videoFormat: Format? = null

    /** Called at each prepare: a new item starts counting. */
    fun onPrepare(displayModeId: Int?) {
        preparedAt = now()
        seeking = false
        hasBeenReady = false
        videoFormat = null
        update(PlaybackStats(displayModeId = displayModeId))
    }

    fun onDisplayMode(modeId: Int?) = update(stats.copy(displayModeId = modeId))

    /** The decoder's counts, read once a second while playing. */
    fun onCounters(counters: DecoderCounters) {
        counters.ensureUpdated()
        update(
            stats.copy(
                renderedFrames = counters.renderedOutputBufferCount,
                droppedFrames = counters.droppedBufferCount,
                maxConsecutiveDropped = counters.maxConsecutiveDroppedBufferCount,
            ),
        )
    }

    override fun onRenderedFirstFrame(eventTime: AnalyticsListener.EventTime, output: Any, renderTimeMs: Long) {
        SeekTrace.frame()
        if (stats.firstFrameMs != null) return
        PlayTrace.firstFrame()
        update(stats.copy(firstFrameMs = (now() - preparedAt).coerceAtLeast(0)))
    }

    override fun onVideoDecoderInitialized(
        eventTime: AnalyticsListener.EventTime,
        decoderName: String,
        initializedTimestampMs: Long,
        initializationDurationMs: Long,
    ) {
        val mime = decoderMime(decoderName)
        update(
            stats.copy(
                videoDecoder = decoderName,
                videoDecoderHardware = isHardware(decoderName),
                videoDecoderMime = mime,
                dynamicRange = videoFormat?.let { rangeOf(it, mime) } ?: stats.dynamicRange,
            ),
        )
    }

    override fun onAudioDecoderInitialized(
        eventTime: AnalyticsListener.EventTime,
        decoderName: String,
        initializedTimestampMs: Long,
        initializationDurationMs: Long,
    ) = update(stats.copy(audioDecoder = decoderName))

    override fun onVideoInputFormatChanged(eventTime: AnalyticsListener.EventTime, format: Format, decoderReuseEvaluation: DecoderReuseEvaluation?) {
        videoFormat = format
        update(
            stats.copy(
                videoCodec = listOfNotNull(format.sampleMimeType, format.codecs).joinToString(" "),
                width = format.width.takeIf { it > 0 },
                height = format.height.takeIf { it > 0 },
                frameRate = format.frameRate.takeIf { it > 0 },
                dynamicRange = rangeOf(format, stats.videoDecoderMime),
                colorInfo = format.colorInfo?.takeIf { it.isValid }?.let(ColorInfo::toLogString),
                bitDepth = format.colorInfo?.lumaBitdepth?.takeIf { it > 0 },
                videoBitrate = format.bitrate.takeIf { it > 0 },
            ),
        )
    }

    override fun onAudioInputFormatChanged(eventTime: AnalyticsListener.EventTime, format: Format, decoderReuseEvaluation: DecoderReuseEvaluation?) {
        update(
            stats.copy(
                audioCodec = listOfNotNull(format.sampleMimeType, format.codecs).joinToString(" "),
                audioChannels = format.channelCount.takeIf { it > 0 },
                audioSampleRate = format.sampleRate.takeIf { it > 0 },
                audioBitrate = format.bitrate.takeIf { it > 0 },
            ),
        )
    }

    /** The network's speed: kept when it moves less than 10 %, so the stats don't change with every load. */
    override fun onBandwidthEstimate(eventTime: AnalyticsListener.EventTime, totalLoadTimeMs: Int, totalBytesLoaded: Long, bitrateEstimate: Long) {
        val was = stats.bandwidth
        if (bitrateEstimate <= 0) return
        if (was == null || kotlin.math.abs(bitrateEstimate - was) > was / 10) update(stats.copy(bandwidth = bitrateEstimate))
    }

    override fun onAudioTrackInitialized(eventTime: AnalyticsListener.EventTime, audioTrackConfig: AudioSink.AudioTrackConfig) {
        update(
            stats.copy(
                audioEncoding = audioTrackConfig.encoding,
                passthrough = isPassthrough(audioTrackConfig.encoding),
                tunneling = audioTrackConfig.tunneling,
            ),
        )
    }

    override fun onPositionDiscontinuity(
        eventTime: AnalyticsListener.EventTime,
        oldPosition: Player.PositionInfo,
        newPosition: Player.PositionInfo,
        reason: Int,
    ) {
        if (reason == Player.DISCONTINUITY_REASON_SEEK || reason == Player.DISCONTINUITY_REASON_SEEK_ADJUSTMENT) seeking = true
        if (reason == Player.DISCONTINUITY_REASON_SEEK) SeekTrace.seeked()
    }

    override fun onPlaybackStateChanged(eventTime: AnalyticsListener.EventTime, state: Int) {
        when (state) {
            Player.STATE_READY -> {
                hasBeenReady = true
                seeking = false
            }
            Player.STATE_BUFFERING -> if (hasBeenReady && !seeking) update(stats.copy(rebuffers = stats.rebuffers + 1))
            else -> Unit
        }
    }

    private fun update(next: PlaybackStats) {
        if (next != stats) {
            stats = next
            onChange(next)
        }
    }

    companion object {
        /** Encoded audio sent as it is, rather than decoded to PCM. */
        fun isPassthrough(encoding: Int): Boolean =
            encoding != Format.NO_VALUE && encoding != C.ENCODING_INVALID && !Util.isEncodingLinearPcm(encoding)

        /**
         * The range [decoderMime] makes of [format]: Dolby Vision decoded by
         * another codec's decoder (Media3's fallback to the base layer) is
         * that layer's, HDR10, HLG or SDR.
         */
        fun rangeOf(format: Format, decoderMime: String? = null): String = when {
            format.sampleMimeType == MimeTypes.VIDEO_DOLBY_VISION && (decoderMime == null || decoderMime == MimeTypes.VIDEO_DOLBY_VISION) -> "Dolby Vision"
            format.colorInfo?.colorTransfer == C.COLOR_TRANSFER_ST2084 -> "HDR10"
            format.colorInfo?.colorTransfer == C.COLOR_TRANSFER_HLG -> "HLG"
            else -> "SDR"
        }
    }
}

/**
 * The trace section from pressing play to the first video frame
 * (pley.play.tapToFirstFrame), an async section since it spans screens:
 * the launcher calls [tapped], the engine ends it.
 */
object PlayTrace {
    private const val COOKIE = 0x706c6579

    @Volatile
    private var open = false

    fun tapped() {
        if (open) Trace.endAsyncSection(TraceNames.PLAY_TAP_TO_FIRST_FRAME, COOKIE)
        Trace.beginAsyncSection(TraceNames.PLAY_TAP_TO_FIRST_FRAME, COOKIE)
        open = true
    }

    fun firstFrame() {
        if (!open) return
        open = false
        Trace.endAsyncSection(TraceNames.PLAY_TAP_TO_FIRST_FRAME, COOKIE)
    }
}

/**
 * The trace section from a seek to the first video frame at the new place
 * (pley.play.seekToFrame): a seek before that frame starts it again.
 */
object SeekTrace {
    private const val COOKIE = 0x7365656b

    @Volatile
    private var open = false

    fun seeked() {
        if (open) Trace.endAsyncSection(TraceNames.PLAY_SEEK_TO_FRAME, COOKIE)
        Trace.beginAsyncSection(TraceNames.PLAY_SEEK_TO_FRAME, COOKIE)
        open = true
    }

    fun frame() {
        if (!open) return
        open = false
        Trace.endAsyncSection(TraceNames.PLAY_SEEK_TO_FRAME, COOKIE)
    }
}
