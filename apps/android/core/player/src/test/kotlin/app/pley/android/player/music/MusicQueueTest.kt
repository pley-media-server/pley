package app.pley.android.player.music

import app.pley.android.model.ItemKind
import app.pley.android.model.ItemSummary
import app.pley.android.player.MusicRepeat
import kotlin.random.Random
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class MusicQueueTest {
    private val album = (1..12L).map { track(it) }

    @Test
    fun aQueuePlaysInTheOrderGivenFromTheTrackChosen() {
        val queue = MusicQueue()
        queue.start(album, startIndex = 4)
        assertEquals(album, queue.tracks)
        assertEquals(5L, queue.current?.id)
        assertTrue(queue.next())
        assertEquals(6L, queue.current?.id)
    }

    @Test
    fun shufflingIsAPermutationThatStartsWithTheChosenTrack() {
        repeat(50) { seed ->
            val queue = MusicQueue(Random(seed))
            queue.start(album, startIndex = 7, shuffle = true)
            assertEquals(0, queue.index)
            assertEquals(8L, queue.current?.id)
            assertEquals(album.map { it.id }.sorted(), queue.tracks.map { it.id }.sorted())
        }
    }

    @Test
    fun shufflingHalfwayKeepsTheTrackAndUnshufflingFindsItsPlace() {
        // Shuffling halfway keeps the track playing and what is left, in some order after it.
        val queue = MusicQueue(Random(3))
        queue.start(album, startIndex = 0)
        queue.next()
        queue.next()
        queue.setShuffle(true)
        assertEquals(3L, queue.current?.id)
        assertEquals(0, queue.index)
        assertEquals(album.size, queue.tracks.toSet().size)

        // Back in order: the same track, at its place in the album.
        queue.next()
        val playing = queue.current
        queue.setShuffle(false)
        assertEquals(album, queue.tracks)
        assertEquals(playing, queue.current)
        assertEquals(album.indexOf(playing), queue.index)
    }

    @Test
    fun aShuffledQueueBroughtBackPlaysAsItDidAndShuffleOffGoesBackToTheOrderGiven() {
        val queue = MusicQueue(Random(7))
        queue.start(album, startIndex = 4, shuffle = true)
        queue.next()
        val state = queue.state
        assertEquals(album.size, state.given.size)

        val back = MusicQueue()
        assertTrue(back.restore(state.tracks, state.given, state.index))
        assertEquals(state.tracks, back.tracks)
        assertEquals(state.index, back.index)
        assertTrue(back.shuffle)
        val playing = back.current
        back.setShuffle(false)
        assertEquals(album, back.tracks)
        assertEquals(playing, back.current)
        assertTrue(back.state.given.isEmpty())

        // An order that doesn't place each track once is refused, and changes nothing.
        assertFalse(back.restore(album.take(3), listOf(0, 0, 1), 0))
        assertEquals(album, back.tracks)
        assertFalse(back.shuffle)
    }

    @Test
    fun aRepeatingQueueGoesRoundItsEndsAndARepeatingTrackComesAgainByItself() {
        val queue = MusicQueue()
        queue.start(album, startIndex = 11)
        assertFalse(queue.hasNext)
        assertEquals(null, queue.upcoming)
        assertFalse(queue.advance())

        queue.repeat = MusicRepeat.All
        assertTrue(queue.hasNext)
        // What comes by itself is said before it comes: the track to ask for ahead.
        assertEquals(0, queue.upcoming)
        assertEquals(1L, queue.trackAt(0)?.id)
        queue.repeat = MusicRepeat.One
        assertEquals(11, queue.upcoming)
        queue.repeat = MusicRepeat.All
        assertTrue(queue.advance())
        assertEquals(1L, queue.current?.id)
        // Previous on the first track goes to the last, unless the track has played a while.
        assertEquals(MusicQueue.Previous.Restart, queue.previous(positionMs = 10_000))
        assertEquals(MusicQueue.Previous.Moved, queue.previous(positionMs = 0))
        assertEquals(12L, queue.current?.id)

        // The track repeats by itself; Next and Previous still move, and stop at the queue's ends as without Repeat.
        queue.repeat = MusicRepeat.One
        assertTrue(queue.advance())
        assertEquals(12L, queue.current?.id)
        assertFalse(queue.hasNext)
        assertFalse(queue.next())
        assertEquals(MusicQueue.Previous.Moved, queue.previous(positionMs = 0))
        assertEquals(11L, queue.current?.id)
        assertTrue(queue.next())
        assertEquals(12L, queue.current?.id)
        queue.jumpTo(0)
        assertEquals(MusicQueue.Previous.Restart, queue.previous(positionMs = 0))

        // A shuffled queue repeats the shuffle it has.
        queue.repeat = MusicRepeat.All
        queue.setShuffle(true)
        val order = queue.tracks
        repeat(order.size) { queue.next() }
        assertEquals(order, queue.tracks)
        assertEquals(order.first(), queue.current)

        // One track alone: nothing to go round to.
        val single = MusicQueue()
        single.start(album.take(1))
        single.repeat = MusicRepeat.All
        assertEquals(MusicQueue.Previous.Restart, single.previous(positionMs = 0))
        assertTrue(single.advance())
        assertEquals(0, single.index)
    }

    @Test
    fun shufflesDifferOverSeeds() {
        val orders = (0 until 20).map { seed ->
            MusicQueue(Random(seed)).apply { start(album, 0, shuffle = true) }.tracks.map { it.id }
        }.toSet()
        assertTrue("Fisher-Yates gives many orders, got ${orders.size}", orders.size > 15)
    }

    @Test
    fun theSameTrackTwiceIsTwoPlaces() {
        val twice = listOf(track(1), track(2), track(1))
        val queue = MusicQueue(Random(1))
        queue.start(twice, startIndex = 2)
        queue.setShuffle(true)
        queue.setShuffle(false)
        assertEquals(2, queue.index)
    }

    @Test
    fun previousRestartsAfterThreeSecondsOrOnTheFirstTrack() {
        val queue = MusicQueue()
        queue.start(album, startIndex = 3)
        assertEquals(MusicQueue.Previous.Restart, queue.previous(positionMs = 3_001))
        assertEquals(3, queue.index)
        assertEquals(MusicQueue.Previous.Moved, queue.previous(positionMs = 3_000))
        assertEquals(2, queue.index)

        queue.start(album, startIndex = 0)
        assertEquals(MusicQueue.Previous.Restart, queue.previous(positionMs = 0))
        assertEquals(0, queue.index)
    }

    @Test
    fun nextStopsAtTheEndAndJumpsStayInside() {
        val queue = MusicQueue()
        queue.start(album, startIndex = album.lastIndex)
        assertFalse(queue.hasNext)
        assertFalse(queue.next())
        assertFalse(queue.jumpTo(album.size))
        assertTrue(queue.jumpTo(0))
        assertEquals(1L, queue.current?.id)
    }

    @Test
    fun anEmptyQueueHasNothing() {
        val queue = MusicQueue()
        queue.start(emptyList(), startIndex = 3, shuffle = true)
        assertTrue(queue.isEmpty)
        assertEquals(null, queue.current)
        assertFalse(queue.shuffle)
    }

    private fun track(id: Long) = ItemSummary(id, 3, ItemKind.Track, "Track $id", index = id.toInt(), parentId = 50, durationMs = 180_000)
}
