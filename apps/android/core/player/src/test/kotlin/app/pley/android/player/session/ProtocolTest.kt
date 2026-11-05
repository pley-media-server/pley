@file:OptIn(ExperimentalCoroutinesApi::class)

package app.pley.android.player.session

import app.pley.android.fixtures.FixtureIds
import app.pley.android.model.PlaybackMethod
import app.pley.android.model.PlaybackMode
import app.pley.android.model.ProgressReport
import app.pley.android.model.ProgressState
import app.pley.android.model.caps.ClientCaps
import app.pley.android.player.PlayRequest
import app.pley.android.player.PlayerPhase
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.runBlocking
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

/** The server protocol: what POST /api/playback carries, the reports' cadence, and the end of a session. */
@RunWith(RobolectricTestRunner::class)
class ProtocolTest {
    private val h = SessionHarness()

    @After
    fun close() = h.close()

    @Test
    fun `a direct play asks for the file as it is, without track indexes`() {
        val c = h.play(PlayRequest(FixtureIds.SAMPLE_1080P))
        val request = h.playbackRequests().single()
        assertEquals(FixtureIds.SAMPLE_1080P, request.itemId)
        assertEquals(FixtureIds.SAMPLE_1080P_FILE, request.fileId)
        assertEquals(0L, request.startMs)
        assertEquals(PlaybackMode.Auto, request.mode)
        assertEquals(SessionHarness.CLIENT_ID, request.clientId)
        assertNull(request.audioIndex)
        assertNull(request.subtitleIndex)
        assertNull(request.maxBitrate)
        assertNull(request.maxHeight)
        // Version 1 and 2 of the caps travel together.
        assertEquals(ClientCaps.VERSION_2, request.caps.capsVersion)
        assertTrue("h264" in request.caps.videoCodecs)
        assertTrue(request.caps.video.orEmpty().isNotEmpty())

        val prepared = h.engine.prepared.single()
        assertEquals("${h.server.url}/api/files/${FixtureIds.SAMPLE_1080P_FILE}/stream", prepared.source.url)
        assertFalse(prepared.source.hls)
        assertEquals("video/mp4", prepared.source.mimeType)
        assertEquals(0L, prepared.startMs)
        assertTrue(prepared.playWhenReady)
        assertEquals("Direct play", c.state.value.method)
        assertEquals(PlaybackMethod.Direct, c.state.value.playbackMethod)
        assertTrue(c.state.value.title.startsWith("Sample 1080p"))
    }

    @Test
    fun `it resumes where the profile left off, unless watched or asked to start over`() {
        runBlocking { h.api.reportProgress(FixtureIds.SAMPLE_HDR10, ProgressReport(positionMs = 7_000, state = ProgressState.Paused)) }
        h.play(PlayRequest(FixtureIds.SAMPLE_HDR10)).close()
        h.play(PlayRequest(FixtureIds.SAMPLE_HDR10, fromStart = true)).close()
        h.play(PlayRequest(FixtureIds.SAMPLE_HDR10, startMs = 3_000)).close()
        h.settle()
        assertEquals(listOf(7_000L, 0L, 3_000L), h.playbackRequests().map { it.startMs })
    }

    @Test
    fun `progress goes every 10 s playing, at once on pause, every 30 s paused, and 1 s after the last seek`() {
        val c = h.play(PlayRequest(FixtureIds.SAMPLE_1080P))
        h.engine.ready()
        val t0 = h.scheduler.currentTime
        h.advance(20_000)
        val playing = h.progress(FixtureIds.SAMPLE_1080P)
        assertEquals(listOf(10_000L, 20_000L), playing.map { it.first - t0 })
        assertTrue(playing.all { it.second.state == ProgressState.Playing })
        assertEquals(listOf(10_000L, 20_000L), playing.map { it.second.positionMs })
        assertTrue(playing.all { !it.second.sessionId.isNullOrEmpty() })

        c.pause()
        h.settle()
        val paused = h.progress(FixtureIds.SAMPLE_1080P).last()
        assertEquals(ProgressState.Paused, paused.second.state)
        assertEquals(20_000L, paused.first - t0)

        h.advance(30_000)
        val heartbeat = h.progress(FixtureIds.SAMPLE_1080P).last()
        assertEquals(ProgressState.Paused, heartbeat.second.state)
        assertEquals(50_000L, heartbeat.first - t0)

        // Two seeks half a second apart: one report, a second after the last.
        val before = h.progress(FixtureIds.SAMPLE_1080P).size
        c.seekTo(4_000)
        h.advance(500)
        c.seekTo(6_000)
        h.advance(900)
        assertEquals(before, h.progress(FixtureIds.SAMPLE_1080P).size)
        h.advance(100)
        val seek = h.progress(FixtureIds.SAMPLE_1080P)
        assertEquals(before + 1, seek.size)
        assertEquals(6_000L, seek.last().second.positionMs)
        assertEquals(listOf(4_000L, 6_000L), h.engine.seeks)
    }

    @Test
    fun `closing reports stopped at the position, then deletes the session`() {
        val c = h.play(PlayRequest(FixtureIds.SAMPLE_1080P))
        h.engine.ready()
        h.advance(4_000)
        c.close()
        h.settle()
        assertEquals(listOf("start", "progress stopped", "delete"), h.timeline())
        val stopped = h.progress(FixtureIds.SAMPLE_1080P).single().second
        assertEquals(4_000L, stopped.positionMs)
        assertEquals(stopped.sessionId, h.deletes().single())
        assertTrue(h.engine.released)
        // Nothing more after closing.
        h.advance(60_000)
        assertEquals(3, h.timeline().size)
    }

    @Test
    fun `the end reports stopped at the duration and deletes the session`() {
        val c = h.play(PlayRequest(FixtureIds.SAMPLE_1080P))
        h.engine.durationMs = 20_000
        h.engine.ready()
        h.advance(5_000)
        h.engine.end()
        h.settle()
        assertEquals(PlayerPhase.Ended, c.state.value.phase)
        assertEquals(listOf("start", "progress stopped", "delete"), h.timeline())
        val stopped = h.progress(FixtureIds.SAMPLE_1080P).single().second
        assertTrue("at the duration: ${stopped.positionMs}", stopped.positionMs >= 19_000)
    }

    @Test
    fun `one server session at a time, but for the next one made ready beside it`() {
        val c = h.play(PlayRequest(FixtureIds.SAMPLE_1080P))
        h.engine.ready()
        h.advance(3_000)
        c.selectQuality("2m")
        h.settle()
        assertEquals(listOf("start", "start", "progress stopped", "delete"), h.timeline())
        assertEquals(listOf(null, true), h.playbackRequests().map { it.keepPrevious })
        assertEquals(2, h.engine.prepared.size)
        assertEquals(3_000L, h.engine.prepared.last().startMs)
        // A recovery starts the next one once the one before ended: that one is what failed.
        h.engine.ready()
        h.engine.fail(EngineError.Http(404, "playback session expired"))
        h.settle()
        assertEquals(listOf("start", "start", "progress stopped", "delete", "progress stopped", "delete", "start"), h.timeline())
        assertNull(h.playbackRequests().last().keepPrevious)
    }
}
