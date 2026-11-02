package app.pley.android.player.session

import app.pley.android.fixtures.PgsMks
import app.pley.android.model.MediaFile
import app.pley.android.model.MediaStream
import app.pley.android.model.PlaybackInfo
import app.pley.android.model.PlaybackMethod
import app.pley.android.model.PlaybackMode
import app.pley.android.model.StreamKind
import app.pley.android.network.ApiException
import app.pley.android.player.PlayRequest
import app.pley.android.player.subtitles.PictureLoad
import app.pley.android.player.subtitles.PictureSource
import app.pley.android.player.subtitles.PictureTrack
import app.pley.android.player.subtitles.SampleStore
import app.pley.android.testing.caps.TestDevices
import app.pley.android.testing.caps.capsOf
import java.io.File
import kotlinx.coroutines.Dispatchers
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

/** Pictures beside a stream (capabilities v2 `picture_subtitles_sideload`): nothing burned in, the .mks drawn here. */
@RunWith(RobolectricTestRunner::class)
class SideloadTest {
    private val dir = File(System.getProperty("java.io.tmpdir"), "pley-sideload-${System.nanoTime()}").apply { mkdirs() }

    /** The .mks asked for, by address; each answer is the fixtures' PGS, a cue every 6 s from 0.5 s. */
    private val asked = mutableListOf<String>()
    private val pictures = PictureSource { fileId, index, url ->
        asked += url
        val mks = File(dir, "$fileId-$index-${asked.size}.mks")
        mks.writeBytes(PgsMks.build((0 until 4).map { n -> Triple(n * 6_000L + 500, n * 6_000L + 4_500, "PGS ${n + 1}") }, 30_000, "fre"))
        PictureLoad(PictureTrack.read(mks, SampleStore(File(dir, "$fileId-$index-${asked.size}.samples")), Dispatchers.Unconfined), complete = true)
    }
    private val h = SessionHarness(caps = capsOf(TestDevices.sdrPhone), pictures = pictures)

    @After
    fun close() {
        h.close()
        dir.deleteRecursively()
    }

    @Test
    fun `the report says pictures are side-loaded`() {
        assertEquals(true, capsOf(TestDevices.sdrPhone).report.pictureSubtitlesSideload)
    }

    @Test
    fun `pictures chosen in a stream need no new session, and show from the server's mks at the stream's positions`() {
        val movie = Catalogue.movieWithTracks(h.api)
        val file = movie.files.first()
        val picture = file.streams.first { it.kind == StreamKind.Subtitle && it.textBased == false }
        val c = h.play(PlayRequest(movie.id, fromStart = true))
        assertTrue(h.engine.source!!.hls)
        assertFalse(c.state.value.subtitles.first { it.index == picture.index.toLong() }.burnIn)
        h.engine.ready()
        c.selectSubtitle(picture.index.toLong())
        h.settle()
        // Nothing to burn in: the session plays on.
        assertEquals(1, h.playbackRequests().size)
        assertNull(h.playbackRequests().single().subtitleIndex)
        assertTrue(asked.single().endsWith("/api/files/${file.id}/subtitles/${picture.index}.mks"))
        assertTrue(c.state.value.subtitleSync)
        h.advance(1_000)
        assertTrue(c.cues.value.single().bitmap != null)
        h.advance(4_000)
        assertEquals(0, c.cues.value.size)
        // Moved 2 s later: the second picture (6.5 s) shows from 8.5 s on screen.
        c.setSubtitleOffset(2_000)
        h.advance(3_000)
        assertEquals(0, c.cues.value.size)
        h.advance(500)
        assertEquals(1, c.cues.value.size)
        // Off: nothing shows, and still one session.
        c.selectSubtitle(null)
        h.settle()
        assertEquals(0, c.cues.value.size)
        assertEquals(1, h.playbackRequests().size)
    }

    @Test
    fun `a subtitle offset is each profile's own`() {
        val movie = Catalogue.movieWithTracks(h.api)
        h.profileId = 1
        val first = h.play(PlayRequest(movie.id, fromStart = true))
        first.setSubtitleOffset(1_500)
        h.settle()
        first.close()
        // Another profile on this device starts in time.
        h.profileId = 2
        val other = h.play(PlayRequest(movie.id, fromStart = true))
        h.settle()
        assertEquals(0L, other.state.value.subtitleOffsetMs)
        other.close()
        // The first finds its own again.
        h.profileId = 1
        val again = h.play(PlayRequest(movie.id, fromStart = true))
        h.settle()
        assertEquals(1_500L, again.state.value.subtitleOffsetMs)
    }

    @Test
    fun `a stream that starts with pictures asks for none to be burned in`() {
        val movie = Catalogue.movieWithTracks(h.api)
        val picture = movie.files.first().streams.first { it.kind == StreamKind.Subtitle && it.textBased == false }
        h.play(PlayRequest(movie.id, fromStart = true, subtitleIndex = picture.index.toLong()))
        assertEquals(1, h.playbackRequests().size)
        assertNull(h.playbackRequests().single().subtitleIndex)
        assertEquals(true, h.playbackRequests().single().caps.pictureSubtitlesSideload)
        assertEquals(1, asked.size)
    }

    @Test
    fun `pictures are asked for from where the player is, so the server copies them out of the file from there first`() {
        val movie = Catalogue.movieWithTracks(h.api)
        val file = movie.files.first()
        val picture = file.streams.first { it.kind == StreamKind.Subtitle && it.textBased == false }
        h.play(PlayRequest(movie.id, startMs = 600_000, subtitleIndex = picture.index.toLong()))
        assertTrue(asked.single(), asked.single().endsWith("/api/files/${file.id}/subtitles/${picture.index}.mks?at=600000"))
    }

    @Test
    fun `a server without the mks burns the pictures in instead`() {
        // An older server: no .mks route.
        val older = SessionHarness(
            caps = capsOf(TestDevices.sdrPhone),
            pictures = PictureSource { _, _, _ -> throw ApiException(404, "not_found", "Not found") },
        )
        try {
            val movie = Catalogue.movieWithTracks(older.api)
            val picture = movie.files.first().streams.first { it.kind == StreamKind.Subtitle && it.textBased == false }
            val c = older.play(PlayRequest(movie.id, fromStart = true))
            older.engine.ready()
            c.selectSubtitle(picture.index.toLong())
            older.settle()
            val requests = older.playbackRequests()
            assertEquals(2, requests.size)
            assertNull(requests.first().subtitleIndex)
            assertEquals(picture.index, requests.last().subtitleIndex)
            assertTrue(c.state.value.subtitles.first { it.index == picture.index.toLong() }.burnIn)
        } finally {
            older.close()
        }
    }
}

/** The plan's side of it, without a server. */
class SideloadPlanTest {
    private val video = MediaStream(index = 0, kind = StreamKind.Video, codec = "h264", width = 1920, height = 1080, default = true)
    private val aac = MediaStream(index = 1, kind = StreamKind.Audio, codec = "aac", default = true)
    private val pgs = MediaStream(index = 2, kind = StreamKind.Subtitle, codec = "hdmv_pgs_subtitle", textBased = false)
    private val file = MediaFile(id = 7, path = "a.mkv", container = "mkv", streams = listOf(video, aac, pgs))
    private val caps = capsOf(TestDevices.sdrPhone)
    private val noSideload = caps.copy(report = caps.report.copy(pictureSubtitlesSideload = null))

    private fun choices(sideload: Boolean) = PlaybackPlan.Choices(file, 0, aac, pgs, Qualities.ORIGINAL, PlaybackMode.Transcode, streaming = true, sideload = sideload)

    @Test
    fun `pictures side-loaded send no subtitle_index, burned ones do`() {
        assertNull(PlaybackPlan.request(1, choices(sideload = true), "c", caps).subtitleIndex)
        assertEquals(2, PlaybackPlan.request(1, choices(sideload = false), "c", caps).subtitleIndex)
    }

    @Test
    fun `side-loading needs the report's word and a reader`() {
        assertTrue(PlaybackPlan.sideloadsPictures(caps))
        assertFalse(PlaybackPlan.sideloadsPictures(caps, canRead = false))
        assertFalse(PlaybackPlan.sideloadsPictures(noSideload))
        assertFalse(PlaybackPlan.sideloadsPictures(null))
        assertEquals("/api/files/7/subtitles/2.mks", PlaybackPlan.pictureSubtitlesPath(7, 2))
        assertEquals("/s.mks", PlaybackPlan.pictureUrlAt("/s.mks", 0))
        assertEquals("/s.mks?at=5000", PlaybackPlan.pictureUrlAt("/s.mks", 5_000))
        assertEquals("/s.mks?k=1&at=5000", PlaybackPlan.pictureUrlAt("/s.mks?k=1", 5_000))
    }

    @Test
    fun `a stream without a burn-in isn't asked again when pictures are side-loaded`() {
        val info = PlaybackInfo(sessionId = "s", method = PlaybackMethod.Transcode, url = "/x.m3u8", hls = true, fileId = 7)
        val sideloaded = choices(sideload = true)
        assertFalse(PlaybackPlan.needsIndexes(info, PlaybackPlan.request(1, sideloaded, "c", caps), sideloaded))
        val burned = choices(sideload = false)
        val sentWithout = PlaybackPlan.request(1, burned, "c", caps).copy(subtitleIndex = null)
        assertTrue(PlaybackPlan.needsIndexes(info, sentWithout, burned))
    }
}
