package app.pley.android.player.session

import app.pley.android.caps.CapsDecision
import app.pley.android.caps.DefaultPlaybackCapsPolicy
import app.pley.android.model.DynamicRange
import app.pley.android.model.ItemDetail
import app.pley.android.model.ItemKind
import app.pley.android.model.MediaFile
import app.pley.android.model.MediaStream
import app.pley.android.model.PlaybackInfo
import app.pley.android.model.PlaybackMethod
import app.pley.android.model.PlaybackMode
import app.pley.android.model.StreamKind
import app.pley.android.model.UserState
import app.pley.android.model.caps.DolbyVisionCaps
import app.pley.android.player.PlayRequest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** What goes in POST /api/playback, worked out without a server. */
class PlaybackPlanTest {
    private val video = MediaStream(index = 0, kind = StreamKind.Video, codec = "hevc", width = 3840, height = 2160, bitDepth = 10, profile = "Main 10", dynamicRange = DynamicRange.Hdr10, default = true)
    private val ac3 = MediaStream(index = 1, kind = StreamKind.Audio, codec = "ac3", language = "eng", default = true, channels = 6)
    private val aac = MediaStream(index = 2, kind = StreamKind.Audio, codec = "aac", language = "fre", channels = 2)
    private val srt = MediaStream(index = 3, kind = StreamKind.Subtitle, codec = "subrip", language = "eng", textBased = true, default = true)
    private val pgs = MediaStream(index = 4, kind = StreamKind.Subtitle, codec = "hdmv_pgs_subtitle", language = "fre", textBased = false)
    private val file = MediaFile(id = 7, path = "a.mkv", container = "mkv", bitrate = 40_000_000, width = 3840, height = 2160, streams = listOf(video, ac3, aac, srt, pgs))
    private val item = ItemDetail(id = 70, libraryId = 1, kind = ItemKind.Movie, title = "A", files = listOf(file))
    private val tv = TestCaps.hdrTv()

    private fun choices(audio: MediaStream? = ac3, subtitle: MediaStream? = null, mode: PlaybackMode = PlaybackMode.Auto, quality: QualityPreset = Qualities.ORIGINAL, streaming: Boolean = false) =
        PlaybackPlan.Choices(file, 1_000, audio, subtitle, quality, mode, streaming)

    @Test
    fun `the file asked for, else the one that plays best here, else the first`() {
        val sdr = file.copy(id = 8, streams = listOf(video.copy(codec = "h264", dynamicRange = DynamicRange.Sdr, bitDepth = 8, profile = "High", width = 1920, height = 1080), aac))
        val two = item.copy(files = listOf(file, sdr))
        assertEquals(8L, PlaybackPlan.file(two, 8, tv, DefaultPlaybackCapsPolicy)?.id)
        assertEquals(7L, PlaybackPlan.file(two, null, tv, DefaultPlaybackCapsPolicy)?.id)
        assertEquals(7L, PlaybackPlan.file(two, 99, null, DefaultPlaybackCapsPolicy)?.id)
        assertNull(PlaybackPlan.file(item.copy(files = emptyList()), null, tv, DefaultPlaybackCapsPolicy))
    }

    @Test
    fun `the start is as asked, from the start, or the resume point unless watched`() {
        val resumed = item.copy(userState = UserState(positionMs = 60_000))
        assertEquals(60_000L, PlaybackPlan.startMs(resumed, PlayRequest(70)))
        assertEquals(0L, PlaybackPlan.startMs(resumed, PlayRequest(70, fromStart = true)))
        assertEquals(5_000L, PlaybackPlan.startMs(resumed, PlayRequest(70, startMs = 5_000)))
        assertEquals(0L, PlaybackPlan.startMs(resumed.copy(userState = UserState(positionMs = 60_000, watched = true)), PlayRequest(70)))
    }

    @Test
    fun `tracks start as asked, the audio else as the server picks it, subtitles as the profile chose, and off is off`() {
        assertEquals(aac, PlaybackPlan.initialAudio(file, 2))
        // Nothing asked: automatic, whatever the details say, the server picks the track this device plays best.
        assertNull(PlaybackPlan.initialAudio(file.copy(selectedAudio = 2), null))
        assertNull(PlaybackPlan.initialAudio(file, null))
        // Its answer names the track that plays: that one, else the file's default.
        assertEquals(aac, PlaybackPlan.playedAudio(file, 2))
        assertEquals(ac3, PlaybackPlan.playedAudio(file, null))
        assertEquals(ac3, PlaybackPlan.playedAudio(file, 99))
        assertEquals(pgs, PlaybackPlan.initialSubtitle(file, 4))
        assertEquals(srt, PlaybackPlan.initialSubtitle(file.copy(selectedSubtitle = 3), null))
        // The track the file flags as default doesn't show by itself.
        assertNull(PlaybackPlan.initialSubtitle(file, null))
        // None asked for (a remote's cast with them off), whatever the profile's choice; one the file doesn't have
        // leaves the choice to it.
        assertNull(PlaybackPlan.initialSubtitle(file.copy(selectedSubtitle = 3), -1))
        assertEquals(srt, PlaybackPlan.initialSubtitle(file.copy(selectedSubtitle = 3), 99))
    }

    @Test
    fun `direct play names the audio only when it isn't the server's own, a stream always, pictures burn only into streams`() {
        val direct = PlaybackPlan.request(70, choices(audio = ac3, subtitle = pgs), "c", tv)
        assertNull(direct.audioIndex)
        assertNull(direct.subtitleIndex)
        assertEquals(7L, direct.fileId)
        assertEquals(1_000L, direct.startMs)
        assertEquals("c", direct.clientId)
        assertEquals(tv.report, direct.caps)
        // Another track than the server would play: named, so it judges that one.
        assertEquals(2, PlaybackPlan.request(70, choices(audio = aac, subtitle = pgs), "c", tv).audioIndex)

        val stream = PlaybackPlan.request(70, choices(audio = aac, subtitle = pgs, streaming = true), "c", tv)
        assertEquals(2, stream.audioIndex)
        assertEquals(4, stream.subtitleIndex)
        // Text subtitles are never burned in.
        assertNull(PlaybackPlan.request(70, choices(subtitle = srt, streaming = true), "c", tv).subtitleIndex)
    }

    @Test
    fun `audio the device neither decodes nor passes through goes to the server`() {
        val noAc3 = TestCaps.hdrTv(passthrough = emptyList())
        assertFalse(PlaybackPlan.audioPlaysHere(ac3, noAc3))
        assertTrue(PlaybackPlan.audioPlaysHere(ac3, tv))
        assertEquals(1, PlaybackPlan.request(70, choices(audio = ac3), "c", noAc3).audioIndex)
    }

    /**
     * A remux whose first track is TrueHD, with E-AC-3 beside it (as most
     * are) and a commentary: a TV whose output takes E-AC-3 but not TrueHD
     * plays the file as it is with the E-AC-3, named to the server, rather
     * than having the server convert the TrueHD.
     */
    @Test
    fun `automatic audio is named to no server, and a download's gives way to a track in its language that plays here`() {
        val trueHd = MediaStream(index = 1, kind = StreamKind.Audio, codec = "truehd", language = "eng", default = true, channels = 8, title = "TrueHD 7.1 Atmos")
        val eac3 = MediaStream(index = 2, kind = StreamKind.Audio, codec = "eac3", language = "eng", channels = 6, title = "DD+ 5.1 Atmos")
        val commentary = MediaStream(index = 3, kind = StreamKind.Audio, codec = "aac", language = "eng", channels = 2, title = "Director's Commentary")
        val french = MediaStream(index = 4, kind = StreamKind.Audio, codec = "ac3", language = "fre", channels = 6)
        val remux = file.copy(streams = listOf(video, trueHd, eac3, commentary, french))

        // From the server: no track named, even for a stream; it picks the E-AC-3 for this TV.
        assertNull(PlaybackPlan.request(70, choices(audio = null).copy(file = remux), "c", tv).audioIndex)
        assertNull(PlaybackPlan.request(70, choices(audio = null, streaming = true).copy(file = remux), "c", tv).audioIndex)

        // A download, with no server to ask: chosen here.
        val started = PlaybackPlan.localAudio(remux, tv)
        assertEquals(eac3, started)
        // The E-AC-3 picked: named, as the server would judge the TrueHD.
        assertEquals(2, PlaybackPlan.request(70, choices(audio = started).copy(file = remux), "c", tv).audioIndex)

        // A receiver that takes TrueHD keeps it, unnamed as before.
        val trueHdTv = TestCaps.hdrTv(passthrough = listOf("ac3", "eac3", "truehd"))
        assertEquals(trueHd, PlaybackPlan.localAudio(remux, trueHdTv))
        assertNull(PlaybackPlan.request(70, choices(audio = trueHd).copy(file = remux), "c", trueHdTv).audioIndex)
        // Asked for, a track is kept; so is one with nothing better in its language.
        assertEquals(trueHd, PlaybackPlan.initialAudio(remux, 1))
        assertEquals(french, PlaybackPlan.localAudio(remux.copy(selectedAudio = 4), tv))
        val noEac3 = TestCaps.hdrTv(passthrough = emptyList())
        assertEquals("never the commentary", trueHd, PlaybackPlan.localAudio(remux, noEac3))
    }

    @Test
    fun `a quality caps the bitrate and height`() {
        val asked = PlaybackPlan.request(70, choices(quality = Qualities.byId("8m"), mode = PlaybackMode.Transcode, streaming = true), "c", tv)
        assertEquals(8_000, asked.maxBitrate)
        assertEquals(1_080, asked.maxHeight)
        assertEquals(PlaybackMode.Transcode, asked.mode)
    }

    @Test
    fun `a stream is certain for other modes, videos that don't play here, containers and audio`() {
        assertFalse(PlaybackPlan.streams(PlaybackMode.Auto, CapsDecision.Direct, file, ac3, tv))
        assertTrue(PlaybackPlan.streams(PlaybackMode.Remux, CapsDecision.Direct, file, ac3, tv))
        assertTrue(PlaybackPlan.streams(PlaybackMode.Auto, CapsDecision.ServerToneMaps, file, ac3, tv))
        assertTrue(PlaybackPlan.streams(PlaybackMode.Auto, CapsDecision.Direct, file.copy(container = "avi"), ac3, tv))
        assertTrue(PlaybackPlan.streams(PlaybackMode.Auto, CapsDecision.Direct, file, ac3, TestCaps.hdrTv(passthrough = emptyList())))
    }

    @Test
    fun `an unexpected stream is asked again with the indexes it needs`() {
        val info = PlaybackInfo(sessionId = "s", method = PlaybackMethod.Remux, url = "/x", hls = true, fileId = 7, audioIndex = 1)
        // The server's pick was thought to be the French track (its choice changed since), so none was named.
        val stale = choices(audio = aac).copy(file = file.copy(selectedAudio = 2))
        val sent = PlaybackPlan.request(70, stale, "c", tv)
        assertNull(sent.audioIndex)
        assertTrue(PlaybackPlan.needsIndexes(info, sent, stale))
        assertFalse(PlaybackPlan.needsIndexes(info, sent, choices(audio = ac3)))
        assertTrue(PlaybackPlan.needsIndexes(info, PlaybackPlan.request(70, choices(subtitle = pgs), "c", tv), choices(subtitle = pgs)))
        assertFalse(PlaybackPlan.needsIndexes(info.copy(hls = false, method = PlaybackMethod.Direct), sent, stale))
        val withIndex = PlaybackPlan.request(70, choices(audio = aac, streaming = true), "c", tv)
        assertFalse(PlaybackPlan.needsIndexes(info, withIndex, choices(audio = aac, streaming = true)))
        // Automatic audio is whatever the server chose: never asked again for it.
        val automatic = choices(audio = null)
        assertFalse(PlaybackPlan.needsIndexes(info, PlaybackPlan.request(70, automatic, "c", tv), automatic))
    }

    @Test
    fun `the Dolby Vision guards`() {
        val p5 = file.copy(streams = listOf(video.copy(dynamicRange = DynamicRange.DolbyVision, dvProfile = 5), ac3))
        val p7 = file.copy(streams = listOf(video.copy(dynamicRange = DynamicRange.DolbyVision, dvProfile = 7, dvBlCompat = 6), ac3))
        val p8 = file.copy(streams = listOf(video.copy(dynamicRange = DynamicRange.DolbyVision, dvProfile = 8, dvBlCompat = 1), ac3))
        fun info(method: PlaybackMethod) = PlaybackInfo("s", method, "/x", method != PlaybackMethod.Direct, 7)
        assertEquals(PlaybackMode.Transcode, DvGuards.retryMode(info(PlaybackMethod.Remux), p5, tv))
        // A remux that names its Dolby Vision (the master playlist) keeps it: never tone-mapped instead.
        val named = info(PlaybackMethod.Remux).copy(videoRange = "Dolby Vision P5")
        assertNull(DvGuards.retryMode(named, p5, TestCaps.hdrTv(dv = listOf(5))))
        val unnamed = info(PlaybackMethod.Remux).copy(videoRange = "Dolby Vision P5 not decoded (wrong colours)")
        assertEquals(PlaybackMode.Transcode, DvGuards.retryMode(unnamed, p5, tv))
        assertNull(DvGuards.retryMode(info(PlaybackMethod.Direct), p5, TestCaps.hdrTv(dv = listOf(5))))
        assertNull(DvGuards.retryMode(info(PlaybackMethod.Transcode), p5, tv))
        assertEquals(PlaybackMode.Remux, DvGuards.retryMode(info(PlaybackMethod.Direct), p7, tv))
        assertNull(DvGuards.retryMode(info(PlaybackMethod.Direct), p7, TestCaps.hdrTv(dv = listOf(7))))
        // Profile 7's decoder, listed on a screen without Dolby Vision: its base layer is what plays.
        val p7Decoder = tv.copy(report = tv.report.copy(dolbyVisionProfiles = listOf(DolbyVisionCaps(7, 9))))
        assertEquals(PlaybackMode.Remux, DvGuards.retryMode(info(PlaybackMethod.Direct), p7, p7Decoder))
        assertNull(DvGuards.retryMode(info(PlaybackMethod.Remux), p7, tv))
        assertNull(DvGuards.retryMode(info(PlaybackMethod.Direct), p8, tv))
        assertNull(DvGuards.retryMode(info(PlaybackMethod.Remux), file, tv))
    }

    @Test
    fun `container types for the engine`() {
        assertEquals("video/x-matroska", PlaybackPlan.mimeTypeOf("mkv"))
        assertEquals("video/mp4", PlaybackPlan.mimeTypeOf("MP4"))
        assertNull(PlaybackPlan.mimeTypeOf(null))
    }
}
