package app.pley.android.player

import android.graphics.Bitmap
import androidx.media3.common.text.Cue
import app.pley.android.player.session.SavedPlaybackState
import app.pley.android.player.sleep.SleepChoice
import kotlinx.coroutines.flow.StateFlow

/**
 * One playback, as a player screen drives it: the server's session,
 * progress reports, tracks, quality, markers and what plays next. The
 * phone's and TV's screens share it; neither talks to the server itself.
 * Called on the main thread.
 */
interface PlaybackController {
    val state: StateFlow<PlayerUiState>

    /** The engine playing, for the screen's surface; `null` until prepared. */
    val engine: PlayerEngine?

    /**
     * The server's subtitles showing now (streams, and subtitle files beside
     * the media), for SubtitleView. Subtitles inside the file being played
     * as it is come from the engine's own player instead.
     */
    val cues: StateFlow<List<Cue>>

    fun play()

    fun pause()

    /**
     * The screen was stopped ([away]), its playback paused with the engine
     * stopped and its session kept a while, or it shows again: meanwhile
     * the remote's commands are left alone, as nothing would show or be
     * heard of them here.
     */
    fun setAway(away: Boolean) {}

    fun seekTo(positionMs: Long)

    /** Another audio track by the server's stream index; remembered for the item. */
    fun selectAudio(index: Long)

    /** Another subtitle track, or none when [index] is `null`; remembered for the item. */
    fun selectSubtitle(index: Long?)

    /**
     * Moves the subtitles by [offsetMs] (positive: later), within ±10 s in
     * 0.1 s steps; remembered for the file on this device. 0 resets.
     */
    fun setSubtitleOffset(offsetMs: Long) {}

    /** Plays at [speed], one of [PlaybackSpeeds.ALL], the pitch kept; for this playback only (the next title plays at 1×). */
    fun setSpeed(speed: Float) {}

    /**
     * Moves the sound [delayMs] against the picture (positive: later),
     * within ±5 s in 50 ms steps; for this playback only. 0 resets.
     */
    fun setAudioDelay(delayMs: Long) {}

    /**
     * Looks online for subtitles of the file playing, in the profile's
     * subtitle language ([PlayerUiState.subtitleSearch] shows how it goes).
     */
    fun searchSubtitles() {}

    /** Adds the subtitle [resultId] of the search to the file, and shows it. */
    fun addFoundSubtitle(resultId: String) {}

    /** The search's list was closed. */
    fun closeSubtitleSearch() {}

    /** Sets the sleep timer (not remembered after this playback). */
    fun setSleep(choice: SleepChoice) {}

    /** "Keep watching" during the sleep timer's notice: the same time again. */
    fun keepWatching() {}

    /** "original" or a quality preset's id ("8m"); remembered for this device and network. */
    fun selectQuality(name: String)

    /** Skips the intro or credits showing now. */
    fun skip()

    /** Plays what comes next now, without waiting for the countdown. */
    fun playNext()

    /** Hides the Up Next card; what plays next doesn't start by itself. */
    fun cancelUpNext()

    /** After the player switched to a transcode by itself: the original again. */
    fun playOriginal()

    /** The details are open: their transcode numbers are asked for every 2 s meanwhile. */
    fun setDetailsOpen(open: Boolean)

    /** The notice was shown. */
    fun dismissNotice()

    /** The seek preview at [positionMs]; `null` until the previews are made. */
    suspend fun thumbnail(positionMs: Long): Bitmap?

    /** What to start again with after the process was killed. */
    fun saveState(): SavedPlaybackState

    /** Tries again after an error. */
    fun retry()

    /** Ends the playback: the last position is reported and the server's session closed. */
    fun close()
}
