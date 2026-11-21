package app.pley.android.player.session

import app.pley.android.data.LocalFile
import app.pley.android.data.LocalMedia
import app.pley.android.fixtures.FixtureIds
import app.pley.android.model.MediaStream
import app.pley.android.model.PlaybackMode
import app.pley.android.model.StreamKind
import app.pley.android.player.PlayRequest
import app.pley.android.player.PlayerPhase
import app.pley.android.testing.caps.TestDevices
import app.pley.android.testing.caps.capsOf
import kotlinx.coroutines.runBlocking
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

/** The recovery ladder, played against the FixtureServer with a fake engine. */
@RunWith(RobolectricTestRunner::class)
class RecoveryFlowTest {
    private var h = SessionHarness()

    @After
    fun close() = h.close()

    private fun starts() = h.timeline().count { it == "start" }

    private companion object {
        /** Generated movies the FixtureServer streams its 20 s clips for: 2 h 40 min of 1080p H.264, and 4K HDR10. */
        const val DISTANT_HARBOR = 7L
        const val FROZEN_HARBOR = 9L

        /** Past the clips' end, long before the movies'. */
        const val RESUME_MS = 3_600_000L
    }

    @Test
    fun `direct play whose audio a decoder fails on is asked again without that codec, and the server converts it`() {
        h.close()
        h = SessionHarness(caps = TestCaps.hdrTv())
        val c = h.play(PlayRequest(FixtureIds.SAMPLE_HDR10))
        assertFalse(h.engine.source!!.hls)
        assertTrue("ac3" in h.playbackRequests()[0].caps.audioCodecs)
        h.engine.ready()
        h.advance(2_000)
        // The engine doesn't say which codec: the file's track that plays (AC-3) is the one.
        h.engine.fail(EngineError.Unsupported(audio = true, container = false, detail = "ERROR_CODE_DECODER_INIT_FAILED"))
        h.settle()
        val again = h.playbackRequests()[1]
        // The same mode, not a remux that would copy the same AC-3 again.
        assertEquals(PlaybackMode.Auto, again.mode)
        assertEquals(2_000L, again.startMs)
        assertFalse("ac3" in again.caps.audioCodecs)
        assertFalse("ac3" in again.caps.audioDecode.orEmpty() + again.caps.audioPassthrough.orEmpty())
        assertTrue(h.engine.prepared.last().source.hls)
        assertEquals("This device couldn't play the original audio.", c.state.value.note)
        assertEquals("Direct stream · Audio transcoding", c.state.value.method)

        // Its video fails next: the ladder goes on, still without AC-3.
        h.engine.ready()
        h.engine.fail(EngineError.Unsupported(audio = false, container = false, detail = "ERROR_CODE_DECODING_FAILED"))
        h.settle()
        val transcode = h.playbackRequests()[2]
        assertEquals(PlaybackMode.Transcode, transcode.mode)
        assertFalse("ac3" in transcode.caps.audioCodecs)
        assertEquals(3, starts())
    }

    @Test
    fun `a track picked whose codec failed here goes to the server, not to the engine`() {
        h.close()
        h = SessionHarness(caps = TestCaps.hdrTv())
        val movie = Catalogue.movieWithTracks(h.api)
        val audio = movie.files.first().streams.filter { it.kind == StreamKind.Audio }
        val ac3 = audio.first { it.codec == "ac3" }
        val eac3 = audio.first { it.codec == "eac3" }
        val c = h.play(PlayRequest(movie.id))
        h.engine.ready()
        h.advance(2_000)
        assertFalse(h.engine.source!!.hls)
        val before = h.engine.audioChoices.size
        c.selectAudio(eac3.index.toLong())
        h.settle()
        assertEquals("switched in place", before + 1, h.engine.audioChoices.size)
        h.engine.fail(EngineError.Unsupported(audio = true, container = false, detail = "ERROR_CODE_DECODER_INIT_FAILED", audioCodec = "eac3"))
        h.settle()
        assertEquals(eac3.index, h.playbackRequests().last().audioIndex)
        assertFalse("eac3" in h.playbackRequests().last().caps.audioCodecs)

        // Back to the AC-3, which plays as it is; then the E-AC-3 again: the server converts it.
        c.selectAudio(ac3.index.toLong())
        h.engine.ready()
        h.settle()
        assertFalse(h.engine.source!!.hls)
        val asked = h.playbackRequests().size
        val switched = h.engine.audioChoices.size
        c.selectAudio(eac3.index.toLong())
        h.settle()
        assertEquals(asked + 1, h.playbackRequests().size)
        assertEquals(eac3.index, h.playbackRequests().last().audioIndex)
        assertEquals("not switched in place", switched, h.engine.audioChoices.size)
    }

    /** AAC is the server's last resort when it converts: there is nothing to leave out, so the ladder goes on. */
    @Test
    fun `direct play whose AAC fails becomes a remux, then a transcode, then an error`() {
        val c = h.play(PlayRequest(FixtureIds.SAMPLE_1080P))
        h.engine.ready()
        h.advance(2_000)
        h.engine.fail(EngineError.Unsupported(audio = true, container = false, detail = "ERROR_CODE_DECODER_INIT_FAILED"))
        h.settle()
        val remux = h.playbackRequests()[1]
        assertEquals(PlaybackMode.Remux, remux.mode)
        assertEquals(2_000L, remux.startMs)
        assertEquals(h.playbackRequests()[0].caps, remux.caps)
        assertTrue(h.engine.prepared.last().source.hls)
        assertEquals("This device couldn't play the original file.", c.state.value.note)
        assertEquals("Direct stream", c.state.value.method)

        h.engine.ready()
        h.engine.fail(EngineError.Unsupported(audio = false, container = false, detail = "ERROR_CODE_DECODING_FAILED"))
        h.settle()
        assertEquals(PlaybackMode.Transcode, h.playbackRequests()[2].mode)
        assertEquals("Transcoding", c.state.value.method)

        h.engine.ready()
        h.engine.fail(EngineError.Unsupported(audio = false, container = false, detail = "ERROR_CODE_DECODING_FAILED"))
        h.settle()
        assertEquals(PlayerPhase.Error("This video can't be played on this device.", canRetry = false), c.state.value.phase)
        assertEquals(3, starts())
        // Each new session came after the one before ended.
        assertEquals(
            listOf("start", "progress stopped", "delete", "start", "progress stopped", "delete", "start"),
            h.timeline(),
        )
    }

    @Test
    fun `a download this device can't decode, with the server away, says the server converts it`() {
        val movie = runBlocking { h.api.item(FixtureIds.SAMPLE_1080P) }
        h.local = object : LocalMedia {
            override fun file(fileId: Long) = LocalFile("file:///downloads/movie.mkv").takeIf { fileId == movie.files.first().id }
        }
        val c = h.play(PlayRequest(FixtureIds.SAMPLE_1080P))
        assertEquals("file:///downloads/movie.mkv", h.engine.source?.url)
        h.engine.ready()
        h.api.unreachable += "startPlayback"
        h.engine.fail(EngineError.Unsupported(audio = false, container = false, detail = "ERROR_CODE_DECODING_FORMAT_UNSUPPORTED"))
        h.settle()
        val phase = c.state.value.phase as PlayerPhase.Error
        assertEquals("This download can't play on this device offline. Connect to your server to play it.", phase.message)
        assertTrue(phase.canRetry)
    }

    @Test
    fun `a download whose audio fails streams from the server without that codec`() {
        h.close()
        h = SessionHarness(caps = TestCaps.hdrTv())
        val movie = runBlocking { h.api.item(FixtureIds.SAMPLE_HDR10) }
        h.local = object : LocalMedia {
            override fun file(fileId: Long) = LocalFile("file:///downloads/movie.mkv").takeIf { fileId == movie.files.first().id }
        }
        h.play(PlayRequest(FixtureIds.SAMPLE_HDR10))
        assertEquals("file:///downloads/movie.mkv", h.engine.source?.url)
        h.engine.ready()
        h.engine.fail(EngineError.Unsupported(audio = true, container = false, detail = "ERROR_CODE_DECODING_FAILED", audioCodec = "ac3"))
        h.settle()
        val streamed = h.playbackRequests().single()
        assertEquals(PlaybackMode.Auto, streamed.mode)
        assertFalse("ac3" in streamed.caps.audioCodecs)
        assertFalse(h.engine.source!!.url.startsWith("file:"))
    }

    @Test
    fun `a download that drops frames plays on as it is with the server away`() {
        val movie = runBlocking { h.api.item(FixtureIds.SAMPLE_1080P) }
        h.local = object : LocalMedia {
            override fun file(fileId: Long) = LocalFile("file:///downloads/movie.mkv").takeIf { fileId == movie.files.first().id }
        }
        val c = h.play(PlayRequest(FixtureIds.SAMPLE_1080P))
        assertEquals("file:///downloads/movie.mkv", h.engine.source?.url)
        h.engine.durationMs = 600_000
        h.engine.ready()
        h.api.unreachable += "startPlayback"
        h.engine.frames = FrameCounts(rendered = 0, dropped = 0, maxConsecutiveDropped = 0)
        h.advance(2_000)
        // 240 frames in 10 s, 12 of them dropped: a smoother copy is asked of the server, out of reach.
        h.engine.frames = FrameCounts(rendered = 228, dropped = 12, maxConsecutiveDropped = 2)
        h.advance(10_000)
        h.settle()

        assertFalse(c.state.value.phase is PlayerPhase.Error)
        assertEquals("the download again", "file:///downloads/movie.mkv", h.engine.source?.url)
        assertFalse(c.state.value.canPlayOriginal)
    }

    @Test
    fun `a download of another file with the same number streams instead`() {
        // A server whose database was made anew numbers its files again: the
        // download kept under this number was of another title.
        val movie = runBlocking { h.api.item(FixtureIds.SAMPLE_1080P) }
        val other = movie.copy(title = "Another", files = movie.files.map { it.copy(path = "Another (1999).mkv") })
        h.local = object : LocalMedia {
            override fun file(fileId: Long) = LocalFile("file:///downloads/another.mkv").takeIf { fileId == movie.files.first().id }

            override suspend fun detail(itemId: Long) = other.takeIf { itemId == movie.id }
        }
        h.play(PlayRequest(FixtureIds.SAMPLE_1080P))
        assertEquals("streamed", 1, h.playbackRequests().size)
        assertFalse(h.engine.source!!.url.startsWith("file:"))
    }

    @Test
    fun `a converted download's subtitles are found among the tracks a conversion keeps`() {
        // Teletext has no place in the server's Matroska: the French track is the file's first.
        val movie = runBlocking { h.api.item(FixtureIds.SAMPLE_1080P) }
        val file = movie.files.first()
        val teletext = MediaStream(index = 20, kind = StreamKind.Subtitle, codec = "dvb_teletext", language = "eng", textBased = true)
        val french = MediaStream(index = 21, kind = StreamKind.Subtitle, codec = "subrip", language = "fre", textBased = true)
        val subtitled = movie.copy(files = listOf(file.copy(streams = file.streams.filter { it.kind != StreamKind.Subtitle } + teletext + french)))
        // Away from the server: the details saved with the download.
        h.api.unreachable += "item"
        h.local = object : LocalMedia {
            override fun file(fileId: Long) = LocalFile("file:///downloads/movie.mkv", converted = true).takeIf { fileId == file.id }

            override suspend fun detail(itemId: Long) = subtitled.takeIf { itemId == movie.id }
        }
        val c = h.play(PlayRequest(FixtureIds.SAMPLE_1080P))
        assertEquals("file:///downloads/movie.mkv", h.engine.source?.url)

        c.selectSubtitle(french.index.toLong())
        h.settle()
        assertEquals(0 to "fre", h.engine.textChoices.last())
        c.selectSubtitle(teletext.index.toLong())
        h.settle()
        assertEquals("not in the file", null to "eng", h.engine.textChoices.last())
    }

    @Test
    fun `a video the device can't decode is transcoded at once`() {
        h.play(PlayRequest(FixtureIds.SAMPLE_1080P))
        h.engine.ready()
        h.engine.fail(EngineError.Unsupported(audio = false, container = false, detail = "ERROR_CODE_DECODING_FORMAT_UNSUPPORTED"))
        h.settle()
        assertEquals(listOf(PlaybackMode.Auto, PlaybackMode.Transcode), h.playbackRequests().map { it.mode })
    }

    @Test
    fun `a track no decoder takes counts as a decoder error`() {
        h.play(PlayRequest(FixtureIds.SAMPLE_1080P))
        h.engine.tracks(
            EngineTracks(
                video = listOf(EngineTrack("und", "video/avc", null, supported = true)),
                audio = listOf(EngineTrack("en", "audio/ac3", 6, supported = false)),
            ),
        )
        h.settle()
        assertEquals(listOf(PlaybackMode.Auto, PlaybackMode.Remux), h.playbackRequests().map { it.mode })
    }

    @Test
    fun `a 4K HDR10 direct play whose decoder fails on a TV is transcoded, and a transcode that fails says so`() {
        h.close()
        h = SessionHarness(caps = TestCaps.hdrTv())
        val c = h.play(PlayRequest(FixtureIds.SAMPLE_HDR10))
        assertFalse(h.engine.source!!.hls)
        h.engine.fail(EngineError.Unsupported(audio = false, container = false, detail = "ERROR_CODE_DECODER_INIT_FAILED"))
        h.settle()
        assertEquals(listOf(PlaybackMode.Auto, PlaybackMode.Transcode), h.playbackRequests().map { it.mode })
        assertTrue(h.engine.source!!.hls)
        assertEquals(PlayerPhase.Buffering, c.state.value.phase)

        h.engine.fail(EngineError.Unsupported(audio = false, container = false, detail = "ERROR_CODE_DECODING_FAILED"))
        h.settle()
        assertEquals(PlayerPhase.Error("This video can't be played on this device.", canRetry = false), c.state.value.phase)
    }

    @Test
    fun `a direct play resumed past the end of its media is transcoded, then fails, and never counts as the end`() {
        val c = h.play(PlayRequest(DISTANT_HARBOR, startMs = RESUME_MS))
        assertFalse(h.engine.source!!.hls)
        // The file holds 20 s where its details say 2 h 40 min: the engine ends at once.
        h.engine.durationMs = 20_000
        h.engine.ready()
        h.engine.end()
        h.settle()
        assertEquals(listOf(PlaybackMode.Auto, PlaybackMode.Transcode), h.playbackRequests().map { it.mode })
        assertEquals(RESUME_MS, h.playbackRequests().last().startMs)
        assertEquals(RESUME_MS, c.state.value.positionMs)
        assertFalse(c.state.value.phase == PlayerPhase.Ended)

        // The transcode falls short too: one new session, then an error to go back from.
        h.engine.ready()
        h.engine.end()
        h.settle()
        assertEquals(3, starts())
        assertEquals(RESUME_MS, h.playbackRequests().last().startMs)
        h.engine.ready()
        h.engine.end()
        h.settle()
        assertEquals(3, starts())
        assertEquals(PlayerPhase.Error("This video ends before the point it was to play from.", canRetry = true), c.state.value.phase)

        // Nothing was reported at the media's end, and closing keeps the resume point.
        c.close()
        h.settle()
        val reports = h.progress(DISTANT_HARBOR).map { it.second.positionMs }
        assertTrue("reports: $reports", reports.isNotEmpty() && reports.all { it == RESUME_MS })
        val state = runBlocking { h.api.item(DISTANT_HARBOR) }.userState
        assertFalse(state.watched)
        assertEquals(RESUME_MS, state.positionMs)
    }

    @Test
    fun `a 4K stream resumed past its end gets one new session, then an error to try again from`() {
        val c = h.play(PlayRequest(FROZEN_HARBOR, startMs = RESUME_MS))
        // HDR10 on an SDR screen: the server converts it.
        assertTrue(h.engine.source!!.hls)
        h.engine.durationMs = 20_000
        repeat(2) {
            h.engine.ready()
            h.engine.end()
            h.settle()
        }
        assertEquals(listOf(PlaybackMode.Auto, PlaybackMode.Auto), h.playbackRequests().map { it.mode })
        assertEquals(PlayerPhase.Error("This video ends before the point it was to play from.", canRetry = true), c.state.value.phase)

        c.retry()
        h.settle()
        assertEquals(3, starts())
        assertEquals(RESUME_MS, h.playbackRequests().last().startMs)
    }

    @Test
    fun `media shorter than the server says still ends when it played to its end`() {
        val c = h.play(PlayRequest(DISTANT_HARBOR, startMs = 0))
        h.engine.durationMs = 20_000
        h.engine.ready()
        h.advance(20_000)
        h.engine.end()
        h.settle()
        assertEquals(1, starts())
        assertFalse(c.state.value.phase is PlayerPhase.Error)
    }

    @Test
    fun `an expired session starts again at the position, three times, then Retry starts over`() {
        val c = h.play(PlayRequest(FixtureIds.SAMPLE_1080P))
        h.engine.ready()
        h.advance(3_000)
        repeat(3) {
            h.engine.fail(EngineError.Http(404, "ERROR_CODE_IO_BAD_HTTP_STATUS"))
            h.settle()
            h.engine.ready()
        }
        assertEquals(4, starts())
        assertTrue(h.playbackRequests().drop(1).all { it.mode == PlaybackMode.Auto && it.startMs == 3_000L })
        h.engine.fail(EngineError.Http(410, "ERROR_CODE_IO_BAD_HTTP_STATUS"))
        h.settle()
        assertEquals(PlayerPhase.Error("Playback stopped and couldn't get going again.", canRetry = true), c.state.value.phase)
        assertEquals(4, starts())

        c.retry()
        h.settle()
        assertEquals(5, starts())
        assertEquals(3_000L, h.playbackRequests().last().startMs)
        h.engine.ready()
        assertEquals(PlayerPhase.Playing, c.state.value.phase)
        // The budget is whole again.
        h.engine.fail(EngineError.Playlist("ERROR_CODE_PARSING_MANIFEST_MALFORMED"))
        h.settle()
        assertEquals(6, starts())
    }

    @Test
    fun `five seconds of playback give every recovery back`() {
        val c = h.play(PlayRequest(FixtureIds.SAMPLE_1080P))
        h.engine.durationMs = 120_000
        repeat(5) {
            h.engine.ready()
            h.advance(6_000)
            h.engine.fail(EngineError.Network("ERROR_CODE_IO_NETWORK_CONNECTION_FAILED"))
            h.settle()
        }
        assertEquals(6, starts())
        assertFalse(c.state.value.phase is PlayerPhase.Error)
    }

    @Test
    fun `network errors start new sessions three times, then say the server can't be reached`() {
        val c = h.play(PlayRequest(FixtureIds.SAMPLE_1080P))
        repeat(4) {
            h.engine.ready()
            h.engine.fail(EngineError.Network("ERROR_CODE_IO_NETWORK_CONNECTION_TIMEOUT"))
            h.settle()
        }
        assertEquals(4, starts())
        assertEquals(
            PlayerPhase.Error("Check that the server is on and that this device is on its network.", canRetry = true, offline = true),
            c.state.value.phase,
        )
    }

    @Test
    fun `a server out of reach mid-playback is waited for where playback stood, saying so`() {
        val c = h.play(PlayRequest(FixtureIds.SAMPLE_1080P))
        h.engine.ready()
        h.advance(2_000)
        h.engine.outOfReach()
        // A minute without the server: no new session (it wouldn't reach the server either), no error.
        h.advance(60_000)
        assertEquals(1, starts())
        assertEquals(PlayerPhase.Buffering, c.state.value.phase)
        assertTrue(c.state.value.reconnecting)
        // It answers again: playback goes on where it stood.
        h.engine.ready()
        h.advance(1_000)
        assertEquals(PlayerPhase.Playing, c.state.value.phase)
        assertFalse(c.state.value.reconnecting)
        assertEquals(1, starts())
        // A stall after that is one again.
        h.engine.starve()
        h.advance(12_500)
        assertEquals(2, starts())
    }

    @Test
    fun `a server out of reach when playing starts says so, and Try again plays once it answers`() {
        h.api.unreachable += "startPlayback"
        val c = h.play(PlayRequest(FixtureIds.SAMPLE_1080P))
        val phase = c.state.value.phase as PlayerPhase.Error
        assertTrue(phase.offline)
        assertTrue(phase.canRetry)
        assertEquals("Nothing answers there. Check that the server is on and that this device is on its network.", phase.message)

        h.api.unreachable -= "startPlayback"
        c.retry()
        h.settle()
        assertFalse(c.state.value.phase is PlayerPhase.Error)
        assertEquals("asked once more", 2, h.api.count("startPlayback"))
    }

    @Test
    fun `a direct play that stands still with data becomes a transcode after 12 s`() {
        val c = h.play(PlayRequest(FixtureIds.SAMPLE_1080P))
        h.engine.ready()
        h.advance(1_000)
        h.engine.stall()
        h.advance(11_500)
        assertEquals(1, starts())
        h.advance(1_000)
        assertEquals(listOf(PlaybackMode.Auto, PlaybackMode.Transcode), h.playbackRequests().map { it.mode })
        assertEquals(1_000L, h.playbackRequests().last().startMs)
        assertEquals("The original didn't play smoothly on this device.", c.state.value.note)
    }

    @Test
    fun `a stream that stops asks the server how its transcode does, then starts a new session`() {
        h.close()
        h = SessionHarness(caps = capsOf(TestDevices.sdrPhone))
        h.play(PlayRequest(FixtureIds.SAMPLE_HDR10))
        assertTrue(h.engine.source!!.hls)
        h.engine.ready()
        h.advance(2_000)
        h.engine.starve()
        h.advance(12_500)
        // The FixtureServer's transcode runs at 4×: not too slow, so a new session.
        assertEquals(1, h.api.count("playbackStatus"))
        assertEquals(2, starts())
        assertEquals(2_000L, h.playbackRequests().last().startMs)
    }

    @Test
    fun `dropped frames switch to a transcode once, and Play original goes back`() {
        val c = h.play(PlayRequest(FixtureIds.SAMPLE_1080P))
        h.engine.durationMs = 600_000
        h.engine.ready()
        h.engine.frames = FrameCounts(rendered = 0, dropped = 0, maxConsecutiveDropped = 0)
        h.advance(2_000)
        // 240 frames in 10 s, 12 of them dropped: 5 %.
        h.engine.frames = FrameCounts(rendered = 228, dropped = 12, maxConsecutiveDropped = 2)
        h.advance(10_000)
        assertEquals(listOf(PlaybackMode.Auto, PlaybackMode.Transcode), h.playbackRequests().map { it.mode })
        assertTrue(c.state.value.canPlayOriginal)
        assertEquals("The original didn't play smoothly on this device.", c.state.value.note)

        c.playOriginal()
        h.settle()
        assertEquals(PlaybackMode.Auto, h.playbackRequests().last().mode)
        assertFalse(c.state.value.canPlayOriginal)
        assertNull(c.state.value.note)

        // Dropping frames again doesn't switch a second time for this item.
        h.engine.ready()
        h.engine.frames = FrameCounts(rendered = 0, dropped = 0, maxConsecutiveDropped = 0)
        h.advance(2_000)
        h.engine.frames = FrameCounts(rendered = 200, dropped = 40, maxConsecutiveDropped = 20)
        h.advance(12_000)
        assertEquals(3, starts())
    }
}
