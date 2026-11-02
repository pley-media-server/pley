package app.pley.android.ui.tv.player

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import kotlin.math.pow

/** Left and Right: presses that add up and seek once, and a held key that scrubs and seeks on release. */
class TvSeekerTest {
    private var position = 600_000L
    private var duration = 3_600_000L
    private val seeks = mutableListOf<Long>()
    private val seeker = TvSeeker({ position }, { duration }, { seeks += it })

    /** How far a key held since 0 moves the target from [from] to [to] ms, repeating every 50 ms. */
    private fun scrubbed(from: Long, to: Long): Long {
        val before = seeker.target ?: 0
        var t = from + 50
        while (t <= to) {
            seeker.repeat(1, t)
            t += 50
        }
        return (seeker.target ?: 0) - before
    }

    @Test
    fun `a held key scrubs twice as fast every 1,5 s`() {
        duration = 36_000_000
        seeker.repeat(1, 0)
        assertTrue(seeker.holding)
        seeker.repeat(1, 50)
        assertEquals(600_000L + (TvSeeker.HOLD_START_RATE * 2.0.pow(50 / 1_500.0) * 50).toLong(), seeker.target)
        // A second held goes about twice as far as the second 1.5 s before it.
        scrubbed(50, 500)
        val early = scrubbed(500, 1_500)
        scrubbed(1_500, 2_000)
        val later = scrubbed(2_000, 3_000)
        assertEquals(2.0, later / early.toDouble(), 0.1)
    }

    @Test
    fun `a held key tops out at a whole video in 10 s, or 300 ms a ms`() {
        assertEquals(20.0, seeker.rateAt(0), 1e-9)
        assertEquals(40.0, seeker.rateAt(1_500), 1e-6)
        assertEquals(160.0, seeker.rateAt(4_500), 1e-6)
        // An hour's video: an hour in 10 s, 360 ms of video a ms.
        assertEquals(360.0, seeker.rateAt(20_000), 1e-9)
        // A short one: 300 ms a ms at most.
        duration = 600_000
        assertEquals(300.0, seeker.rateAt(20_000), 1e-9)
    }

    @Test
    fun `a gap between repeats counts as 200 ms at most`() {
        seeker.repeat(1, 0)
        seeker.repeat(1, 1_000)
        assertEquals(600_000L + (TvSeeker.HOLD_START_RATE * 2.0.pow(1_000 / 1_500.0) * 200).toLong(), seeker.target)
    }

    @Test
    fun `presses in a row add up and seek once when they stop`() {
        seeker.press(1)
        seeker.press(1)
        seeker.press(1)
        assertEquals(630_000L, seeker.target)
        assertEquals(30_000L, seeker.badgeMs)
        assertTrue(seeks.isEmpty())
        seeker.settle(seeker.presses - 1)
        assertTrue("an earlier press's wait doesn't seek", seeks.isEmpty())
        seeker.settle(seeker.presses)
        assertEquals(listOf(630_000L), seeks)
        assertNull(seeker.target)
    }

    @Test
    fun `a held key scrubs faster the longer it's held and seeks once on release`() {
        seeker.press(1)
        var t = 1_000L
        seeker.repeat(1, t)
        assertTrue(seeker.holding)
        repeat(100) {
            t += 50
            seeker.repeat(1, t)
        }
        val target = seeker.target!!
        assertTrue("5 s held goes well past one step: $target", target > 610_000L + 150_000L)
        seeker.settle(seeker.presses)
        assertTrue("nothing seeks while the key is held", seeks.isEmpty())
        assertTrue(seeker.release())
        assertEquals(listOf(target), seeks)
        assertFalse(seeker.holding)
        assertFalse(seeker.release())
        assertEquals(1, seeks.size)
    }

    @Test
    fun `OK confirms a seek to come or one just made, once`() {
        assertFalse("no seek: OK plays or pauses", seeker.confirm())
        seeker.press(1)
        assertTrue(seeker.confirm())
        assertEquals(listOf(610_000L), seeks)
        assertNull(seeker.target)
        assertFalse("confirmed: the next OK pauses", seeker.confirm())

        seeker.press(1)
        seeker.settle(seeker.presses)
        assertTrue("a seek just made", seeker.confirm())
        assertFalse(seeker.confirm())

        seeker.press(1)
        seeker.settle(seeker.presses)
        seeker.settled(seeker.seeks - 1)
        assertTrue("an earlier seek's wait doesn't end this one's", seeker.confirm())

        seeker.press(1)
        seeker.settle(seeker.presses)
        seeker.settled(seeker.seeks)
        assertFalse("long after the seek", seeker.confirm())
        assertEquals(4, seeks.size)
    }

    @Test
    fun `seeks stay inside the video`() {
        position = 5_000
        seeker.press(-1)
        assertEquals(0L, seeker.target)
        seeker.cancel()
        position = duration - 2_000
        seeker.press(1)
        assertEquals(duration - TvSeeker.END_MARGIN_MS, seeker.target)
    }

    @Test
    fun `the badge goes after its own press only`() {
        seeker.press(-1)
        val first = seeker.presses
        seeker.press(-1)
        seeker.hideBadge(first)
        assertEquals(-20_000L, seeker.badgeMs)
        seeker.hideBadge(seeker.presses)
        assertNull(seeker.badgeMs)
    }
}
