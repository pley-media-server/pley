@file:OptIn(ExperimentalCoroutinesApi::class)

package app.pley.android.player.sleep

import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** The sleep timer on virtual time: its notice, its end, Keep listening, the end of an item. */
class SleepTimerTest {
    private var fired = 0
    private var left: Long? = null
    private var key: Any? = 1L

    private fun TestScope.timer() = SleepTimer(
        scope = backgroundScope,
        now = { testScheduler.currentTime },
        itemLeftMs = { left },
        itemKey = { key },
        onFire = { fired++ },
    )

    @Test
    fun `minutes count down, the notice shows the last 30 s once a second, then it pauses`() = runTest {
        val t = timer()
        t.set(SleepChoice.Minutes(15))
        runCurrent()
        assertEquals(SleepChoice.Minutes(15), t.state.value.choice)
        assertEquals(15, t.state.value.minutesLeft)
        assertNull(t.state.value.secondsLeft)
        advanceTimeBy(14 * MINUTE + 29_000)
        runCurrent()
        assertNull(t.state.value.secondsLeft)
        advanceTimeBy(1_000)
        runCurrent()
        assertEquals(30, t.state.value.secondsLeft)
        advanceTimeBy(15_000)
        runCurrent()
        assertEquals(15, t.state.value.secondsLeft)
        assertEquals(0, fired)
        advanceTimeBy(15_000)
        runCurrent()
        assertEquals(1, fired)
        assertEquals(SleepState(), t.state.value)
    }

    @Test
    fun `Keep listening adds the same time again`() = runTest {
        val t = timer()
        t.set(SleepChoice.Minutes(30))
        advanceTimeBy(30 * MINUTE - 10_000)
        runCurrent()
        assertEquals(10, t.state.value.secondsLeft)
        t.extend()
        runCurrent()
        assertNull(t.state.value.secondsLeft)
        assertEquals(31, t.state.value.minutesLeft)
        advanceTimeBy(20_000)
        runCurrent()
        assertEquals(0, fired)
        advanceTimeBy(30 * MINUTE)
        runCurrent()
        assertEquals(1, fired)
    }

    @Test
    fun `Off stops it`() = runTest {
        val t = timer()
        t.set(SleepChoice.Minutes(15))
        advanceTimeBy(10 * MINUTE)
        t.set(SleepChoice.Off)
        advanceTimeBy(10 * MINUTE)
        runCurrent()
        assertEquals(0, fired)
        assertEquals(SleepState(), t.state.value)
    }

    @Test
    fun `the end of the item ends it, with its notice by the item's time left`() = runTest {
        val t = timer()
        left = 10 * MINUTE
        t.set(SleepChoice.EndOfItem)
        runCurrent()
        assertTrue(t.endsWith(1L))
        assertNull(t.state.value.secondsLeft)
        left = 25_000
        advanceTimeBy(1_000)
        runCurrent()
        assertEquals(25, t.state.value.secondsLeft)
        // Paused: the time left stays.
        advanceTimeBy(5_000)
        runCurrent()
        assertEquals(25, t.state.value.secondsLeft)
        assertEquals(0, fired)
        assertTrue(t.itemEnded(1L))
        assertEquals(1, fired)
        assertEquals(SleepState(), t.state.value)
        assertFalse(t.itemEnded(2L))
    }

    @Test
    fun `Keep listening at the end of an item waits for the next one's end`() = runTest {
        val t = timer()
        left = 20_000
        t.set(SleepChoice.EndOfItem)
        runCurrent()
        assertEquals(20, t.state.value.secondsLeft)
        t.extend()
        advanceTimeBy(1_000)
        runCurrent()
        assertNull(t.state.value.secondsLeft)
        assertFalse(t.endsWith(1L))
        assertFalse(t.itemEnded(1L))
        assertEquals(0, fired)
        // The next item: its end is the timer's.
        key = 2L
        left = 15_000
        advanceTimeBy(1_000)
        runCurrent()
        assertEquals(15, t.state.value.secondsLeft)
        assertTrue(t.itemEnded(2L))
        assertEquals(1, fired)
    }

    private companion object {
        const val MINUTE = 60_000L
    }
}
