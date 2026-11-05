package app.pley.android.player.session

import app.pley.android.data.LocalFile
import app.pley.android.data.LocalMedia
import app.pley.android.fixtures.FixtureIds
import app.pley.android.caps.CapsSnapshot
import app.pley.android.model.Control
import app.pley.android.model.PlayingSession
import app.pley.android.model.ProgressState
import app.pley.android.model.ServerEvent
import app.pley.android.model.StreamKind
import app.pley.android.model.format.formatClock
import app.pley.android.player.PlayRequest
import app.pley.android.player.PlayerPhase
import app.pley.android.testing.caps.TestDevices
import app.pley.android.testing.caps.capsOf
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.runBlocking
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

/**
 * The profile's remote, on another device (docs/API.md, "Remote control"):
 * what a video obeys, its tracks included, and that it reports at once.
 */
@RunWith(RobolectricTestRunner::class)
class RemoteTest {
    private val events = MutableSharedFlow<ServerEvent>(extraBufferCapacity = 8)

    /** What the player told the app of its session, in order (LiveUpdates.playing). */
    private val held = mutableListOf<Boolean>()

    /** The sessions the player said it plays, in order (RemoteRepository.here). */
    private val here = mutableListOf<String?>()

    /** The sessions held for the remote now, by who said so. */
    private val holding = mutableMapOf<Any, String>()
    private var h = harness(capsOf(TestDevices.sdrPhone))

    /** A phone (or [caps]'s device) that hears [events], telling the app what it holds. */
    private fun harness(caps: CapsSnapshot) = SessionHarness(
        caps = caps,
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

    private fun command(
        control: Control,
        positionMs: Long? = null,
        offsetMs: Long? = null,
        session: String = sessionId(),
        streamIndex: Int? = null,
        quality: String? = null,
    ) {
        events.tryEmit(ServerEvent.PlayerCommand(session, control, positionMs, offsetMs, streamIndex, quality))
        h.settle()
    }

    @Test
    fun `a video says what it obeys, next only when something can come next, tracks when its file has them, and its quality`() {
        playing()
        assertEquals(
            listOf(Control.Play, Control.Pause, Control.Seek, Control.Skip, Control.Next, Control.Audio, Control.Quality),
            h.playbackRequests().last().controls,
        )
        val movie = h.play(PlayRequest(FixtureIds.SAMPLE_1080P, startMs = 0))
        assertEquals(listOf(Control.Play, Control.Pause, Control.Seek, Control.Skip, Control.Audio, Control.Quality), h.playbackRequests().last().controls)
        movie.close()
        val tracks = h.play(PlayRequest(Catalogue.movieWithTracks(h.api).id, startMs = 0))
        assertEquals(
            listOf(Control.Play, Control.Pause, Control.Seek, Control.Skip, Control.Audio, Control.Subtitles, Control.Quality),
            h.playbackRequests().last().controls,
        )
        tracks.close()
    }

    @Test
    fun `every report says the tracks playing, -1 for no subtitles, and the quality picked`() {
        playing()
        val first = h.progress(SessionHarness.S1E1).last().second
        assertEquals(1, first.audioIndex)
        assertEquals(PlayingSession.SUBTITLES_OFF, first.subtitleIndex)
        assertEquals("original", first.quality)
    }

    @Test
    fun `the remote's quality plays as the menu's pick would, is kept as the pick, and is said at once and by its session`() {
        val c = h.play(PlayRequest(FixtureIds.SAMPLE_1080P, startMs = 0))
        h.engine.ready()
        h.advance(10_500)
        val first = sessionId(FixtureIds.SAMPLE_1080P)
        val reports = h.progress(FixtureIds.SAMPLE_1080P).size
        command(Control.Quality, quality = "4m", session = first)
        // Said at once by the session playing, then by the one its pick needed as it starts.
        val said = h.progress(FixtureIds.SAMPLE_1080P).drop(reports)
        assertEquals("4m", said.first().second.quality)
        assertEquals(first, said.first().second.sessionId)
        assertEquals(4_000, h.playbackRequests().last().maxBitrate)
        assertEquals(listOf("4m"), h.remembered)
        assertTrue(c.state.value.qualities.first { it.id == "4m" }.selected)
        val next = h.progress(FixtureIds.SAMPLE_1080P).last().second
        assertTrue(next.sessionId != first)
        assertEquals("4m", next.quality)

        // Automatic, then what plays already: said again, nothing asked of the server.
        command(Control.Quality, quality = "auto", session = next.sessionId!!)
        assertEquals("auto", h.progress(FixtureIds.SAMPLE_1080P).last().second.quality)
        val requests = h.playbackRequests().size
        val before = h.progress(FixtureIds.SAMPLE_1080P).size
        command(Control.Quality, quality = "auto", session = sessionId(FixtureIds.SAMPLE_1080P))
        assertEquals(before + 1, h.progress(FixtureIds.SAMPLE_1080P).size)
        assertEquals(requests, h.playbackRequests().size)
        c.close()
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
        assertNull(c.state.value.remoteNotice)
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
    fun `play and pause that change nothing are said again, so a remote a little behind settles`() {
        playing()
        val before = h.progress(SessionHarness.S1E1).size
        command(Control.Play)
        assertTrue(h.engine.playWhenReady)
        assertEquals(before + 1, h.progress(SessionHarness.S1E1).size)
        assertEquals(ProgressState.Playing, h.progress(SessionHarness.S1E1).last().second.state)

        command(Control.Pause)
        val paused = h.progress(SessionHarness.S1E1).size
        command(Control.Pause)
        assertFalse(h.engine.playWhenReady)
        assertEquals(paused + 1, h.progress(SessionHarness.S1E1).size)
        assertEquals(ProgressState.Paused, h.progress(SessionHarness.S1E1).last().second.state)
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
    fun `the remote's pause and play say so for a moment, and its pause is the remote's until it plays`() {
        val c = playing()
        command(Control.Pause)
        assertEquals(PlayerPhase.Paused, c.state.value.phase)
        assertTrue(c.state.value.pausedByRemote)
        assertEquals("Paused", c.state.value.remoteNotice)
        c.dismissRemoteNotice()
        assertNull(c.state.value.remoteNotice)

        // Paused already: nothing changed, nothing said.
        command(Control.Pause)
        assertNull(c.state.value.remoteNotice)

        command(Control.Play)
        assertEquals("Playing", c.state.value.remoteNotice)
        assertFalse(c.state.value.pausedByRemote)
        c.dismissRemoteNotice()
        command(Control.Play)
        assertNull(c.state.value.remoteNotice)

        // The screen's own pause is the person's.
        c.pause()
        h.settle()
        assertEquals(PlayerPhase.Paused, c.state.value.phase)
        assertFalse(c.state.value.pausedByRemote)
        assertNull(c.state.value.remoteNotice)
    }

    @Test
    fun `the remote's seek says where it went, and its skip how far it asked`() {
        val c = playing()
        command(Control.Seek, positionMs = 4_000)
        assertEquals("Jumped to 0:04", c.state.value.remoteNotice)
        command(Control.Seek, positionMs = 10 * 60 * 60_000L)
        val end = c.state.value.durationMs - SessionPlaybackController.SEEK_END_MARGIN_MS
        assertEquals("Jumped to ${formatClock(end / 1000.0)}", c.state.value.remoteNotice)

        command(Control.Seek, positionMs = 6_000)
        command(Control.Skip, offsetMs = 30_000)
        assertEquals("+30 s", c.state.value.remoteNotice)
        // Stopped at the start, it still says what was asked for, as the TV's seek badge does.
        command(Control.Skip, offsetMs = -60_000)
        assertEquals(0L, h.engine.seeks.last())
        assertEquals("−60 s", c.state.value.remoteNotice)
        assertFalse(c.state.value.pausedByRemote)
    }

    @Test
    fun `the remote's tracks and quality are said by the menus' names, and a pick of what plays says nothing`() {
        h.close()
        h = harness(TestCaps.hdrTv())
        val movie = Catalogue.movieWithTracks(h.api)
        val streams = movie.files.first().streams
        val audio = streams.filter { it.kind == StreamKind.Audio }
        val text = streams.first { it.kind == StreamKind.Subtitle && it.textBased == true }
        val c = playing(movie.id)
        command(Control.Audio, streamIndex = audio[1].index, session = sessionId(movie.id))
        assertEquals("Audio: ${c.state.value.audio.first { it.selected }.label}", c.state.value.remoteNotice)
        c.dismissRemoteNotice()
        command(Control.Audio, streamIndex = audio[1].index, session = sessionId(movie.id))
        assertNull(c.state.value.remoteNotice)

        command(Control.Subtitles, streamIndex = text.index, session = sessionId(movie.id))
        assertEquals("Subtitles: ${c.state.value.subtitles.first { it.selected }.label}", c.state.value.remoteNotice)
        command(Control.Subtitles, streamIndex = PlayingSession.SUBTITLES_OFF, session = sessionId(movie.id))
        assertEquals("Subtitles off", c.state.value.remoteNotice)

        command(Control.Quality, quality = "4m", session = sessionId(movie.id))
        assertEquals("Quality: ${c.state.value.qualities.first { it.selected }.label}", c.state.value.remoteNotice)
        c.dismissRemoteNotice()
        command(Control.Quality, quality = "4m", session = sessionId(movie.id))
        assertNull(c.state.value.remoteNotice)
        c.close()
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
    fun `a screen stopped obeys nothing and lets the event stream go, until it shows again`() {
        val c = playing()
        c.pause()
        c.setAway(true)
        h.settle()
        assertEquals(false, held.last())
        val requests = h.playbackRequests().size
        // Its engine stopped, it would say it plays and play nothing; a next episode would sound with no screen.
        command(Control.Play)
        command(Control.Next)
        assertFalse(h.engine.playWhenReady)
        assertEquals(requests, h.playbackRequests().size)
        assertEquals(PlayerPhase.Paused, c.state.value.phase)

        c.setAway(false)
        assertEquals(true, held.last())
        command(Control.Play)
        assertTrue(h.engine.playWhenReady)
    }

    @Test
    fun `a next episode downloaded lets the event stream go, having no session on the server`() {
        val second = runBlocking { h.api.item(FixtureIds.SHOW_S1E2) }
        h.local = object : LocalMedia {
            override fun file(fileId: Long) = LocalFile("file:///downloads/e2/media.mkv").takeIf { fileId == second.files.first().id }
        }
        val c = playing()
        assertEquals(true, held.last())
        command(Control.Next)
        assertEquals(FixtureIds.SHOW_S1E2, c.state.value.itemId)
        assertEquals("file:///downloads/e2/media.mkv", h.engine.source?.url)
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

    @Test
    fun `the remote's audio and subtitles change as the player's menu changes them, each reported at once`() {
        h.close()
        h = harness(TestCaps.hdrTv())
        val movie = Catalogue.movieWithTracks(h.api)
        val streams = movie.files.first().streams
        val audio = streams.filter { it.kind == StreamKind.Audio }
        val text = streams.first { it.kind == StreamKind.Subtitle && it.textBased == true }
        val c = playing(movie.id)
        // Played as it is: the engine switches, nothing is asked of the server again.
        val reports = h.progress(movie.id).size
        command(Control.Audio, streamIndex = audio[1].index, session = sessionId(movie.id))
        assertEquals(1 to audio[1].language, h.engine.audioChoices.last())
        assertTrue(c.state.value.audio.first { it.index == audio[1].index.toLong() }.selected)
        assertEquals(1, h.playbackRequests().size)
        // Saved for the title, as the menu's choice is.
        assertEquals(audio[1].index, h.tracksChosen.last().second.audioIndex)
        val picked = h.progress(movie.id)
        assertEquals(reports + 1, picked.size)
        assertEquals(audio[1].index, picked.last().second.audioIndex)

        command(Control.Subtitles, streamIndex = text.index, session = sessionId(movie.id))
        assertEquals(text.index, h.progress(movie.id).last().second.subtitleIndex)
        assertTrue(c.state.value.subtitles.first { it.index == text.index.toLong() }.selected)
        command(Control.Subtitles, streamIndex = PlayingSession.SUBTITLES_OFF, session = sessionId(movie.id))
        assertEquals(null to null, h.engine.textChoices.last())
        assertEquals(PlayingSession.SUBTITLES_OFF, h.progress(movie.id).last().second.subtitleIndex)
        assertEquals(reports + 3, h.progress(movie.id).size)
    }

    @Test
    fun `a pick of what plays already is said again at once, so a remote a little behind settles`() {
        val c = playing()
        val before = h.progress(SessionHarness.S1E1).size
        command(Control.Audio, streamIndex = 1)
        command(Control.Subtitles, streamIndex = PlayingSession.SUBTITLES_OFF)
        assertEquals(before + 2, h.progress(SessionHarness.S1E1).size)
        assertEquals(1, h.playbackRequests().size)
        assertTrue(h.tracksChosen.isEmpty())
        assertEquals(PlayerPhase.Playing, c.state.value.phase)
    }

    @Test
    fun `a stream's track from the remote needs a new session, which says it as soon as it starts`() {
        val movie = Catalogue.movieWithTracks(h.api)
        val audio = movie.files.first().streams.filter { it.kind == StreamKind.Audio }
        playing(movie.id)
        assertTrue(h.engine.source!!.hls)
        val first = sessionId(movie.id)
        val reports = h.progress(movie.id).size
        command(Control.Audio, streamIndex = audio[1].index, session = first)
        // The session playing doesn't say the new track: it still plays the old one.
        assertTrue(h.progress(movie.id).drop(reports).none { it.second.sessionId == first && it.second.audioIndex == audio[1].index })
        assertEquals(audio[1].index, h.playbackRequests().last().audioIndex)
        val next = h.progress(movie.id).last().second
        assertTrue(next.sessionId != first)
        assertEquals(audio[1].index, next.audioIndex)
    }

    @Test
    fun `a screen stopped takes no track from the remote`() {
        val c = playing()
        c.setAway(true)
        h.settle()
        val before = h.progress(SessionHarness.S1E1).size
        command(Control.Subtitles, streamIndex = PlayingSession.SUBTITLES_OFF)
        assertEquals(before, h.progress(SessionHarness.S1E1).size)
    }
}
