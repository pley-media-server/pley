package app.pley.android.player.engine

import androidx.media3.common.C
import androidx.media3.common.ColorInfo
import androidx.media3.common.Format
import androidx.media3.common.MimeTypes
import androidx.media3.common.Player
import androidx.media3.exoplayer.DecoderCounters
import androidx.media3.exoplayer.analytics.AnalyticsListener
import androidx.media3.common.Timeline
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

@RunWith(RobolectricTestRunner::class)
class AnalyticsTest {
    @Test
    fun overOnePercentDroppedInTenSecondsStruggles() {
        val detector = StruggleDetector()
        // 24 fps, one frame dropped per second: about 4 %.
        var rendered = 0
        var dropped = 0
        var struggling = false
        for (second in 0..10) {
            struggling = detector.add(second * 1000L, rendered, dropped, 1)
            rendered += 23
            dropped += 1
        }
        assertTrue(struggling)
    }

    @Test
    fun aFewDropsAreFine() {
        val detector = StruggleDetector()
        var struggling = false
        for (second in 0..30) {
            // Two drops in 30 s of 24 fps.
            struggling = struggling || detector.add(second * 1000L, second * 24, if (second >= 15) 2 else 0, 1)
        }
        assertFalse(struggling)
    }

    @Test
    fun tenInARowStruggles() {
        val detector = StruggleDetector()
        assertTrue(detector.add(0, 1000, 10, 10))
    }

    @Test
    fun theWindowSlides() {
        val detector = StruggleDetector()
        // A burst of 20 drops early on, then clean playback.
        detector.add(0, 0, 0, 0)
        assertTrue(detector.add(5_000, 1_000, 20, 5))
        var last = true
        for (second in 6..30) last = detector.add(second * 1000L, 1_000 + (second - 5) * 24, 20, 5)
        assertFalse(last)
    }

    @Test
    fun statsFollowThePlayback() {
        var now = 1_000L
        var latest = PlaybackStats()
        val analytics = PlaybackAnalytics({ now }, { name -> name.startsWith("c2.vendor") }) { latest = it }
        val time = AnalyticsListener.EventTime(0, Timeline.EMPTY, 0, null, 0, Timeline.EMPTY, 0, null, 0, 0)
        analytics.onPrepare(displayModeId = 46)
        analytics.onVideoDecoderInitialized(time, "c2.vendor.hevc.decoder", 0, 10)
        val hdr10 = ColorInfo.Builder().setColorSpace(C.COLOR_SPACE_BT2020).setColorTransfer(C.COLOR_TRANSFER_ST2084).build()
        analytics.onVideoInputFormatChanged(
            time,
            Format.Builder().setSampleMimeType(MimeTypes.VIDEO_H265).setCodecs("hvc1.2.4.L153").setWidth(3840).setHeight(2160).setFrameRate(23.976f).setColorInfo(hdr10).build(),
            null,
        )
        now = 1_640L
        analytics.onRenderedFirstFrame(time, Any(), 0)
        analytics.onPlaybackStateChanged(time, Player.STATE_READY)
        analytics.onPlaybackStateChanged(time, Player.STATE_BUFFERING)
        analytics.onPositionDiscontinuity(time, POSITION, POSITION, Player.DISCONTINUITY_REASON_SEEK)
        analytics.onPlaybackStateChanged(time, Player.STATE_BUFFERING)
        analytics.onPlaybackStateChanged(time, Player.STATE_READY)
        analytics.onCounters(DecoderCounters().apply { renderedOutputBufferCount = 240; droppedBufferCount = 1; maxConsecutiveDroppedBufferCount = 1 })

        assertEquals(640L, latest.firstFrameMs)
        assertEquals("c2.vendor.hevc.decoder", latest.videoDecoder)
        assertEquals(true, latest.videoDecoderHardware)
        assertEquals("video/hevc hvc1.2.4.L153", latest.videoCodec)
        assertEquals("HDR10", latest.dynamicRange)
        assertEquals(23.976f, latest.frameRate)
        assertEquals(46, latest.displayModeId)
        assertEquals("a stall while playing counts, a seek doesn't", 1, latest.rebuffers)
        assertEquals(240, latest.renderedFrames)
        assertEquals(1, latest.droppedFrames)
    }

    @Test
    fun dolbyVisionIsWhatItsDecoderMakesOfIt() {
        var latest = PlaybackStats()
        val decoders = mapOf("c2.vendor.dvhe.decoder" to MimeTypes.VIDEO_DOLBY_VISION, "c2.vendor.hevc.decoder" to MimeTypes.VIDEO_H265)
        val analytics = PlaybackAnalytics({ 0L }, { true }, { decoders[it] }) { latest = it }
        val time = AnalyticsListener.EventTime(0, Timeline.EMPTY, 0, null, 0, Timeline.EMPTY, 0, null, 0, 0)
        val hdr10 = ColorInfo.Builder().setColorSpace(C.COLOR_SPACE_BT2020).setColorTransfer(C.COLOR_TRANSFER_ST2084).build()
        val dv = Format.Builder().setSampleMimeType(MimeTypes.VIDEO_DOLBY_VISION).setCodecs("dvhe.08.06").setColorInfo(hdr10).build()
        analytics.onPrepare(displayModeId = null)
        // The format comes before its decoder: Dolby Vision until it's known.
        analytics.onVideoInputFormatChanged(time, dv, null)
        assertEquals("Dolby Vision", latest.dynamicRange)
        // Media3 fell back to the base layer's decoder: HDR10.
        analytics.onVideoDecoderInitialized(time, "c2.vendor.hevc.decoder", 0, 10)
        assertEquals("HDR10", latest.dynamicRange)
        assertEquals(MimeTypes.VIDEO_H265, latest.videoDecoderMime)
        analytics.onPrepare(displayModeId = null)
        analytics.onVideoInputFormatChanged(time, dv, null)
        analytics.onVideoDecoderInitialized(time, "c2.vendor.dvhe.decoder", 0, 10)
        assertEquals("Dolby Vision", latest.dynamicRange)
    }

    private companion object {
        val POSITION = Player.PositionInfo(null, 0, null, null, 0, 0, 0, C.INDEX_UNSET, C.INDEX_UNSET)
    }
}
