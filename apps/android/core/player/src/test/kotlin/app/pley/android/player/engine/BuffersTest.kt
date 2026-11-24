package app.pley.android.player.engine

import androidx.media3.common.C
import androidx.media3.test.utils.FakeTimeline
import androidx.media3.datasource.DataSpec
import androidx.media3.datasource.HttpDataSource
import androidx.media3.exoplayer.LoadControl
import androidx.media3.exoplayer.analytics.PlayerId
import androidx.media3.exoplayer.source.LoadEventInfo
import androidx.media3.exoplayer.source.MediaLoadData
import androidx.media3.exoplayer.source.MediaSource
import androidx.media3.exoplayer.upstream.LoadErrorHandlingPolicy
import app.pley.android.common.DeviceProfile
import app.pley.android.common.FormFactor
import app.pley.android.common.Tier
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import java.io.IOException

@RunWith(RobolectricTestRunner::class)
class BuffersTest {
    private val gib = 1L shl 30

    private fun tv(tier: Tier, ram: Long) = DeviceProfile(FormFactor.Tv, fireTv = true, tier = tier, model = "AFTSS", totalRamBytes = ram, lowRam = false)

    @Test
    fun tvsBufferByTierWithNoBackBuffer() {
        val mib = BufferPlan.MIB
        assertEquals(BufferPlan(48 * mib, 15_000, 30_000, 0), BufferPlan.of(tv(Tier.T0, 900L * 1024 * 1024)))
        assertEquals(BufferPlan(64 * mib, 15_000, 30_000, 0), BufferPlan.of(tv(Tier.T0, 14 * gib / 10)))
        assertEquals(BufferPlan(96 * mib, 50_000, 50_000, 0), BufferPlan.of(tv(Tier.T1, 2 * gib)))
        assertEquals(BufferPlan(120 * mib, 50_000, 50_000, 0), BufferPlan.of(tv(Tier.T2, 3 * gib)))
    }

    @Test
    fun phonesBuffer15To50SecondsWith10Behind() {
        val phone = DeviceProfile(FormFactor.Phone, fireTv = false, tier = Tier.T2, model = "SM-N986W", totalRamBytes = 12 * gib, lowRam = false)
        assertEquals(BufferPlan(C.LENGTH_UNSET, 15_000, 50_000, 10_000), BufferPlan.of(phone))
    }

    @Test
    fun musicBuffersMinutesAheadWithinFewerBytesOnEveryDevice() {
        val mib = BufferPlan.MIB
        val phone = DeviceProfile(FormFactor.Phone, fireTv = false, tier = Tier.T2, model = "SM-N986W", totalRamBytes = 12 * gib, lowRam = false)
        val lowRam = phone.copy(tier = Tier.T0, totalRamBytes = 2 * gib, lowRam = true)
        assertEquals(BufferPlan(32 * mib, 50_000, 600_000, 10_000), BufferPlan.of(phone, music = true))
        assertEquals(BufferPlan(16 * mib, 50_000, 600_000, 10_000), BufferPlan.of(lowRam, music = true))
        assertEquals(BufferPlan(32 * mib, 50_000, 600_000, 10_000), BufferPlan.of(tv(Tier.T2, 3 * gib), music = true))
        assertEquals(BufferPlan(16 * mib, 50_000, 600_000, 10_000), BufferPlan.of(tv(Tier.T0, 900L * 1024 * 1024), music = true))
        // Video keeps the device's.
        assertEquals(BufferPlan.PHONE, BufferPlan.of(phone, music = false))
        assertEquals(BufferPlan(120 * mib, 50_000, 50_000, 0), BufferPlan.of(tv(Tier.T2, 3 * gib), music = false))
    }

    @Test
    fun playbackStartsSoonerFromHome() {
        val plan = BufferPlan.PHONE
        assertEquals(500, plan.startMs(home = true, rebuffering = false))
        assertEquals(1_000, plan.startMs(home = false, rebuffering = false))
        assertEquals(1_500, plan.startMs(home = true, rebuffering = true))
        assertEquals(2_000, plan.startMs(home = false, rebuffering = true))
    }

    @Test
    fun homeServersAreThoseOnTheLocalNetwork() {
        assertTrue(BufferPlan.isHome("http://192.168.50.10:7539/api/files/1/stream"))
        assertTrue(BufferPlan.isHome("http://10.0.0.5:7539/x"))
        assertTrue(BufferPlan.isHome("http://nas.local:7539/x"))
        assertTrue(BufferPlan.isHome("http://nas:7539/x"))
        assertFalse(BufferPlan.isHome("https://pley.example.com/x"))
        assertFalse("Tailscale crosses the internet", BufferPlan.isHome("http://nas.tail1234.ts.net:7539/x"))
        assertFalse(BufferPlan.isHome("http://100.101.102.103:7539/x"))
        assertFalse(BufferPlan.isHome("not a url"))
    }

    @Test
    fun awayFromHomePlaybackWaitsALittleLonger() {
        val control = PleyLoadControl(BufferPlan.of(tv(Tier.T2, 3 * gib)))
        val player = PlayerId.UNSET
        control.onPrepared(player)
        val timeline = FakeTimeline()
        val period = MediaSource.MediaPeriodId(timeline.getUidOfPeriod(0))
        fun params(bufferedMs: Long, rebuffering: Boolean) = LoadControl.Parameters(
            player, timeline, period, 0, bufferedMs * 1000, 1f, true, rebuffering, C.TIME_UNSET, C.TIME_UNSET,
        )
        control.home = true
        assertTrue(control.shouldStartPlayback(params(600, rebuffering = false)))
        assertFalse(control.shouldStartPlayback(params(400, rebuffering = false)))
        assertTrue(control.shouldStartPlayback(params(1_600, rebuffering = true)))
        control.home = false
        assertFalse(control.shouldStartPlayback(params(600, rebuffering = false)))
        assertTrue(control.shouldStartPlayback(params(1_100, rebuffering = false)))
        assertFalse(control.shouldStartPlayback(params(1_600, rebuffering = true)))
        assertTrue(control.shouldStartPlayback(params(2_100, rebuffering = true)))
        assertEquals(0L, control.getBackBufferDurationUs(player))

        // Live starts on the small thresholds wherever the server is.
        control.live = true
        assertTrue(control.shouldStartPlayback(params(600, rebuffering = false)))
        assertFalse(control.shouldStartPlayback(params(400, rebuffering = false)))
        assertTrue(control.shouldStartPlayback(params(1_600, rebuffering = true)))
        assertFalse(control.shouldStartPlayback(params(1_400, rebuffering = true)))
    }

    private fun loadError(error: IOException, count: Int) = LoadErrorHandlingPolicy.LoadErrorInfo(
        LoadEventInfo(0, DataSpec.Builder().setUri("http://nas:7539/a").build(), 0),
        MediaLoadData(C.DATA_TYPE_MEDIA),
        error,
        count,
    )

    private fun http(code: Int) = HttpDataSource.InvalidResponseCodeException(
        code,
        null,
        null,
        emptyMap(),
        DataSpec.Builder().setUri("http://nas:7539/a").build(),
        ByteArray(0),
    )

    @Test
    fun goneSessionsAreNotRetried() {
        val retries = MediaRetries()
        assertEquals(C.TIME_UNSET, retries.getRetryDelayMsFor(loadError(http(404), 1)))
        assertEquals(C.TIME_UNSET, retries.getRetryDelayMsFor(loadError(http(410), 1)))
        assertEquals(C.TIME_UNSET, retries.getRetryDelayMsFor(loadError(IOException(http(401)), 1)))
        val options = LoadErrorHandlingPolicy.FallbackOptions(1, 0, 2, 0)
        assertEquals(null, retries.getFallbackSelectionFor(options, loadError(http(404), 1)))
    }

    @Test
    fun errorAnswersAreRetriedFiveTimesWithBackoff() {
        val retries = MediaRetries()
        val delays = (1..6).map { retries.getRetryDelayMsFor(loadError(http(500), it)) }
        assertEquals(listOf(500L, 1_000L, 2_000L, 4_000L, 8_000L, C.TIME_UNSET), delays)
        // The retries decide when a load gives up: Media3 doesn't stop it after a count of its own.
        assertEquals(Int.MAX_VALUE, retries.getMinimumLoadableRetryCount(C.DATA_TYPE_MEDIA))
    }
}
