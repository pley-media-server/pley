package app.pley.android.caps

import app.pley.android.model.DynamicRange
import app.pley.android.model.caps.DeviceForm
import app.pley.android.model.caps.DolbyVisionCaps
import app.pley.android.model.caps.VideoDecoderCaps
import app.pley.android.testing.caps.capsOf
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** What the two devices probed over adb are classified as. */
class RealDevicesTest {
    @Test
    fun googleTvStreamerOnAnHdr10Monitor() {
        val input = RealDevices.googleTvStreamer
        assertEquals("Google TV Streamer", input.device.model)
        assertEquals(34, input.device.sdk)
        assertTrue("aliases and secure decoders are left out", input.decoders.none { it.name.endsWith(".secure") || it.name.startsWith("OMX.") })

        val caps = capsOf(input)
        val report = caps.report
        assertEquals(listOf("h264", "hevc", "vp9", "av1", "mpeg2video", "vp8"), report.videoCodecs)
        assertTrue("no software decoder on a TV", report.video!!.all { it.hardware })
        assertEquals(
            listOf(
                VideoDecoderCaps("h264", true, listOf("Constrained Baseline", "Baseline", "Main", "High", "High 10"), 3840, 2160, 60.0, 10, 52),
                VideoDecoderCaps("hevc", true, listOf("Main", "Main 10", "Main Still Picture"), 3840, 2160, 60.0, 10, 153),
                VideoDecoderCaps("vp9", true, listOf("Profile 0", "Profile 1", "Profile 2"), 3840, 2160, 60.0, 10, 51),
                VideoDecoderCaps("av1", true, listOf("Main"), 3840, 2160, 60.0, 10, 13),
                VideoDecoderCaps("mpeg2video", true, listOf("Main"), 1920, 1080, 60.0, 8, null),
                VideoDecoderCaps("vp8", true, emptyList(), 1920, 1080, 60.0, 8, null),
            ),
            report.video,
        )

        // The monitor shows HDR10 only, in every mode.
        assertEquals(listOf(DynamicRange.Hdr10), caps.details.screenHdrTypes)
        assertEquals(listOf(DynamicRange.Hdr10), report.hdrTypes)
        assertTrue(report.hdr)
        // Dolby Vision decoders for profiles 4, 5, 8, 9 and 10 (not 7), up to 2160p60, unused on this
        // screen: listed, so the server says the screen is what doesn't show Dolby Vision.
        assertEquals(
            listOf(4, 5, 8, 9, 10).map { DolbyVisionCaps(it, 9) },
            caps.details.dolbyVisionProfiles,
        )
        assertEquals(caps.details.dolbyVisionProfiles, report.dolbyVisionProfiles)
        assertFalse(report.dolbyVision)

        assertEquals(3840 to 2160, report.display!!.width to report.display!!.height)
        assertEquals(listOf(23.976f, 24f, 25f, 29.97f, 30f, 50f, 59.94f, 60f), report.display!!.refreshRates)
        assertEquals(MatchContent.Never, input.display!!.matchContent)

        // Stereo PCM to the monitor: nothing passes through.
        assertEquals(emptyList<String>(), report.audioPassthrough)
        assertEquals(2, report.audioChannels)
        assertEquals(listOf("aac", "mp3", "opus", "vorbis", "flac", "ac3", "eac3", "pcm_s16le", "pcm_s24le"), report.audioDecode)

        assertEquals(listOf("google-tv-streamer-dv-hdr10plus-hybrids", "google-tv-lossless-passthrough"), caps.details.quirks)
        assertTrue(caps.details.dvHdr10PlusHybridsBroken)
        assertEquals(DeviceForm.Tv, report.device!!.form)
        assertEquals("4 GB: T2", "high", report.device!!.tier)
        assertFalse(report.device!!.fireTv)
    }

    @Test
    fun googleTvStreamerOnADolbyVisionTv() {
        // The same box on a screen that shows Dolby Vision: its profiles go to the server.
        val input = RealDevices.googleTvStreamer
        val dvScreen = input.display!!.copy(modes = input.display!!.modes.map { it.copy(hdrTypes = listOf(1, 2, 3)) }, hdrCapabilities = listOf(1, 2, 3))
        val caps = capsOf(input.copy(display = dvScreen))
        assertEquals(listOf(4, 5, 8, 9, 10), caps.report.dolbyVisionProfiles!!.map { it.profile })
        assertTrue(caps.report.dolbyVision)
        assertEquals(listOf(DynamicRange.Hdr10, DynamicRange.Hlg, DynamicRange.DolbyVision), caps.report.hdrTypes)
    }

    @Test
    fun googleTvStreamerWithDolbyVisionTurnedOff() {
        // The TV shows Dolby Vision, but the person turned it off in the system's settings: the
        // modes still list it, getHdrCapabilities doesn't, and Media3 wouldn't decode it.
        val input = RealDevices.googleTvStreamer
        val off = input.display!!.copy(modes = input.display!!.modes.map { it.copy(hdrTypes = listOf(1, 2, 3)) }, hdrCapabilities = listOf(2, 3))
        val caps = capsOf(input.copy(display = off))
        assertEquals(listOf(DynamicRange.Hdr10, DynamicRange.Hlg), caps.report.hdrTypes)
        assertFalse(caps.report.dolbyVision)
    }

    @Test
    fun galaxyNote20Ultra() {
        val input = RealDevices.galaxyNote20Ultra
        assertEquals("SM-N986W", input.device.model)
        assertEquals(33, input.device.sdk)
        val caps = capsOf(input)
        val report = caps.report
        assertEquals(listOf("h264", "hevc", "vp9", "av1", "vp8"), report.videoCodecs)
        val av1 = report.video!!.single { it.codec == "av1" }
        assertEquals("no AV1 hardware: software within the phone limit", VideoDecoderCaps("av1", false, listOf("Main"), 1920, 1080, 30.0, 8, 15), av1)
        val hevc = report.video!!.single { it.codec == "hevc" }
        assertEquals(VideoDecoderCaps("hevc", true, listOf("Main", "Main 10", "Main Still Picture"), 3840, 2160, 60.0, 10, 183), hevc)

        // getHdrCapabilities (Android 13): HDR10, HLG and HDR10+; the Qualcomm HEVC decoder lists HDR10+.
        assertEquals(listOf(DynamicRange.Hdr10, DynamicRange.Hdr10Plus, DynamicRange.Hlg), report.hdrTypes)
        assertTrue(report.hdr)
        assertFalse("no Dolby Vision decoder", report.dolbyVision)
        assertEquals(emptyList<DolbyVisionCaps>(), caps.details.dolbyVisionProfiles)

        assertEquals(1080 to 2316, report.display!!.width to report.display!!.height)
        assertEquals(listOf(48f, 60f, 96f, 120f), report.display!!.refreshRates)
        assertEquals(2, report.audioChannels)
        assertEquals(listOf("aac", "mp3", "opus", "vorbis", "flac", "ac3", "eac3", "ac4", "pcm_s16le", "pcm_s24le"), report.audioDecode)
        assertEquals(emptyList<String>(), caps.details.quirks)
        assertEquals(DeviceForm.Phone, report.device!!.form)
        assertEquals("high", report.device!!.tier)
    }
}
