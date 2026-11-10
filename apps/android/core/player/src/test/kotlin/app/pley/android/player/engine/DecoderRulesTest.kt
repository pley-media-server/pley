package app.pley.android.player.engine

import android.media.MediaCodecInfo.CodecProfileLevel
import androidx.media3.common.C
import androidx.media3.common.ColorInfo
import androidx.media3.common.Format
import androidx.media3.common.MimeTypes
import androidx.media3.exoplayer.mediacodec.MediaCodecInfo
import androidx.media3.exoplayer.mediacodec.MediaCodecSelector
import androidx.media3.exoplayer.mediacodec.MediaCodecUtil
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import app.pley.android.caps.DecoderPolicy
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(RobolectricTestRunner::class)
class DecoderRulesTest {
    private fun info(name: String, mime: String, hardware: Boolean) =
        MediaCodecInfo.newInstance(name, mime, mime, null, hardware, !hardware, hardware, false, false)

    private val listed = mapOf(
        MimeTypes.VIDEO_H264 to listOf(info("c2.android.avc.decoder", MimeTypes.VIDEO_H264, false), info("c2.vendor.avc.decoder", MimeTypes.VIDEO_H264, true)),
        MimeTypes.VIDEO_H265 to listOf(info("c2.vendor.hevc.decoder", MimeTypes.VIDEO_H265, true), info("c2.android.hevc.decoder", MimeTypes.VIDEO_H265, false)),
        MimeTypes.VIDEO_DOLBY_VISION to listOf(info("c2.vendor.dolby.decoder", MimeTypes.VIDEO_DOLBY_VISION, true)),
        MimeTypes.VIDEO_AV1 to listOf(info("c2.android.av1.decoder", MimeTypes.VIDEO_AV1, false)),
        MimeTypes.AUDIO_AAC to listOf(info("c2.android.aac.decoder", MimeTypes.AUDIO_AAC, false)),
    )
    private val base = MediaCodecSelector { mime, _, _ -> listed[mime].orEmpty() }

    private fun names(rules: DecoderRules, mime: String, format: Format?) =
        rules.selectorFor(base, format).getDecoderInfos(mime, false, false).map { it.name }

    private fun video(mime: String, width: Int, height: Int, fps: Float, codecs: String? = null, color: ColorInfo? = null): Format =
        Format.Builder().setSampleMimeType(mime).setWidth(width).setHeight(height).setFrameRate(fps).setCodecs(codecs).setColorInfo(color).build()

    private val hdr10 = ColorInfo.Builder().setColorSpace(C.COLOR_SPACE_BT2020).setColorTransfer(C.COLOR_TRANSFER_ST2084).setLumaBitdepth(10).setChromaBitdepth(10).build()

    @Test
    fun tvsKeepHardwareVideoDecodersOnly() {
        val rules = DecoderRules({ DecoderPolicy(tv = true) })
        assertEquals(listOf("c2.vendor.avc.decoder"), names(rules, MimeTypes.VIDEO_H264, video(MimeTypes.VIDEO_H264, 1280, 720, 24f)))
        assertEquals(emptyList<String>(), names(rules, MimeTypes.VIDEO_AV1, video(MimeTypes.VIDEO_AV1, 1280, 720, 24f)))
        // Audio decoders are never limited.
        assertEquals(listOf("c2.android.aac.decoder"), names(rules, MimeTypes.AUDIO_AAC, null))
    }

    @Test
    fun phonesPutHardwareFirstAndKeepSoftwareUpTo1080p30EightBitSdr() {
        val rules = DecoderRules({ DecoderPolicy(tv = false) })
        assertEquals(
            listOf("c2.vendor.avc.decoder", "c2.android.avc.decoder"),
            names(rules, MimeTypes.VIDEO_H264, video(MimeTypes.VIDEO_H264, 1920, 1080, 29.97f, "avc1.640028")),
        )
        // Portrait counts the same.
        assertEquals(2, names(rules, MimeTypes.VIDEO_H264, video(MimeTypes.VIDEO_H264, 1080, 1920, 30f)).size)
        // 60 fps, 4K, 10-bit or HDR: hardware only.
        assertEquals(listOf("c2.vendor.avc.decoder"), names(rules, MimeTypes.VIDEO_H264, video(MimeTypes.VIDEO_H264, 1920, 1080, 60f)))
        assertEquals(listOf("c2.vendor.hevc.decoder"), names(rules, MimeTypes.VIDEO_H265, video(MimeTypes.VIDEO_H265, 3840, 2160, 24f)))
        assertEquals(listOf("c2.vendor.hevc.decoder"), names(rules, MimeTypes.VIDEO_H265, video(MimeTypes.VIDEO_H265, 1920, 1080, 24f, "hvc1.2.4.L120.B0")))
        assertEquals(listOf("c2.vendor.hevc.decoder"), names(rules, MimeTypes.VIDEO_H265, video(MimeTypes.VIDEO_H265, 1920, 1080, 24f, color = hdr10)))
        // AV1 in software only, as on the Note20 Ultra: 1080p30 8-bit plays, 4K doesn't.
        assertEquals(1, names(rules, MimeTypes.VIDEO_AV1, video(MimeTypes.VIDEO_AV1, 1920, 1080, 24f, "av01.0.08M.08")).size)
        assertEquals(0, names(rules, MimeTypes.VIDEO_AV1, video(MimeTypes.VIDEO_AV1, 3840, 2160, 24f, "av01.0.12M.10")).size)
    }

    @Test
    fun theDebugSwitchAllowsSoftwareAtAnySize() {
        val rules = DecoderRules({ DecoderPolicy(tv = true, allowSoftware = true) })
        assertEquals(1, names(rules, MimeTypes.VIDEO_AV1, video(MimeTypes.VIDEO_AV1, 3840, 2160, 60f, "av01.0.12M.10")).size)
    }

    @Test
    fun dolbyVisionDecodersAreLeftOutWhereDolbyVisionIsNotShown() {
        val format = video(MimeTypes.VIDEO_DOLBY_VISION, 3840, 2160, 23.976f, "dvh1.08.06")
        val hdr10Screen = DecoderRules({ DecoderPolicy(tv = true) }, dolbyVisionShown = { false })
        assertEquals(emptyList<String>(), names(hdr10Screen, MimeTypes.VIDEO_DOLBY_VISION, format))
        val dvScreen = DecoderRules({ DecoderPolicy(tv = true) }, dolbyVisionShown = { true })
        assertEquals(listOf("c2.vendor.dolby.decoder"), names(dvScreen, MimeTypes.VIDEO_DOLBY_VISION, format))
        val unknown = DecoderRules({ DecoderPolicy(tv = true) })
        assertEquals(listOf("c2.vendor.dolby.decoder"), names(unknown, MimeTypes.VIDEO_DOLBY_VISION, format))
        // Profiles 5 and 7: no other decoder plays them, so theirs stays.
        val p5 = video(MimeTypes.VIDEO_DOLBY_VISION, 3840, 2160, 23.976f, "dvhe.05.06")
        assertEquals(listOf("c2.vendor.dolby.decoder"), names(hdr10Screen, MimeTypes.VIDEO_DOLBY_VISION, p5))
        val p7 = video(MimeTypes.VIDEO_DOLBY_VISION, 3840, 2160, 23.976f, "dvhe.07.06")
        assertEquals(listOf("c2.vendor.dolby.decoder"), names(hdr10Screen, MimeTypes.VIDEO_DOLBY_VISION, p7))
    }

    @Test
    fun profileSevenOnADrivePlaysOnTheHevcDecoderAsItsBaseLayer() {
        val p7 = video(MimeTypes.VIDEO_DOLBY_VISION, 3840, 2160, 23.976f, "dvhe.07.06")
        val drive = DecoderRules({ DecoderPolicy(tv = true) }, dolbyVisionShown = { true }, profile7BaseLayer = { true })
        assertEquals(listOf("c2.vendor.hevc.decoder"), names(drive, MimeTypes.VIDEO_DOLBY_VISION, p7))
        // Other profiles keep their decoders: profile 8.1 its own where Dolby Vision is shown.
        val p8 = video(MimeTypes.VIDEO_DOLBY_VISION, 3840, 2160, 23.976f, "dvh1.08.06")
        assertEquals(listOf("c2.vendor.dolby.decoder"), names(drive, MimeTypes.VIDEO_DOLBY_VISION, p8))
        // A server's stream: profile 7's decoder stays.
        val stream = DecoderRules({ DecoderPolicy(tv = true) }, dolbyVisionShown = { false })
        assertEquals(listOf("c2.vendor.dolby.decoder"), names(stream, MimeTypes.VIDEO_DOLBY_VISION, p7))
    }

    @Test
    fun dolbyVisionShownGetsItsOwnDecodersFirstWhateverTheModeInUse() {
        val context = RuntimeEnvironment.getApplication()
        val format = video(MimeTypes.VIDEO_DOLBY_VISION, 3840, 2160, 23.976f, "dvh1.08.06")
        val shown = DecoderRules({ DecoderPolicy(tv = true) }, dolbyVisionShown = { true })
        assertEquals(
            "then the base layer's, should the Dolby Vision decoder fail",
            listOf("c2.vendor.dolby.decoder", "c2.vendor.hevc.decoder"),
            shown.dolbyVisionDecoders(context, base, format, secure = false, tunneling = false)?.map { it.name },
        )
        // Not shown, unknown, or not Dolby Vision: Media3's own list.
        assertNull(DecoderRules({ DecoderPolicy(tv = true) }, dolbyVisionShown = { false }).dolbyVisionDecoders(context, base, format, false, false))
        assertNull(DecoderRules({ DecoderPolicy(tv = true) }).dolbyVisionDecoders(context, base, format, false, false))
        val hevc = video(MimeTypes.VIDEO_H265, 3840, 2160, 23.976f, "hvc1.2.4.L153.B0")
        assertNull(shown.dolbyVisionDecoders(context, base, hevc, false, false))
    }

    @Test
    fun aRepackagedStreamsSamplesAreNamedDolbyVisionAsTheirTrackIs() {
        val rules = DecoderRules({ DecoderPolicy(tv = true) }, dolbyVisionShown = { true })
        // An HLS remux in MPEG-TS: the playlist names the track, the segments their HEVC.
        val track = video(MimeTypes.VIDEO_DOLBY_VISION, 3840, 2160, 23.976f, "dvh1.08.06").buildUpon().setId("0").build()
        val sample = video(MimeTypes.VIDEO_H265, 3840, 1606, 23.976f, "hvc1.2.4.L150.B0", hdr10).buildUpon().setId("0").build()
        val named = rules.named(sample, arrayOf(track))
        assertEquals(MimeTypes.VIDEO_DOLBY_VISION, named.sampleMimeType)
        assertEquals("dvh1.08.06", named.codecs)
        assertEquals("the picture stays the sample's", 1606 to hdr10, named.height to named.colorInfo)
        // Profiles 5 and 7, which the playlist names alone.
        for (codecs in listOf("dvh1.05.06", "dvh1.07.06")) {
            val alone = track.buildUpon().setCodecs(codecs).build()
            assertEquals(codecs, rules.named(sample, arrayOf(alone)).codecs)
        }

        // As it is: Dolby Vision already, a track that isn't, one over another codec.
        val dolbyVision = video(MimeTypes.VIDEO_DOLBY_VISION, 3840, 2160, 23.976f, "dvhe.08.06")
        assertSame(dolbyVision, rules.named(dolbyVision, arrayOf(track)))
        val hevc = video(MimeTypes.VIDEO_H265, 3840, 2160, 23.976f, "hvc1.2.4.L153.B0").buildUpon().setId("0").build()
        assertSame(sample, rules.named(sample, arrayOf(hevc)))
        val h264 = video(MimeTypes.VIDEO_H264, 1920, 1080, 23.976f, "avc1.640028").buildUpon().setId("0").build()
        assertSame(h264, rules.named(h264, arrayOf(track)))
        // A sample goes by its own track, the one with its id: not by another that is Dolby Vision, nor without one.
        val second = sample.buildUpon().setId("1").build()
        assertSame(second, rules.named(second, arrayOf(track, hevc.buildUpon().setId("1").build())))
        assertSame(second, rules.named(second, arrayOf(track)))
        assertSame(sample, rules.named(sample, emptyArray()))
        // The server's stream of one size has no ids, as Media3 reads it: its one track is the sample's.
        val (unnamedTrack, unnamedSample) = track.buildUpon().setId(null as String?).build() to sample.buildUpon().setId(null as String?).build()
        assertEquals("dvh1.08.06", rules.named(unnamedSample, arrayOf(unnamedTrack)).codecs)
        // Several without ids: which is the sample's can't be told.
        assertSame(unnamedSample, rules.named(unnamedSample, arrayOf(unnamedTrack, unnamedTrack)))
    }

    @Test
    fun aSampleNamedDolbyVisionGoesWhereAFilesDoes() {
        val context = RuntimeEnvironment.getApplication()
        val track = video(MimeTypes.VIDEO_DOLBY_VISION, 3840, 2160, 23.976f, "dvh1.08.06").buildUpon().setId("0").build()
        val sample = video(MimeTypes.VIDEO_H265, 3840, 2160, 23.976f, "hvc1.2.4.L150.B0", hdr10).buildUpon().setId("0").build()
        val shown = DecoderRules({ DecoderPolicy(tv = true) }, dolbyVisionShown = { true })
        val named = shown.named(sample, arrayOf(track))
        assertEquals(
            listOf("c2.vendor.dolby.decoder", "c2.vendor.hevc.decoder"),
            shown.dolbyVisionDecoders(context, base, named, secure = false, tunneling = false)?.map { it.name },
        )
        // Not shown: profile 8 gets no Dolby Vision decoder (Media3 then takes its base layer's), profile 5 keeps its own.
        val notShown = DecoderRules({ DecoderPolicy(tv = true) }, dolbyVisionShown = { false })
        assertEquals(emptyList<String>(), names(notShown, MimeTypes.VIDEO_DOLBY_VISION, named))
        val p5 = notShown.named(sample, arrayOf(track.buildUpon().setCodecs("dvh1.05.06").build()))
        assertEquals(listOf("c2.vendor.dolby.decoder"), names(notShown, MimeTypes.VIDEO_DOLBY_VISION, p5))
        // The player's details say what its decoder makes of it.
        assertEquals("Dolby Vision", PlaybackAnalytics.rangeOf(named, MimeTypes.VIDEO_DOLBY_VISION))
        assertEquals("HDR10", PlaybackAnalytics.rangeOf(named, MimeTypes.VIDEO_H265))
    }

    @Test
    fun dolbyVisionCodecStringsNameTheCodecUnderThem() {
        assertEquals(MimeTypes.VIDEO_H265, DecoderRules.baseLayerOf("dvh1.08.06"))
        assertEquals(MimeTypes.VIDEO_H265, DecoderRules.baseLayerOf("dvhe.05.06,ec-3"))
        assertEquals(MimeTypes.VIDEO_H264, DecoderRules.baseLayerOf("dvav.09.05"))
        assertEquals(MimeTypes.VIDEO_AV1, DecoderRules.baseLayerOf("dav1.10.06"))
        assertNull(DecoderRules.baseLayerOf("hvc1.2.4.L153.B0"))
        assertNull(DecoderRules.baseLayerOf(null))
    }

    @Test
    fun theFormatsBitDepthComesFromItsCodecString() {
        val depth = DecoderRules.Picture::bitDepthOf
        assertEquals(10, depth("hvc1.2.4.L153.B0"))
        assertEquals(10, depth("hev1.A2.4.L120.B0"))
        assertEquals(8, depth("hvc1.1.6.L120.90"))
        assertEquals(10, depth("avc1.6E0028"))
        assertEquals(8, depth("avc1.640028"))
        assertEquals(10, depth("vp09.02.40.10"))
        assertEquals(8, depth("vp09.00.40.08"))
        assertEquals(10, depth("av01.0.12M.10"))
        assertEquals(10, depth("dvh1.08.06"))
        assertEquals(8, depth(null))
    }

    @Test
    fun dolbyVisionCodecStringsGiveTheProfileAndTheBaseLayersDecoder() {
        fun dv(codecs: String) = Format.Builder().setSampleMimeType(MimeTypes.VIDEO_DOLBY_VISION).setCodecs(codecs).build()
        // Profile 8 (dvh1.08) and its HEVC base layer.
        assertEquals(CodecProfileLevel.DolbyVisionProfileDvheSt, MediaCodecUtil.getCodecProfileAndLevel(dv("dvh1.08.06"))?.first)
        assertEquals(MimeTypes.VIDEO_H265, MediaCodecUtil.getAlternativeCodecMimeType(dv("dvh1.08.06")))
        assertEquals(MimeTypes.VIDEO_H265, MediaCodecUtil.getAlternativeCodecMimeType(dv("dvhe.08.09")))
        // Profile 5 has no backward-compatible base layer: no alternative.
        assertEquals(CodecProfileLevel.DolbyVisionProfileDvheStn, MediaCodecUtil.getCodecProfileAndLevel(dv("dvhe.05.06"))?.first)
        assertNull(MediaCodecUtil.getAlternativeCodecMimeType(dv("dvhe.05.06")))
        // Profile 9 (AVC base) and 10 (AV1 base).
        assertEquals(MimeTypes.VIDEO_H264, MediaCodecUtil.getAlternativeCodecMimeType(dv("dvav.09.05")))
        assertEquals(MimeTypes.VIDEO_AV1, MediaCodecUtil.getAlternativeCodecMimeType(dv("dav1.10.06")))
    }

    @Test
    fun picturesFromFormats() {
        val picture = DecoderRules.Picture.of(video(MimeTypes.VIDEO_H265, 3840, 2160, 23.976f, "hvc1.2.4.L153", hdr10))
        assertEquals(3840, picture.width)
        assertEquals(10, picture.bitDepth)
        assertTrue(picture.hdr)
        val unknown = DecoderRules.Picture.of(Format.Builder().setSampleMimeType(MimeTypes.VIDEO_H264).build())
        assertEquals(0, unknown.width)
        assertFalse(unknown.hdr)
        assertTrue(DecoderRules.Picture.of(Format.Builder().setSampleMimeType(MimeTypes.VIDEO_DOLBY_VISION).build()).hdr)
    }
}
