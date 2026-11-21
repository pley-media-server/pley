package app.pley.android.player.music

import android.os.Looper
import androidx.annotation.OptIn
import androidx.media3.common.C
import androidx.media3.common.MediaItem
import androidx.media3.common.PlaybackException
import androidx.media3.common.Player
import androidx.media3.common.SimpleBasePlayer
import androidx.media3.common.util.UnstableApi
import app.pley.android.model.LiveChannel
import app.pley.android.player.live.LivePhase
import app.pley.android.player.live.LivePlayback
import app.pley.android.player.live.LiveState
import app.pley.android.player.live.LiveTrouble
import com.google.common.util.concurrent.Futures
import com.google.common.util.concurrent.ListenableFuture
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.launch

/**
 * A Live TV channel's sound in the car, as radio: the channels of the list
 * it was picked from, Next and Previous going through them, watched through
 * the server as the phone's player watches ([LivePlayback], on the music's
 * engine: no picture is decoded, and the audio focus is its own). It is
 * live: no seeking, no position. Pause lets the channel's connection go,
 * as does another app's sound taking over; Play tunes it again.
 *
 * [player] is what the music service's session shows while the radio
 * plays. Made and used on the main thread.
 */
@OptIn(markerClass = [UnstableApi::class])
class LiveRadio(
    /** A playback for the radio, in [scope] ([LivePlayback.create] with `radio`). */
    private val make: (scope: CoroutineScope) -> LivePlayback,
    /** A channel as the car shows it ([app.pley.android.player.music.library.LiveLibrary.channel]). */
    private val describe: (LiveChannel) -> MediaItem,
    /** Why a channel doesn't play, in the language shown. */
    private val words: (LiveTrouble) -> String,
) {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    private var playback: LivePlayback? = null

    /** [playback]'s own: what it started (the server's events it follows) ends with it, not with the service. */
    private var playbackScope: CoroutineScope? = null
    private var following: Job? = null
    private var channels: List<LiveChannel> = emptyList()
    private var index = 0

    /** The channel was picked and not tuned yet ([cue]): what the playback says is of the one before. */
    private var waits = false

    val player = RadioPlayer()

    /** A channel is picked: tuned, or kept to tune again on Play. */
    val active: Boolean get() = channels.isNotEmpty()

    /** Plays [list]'s channel at [at], Next and Previous going through [list]. */
    fun play(list: List<LiveChannel>, at: Int) {
        if (list.isEmpty()) return
        channels = list
        index = at.coerceIn(list.indices)
        waits = false
        playback().tune(channels[index].id)
        player.changed()
    }

    /**
     * Makes [list]'s channel at [at] the radio's without tuning it: it
     * waits, and Play tunes it. The channel played when the car was last
     * connected, or one a car or the Assistant asked to have ready before
     * it asks to play. A channel playing meanwhile is let go.
     */
    fun cue(list: List<LiveChannel>, at: Int) {
        if (list.isEmpty()) return
        channels = list
        index = at.coerceIn(list.indices)
        waits = true
        playback?.stop()
        player.changed()
    }

    /** Lets the channel go, keeping which it was: Play tunes it again. */
    fun pause() {
        playback?.stop()
    }

    /** Tunes the channel let go again, from the live edge; one that waits ([cue]), for the first time. */
    fun resume() {
        val made = playback
        if (made == null || waits) return play(channels, index)
        if (made.state.value.phase is LivePhase.Failed) made.retry() else made.resume()
    }

    /** Stops for good: the music plays now, or the car left. */
    fun close() {
        following?.cancel()
        following = null
        playback?.close()
        playback = null
        playbackScope?.cancel()
        playbackScope = null
        channels = emptyList()
        index = 0
        waits = false
        player.changed()
    }

    /** Ends it with the service. */
    fun release() {
        close()
        scope.cancel()
    }

    /** Channel up ([by] 1) or down (-1) in the list, round its ends. */
    private fun step(by: Int) {
        if (channels.size < 2) return
        index = Math.floorMod(index + by, channels.size)
        waits = false
        playback().tune(channels[index].id)
        player.changed()
    }

    private fun playback(): LivePlayback = playback ?: run {
        val own = CoroutineScope(scope.coroutineContext + SupervisorJob(scope.coroutineContext[Job]))
        playbackScope = own
        make(own)
    }.also { made ->
        playback = made
        // The profile's remote, on another device, changes channel as Next and Previous do.
        made.zap = ::step
        following = scope.launch { made.state.collect { player.changed() } }
    }

    /** What the playback does; nothing yet for a channel that waits. */
    private val liveState: LiveState get() = if (waits) LiveState() else playback?.state?.value ?: LiveState()

    /** The session's player: the radio's channels and what the playback is doing. */
    inner class RadioPlayer : SimpleBasePlayer(Looper.getMainLooper()) {
        internal fun changed() = invalidateState()

        override fun getState(): State {
            val live = liveState
            val phase = live.phase
            val playing = phase is LivePhase.Tuning || phase is LivePhase.Playing
            val playbackState = when {
                channels.isEmpty() -> Player.STATE_IDLE
                phase is LivePhase.Failed -> Player.STATE_IDLE
                phase is LivePhase.Tuning || live.buffering -> Player.STATE_BUFFERING
                else -> Player.STATE_READY
            }
            val error = (phase as? LivePhase.Failed)?.let { PlaybackException(words(it.trouble), null, PlaybackException.ERROR_CODE_REMOTE_ERROR) }
            return State.Builder()
                .setAvailableCommands(COMMANDS)
                .setPlayWhenReady(playing, Player.PLAY_WHEN_READY_CHANGE_REASON_USER_REQUEST)
                .setPlaybackState(playbackState)
                .setPlayerError(error)
                .setPlaylist(channels.mapIndexed(::itemData))
                .setCurrentMediaItemIndex(if (channels.isEmpty()) C.INDEX_UNSET else index)
                .setContentPositionMs(0)
                .build()
        }

        // By place as well as channel: a list can name a channel twice, and a playlist's uids differ.
        private fun itemData(position: Int, channel: LiveChannel): MediaItemData = MediaItemData.Builder(position to channel.id)
            .setMediaItem(describe(channel))
            // Live: what airs now, without an end, a length or a place to seek to.
            .setIsDynamic(true)
            .setIsSeekable(false)
            .setLiveConfiguration(MediaItem.LiveConfiguration.Builder().build())
            .setDurationUs(C.TIME_UNSET)
            .build()

        override fun handleSetPlayWhenReady(playWhenReady: Boolean): ListenableFuture<*> {
            if (playWhenReady) this@LiveRadio.resume() else this@LiveRadio.pause()
            return Futures.immediateVoidFuture()
        }

        override fun handleSeek(mediaItemIndex: Int, positionMs: Long, seekCommand: Int): ListenableFuture<*> {
            when (seekCommand) {
                Player.COMMAND_SEEK_TO_NEXT, Player.COMMAND_SEEK_TO_NEXT_MEDIA_ITEM -> step(1)
                Player.COMMAND_SEEK_TO_PREVIOUS, Player.COMMAND_SEEK_TO_PREVIOUS_MEDIA_ITEM -> step(-1)
                Player.COMMAND_SEEK_TO_MEDIA_ITEM -> if (mediaItemIndex in channels.indices && mediaItemIndex != index) {
                    index = mediaItemIndex
                    waits = false
                    playback().tune(channels[index].id)
                    changed()
                }
                else -> Unit
            }
            return Futures.immediateVoidFuture()
        }

        override fun handleStop(): ListenableFuture<*> {
            this@LiveRadio.pause()
            return Futures.immediateVoidFuture()
        }

        // What plays is set by the service as the car picks it (MusicService.onSetMediaItems): the session's own
        // setMediaItems that follows changes nothing.
        override fun handleSetMediaItems(mediaItems: MutableList<MediaItem>, startIndex: Int, startPositionMs: Long): ListenableFuture<*> =
            Futures.immediateVoidFuture()

        // The list is the one picked from: a controller's edits to it (a car's queue) change nothing.
        override fun handleAddMediaItems(index: Int, mediaItems: List<MediaItem>): ListenableFuture<*> = Futures.immediateVoidFuture()

        override fun handleRemoveMediaItems(fromIndex: Int, toIndex: Int): ListenableFuture<*> = Futures.immediateVoidFuture()

        override fun handleMoveMediaItems(fromIndex: Int, toIndex: Int, newIndex: Int): ListenableFuture<*> = Futures.immediateVoidFuture()

        override fun handleReplaceMediaItems(fromIndex: Int, toIndex: Int, mediaItems: List<MediaItem>): ListenableFuture<*> =
            Futures.immediateVoidFuture()

        override fun handlePrepare(): ListenableFuture<*> = Futures.immediateVoidFuture()

        override fun handleRelease(): ListenableFuture<*> = Futures.immediateVoidFuture()
    }

    private companion object {
        /** What the radio does: play and pause, next and previous, a channel of the list; no seeking within one. */
        val COMMANDS: Player.Commands = Player.Commands.Builder()
            .addAll(
                Player.COMMAND_PLAY_PAUSE,
                Player.COMMAND_PREPARE,
                Player.COMMAND_STOP,
                Player.COMMAND_SEEK_TO_NEXT,
                Player.COMMAND_SEEK_TO_NEXT_MEDIA_ITEM,
                Player.COMMAND_SEEK_TO_PREVIOUS,
                Player.COMMAND_SEEK_TO_PREVIOUS_MEDIA_ITEM,
                Player.COMMAND_SEEK_TO_MEDIA_ITEM,
                Player.COMMAND_GET_TIMELINE,
                Player.COMMAND_GET_CURRENT_MEDIA_ITEM,
                Player.COMMAND_GET_METADATA,
                Player.COMMAND_SET_MEDIA_ITEM,
                Player.COMMAND_CHANGE_MEDIA_ITEMS,
                Player.COMMAND_RELEASE,
            )
            .build()
    }
}
