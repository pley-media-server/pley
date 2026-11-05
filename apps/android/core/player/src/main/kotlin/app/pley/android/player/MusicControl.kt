package app.pley.android.player

import app.pley.android.model.ItemSummary
import app.pley.android.player.sleep.SleepChoice
import app.pley.android.player.sleep.SleepState
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow

/**
 * The track playing in the background, for the mini player and Now
 * Playing. It changes when the track, the queue or playing changes, never
 * with the position: that is read with [MusicControl.position] while
 * drawing.
 */
data class NowPlaying(
    val itemId: Long,
    val title: String,
    /** The track's artist ("Harbour Lights"), else the album's. */
    val artist: String?,
    val playing: Boolean,
    /** The album's title. */
    val album: String? = null,
    val albumId: Long? = null,
    val artistId: Long? = null,
    /** The album art's full address on the server, without a width; `null` without one. */
    val artUrl: String? = null,
    /** The track's length as the server knows it; 0 when it doesn't. */
    val durationMs: Long = 0,
    /** Where the track is in the queue, from 0, and how long the queue is. */
    val index: Int = 0,
    val count: Int = 1,
    val shuffle: Boolean = false,
    /** Waiting for the server or the network before it plays. */
    val loading: Boolean = false,
    /** The queue played to its end: Play starts the last track again. */
    val ended: Boolean = false,
    /** Why it doesn't play; `null` while it does. */
    val error: MusicError? = null,
) {
    val hasNext: Boolean get() = index + 1 < count

    /** "Artist · Album", as the mini player says it. */
    val byline: String get() = listOfNotNull(artist, album).filter { it.isNotBlank() }.joinToString(" · ")
}

/** Why music stopped playing. */
enum class MusicError {
    /** Nobody is signed in (anymore). */
    SignedOut,

    /** The server couldn't be reached. */
    Offline,

    /** The server couldn't play the track, nor convert it. */
    Unplayable,

    /** The server stopped the playback (an administrator, or the profile elsewhere). */
    Stopped,
}

/** The queue in the order it plays, the current track at [index]. */
data class MusicQueueState(
    val tracks: List<ItemSummary> = emptyList(),
    val index: Int = 0,
    val shuffle: Boolean = false,
) {
    val current: ItemSummary? get() = tracks.getOrNull(index)
}

/** Where the track is, read when drawn (the progress line, the scrubber). */
data class MusicPosition(val positionMs: Long = 0, val durationMs: Long = 0, val bufferedMs: Long = 0) {
    /** How far along, 0 to 1. */
    val fraction: Float get() = if (durationMs > 0) (positionMs.toFloat() / durationMs).coerceIn(0f, 1f) else 0f
}

/**
 * Music, which keeps playing in the background with the lock screen's and
 * the notification's controls (a media session service). Video never does:
 * a video or a channel starting pauses the music and ends its session
 * ([pauseForVideo]).
 *
 * Everything is called on the main thread. The members besides
 * [nowPlaying] and [pauseForVideo] do nothing by default, for fakes.
 */
interface MusicControl {
    /** What plays now; `null` when nothing does. */
    val nowPlaying: StateFlow<NowPlaying?>

    /** The queue as it plays; empty when nothing does. */
    val queue: StateFlow<MusicQueueState> get() = NO_QUEUE

    /** Where the track is now. Cheap; read it while drawing, not in state. */
    fun position(): MusicPosition = MusicPosition()

    /**
     * Plays [tracks] from [startIndex]. With [shuffle], they play in a
     * random order that starts with the track at [startIndex].
     */
    fun play(tracks: List<ItemSummary>, startIndex: Int = 0, shuffle: Boolean = false) {}

    /** Plays or pauses; after the queue's end, plays its last track again. */
    fun toggle() {}

    /** The next track, if there is one. */
    fun next() {}

    /** Back to the start of the track, or the track before when it has played under 3 seconds. */
    fun previous() {}

    fun seekTo(positionMs: Long) {}

    /** Plays the queue's track at [index]. */
    fun jumpTo(index: Int) {}

    /** Shuffles what is left of the queue around the current track, or puts it back in order. */
    fun setShuffle(shuffle: Boolean) {}

    /** Stops and forgets the queue (signing out). */
    fun stop() {}

    /** Waits until the reports made so far went out, the end of the session [stop] ended too. */
    suspend fun flush() {}

    /** Drops the reports that haven't gone out yet: a switch of profile would send them as the next profile. */
    fun drop() {}

    /**
     * Pauses the music because a video or a channel starts, and ends its
     * session on the server; Play starts a new one where it was.
     */
    fun pauseForVideo()

    /** The sleep timer of the music. */
    val sleep: StateFlow<SleepState> get() = NO_SLEEP

    /** Sets the sleep timer (not remembered after the music stops). */
    fun setSleep(choice: SleepChoice) {}

    /** "Keep listening" during the sleep timer's notice: the same time again. */
    fun keepListening() {}
}

private val NO_QUEUE: StateFlow<MusicQueueState> = MutableStateFlow(MusicQueueState()).asStateFlow()
private val NO_SLEEP: StateFlow<SleepState> = MutableStateFlow(SleepState()).asStateFlow()
