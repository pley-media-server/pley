package app.pley.android.player.engine

/**
 * What an engine knows beyond the player's own state, in one immutable
 * value: the session reads it to show "Matching refresh rate…", to ask the
 * server for help, and to fill Playback details.
 */
data class EngineState(
    /** The TV is changing its display mode for the video: playback waits, the picture may go dark. */
    val switchingDisplayMode: Boolean = false,
    /** The refresh rate the display is switching to (23.976…), while [switchingDisplayMode]. */
    val switchingToHz: Float? = null,
    /**
     * The TV didn't change its mode twice in a row on this display, so Auto
     * stopped asking: show once where the device's own setting is.
     */
    val frameRateHint: Boolean = false,
    /** Tracks this device can't play as they are: the server has to remux or transcode. */
    val needsServerHelp: NeedsServerHelp? = null,
    /** The video decoder drops frames: over 1 % in 10 s, or 10 in a row. */
    val struggling: Boolean = false,
    /** The audio output failed and the engine started it again at the same position. */
    val audioRestarts: Int = 0,
    /**
     * The server is out of reach: a load failed for it and nothing came in
     * since. The engine keeps trying from where playback stood (MediaRetries).
     */
    val reconnecting: Boolean = false,
    val stats: PlaybackStats = PlaybackStats(),
)

/**
 * The file has a video or an audio stream, and the device can play none of
 * them: the server must remux (audio) or transcode (video).
 */
data class NeedsServerHelp(val video: Boolean, val audio: Boolean)

/** What Playback details and Diagnostics show of a playback, and what the benchmarks read. */
data class PlaybackStats(
    /** From [app.pley.android.player.PlayerEngine.prepare] to the first frame drawn. */
    val firstFrameMs: Long? = null,
    val videoDecoder: String? = null,
    /** Whether [videoDecoder] is a hardware decoder; `null` when unknown. */
    val videoDecoderHardware: Boolean? = null,
    /** The MIME type [videoDecoder] decodes: `video/hevc` for Dolby Vision played as its base layer; `null` when unknown. */
    val videoDecoderMime: String? = null,
    val audioDecoder: String? = null,
    /** The video's MIME type and codec string: `video/hevc hvc1.2.4.L153`. */
    val videoCodec: String? = null,
    val width: Int? = null,
    val height: Int? = null,
    val frameRate: Float? = null,
    /**
     * "HDR10", "HLG", "Dolby Vision", "SDR", as the decoder makes it (Dolby
     * Vision decoded as its base layer is that layer's); `null` before the
     * video's format is known.
     */
    val dynamicRange: String? = null,
    /** The colour information as Media3 reads it (primaries, transfer, range). */
    val colorInfo: String? = null,
    /** The video's bit depth as decoded (luma), when the format says. */
    val bitDepth: Int? = null,
    /** The video track's bitrate as the media declares it (an HLS variant's, a container's), bits per second. */
    val videoBitrate: Int? = null,
    val audioCodec: String? = null,
    val audioChannels: Int? = null,
    /** Hz. */
    val audioSampleRate: Int? = null,
    /** The audio track's declared bitrate, bits per second. */
    val audioBitrate: Int? = null,
    /** The network's speed as the player measures it, bits per second. */
    val bandwidth: Long? = null,
    /** Media3's C.ENCODING_* of the audio output. */
    val audioEncoding: Int? = null,
    /** Encoded audio (AC3, E-AC3, TrueHD, DTS…) goes to the TV or receiver as it is. */
    val passthrough: Boolean = false,
    /** Tunneled playback: the TV times the frames against the sound itself. */
    val tunneling: Boolean = false,
    val renderedFrames: Int = 0,
    val droppedFrames: Int = 0,
    val maxConsecutiveDropped: Int = 0,
    /** Stalls while playing, seeks aside. */
    val rebuffers: Int = 0,
    /** The display mode the engine asked for (its id), `null` when it left the mode alone. */
    val displayModeId: Int? = null,
)
