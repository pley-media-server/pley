package app.pley.android.ui.tv.kit

import android.view.KeyEvent as NativeKeyEvent
import androidx.compose.ui.input.key.KeyEvent
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.async
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

/**
 * D-pad rule 5 on a fake clock: a held key moves focus by itself from
 * 180 ms, every 45 ms on the gate's own clock whatever the frames do, the
 * system's repeats are dropped, a hold ends with its key-up (or 500 ms
 * without a sign of the key), and focus rests 400 ms after the last move.
 */
@OptIn(ExperimentalCoroutinesApi::class)
@RunWith(RobolectricTestRunner::class)
class KeyRepeatGateTest {
    private var now = 10_000L
    private val gate = KeyRepeatGate(firstRepeatMs = 500) { now }
    private var pressedAt = 0L
    private var repeats = 0

    private fun down(code: Int = NativeKeyEvent.KEYCODE_DPAD_RIGHT, repeat: Int = 0) =
        KeyEvent(NativeKeyEvent(0, now, NativeKeyEvent.ACTION_DOWN, code, repeat))

    private fun up(code: Int = NativeKeyEvent.KEYCODE_DPAD_RIGHT) = KeyEvent(NativeKeyEvent(0, now, NativeKeyEvent.ACTION_UP, code, 0))

    private fun press(): Boolean {
        pressedAt = now
        repeats = 0
        return gate.admit(down())
    }

    /**
     * Frames [frameMs] apart for [ms], the system repeating the key as it
     * does (from 500 ms after the press, every 50): when the gate moved
     * focus, from the first frame's time.
     */
    private fun frames(ms: Long, frameMs: (Int) -> Long): List<Long> {
        val start = now
        val moves = mutableListOf<Long>()
        var frame = 0
        while (now - start < ms) {
            now += frameMs(frame++)
            while (now >= pressedAt + 500 + 50L * repeats) gate.admit(down(repeat = ++repeats))
            if (gate.tick() != null) moves += now - start
        }
        return moves
    }

    @Test
    fun aPressGoesThrough() {
        assertTrue(press())
        assertFalse(gate.holding)
        assertTrue(gate.admit(up()))
        assertFalse(gate.down)
    }

    @Test
    fun aHeldKeyMovesBy180MsAsARepeatOfItself() {
        assertTrue(press())
        now += 179
        assertNull(gate.tick())
        assertFalse(gate.holding)
        now += 1
        val move = gate.tick()
        assertTrue(gate.holding)
        assertNotNull(move)
        assertEquals(NativeKeyEvent.ACTION_DOWN, move!!.action)
        assertEquals(NativeKeyEvent.KEYCODE_DPAD_RIGHT, move.keyCode)
        assertEquals(1, move.repeatCount)
        // The next is 45 ms on.
        now += 44
        assertNull(gate.tick())
        now += 1
        assertEquals(2, gate.tick()?.repeatCount)
    }

    @Test
    fun heldMovesKeepA45MsPaceWhateverTheFrames() {
        press()
        now += 180
        assertNotNull(gate.tick())
        // Frames that divide the pace: a move exactly every 45 ms.
        assertEquals((1..20).map { it * 45L }, frames(900) { 15 })
        // Uneven frames, 10 and 20 ms apart: still 20 moves in 900 ms, none closer than 22 ms.
        val uneven = frames(900) { if (it % 2 == 0) 10 else 20 }
        assertEquals(20, uneven.size)
        assertTrue(uneven.zipWithNext { a, b -> b - a }.all { it >= 22 })
    }

    @Test
    fun aStallBringsOneMoveNotABurst() {
        press()
        now += 180
        gate.tick()
        // Nothing drew for 200 ms: one move, then the next 22 ms later at the soonest.
        now += 200
        assertNotNull(gate.tick())
        now += 16
        assertNull(gate.tick())
        now += 6
        assertNotNull(gate.tick())
    }

    @Test
    fun theSystemsRepeatsAreDropped() {
        assertTrue(press())
        now += 500
        assertFalse(gate.admit(down(repeat = 1)))
        // No frame came meanwhile: the repeat starts the hold, whose first move is due at once.
        assertTrue(gate.holding)
        assertNotNull(gate.tick())
        now += 50
        assertFalse(gate.admit(down(repeat = 2)))
    }

    @Test
    fun otherKeysAreNeverHeldBack() {
        press()
        now += 10
        assertTrue(gate.admit(down(NativeKeyEvent.KEYCODE_DPAD_CENTER, repeat = 3)))
        assertTrue(gate.admit(down(NativeKeyEvent.KEYCODE_BACK, repeat = 1)))
    }

    @Test
    fun anotherKeysPressEndsTheHold() {
        press()
        now += 180
        gate.tick()
        assertTrue(gate.holding)
        assertTrue(gate.admit(down(NativeKeyEvent.KEYCODE_DPAD_DOWN)))
        assertFalse(gate.holding)
        now += 180
        assertEquals(NativeKeyEvent.KEYCODE_DPAD_DOWN, gate.tick()?.keyCode)
    }

    @Test
    fun theKeyUpEndsTheHoldAndFocusRests400MsLater() {
        press()
        now += 180
        gate.tick()
        now += 30
        assertTrue(gate.admit(up()))
        assertFalse(gate.holding)
        assertFalse(gate.down)
        assertNull(gate.tick())
        now += 399
        assertFalse(gate.rested())
        now += 1
        assertTrue(gate.rested())
    }

    @Test
    fun aHoldThatRepeatsIsNeverAtRest() {
        press()
        frames(2_000) { 16 }
        assertTrue(gate.holding)
        assertFalse(gate.rested())
    }

    @Test
    fun aLostKeyUpEndsTheHold() {
        press()
        now += 180
        gate.tick()
        assertTrue(gate.holding)
        // No repeat came: the first may take 500 ms, then 500 more without one and the key is up.
        now += 820
        gate.tick()
        assertTrue(gate.holding)
        now += 1
        assertNull(gate.tick())
        assertFalse(gate.holding)
        assertFalse(gate.down)

        // Once the system repeats, 500 ms without a repeat is enough.
        press()
        now += 500
        gate.admit(down(repeat = 1))
        assertTrue(gate.holding)
        now += 501
        assertTrue(gate.rested())
        assertFalse(gate.holding)
    }

    @Test
    fun quietCountsFromTheLastKeyPressedOfAnyKind() {
        gate.admit(down())
        now += 700
        assertEquals(700L, gate.quietFor())
        gate.admit(down(NativeKeyEvent.KEYCODE_DPAD_CENTER))
        now += 100
        assertEquals(100L, gate.quietFor())
        // Letting a key go isn't pressing one.
        gate.admit(up(NativeKeyEvent.KEYCODE_DPAD_CENTER))
        now += 50
        assertEquals(150L, gate.quietFor())
        // The app coming back to the front counts as a press.
        gate.touched()
        now += 20
        assertEquals(20L, gate.quietFor())
    }

    @Test
    fun awaitReleaseReturnsWhenTheHoldEnds() = runTest {
        val clock = testScheduler
        val gate = KeyRepeatGate(firstRepeatMs = 500) { clock.currentTime }
        // No hold: at once.
        gate.awaitRelease()
        gate.admit(KeyEvent(NativeKeyEvent(0, 0, NativeKeyEvent.ACTION_DOWN, NativeKeyEvent.KEYCODE_DPAD_RIGHT, 0)))
        advanceTimeBy(180)
        gate.tick()
        assertTrue(gate.holding)
        val released = async { gate.awaitRelease() }
        runCurrent()
        assertFalse(released.isCompleted)
        gate.admit(KeyEvent(NativeKeyEvent(0, 0, NativeKeyEvent.ACTION_UP, NativeKeyEvent.KEYCODE_DPAD_RIGHT, 0)))
        runCurrent()
        assertTrue(released.isCompleted)
    }

    @Test
    fun awaitRestWaitsForTheLastMovePlus400Ms() = runTest {
        val clock = testScheduler
        val gate = KeyRepeatGate { clock.currentTime }
        gate.admit(KeyEvent(NativeKeyEvent(0, 0, NativeKeyEvent.ACTION_DOWN, NativeKeyEvent.KEYCODE_DPAD_DOWN, 0)))
        gate.admit(KeyEvent(NativeKeyEvent(0, 0, NativeKeyEvent.ACTION_UP, NativeKeyEvent.KEYCODE_DPAD_DOWN, 0)))
        val waited = async { gate.awaitRest() }
        advanceTimeBy(399)
        runCurrent()
        assertFalse(waited.isCompleted)
        advanceTimeBy(2)
        runCurrent()
        assertTrue(waited.isCompleted)
    }
}
