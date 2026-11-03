package app.pley.android.screens.cast

import app.pley.android.model.CastAction
import app.pley.android.model.CastPlayState
import app.pley.android.model.CastReport
import app.pley.android.model.ServerEvent
import app.pley.android.player.PlayRequest
import app.pley.android.player.PlaybackController
import app.pley.android.player.PlayerPhase
import app.pley.android.player.PlayerUiState
import app.pley.android.testing.player.session.FakePlaybackController
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** A TV being cast to: what it plays when told, how it steers its player, and what it says it plays. */
@OptIn(ExperimentalCoroutinesApi::class)
class CastReceiverTest {
    private class Player(override val controller: FakePlaybackController = FakePlaybackController()) : CastReceiver.Player {
        var exits = 0

        override fun exit() {
            exits++
        }
    }

    private class Setup(val cast: FakeCast, val launcher: RecordingLauncher, val receiver: CastReceiver)

    private fun TestScope.setup(ready: Boolean = true): Setup {
        val cast = FakeCast { testScheduler.currentTime }
        val launcher = RecordingLauncher()
        val receiver = CastReceiver(cast, launcher, backgroundScope) { testScheduler.currentTime }
        receiver.ready.value = ready
        receiver.start()
        runCurrent()
        return Setup(cast, launcher, receiver)
    }

    private fun playing(itemId: Long = 101, positionMs: Long = 0, phase: PlayerPhase = PlayerPhase.Playing) =
        PlayerUiState(phase = phase, itemId = itemId, positionMs = positionMs, durationMs = 6_000_000)

    @Test
    fun `a title sent plays from its position, or from where the profile left off without asking`() = runTest {
        val s = setup()
        s.cast.commands.emit(ServerEvent.CastPlay("tv", 101, fileId = 1001, positionMs = 754_000))
        s.cast.commands.emit(ServerEvent.CastPlay("tv", 203))
        runCurrent()
        assertEquals(listOf(PlayRequest(101, 1001, startMs = 754_000), PlayRequest(203)), s.launcher.played)
        assertTrue(s.launcher.played.none { it.fromStart })
    }

    @Test
    fun `what plays leaves as Back would before the title sent opens`() = runTest {
        val s = setup()
        val open = Player()
        s.receiver.opened(open)
        s.receiver.handle(ServerEvent.CastPlay("tv", 203))
        assertEquals(1, open.exits)
        assertEquals(PlayRequest(203), s.launcher.played.single())
    }

    @Test
    fun `nothing is played while signed out or choosing a profile`() = runTest {
        val s = setup(ready = false)
        s.receiver.handle(ServerEvent.CastPlay("tv", 101))
        assertTrue(s.launcher.played.isEmpty())
        s.receiver.ready.value = true
        s.receiver.handle(ServerEvent.CastPlay("tv", 101))
        assertEquals(1, s.launcher.played.size)
    }

    @Test
    fun `commands steer the open player, cast or not, and nothing without one`() = runTest {
        val s = setup()
        s.receiver.handle(ServerEvent.CastControl("tv", CastAction.Pause))
        val open = Player()
        s.receiver.opened(open)
        s.receiver.handle(ServerEvent.CastControl("tv", CastAction.Pause))
        s.receiver.handle(ServerEvent.CastControl("tv", CastAction.Resume))
        s.receiver.handle(ServerEvent.CastControl("tv", CastAction.Seek, 90_000))
        // A seek without a position, and an action this app doesn't know, do nothing.
        s.receiver.handle(ServerEvent.CastControl("tv", CastAction.Seek))
        s.receiver.handle(ServerEvent.CastControl("tv", CastAction.Unknown))
        assertEquals(listOf("pause", "play", "seekTo"), open.controller.calls.map { it.name })
        assertEquals(listOf<Any?>(90_000L), open.controller.calls("seekTo").single().args)

        s.receiver.handle(ServerEvent.CastControl("tv", CastAction.Stop))
        assertEquals(1, open.exits)
        s.receiver.closed(open)
        s.receiver.handle(ServerEvent.CastControl("tv", CastAction.Pause))
        assertEquals(3, open.controller.calls.size)
    }

    @Test
    fun `the open player says what it plays, and nothing once it closes`() = runTest {
        val s = setup()
        val open = Player(FakePlaybackController(PlayerUiState()))
        s.receiver.opened(open)
        runCurrent()
        // The title isn't known yet: nothing to say.
        assertTrue(s.cast.reports.isEmpty())

        open.controller.set(playing(phase = PlayerPhase.Preparing))
        runCurrent()
        open.controller.set(playing(positionMs = 500))
        runCurrent()
        assertEquals(
            listOf(CastReport(101, 0, 6_000_000, CastPlayState.Buffering), CastReport(101, 500, 6_000_000, CastPlayState.Playing)),
            s.cast.reports,
        )

        s.receiver.closed(open)
        assertEquals(CastReport(), s.cast.reports.last())
        // Closed: its changes say nothing more.
        open.controller.set(playing(positionMs = 9_000))
        runCurrent()
        assertEquals(3, s.cast.reports.size)
    }

    @Test
    fun `a player that takes another's place reports alone`() = runTest {
        val s = setup()
        val first = Player(FakePlaybackController(playing()))
        s.receiver.opened(first)
        runCurrent()
        val second = Player(FakePlaybackController(playing(itemId = 203)))
        s.receiver.opened(second)
        runCurrent()
        // The first leaves after the second came: nothing says the TV plays nothing.
        s.receiver.closed(first)
        first.controller.set(playing(positionMs = 9_000))
        runCurrent()
        assertEquals(listOf(101L, 203L), s.cast.reports.map { it.itemId })
    }
}

/** When a TV says what it plays. */
class CastReportsTest {
    private fun state(itemId: Long = 101, positionMs: Long = 0, phase: PlayerPhase = PlayerPhase.Playing, speed: Float = 1f) =
        PlayerUiState(phase = phase, itemId = itemId, positionMs = positionMs, durationMs = 6_000_000, speed = speed)

    @Test
    fun `a start, a pause, a resume and buffering are said at once`() {
        val reports = CastReports()
        assertEquals(CastPlayState.Buffering, reports.next(state(phase = PlayerPhase.Preparing), 0)?.state)
        assertEquals(CastPlayState.Playing, reports.next(state(positionMs = 100), 100)?.state)
        assertNull(reports.next(state(positionMs = 600), 600))
        assertEquals(CastPlayState.Paused, reports.next(state(positionMs = 1_000, phase = PlayerPhase.Paused), 1_000)?.state)
        assertNull(reports.next(state(positionMs = 1_000, phase = PlayerPhase.Paused), 30_000))
        assertEquals(CastPlayState.Playing, reports.next(state(positionMs = 1_000), 31_000)?.state)
        assertEquals(CastPlayState.Buffering, reports.next(state(positionMs = 1_200, phase = PlayerPhase.Buffering), 31_200)?.state)
    }

    @Test
    fun `every 10 s while it plays`() {
        val reports = CastReports()
        reports.next(state(), 0)
        var said = 0
        for (t in 500L..30_000L step 500) {
            if (reports.next(state(positionMs = t), t) != null) said++
        }
        assertEquals(3, said)
    }

    @Test
    fun `a seek is said, a speed that isn't 1 is not a seek`() {
        val reports = CastReports()
        reports.next(state(), 0)
        assertEquals(90_000L, reports.next(state(positionMs = 90_000), 500)?.positionMs)
        // Back 10 s while paused.
        reports.next(state(positionMs = 90_000, phase = PlayerPhase.Paused), 1_000)
        assertEquals(80_000L, reports.next(state(positionMs = 80_000, phase = PlayerPhase.Paused), 1_500)?.positionMs)

        val fast = CastReports()
        fast.next(state(speed = 2f), 0)
        assertNull(fast.next(state(positionMs = 8_000, speed = 2f), 4_000))
    }

    @Test
    fun `the end and a failure play nothing, the next episode is a new title`() {
        val reports = CastReports()
        reports.next(state(positionMs = 5_000_000), 0)
        assertEquals(CastReport(), reports.next(state(positionMs = 6_000_000, phase = PlayerPhase.Ended), 100))
        assertNull(reports.closed())
        assertEquals(203L, reports.next(state(itemId = 203), 200)?.itemId)
        assertEquals(CastReport(), reports.next(state(itemId = 203, phase = PlayerPhase.Error("failed", canRetry = true)), 300))
        // Closing a player that never knew its title says nothing.
        assertNull(CastReports().closed())
    }
}
