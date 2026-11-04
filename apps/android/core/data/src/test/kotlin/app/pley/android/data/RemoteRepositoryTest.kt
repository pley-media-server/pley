@file:OptIn(ExperimentalCoroutinesApi::class)

package app.pley.android.data

import app.pley.android.model.Control
import app.pley.android.model.ItemKind
import app.pley.android.model.ItemSummary
import app.pley.android.model.PlayerCommandRequest
import app.pley.android.model.PlayingChannel
import app.pley.android.model.PlayingKind
import app.pley.android.model.PlayingSession
import app.pley.android.model.ProgressState
import app.pley.android.model.ServerEvent
import app.pley.android.network.ApiException
import app.pley.android.network.LiveEvent
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder

/**
 * The remote's repository over the real data side: the profile's
 * sessions (sessions/list.json: an episode on the Living room TV, a
 * channel on the Kitchen phone) read when followed and again at each event
 * that may change them, without this device's own; and the web's rules
 * for estimating a position and telling that a player answered.
 */
class RemoteRepositoryTest {
    @get:Rule
    val folder = TemporaryFolder()

    private fun TestScope.time(ms: Long) {
        testScheduler.advanceTimeBy(ms)
        testScheduler.runCurrent()
    }

    @Test
    fun `followed, it reads the sessions, each kept with when its player reported by this device's clock`() = runTest {
        val data = signedIn(folder.newFolder())
        time(10_000)
        val remote = data.graph.remote
        collect(remote.sessions)
        testScheduler.runCurrent()
        val sessions = remote.sessions.value
        assertEquals(listOf("6f1c2a9e", "5d0f7f2a"), sessions.map { it.sessionId })
        // Answered at 10 s: reported 4.2 s and 0.9 s before that.
        assertEquals(listOf(5_800L, 9_100L), sessions.map { it.reportedAt })
        assertEquals(10_000L, remote.clock())
    }

    @Test
    fun `this device's own sessions are left out, as long as its players play them`() = runTest {
        val data = signedIn(folder.newFolder())
        val remote = data.graph.remote
        collect(remote.sessions)
        testScheduler.runCurrent()
        val video = Any()
        val music = Any()
        remote.here(video, "5d0f7f2a")
        testScheduler.runCurrent()
        assertEquals(listOf("6f1c2a9e"), remote.sessions.value.map { it.sessionId })

        remote.here(music, "6f1c2a9e")
        testScheduler.runCurrent()
        assertEquals(emptyList<RemoteSession>(), remote.sessions.value)
    }

    @Test
    fun `a session this device let go doesn't come back as another device's, before the list is read again or after`() = runTest {
        val data = signedIn(folder.newFolder())
        val remote = data.graph.remote
        collect(remote.sessions)
        testScheduler.runCurrent()
        val music = Any()
        remote.here(music, "5d0f7f2a")
        testScheduler.runCurrent()

        // The next track: another session, the one before ended here but still in the list read before.
        remote.here(music, "6f1c2a9e")
        testScheduler.runCurrent()
        assertEquals(emptyList<RemoteSession>(), remote.sessions.value)
        remote.here(music, null)
        testScheduler.runCurrent()
        assertEquals(emptyList<RemoteSession>(), remote.sessions.value)
        assertTrue(remote.refresh()!!.isNotEmpty())
        assertEquals(emptyList<RemoteSession>(), remote.sessions.value)
    }

    @Test
    fun `sessions changed, a session stopped and every connection read them again, nothing else`() = runTest {
        val data = signedIn(folder.newFolder())
        val remote = data.graph.remote
        collect(remote.sessions)
        testScheduler.runCurrent()
        assertTrue(data.events.send(LiveEvent.Reconnected))
        testScheduler.runCurrent()
        val reads = data.api.calls("playingSessions").size

        data.api.answer("playingSessions", emptyList<PlayingSession>())
        data.events.send(LiveEvent.Event(ServerEvent.SessionsChanged))
        testScheduler.runCurrent()
        assertEquals(reads + 1, data.api.calls("playingSessions").size)
        assertEquals(emptyList<RemoteSession>(), remote.sessions.value)

        data.events.send(LiveEvent.Event(ServerEvent.SessionStopped("6f1c2a9e", null, byAdmin = false)))
        testScheduler.runCurrent()
        data.events.send(LiveEvent.Dropped)
        data.events.send(LiveEvent.Reconnected)
        testScheduler.runCurrent()
        assertEquals(reads + 3, data.api.calls("playingSessions").size)

        data.events.send(LiveEvent.Event(ServerEvent.LibrariesChanged))
        data.events.send(LiveEvent.Event(ServerEvent.PlayerCommand("6f1c2a9e", Control.Pause)))
        testScheduler.runCurrent()
        assertEquals(reads + 3, data.api.calls("playingSessions").size)
    }

    @Test
    fun `nobody following, nothing is read`() = runTest {
        val data = signedIn(folder.newFolder())
        data.graph.remote
        data.graph.liveUpdates.start()
        testScheduler.runCurrent()
        data.events.send(LiveEvent.Event(ServerEvent.SessionsChanged))
        testScheduler.runCurrent()
        assertEquals(0, data.api.calls("playingSessions").size)
    }

    @Test
    fun `a failed read keeps what showed, and an older server has no remote`() = runTest {
        val data = signedIn(folder.newFolder())
        val remote = data.graph.remote
        collect(remote.sessions)
        testScheduler.runCurrent()
        data.api.fail("playingSessions", ApiException(500, "internal", "broken"))
        assertNull(remote.refresh())
        assertEquals(2, remote.sessions.value.size)

        data.api.reset()
        data.api.fail("playingSessions", ApiException(404, "not_found", "no route"))
        assertEquals(emptyList<RemoteSession>(), remote.refresh())
        testScheduler.runCurrent()
        assertEquals(emptyList<RemoteSession>(), remote.sessions.value)
    }

    @Test
    fun `commands go to the session, and one gone or refused has the list read again`() = runTest {
        val data = signedIn(folder.newFolder())
        val remote = data.graph.remote
        collect(remote.sessions)
        testScheduler.runCurrent()
        val reads = data.api.calls("playingSessions").size

        remote.command("6f1c2a9e", PlayerCommandRequest(Control.Skip, offsetMs = 30_000))
        testScheduler.runCurrent()
        assertEquals(listOf("6f1c2a9e", PlayerCommandRequest(Control.Skip, offsetMs = 30_000)), data.api.calls("sendCommand").single().args)
        assertEquals(reads, data.api.calls("playingSessions").size)

        data.api.fail("sendCommand", ApiException(404, "not_found", "no such session"))
        val refused = runCatching { remote.command("6f1c2a9e", PlayerCommandRequest(Control.Pause)) }.exceptionOrNull()
        testScheduler.runCurrent()
        assertEquals(404, (refused as ApiException).status)
        assertEquals(reads + 1, data.api.calls("playingSessions").size)

        remote.stop("5d0f7f2a")
        testScheduler.runCurrent()
        assertEquals(listOf("5d0f7f2a"), data.api.calls("stopSession").single().args)
        assertEquals(reads + 2, data.api.calls("playingSessions").size)
    }

    @Test
    fun `while it plays the position moves on with this device's clock, never past the end`() {
        val playing = remote(session(state = ProgressState.Playing, positionMs = 60_000, durationMs = 100_000), reportedAt = 1_000)
        assertEquals(60_000L, playing.positionAt(1_000))
        assertEquals(65_000L, playing.positionAt(6_000))
        assertEquals(100_000L, playing.positionAt(500_000))
        // A clock behind the report doesn't move it back.
        assertEquals(60_000L, playing.positionAt(0))

        val paused = remote(session(state = ProgressState.Paused, positionMs = 60_000, durationMs = 100_000), reportedAt = 1_000)
        assertEquals(60_000L, paused.positionAt(30_000))
        val unknownLength = remote(session(state = ProgressState.Playing, positionMs = 60_000, durationMs = null), reportedAt = 0)
        assertEquals(560_000L, unknownLength.positionAt(500_000))
    }

    @Test
    fun `a pause is answered by the state changing, a next by the session going or playing something else`() {
        val before = remote(session(state = ProgressState.Playing), reportedAt = 0)
        val pause = SentCommand(Control.Pause, before, sentAt = 1_000)
        assertFalse(RemoteRepository.answered(pause, before, 2_000))
        // Reported again, still playing: not yet.
        assertFalse(RemoteRepository.answered(pause, remote(session(state = ProgressState.Playing, positionMs = 62_000, updatedAt = 110), 2_000), 2_000))
        assertTrue(RemoteRepository.answered(pause, remote(session(state = ProgressState.Paused), 2_000), 2_000))

        val next = SentCommand(Control.Next, before, sentAt = 1_000)
        assertTrue(RemoteRepository.answered(next, null, 2_000))
        assertTrue(RemoteRepository.answered(next, remote(session(itemId = 205), 2_000), 2_000))
        assertFalse(RemoteRepository.answered(next, before, 2_000))
    }

    @Test
    fun `a seek or a skip is answered by a report from somewhere new, or from where it was sent to`() {
        // At 60 s, playing, reported at 0.
        val before = remote(session(state = ProgressState.Playing, positionMs = 60_000, updatedAt = 100), reportedAt = 0)
        val skip = SentCommand(Control.Skip, before, sentAt = 1_000, target = 91_000)
        // Not reported since.
        assertFalse(RemoteRepository.answered(skip, before, 3_000))
        // Reported where it would have played to anyway: nothing moved.
        assertFalse(RemoteRepository.answered(skip, remote(session(state = ProgressState.Playing, positionMs = 62_000, updatedAt = 102), 2_000), 3_000))
        // Reported where it was sent to.
        assertTrue(RemoteRepository.answered(skip, remote(session(state = ProgressState.Playing, positionMs = 91_500, updatedAt = 102), 2_000), 3_000))

        // A seek a few seconds from where it is moves nothing a report shows, but lands on its target.
        val seek = SentCommand(Control.Seek, before, sentAt = 1_000, target = 63_000)
        assertTrue(RemoteRepository.answered(seek, remote(session(state = ProgressState.Playing, positionMs = 63_200, updatedAt = 102), 2_000), 3_000))
    }

    @Test
    fun `where a command should take the session`() {
        val at = remote(session(state = ProgressState.Playing, positionMs = 60_000, durationMs = 100_000), reportedAt = 0)
        assertEquals(30_000L, RemoteRepository.target(at, Control.Seek, 2_000, positionMs = 30_000))
        assertEquals(92_000L, RemoteRepository.target(at, Control.Skip, 2_000, offsetMs = 30_000))
        assertEquals(100_000L, RemoteRepository.target(at, Control.Skip, 30_000, offsetMs = 30_000))
        assertEquals(0L, RemoteRepository.target(at, Control.Skip, 0, offsetMs = -600_000))
        assertEquals(0L, RemoteRepository.target(at, Control.Previous, 0))
        assertNull(RemoteRepository.target(at, Control.Pause, 0))
        val channel = remote(session(kind = PlayingKind.Live), reportedAt = 0)
        assertNull(RemoteRepository.target(channel, Control.Previous, 0))
    }

    @Test
    fun `devices of the same name are numbered, devices without one called as given`() {
        val sessions = listOf(
            remote(session(id = "a", device = "Living room TV"), 0),
            remote(session(id = "b", device = null), 0),
            remote(session(id = "c", device = "Living room TV"), 0),
            remote(session(id = "d", device = "Kitchen phone"), 0),
        )
        assertEquals(
            mapOf("a" to "Living room TV (1)", "b" to "Unknown device", "c" to "Living room TV (2)", "d" to "Kitchen phone"),
            RemoteRepository.deviceNames(sessions, "Unknown device"),
        )
    }

    private fun remote(session: PlayingSession, reportedAt: Long) = RemoteSession(session, reportedAt)

    private fun session(
        id: String = "s1",
        kind: PlayingKind = PlayingKind.Item,
        device: String? = "Living room TV",
        itemId: Long = 204,
        state: ProgressState = ProgressState.Playing,
        positionMs: Long = 60_000,
        durationMs: Long? = 3_209_000,
        updatedAt: Long = 100,
    ) = PlayingSession(
        sessionId = id,
        kind = kind,
        device = device,
        item = ItemSummary(itemId, 2, ItemKind.Episode, "Half Loop").takeIf { kind == PlayingKind.Item },
        channel = PlayingChannel(1, "Pley News HD").takeIf { kind == PlayingKind.Live },
        state = state,
        positionMs = positionMs,
        durationMs = durationMs,
        updatedAt = updatedAt,
    )
}
