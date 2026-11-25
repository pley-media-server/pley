package app.pley.android.player.music

import app.pley.android.player.EngineSource

/** What the notification and the lock screen show of a track. */
data class TrackInfo(
    val title: String,
    val artist: String?,
    val album: String?,
    /** The album art's full address, sized for the notification; `null` without one. */
    val artUrl: String?,
    /** The track's id, on the player's item ([EngineMusicOutput.TRACK_ID]): the media session tells which queue entry it is. */
    val id: Long? = null,
)

/**
 * The sound side of music: one track at a time from the server, and what
 * it does. The app's is the engine's audio-only player ([EngineMusicOutput]);
 * tests drive a fake. Main thread only.
 */
interface MusicOutput {
    /** Loads [source] at [startMs] with [info] for the notification, and plays it when [play]. */
    fun load(source: EngineSource, startMs: Long, play: Boolean, info: TrackInfo)

    /** Whether it takes the next track ahead ([setNext]). */
    val takesNext: Boolean get() = false

    /**
     * Puts [source], with [info], after the track loaded, in place of any
     * put there before (`null`: nothing): as that track ends, the output
     * goes on to it without a gap and says so ([Listener.onNext]), rather
     * than ending. A [load] takes it back.
     */
    fun setNext(source: EngineSource?, info: TrackInfo?) {}

    fun play()

    fun pause()

    fun seekTo(positionMs: Long)

    /** Plays at [speed] (1 is as recorded), this track and the next, until told otherwise. */
    fun setSpeed(speed: Float) {}

    /** Stops and lets go of the track, keeping the player for the next. */
    fun stop()

    /** Lets go of the player, its decoder and its audio output. */
    fun release()

    val positionMs: Long

    /** The track's length as the player knows it; 0 before it does. */
    val durationMs: Long

    val bufferedMs: Long

    /** Whether it wants to play (playing, or waiting for data to). */
    val playWhenReady: Boolean

    fun setListener(listener: Listener?)

    /** What the output tells the queue. */
    interface Listener {
        /** Sound comes out, or stopped coming out. */
        fun onPlayingChanged(playing: Boolean)

        /** Waiting for data, or not anymore. */
        fun onBuffering(buffering: Boolean) {}

        /** Asked to play or pause (by the person, the notification or audio focus). */
        fun onPlayWhenReadyChanged(playWhenReady: Boolean) {}

        /** The track played to its end. */
        fun onEnded()

        /**
         * The track played to its end, and the one [setNext] put after it
         * plays now. Should that one fail, it is said after this ([onError]).
         */
        fun onNext() {}

        fun onError(error: OutputError)
    }
}

/** Why a track stopped. */
enum class OutputError {
    /** The device can't decode it: the server should convert it. */
    Decode,

    /** The server's session is gone (404, it expired): a new one is needed. */
    Gone,

    /** The server stopped the session (410): it is over, no new one is asked for. */
    Stopped,

    /** The network failed on the way. */
    Network,

    /** Anything else. */
    Other,
}
