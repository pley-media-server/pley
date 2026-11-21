package app.pley.android.player.session

import app.pley.android.data.LocalFile
import app.pley.android.data.LocalMedia
import app.pley.android.fixtures.FixtureIds
import app.pley.android.model.StreamKind
import app.pley.android.player.PlayRequest
import app.pley.android.player.PlayerPhase
import app.pley.android.player.SubtitleSearchUi
import app.pley.android.testing.caps.TestDevices
import app.pley.android.testing.caps.capsOf
import app.pley.android.testing.player.session.FakeEngineLink.SoundNeeds
import kotlinx.coroutines.runBlocking
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

/** Speed, audio delay and Find subtitles… in a playback, against the FixtureServer. */
@RunWith(RobolectricTestRunner::class)
class PlayerExtrasTest {
    private val h = SessionHarness()

    @After
    fun close() = h.close()

    private fun playing(id: Long = FixtureIds.SHOW_S1E1) = h.play(PlayRequest(id, startMs = 0)).also {
        h.engine.ready()
        h.advance(2_000)
    }

    @Test
    fun `paused before the stream is ready, it opens paused`() {
        // A car's screen covered as the player opens: the pause holds once the stream comes.
        val c = h.starting(PlayRequest(FixtureIds.SHOW_S1E1, startMs = 0))
        c.pause()
        h.settle()
        assertFalse(h.engine.prepared.last().playWhenReady)
    }

    @Test
    fun `the speed is this title's, kept by a new session and gone with the next title`() {
        val c = playing()
        c.setSpeed(1.5f)
        assertEquals(1.5f, h.engine.speed)
        assertEquals(1.5f, c.state.value.speed)
        // Off the list: the nearest offered.
        c.setSpeed(1.6f)
        assertEquals(1.5f, h.engine.speed)
        c.selectQuality("4m")
        h.settle()
        assertEquals(2, h.engine.prepared.size)
        assertEquals(1.5f, h.engine.speed)
        c.playNext()
        h.settle()
        assertEquals(1f, h.engine.speed)
        assertEquals(1f, c.state.value.speed)
    }

    @Test
    fun `another speed has the sound decoded here and untunneled, and the delay untunneled, at once`() {
        val c = playing()
        c.setSpeed(1.5f)
        assertEquals(SoundNeeds(decoded = true, untunneled = true, reload = true), h.engine.soundNeeds)
        c.setSpeed(1f)
        assertEquals(SoundNeeds(decoded = false, untunneled = false, reload = true), h.engine.soundNeeds)
        c.setAudioDelay(200)
        assertEquals(200L, h.engine.audioDelayMs)
        assertEquals(SoundNeeds(decoded = false, untunneled = true, reload = true), h.engine.soundNeeds)
        assertEquals(1, h.playbackRequests().size)
        // The next title goes out as it would, from its first load.
        c.playNext()
        h.settle()
        assertEquals(SoundNeeds(decoded = false, untunneled = false, reload = false), h.engine.soundNeeds)
    }

    @Test
    fun `sound that nothing here decodes is converted by the server for another speed, and comes back at 1x`() {
        SessionHarness(caps = capsOf(TestDevices.googleTvStreamer)).use { tv ->
            val c = tv.play(PlayRequest(FixtureIds.SHOW_S1E1, startMs = 0))
            tv.engine.ready()
            tv.advance(2_000)
            c.setSpeed(1.5f)
            tv.engine.soundUndecodable = true
            tv.advance(1_000)
            tv.settle()
            val asked = tv.playbackRequests()
            assertEquals(2, asked.size)
            assertTrue(asked[0].caps.audioPassthrough.orEmpty().isNotEmpty())
            assertEquals("the device's decoders only", emptyList<String>(), asked[1].caps.audioPassthrough)
            assertEquals(asked[1].caps.audioDecode, asked[1].caps.audioCodecs)
            assertEquals(true, asked[1].keepPrevious)
            assertEquals(1.5f, tv.engine.speed)
            assertEquals(1.5f, c.state.value.speed)
            assertNull(c.state.value.speedNote)
            assertEquals(2, tv.engine.prepared.size)

            tv.engine.ready()
            tv.advance(1_000)
            c.setSpeed(1f)
            tv.settle()
            val again = tv.playbackRequests()
            assertEquals(3, again.size)
            assertEquals(asked[0].caps.audioPassthrough, again[2].caps.audioPassthrough)
            assertEquals(SoundNeeds(decoded = false, untunneled = false, reload = false), tv.engine.soundNeeds)
        }
    }

    @Test
    fun `a download whose sound nothing here decodes plays at 1x, and says why`() {
        val movie = runBlocking { h.api.item(FixtureIds.SAMPLE_1080P) }
        h.local = object : LocalMedia {
            override fun file(fileId: Long) = LocalFile("file:///downloads/movie.mkv").takeIf { fileId == movie.files.first().id }
        }
        val c = h.play(PlayRequest(FixtureIds.SAMPLE_1080P))
        h.engine.ready()
        h.advance(2_000)
        c.setSpeed(2f)
        h.engine.soundUndecodable = true
        h.advance(1_000)
        h.settle()
        assertEquals("no server session", 0, h.playbackRequests().size)
        assertEquals(1f, h.engine.speed)
        assertEquals(1f, c.state.value.speed)
        assertNotNull(c.state.value.speedNote)
        c.setSpeed(2f)
        assertEquals(1f, h.engine.speed)
        // The delay still moves it.
        c.setAudioDelay(200)
        assertEquals(200L, h.engine.audioDelayMs)
    }

    @Test
    fun `a download's other audio track may be tried at another speed again`() {
        val movie = Catalogue.movieWithTracks(h.api)
        val audio = movie.files.first().streams.filter { it.kind == StreamKind.Audio }
        h.local = object : LocalMedia {
            override fun file(fileId: Long) = LocalFile("file:///downloads/movie.mkv").takeIf { fileId == movie.files.first().id }
        }
        val c = h.play(PlayRequest(movie.id))
        h.engine.ready()
        h.advance(2_000)
        c.setSpeed(2f)
        h.engine.soundUndecodable = true
        h.advance(1_000)
        assertNotNull(c.state.value.speedNote)
        c.selectAudio(audio[1].index.toLong())
        h.settle()
        assertNull(c.state.value.speedNote)
        c.setSpeed(2f)
        assertEquals(2f, h.engine.speed)
    }

    @Test
    fun `a stream the server can't convert for another speed plays on as it is, at 1x, and says why`() {
        // The transcoder away, or every transcode taken: the file plays as it is, so ending it frees none.
        for (status in listOf(500, 503)) {
            SessionHarness(caps = capsOf(TestDevices.googleTvStreamer)).use { tv ->
                val c = tv.play(PlayRequest(FixtureIds.SHOW_S1E1, startMs = 0))
                tv.engine.ready()
                tv.advance(2_000)
                assertFalse("played as it is", tv.engine.source!!.hls)
                tv.api.refusedWith = status
                c.setSpeed(1.5f)
                tv.engine.soundUndecodable = true
                tv.advance(1_000)
                tv.settle()
                assertEquals("$status: asked beside it, once", 2, tv.api.count("startPlayback"))
                assertEquals("$status: the one playing plays on", 1, tv.engine.prepared.size)
                assertTrue("$status", tv.deletes().isEmpty())
                assertEquals(PlayerPhase.Playing, c.state.value.phase)
                assertEquals(1f, tv.engine.speed)
                assertEquals(1f, c.state.value.speed)
                assertNotNull(c.state.value.speedNote)
                assertEquals(SoundNeeds(decoded = false, untunneled = false, reload = true), tv.engine.soundNeeds)
            }
        }
    }

    @Test
    fun `a converted session whose sound still goes out as it is plays at 1x, and says why`() {
        SessionHarness(caps = capsOf(TestDevices.googleTvStreamer)).use { tv ->
            val c = tv.play(PlayRequest(FixtureIds.SHOW_S1E1, startMs = 0))
            tv.engine.ready()
            tv.advance(2_000)
            c.setSpeed(1.5f)
            tv.engine.soundUndecodable = true
            tv.advance(1_000)
            tv.settle()
            assertEquals(2, tv.engine.prepared.size)
            assertNull("the next session plays now", c.state.value.speedNote)
            // Its file played as it is all the same: the sound can't follow.
            tv.engine.ready()
            tv.engine.soundUndecodable = true
            tv.advance(1_000)
            tv.settle()
            assertEquals(2, tv.playbackRequests().size)
            assertEquals(1f, tv.engine.speed)
            assertEquals(1f, c.state.value.speed)
            assertNotNull(c.state.value.speedNote)
        }
    }

    @Test
    fun `the audio delay goes in 50 ms steps within 5 s either way, and resets with the next title`() {
        val c = playing()
        c.setAudioDelay(123)
        assertEquals(100L, h.engine.audioDelayMs)
        c.setAudioDelay(-7_000)
        assertEquals(-5_000L, h.engine.audioDelayMs)
        assertEquals(-5_000L, c.state.value.audioDelayMs)
        c.setAudioDelay(5_000)
        c.selectQuality("4m")
        h.settle()
        assertEquals(5_000L, h.engine.audioDelayMs)
        c.playNext()
        h.settle()
        assertEquals(0L, h.engine.audioDelayMs)
        assertEquals(0L, c.state.value.audioDelayMs)
    }

    @Test
    fun `a subtitle found online is added, listed and shown`() {
        val c = playing()
        assertTrue(c.state.value.canFindSubtitles)
        c.searchSubtitles()
        h.settle()
        val found = c.state.value.subtitleSearch as SubtitleSearchUi.Found
        assertEquals(2, found.results.size)
        assertTrue(found.results.first().hint!!.contains("Made for this file"))
        c.addFoundSubtitle(found.results.first().id)
        h.settle()
        val state = c.state.value
        assertNull(state.subtitleSearch)
        assertEquals("Subtitles added", state.notice)
        val added = state.subtitles.single { it.selected }
        assertEquals(5_000L, added.index)
        // Shown as a subtitle file beside the media: the server's WebVTT.
        h.advance(1_000)
        assertTrue(h.api.count("subtitles") >= 1)
        val post = h.journal.entries().single { it.method == "POST" && it.path.endsWith("/subtitles") }
        assertTrue(post.body, post.body.contains("\"result\":\"opensubtitles:"))
    }

    @Test
    fun `a server without subtitle search says it isn't set up`() {
        h.server.subtitleSearch = false
        val c = playing()
        c.searchSubtitles()
        h.settle()
        assertEquals(SubtitleSearchUi.Unavailable("Subtitle search isn't set up on this server"), c.state.value.subtitleSearch)
        c.closeSubtitleSearch()
        assertNull(c.state.value.subtitleSearch)
    }
}
