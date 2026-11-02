package app.pley.android.player.live

import android.os.Looper
import androidx.media3.common.MimeTypes
import androidx.media3.common.PlaybackException
import androidx.media3.common.Player
import androidx.media3.common.util.UnstableApi
import androidx.media3.datasource.DataSpec
import androidx.media3.datasource.HttpDataSource
import app.pley.android.model.BusyConnection
import app.pley.android.model.LivePlan
import app.pley.android.network.ApiException
import app.pley.android.testing.Fixtures
import app.pley.android.network.OfflineException
import app.pley.android.testing.data.FakeLiveTv
import app.pley.android.testing.player.engine.FakePlayerEngine
import java.io.IOException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.TestCoroutineScheduler
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf

/** Watching a channel: tuning with this device's id, zapping, the server's refusals, tuning again, no pausing, the background. */
@RunWith(RobolectricTestRunner::class)
@androidx.annotation.OptIn(UnstableApi::class)
class LivePlaybackTest {
    private val scheduler = TestCoroutineScheduler()
    private val scope = CoroutineScope(StandardTestDispatcher(scheduler) + SupervisorJob())
    private val live = FakeLiveTv()
    private val engines = mutableListOf<FakePlayerEngine>()
    private val playback = LivePlayback(live, { FakePlayerEngine().also { engines += it } }, { "android-1" }, scope)

    @After
    fun close() {
        playback.close()
        scope.cancel()
    }

    /** Runs what is due, the player's callbacks included. */
    private fun settle() {
        scheduler.runCurrent()
        shadowOf(Looper.getMainLooper()).idle()
        scheduler.runCurrent()
    }

    private fun engine() = engines.single()

    @Test
    fun `a channel is watched with this device's id and plays its HLS from the live edge`() {
        playback.tune(1)
        assertEquals(LivePhase.Tuning, playback.state.value.phase)
        settle()
        assertEquals(listOf("watch 1 android-1"), live.sent)
        // No plan in time: the stream.ts read meanwhile gives way to the HLS.
        val prepared = engine().prepared.last()
        assertEquals("http://127.0.0.1:7539/api/live/sessions/live-1/index.m3u8", prepared.source.url)
        assertTrue(prepared.source.hls && prepared.source.live && prepared.playWhenReady)

        engine().player.ready()
        settle()
        assertEquals(LivePhase.Playing, playback.state.value.phase)
        assertEquals(1L, playback.state.value.channelId)
    }

    @Test
    fun `a channel sent as it is plays its stream ts, and falls back to its HLS when that fails`() {
        live.plan = Fixtures.decode<LivePlan>("live/plan.json")
        playback.tune(1)
        settle()
        assertEquals(listOf("live-1"), live.plansAsked)
        // Read at once, under an id of its own, held for the plan, then played.
        val read = engine().prepared.single()
        assertEquals("http://127.0.0.1:7539/api/live/channels/1/stream.ts?client_id=android-1-stream", read.source.url)
        assertTrue(!read.source.hls && read.source.live && read.source.mimeType == MimeTypes.VIDEO_MP2T && !read.playWhenReady)
        assertTrue(engine().player.playWhenReady)
        engine().player.ready()
        settle()
        assertEquals(LivePhase.Playing, playback.state.value.phase)

        engine().player.fail(PlaybackException("no", null, PlaybackException.ERROR_CODE_PARSING_CONTAINER_UNSUPPORTED))
        settle()
        // The session still on at the server, it broke off: watched again, its HLS played.
        assertEquals(listOf("watch 1 android-1", "leave live-1", "watch 1 android-1"), live.sent)
        assertEquals(listOf("live-1", "live-1"), live.plansAsked)
        val hls = engine().prepared.last().source
        assertTrue(hls.hls && hls.url.endsWith("/api/live/sessions/live-1/index.m3u8"))
        assertEquals(LivePhase.Tuning, playback.state.value.phase)
    }

    @Test
    fun `a channel made into something else, or no plan in time, plays its HLS`() {
        live.plan = Fixtures.decode<LivePlan>("live/plan.json").copy(video = "h264_nvenc", audio = "aac")
        playback.tune(1)
        settle()
        assertEquals(listOf(false, true), engine().prepared.map { it.source.hls })
        assertTrue(engine().prepared.last().playWhenReady)

        live.plan = null
        playback.tune(2)
        settle()
        assertTrue(engine().prepared.last().source.hls)
    }

    @Test
    fun `zapping lets the last channel go and keeps the one engine`() {
        playback.tune(1)
        settle()
        engine().player.ready()
        settle()
        playback.tune(2)
        settle()
        assertEquals(listOf("watch 1 android-1", "leave live-1", "watch 2 android-1"), live.sent)
        assertEquals(1, engines.size)
        assertEquals(2, engine().prepared.count { it.source.hls })
        assertEquals(LivePhase.Tuning, playback.state.value.phase)
    }

    @Test
    fun `every connection in use says which channels hold them, and trying again tunes again`() {
        val message = "The one connection of Test Provider is in use, watching Film Four."
        live.watchFails = ApiException(409, "conflict", message)
        playback.tune(1)
        settle()
        val failed = playback.state.value.phase as LivePhase.Failed
        assertEquals(LiveTrouble.Kind.Busy, failed.trouble.kind)
        assertEquals(message, failed.trouble.message)
        assertTrue(failed.trouble.canRetry)
        // The stream.ts read as the channel was asked for stops; no HLS was asked for.
        assertEquals(Player.STATE_IDLE, engine().player.playbackState)
        assertEquals(0, engine().prepared.count { it.source.hls })

        live.watchFails = null
        playback.retry()
        settle()
        assertEquals(listOf("watch 1 android-1", "watch 1 android-1"), live.sent)
        assertEquals(1, engine().prepared.count { it.source.hls })
    }

    @Test
    fun `a connection the server says may be taken is taken over by watching again with it`() {
        val held = BusyConnection(channelId = 2, channelName = "Sport 1080i", people = 1, yours = true, canTakeOver = true)
        live.watchFails = ApiException(409, "conflict", "In use.", connections = listOf(held))
        playback.tune(1)
        settle()
        val failed = playback.state.value.phase as LivePhase.Failed
        assertEquals(listOf(held), failed.trouble.connections)

        live.watchFails = null
        playback.retry(takeOver = 2)
        settle()
        assertEquals("watch 1 android-1 taking 2", live.sent.last())
        // Taken over from here later, the server's sentence says who took it, as it is.
        assertEquals("Bob started watching Film Four on this connection.", LiveTrouble.of(ApiException(410, "gone", "Bob started watching Film Four on this connection.")).message)
    }

    @Test
    fun `stream ts is read as the channel is asked for, and after the answer when a connection is taken over`() {
        live.watchFails = ApiException(409, "conflict", "In use.")
        playback.tune(1)
        settle()
        // Read before the server refused the channel: the server tunes for whichever comes first.
        assertEquals(listOf("http://127.0.0.1:7539/api/live/channels/1/stream.ts?client_id=android-1-stream"), engine().prepared.map { it.source.url })
        playback.retry(takeOver = 2)
        settle()
        // Only the watch takes a connection over: nothing more read before it answered.
        assertEquals(1, engine().prepared.size)
        assertEquals("watch 1 android-1 taking 2", live.sent.last())
    }

    @Test
    fun `a stream ts that ends because its connection was taken says who took it`() {
        live.plan = Fixtures.decode<LivePlan>("live/plan.json")
        playback.tune(1)
        settle()
        engine().player.ready()
        settle()
        val why = "Bob started watching Film Four on this connection."
        live.planFails = ApiException(410, "session_stopped", why)
        engine().player.end()
        settle()
        val failed = playback.state.value.phase as LivePhase.Failed
        assertEquals(LiveTrouble.Kind.Stopped, failed.trouble.kind)
        assertEquals(why, failed.trouble.message)
        // Gone at the server already: not left, not watched again.
        assertEquals(listOf("watch 1 android-1"), live.sent)
    }

    @Test
    fun `a stall shows only once it has lasted`() {
        playback.tune(1)
        settle()
        engine().player.ready()
        settle()
        engine().player.stall()
        settle()
        scheduler.advanceTimeBy(LivePlayback.STALL_SHOWN_MS / 2)
        settle()
        assertFalse(playback.state.value.buffering)
        // Over before it showed: never shown.
        engine().player.ready()
        settle()
        scheduler.advanceTimeBy(LivePlayback.STALL_SHOWN_MS)
        settle()
        assertFalse(playback.state.value.buffering)

        engine().player.stall()
        settle()
        scheduler.advanceTimeBy(LivePlayback.STALL_SHOWN_MS + 1)
        settle()
        assertTrue(playback.state.value.buffering)
        engine().player.ready()
        settle()
        assertFalse(playback.state.value.buffering)
    }

    @Test
    fun `a player failure tunes the channel again twice, after 1 s and 3 s, before it says why`() {
        playback.tune(1)
        settle()
        engine().player.ready()
        settle()
        val broke = PlaybackException("broke", null, PlaybackException.ERROR_CODE_DECODING_FAILED)
        engine().player.fail(broke)
        settle()
        // The server may have let the channel go, or the network blinked: tuning, not failed.
        assertEquals(LivePhase.Tuning, playback.state.value.phase)
        scheduler.advanceTimeBy(LivePlayback.FIRST_RETRY_MS - 1)
        settle()
        assertEquals(1, live.sent.count { it.startsWith("watch") })
        scheduler.advanceTimeBy(2)
        settle()
        assertEquals(listOf("watch 1 android-1", "leave live-1", "watch 1 android-1"), live.sent)
        engine().player.fail(broke)
        settle()
        scheduler.advanceTimeBy(LivePlayback.NEXT_RETRY_MS - 1)
        settle()
        assertEquals(2, live.sent.count { it.startsWith("watch") })
        scheduler.advanceTimeBy(2)
        settle()
        assertEquals(3, live.sent.count { it.startsWith("watch") })
        engine().player.fail(broke)
        settle()
        assertEquals(LiveTrouble.Kind.Other, (playback.state.value.phase as LivePhase.Failed).trouble.kind)

        // Try again starts the count again.
        playback.retry()
        settle()
        engine().player.fail(broke)
        settle()
        assertEquals(LivePhase.Tuning, playback.state.value.phase)
    }

    @Test
    fun `a channel the server stopped says so at once`() {
        playback.tune(1)
        settle()
        engine().player.ready()
        settle()
        val stopped = HttpDataSource.InvalidResponseCodeException(410, "Gone", null, emptyMap(), DataSpec.Builder().setUri("http://x").build(), ByteArray(0))
        engine().player.fail(PlaybackException("gone", stopped, PlaybackException.ERROR_CODE_IO_BAD_HTTP_STATUS))
        settle()
        assertEquals(LiveTrouble.Kind.Stopped, (playback.state.value.phase as LivePhase.Failed).trouble.kind)
        scheduler.advanceTimeBy(LivePlayback.NEXT_RETRY_MS * 2)
        settle()
        assertEquals(1, live.sent.count { it.startsWith("watch") })
    }

    @Test
    fun `refusals and failures are told apart`() {
        assertEquals(LiveTrouble.Kind.Missing, LiveTrouble.of(ApiException(404, "not_found", "gone")).kind)
        assertFalse(LiveTrouble.of(ApiException(404, "not_found", "gone")).canRetry)
        assertEquals(LiveTrouble.Kind.Stopped, LiveTrouble.of(ApiException(410, "gone", "stopped")).kind)
        assertEquals(LiveTrouble.Kind.NotAllowed, LiveTrouble.of(ApiException(403, "forbidden", "no")).kind)
        assertEquals(LiveTrouble.Kind.Unavailable, LiveTrouble.of(ApiException(503, "unavailable", "provider")).kind)
        assertEquals(LiveTrouble.Kind.Offline, LiveTrouble.of(OfflineException(IOException("down"))).kind)
    }

    @Test
    fun `live has no pause, so a channel paused by something else plays on`() {
        playback.tune(1)
        settle()
        engine().player.ready()
        settle()
        // Headphones pulled out, a stray media key.
        engine().player.pause()
        settle()
        assertTrue(engine().player.playWhenReady)
    }

    @Test
    fun `the background lets the connection and the decoders go, and coming back tunes again`() {
        playback.tune(3)
        settle()
        engine().player.ready()
        settle()
        playback.stop()
        settle()
        assertTrue(engine().released)
        assertNull(playback.engine.value)
        assertEquals(LivePhase.Idle, playback.state.value.phase)
        assertEquals(3L, playback.state.value.channelId)
        assertEquals("leave live-3", live.sent.last())

        playback.resume()
        settle()
        assertEquals("watch 3 android-1", live.sent.last())
        assertEquals(2, engines.size)
    }

    @Test
    fun `a closed playback tunes nothing more`() {
        playback.tune(1)
        settle()
        playback.close()
        playback.tune(2)
        settle()
        assertEquals(listOf("watch 1 android-1", "leave live-1"), live.sent)
    }
}
