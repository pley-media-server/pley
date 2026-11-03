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
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

/** D-pad rule 5 on a fake clock: held keys move focus at most every 100 ms, and focus rests after 400 ms. */
@OptIn(ExperimentalCoroutinesApi::class)
@RunWith(RobolectricTestRunner::class)
class KeyRepeatGateTest {
    private var now = 10_000L
    private val gate = KeyRepeatGate { now }

    private fun down(code: Int = NativeKeyEvent.KEYCODE_DPAD_RIGHT, repeat: Int = 0) =
        KeyEvent(NativeKeyEvent(0, now, NativeKeyEvent.ACTION_DOWN, code, repeat))

    private fun up(code: Int = NativeKeyEvent.KEYCODE_DPAD_RIGHT) = KeyEvent(NativeKeyEvent(0, now, NativeKeyEvent.ACTION_UP, code, 0))

    @Test
    fun aPressGoesThrough() {
        assertTrue(gate.admit(down()))
        assertFalse(gate.holding)
        assertTrue(gate.admit(up()))
    }

    @Test
    fun aHeldKeyMovesAtMostEvery100Ms() {
        assertTrue(gate.admit(down()))
        // The TV repeats after 400 ms, then every 50 ms: every other repeat goes through.
        now += 400
        var moves = 0
        repeat(10) { i ->
            if (gate.admit(down(repeat = i + 1))) moves++
            now += 50
        }
        assertTrue(gate.holding)
        assertEquals(5, moves)
        gate.admit(up())
        assertFalse(gate.holding)
    }

    @Test
    fun otherKeysAreNeverHeldBack() {
        gate.admit(down())
        now += 10
        assertTrue(gate.admit(down(NativeKeyEvent.KEYCODE_DPAD_CENTER, repeat = 3)))
        assertTrue(gate.admit(down(NativeKeyEvent.KEYCODE_BACK, repeat = 1)))
    }

    @Test
    fun focusRestsAfter400MsWithoutAKeyHeld() {
        gate.admit(down())
        gate.admit(up())
        now += 399
        assertFalse(gate.rested())
        now += 1
        assertTrue(gate.rested())

        gate.admit(down())
        now += 500
        gate.admit(down(repeat = 1))
        now += 450
        // Still held (repeats keep coming): not at rest, however long.
        gate.admit(down(repeat = 2))
        assertFalse(gate.rested())
    }

    @Test
    fun aLostKeyUpEndsTheHold() {
        gate.admit(down())
        gate.admit(down(repeat = 1))
        assertTrue(gate.holding)
        now += 1_000
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
