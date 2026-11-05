@file:OptIn(ExperimentalCoroutinesApi::class)

package app.pley.android.data

import app.pley.android.model.CastKind
import app.pley.android.model.CastRequest
import app.pley.android.model.Control
import app.pley.android.model.DeviceKind
import app.pley.android.model.ItemKind
import app.pley.android.model.ItemSummary
import app.pley.android.model.Me
import app.pley.android.model.PlayerCommandRequest
import app.pley.android.model.PlayingChannel
import app.pley.android.model.PlayingKind
import app.pley.android.model.PlayingSession
import app.pley.android.model.ProgressState
import app.pley.android.model.RemoteDevice
import app.pley.android.model.ServerEvent
import app.pley.android.network.ApiException
import app.pley.android.network.LiveEvent
import app.pley.android.testing.Fixtures
import app.pley.android.testing.network.FakePleyApi
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.async
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
 * channel on the Kitchen phone) and devices (remote/devices.json: the
 * Living room TV and a browser) read when followed and again at each
 * event that may change them, without this device's own; and the web's
 * rules for listing devices, estimating a position and telling that a
 * player answered.
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
    fun `a read that comes back after a profile switch leaves the new profile's list as it is`() = runTest {
        val data = signedIn(folder.newFolder()) {
            apiOf[3] = FakePleyApi(issuer).apply { answer("playingSessions", listOf(session(id = "sams"))) }
        }
        val remote = data.graph.remote
        collect(remote.sessions)
        testScheduler.runCurrent()
        assertEquals(listOf("6f1c2a9e", "5d0f7f2a"), remote.sessions.value.map { it.sessionId })

        // Alex's list is asked for, and comes back once Sam's is read.
        data.api.hold()
        val late = async { remote.refresh() }
        testScheduler.runCurrent()
        val me: Me = Fixtures.decode("me/app_token.json")
        data.api.answer("me", me.copy(id = 3, username = "sam", name = "Sam", displayName = "Sam"))
        data.graph.session.switchProfile(3, pin = "1234")
        testScheduler.runCurrent()
        assertEquals(listOf("sams"), remote.sessions.value.map { it.sessionId })
        data.api.release()
        testScheduler.runCurrent()

        assertNull(late.await())
        assertEquals(listOf("sams"), remote.sessions.value.map { it.sessionId })
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
    fun `a cast goes to the receiver, and one to a device gone or no longer taking it has the lists read again`() = runTest {
        val data = signedIn(folder.newFolder())
        val remote = data.graph.remote
        collect(remote.devices)
        testScheduler.runCurrent()
        val reads = data.api.calls("remoteDevices").size

        remote.cast("tv-1", CastRequest(itemId = 204, startMs = 60_000))
        testScheduler.runCurrent()
        assertEquals(listOf("tv-1", CastRequest(itemId = 204, startMs = 60_000)), data.api.calls("cast").single().args)
        // Its session tells the remote it plays, through the events.
        assertEquals(reads, data.api.calls("remoteDevices").size)

        data.api.fail("cast", ApiException(404, "not_found", "no such device"))
        val gone = runCatching { remote.cast("tv-1", CastRequest(playlistId = 9)) }.exceptionOrNull()
        testScheduler.runCurrent()
        assertEquals(404, (gone as ApiException).status)
        assertEquals(reads + 1, data.api.calls("remoteDevices").size)

        data.api.fail("cast", ApiException(409, "not_castable", "this device doesn't play Live TV from a remote"))
        runCatching { remote.cast("tv-1", CastRequest(channelId = 1)) }
        testScheduler.runCurrent()
        assertEquals(reads + 2, data.api.calls("remoteDevices").size)

        // Too many at once changes nothing listed.
        data.api.fail("cast", ApiException(429, "too_many_requests", "10 casts a second at most"))
        runCatching { remote.cast("tv-1", CastRequest(itemId = 204)) }
        testScheduler.runCurrent()
        assertEquals(reads + 2, data.api.calls("remoteDevices").size)
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
    fun `a command shows what it asks at once, until it is answered or the wait is over`() {
        // At 60 s, playing, reported at 0; the commands go at 2 s.
        val playing = remote(session(state = ProgressState.Playing, positionMs = 60_000), reportedAt = 0)
        assertEquals(playing, RemoteRepository.asSent(playing, null))

        val seek = RemoteRepository.asSent(playing, SentCommand(Control.Seek, playing, sentAt = 2_000, target = 10_000))
        assertEquals(10_000L, seek.positionAt(2_000))
        assertEquals(13_000L, seek.positionAt(5_000))

        val paused = RemoteRepository.asSent(playing, SentCommand(Control.Pause, playing, sentAt = 2_000))
        assertEquals(ProgressState.Paused, paused.session.state)
        assertEquals(62_000L, paused.positionAt(20_000))
        // Where the controls showed it as the pause went: a skip just before it still on its way.
        val pausedAfterSkip = RemoteRepository.asSent(playing, SentCommand(Control.Pause, playing, sentAt = 2_000, target = 92_000))
        assertEquals(92_000L, pausedAfterSkip.positionAt(20_000))
        val stopped = remote(session(state = ProgressState.Paused, positionMs = 60_000), reportedAt = 0)
        val played = RemoteRepository.asSent(stopped, SentCommand(Control.Play, stopped, sentAt = 2_000))
        assertEquals(ProgressState.Playing, played.session.state)
        assertEquals(63_000L, played.positionAt(5_000))

        assertEquals(2, RemoteRepository.asSent(playing, SentCommand(Control.Audio, playing, 2_000, streamIndex = 2)).session.audioIndex)
        assertEquals(3, RemoteRepository.asSent(playing, SentCommand(Control.Subtitles, playing, 2_000, streamIndex = 3)).session.subtitleIndex)
        assertEquals("4m", RemoteRepository.asSent(playing, SentCommand(Control.Quality, playing, 2_000, quality = "4m")).session.quality)
        // What a next plays isn't known.
        assertEquals(playing, RemoteRepository.asSent(playing, SentCommand(Control.Next, playing, sentAt = 2_000)))
    }

    @Test
    fun `a session a command ended stays listed until its device plays again`() {
        val ended = remote(session(id = "s1", deviceId = "app-12"), reportedAt = 0)
        val kitchen = remote(session(id = "s2", device = "Kitchen phone", deviceId = "app-15"), reportedAt = 0)
        val tv = receiver("tv-1", "app-12", "Living room TV")
        val listed = listOf(OtherDevice("app-15", session = kitchen))

        // A device that only played comes back; one that takes casts plays it again.
        assertEquals(RemoteRepository.devicesOf(listOf(kitchen, ended), emptyList()), RemoteRepository.withEnded(listed, ended))
        val ready = RemoteRepository.devicesOf(listOf(kitchen), listOf(tv))
        assertEquals(OtherDevice("app-12", tv, ended), RemoteRepository.withEnded(ready, ended).first { it.deviceId == "app-12" })
        // Playing again, or still playing it: as listed.
        val again = RemoteRepository.devicesOf(listOf(kitchen, remote(session(id = "s3", deviceId = "app-12"), 0)), listOf(tv))
        assertEquals(again, RemoteRepository.withEnded(again, ended))
        assertEquals(listed, RemoteRepository.withEnded(listed, null))

        assertTrue(listOf(Control.Next, Control.Previous, Control.Audio, Control.Subtitles, Control.Quality).all(RemoteRepository::mayEndSession))
        assertFalse(listOf(Control.Play, Control.Pause, Control.Seek, Control.Skip).any(RemoteRepository::mayEndSession))
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
        val devices = listOf(
            OtherDevice("a", session = remote(session(id = "s1", device = "Living room TV"), 0)),
            OtherDevice("b", session = remote(session(id = "s2", device = null), 0)),
            OtherDevice("c", receiver = receiver("tv-2", "c", "Living room TV")),
            OtherDevice("d", session = remote(session(id = "s4", device = "Kitchen phone"), 0)),
        )
        assertEquals(
            mapOf("a" to "Living room TV (1)", "b" to "Unknown device", "c" to "Living room TV (2)", "d" to "Kitchen phone"),
            RemoteRepository.deviceNames(devices, "Unknown device") { name, n -> "$name ($n)" },
        )
    }

    @Test
    fun `the devices are each receiver with what its device plays, and each other player, playing first, then by name`() {
        val older = remote(session(id = "s1", device = "Living room TV", deviceId = "app-12", itemId = 203), 0)
        val newer = remote(session(id = "s2", device = "Living room TV", deviceId = "app-12", itemId = 204), 0)
        val jellyfin = remote(session(id = "s3", device = "Bedroom box", deviceId = "token-4"), 0)
        val devices = RemoteRepository.devicesOf(
            sessions = listOf(older, jellyfin, newer),
            receivers = listOf(
                receiver("web-1", "web-9f2c", "firefox on macOS", DeviceKind.Browser),
                receiver("tv-1", "app-12", "Living room TV", DeviceKind.Tv),
                receiver("box", "app-20", null),
                receiver("tv-2", "app-14", "Den TV", DeviceKind.Tv),
            ),
        )
        assertEquals(listOf("token-4", "app-12", "app-14", "web-9f2c", "app-20"), devices.map { it.deviceId })
        // A device with several sessions shows its newest.
        assertEquals("s2", devices[1].session?.sessionId)
        assertEquals(DeviceKind.Tv, devices[1].kind)
        assertTrue(devices[1].takes(CastKind.Item))
        // A Jellyfin app's: no receiver, nothing to cast to.
        assertEquals(null, devices[0].receiver)
        assertFalse(devices[0].takes(CastKind.Item))
        assertEquals(listOf(true, true, false, false, false), devices.map { it.playing })
    }

    @Test
    fun `followed, it reads the devices with the sessions, and a failed read keeps them`() = runTest {
        val data = signedIn(folder.newFolder())
        val remote = data.graph.remote
        collect(remote.devices)
        testScheduler.runCurrent()
        // The Living room TV plays and takes casts, the Kitchen phone plays and takes none, the browser plays nothing.
        assertEquals(listOf("Kitchen phone", "Living room TV", "Firefox on macOS"), remote.devices.value.map { it.name })
        assertEquals(listOf(true, true, false), remote.devices.value.map { it.playing })
        assertEquals(listOf(false, true, true), remote.devices.value.map { it.takes(CastKind.Item) })

        data.api.fail("remoteDevices", ApiException(500, "internal", "broken"))
        data.events.send(LiveEvent.Event(ServerEvent.SessionsChanged))
        testScheduler.runCurrent()
        assertEquals(3, remote.devices.value.size)
        // An older server has none.
        data.api.reset()
        data.api.fail("remoteDevices", ApiException(404, "not_found", "no route"))
        data.events.send(LiveEvent.Event(ServerEvent.SessionsChanged))
        testScheduler.runCurrent()
        assertEquals(listOf("Kitchen phone", "Living room TV"), remote.devices.value.map { it.name })
    }

    @Test
    fun `this device's own receiver is left out`() = runTest {
        // The install id the players send, which the event stream names this device by.
        val data = signedIn(folder.newFolder()) { legacy.values["device" to "client_id"] = "web-tab-41c9" }
        val remote = data.graph.remote
        collect(remote.devices)
        testScheduler.runCurrent()
        assertEquals(listOf("Kitchen phone", "Living room TV"), remote.devices.value.map { it.name })
    }

    @Test
    fun `a track is answered by the player saying it plays it, not by its state`() {
        val before = remote(session(state = ProgressState.Playing).copy(audioIndex = 1, subtitleIndex = -1), reportedAt = 0)
        val audio = SentCommand(Control.Audio, before, sentAt = 1_000, streamIndex = 2)
        assertFalse(RemoteRepository.answered(audio, before, 2_000))
        // Buffering while it switches is no answer yet.
        assertFalse(RemoteRepository.answered(audio, remote(before.session.copy(state = ProgressState.Buffering), 2_000), 2_000))
        assertTrue(RemoteRepository.answered(audio, remote(before.session.copy(audioIndex = 2), 2_000), 2_000))
        val off = SentCommand(Control.Subtitles, remote(before.session.copy(subtitleIndex = 3), 0), sentAt = 1_000, streamIndex = -1)
        assertTrue(RemoteRepository.answered(off, before, 2_000))
        // A new session for it, of the same title, says it as it starts.
        assertTrue(RemoteRepository.answered(audio, remote(before.session.copy(sessionId = "s9", audioIndex = 2), 2_000), 2_000))
    }

    @Test
    fun `a quality is answered by the player saying it picked it`() {
        val before = remote(session(state = ProgressState.Playing).copy(quality = "auto"), reportedAt = 0)
        val quality = SentCommand(Control.Quality, before, sentAt = 1_000, quality = "8m")
        assertFalse(RemoteRepository.answered(quality, before, 2_000))
        assertFalse(RemoteRepository.answered(quality, remote(before.session.copy(state = ProgressState.Buffering), 2_000), 2_000))
        assertTrue(RemoteRepository.answered(quality, remote(before.session.copy(quality = "8m"), 2_000), 2_000))
        // The new session its pick needed says it too.
        assertTrue(RemoteRepository.answered(quality, remote(before.session.copy(sessionId = "s9", quality = "8m"), 2_000), 2_000))
    }

    private fun remote(session: PlayingSession, reportedAt: Long) = RemoteSession(session, reportedAt)

    private fun receiver(id: String, deviceId: String, name: String?, kind: DeviceKind = DeviceKind.Tv) =
        RemoteDevice(id, deviceId, name, kind, listOf(CastKind.Item))

    private fun session(
        id: String = "s1",
        kind: PlayingKind = PlayingKind.Item,
        device: String? = "Living room TV",
        itemId: Long = 204,
        state: ProgressState = ProgressState.Playing,
        positionMs: Long = 60_000,
        durationMs: Long? = 3_209_000,
        updatedAt: Long = 100,
        deviceId: String = "",
    ) = PlayingSession(
        sessionId = id,
        kind = kind,
        device = device,
        deviceId = deviceId,
        item = ItemSummary(itemId, 2, ItemKind.Episode, "Episode Two").takeIf { kind == PlayingKind.Item },
        channel = PlayingChannel(1, "Pley News HD").takeIf { kind == PlayingKind.Live },
        state = state,
        positionMs = positionMs,
        durationMs = durationMs,
        updatedAt = updatedAt,
    )
}
