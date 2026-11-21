package app.pley.android.player.music

import androidx.annotation.OptIn
import androidx.media3.common.C
import androidx.media3.common.ForwardingSimpleBasePlayer
import androidx.media3.common.MediaItem
import androidx.media3.common.MediaMetadata
import androidx.media3.common.PlaybackException
import androidx.media3.common.Player
import androidx.media3.common.Timeline
import androidx.media3.common.util.UnstableApi
import app.pley.android.model.Chapter
import app.pley.android.model.ItemSummary
import app.pley.android.player.BOOK_BACK_MS
import app.pley.android.player.BOOK_FORWARD_MS
import app.pley.android.player.MusicError
import app.pley.android.player.MusicRepeat
import app.pley.android.player.NowPlaying
import com.google.common.util.concurrent.Futures
import com.google.common.util.concurrent.ListenableFuture
import kotlin.math.abs

/**
 * The player the media session shows (notification, lock screen, the
 * system's media controls, Bluetooth buttons, Android Auto): the output's
 * one-track player, shown as [playback]'s whole queue, so a car lists
 * what comes next and picks from it. Play, Pause, Previous, Next, a pick
 * from the queue, Shuffle and Repeat go through [playback]: after the
 * queue's end, Play starts its last track again. What a car asks to play
 * is made ready by the session's callback (MusicService) and started by
 * the session's Play, so items set here change nothing; nor can the queue
 * be edited from outside.
 *
 * A track is the same entry from the moment it is the queue's until the
 * next one is: its name, art and id as [describe] gives them, whether the
 * output still asks the server for it, loads it or plays it. A car draws
 * what plays again each time the entry changes, so one that changed as
 * the track loaded, and again as its art came, flickered at every skip.
 * The same holds for what it may do: seeking stays on offer between
 * tracks (a seek then changes nothing), so a car's scrubber isn't taken
 * away and put back, and the queue's entries don't change as one loads,
 * so the car isn't sent the whole queue again at each skip. A queue that
 * waits for Play (brought back as a car connects, or asked to be ready)
 * shows paused on its track, where it will start.
 * [words] says why a track doesn't play, in the language shown. Call
 * [queueChanged] when [playback]'s queue changes.
 */
@OptIn(markerClass = [UnstableApi::class])
internal class QueuePlayer(
    player: Player,
    private val playback: MusicPlayback,
    private val describe: (track: ItemSummary) -> MediaItem = { plainItem(it) },
    private val words: (MusicError, nextIn: Int?) -> String = { error, _ -> error.name },
    /** A chapter's name: its own, else its number ("Chapter 3"), from 0. */
    private val chapterName: (Chapter, index: Int) -> String = { chapter, i -> chapter.title?.trim()?.takeIf { it.isNotEmpty() } ?: "${i + 1}" },
) : ForwardingSimpleBasePlayer(player) {
    /** Shows [playback]'s queue as it is now. */
    fun queueChanged() = invalidateState()

    /** The tracks [entries] list, as they were when last described. */
    private var listedTracks: List<ItemSummary> = emptyList()
    private var entries: List<MediaItemData> = emptyList()

    override fun getState(): State {
        val base = super.getState()
        val queue = playback.queueState.value
        val commands = base.availableCommands.buildUpon().addAll(*QUEUE_COMMANDS).removeAll(*NOT_OURS).build()
        val state = base.buildUpon()
            .setAvailableCommands(commands)
            // A book's back and forward buttons (MusicService), and any controller's own.
            .setSeekBackIncrementMs(SEEK_BACK_MS)
            .setSeekForwardIncrementMs(SEEK_FORWARD_MS)
            .setShuffleModeEnabled(queue.shuffle)
            .setRepeatMode(repeatModeOf(queue.repeat))
        if (queue.tracks.isEmpty()) return state.build()
        val index = base.currentMediaItemIndex
        val window = if (!base.timeline.isEmpty && index in 0 until base.timeline.windowCount) base.timeline.getWindow(index, Timeline.Window()) else null
        val current = queue.tracks.getOrNull(queue.index)
        // Whether the output plays the queue's track now; not while the next one is asked of the server and loads, nor
        // while the same track is (it repeats, or comes again): the output still holds the play before.
        val loaded = playback.held && window != null && current != null &&
            window.mediaItem.mediaMetadata.extras?.getLong(EngineMusicOutput.TRACK_ID, -1) == current.id
        val listed = entriesOf(queue.tracks)
        val playlist = if (loaded && window != null && queue.index in listed.indices) {
            listed.toMutableList().also { it[queue.index] = playing(it[queue.index], base, window, playback.nowPlaying.value) }
        } else {
            listed
        }
        state.setPlaylist(playlist).setCurrentMediaItemIndex(queue.index.coerceIn(0, playlist.lastIndex))
        val now = playback.nowPlaying.value ?: return state.build()
        val failed = now.error
        if (failed != null) {
            // A track that failed for good says why in Pley's words, also when no player failed (the server out of
            // reach as the track was asked for), where the session would say nothing plays and nothing is wrong.
            state.setPlaybackState(Player.STATE_IDLE)
                .setIsLoading(false)
                .setPlayerError(PlaybackException(words(failed, now.nextIn), null, errorCodeOf(failed)))
                .setPlayWhenReady(false, Player.PLAY_WHEN_READY_CHANGE_REASON_USER_REQUEST)
            if (!loaded) state.atTheStart()
            return state.build()
        }
        // Between tracks, and while a failed one is tried again, the output has nothing loaded, or an error: the session
        // says the queue's track is loading, playing as it will, so a car doesn't show its own loading screen for nothing
        // playing at each change.
        val starting = !now.ended && (now.loading || !loaded || base.playerError != null)
        if (starting) {
            state.setPlaybackState(Player.STATE_BUFFERING)
                .setPlayerError(null)
                // Playing as it will: the output's own state between tracks (stopped, buffering) isn't a pause.
                .setPlayWhenReady(now.wantsPlay, Player.PLAY_WHEN_READY_CHANGE_REASON_USER_REQUEST)
            if (!loaded) state.atTheStart()
        }
        return state.build()
    }

    /**
     * Every entry of the queue as [describe] gives its track: the one
     * playing too while it is asked for and loads. Described again only
     * when the queue's tracks change: a long queue (an artist's every
     * album) isn't described anew, on the main thread, at each change of
     * what plays.
     */
    private fun entriesOf(tracks: List<ItemSummary>): List<MediaItemData> {
        if (tracks == listedTracks) return entries
        listedTracks = tracks
        entries = tracks.mapIndexed { i, track ->
            val item = describe(track)
            // Seekable, loaded or not: every track is, and an entry that said so only once loaded would change the queue.
            MediaItemData.Builder(Uid(i, track.id)).setIsSeekable(true)
                .setMediaItem(item)
                .setMediaMetadata(item.mediaMetadata)
                .setDurationUs(track.durationMs?.takeIf { it > 0 }?.let { it * 1000 } ?: C.TIME_UNSET)
                .build()
        }
        return entries
    }

    /**
     * The track the output plays: [listed], the entry it had while it
     * loaded, with the output's tracks, and its length only where the
     * library's is none or wrong (never none: a car would take its
     * progress bar away). A book's part with chapters is named for the
     * chapter playing, the book under it, as the web's audio bar has it.
     */
    private fun playing(listed: MediaItemData, base: State, window: Timeline.Window, now: NowPlaying?): MediaItemData {
        val art = withArt(listed.mediaItem.mediaMetadata, base.currentMetadata)
        val chapter = now?.currentChapter
        val metadata = if (now != null && chapter != null) art.buildUpon().setTitle(chapterName(chapter, now.chapter)).build() else art
        return listed.buildUpon()
            .setMediaItem(listed.mediaItem.buildUpon().setMediaMetadata(metadata).build())
            // Said outright: left out, the file's own tags (a cover inside it too) would be mixed in as they are read.
            .setMediaMetadata(metadata)
            .setTracks(base.currentTracks)
            .setDurationUs(lengthOf(window.durationUs, listed.durationUs))
            .build()
    }

    /**
     * The start of a track the output doesn't hold yet (or where a queue that waits will start), nothing of it
     * buffered: where the output is and what it has buffered are the last track's.
     */
    private fun State.Builder.atTheStart(): State.Builder = setContentPositionMs(playback.position().positionMs)
        .setContentBufferedPositionMs(PositionSupplier.ZERO)
        .setTotalBufferedDurationMs(PositionSupplier.ZERO)

    /**
     * [listed], the entry's name and art as the queue lists it, with the
     * cover the output fetched ([playing]'s) when the entry names none by
     * address: a session without a library to read covers from.
     */
    private fun withArt(listed: MediaMetadata, playing: MediaMetadata): MediaMetadata {
        val cover = playing.artworkData
        if (listed.artworkUri != null || listed.artworkData != null || cover == null) return listed
        return listed.buildUpon().setArtworkData(cover, playing.artworkDataType).build()
    }

    /**
     * The length of the track the output plays: the one the queue [listed]
     * it with, unless the output knows a length that differs by more than
     * [LENGTH_SLACK_US] (or the library knows none). The two rarely agree
     * to the millisecond, and a car draws Now Playing again when the length
     * changes: the entry would change at every track as it starts.
     */
    private fun lengthOf(outputUs: Long, listed: Long): Long = when {
        outputUs == C.TIME_UNSET -> listed
        listed == C.TIME_UNSET -> outputUs
        abs(outputUs - listed) <= LENGTH_SLACK_US -> listed
        else -> outputUs
    }

    override fun handleSetPlayWhenReady(playWhenReady: Boolean): ListenableFuture<*> {
        if (playWhenReady) playback.play() else playback.pause()
        return Futures.immediateVoidFuture()
    }

    override fun handleSeek(mediaItemIndex: Int, positionMs: Long, seekCommand: Int): ListenableFuture<*> {
        val current = playback.queueState.value.index
        when (seekCommand) {
            Player.COMMAND_SEEK_TO_NEXT, Player.COMMAND_SEEK_TO_NEXT_MEDIA_ITEM -> playback.next()
            Player.COMMAND_SEEK_TO_PREVIOUS, Player.COMMAND_SEEK_TO_PREVIOUS_MEDIA_ITEM -> playback.previous()
            else -> when {
                mediaItemIndex != C.INDEX_UNSET && mediaItemIndex != current -> playback.jumpTo(mediaItemIndex)
                else -> playback.seekTo(if (positionMs == C.TIME_UNSET) 0 else positionMs)
            }
        }
        return Futures.immediateVoidFuture()
    }

    override fun handleSetShuffleModeEnabled(shuffleModeEnabled: Boolean): ListenableFuture<*> {
        playback.setShuffle(shuffleModeEnabled)
        return Futures.immediateVoidFuture()
    }

    override fun handleSetRepeatMode(repeatMode: Int): ListenableFuture<*> {
        playback.setRepeat(repeatOf(repeatMode))
        return Futures.immediateVoidFuture()
    }

    override fun handleStop(): ListenableFuture<*> {
        playback.stop()
        return Futures.immediateVoidFuture()
    }

    // The output prepares its tracks itself (MusicPlayback): one prepared from outside would be the last one again.
    override fun handlePrepare(): ListenableFuture<*> = Futures.immediateVoidFuture()

    override fun handleSetMediaItems(mediaItems: List<MediaItem>, startIndex: Int, startPositionMs: Long): ListenableFuture<*> = Futures.immediateVoidFuture()

    override fun handleAddMediaItems(index: Int, mediaItems: List<MediaItem>): ListenableFuture<*> = Futures.immediateVoidFuture()

    override fun handleRemoveMediaItems(fromIndex: Int, toIndex: Int): ListenableFuture<*> = Futures.immediateVoidFuture()

    override fun handleMoveMediaItems(fromIndex: Int, toIndex: Int, newIndex: Int): ListenableFuture<*> = Futures.immediateVoidFuture()

    override fun handleReplaceMediaItems(fromIndex: Int, toIndex: Int, mediaItems: List<MediaItem>): ListenableFuture<*> = Futures.immediateVoidFuture()

    /** A queue entry: the same track can be in the queue twice. */
    private data class Uid(val position: Int, val trackId: Long)

    companion object {
        private val QUEUE_COMMANDS = intArrayOf(
            Player.COMMAND_PLAY_PAUSE,
            Player.COMMAND_PREPARE,
            Player.COMMAND_STOP,
            Player.COMMAND_SEEK_TO_NEXT,
            Player.COMMAND_SEEK_TO_NEXT_MEDIA_ITEM,
            Player.COMMAND_SEEK_TO_PREVIOUS,
            Player.COMMAND_SEEK_TO_PREVIOUS_MEDIA_ITEM,
            Player.COMMAND_SEEK_TO_MEDIA_ITEM,
            Player.COMMAND_SEEK_TO_DEFAULT_POSITION,
            // Within the track: the output takes them away while it prepares one, which the session doesn't pass on.
            Player.COMMAND_SEEK_IN_CURRENT_MEDIA_ITEM,
            Player.COMMAND_SEEK_BACK,
            Player.COMMAND_SEEK_FORWARD,
            Player.COMMAND_SET_SHUFFLE_MODE,
            Player.COMMAND_SET_REPEAT_MODE,
            Player.COMMAND_GET_TIMELINE,
            Player.COMMAND_GET_CURRENT_MEDIA_ITEM,
            Player.COMMAND_GET_METADATA,
            // What a car picks to play goes through the session's callback (MusicService).
            Player.COMMAND_SET_MEDIA_ITEM,
            Player.COMMAND_CHANGE_MEDIA_ITEMS,
        )

        /** How far the output's length may be from the library's and the library's still stand ([lengthOf]). */
        private const val LENGTH_SLACK_US = 2_000_000L

        /** How far back and forward a seek by a step goes, as audiobook apps step. */
        const val SEEK_BACK_MS = BOOK_BACK_MS
        const val SEEK_FORWARD_MS = BOOK_FORWARD_MS

        /** The queue plays at its speed. */
        private val NOT_OURS = intArrayOf(Player.COMMAND_SET_SPEED_AND_PITCH)

        fun repeatModeOf(repeat: MusicRepeat): Int = when (repeat) {
            MusicRepeat.Off -> Player.REPEAT_MODE_OFF
            MusicRepeat.All -> Player.REPEAT_MODE_ALL
            MusicRepeat.One -> Player.REPEAT_MODE_ONE
        }

        fun repeatOf(@Player.RepeatMode mode: Int): MusicRepeat = when (mode) {
            Player.REPEAT_MODE_ALL -> MusicRepeat.All
            Player.REPEAT_MODE_ONE -> MusicRepeat.One
            else -> MusicRepeat.Off
        }

        /** [error] as a session's controllers know errors. */
        private fun errorCodeOf(error: MusicError): Int = when (error) {
            MusicError.SignedOut -> PlaybackException.ERROR_CODE_AUTHENTICATION_EXPIRED
            MusicError.Offline -> PlaybackException.ERROR_CODE_IO_NETWORK_CONNECTION_FAILED
            MusicError.Unplayable -> PlaybackException.ERROR_CODE_IO_UNSPECIFIED
            MusicError.Stopped -> PlaybackException.ERROR_CODE_REMOTE_ERROR
        }

        /** A track with its title, artist and album only. */
        fun plainItem(track: ItemSummary): MediaItem = MediaItem.Builder()
            .setMediaId("track:${track.id}")
            .setMediaMetadata(
                MediaMetadata.Builder()
                    .setTitle(track.title)
                    .setArtist(track.grandparentTitle)
                    .setAlbumTitle(track.parentTitle)
                    .setIsPlayable(true)
                    .setIsBrowsable(false)
                    .setMediaType(MediaMetadata.MEDIA_TYPE_MUSIC)
                    .build(),
            )
            .build()
    }
}
