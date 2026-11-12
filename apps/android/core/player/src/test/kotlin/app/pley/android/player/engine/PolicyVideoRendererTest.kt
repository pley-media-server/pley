package app.pley.android.player.engine

import android.graphics.SurfaceTexture
import android.media.MediaCodecInfo.CodecCapabilities
import android.media.MediaCodecInfo.CodecProfileLevel
import android.os.SystemClock
import android.view.Surface
import androidx.media3.common.C
import androidx.media3.common.ColorInfo
import androidx.media3.common.Format
import androidx.media3.common.MimeTypes
import androidx.media3.common.util.Clock
import androidx.media3.exoplayer.Renderer
import androidx.media3.exoplayer.RendererCapabilities
import androidx.media3.exoplayer.RendererConfiguration
import androidx.media3.exoplayer.analytics.PlayerId
import androidx.media3.exoplayer.drm.DrmSessionEventListener
import androidx.media3.exoplayer.drm.DrmSessionManager
import androidx.media3.exoplayer.source.MediaSource
import androidx.media3.exoplayer.upstream.DefaultAllocator
import androidx.media3.exoplayer.video.MediaCodecVideoRenderer
import androidx.media3.test.utils.FakeSampleStream
import androidx.media3.test.utils.robolectric.ShadowMediaCodecConfig
import androidx.test.core.app.ApplicationProvider
import app.pley.android.caps.DecoderPolicy
import com.google.common.collect.ImmutableList
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

    /**
     * The MIME type of the decoder the renderer starts for a repackaged
     * stream as Media3 reads it from HLS in MPEG-TS: the track named Dolby
     * Vision by the playlist, its samples HEVC by their segments, neither
     * with an id.
     */
    private fun decoderStarted(dolbyVisionShown: Boolean): List<String> {
        val context = ApplicationProvider.getApplicationContext<android.content.Context>()
        val track = Format.Builder()
            .setSampleMimeType(MimeTypes.VIDEO_DOLBY_VISION)
            .setCodecs("dvh1.08.06")
            .setWidth(1920)
            .setHeight(1080)
            .build()
        val sample = track.buildUpon().setSampleMimeType(MimeTypes.VIDEO_H265).setCodecs("hvc1.2.4.L120.B0").setColorInfo(hdr10).build()
        val started = mutableListOf<String>()
        val rules = DecoderRules({ DecoderPolicy(tv = true, allowSoftware = true) }, dolbyVisionShown = { dolbyVisionShown })
        val renderer = PolicyVideoRenderer(MediaCodecVideoRenderer.Builder(context), context, rules) { _, mimeType -> started += mimeType }
        renderer.init(0, PlayerId.UNSET, Clock.DEFAULT)
        renderer.handleMessage(Renderer.MSG_SET_VIDEO_OUTPUT, Surface(SurfaceTexture(0)))
        val stream = FakeSampleStream(
            DefaultAllocator(true, 1024),
            null,
            DrmSessionManager.DRM_UNSUPPORTED,
            DrmSessionEventListener.EventDispatcher(),
            sample,
            listOf(FakeSampleStream.FakeSampleStreamItem.oneByteSample(0, C.BUFFER_FLAG_KEY_FRAME), FakeSampleStream.FakeSampleStreamItem.END_OF_STREAM_ITEM),
        )
        stream.writeData(0)
        renderer.enable(RendererConfiguration.DEFAULT, arrayOf(track), stream, 0, false, true, 0, 0, MediaSource.MediaPeriodId(Any()))
        renderer.render(0, SystemClock.elapsedRealtime() * 1000)
        renderer.disable()
        renderer.release()
        return started
    }

    @Test
    fun aRepackagedStreamNamedDolbyVisionByItsPlaylistStartsTheDolbyVisionDecoder() {
        val level = CodecProfileLevel().apply {
            profile = CodecProfileLevel.DolbyVisionProfileDvheSt
            level = CodecProfileLevel.DolbyVisionLevelUhd24
        }
        val dolbyVision = ShadowMediaCodecConfig.CodecInfo(
            "test.dolby.decoder",
            MimeTypes.VIDEO_DOLBY_VISION,
            ImmutableList.of(level),
            ImmutableList.of(CodecCapabilities.COLOR_FormatYUV420Flexible),
        )
        codecs.addDecoders(ShadowMediaCodecConfig.CODEC_INFO_HEVC, dolbyVision)
        assertEquals(listOf(MimeTypes.VIDEO_DOLBY_VISION), decoderStarted(dolbyVisionShown = true))
        // A screen that doesn't show it: the base layer's decoder, as for a file.
        assertEquals(listOf(MimeTypes.VIDEO_H265), decoderStarted(dolbyVisionShown = false))
    }
}
