package app.pley.android.player

import android.content.Context
import android.view.Window
import androidx.media3.common.Player
import app.pley.android.common.DeviceProfile
import app.pley.android.model.MediaStream
import app.pley.android.player.engine.EngineState
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow

/** Where the engine reads a video from: a file as it is, or the server's HLS playlist. */
data class EngineSource(
    val url: String,
    val hls: Boolean,
    /** The container's MIME type when the address doesn't tell (`video/x-matroska`…). */
    val mimeType: String? = null,
    /**
     * The video stream as it will arrive (the file's for direct play, what
     * the server sends otherwise): its frame rate and dynamic range choose
     * the TV's display mode before the first frame. `null` leaves the mode.
     */
    val video: MediaStream? = null,
    /** How long the item is; display modes only change for 10 minutes or more. */
    val durationMs: Long? = null,
    /**
     * Whether the video comes out as Dolby Vision, as [PlayerEngine.dolbyVisionShown]
     * told the session before it asked the server; `null`: the engine judges.
     */
    val dolbyVision: Boolean? = null,
    /**
     * A live channel (its HLS, or its MPEG-TS as the provider sends it):
     * it starts at the live edge, whatever the start position.
     */
    val live: Boolean = false,
    /**
     * Dolby Vision profile 7 plays on the HEVC decoder, as its HDR10 base
     * layer: a file on a drive, which has no server to make 8.1 of it.
     */
    val profile7BaseLayer: Boolean = false,
)

/**
 * Media3 as pley plays with it: the decoders this device should use, buffers
 * sized for its memory, the TV's display mode matched to the video, and
 * audio that follows the output. One engine per player screen; music has
 * its own.
 */
interface PlayerEngine {
    /** The player, for surfaces, subtitles and the media session. */
    val player: Player

    /**
     * What the engine knows beyond the player: a display mode change under
     * way, tracks the device can't play, a decoder that struggles, stats.
     */
    val state: StateFlow<EngineState> get() = IdleEngineState.flow

    /** Loads [source] at [startMs]; plays at once when [playWhenReady]. */
    fun prepare(source: EngineSource, startMs: Long, playWhenReady: Boolean)

    /**
     * Whether [video], [durationMs] long, would come out as Dolby Vision
     * here: the screen shows it in the display mode the video would play
     * in. Asked before the server is, so it sends what plays; `null` when
     * the engine can't tell.
     */
    suspend fun dolbyVisionShown(video: MediaStream?, durationMs: Long?): Boolean? = null

    /**
     * The player screen's window (its onStart): a TV changes the display mode
     * through it. A [prepare] made before waits for the window to decide.
     */
    fun attachWindow(window: Window) {}

    /** The window goes (onStop): the display mode returns to the system's choice. */
    fun detachWindow() {}

    /** Lets go of the decoders and the audio output (the TV's player does in onStop). */
    fun release()

    /**
     * Plays the sound [delayMs] later than the picture (negative: earlier),
     * from now on, for every media this engine loads.
     */
    fun setAudioDelay(delayMs: Long) {}

    /**
     * What the speed and the audio delay need of the sound: [decoded] here,
     * rather than passed through to a receiver as it is (a speed other than
     * 1×), and [untunneled] (any speed or delay: a tunneled output keeps
     * the picture with the sound itself). For every media loaded from now
     * on; [reload] for the media playing too, loaded again where it is when
     * its sound goes out another way. Sound that nothing here decodes stays
     * as it is ([EngineState.soundUndecodable]).
     */
    fun setSoundNeeds(decoded: Boolean, untunneled: Boolean, reload: Boolean) {}

    /** Makes engines for this device. */
    fun interface Factory {
        /** An engine for video, or for music when [music] (no video decoders, offload with the screen off). */
        fun create(context: Context, profile: DeviceProfile, music: Boolean): PlayerEngine
    }
}

/** The state of an engine that reports none (fakes). */
private object IdleEngineState {
    val flow: StateFlow<EngineState> = MutableStateFlow(EngineState()).asStateFlow()
}
