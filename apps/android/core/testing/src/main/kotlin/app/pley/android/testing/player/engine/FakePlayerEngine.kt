package app.pley.android.testing.player.engine

import android.content.Context
import android.os.Looper
import android.view.Window
import androidx.annotation.OptIn
import androidx.media3.common.MediaItem
import androidx.media3.common.PlaybackException
import androidx.media3.common.Player
import androidx.media3.common.SimpleBasePlayer
import androidx.media3.common.util.UnstableApi
import app.pley.android.common.DeviceProfile
import app.pley.android.player.EngineSource
import app.pley.android.player.PlayerEngine
import app.pley.android.player.engine.EngineState
import com.google.common.util.concurrent.Futures
import com.google.common.util.concurrent.ListenableFuture
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.update

/**
 * A [PlayerEngine] for tests of what drives an engine: it records what it
 * was asked, and the test sets its [state] (a display mode switch, tracks
 * the device can't play, a struggling decoder) and its player's.
 */
@OptIn(markerClass = [UnstableApi::class])
class FakePlayerEngine(looper: Looper = Looper.getMainLooper()) : PlayerEngine {
    override val player: FakeVideoPlayer = FakeVideoPlayer(looper)

    val mutableState = MutableStateFlow(EngineState())
    override val state: StateFlow<EngineState> get() = mutableState

    data class Prepared(val source: EngineSource, val startMs: Long, val playWhenReady: Boolean)

    val prepared = mutableListOf<Prepared>()
    var window: Window? = null
        private set
    var released = false
        private set

    override fun prepare(source: EngineSource, startMs: Long, playWhenReady: Boolean) {
        check(!released) { "The engine was released" }
        prepared += Prepared(source, startMs, playWhenReady)
        player.load(source.url, startMs, playWhenReady)
    }

    override fun attachWindow(window: Window) {
        this.window = window
    }

    override fun detachWindow() {
        window = null
    }

    override fun release() {
        released = true
        player.release()
    }

    /** Changes the engine's state as a real one would. */
    fun update(change: (EngineState) -> EngineState) = mutableState.update(change)

    /** Makes [FakePlayerEngine]s and keeps them, for factories under test. */
    class Factory : PlayerEngine.Factory {
        val made = mutableListOf<FakePlayerEngine>()

        override fun create(context: Context, profile: DeviceProfile, music: Boolean): PlayerEngine =
            FakePlayerEngine().also { made += it }
    }
}

/**
 * A player without decoders: one item, which is ready as soon as it is
 * prepared; play, pause and seek change its state at once. The test moves
 * time with [advanceTo], shows a picture with [renderFirstFrame] and ends
 * the item with [end].
 */
@OptIn(markerClass = [UnstableApi::class])
class FakeVideoPlayer(looper: Looper) : SimpleBasePlayer(looper) {
    private var current = State.Builder()
        .setAvailableCommands(
            Player.Commands.Builder()
                .addAll(
                    COMMAND_PLAY_PAUSE,
                    COMMAND_PREPARE,
                    COMMAND_STOP,
                    COMMAND_RELEASE,
                    COMMAND_SEEK_IN_CURRENT_MEDIA_ITEM,
                    COMMAND_SEEK_BACK,
                    COMMAND_SEEK_FORWARD,
                    COMMAND_GET_CURRENT_MEDIA_ITEM,
                    COMMAND_GET_TIMELINE,
                    COMMAND_SET_VOLUME,
                    COMMAND_GET_VOLUME,
                )
                .build(),
        )
        .build()

    var durationMs: Long = 60 * 60 * 1000L

    override fun getState(): State = current

    /** What [FakePlayerEngine.prepare] loads: the item at [startMs], buffering. */
    fun load(url: String, startMs: Long, playWhenReady: Boolean) {
        val item = MediaItemData.Builder(url)
            .setMediaItem(MediaItem.fromUri(url))
            .setDurationUs(durationMs * 1000)
            .build()
        current = current.buildUpon()
            .setPlaylist(listOf(item))
            .setContentPositionMs(startMs)
            .setPlayWhenReady(playWhenReady, PLAY_WHEN_READY_CHANGE_REASON_USER_REQUEST)
            .setPlayerError(null)
            .setPlaybackState(STATE_BUFFERING)
            .build()
        invalidateState()
    }

    /** The buffer filled: ready to play. */
    fun ready() = setPlaybackState(STATE_READY)

    /** The item played to its end. */
    fun end() = setPlaybackState(STATE_ENDED)

    /** The buffer ran dry while playing: waiting for data. */
    fun stall() = setPlaybackState(STATE_BUFFERING)

    /** Playing failed with [error]: the player stops, as Media3's does. */
    fun fail(error: PlaybackException) {
        current = current.buildUpon().setPlayerError(error).setPlaybackState(STATE_IDLE).build()
        invalidateState()
    }

    /** The first frame is on the surface: the player says so once, as Media3's does. */
    fun renderFirstFrame() {
        current = current.buildUpon().setNewlyRenderedFirstFrame(true).build()
        invalidateState()
        current = current.buildUpon().setNewlyRenderedFirstFrame(false).build()
    }

    /** The position moved to [positionMs] (time passed while playing). */
    fun advanceTo(positionMs: Long) {
        current = current.buildUpon().setContentPositionMs(positionMs).build()
        invalidateState()
    }

    private fun setPlaybackState(state: Int) {
        current = current.buildUpon().setPlaybackState(state).build()
        invalidateState()
    }

    override fun handleSetPlayWhenReady(playWhenReady: Boolean): ListenableFuture<*> {
        current = current.buildUpon().setPlayWhenReady(playWhenReady, PLAY_WHEN_READY_CHANGE_REASON_USER_REQUEST).build()
        return Futures.immediateVoidFuture()
    }

    override fun handlePrepare(): ListenableFuture<*> {
        if (current.playlist.isNotEmpty()) current = current.buildUpon().setPlaybackState(STATE_BUFFERING).build()
        return Futures.immediateVoidFuture()
    }

    override fun handleStop(): ListenableFuture<*> {
        current = current.buildUpon().setPlaybackState(STATE_IDLE).build()
        return Futures.immediateVoidFuture()
    }

    override fun handleRelease(): ListenableFuture<*> = Futures.immediateVoidFuture()

    override fun handleSeek(mediaItemIndex: Int, positionMs: Long, seekCommand: Int): ListenableFuture<*> {
        current = current.buildUpon().setContentPositionMs(positionMs.coerceAtLeast(0)).build()
        return Futures.immediateVoidFuture()
    }

    override fun handleSetVolume(volume: Float, volumeOperationType: Int): ListenableFuture<*> {
        current = current.buildUpon().setVolume(volume).build()
        return Futures.immediateVoidFuture()
    }
}
