package app.pley.android.player.music

import app.pley.android.model.ItemSummary
import app.pley.android.player.MusicQueueState
import app.pley.android.player.MusicRepeat
import kotlin.random.Random

/**
 * The tracks to play and which one plays (apps/web/src/audio/AudioPlayer.tsx).
 * The queue keeps the order it was given in; shuffled, it plays a
 * Fisher-Yates permutation of it that starts with the current track, and
 * unshuffled it goes back to the order given, at the same track. The same
 * track may be in it twice: positions, not ids, say which one plays.
 *
 * With [repeat] on the queue, the track after the last is the first, in
 * the order it has (a shuffled queue plays the same shuffle again), and
 * Previous on the first goes to the last. With [repeat] on the track, it
 * plays again once it ends ([advance]); Next and Previous still move, and
 * stop at the queue's ends as without Repeat, as Media3 has it for the
 * car, the notification and Bluetooth.
 */
class MusicQueue(private val random: Random = Random.Default) {
    private var given: List<ItemSummary> = emptyList()

    /** Play position → position in [given]. */
    private var order: IntArray = IntArray(0)

    /** Where in [order] the current track is. */
    var index: Int = 0
        private set

    var shuffle: Boolean = false
        private set

    val size: Int get() = order.size

    val isEmpty: Boolean get() = order.isEmpty()

    /** The tracks in the order they play. */
    val tracks: List<ItemSummary> get() = order.map { given[it] }

    val current: ItemSummary? get() = order.getOrNull(index)?.let { given[it] }

    /** What plays after the queue's end, or the track's; kept as the queue is started over. */
    var repeat: MusicRepeat = MusicRepeat.Off

    /** Whether [next] has a track to go to. */
    val hasNext: Boolean get() = index + 1 < order.size || (repeat == MusicRepeat.All && order.isNotEmpty())

    val state: MusicQueueState get() = MusicQueueState(tracks, index, shuffle, repeat)

    /**
     * Starts over with [tracks] at [startIndex]; with [shuffle], in a
     * random order that begins with that track.
     */
    fun start(tracks: List<ItemSummary>, startIndex: Int = 0, shuffle: Boolean = false) {
        given = tracks.toList()
        val start = if (tracks.isEmpty()) 0 else startIndex.coerceIn(0, tracks.lastIndex)
        order = IntArray(given.size) { it }
        index = start
        this.shuffle = false
        if (shuffle) setShuffle(true)
    }

    /** Forgets every track. */
    fun clear() = start(emptyList())

    /**
     * Shuffles: the current track first, the rest after it in a random
     * order. Unshuffles: the order given, at the same track.
     */
    fun setShuffle(on: Boolean) {
        if (on == shuffle || order.isEmpty()) {
            shuffle = on && order.isNotEmpty()
            return
        }
        val playing = order[index]
        if (on) {
            val rest = order.filterIndexed { i, _ -> i != index }.toIntArray()
            fisherYates(rest)
            order = intArrayOf(playing) + rest
            index = 0
        } else {
            order = IntArray(given.size) { it }
            index = playing
        }
        shuffle = on
    }

    /** Moves to the next track, with [repeat] on to the first after the last; `false` at the end. */
    fun next(): Boolean {
        if (!hasNext) return false
        index = if (index + 1 < order.size) index + 1 else 0
        return true
    }

    /**
     * The track that plays by itself once the current one has ended: the
     * same one with [repeat] on the track, else as [next]; `false` at the end.
     */
    fun advance(): Boolean = (repeat == MusicRepeat.One && order.isNotEmpty()) || next()

    /**
     * What Previous does (the web's rule): after [RESTART_AFTER_MS] of the
     * track, or on the first track, it starts again; otherwise the track
     * before plays. On the first track of a queue that repeats, that is
     * the last.
     */
    fun previous(positionMs: Long): Previous {
        val wraps = repeat == MusicRepeat.All && order.size > 1
        if (positionMs > RESTART_AFTER_MS || (index == 0 && !wraps)) return Previous.Restart
        index = if (index == 0) order.lastIndex else index - 1
        return Previous.Moved
    }

    /** Moves to the track at [position] of the play order; `false` when there is none. */
    fun jumpTo(position: Int): Boolean {
        if (position !in order.indices) return false
        index = position
        return true
    }

    private fun fisherYates(values: IntArray) {
        for (i in values.lastIndex downTo 1) {
            val j = random.nextInt(i + 1)
            val t = values[i]
            values[i] = values[j]
            values[j] = t
        }
    }

    /** What [previous] did. */
    enum class Previous { Restart, Moved }

    companion object {
        /** Previous restarts a track that has played longer than this. */
        const val RESTART_AFTER_MS = 3_000L
    }
}
