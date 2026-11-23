package app.pley.android.player

import app.pley.android.model.Chapter
import app.pley.android.model.ItemKind
import app.pley.android.model.ItemSummary
import app.pley.android.player.sleep.SleepChoice
import app.pley.android.player.sleep.SleepState
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow

/**
 * The track playing in the background, for the mini player and Now
 * Playing. It changes when the track, the queue or playing changes, never
 * with the position as it plays: that is read with [MusicControl.position]
 * while drawing, and again when [seeks] says it was moved.
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
    val repeat: MusicRepeat = MusicRepeat.Off,
    /**
     * Whether it plays, or will as soon as it can (a track starting, or
     * waiting for data): what Play/Pause shows. Paused, it stays `false`
     * through a seek's loading; [playing] is sound coming out.
     */
    val wantsPlay: Boolean = playing,
    /**
     * How many times the position was moved other than by playing: a seek
     * from here, a car, the notification or the profile's remote. Whatever
     * shows the position reads it again when this changes, paused too.
     */
    val seeks: Int = 0,
    /**
     * A track that can't be played, while the music plays on: the seconds
     * before the next one plays by itself; `null` when none will.
     */
    val nextIn: Int? = null,
    /** An audiobook's part, not music: back and forward, and Speed, in place of Shuffle and Repeat. */
    val book: Boolean = false,
    /**
     * The chapters of the part playing (an .m4b's, from its file), by start;
     * empty for music and for a part with fewer than two. Previous and Next
     * go from chapter to chapter in them.
     */
    val chapters: List<Chapter> = emptyList(),
    /** Which of [chapters] plays, -1 for none; it changes as a chapter starts, not with the position. */
    val chapter: Int = -1,
) {
    /** Whether Next has a track to go to: the one after, or with the queue repeating the first again. */
    val hasNext: Boolean get() = index + 1 < count || repeat == MusicRepeat.All || hasNextChapter

    /** Whether a chapter comes after the one playing in this part (the first, before it). */
    val hasNextChapter: Boolean get() = chapters.isNotEmpty() && chapter < chapters.lastIndex

    /** The chapter playing, `null` without. */
    val currentChapter: Chapter? get() = chapters.getOrNull(chapter)

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

/**
 * What plays once the queue ends, or a track does: nothing more ([Off]),
 * the queue from its start again ([All]), or the same track ([One]).
 */
enum class MusicRepeat {
    Off,
    All,
    One,
    ;

    /** The one a press of Repeat goes to: off, the queue, the track, off again. */
    val next: MusicRepeat get() = entries[(ordinal + 1) % entries.size]
}

/** The queue in the order it plays, the current track at [index]. */
data class MusicQueueState(
    val tracks: List<ItemSummary> = emptyList(),
    val index: Int = 0,
    val shuffle: Boolean = false,
    val repeat: MusicRepeat = MusicRepeat.Off,
    /** Shuffled, where each of [tracks] is in the order the queue was given, which Shuffle off plays; empty otherwise. */
    val given: List<Int> = emptyList(),
) {
    val current: ItemSummary? get() = tracks.getOrNull(index)

    /** An audiobook's parts, not music. */
    val isBook: Boolean get() = tracks.firstOrNull()?.kind == ItemKind.Part
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
 * ([pauseForVideo]). A TV's home screen has no such controls, so the TV
 * app pauses the music when the person leaves it ([pauseForLeave]).
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

    /** Plays [tracks] from the one at [startIndex], [startMs] into it: a book where the profile left it. */
    fun playFrom(tracks: List<ItemSummary>, startIndex: Int, startMs: Long) = play(tracks, startIndex)

    /** Plays or pauses; after the queue's end, plays its last track again. */
    fun toggle() {}

    /** The next track, if there is one; after the last, with repeat on, the first. */
    fun next() {}

    /**
     * Back to the start of the track, or the track before when it has
     * played under 3 seconds (before the first, with the queue repeating,
     * the last).
     */
    fun previous() {}

    fun seekTo(positionMs: Long) {}

    /** Moves [offsetMs] from where the track is (a book's back 15 seconds and forward 30), within the track. */
    fun seekBy(offsetMs: Long) {}

    /** The speed a book's parts play at; music always plays as recorded. Kept on the device. */
    val bookSpeed: StateFlow<Float> get() = NO_SPEED

    /** Books play at [speed] (one of [BOOK_SPEEDS]) from now on. */
    fun setBookSpeed(speed: Float) {}

    /** Plays the queue's track at [index]. */
    fun jumpTo(index: Int) {}

    /** Shuffles what is left of the queue around the current track, or puts it back in order. */
    fun setShuffle(shuffle: Boolean) {}

    /** Repeats the queue, the track playing, or nothing; kept until the music stops. */
    fun setRepeat(repeat: MusicRepeat) {}

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

    /**
     * The person leaves the TV app: music meant to play pauses as for a
     * video ([pauseForVideo]), and with [playOnReturn], [returnAfterLeave]
     * plays it on. Paused music stays paused.
     */
    fun pauseForLeave(playOnReturn: Boolean) {}

    /**
     * Back in the TV app: music [pauseForLeave] paused plays on, with
     * [playOn], unless it was played, paused or changed meanwhile.
     */
    fun returnAfterLeave(playOn: Boolean) {}

    /** The sleep timer of the music. */
    val sleep: StateFlow<SleepState> get() = NO_SLEEP

    /** Sets the sleep timer (not remembered after the music stops). */
    fun setSleep(choice: SleepChoice) {}

    /** "Keep listening" during the sleep timer's notice: the same time again. */
    fun keepListening() {}
}

/**
 * The speeds a book plays at, in the order the car's Speed button goes
 * through them (each has Media3's icon that says it); the app's pages list
 * them slowest first.
 */
val BOOK_SPEEDS: List<Float> = listOf(1.0f, 1.2f, 1.5f, 1.8f, 2.0f, 0.8f)

/**
 * Which of [chapters] (by start) [positionMs] is in, -1 before the first:
 * a quarter of a second early, so a seek to a chapter's start is in it,
 * as the web's audio bar has it (apps/web/src/audio/AudioPlayerBar.tsx).
 */
fun chapterIndexAt(chapters: List<Chapter>, positionMs: Long): Int =
    chapters.indexOfLast { it.startMs <= positionMs + CHAPTER_EARLY_MS }

private const val CHAPTER_EARLY_MS = 250L

/** How far a book's back and forward buttons go, as audiobook apps step. */
const val BOOK_BACK_MS = 15_000L
const val BOOK_FORWARD_MS = 30_000L

private val NO_QUEUE: StateFlow<MusicQueueState> = MutableStateFlow(MusicQueueState()).asStateFlow()
private val NO_SPEED: StateFlow<Float> = MutableStateFlow(1f).asStateFlow()
private val NO_SLEEP: StateFlow<SleepState> = MutableStateFlow(SleepState()).asStateFlow()
