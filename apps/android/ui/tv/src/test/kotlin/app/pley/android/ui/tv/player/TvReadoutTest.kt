package app.pley.android.ui.tv.player

import android.view.Display
import app.pley.android.model.DynamicRange
import app.pley.android.model.MediaFile
import app.pley.android.model.MediaStream
import app.pley.android.model.PlaybackMethod
import app.pley.android.model.StreamAction
import app.pley.android.model.StreamKind
import app.pley.android.model.TranscodeInfo
import app.pley.android.player.DetailRow
import app.pley.android.player.PlaybackDetailsUi
import app.pley.android.player.SubtitleDelivery
import app.pley.android.player.engine.PlaybackStats
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment

/** The Info panel's and Playback details' readout: the server's media info in full, the decision, and this TV. */
@RunWith(RobolectricTestRunner::class)
class TvReadoutTest {
    private val r = RuntimeEnvironment.getApplication().resources

    private val video = MediaStream(
        0, StreamKind.Video, "hevc", profile = "Main 10", level = 153, width = 3840, height = 2160, frameRate = 23.976,
        bitDepth = 10, pixFmt = "yuv420p10le", dynamicRange = DynamicRange.DolbyVision, dvProfile = 8, dvLevel = 6, dvBlCompat = 1,
        colorPrimaries = "bt2020", colorTransfer = "smpte2084", colorSpace = "bt2020nc", bitrate = 58_000_000,
    )
    private val audio = MediaStream(1, StreamKind.Audio, "eac3", language = "eng", channels = 6, channelLayout = "5.1(side)", sampleRate = 48_000, default = true)
    private val srt = MediaStream(2, StreamKind.Subtitle, "subrip", language = "eng", textBased = true, forced = true)
    private val file = MediaFile(id = 1, path = "Film (2024).mkv", size = 62_000_000_000, container = "mkv", durationMs = 7_200_000, bitrate = 62_000_000, streams = listOf(video, audio, srt))

    private fun details(method: PlaybackMethod = PlaybackMethod.Direct, transcode: TranscodeInfo? = null) = PlaybackDetailsUi(
        method = if (method == PlaybackMethod.Direct) "Direct play" else "Transcoding",
        why = null,
        pipeline = null,
        rows = emptyList(),
        engine = emptyList(),
        playbackMethod = method,
        file = file,
        video = video,
        audio = audio,
        subtitle = srt,
        subtitleDelivery = SubtitleDelivery.File,
        transcode = transcode,
    )

    private val stats = PlaybackStats(
        videoDecoder = "c2.amlogic.hevc.decoder", videoDecoderHardware = true, videoCodec = "video/dolby-vision dvhe.08.06",
        width = 3840, height = 2160, frameRate = 23.976f, dynamicRange = "Dolby Vision", audioCodec = "audio/eac3", audioEncoding = 6,
        passthrough = true, renderedFrames = 990, droppedFrames = 10, bandwidth = 120_000_000,
    )

    private fun facts(input: ReadoutInput) = TvReadout.sections(r, input).associate { s -> s.title to s.facts.associate { it.label to it.value } }

    @Test
    fun `the panel shows the file, each stream in full, the decision and this TV`() {
        val display = DisplayNow(3840, 2160, 23.976f, listOf(Display.HdrCapabilities.HDR_TYPE_HDR10))
        val s = facts(ReadoutInput(details(), stats, bufferedMs = 45_000, quality = "Original", display = display))
        assertEquals("Direct play", s.getValue("How it plays")["Playback"])
        // The file as it is needs no reason.
        assertEquals(null, s.getValue("How it plays")["Why"])
        assertEquals("MKV", s.getValue("File")["Container"])
        assertEquals("62 Mbps", s.getValue("File")["Overall bitrate"])
        val v = s.getValue("Video")
        assertEquals("HEVC", v["Codec"])
        assertEquals("Main 10 · Level 5.1", v["Profile"])
        assertEquals("3840×2160 (4K)", v["Resolution"])
        assertEquals("23.976 fps", v["Frame rate"])
        assertEquals("10-bit · 4:2:0", v["Bit depth"])
        assertEquals("Dolby Vision P8.1", v["HDR"])
        assertEquals("Profile 8.1 · Level 6 · HDR10-compatible base layer", v["Dolby Vision"])
        assertEquals("BT.2020", v["Primaries"])
        assertEquals("PQ (SMPTE ST 2084)", v["Transfer"])
        assertEquals("BT.2020 non-constant", v["Matrix"])
        assertEquals("As in the file", v["Current bitrate"])
        val a = s.getValue("Audio")
        assertEquals("5.1 (5.1(side))", a["Channels"])
        assertEquals("48 kHz", a["Sample rate"])
        assertEquals("English", a["Language"])
        assertEquals("Passed through as E-AC-3", a["Output"])
        val sub = s.getValue("Subtitles")
        assertEquals("SRT, text", sub["Format"])
        assertEquals("Forced", sub["Flags"])
        assertEquals("From the file", sub["Shown"])
        val tv = s.getValue("This TV")
        assertEquals("c2.amlogic.hevc.decoder (hardware)", tv["Video decoder"])
        assertEquals("dvhe.08.06", tv["Codec string"])
        assertEquals("3840×2160 at 23.976 Hz · Unchanged", tv["Display"])
        // A Dolby Vision 8.1 stream on a screen that takes HDR10 only: its base layer.
        assertEquals("On (HDR10)", tv["HDR output"])
        assertEquals("10 of 1000", tv["Dropped frames"])
        assertEquals("45 s", tv["Buffered ahead"])
        assertEquals("120 Mbps", tv["Network"])
    }

    @Test
    fun `Dolby Vision decoded as its base layer says so`() {
        val display = DisplayNow(3840, 2160, 23.976f, listOf(Display.HdrCapabilities.HDR_TYPE_HDR10))
        val base = stats.copy(videoDecoderMime = "video/hevc", dynamicRange = "HDR10")
        val tv = facts(ReadoutInput(details(), base, bufferedMs = 0, quality = "", display = display)).getValue("This TV")
        assertEquals("3840×2160 · 23.976 fps · HDR10 · HEVC", tv["Decoded"])
        // What arrives is still the file's Dolby Vision.
        assertEquals("dvhe.08.06", tv["Codec string"])
    }

    @Test
    fun `HDR output says when the screen can't take the video's HDR`() {
        val sdrScreen = DisplayNow(1920, 1080, 60f, emptyList())
        assertEquals("Off, Dolby Vision shown as SDR", TvReadout.hdrOutput(r, stats, sdrScreen))
        assertEquals("Off, the video is SDR", TvReadout.hdrOutput(r, stats.copy(dynamicRange = "SDR"), sdrScreen))
        assertEquals("On (HLG)", TvReadout.hdrOutput(r, stats.copy(dynamicRange = "HLG"), sdrScreen.copy(hdrTypes = listOf(Display.HdrCapabilities.HDR_TYPE_HLG))))
    }

    @Test
    fun `HDR output names HDR10+ where the screen takes it`() {
        val pq = stats.copy(dynamicRange = "HDR10")
        val hdr10plusScreen = DisplayNow(3840, 2160, 23.976f, listOf(Display.HdrCapabilities.HDR_TYPE_HDR10, Display.HdrCapabilities.HDR_TYPE_HDR10_PLUS))
        val hdr10Screen = hdr10plusScreen.copy(hdrTypes = listOf(Display.HdrCapabilities.HDR_TYPE_HDR10))
        // The decoder says HDR10 for both: the stream tells HDR10+ apart.
        assertEquals("On (HDR10+)", TvReadout.hdrOutput(r, pq, hdr10plusScreen, hdr10plus = true))
        assertEquals("On (HDR10)", TvReadout.hdrOutput(r, pq, hdr10Screen, hdr10plus = true))
        assertEquals("On (HDR10)", TvReadout.hdrOutput(r, pq, hdr10plusScreen))
    }

    @Test
    fun `the panel names profile 7's layer, HDR10+ with Dolby Vision, and what the screen gets`() {
        val p7 = video.copy(dvProfile = 7, dvBlCompat = 6, dvEl = "fel", hdr10plus = true)
        val d = details().copy(video = p7, file = file.copy(streams = listOf(p7, audio, srt)), rows = listOf(DetailRow("Shown as", "HDR10+ (Dolby Vision base layer)")))
        val s = facts(ReadoutInput(d, stats))
        val v = s.getValue("Video")
        assertEquals("Dolby Vision P7 FEL + HDR10+", v["HDR"])
        assertEquals("Profile 7 · Level 6 · Blu-ray HDR10-compatible base layer · full enhancement layer", v["Dolby Vision"])
        assertEquals("HDR10+ (Dolby Vision base layer)", s.getValue("How it plays")["Shown as"])
    }

    @Test
    fun `a transcode says what it makes, and the bitrate it aims for`() {
        val t = TranscodeInfo("j", video = StreamAction.Transcode, audio = StreamAction.Transcode, videoOut = "H.264 1920×1080", audioOut = "AAC 2.0", bitrateOut = 8_000_000)
        val d = details(PlaybackMethod.Transcode, t)
        assertEquals("H.264 1080p · AAC 2.0", TvReadout.transcodeTarget(d))
        val compact = TvReadout.compact(r, ReadoutInput(d, stats.copy(videoBitrate = null)))
        val lines = compact.associate { it.label to it.value }
        assertEquals("H.264 1080p · AAC 2.0", lines["Converts to"])
        assertEquals("62 Mbps → 8.0 Mbps", lines["Bitrate"])
        assertTrue(lines.getValue("Video").startsWith("HEVC Main 10 L5.1 · 3840×2160 · 23.976 fps · 10-bit · 4:2:0 · Dolby Vision P8.1"))
    }

    @Test
    fun `nothing but the waiting note before the server answers`() {
        assertTrue(TvReadout.sections(r, ReadoutInput()).isEmpty())
        assertTrue(TvReadout.compact(r, ReadoutInput()).isEmpty())
    }
}
