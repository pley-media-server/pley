package app.pley.android.model.format

import app.pley.android.model.DynamicRange
import app.pley.android.model.MediaFile
import app.pley.android.model.MediaStream
import app.pley.android.model.PleyJson
import app.pley.android.model.StreamKind
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/** Codec levels, frame rates, colour and Dolby Vision in words. */
class TechnicalTest {
    private fun video(
        codec: String = "hevc",
        range: DynamicRange? = DynamicRange.Hdr10,
        dvProfile: Int? = null,
        dvLevel: Int? = null,
        dvBlCompat: Int? = null,
        hdr10plus: Boolean? = null,
    ) = MediaStream(
        index = 0, kind = StreamKind.Video, codec = codec, profile = "Main 10", level = 153, bitDepth = 10, pixFmt = "yuv420p10le",
        dynamicRange = range, dvProfile = dvProfile, dvLevel = dvLevel, dvBlCompat = dvBlCompat, hdr10plus = hdr10plus,
    )

    @Test
    fun `levels are numbered per codec`() {
        assertEquals("5.1", levelLabel("hevc", 153))
        assertEquals("5", levelLabel("hevc", 150))
        assertEquals("4.1", levelLabel("h264", 41))
        assertEquals("4", levelLabel("h264", 40))
        assertEquals("5.1", levelLabel("av1", 13))
        assertNull(levelLabel("hevc", -99))
        assertNull(levelLabel("mpeg2video", 4))
        assertEquals("Main 10 · Level 5.1", profileLevelLabel(video()))
    }

    @Test
    fun `frame rates, sample rates and bit depth`() {
        assertEquals("23.976 fps", frameRateLabel(23.976))
        assertEquals("24 fps", frameRateLabel(24.0))
        assertEquals("29.97 fps", frameRateLabel(29.97))
        assertNull(frameRateLabel(0.0))
        assertEquals("48 kHz", sampleRateLabel(48_000))
        assertEquals("44.1 kHz", sampleRateLabel(44_100))
        assertEquals("10-bit · 4:2:0", bitDepthLabel(video()))
        assertEquals("4:2:2", chromaLabel("yuv422p10le"))
    }

    @Test
    fun `colour names`() {
        assertEquals("BT.2020", colorPrimariesLabel("bt2020"))
        assertEquals("PQ (SMPTE ST 2084)", colorTransferLabel("smpte2084"))
        assertEquals("HLG (ARIB STD-B67)", colorTransferLabel("arib-std-b67"))
        assertEquals("BT.2020 non-constant", colorMatrixLabel("bt2020nc"))
        assertEquals("weird", colorMatrixLabel("weird"))
        assertNull(colorPrimariesLabel("unknown"))
        assertNull(colorPrimariesLabel(null))
    }

    @Test
    fun `Dolby Vision in full`() {
        val p81 = video(range = DynamicRange.DolbyVision, dvProfile = 8, dvLevel = 6, dvBlCompat = 1)
        assertEquals("Profile 8.1 · Level 6 · HDR10-compatible base layer", dolbyVisionDetails(p81))
        assertEquals("Dolby Vision P8.1", hdrFormatLabel(p81))
        assertEquals("Dolby Vision P8.1 + HDR10+", hdrFormatLabel(p81.copy(hdr10plus = true)))
        val p5 = video(range = DynamicRange.DolbyVision, dvProfile = 5, dvLevel = 6, dvBlCompat = 0)
        assertEquals("Profile 5 · Level 6 · no compatible base layer", dolbyVisionDetails(p5))
        val p7 = video(range = DynamicRange.DolbyVision, dvProfile = 7, dvLevel = 6, dvBlCompat = 6)
        assertEquals("Profile 7 · Level 6 · Blu-ray HDR10-compatible base layer", dolbyVisionDetails(p7))
        // A profile 7 converted elsewhere, its id kept: profile 8, as its label says.
        val p8BluRay = p7.copy(dvProfile = 8)
        assertEquals("Profile 8 · Level 6 · Blu-ray HDR10-compatible base layer", dolbyVisionDetails(p8BluRay))
        assertEquals("Dolby Vision P8", hdrFormatLabel(p8BluRay))
        assertNull(dolbyVisionDetails(video()))
        assertEquals("HDR10", hdrFormatLabel(video()))
        assertEquals("SDR", hdrFormatLabel(video(range = DynamicRange.Sdr)))
    }

    @Test
    fun `dimensions with their name`() {
        assertEquals("3840×2160 (4K)", dimensionsLabel(3840, 2160))
        assertEquals("1920×804 (1080p)", dimensionsLabel(1920, 804))
        assertNull(dimensionsLabel(null, 1080))
    }

    @Test
    fun `the server's hdr10plus flag is read`() {
        val json = """
            {"id":1,"path":"a.mkv","streams":[{"index":0,"kind":"video","codec":"hevc","dynamic_range":"dolby_vision",
            "dv_profile":8,"dv_level":6,"dv_bl_compat":1,"hdr10plus":true,"color_primaries":"bt2020"}]}
        """.trimIndent()
        val file = PleyJson.decodeFromString(MediaFile.serializer(), json)
        val v = file.streams.single()
        assertEquals(true, v.hdr10plus)
        assertEquals(6, v.dvLevel)
        assertEquals("bt2020", v.colorPrimaries)
    }
}
