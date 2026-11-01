package app.pley.android.player.session

import app.pley.android.fixtures.FixtureIds
import app.pley.android.model.PlaybackMode
import app.pley.android.network.ApiException
import app.pley.android.player.PlayRequest
import app.pley.android.player.PlayerPhase
import okhttp3.HttpUrl.Companion.toHttpUrl
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

/**
 * Changes while it plays (a quality, a track, Automatic's step): the next
 * session is made ready beside the one playing, and the player moves to it
 * on the last picture. Against the FixtureServer, with the fake engine.
 */
@RunWith(RobolectricTestRunner::class)
class PreparedSwitchTest {
    private val h = SessionHarness()

    @After
    fun close() = h.close()

    /** The 1080p sample, played as it is, 10 s in. */
    private fun playing() = h.play(PlayRequest(FixtureIds.SAMPLE_1080P)).also {
        h.engine.ready()
        h.advance(10_000)
    }

    private fun sessionsStarted() = moves().count { it == "start" }

    /** What the server was told, but progress while playing. */
    private fun moves() = h.timeline().filter { it != "progress playing" && it != "progress paused" }

    @Test
    fun `a quality picked starts beside the one playing, which plays on until the next one's stream is ready`() {
        val c = playing()
        val first = h.engine.source!!
        h.warmupMs = 3_000
        c.selectQuality("4m")
        h.settle()
        val asked = h.playbackRequests().last()
        assertEquals(true, asked.keepPrevious)
        assertEquals(PlaybackMode.Transcode, asked.mode)
        assertEquals(10_000L, asked.startMs)
        // Nothing ended yet: the original plays on, and nothing waits on screen.
        assertEquals(listOf("start", "start"), moves())
        assertEquals(1, h.engine.prepared.size)
        assertEquals(PlayerPhase.Playing, c.state.value.phase)
        assertFalse(c.state.value.handoff)
        // The stream is asked for where the player will be by the time it is ready.
        val (url, from) = h.warmups.single()
        assertTrue(url, url.endsWith("/index.m3u8"))
        assertEquals(14_000L, from)

        h.advance(3_000)
        // Ready: the one before ends where it got to, and the player moves over from there.
        assertEquals(listOf("start", "start", "progress stopped", "delete"), moves())
        val stopped = h.progress(FixtureIds.SAMPLE_1080P).last().second
        assertTrue("stopped at ${stopped.positionMs}", stopped.positionMs in 12_500L..13_500L)
        assertEquals(stopped.sessionId, h.deletes().single())
        assertEquals(2, h.engine.prepared.size)
        assertTrue(h.engine.prepared.last().source.hls)
        assertEquals(stopped.positionMs, h.engine.prepared.last().startMs)
        assertTrue(h.engine.source != first)
        // The last picture stays, and the spinner waits longer, until the new stream shows its own.
        assertTrue(c.state.value.handoff)
        h.engine.ready()
        assertFalse(c.state.value.handoff)
        assertEquals(PlayerPhase.Playing, c.state.value.phase)
        assertEquals("4 Mbps 720p", c.state.value.quality)
    }

    @Test
    fun `with no transcode free for two at once it moves as before`() {
        val c = playing()
        h.api.noTranscodeFreeBeside = true
        c.selectQuality("4m")
        h.settle()
        // Asked for beside it and refused, then asked for again once the one playing ended, as before.
        assertEquals(3, h.api.count("startPlayback"))
        assertEquals(503, (h.api.failures.single().second as ApiException).status)
        assertEquals(listOf(null, null), h.playbackRequests().map { it.keepPrevious })
        assertEquals(listOf("start", "progress stopped", "delete", "start"), moves())
        assertEquals(2, h.engine.prepared.size)
        assertFalse(c.state.value.handoff)
        assertTrue(h.warmups.isEmpty())
        h.engine.ready()
        assertEquals(PlayerPhase.Playing, c.state.value.phase)
    }

    @Test
    fun `a stream that takes long to get ready is moved to after 20 s all the same`() {
        val c = playing()
        h.warmupMs = 60_000
        c.selectQuality("2m")
        h.advance(19_000)
        assertEquals(1, h.engine.prepared.size)
        assertEquals(listOf("start", "start"), moves())
        h.advance(1_500)
        assertEquals(2, h.engine.prepared.size)
        assertTrue(h.engine.source!!.hls)
        assertTrue(c.state.value.handoff)
    }

    @Test
    fun `a second change while the first gets ready drops the first`() {
        val c = playing()
        h.warmupMs = 5_000
        c.selectQuality("4m")
        h.advance(1_000)
        c.selectQuality("2m")
        h.advance(6_000)
        val requests = h.playbackRequests()
        assertEquals(listOf(null, true, true), requests.map { it.keepPrevious })
        assertEquals(listOf(null, 4_000, 2_000), requests.map { it.maxBitrate })
        // The first one made ready is deleted without a stop (it never played) as the second is asked
        // for: the delete goes through the reports' queue and the start doesn't, so the server may hear
        // either first. Then the original ends.
        val moves = moves()
        assertEquals(listOf("start", "start"), moves.take(2))
        assertEquals(listOf("delete", "start"), moves.subList(2, 4).sorted())
        assertEquals(listOf("progress stopped", "delete"), moves.drop(4))
        val (dropped, original) = h.deletes()
        assertEquals(h.progress(FixtureIds.SAMPLE_1080P).last().second.sessionId, original)
        assertTrue(dropped != original)
        assertEquals(1, h.progress(FixtureIds.SAMPLE_1080P).count { it.second.state.wire == "stopped" })
        assertEquals(2, h.engine.prepared.size)
        assertEquals("2 Mbps 480p", c.state.value.quality)
    }

    @Test
    fun `a stall while the next one gets ready waits for it, and the stall watch starts over`() {
        val c = playing()
        h.warmupMs = 15_000
        c.selectQuality("4m")
        h.engine.starve()
        h.advance(14_000)
        // No recovery of the one playing: the next one is on its way.
        assertEquals(2, sessionsStarted())
        h.advance(2_000)
        assertEquals(2, sessionsStarted())
        assertEquals(2, h.engine.prepared.size)
        assertTrue(c.state.value.handoff)
    }

    @Test
    fun `a stall without a change starts again at once`() {
        playing()
        h.engine.stall()
        h.advance(13_000)
        // The recovery ladder: the one that stalled ends before the next starts.
        val timeline = moves()
        assertTrue(timeline.toString(), timeline.indexOf("delete") < timeline.lastIndexOf("start"))
        assertNull(h.playbackRequests().last().keepPrevious)
        assertTrue(h.warmups.isEmpty())
    }

    @Test
    fun `closing while the next one gets ready ends both`() {
        val c = playing()
        h.warmupMs = 5_000
        c.selectQuality("4m")
        h.advance(1_000)
        c.close()
        h.settle()
        assertEquals(2, h.deletes().size)
        assertEquals(1, h.progress(FixtureIds.SAMPLE_1080P).count { it.second.state.wire == "stopped" })
    }

    @Test
    fun `a change before the media shows starts at once`() {
        val c = h.play(PlayRequest(FixtureIds.SAMPLE_1080P))
        c.selectQuality("4m")
        h.settle()
        assertEquals(listOf(null, null), h.playbackRequests().map { it.keepPrevious })
        assertEquals(listOf("start", "progress stopped", "delete", "start"), moves())
    }

    @Test
    fun `hls warm-up reads the first size listed and the segment holding the start`() {
        val base = "http://nas:7539/api/playback/s1/index.m3u8".toHttpUrl()
        val master = """
            #EXTM3U
            #EXT-X-STREAM-INF:BANDWIDTH=4000000,RESOLUTION=1280x720
            720/index.m3u8

            #EXT-X-STREAM-INF:BANDWIDTH=8000000,RESOLUTION=1920x1080
            1080/index.m3u8
        """.trimIndent()
        assertEquals(
            listOf("http://nas:7539/api/playback/s1/720/index.m3u8", "http://nas:7539/api/playback/s1/1080/index.m3u8"),
            HlsWarmup.variants(master, base).map { it.toString() },
        )
        val media = """
            #EXTM3U
            #EXT-X-TARGETDURATION:4
            #EXTINF:4.000,
            0.ts
            #EXTINF:4.000,
            1.ts
            #EXTINF:2.5,
            2.ts
            #EXT-X-ENDLIST
        """.trimIndent()
        assertTrue(HlsWarmup.variants(media, base).isEmpty())
        assertEquals("http://nas:7539/api/playback/s1/0.ts", HlsWarmup.segmentAt(media, base, 0).toString())
        assertEquals("http://nas:7539/api/playback/s1/1.ts", HlsWarmup.segmentAt(media, base, 4_000).toString())
        assertEquals("http://nas:7539/api/playback/s1/2.ts", HlsWarmup.segmentAt(media, base, 9_000).toString())
        // Past the end: the last one.
        assertEquals("http://nas:7539/api/playback/s1/2.ts", HlsWarmup.segmentAt(media, base, 60_000).toString())
        assertNull(HlsWarmup.segmentAt("#EXTM3U", base, 0))
    }
}
