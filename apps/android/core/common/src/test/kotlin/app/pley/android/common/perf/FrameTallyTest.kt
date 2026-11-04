package app.pley.android.common.perf

import java.util.Locale
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class FrameTallyTest {
    private val sixty = FrameTally.intervalNanos(60f)

    private fun FrameTally.frames(count: Int, ms: Double = 10.0) = repeat(count) { add((ms * 1_000_000).toLong(), sixty) }

    @Test
    fun aHalfSecondOfFramesGivesFramesASecondTheLongestAndTheLateOnes() {
        val tally = FrameTally()
        tally.frames(28)
        tally.add(21_400_000, sixty)
        tally.add(17_000_000, sixty)
        val reading = tally.close()!!
        assertEquals(30, reading.frames)
        assertEquals(60, reading.fps)
        assertEquals(21.4, reading.longestMs, 0.001)
        assertEquals(2, reading.missed)
    }

    @Test
    fun aFrameJustInTimeIsNotLate() {
        val tally = FrameTally()
        tally.add(sixty, sixty)
        tally.frames(2)
        assertEquals(0, tally.close()!!.missed)
    }

    @Test
    fun twoFramesOrFewerAreAScreenAtRest() {
        val tally = FrameTally()
        tally.frames(2, ms = 40.0)
        assertNull(tally.close())
        tally.frames(3)
        assertEquals(6, tally.close()!!.fps)
    }

    @Test
    fun closingStartsTheNextWindowEmpty() {
        val tally = FrameTally()
        assertFalse(tally.open)
        tally.frames(10, ms = 30.0)
        assertTrue(tally.open)
        tally.close()
        assertFalse(tally.open)
        tally.frames(5)
        val next = tally.close()!!
        assertEquals(10.0, next.longestMs, 0.001)
        assertEquals(0, next.missed)
    }

    @Test
    fun framesWhoseTimesWereLostCountButAreNotTimed() {
        val tally = FrameTally()
        tally.frames(1)
        tally.lost(4)
        val reading = tally.close()!!
        assertEquals(5, reading.frames)
        assertEquals(10.0, reading.longestMs, 0.001)
        assertEquals(0, reading.missed)
    }

    @Test
    fun aFramesDeadlineFollowsTheDisplay() {
        assertEquals(16_666_666L, FrameTally.intervalNanos(60f))
        assertEquals(20_000_000L, FrameTally.intervalNanos(50f))
        assertEquals(41_666_666L, FrameTally.intervalNanos(24f))
        // A display that doesn't say is taken for 60 Hz.
        assertEquals(16_666_666L, FrameTally.intervalNanos(0f))
    }

    @Test
    fun theLineAndTheBoxReadTheSameInEveryLanguage() {
        val before = Locale.getDefault()
        Locale.setDefault(Locale.FRANCE)
        try {
            val reading = FrameReading(frames = 29, fps = 58, longestMs = 21.43, missed = 2)
            assertEquals("fps=58 longest=21.4 missed=2", reading.line)
            assertEquals("58 fps  21.4 ms  2 late", reading.label)
        } finally {
            Locale.setDefault(before)
        }
    }

    @Test
    fun theBoxIsGreenWithNoneLateAmberWithAFewRedWithMany() {
        assertEquals(FrameReading.Grade.Smooth, FrameReading(30, 60, 12.0, 0).grade)
        assertEquals(FrameReading.Grade.Some, FrameReading(30, 60, 25.0, 7).grade)
        assertEquals(FrameReading.Grade.Many, FrameReading(30, 60, 40.0, 8).grade)
    }
}
