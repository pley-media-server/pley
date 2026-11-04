package app.pley.android.player.session

import app.pley.android.fixtures.FixtureIds
import app.pley.android.model.Control
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

/** The profile's remote, on another device (docs/API.md, "Remote control"): what a video obeys, and that it reports at once. */
@RunWith(RobolectricTestRunner::class)
class RemoteTest {
    private val events = MutableSharedFlow<ServerEvent>(extraBufferCapacity = 8)

    /** What the player told the app of its session, in order (LiveUpdates.playing). */
    private val held = mutableListOf<Boolean>()

    /** The sessions the player said it plays, in order (RemoteRepository.here). */
    private val here = mutableListOf<String?>()

    /** The sessions held for the remote now, by who said so. */
    private val holding = mutableMapOf<Any, String>()
    private val h = SessionHarness(
        caps = capsOf(TestDevices.sdrPhone),
        events = events,
        playing = { _, playing -> held += playing },
        here = { player, id ->
            here += id
            if (id != null) holding[player] = id else holding -= player
        },
    )

    @After
    fun close() = h.close()

    private fun playing(itemId: Long = SessionHarness.S1E1): SessionPlaybackController {
        val c = h.play(PlayRequest(itemId, startMs = 0))
        h.engine.ready()
        // The first progress report names the session.
        h.advance(10_500)
        return c
    }

    private fun sessionId(itemId: Long = SessionHarness.S1E1): String = h.progress(itemId).last().second.sessionId!!

    private fun command(control: Control, positionMs: Long? = null, offsetMs: Long? = null, session: String = sessionId()) {
        events.tryEmit(ServerEvent.PlayerCommand(session, control, positionMs, offsetMs))
        h.settle()
    }

    @Test
    fun `a video says what it obeys, next only when something can come next`() {
        playing()
        assertEquals(listOf(Control.Play, Control.Pause, Control.Seek, Control.Skip, Control.Next), h.playbackRequests().last().controls)
        val movie = h.play(PlayRequest(FixtureIds.SAMPLE_1080P, startMs = 0))
        assertEquals(listOf(Control.Play, Control.Pause, Control.Seek, Control.Skip), h.playbackRequests().last().controls)
        movie.close()
    }

    @Test
    fun `a command for another session changes nothing`() {
        val c = playing()
        val reports = h.progress(SessionHarness.S1E1).size
        command(Control.Pause, session = "someone-else")
        command(Control.Seek, positionMs = 5_000, session = "someone-else")
        assertTrue(h.engine.playWhenReady)
        assertTrue(h.engine.seeks.isEmpty())
        assertEquals(PlayerPhase.Playing, c.state.value.phase)
        assertEquals(reports, h.progress(SessionHarness.S1E1).size)
    }

    @Test
    fun `pause and play act, and are reported at once`() {
        playing()
        command(Control.Pause)
        assertFalse(h.engine.playWhenReady)
        assertEquals(ProgressState.Paused, h.progress(SessionHarness.S1E1).last().second.state)

        command(Control.Play)
        assertTrue(h.engine.playWhenReady)
        assertEquals(ProgressState.Playing, h.progress(SessionHarness.S1E1).last().second.state)
    }

    @Test
    fun `a seek goes there and is reported straight away, within the title`() {
        val c = playing()
        val before = h.progress(SessionHarness.S1E1).size
        command(Control.Seek, positionMs = 4_000)
        assertEquals(4_000L, h.engine.seeks.last())
        // Not a second later, as after a seek on the screen.
        val reported = h.progress(SessionHarness.S1E1)
        assertEquals(before + 1, reported.size)
        assertEquals(4_000L, reported.last().second.positionMs)

        command(Control.Seek, positionMs = 10 * 60 * 60_000L)
        val end = c.state.value.durationMs - SessionPlaybackController.SEEK_END_MARGIN_MS
        assertEquals(end, h.engine.seeks.last())
        assertEquals(end, h.progress(SessionHarness.S1E1).last().second.positionMs)
    }

    @Test
    fun `a skip goes from where it is now, and stops at the start`() {
        playing()
        command(Control.Seek, positionMs = 6_000)
        command(Control.Skip, offsetMs = 3_000)
        assertEquals(9_000L, h.engine.seeks.last())
        assertEquals(9_000L, h.progress(SessionHarness.S1E1).last().second.positionMs)
        command(Control.Skip, offsetMs = -30_000)
        assertEquals(0L, h.engine.seeks.last())
    }

    @Test
    fun `next plays the next episode, and nothing at the end, without a word here`() {
        val c = playing()
        command(Control.Next)
        assertEquals(listOf(FixtureIds.SHOW_S1E1, FixtureIds.SHOW_S1E2), h.playbackRequests().map { it.itemId })
        assertEquals(FixtureIds.SHOW_S1E2, c.state.value.itemId)
        c.close()
        h.settle()

        val movie = h.play(PlayRequest(FixtureIds.SAMPLE_1080P, startMs = 0))
        h.engine.ready()
        h.advance(10_500)
        val requests = h.playbackRequests().size
        command(Control.Next, session = sessionId(FixtureIds.SAMPLE_1080P))
        command(Control.Previous, session = sessionId(FixtureIds.SAMPLE_1080P))
        assertEquals(requests, h.playbackRequests().size)
        assertNull(movie.state.value.notice)
    }

    @Test
    fun `a stopped player obeys nothing more`() {
        val c = playing()
        val id = sessionId()
        events.tryEmit(ServerEvent.SessionStopped(id, null, byAdmin = false))
        h.settle()
        command(Control.Play, session = id)
        assertFalse(h.engine.playWhenReady)
        assertTrue(c.state.value.stoppedByServer)
    }

    @Test
    fun `the event stream is held from the first session to the end of playback`() {
        val c = playing()
        assertEquals(true, held.last())
        // The next episode keeps it.
        command(Control.Next)
        assertTrue(held.all { it })
        c.close()
        h.settle()
        assertEquals(false, held.last())
    }

    @Test
    fun `the remote is told which session plays here, so it shows none for this device`() {
        val c = playing()
        val first = sessionId()
        assertEquals(first, here.last())
        // The next episode is another session.
        command(Control.Next)
        assertEquals(FixtureIds.SHOW_S1E2, c.state.value.itemId)
        assertTrue(here.last() != null && here.last() != first)
        c.close()
        h.settle()
        assertNull(here.last())
    }

    @Test
    fun `a title that ends with nothing next lets the event stream go`() {
        val movie = h.play(PlayRequest(FixtureIds.SAMPLE_1080P, startMs = 0))
        h.engine.ready()
        h.advance(10_500)
        assertEquals(true, held.last())
        h.engine.end()
        h.advance(500)
        assertEquals(PlayerPhase.Ended, movie.state.value.phase)
        assertEquals(false, held.last())
    }

    @Test
    fun `a session made ready beside the one playing is held from its start, not shown as another device's`() {
        val c = h.play(PlayRequest(FixtureIds.SAMPLE_1080P, startMs = 0))
        h.engine.ready()
        h.advance(10_500)
        val first = sessionId(FixtureIds.SAMPLE_1080P)
        h.warmupMs = 3_000
        c.selectQuality("4m")
        h.settle()
        // Still getting ready: both are this device's.
        val next = holding.values.single { it != first }
        assertEquals(setOf(first, next), holding.values.toSet())

        h.advance(3_000)
        assertEquals(listOf(next), holding.values.toList())
    }

    @Test
    fun `a session the server stopped lets the event stream go`() {
        playing()
        events.tryEmit(ServerEvent.SessionStopped(sessionId(), null, byAdmin = true))
        h.settle()
        assertEquals(false, held.last())
    }
}
