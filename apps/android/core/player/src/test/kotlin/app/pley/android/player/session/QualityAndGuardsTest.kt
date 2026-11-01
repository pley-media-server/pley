package app.pley.android.player.session

import app.pley.android.fixtures.FixtureIds
import app.pley.android.model.DynamicRange
import app.pley.android.model.PlaybackMode
import app.pley.android.player.PlayRequest
import app.pley.android.testing.caps.TestDevices
import app.pley.android.testing.caps.capsOf
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

/** Qualities, the Dolby Vision guards and the details panel, against the FixtureServer. */
@RunWith(RobolectricTestRunner::class)
class QualityAndGuardsTest {
    private var h = SessionHarness()

    @After
    fun close() = h.close()

    private fun use(harness: SessionHarness) {
        h.close()
        h = harness
    }

    @Test
    fun `a quality transcodes at its cap and is remembered`() {
        val c = h.play(PlayRequest(FixtureIds.SAMPLE_1080P))
        h.engine.ready()
        h.advance(2_000)
        assertEquals(listOf("original", "4m", "2m"), c.state.value.qualities.map { it.id }.filter { it == "original" || it == "4m" || it == "2m" })
        c.selectQuality("4m")
        h.settle()
        val asked = h.playbackRequests().last()
        assertEquals(PlaybackMode.Transcode, asked.mode)
        assertEquals(4_000, asked.maxBitrate)
        assertEquals(720, asked.maxHeight)
        assertEquals(2_000L, asked.startMs)
        assertEquals(listOf("4m"), h.remembered)
        assertEquals("4 Mbps 720p", c.state.value.quality)
        assertTrue(c.state.value.qualities.first { it.id == "4m" }.selected)
        assertNull(c.state.value.qualities.first { it.id == "4m" }.hint)

        c.selectQuality("original")
        h.settle()
        assertEquals(PlaybackMode.Auto, h.playbackRequests().last().mode)
        assertNull(h.playbackRequests().last().maxBitrate)
    }

    @Test
    fun `the remembered quality plays when the file is offered it, else the original`() {
        use(SessionHarness(quality = "2m"))
        h.play(PlayRequest(FixtureIds.SAMPLE_1080P)).close()
        h.settle()
        assertEquals(2_000, h.playbackRequests().single().maxBitrate)
        use(SessionHarness(quality = "30m"))
        h.play(PlayRequest(FixtureIds.SAMPLE_1080P))
        assertNull(h.playbackRequests().single().maxBitrate)
        assertEquals(PlaybackMode.Auto, h.playbackRequests().single().mode)
    }

    @Test
    fun `the original converted by the server stays the original in the menu`() {
        use(SessionHarness(caps = capsOf(TestDevices.sdrPhone)))
        val c = h.play(PlayRequest(FixtureIds.SAMPLE_HDR10))
        val state = c.state.value
        assertEquals("Transcoding", state.method)
        assertEquals("Original", state.quality)
        assertTrue(state.qualities.first().hint!!.startsWith("4K"))
    }

    @Test
    fun `a remux of Dolby Vision profile 5 is asked again as a transcode`() {
        // Profile 5 decoded, AC-3 not: the server would remux, losing the Dolby Vision signalling.
        use(SessionHarness(caps = TestCaps.hdrTv(dv = listOf(5), passthrough = emptyList())))
        val c = h.play(PlayRequest(FixtureIds.DOLBY_VISION_P5))
        h.settle()
        assertEquals(listOf(PlaybackMode.Auto, PlaybackMode.Transcode), h.playbackRequests().map { it.mode })
        assertEquals(1, h.engine.prepared.size)
        assertEquals("Transcoding", c.state.value.method)
        // The remux was deleted without moving the resume point.
        assertEquals(1, h.deletes().size)
        assertTrue(h.progress(FixtureIds.DOLBY_VISION_P5).isEmpty())
        // Later sessions of the item keep the guard's mode.
        h.engine.ready()
        h.engine.fail(EngineError.Http(404, "expired"))
        h.settle()
        assertEquals(PlaybackMode.Transcode, h.playbackRequests().last().mode)
    }

    @Test
    fun `direct play of Dolby Vision profile 7 without a decoder for it is asked again as a remux`() {
        use(SessionHarness(caps = TestCaps.hdrTv()))
        h.play(PlayRequest(FixtureIds.DOLBY_VISION_P7))
        assertEquals(listOf(PlaybackMode.Auto, PlaybackMode.Remux), h.playbackRequests().map { it.mode })
        assertTrue(h.engine.source!!.hls)
    }

    @Test
    fun `Dolby Vision profile 7 plays as it is with a decoder for it`() {
        use(SessionHarness(caps = TestCaps.hdrTv(dv = listOf(7, 8))))
        h.engine.dolbyVision = true
        h.play(PlayRequest(FixtureIds.DOLBY_VISION_P7))
        assertEquals(listOf(PlaybackMode.Auto), h.playbackRequests().map { it.mode })
        assertFalse(h.engine.source!!.hls)
        assertEquals("the engine's decoder follows what the server was told", true, h.engine.source!!.dolbyVision)
    }

    @Test
    fun `Dolby Vision the display mode it plays in doesn't show is asked for without it`() {
        use(SessionHarness(caps = TestCaps.hdrTv(dv = listOf(7, 8))))
        h.engine.dolbyVision = false
        h.play(PlayRequest(FixtureIds.DOLBY_VISION_P7))
        val sent = h.playbackRequests().first().caps
        assertFalse(DynamicRange.DolbyVision in sent.hdrTypes.orEmpty())
        assertEquals("its decoders still say what they decode", listOf(7, 8), sent.dolbyVisionProfiles.orEmpty().map { it.profile })
        // No screen that shows profile 7 then: its HDR10 base layer, remuxed.
        assertEquals(listOf(PlaybackMode.Auto, PlaybackMode.Remux), h.playbackRequests().map { it.mode })
        assertEquals(false, h.engine.source!!.dolbyVision)
    }

    @Test
    fun `the details say how it plays, with the transcode's numbers every 2 s while open`() {
        use(SessionHarness(caps = capsOf(TestDevices.sdrPhone)))
        val c = h.play(PlayRequest(FixtureIds.SAMPLE_HDR10))
        h.engine.ready()
        assertNull(c.state.value.details)
        c.setDetailsOpen(true)
        h.settle()
        val details = c.state.value.details!!
        assertEquals("Transcoding", details.method)
        assertTrue(details.why, details.why!!.contains("converted to SDR"))
        assertEquals("CPU", details.pipeline)
        val rows = details.rows.associate { it.label to it.value }
        assertEquals("HEVC 4K → H.264 1080p", rows["Video"])
        assertTrue(rows["Speed"]!!, rows["Speed"]!!.startsWith("4.0× real time"))
        assertEquals("x264 veryfast (CPU)", rows["Encoder"])
        assertEquals(listOf("Video decoder: fake"), details.engine)
        assertEquals(1, h.api.count("playbackStatus"))
        h.advance(4_000)
        assertEquals(3, h.api.count("playbackStatus"))
        c.setDetailsOpen(false)
        assertNull(c.state.value.details)
        h.advance(4_000)
        assertEquals(3, h.api.count("playbackStatus"))
    }

    @Test
    fun `direct play's details don't ask the server`() {
        val c = h.play(PlayRequest(FixtureIds.SAMPLE_1080P))
        c.setDetailsOpen(true)
        h.advance(4_000)
        assertEquals("Direct play", c.state.value.details?.method)
        // The file as it is needs no reason.
        assertNull(c.state.value.details!!.why)
        assertEquals(0, h.api.count("playbackStatus"))
    }
}
