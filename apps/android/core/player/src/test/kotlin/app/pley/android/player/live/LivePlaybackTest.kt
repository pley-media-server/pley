package app.pley.android.player.live

import android.os.Looper
import androidx.media3.common.MimeTypes
import androidx.media3.common.PlaybackException
import androidx.media3.common.Player
import androidx.media3.common.util.UnstableApi
import androidx.media3.datasource.DataSpec
import androidx.media3.datasource.HttpDataSource
import app.pley.android.caps.CapsSnapshot
import app.pley.android.common.UiText
import app.pley.android.model.BusyConnection
import app.pley.android.model.Control
import app.pley.android.model.LivePlan
import app.pley.android.model.ServerEvent
import app.pley.android.model.caps.ClientCaps
import app.pley.android.model.caps.VideoDecoderCaps
import app.pley.android.network.ApiException
import app.pley.android.testing.Fixtures
import app.pley.android.network.OfflineException
import app.pley.android.player.inEnglish
import app.pley.android.testing.data.FakeLiveTv
import app.pley.android.testing.player.engine.FakePlayerEngine
import java.io.IOException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.launch
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
    private val events = MutableSharedFlow<ServerEvent>(extraBufferCapacity = 8)
    /** What the playback told the app of its session, in order (LiveUpdates.playing). */
    private val held = mutableListOf<Boolean>()

    /** The sessions it said it watches, in order (RemoteRepository.here). */
    private val here = mutableListOf<String?>()
    private val playback = LivePlayback(
        live,
        { FakePlayerEngine().also { engines += it } },
        { "android-1" },
        scope,
        events = events,
        playing = { _, playing -> held += playing },
        here = { _, id -> here += id },
        now = { scheduler.currentTime },
    )

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

    /** Channel 1, sent as it is, playing from its `stream.ts`. */
    private fun playStream() {
        live.plan = Fixtures.decode<LivePlan>("live/plan.json")
        playback.tune(1)
        settle()
        engine().player.ready()
        settle()
    }

    /** [seconds] go by, a look of the watch in each: the stream comes or not, and plays on or stands where it is. */
    private fun pass(seconds: Int, coming: Boolean, playing: Boolean = false) = repeat(seconds) {
        if (coming) engine().receive()
        if (playing) engine().player.advanceTo(engine().player.currentPosition + 1_000)
        scheduler.advanceTimeBy(LivePlayback.STUCK_LOOK_MS)
        settle()
    }

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
    fun `sound the server converts plays from stream ts where this device decodes it or sends it on as it is`() {
        var caps: CapsSnapshot? = CapsSnapshot(ClientCaps(audioCodecs = listOf("aac", "ac3")), emptyList())
        val playback = LivePlayback(live, { FakePlayerEngine().also { engines += it } }, { "android-1" }, scope, caps = { caps })
        live.plan = Fixtures.decode<LivePlan>("live/plan.json").copy(audio = "aac", audioCodec = "ac3", audioChannels = 6)
        playback.tune(1)
        settle()
        // The stream.ts read as the channel was asked for plays: no HLS.
        assertEquals(listOf(false), engine().prepared.map { it.source.hls })
        assertTrue(engine().player.playWhenReady)

        // Not where it neither decodes nor passes it, nor before the device is probed: the HLS made for every player.
        caps = CapsSnapshot(ClientCaps(audioCodecs = listOf("aac")), emptyList())
        playback.tune(2)
        settle()
        assertTrue(engine().prepared.last().source.hls)
        caps = null
        playback.tune(1)
        settle()
        assertTrue(engine().prepared.last().source.hls)
        playback.close()
    }

    @Test
    fun `a plan plays as sent with its picture as it is, and its sound as it is or one the device plays`() {
        val passes = CapsSnapshot(ClientCaps(audioCodecs = listOf("aac"), audioPassthrough = listOf("eac3")), emptyList())
        assertTrue(LivePlayback.playsAsSent(LivePlan(video = "copy", audio = "aac", audioCodec = "eac3"), passes))
        assertTrue(LivePlayback.playsAsSent(LivePlan(video = "copy", audio = "none"), null))
        assertFalse(LivePlayback.playsAsSent(LivePlan(video = "h264_vaapi", audio = "aac", audioCodec = "eac3"), passes))
        // DTS isn't one the server's plan converts for some players only: it goes to HLS everywhere.
        val dts = CapsSnapshot(ClientCaps(audioCodecs = listOf("aac", "dts")), emptyList())
        assertFalse(LivePlayback.playsAsSent(LivePlan(video = "copy", audio = "aac", audioCodec = "dts"), dts))
        assertFalse(LivePlayback.playsAsSent(LivePlan(video = "copy", audio = "aac", audioCodec = "mp2"), passes))
    }

    /** A device with [decoders] that decodes, or sends on as it is, [sounds]. */
    private fun device(vararg decoders: VideoDecoderCaps, sounds: List<String> = listOf("aac", "eac3")) =
        CapsSnapshot(ClientCaps(videoCodecs = decoders.map { it.codec }.distinct(), audioCodecs = sounds, video = decoders.toList()), emptyList())

    /** A hardware HEVC decoder with these limits. */
    private fun hevc(maxBitDepth: Int? = 10, maxWidth: Int? = 3840, maxHeight: Int? = 2160, maxFps: Double? = 60.0) =
        VideoDecoderCaps("hevc", hardware = true, maxWidth = maxWidth, maxHeight = maxHeight, maxFps = maxFps, maxBitDepth = maxBitDepth)

    @Test
    fun `with Direct Play, a channel plays as sent where a decoder takes its video and the device its sound`() {
        // HEVC, 10-bit, 3840×2160 at 50, with E-AC3: the server would make H.264 and AAC of it.
        val plan = Fixtures.decode<LivePlan>("live/plan_hevc.json")
        val tv = device(hevc())
        assertNull(LivePlayback.notAsSent(plan, tv))
        assertTrue(LivePlayback.playsAsSent(plan, tv))

        // An 8-bit decoder takes 8-bit HEVC, and a plan that doesn't say its depth, not 10-bit.
        val eightBit = device(hevc(maxBitDepth = 8))
        assertEquals(NotAsSent.Video, LivePlayback.notAsSent(plan, eightBit))
        assertNull(LivePlayback.notAsSent(plan.copy(bitDepth = 8), eightBit))
        assertNull(LivePlayback.notAsSent(plan.copy(bitDepth = null), eightBit))
        // No decoder for it, or one it is too big or too fast for.
        assertEquals(NotAsSent.Video, LivePlayback.notAsSent(plan, device(VideoDecoderCaps("h264", hardware = true, maxBitDepth = 8))))
        assertEquals(NotAsSent.Video, LivePlayback.notAsSent(plan, device(hevc(maxWidth = 1920, maxHeight = 1080))))
        assertEquals(NotAsSent.Video, LivePlayback.notAsSent(plan, device(hevc(maxFps = 30.0))))
        // One of two entries takes it: 2160p at 30, 1080p at 60.
        assertNull(LivePlayback.notAsSent(plan.copy(width = 1920, height = 1080), device(hevc(maxFps = 30.0), hevc(maxWidth = 1920, maxHeight = 1080))))
        // A decoder that gives no limits (a quirk's), and a report that lists codecs alone, which says no
        // bit depth: 8 bits.
        assertNull(LivePlayback.notAsSent(plan, device(VideoDecoderCaps("hevc", hardware = true))))
        val named = CapsSnapshot(ClientCaps(videoCodecs = listOf("hevc"), audioCodecs = listOf("eac3")), emptyList())
        assertEquals(NotAsSent.Video, LivePlayback.notAsSent(plan, named))
        assertNull(LivePlayback.notAsSent(plan.copy(bitDepth = 8), named))

        // Sound it neither decodes nor sends on: the server's, though the video would play. Sent to a receiver as it is, it plays.
        assertEquals(NotAsSent.Audio, LivePlayback.notAsSent(plan, device(hevc(), sounds = listOf("aac"))))
        assertNull(LivePlayback.notAsSent(plan, CapsSnapshot(ClientCaps(audioCodecs = listOf("aac"), video = listOf(hevc()), audioPassthrough = listOf("eac3")), emptyList())))
        // Either alone: the video copied, the sound decoded here; a radio channel's sound.
        assertNull(LivePlayback.notAsSent(plan.copy(video = "copy", audioCodec = "mp3"), device(sounds = listOf("mp3"))))
        assertNull(LivePlayback.notAsSent(plan.copy(video = "none", audioCodec = "mp3"), device(sounds = listOf("mp3"))))

        // Interlaced video the server converts stays the server's; copied (its deinterlacing off), it is as sent.
        assertEquals(NotAsSent.Interlaced, LivePlayback.notAsSent(plan.copy(interlaced = true), tv))
        assertNull(LivePlayback.notAsSent(plan.copy(interlaced = true, video = "copy"), tv))
        // Deinterlace off, the server would send it interlaced all the same: as sent.
        assertNull(LivePlayback.notAsSent(plan.copy(interlaced = true, deinterlace = false), tv))
        // Off on the server, nothing more plays as sent than did.
        assertEquals(NotAsSent.Off, LivePlayback.notAsSent(plan.copy(directPlay = false), tv))
        assertFalse(LivePlayback.playsAsSent(plan.copy(directPlay = false), tv))
        // Not before the device is probed.
        assertEquals(NotAsSent.Unknown, LivePlayback.notAsSent(plan, null))
        // Nor what Media3 doesn't read from MPEG-TS, decoded here or not, nor a codec the plan doesn't name.
        assertEquals(NotAsSent.Unknown, LivePlayback.notAsSent(plan.copy(videoCodec = "av1"), device(VideoDecoderCaps("av1", hardware = true))))
        assertEquals(NotAsSent.Video, LivePlayback.notAsSent(plan.copy(videoCodec = "av1"), tv))
        assertEquals(NotAsSent.Unknown, LivePlayback.notAsSent(plan.copy(audioCodec = "dts"), device(hevc(), sounds = listOf("dts"))))
        assertEquals(NotAsSent.Audio, LivePlayback.notAsSent(plan.copy(audioCodec = "dts"), tv))
        assertEquals(NotAsSent.Unknown, LivePlayback.notAsSent(plan.copy(videoCodec = null), tv))
        assertEquals(NotAsSent.Unknown, LivePlayback.notAsSent(plan.copy(audioCodec = null), tv))
    }

    @Test
    fun `an older server's plan says nothing new, and its channels play as they did`() {
        val plan = Fixtures.decode<LivePlan>("live/plan.json")
        assertNull(plan.directPlay)
        assertEquals(0L, plan.kbps)
        assertNull(plan.bitDepth)
        assertNull(plan.outHeight)
        assertNull(plan.videoKbps)
        val tv = device(hevc())
        // Sent as it is, and with sound the server converts that this device plays: as before.
        assertTrue(LivePlayback.playsAsSent(plan, null))
        assertTrue(LivePlayback.playsAsSent(plan.copy(audio = "aac", audioCodec = "eac3"), tv))
        // HEVC it would decode stays the server's to convert, with no word of a setting that server lacks.
        assertEquals(NotAsSent.Unknown, LivePlayback.notAsSent(plan.copy(videoCodec = "hevc", video = "libx264"), tv))
    }

    /** How [playback] says the channel plays, once the server answered. */
    private fun details(playback: LivePlayback): LivePlaying? {
        var said: LivePlaying? = null
        scope.launch { said = playback.details() }
        settle()
        return said
    }

    @Test
    fun `a channel this device decodes plays its stream ts with no HLS asked for, and its details say how it plays`() {
        var caps = device(hevc())
        val playback = LivePlayback(live, { FakePlayerEngine().also { engines += it } }, { "android-1" }, scope, caps = { caps })
        val plan = Fixtures.decode<LivePlan>("live/plan_hevc.json")
        live.plan = plan
        playback.tune(1)
        settle()
        // The stream.ts read as the channel was asked for plays: the server makes no transcode.
        assertEquals(listOf(false), engine().prepared.map { it.source.hls })
        assertTrue(engine().player.playWhenReady)
        // Nothing to say while it tunes.
        assertNull(details(playback))
        engine().player.ready()
        settle()
        assertEquals(LivePlaying(plan, asSent = true), details(playback))
        // Asked again, the plan says what the provider sends now; one that doesn't come leaves the last.
        live.plan = plan.copy(kbps = 9_000)
        assertEquals(9_000L, details(playback)?.plan?.kbps)
        live.plan = null
        assertEquals(9_000L, details(playback)?.plan?.kbps)
        assertEquals(listOf("live-1", "live-1", "live-1", "live-1"), live.plansAsked)

        // It fails: the HLS, which the plan didn't want, so the details give no reason.
        live.plan = plan
        engine().player.fail(PlaybackException("no", null, PlaybackException.ERROR_CODE_DECODING_FAILED))
        settle()
        assertTrue(engine().prepared.last().source.hls)
        engine().player.ready()
        settle()
        assertEquals(LivePlaying(plan, asSent = false), details(playback))

        // On a device without the decoder, the HLS from the start, and why.
        caps = device(VideoDecoderCaps("h264", hardware = true, maxBitDepth = 8))
        playback.tune(2)
        settle()
        assertTrue(engine().prepared.last().source.hls)
        engine().player.ready()
        settle()
        assertEquals(LivePlaying(plan, asSent = false, why = NotAsSent.Video), details(playback))

        // Let go, there is nothing to say, and nothing is asked.
        playback.stop()
        val asked = live.plansAsked.size
        assertNull(details(playback))
        assertEquals(asked, live.plansAsked.size)
        playback.close()
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
        assertEquals(message, failed.trouble.text.inEnglish())
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
        assertEquals("Bob started watching Film Four on this connection.", LiveTrouble.of(ApiException(410, "gone", "Bob started watching Film Four on this connection.")).text.inEnglish())
    }

    @Test
    fun `a stream ts refused before the watch answers waits for the watch, asking for the channel once`() {
        val message = "The one connection of Test Provider is in use, watching Film Four."
        live.watchFails = ApiException(409, "conflict", message)
        live.watchTakesMs = 500
        playback.tune(1)
        settle()
        val busy = HttpDataSource.InvalidResponseCodeException(409, "Conflict", null, emptyMap(), DataSpec.Builder().setUri("http://x").build(), ByteArray(0))
        engine().player.fail(PlaybackException("busy", busy, PlaybackException.ERROR_CODE_IO_BAD_HTTP_STATUS))
        settle()
        assertEquals(LivePhase.Tuning, playback.state.value.phase)
        scheduler.advanceTimeBy(600)
        settle()
        // The watch's own answer says why: the channel was asked for once.
        assertEquals(listOf("watch 1 android-1"), live.sent)
        val failed = playback.state.value.phase as LivePhase.Failed
        assertEquals(LiveTrouble.Kind.Busy, failed.trouble.kind)
        assertEquals(message, failed.trouble.text.inEnglish())
    }

    @Test
    fun `a stream ts that fails before the watch answers plays the HLS of that watch`() {
        live.plan = Fixtures.decode<LivePlan>("live/plan.json")
        live.watchTakesMs = 500
        playback.tune(1)
        settle()
        engine().player.fail(PlaybackException("no", null, PlaybackException.ERROR_CODE_PARSING_CONTAINER_UNSUPPORTED))
        settle()
        scheduler.advanceTimeBy(600)
        settle()
        assertEquals(listOf("watch 1 android-1"), live.sent)
        val hls = engine().prepared.last()
        assertTrue(hls.source.hls && hls.source.url.endsWith("/api/live/sessions/live-1/index.m3u8") && hls.playWhenReady)
        assertEquals(LivePhase.Tuning, playback.state.value.phase)
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
        assertEquals(why, failed.trouble.text.inEnglish())
        // Gone at the server already: not left, not watched again.
        assertEquals(listOf("watch 1 android-1"), live.sent)
    }

    @Test
    fun `a stream ts that stands still while it comes is read again, on the same session`() {
        playStream()
        pass(10, coming = true, playing = true)
        // The provider's clock started over: nothing more plays or is buffered, and the stream comes on.
        pass(LivePlayback.STUCK_AFTER - 1, coming = true)
        assertEquals(1, engine().prepared.size)
        assertEquals(listOf("live-1"), live.plansAsked)
        pass(1, coming = true)

        // Its plan asked for, so the session counts as seen, then the same stream.ts, playing at once.
        assertEquals(listOf("live-1", "live-1"), live.plansAsked)
        assertEquals(listOf("watch 1 android-1"), live.sent)
        assertEquals(2, engine().prepared.size)
        val again = engine().prepared.last()
        assertEquals("http://127.0.0.1:7539/api/live/channels/1/stream.ts?client_id=android-1-stream", again.source.url)
        assertTrue(!again.source.hls && again.source.live && again.source.mimeType == MimeTypes.VIDEO_MP2T && again.playWhenReady)
        // The spinner stays until it plays.
        assertEquals(LivePhase.Playing, playback.state.value.phase)
        assertTrue(playback.state.value.buffering)
        engine().player.ready()
        settle()
        assertFalse(playback.state.value.buffering)

        // It plays on, watched as before: nothing more is asked.
        pass(30, coming = true, playing = true)
        assertEquals(2, engine().prepared.size)
        assertEquals(listOf("watch 1 android-1"), live.sent)
    }

    @Test
    fun `a stream ts that stands still again soon after it was read again is watched as HLS`() {
        playStream()
        pass(10, coming = true, playing = true)
        pass(LivePlayback.STUCK_AFTER, coming = true)
        engine().player.ready()
        settle()
        // A provider whose clock starts over at every break: the transcoder's segments take the jump.
        pass(60, coming = true, playing = true)
        pass(LivePlayback.STUCK_AFTER, coming = true)
        assertEquals(listOf("watch 1 android-1", "leave live-1", "watch 1 android-1"), live.sent)
        assertEquals(listOf(false, false, true), engine().prepared.map { it.source.hls })
        assertTrue(engine().prepared.last().playWhenReady)

        // The HLS isn't watched for standing still: Media3 says when it fails.
        pass(30, coming = true)
        assertEquals(3, engine().prepared.size)
    }

    @Test
    fun `a stream ts is read again a second time two minutes on`() {
        playStream()
        pass(10, coming = true, playing = true)
        pass(LivePlayback.STUCK_AFTER, coming = true)
        engine().player.ready()
        settle()
        pass((LivePlayback.READ_AGAIN_ONCE_IN_MS / 1_000).toInt(), coming = true, playing = true)
        pass(LivePlayback.STUCK_AFTER, coming = true)
        assertEquals(listOf(false, false, false), engine().prepared.map { it.source.hls })
        assertEquals(listOf("watch 1 android-1"), live.sent)

        // Tuned anew, the channel counts from nothing.
        playback.tune(1)
        settle()
        engine().player.ready()
        settle()
        pass(10, coming = true, playing = true)
        pass(LivePlayback.STUCK_AFTER, coming = true)
        assertEquals(listOf(false, false, false, false, false), engine().prepared.map { it.source.hls })
    }

    @Test
    fun `a stream ts that stands still without ever playing is watched as HLS`() {
        playStream()
        // One look further is where it started, not playing.
        pass(1, coming = true, playing = true)
        pass(LivePlayback.STUCK_AFTER, coming = true)
        assertEquals(listOf("watch 1 android-1", "leave live-1", "watch 1 android-1"), live.sent)
        assertEquals(listOf(false, true), engine().prepared.map { it.source.hls })
    }

    @Test
    fun `a provider gone quiet isn't read again`() {
        playStream()
        pass(10, coming = true, playing = true)
        // Nothing comes: the stall's spinner, and the wait for the provider.
        pass(60, coming = false)
        assertEquals(1, engine().prepared.size)
        assertEquals(listOf("live-1"), live.plansAsked)

        // Nor one whose stream came on too few of the seconds it stood still.
        pass(1, coming = true, playing = true)
        pass(LivePlayback.STUCK_COMING - 2, coming = true)
        pass(LivePlayback.STUCK_AFTER - LivePlayback.STUCK_COMING + 1, coming = false)
        pass(1, coming = true)
        assertEquals(1, engine().prepared.size)
        // One more of them, and it stands still for good after all.
        pass(1, coming = true)
        assertEquals(2, engine().prepared.size)
    }

    @Test
    fun `a stream ts that came and then ended isn't read again`() {
        playStream()
        pass(10, coming = true, playing = true)
        // Its last seconds come, then nothing: its end is the player's to tell.
        pass(LivePlayback.STUCK_COMING, coming = true)
        pass(60, coming = false)
        assertEquals(1, engine().prepared.size)
        assertEquals(listOf("live-1"), live.plansAsked)
    }

    @Test
    fun `a stream ts that buffers on, or plays on, isn't standing still`() {
        playStream()
        pass(10, coming = true, playing = true)
        repeat(30) { second ->
            engine().player.bufferTo(20_000 + second * 1_000L)
            pass(1, coming = true)
        }
        // Standing still starts over each time it moves.
        repeat(5) {
            pass(LivePlayback.STUCK_AFTER - 1, coming = true)
            pass(1, coming = true, playing = true)
        }
        assertEquals(1, engine().prepared.size)
        assertEquals(listOf("live-1"), live.plansAsked)
    }

    @Test
    fun `a stream ts that stands still on a session the server ended says why`() {
        playStream()
        pass(10, coming = true, playing = true)
        val why = "Bob started watching Film Four on this connection."
        live.planFails = ApiException(410, "session_stopped", why)
        pass(LivePlayback.STUCK_AFTER, coming = true)
        val failed = playback.state.value.phase as LivePhase.Failed
        assertEquals(LiveTrouble.Kind.Stopped, failed.trouble.kind)
        assertEquals(why, failed.trouble.text.inEnglish())
        // Gone at the server already: not left, not read or watched again.
        assertEquals(listOf("watch 1 android-1"), live.sent)
        assertEquals(1, engine().prepared.size)
        live.planFails = null
        pass(30, coming = true)
        assertEquals(1, engine().prepared.size)
    }

    @Test
    fun `a stream ts is read again though its session's plan couldn't be asked for`() {
        playStream()
        pass(10, coming = true, playing = true)
        // Only the server saying the session ended stops it.
        live.planFails = OfflineException(IOException("down"))
        pass(LivePlayback.STUCK_AFTER, coming = true)
        assertEquals(LivePhase.Playing, playback.state.value.phase)
        assertEquals(listOf(false, false), engine().prepared.map { it.source.hls })
        assertEquals(listOf("watch 1 android-1"), live.sent)
    }

    @Test
    fun `a channel changed or left is no longer watched for standing still`() {
        playStream()
        pass(10, coming = true, playing = true)
        pass(LivePlayback.STUCK_AFTER - 1, coming = true)
        // Its HLS, which says by itself when it fails.
        live.plan = null
        playback.tune(2)
        settle()
        engine().player.ready()
        settle()
        pass(30, coming = true)
        assertEquals(listOf(false, false, true), engine().prepared.map { it.source.hls })

        playStream()
        pass(10, coming = true, playing = true)
        playback.stop()
        settle()
        pass(30, coming = true)
        assertEquals(listOf("live-1", "live-2", "live-1"), live.plansAsked)
        assertEquals(1, engines.size)
    }

    @Test
    fun `a connection taken over stops the channel at once and says why, without tuning again`() {
        playback.tune(1)
        settle()
        engine().player.ready()
        settle()

        // Another channel's connection: nothing here.
        events.tryEmit(ServerEvent.LiveTakenOver("other", 2, "Someone else needed this channel's connection to watch another channel."))
        settle()
        assertEquals(LivePhase.Playing, playback.state.value.phase)

        val why = "Sam is watching Film Four now, on the connection this channel used."
        events.tryEmit(ServerEvent.LiveTakenOver("stream-session", 1, why))
        settle()
        val failed = playback.state.value.phase as LivePhase.Failed
        assertEquals(LiveTrouble(LiveTrouble.Kind.Stopped, UiText.Sentence(UiText.raw(why))), failed.trouble)
        assertTrue(failed.trouble.canRetry)

        // The 410 and the stream's end that follow change nothing; gone at the server already: not left.
        live.planFails = ApiException(410, "session_stopped", why)
        engine().player.end()
        events.tryEmit(ServerEvent.LiveTakenOver("live-1", 1, why))
        scheduler.advanceTimeBy(LivePlayback.NEXT_RETRY_MS * 2)
        settle()
        assertEquals(failed, playback.state.value.phase)
        assertEquals(listOf("watch 1 android-1"), live.sent)

        // Watch again tunes it again.
        playback.retry()
        settle()
        assertEquals(2, live.sent.count { it.startsWith("watch 1") })
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
    fun `a connection taken over stays stopped through the background, until watch again`() {
        playback.tune(1)
        settle()
        engine().player.ready()
        settle()
        val why = "Sam is watching Film Four now, on the connection this channel used."
        events.tryEmit(ServerEvent.LiveTakenOver("stream-session", 1, why))
        settle()
        val stopped = playback.state.value.phase

        // The screen saver, the lock screen or Home, then back.
        playback.stop()
        settle()
        assertTrue(engine().released)
        assertNull(playback.engine.value)
        playback.resume()
        settle()
        assertEquals(stopped, playback.state.value.phase)
        assertEquals(LiveTrouble(LiveTrouble.Kind.Stopped, UiText.Sentence(UiText.raw(why))), (playback.state.value.phase as LivePhase.Failed).trouble)
        assertEquals(listOf("watch 1 android-1"), live.sent)

        // Watch again tunes it again.
        playback.retry()
        settle()
        assertEquals(2, live.sent.count { it.startsWith("watch 1") })
        assertEquals(LivePhase.Tuning, playback.state.value.phase)
    }

    @Test
    fun `stopped from elsewhere, it stops and says who and why`() {
        playback.tune(1)
        settle()
        engine().player.ready()
        settle()
        events.tryEmit(ServerEvent.SessionStopped("someone-else", "Bye", byAdmin = true))
        settle()
        assertEquals(LivePhase.Playing, playback.state.value.phase)

        events.tryEmit(ServerEvent.SessionStopped("live-1", "Server maintenance", byAdmin = true))
        settle()
        val trouble = (playback.state.value.phase as LivePhase.Failed).trouble
        assertEquals(LiveTrouble(LiveTrouble.Kind.Stopped, LivePlayback.stoppedText("Server maintenance", byAdmin = true)), trouble)
        assertEquals("Live TV was stopped by the server's administrator.\nServer maintenance", trouble.text.inEnglish())
        assertTrue(trouble.canRetry)
        // Gone at the server already: nothing is let go, and nothing tunes it again by itself.
        assertEquals(listOf("watch 1 android-1"), live.sent)
        assertEquals("Live TV was stopped from another device.", LivePlayback.stoppedText(null, byAdmin = false).inEnglish())
    }

    @Test
    fun `the remote's next and previous change channel, where the player goes through channels`() {
        val zaps = mutableListOf<Int>()
        playback.zap = { by -> zaps += by }
        playback.tune(1)
        settle()
        assertEquals(listOf(listOf(Control.Next, Control.Previous)), live.watchedControls)
        events.tryEmit(ServerEvent.PlayerCommand("live-1", Control.Next))
        events.tryEmit(ServerEvent.PlayerCommand("live-1", Control.Previous))
        // No pausing or going back on Live TV, and another session's are someone else's.
        events.tryEmit(ServerEvent.PlayerCommand("live-1", Control.Pause))
        events.tryEmit(ServerEvent.PlayerCommand("someone-else", Control.Next))
        settle()
        assertEquals(listOf(1, -1), zaps)
    }

    @Test
    fun `without channels to go through, it declares and obeys nothing`() {
        playback.tune(1)
        settle()
        assertEquals(listOf(emptyList<Control>()), live.watchedControls)
        events.tryEmit(ServerEvent.PlayerCommand("live-1", Control.Next))
        settle()
        assertEquals(listOf("watch 1 android-1"), live.sent)
    }

    @Test
    fun `the event stream is held while a session is`() {
        playback.tune(1)
        settle()
        assertEquals(true, held.last())
        playback.stop()
        settle()
        assertEquals(false, held.last())
        playback.resume()
        settle()
        assertEquals(true, held.last())
        playback.close()
        assertEquals(false, held.last())
    }

    @Test
    fun `the remote is told which session watches here`() {
        playback.tune(1)
        settle()
        assertEquals("live-1", here.last())
        playback.stop()
        settle()
        assertNull(here.last())
        playback.close()
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
