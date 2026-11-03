package app.pley.android.player.session

import app.pley.android.model.Chapter
import app.pley.android.model.DynamicRange
import app.pley.android.model.MediaStream
import app.pley.android.player.EngineSource
import app.pley.android.player.PlayerEngine

/** Where the engine stands, as the session needs to know it. */
enum class EngineStatus {
    /** Nothing loaded, or it stopped on an error. */
    Idle,

    /** Waiting for media. */
    Buffering,

    /** Media to play; playing when it is meant to. */
    Ready,

    /** The end of the media. */
    Ended,
}

/**
 * Why the engine stopped, sorted as the recovery ladder reads it. [detail]
 * is for logs and the details panel, not a sentence for people.
 */
sealed interface EngineError {
    val detail: String

    /**
     * The device can't play what it was given: a decoder failed or refused
     * the format, or the file couldn't be read. [audio] when the audio was
     * the trouble, [container] when the file itself couldn't be opened.
     */
    data class Unsupported(val audio: Boolean, val container: Boolean, override val detail: String) : EngineError

    /** The server answered [status] for the media: 404 and 410 mean the session expired. */
    data class Http(val status: Int, override val detail: String) : EngineError

    /**
     * The server stopped the session (410 Gone, `session_stopped`): it won't
     * play again. [message] is the one given, if any.
     */
    data class Stopped(val message: String?, override val detail: String) : EngineError

    /** The HLS playlist couldn't be loaded or read: only a new session helps. */
    data class Playlist(override val detail: String) : EngineError

    /** The server couldn't be reached, after the engine's own retries. */
    data class Network(override val detail: String) : EngineError

    data class Other(override val detail: String) : EngineError
}

/** A track the engine found in the media, in the media's order within its kind. */
data class EngineTrack(
    /** ISO 639 as the container says it; `null` when it doesn't. */
    val language: String?,
    /** The sample MIME type ("audio/eac3"); for text read as the media loads, its own ("application/x-subrip"). */
    val mimeType: String?,
    val channels: Int?,
    /** A decoder (or the passthrough output) takes it. */
    val supported: Boolean,
    /** Its name in the file ("Commentary"); `null` without one. */
    val label: String? = null,
    /** The file flags it as its default, or (subtitles) as forced. */
    val default: Boolean = false,
    val forced: Boolean = false,
    /** It plays, or shows, now. */
    val selected: Boolean = false,
    /** RFC 6381 ("dvhe.07.06", "hvc1.2.4.L153"); `null` when the container doesn't say. */
    val codecs: String? = null,
    /** A video's size and frame rate; 0 when unknown. */
    val width: Int = 0,
    val height: Int = 0,
    val frameRate: Float = 0f,
    /** A video's dynamic range, as its format says it; `null` for other tracks. */
    val range: DynamicRange? = null,
    val bitDepth: Int? = null,
)

/** The tracks of the media loaded, by kind, and its chapters. */
data class EngineTracks(
    val video: List<EngineTrack> = emptyList(),
    val audio: List<EngineTrack> = emptyList(),
    val text: List<EngineTrack> = emptyList(),
    /** The container's chapters (Matroska's, MP4's), by start; an end the file doesn't give is 0. */
    val chapters: List<Chapter> = emptyList(),
)

/** Video frames shown and dropped since the media was loaded (Media3's DecoderCounters). */
data class FrameCounts(
    val rendered: Long,
    val dropped: Long,
    /** The longest run of frames dropped one after another. */
    val maxConsecutiveDropped: Int,
)

/**
 * What a playback session needs of the engine: commands, where it is, and
 * what happens to it. [Media3EngineLink] links a [PlayerEngine]; tests use a
 * fake. Called on the main thread only, and the listener is called there.
 */
interface EngineLink {
    /** The engine, for the screen's surface and subtitles; `null` for engines that have none (fakes). */
    val engine: PlayerEngine?

    val positionMs: Long

    /** How far the media is loaded, in ms of the media. */
    val bufferedMs: Long

    /** 0 while unknown. */
    val durationMs: Long
    val status: EngineStatus

    /** Meant to play (not paused), whether or not it is moving now. */
    val playWhenReady: Boolean

    /** The server is out of reach and the engine keeps trying from where it stood. */
    val reconnecting: Boolean get() = false

    /**
     * The sound was to be decoded here ([setSoundNeeds]), but it goes to a
     * receiver as it is (AC3, E-AC3, TrueHD, DTS) and nothing here decodes
     * it: it plays at its own speed.
     */
    val soundUndecodable: Boolean get() = false

    /**
     * The network's speed in kbps as the engine measured it from what it
     * downloaded for this media (Media3's bandwidth estimate); `null` until
     * it measured something.
     */
    val bandwidthKbps: Long? get() = null

    fun setListener(listener: Listener?)

    /** Loads [source] at [startMs]; the track choices of the media before are forgotten. */
    fun prepare(source: EngineSource, startMs: Long, playWhenReady: Boolean)

    /** The engine's [PlayerEngine.dolbyVisionShown]; `null` without an engine. */
    suspend fun dolbyVisionShown(video: MediaStream?, durationMs: Long?): Boolean? = engine?.dolbyVisionShown(video, durationMs)

    fun play()

    fun pause()

    /** Stops loading and lets go of the media, keeping the engine (a session the server stopped). */
    fun stop() = pause()

    fun seekTo(positionMs: Long)

    /** Plays at [speed] times the normal speed, the sound's pitch kept; for every media loaded after too. */
    fun setSpeed(speed: Float) {}

    /** Plays the sound [delayMs] later than the picture (negative: earlier); for every media loaded after too. */
    fun setAudioDelay(delayMs: Long) {}

    /** The engine's [PlayerEngine.setSoundNeeds]: the sound decoded here, untunneled, for the speed and the delay. */
    fun setSoundNeeds(decoded: Boolean, untunneled: Boolean, reload: Boolean) {}

    /**
     * Plays the [ordinal]th audio track of the media (its order among the
     * file's audio streams), checked against [language]. Applied once the
     * media's tracks are known.
     */
    fun selectAudio(ordinal: Int, language: String?)

    /**
     * Shows the [ordinal]th subtitle track of the media, or none when
     * `null`: off is off, also for tracks the file flags as default or forced.
     */
    fun selectText(ordinal: Int?, language: String?)

    /** Frames so far; `null` when the engine doesn't count them. */
    fun frameCounts(): FrameCounts?

    /** Lines for the details panel: decoders, picture, audio output. */
    fun stats(): List<String>

    fun release()

    /** What happens to the engine. Defaults ignore an event. */
    interface Listener {
        fun onStatus(status: EngineStatus) {}

        /** Paused or meant to play again, whoever asked (the person, audio focus, a cable unplugged). */
        fun onPlayWhenReady(playWhenReady: Boolean) {}

        /** Moving or not. */
        fun onPlaying(playing: Boolean) {}

        fun onTracks(tracks: EngineTracks) {}

        /** It stopped: the session decides what to do about it. */
        fun onError(error: EngineError) {}
    }
}
