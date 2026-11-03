package app.pley.android.player.engine

import androidx.media3.common.C
import androidx.media3.common.ColorInfo
import androidx.media3.common.Format
import androidx.media3.common.MimeTypes
import androidx.media3.exoplayer.RendererCapabilities
import androidx.media3.exoplayer.video.MediaCodecVideoRenderer
import androidx.media3.test.utils.robolectric.ShadowMediaCodecConfig
import androidx.test.core.app.ApplicationProvider
import app.pley.android.caps.DecoderPolicy
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

/** The video renderer's word on a format, which decides whether Media3 plays its track at all. */
@RunWith(RobolectricTestRunner::class)
class PolicyVideoRendererTest {
    @get:Rule
    val codecs: ShadowMediaCodecConfig = ShadowMediaCodecConfig.withNoDefaultSupportedCodecs()

    private val hdr10 = ColorInfo.Builder().setColorSpace(C.COLOR_SPACE_BT2020).setColorTransfer(C.COLOR_TRANSFER_ST2084).setLumaBitdepth(10).setChromaBitdepth(10).build()

    /** Profile 7 as Media3 reads it from a Matroska file: its Dolby Vision type, the HDR10 base layer's colour. */
    private val profile7 = Format.Builder()
        .setSampleMimeType(MimeTypes.VIDEO_DOLBY_VISION)
        .setCodecs("dvhe.07.06")
        .setWidth(1920)
        .setHeight(1080)
        .setFrameRate(23.976f)
        .setColorInfo(hdr10)
        .build()

    private fun support(drive: Boolean, format: Format): Int {
        val context = ApplicationProvider.getApplicationContext<android.content.Context>()
        // Software allowed: the test's decoders aren't hardware ones.
        val rules = DecoderRules({ DecoderPolicy(tv = true, allowSoftware = true) }, dolbyVisionShown = { false }, profile7BaseLayer = { drive })
        val renderer = PolicyVideoRenderer(MediaCodecVideoRenderer.Builder(context), context, rules)
        return RendererCapabilities.getFormatSupport(renderer.supportsFormat(format))
    }

    @Test
    fun profileSevenOnADriveIsAFormatTheHevcDecoderPlays() {
        codecs.addDecoders(ShadowMediaCodecConfig.CODEC_INFO_HEVC)
        assertEquals(C.FORMAT_HANDLED, support(drive = true, profile7))
        // A server's stream is never profile 7 played this way: no decoder here plays it.
        assertNotEquals(C.FORMAT_HANDLED, support(drive = false, profile7))
        // Other formats are weighed as they are.
        val hevc = profile7.buildUpon().setSampleMimeType(MimeTypes.VIDEO_H265).setCodecs(null).build()
        assertEquals(C.FORMAT_HANDLED, support(drive = true, hevc))
    }
}
