package app.pley.android.player.session

import androidx.annotation.OptIn
import androidx.media3.common.C
import androidx.media3.common.Format
import androidx.media3.common.MimeTypes
import androidx.media3.common.PlaybackException
import androidx.media3.common.Player
import androidx.media3.common.TrackSelectionOverride
import androidx.media3.common.Tracks
import androidx.media3.common.util.UnstableApi
import androidx.media3.common.util.Util
import androidx.media3.datasource.HttpDataSource
import androidx.media3.exoplayer.DecoderCounters
import androidx.media3.exoplayer.ExoPlaybackException
import androidx.media3.exoplayer.ExoPlayer
import androidx.media3.exoplayer.analytics.AnalyticsListener
import androidx.media3.extractor.metadata.Chapter as ContainerChapter
import app.pley.android.model.Chapter
import app.pley.android.model.DynamicRange
import app.pley.android.model.PleyJson
import app.pley.android.player.EngineSource
import app.pley.android.player.PlayerEngine
import java.util.Locale

/**
 * [EngineLink] over a [PlayerEngine]: Media3's player events sorted for the
 * session, tracks chosen by their order in the file, and the frame counts
 * and decoders of an ExoPlayer for the details.
 */
@OptIn(UnstableApi::class)
class Media3EngineLink(override val engine: PlayerEngine) : EngineLink {
    private val player: Player get() = engine.player
    private var listener: EngineLink.Listener? = null

    /** A track choice waiting for the media's tracks. */
    private data class Wanted(val ordinal: Int?, val language: String?)

    private var wantedAudio: Wanted? = null
    private var wantedText: Wanted? = Wanted(null, null)
    private var videoDecoder: String? = null
    private var audioDecoder: String? = null
    private var bandwidth: Long? = null
    private var speed = 1f
    private var reportedPlayWhenReady = engine.player.playWhenReady

    private val events = object : Player.Listener {
        override fun onPlaybackStateChanged(playbackState: Int) {
            listener?.onStatus(statusOf(playbackState))
        }

        override fun onPlayWhenReadyChanged(playWhenReady: Boolean, reason: Int) {
            // Media3 also reports a new reason alone (paused already, now "becoming noisy"): nothing for the session.
            if (playWhenReady == reportedPlayWhenReady) return
            reportedPlayWhenReady = playWhenReady
            listener?.onPlayWhenReady(playWhenReady)
        }

        override fun onIsPlayingChanged(isPlaying: Boolean) {
            listener?.onPlaying(isPlaying)
        }

        override fun onTracksChanged(tracks: Tracks) {
            if (tracks.groups.isEmpty()) return
            apply(tracks)
            listener?.onTracks(describe(tracks))
        }

        override fun onPlayerError(error: PlaybackException) {
            listener?.onError(errorOf(error))
        }
    }

    private val analytics = object : AnalyticsListener {
        override fun onVideoDecoderInitialized(
            eventTime: AnalyticsListener.EventTime,
            decoderName: String,
            initializedTimestampMs: Long,
            initializationDurationMs: Long,
        ) {
            videoDecoder = decoderName
        }

        override fun onAudioDecoderInitialized(
            eventTime: AnalyticsListener.EventTime,
            decoderName: String,
            initializedTimestampMs: Long,
            initializationDurationMs: Long,
        ) {
            audioDecoder = decoderName
        }

        override fun onBandwidthEstimate(eventTime: AnalyticsListener.EventTime, totalLoadTimeMs: Int, totalBytesLoaded: Long, bitrateEstimate: Long) {
            if (bitrateEstimate > 0) bandwidth = bitrateEstimate / 1000
        }
    }

    init {
        player.addListener(events)
        (player as? ExoPlayer)?.addAnalyticsListener(analytics)
    }

    override val positionMs: Long get() = player.currentPosition.coerceAtLeast(0)
    override val bufferedMs: Long get() = player.bufferedPosition.coerceAtLeast(0)
    override val durationMs: Long get() = player.duration.takeIf { it != C.TIME_UNSET && it > 0 } ?: 0
    override val status: EngineStatus get() = statusOf(player.playbackState)
    override val playWhenReady: Boolean get() = player.playWhenReady
    override val reconnecting: Boolean get() = engine.state.value.reconnecting
    override val bandwidthKbps: Long? get() = bandwidth
    override val soundUndecodable: Boolean get() = engine.state.value.soundUndecodable

    override fun setListener(listener: EngineLink.Listener?) {
        this.listener = listener
    }

    override fun prepare(source: EngineSource, startMs: Long, playWhenReady: Boolean) {
        wantedAudio = null
        wantedText = Wanted(null, null)
        videoDecoder = null
        audioDecoder = null
        // Off until a track is chosen: a file's default or forced subtitles don't show by themselves.
        player.trackSelectionParameters = player.trackSelectionParameters.buildUpon()
            .clearOverridesOfType(C.TRACK_TYPE_AUDIO)
            .clearOverridesOfType(C.TRACK_TYPE_TEXT)
            .setTrackTypeDisabled(C.TRACK_TYPE_TEXT, true)
            .build()
        engine.prepare(source, startMs, playWhenReady)
        if (player.playbackParameters.speed != speed) player.setPlaybackSpeed(speed)
    }

    override fun setSpeed(speed: Float) {
        this.speed = speed
        // PlaybackParameters' pitch stays 1: the sound is stretched in time, not raised.
        player.setPlaybackSpeed(speed)
    }

    override fun setAudioDelay(delayMs: Long) = engine.setAudioDelay(delayMs)

    override fun setSoundNeeds(decoded: Boolean, untunneled: Boolean, reload: Boolean) = engine.setSoundNeeds(decoded, untunneled, reload)

    override fun play() {
        if (player.playbackState == Player.STATE_ENDED) player.seekTo(0)
        player.play()
    }

    override fun pause() = player.pause()

    override fun stop() {
        player.pause()
        player.stop()
    }

    override fun seekTo(positionMs: Long) = player.seekTo(positionMs.coerceAtLeast(0))

    override fun selectAudio(ordinal: Int, language: String?) {
        wantedAudio = Wanted(ordinal, language)
        apply(player.currentTracks)
    }

    override fun selectText(ordinal: Int?, language: String?) {
        wantedText = Wanted(ordinal, language)
        apply(player.currentTracks)
    }

    override fun frameCounts(): FrameCounts? {
        val counters: DecoderCounters = (player as? ExoPlayer)?.videoDecoderCounters ?: return null
        counters.ensureUpdated()
        return FrameCounts(
            rendered = counters.renderedOutputBufferCount.toLong(),
            dropped = counters.droppedBufferCount.toLong(),
            maxConsecutiveDropped = counters.maxConsecutiveDroppedBufferCount,
        )
    }

    override fun stats(): List<String> {
        val exo = player as? ExoPlayer ?: return emptyList()
        val lines = mutableListOf<String>()
        exo.videoFormat?.let { f ->
            val rate = if (f.frameRate > 0) String.format(Locale.ROOT, " %.3f fps", f.frameRate) else ""
            lines += "Video: ${f.sampleMimeType ?: f.codecs ?: "?"} ${f.width}×${f.height}$rate"
        }
        videoDecoder?.let { lines += "Video decoder: $it" }
        exo.audioFormat?.let { f -> lines += "Audio: ${f.sampleMimeType ?: f.codecs ?: "?"} ${f.channelCount} ch" }
        audioDecoder?.let { lines += "Audio decoder: $it" }
        frameCounts()?.let { lines += "Dropped frames: ${it.dropped} of ${it.rendered + it.dropped}" }
        return lines
    }

    override fun release() {
        listener = null
        player.removeListener(events)
        (player as? ExoPlayer)?.removeAnalyticsListener(analytics)
        engine.release()
    }

    /** Applies the wanted tracks to [tracks], once the media has them. */
    private fun apply(tracks: Tracks) {
        if (tracks.groups.isEmpty()) return
        val builder = player.trackSelectionParameters.buildUpon()
        var changed = false
        wantedAudio?.let { wanted ->
            val group = pick(tracks, C.TRACK_TYPE_AUDIO, wanted)
            if (group != null) {
                builder.clearOverridesOfType(C.TRACK_TYPE_AUDIO).addOverride(TrackSelectionOverride(group.mediaTrackGroup, 0))
                changed = true
            }
        }
        wantedText?.let { wanted ->
            val group = wanted.ordinal?.let { pick(tracks, C.TRACK_TYPE_TEXT, wanted) }
            if (group == null) {
                builder.clearOverridesOfType(C.TRACK_TYPE_TEXT).setTrackTypeDisabled(C.TRACK_TYPE_TEXT, true)
            } else {
                builder.clearOverridesOfType(C.TRACK_TYPE_TEXT)
                    .setTrackTypeDisabled(C.TRACK_TYPE_TEXT, false)
                    .addOverride(TrackSelectionOverride(group.mediaTrackGroup, 0))
            }
            changed = true
        }
        if (changed) {
            val next = builder.build()
            if (next != player.trackSelectionParameters) player.trackSelectionParameters = next
        }
    }

    private fun describe(tracks: Tracks): EngineTracks {
        fun of(type: Int) = tracks.groups.filter { it.type == type }.map { group ->
            val format = group.getTrackFormat(0)
            EngineTrack(
                language = format.language,
                // Text read as the media loads carries its own MIME type in its codecs.
                mimeType = if (format.sampleMimeType == MimeTypes.APPLICATION_MEDIA3_CUES) format.codecs ?: format.sampleMimeType else format.sampleMimeType,
                channels = format.channelCount.takeIf { it != Format.NO_VALUE },
                supported = group.isSupported,
                label = format.label,
                default = format.selectionFlags and C.SELECTION_FLAG_DEFAULT != 0,
                forced = format.selectionFlags and C.SELECTION_FLAG_FORCED != 0,
                selected = group.isSelected,
                codecs = format.codecs,
                width = format.width.coerceAtLeast(0),
                height = format.height.coerceAtLeast(0),
                frameRate = format.frameRate.coerceAtLeast(0f),
                range = if (type == C.TRACK_TYPE_VIDEO) rangeOf(format) else null,
                bitDepth = format.colorInfo?.lumaBitdepth?.takeIf { it != Format.NO_VALUE },
            )
        }
        return EngineTracks(video = of(C.TRACK_TYPE_VIDEO), audio = of(C.TRACK_TYPE_AUDIO), text = of(C.TRACK_TYPE_TEXT), chapters = chaptersOf(tracks))
    }

    companion object {
        internal fun statusOf(state: Int): EngineStatus = when (state) {
            Player.STATE_BUFFERING -> EngineStatus.Buffering
            Player.STATE_READY -> EngineStatus.Ready
            Player.STATE_ENDED -> EngineStatus.Ended
            else -> EngineStatus.Idle
        }

        /**
         * The group of [type] at the wanted ordinal, cross-checked on its
         * language: when the container's order and the server's disagree
         * (a language that doesn't match), the first track of the wanted
         * language plays instead.
         */
        private fun pick(tracks: Tracks, type: Int, wanted: Wanted): Tracks.Group? {
            val groups = tracks.groups.filter { it.type == type }
            val ordinal = wanted.ordinal ?: return null
            val at = groups.getOrNull(ordinal)
            val language = wanted.language?.let(::normalized)
            if (language == null) return at
            val atLanguage = at?.getTrackFormat(0)?.language?.let(::normalized)
            if (at != null && (atLanguage == null || atLanguage == language)) return at
            return groups.firstOrNull { it.getTrackFormat(0).language?.let(::normalized) == language } ?: at
        }

        private fun normalized(code: String): String = Util.normalizeLanguageCode(code)

        /** A video's dynamic range as its format says it: Dolby Vision by its MIME type, HDR10 and HLG by their transfer. */
        private fun rangeOf(format: Format): DynamicRange = when {
            format.sampleMimeType == MimeTypes.VIDEO_DOLBY_VISION -> DynamicRange.DolbyVision
            format.colorInfo?.colorTransfer == C.COLOR_TRANSFER_ST2084 -> DynamicRange.Hdr10
            format.colorInfo?.colorTransfer == C.COLOR_TRANSFER_HLG -> DynamicRange.Hlg
            else -> DynamicRange.Sdr
        }

        /** The chapters Media3 read from the container, which it puts on the tracks' formats: once each, by start. */
        private fun chaptersOf(tracks: Tracks): List<Chapter> {
            val found = sortedMapOf<Long, Chapter>()
            for (group in tracks.groups) {
                for (track in 0 until group.length) {
                    val metadata = group.getTrackFormat(track).metadata ?: continue
                    for (n in 0 until metadata.length()) {
                        val entry = metadata.get(n) as? ContainerChapter ?: continue
                        if (entry.isHidden || entry.startTimeMs < 0 || found.size >= MAX_CHAPTERS) continue
                        found.getOrPut(entry.startTimeMs) {
                            Chapter(entry.startTimeMs, entry.endTimeMs.takeIf { it > entry.startTimeMs } ?: 0, entry.title?.value?.takeIf { it.isNotBlank() })
                        }
                    }
                }
            }
            return found.values.toList()
        }

        /** The most chapters taken from a file: more is a file that isn't right. */
        private const val MAX_CHAPTERS = 256

        /** What a Media3 error means for the session. */
        fun errorOf(error: PlaybackException): EngineError {
            val detail = "${error.errorCodeName}: ${error.message}".trimEnd(' ', ':')
            val code = error.errorCode
            return when {
                code == PlaybackException.ERROR_CODE_IO_BAD_HTTP_STATUS || code == PlaybackException.ERROR_CODE_IO_FILE_NOT_FOUND -> {
                    val http = error.cause as? HttpDataSource.InvalidResponseCodeException
                    val status = http?.responseCode ?: if (code == PlaybackException.ERROR_CODE_IO_FILE_NOT_FOUND) 404 else 0
                    if (status == GONE) EngineError.Stopped(stopMessage(http?.responseBody), detail) else EngineError.Http(status, detail)
                }
                code == PlaybackException.ERROR_CODE_PARSING_MANIFEST_MALFORMED ||
                    code == PlaybackException.ERROR_CODE_PARSING_MANIFEST_UNSUPPORTED -> EngineError.Playlist(detail)
                code == PlaybackException.ERROR_CODE_PARSING_CONTAINER_MALFORMED ||
                    code == PlaybackException.ERROR_CODE_PARSING_CONTAINER_UNSUPPORTED ->
                    EngineError.Unsupported(audio = false, container = true, detail = detail)
                code in DECODER_ERRORS -> {
                    val mime = (error as? ExoPlaybackException)?.rendererFormat?.sampleMimeType
                    EngineError.Unsupported(audio = MimeTypes.isAudio(mime), container = false, detail = detail)
                }
                code in AUDIO_OUTPUT_ERRORS -> EngineError.Unsupported(audio = true, container = false, detail = detail)
                code in NETWORK_ERRORS -> EngineError.Network(detail)
                else -> EngineError.Other(detail)
            }
        }

        /** The server stopped the session: its requests answer this. */
        private const val GONE = 410

        /** The `message` of a 410's body (`{ error: "session_stopped", message }`); `null` without one. */
        internal fun stopMessage(body: ByteArray?): String? {
            if (body == null || body.isEmpty()) return null
            return try {
                val json = PleyJson.parseToJsonElement(body.decodeToString()) as? kotlinx.serialization.json.JsonObject ?: return null
                (json["message"] as? kotlinx.serialization.json.JsonPrimitive)?.takeIf { it.isString }?.content?.takeIf { it.isNotBlank() }
            } catch (_: Exception) {
                null
            }
        }

        private val DECODER_ERRORS = setOf(
            PlaybackException.ERROR_CODE_DECODER_INIT_FAILED,
            PlaybackException.ERROR_CODE_DECODER_QUERY_FAILED,
            PlaybackException.ERROR_CODE_DECODING_FAILED,
            PlaybackException.ERROR_CODE_DECODING_FORMAT_EXCEEDS_CAPABILITIES,
            PlaybackException.ERROR_CODE_DECODING_FORMAT_UNSUPPORTED,
        )

        private val AUDIO_OUTPUT_ERRORS = setOf(
            PlaybackException.ERROR_CODE_AUDIO_TRACK_INIT_FAILED,
            PlaybackException.ERROR_CODE_AUDIO_TRACK_WRITE_FAILED,
        )

        private val NETWORK_ERRORS = setOf(
            PlaybackException.ERROR_CODE_IO_UNSPECIFIED,
            PlaybackException.ERROR_CODE_IO_NETWORK_CONNECTION_FAILED,
            PlaybackException.ERROR_CODE_IO_NETWORK_CONNECTION_TIMEOUT,
            PlaybackException.ERROR_CODE_IO_INVALID_HTTP_CONTENT_TYPE,
            PlaybackException.ERROR_CODE_IO_READ_POSITION_OUT_OF_RANGE,
            PlaybackException.ERROR_CODE_TIMEOUT,
        )
    }
}
