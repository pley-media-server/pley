package app.pley.android.player.session

import app.pley.android.model.ProgressState
import app.pley.android.model.ServerEvent
import app.pley.android.player.PlayRequest
import app.pley.android.player.PlayerPhase
import app.pley.android.testing.caps.TestDevices
import app.pley.android.testing.caps.capsOf
import kotlinx.coroutines.flow.MutableSharedFlow
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

/** A playback the server stopped (DELETE /api/sessions/{id}): by its event, or by its stream's 410. */
@RunWith(RobolectricTestRunner::class)
class StopTest {
    private val events = MutableSharedFlow<ServerEvent>(extraBufferCapacity = 8)
    private val h = SessionHarness(caps = capsOf(TestDevices.sdrPhone), events = events)

    @After
    fun close() = h.close()

    private fun playing(): SessionPlaybackController {
        val c = h.play(PlayRequest(SessionHarness.S1E1, startMs = 0))
        h.engine.ready()
        // The first progress report names the session.
        h.advance(10_500)
        return c
    }

    private fun sessionId(): String = h.progress(SessionHarness.S1E1).last().second.sessionId!!

    @Test
    fun `the event stops the player, says who and why, and nothing plays again`() {
        val c = playing()
        val id = sessionId()
        events.tryEmit(ServerEvent.SessionStopped(id, "Server maintenance", byAdmin = true))
        h.settle()
        val phase = c.state.value.phase as PlayerPhase.Error
        assertFalse(phase.canRetry)
        assertTrue(phase.message, phase.message.contains("administrator"))
        assertTrue(phase.message, phase.message.contains("Server maintenance"))
        assertTrue(c.state.value.stoppedByServer)
        assertFalse(h.engine.playWhenReady)
        // Where it was is kept; the server ended the session itself, so nothing deletes it.
        assertEquals(ProgressState.Stopped, h.progress(SessionHarness.S1E1).last().second.state)
        assertTrue(h.deletes().isEmpty())
        // Nothing starts it again.
        c.retry()
        c.play()
        c.seekTo(30_000)
        h.advance(1_000)
        assertEquals(1, h.playbackRequests().size)
    }

    @Test
    fun `stopped from another device says so`() {
        val c = playing()
        events.tryEmit(ServerEvent.SessionStopped(sessionId(), null, byAdmin = false))
        h.settle()
        val phase = c.state.value.phase as PlayerPhase.Error
        assertEquals("Playback was stopped from another device.", phase.message)
    }

    @Test
    fun `another session's stop changes nothing`() {
        val c = playing()
        events.tryEmit(ServerEvent.SessionStopped("someone-else", "Bye", byAdmin = true))
        h.advance(1_000)
        assertEquals(PlayerPhase.Playing, c.state.value.phase)
        assertFalse(c.state.value.stoppedByServer)
    }

    @Test
    fun `a 410 from the stream stops it too, for a player that missed the event`() {
        val c = playing()
        h.engine.fail(EngineError.Stopped(null, "ERROR_CODE_IO_BAD_HTTP_STATUS: 410"))
        h.settle()
        val phase = c.state.value.phase as PlayerPhase.Error
        assertEquals("Playback was stopped by the server's administrator.", phase.message)
        assertFalse(phase.canRetry)
        // No new session is asked for, as an expired one would be.
        h.advance(5_000)
        assertEquals(1, h.playbackRequests().size)
    }

    @Test
    fun `a 410's message comes from its body`() {
        assertEquals("Disk full", Media3EngineLink.stopMessage("""{"code":"session_stopped","error":"session_stopped","message":"Disk full"}""".toByteArray()))
        assertNull(Media3EngineLink.stopMessage("""{"error":"session_stopped","message":null}""".toByteArray()))
        assertNull(Media3EngineLink.stopMessage("not json".toByteArray()))
        assertNull(Media3EngineLink.stopMessage(null))
    }
}
