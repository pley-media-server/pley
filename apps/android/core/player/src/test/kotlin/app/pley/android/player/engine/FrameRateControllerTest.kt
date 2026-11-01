package app.pley.android.player.engine

import app.pley.android.caps.DisplayState
import app.pley.android.caps.MatchContent
import kotlinx.coroutines.test.currentTime
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class FrameRateControllerTest {
    private class FakeHost(var display: DisplayState, var answers: Boolean = true) : DisplayModeHost {
        val requested = mutableListOf<Int>()
        val waits = mutableListOf<Long>()

        override val displayKey = "0:HDMI"

        override fun read(): DisplayState = display

        override fun request(modeId: Int) {
            requested += modeId
        }

        override suspend fun awaitMode(modeId: Int, timeoutMs: Long): Boolean {
            waits += timeoutMs
            if (answers) display = display.copy(activeModeId = modeId)
            return answers
        }
    }

    private class FakeMemory : FrameRateMemory {
        val timeouts = mutableMapOf<String, Int>()
        var hintTaken = false

        override suspend fun timeouts(displayKey: String) = timeouts[displayKey] ?: 0

        override suspend fun setTimeouts(displayKey: String, count: Int) {
            timeouts[displayKey] = count
        }

        override suspend fun turnedOffHdrTypes(displayKey: String) = emptySet<Int>()

        override suspend fun setTurnedOffHdrTypes(displayKey: String, types: Set<Int>) = Unit

        override suspend fun takeHint(): Boolean = !hintTaken.also { hintTaken = true }
    }

    private val streamer = ModeChoiceTest.STREAMER.copy(matchContent = MatchContent.Always)
    private val film = MatchRequest(fps = 23.976, durationMs = 7_200_000)

    @Test
    fun switchesWaitsForTheDisplayThenSettles() = runTest {
        val host = FakeHost(streamer)
        val controller = FrameRateController(host, FakeMemory(), sdk = 34)
        var switching = false
        val outcome = controller.match(film) { switching = true }
        assertTrue(switching)
        assertTrue(outcome.switched)
        assertEquals(listOf(46), host.requested)
        assertEquals(listOf(FrameRateController.SWITCH_TIMEOUT_MS), host.waits)
        assertEquals(FrameRateController.SETTLE_MS, currentTime)
        assertEquals(46, controller.requestedModeId)
    }

    @Test
    fun aMatchingModeNeedsNoWait() = runTest {
        val host = FakeHost(streamer)
        val outcome = FrameRateController(host, FakeMemory(), sdk = 34).match(film.copy(fps = 60.0))
        assertEquals(ModeDecision.Keep(KeepReason.AlreadyMatching), outcome.decision)
        assertEquals(emptyList<Int>(), host.requested)
        assertEquals(0L, currentTime)
    }

    @Test
    fun twoTimeoutsInARowStopAutoAndShowTheHintOnce() = runTest {
        val host = FakeHost(streamer, answers = false)
        val memory = FakeMemory()
        val controller = FrameRateController(host, memory, sdk = 34)

        val first = controller.match(film)
        assertTrue(first.timedOut)
        assertFalse(first.showHint)
        val second = controller.match(film)
        assertTrue(second.timedOut)
        assertTrue(second.showHint)
        assertEquals(2, memory.timeouts["0:HDMI"])

        val third = controller.match(film)
        assertEquals(ModeDecision.Keep(KeepReason.GaveUp), third.decision)
        assertFalse("the hint shows once", third.showHint)
        assertEquals(listOf(46, 46), host.requested)

        // Always still asks.
        val always = controller.match(film.copy(setting = RefreshRateMatching.Always))
        assertTrue(always.timedOut)
    }

    @Test
    fun aSuccessResetsTheTimeouts() = runTest {
        val host = FakeHost(streamer, answers = false)
        val memory = FakeMemory()
        val controller = FrameRateController(host, memory, sdk = 34)
        controller.match(film)
        host.answers = true
        assertTrue(controller.match(film).switched)
        assertEquals(0, memory.timeouts["0:HDMI"])
    }

    @Test
    fun clearHandsTheModeBackOnce() = runTest {
        val host = FakeHost(streamer)
        val controller = FrameRateController(host, FakeMemory(), sdk = 34)
        controller.clear()
        assertEquals(emptyList<Int>(), host.requested)
        controller.match(film)
        controller.clear()
        controller.clear()
        assertEquals(listOf(46, 0), host.requested)
    }
}
