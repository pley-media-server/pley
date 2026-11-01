package app.pley.android.player.engine

import android.app.Activity
import android.content.Context
import android.content.Intent
import android.graphics.SurfaceTexture
import android.media.AudioManager
import android.os.Looper
import android.os.SystemClock
import android.view.Surface
import androidx.media3.common.Player
import androidx.media3.test.utils.FakeClock
import androidx.media3.test.utils.robolectric.RobolectricUtil
import androidx.media3.test.utils.robolectric.ShadowMediaCodecConfig
import androidx.media3.test.utils.robolectric.TestPlayerRunHelper
import androidx.test.core.app.ApplicationProvider
import app.pley.android.caps.CapsDetails
import app.pley.android.caps.CapsSnapshot
import app.pley.android.caps.DecoderPolicy
import app.pley.android.caps.DisplayModeInfo
import app.pley.android.caps.DisplayState
import app.pley.android.caps.MatchContent
import app.pley.android.common.DeviceProfile
import app.pley.android.common.FormFactor
import app.pley.android.common.Tier
import app.pley.android.model.DynamicRange
import app.pley.android.model.MediaStream
import app.pley.android.model.StreamKind
import app.pley.android.model.caps.ClientCaps
import app.pley.android.model.caps.DolbyVisionCaps
import app.pley.android.player.EngineSource
import mockwebserver3.Dispatcher
import mockwebserver3.MockResponse
import mockwebserver3.MockWebServer
import mockwebserver3.RecordedRequest
import mockwebserver3.SocketEffect
import okhttp3.Headers
import okhttp3.OkHttpClient
import okio.Buffer
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import java.io.File
import java.time.Duration
import java.util.concurrent.atomic.AtomicInteger
import kotlinx.coroutines.runBlocking

@RunWith(RobolectricTestRunner::class)
class Media3EngineTest {
    @get:Rule
    val codecs: ShadowMediaCodecConfig = ShadowMediaCodecConfig.withNoDefaultSupportedCodecs()

    private val context: Context = ApplicationProvider.getApplicationContext()
    private val engines = mutableListOf<Media3Engine>()

    private val phone = DeviceProfile(FormFactor.Phone, fireTv = false, tier = Tier.T2, model = "Pixel 7a", totalRamBytes = 8L shl 30, lowRam = false)
    private val tv = DeviceProfile(FormFactor.Tv, fireTv = false, tier = Tier.T2, model = "Google TV Streamer", totalRamBytes = 4L shl 30, lowRam = false)

    /** The fixture server's 20 s clip: H.264 High 1080p24 8-bit SDR, AAC stereo. */
    private val clip: ByteArray by lazy {
        javaClass.classLoader!!.getResourceAsStream("app/pley/android/fixtures/media/sample-1080p.mp4")!!.use { it.readBytes() }
    }

    private class MemoryFrameRate(var timeouts: Int = 0) : FrameRateMemory {
        val turnedOff = mutableMapOf<String, Set<Int>>()

        override suspend fun timeouts(displayKey: String) = timeouts

        override suspend fun setTimeouts(displayKey: String, count: Int) = Unit

        override suspend fun turnedOffHdrTypes(displayKey: String) = turnedOff[displayKey].orEmpty()

        override suspend fun setTurnedOffHdrTypes(displayKey: String, types: Set<Int>) {
            turnedOff[displayKey] = types
        }

        override suspend fun takeHint() = false
    }

    /** A display that switches at once, then shows the mode's HDR types, less those [turnedOff] in the system's settings. */
    private class FakeHost(var display: DisplayState, private val turnedOff: Set<Int> = emptySet()) : DisplayModeHost {
        val requested = mutableListOf<Int>()

        override val displayKey = "0:HDMI"

        override fun read() = display

        override fun request(modeId: Int) {
            requested += modeId
        }

        override suspend fun awaitMode(modeId: Int, timeoutMs: Long): Boolean {
            val shows = display.modes.firstOrNull { it.id == modeId }?.hdrTypes?.let { it - turnedOff }
            display = display.copy(activeModeId = modeId, hdrCapabilities = shows ?: display.hdrCapabilities)
            return true
        }
    }

    private fun snapshot(policy: DecoderPolicy) = CapsSnapshot(ClientCaps(), emptyList(), CapsDetails(policy = policy))

    private fun engine(
        profile: DeviceProfile = phone,
        caps: CapsSnapshot? = null,
        client: OkHttpClient = OkHttpClient(),
        host: DisplayModeHost? = null,
        settings: EngineSettings = EngineSettings(),
        music: Boolean = false,
        now: () -> Long = SystemClock::elapsedRealtime,
        memory: FrameRateMemory = MemoryFrameRate(),
    ): Media3Engine = Media3Engine(
        context = context,
        profile = profile,
        music = music,
        caps = { caps },
        mediaClient = { client },
        settings = { settings },
        frameRateMemory = memory,
        focusPort = if (music) null else AndroidFocusPort(context),
        sdk = 34,
        now = now,
        hostFor = { host ?: WindowModeHost(it) },
        // Moves on by itself whenever the player's threads wait, even while a file is really being read, so a
        // test that waits for READY prepares paused: Media3's stuck-buffering check could reach 10 minutes first.
        playerClock = FakeClock(true),
    ).also { engines += it }

    @After
    fun releaseAll() = engines.forEach { it.release() }

    private fun file(): String {
        val f = File.createTempFile("clip", ".mp4").apply { deleteOnExit() }
        f.writeBytes(clip)
        return f.toURI().toString()
    }

    @Test
    fun aBundledMp4ReachesReadyAndPlaysOnAPhone() {
        codecs.addDecoders(ShadowMediaCodecConfig.CODEC_INFO_AVC, ShadowMediaCodecConfig.CODEC_INFO_AAC)
        val engine = engine()
        engine.player.setVideoSurface(Surface(SurfaceTexture(1)))
        engine.prepare(EngineSource(file(), hls = false, mimeType = "video/mp4"), startMs = 0, playWhenReady = false)

        TestPlayerRunHelper.run(engine.player).untilState(Player.STATE_READY)
        engine.player.play()
        assertTrue("audio focus granted", engine.player.playWhenReady)
        TestPlayerRunHelper.run(engine.player).untilPositionAtLeast(2_000)

        // The first frame itself is checked on a device (EngineOnDeviceTest).
        val state = engine.state.value
        assertNull(state.needsServerHelp)
        assertEquals(1920, state.stats.width)
        assertEquals("SDR", state.stats.dynamicRange)
        assertNotNull(state.stats.videoDecoder)
    }

    @Test
    fun musicPlaysTheAudioAloneAndMissesNothing() {
        codecs.addDecoders(ShadowMediaCodecConfig.CODEC_INFO_AVC, ShadowMediaCodecConfig.CODEC_INFO_AAC)
        val engine = engine(music = true)
        engine.prepare(EngineSource(file(), hls = false), startMs = 0, playWhenReady = false)
        TestPlayerRunHelper.run(engine.player).untilState(Player.STATE_READY)
        engine.player.play()
        TestPlayerRunHelper.run(engine.player).untilPositionAtLeast(1_000)
        assertNull(engine.state.value.needsServerHelp)
        assertNull("no video decoder for music", engine.state.value.stats.videoDecoder)
        assertNotNull(engine.state.value.stats.audioDecoder)
        assertTrue(engine.player.trackSelectionParameters.audioOffloadPreferences.audioOffloadMode != 0)
    }

    @Test
    fun videoWithoutAnAllowedDecoderNeedsTheServer() {
        codecs.addDecoders(ShadowMediaCodecConfig.CODEC_INFO_AAC)
        val engine = engine()
        engine.prepare(EngineSource(file(), hls = false), startMs = 0, playWhenReady = false)
        TestPlayerRunHelper.run(engine.player).untilState(Player.STATE_READY)
        assertEquals(NeedsServerHelp(video = true, audio = false), engine.state.value.needsServerHelp)
    }

    @Test
    fun theMediaClientCarriesTheTokenAndTheFileArrivesInRanges() {
        codecs.addDecoders(ShadowMediaCodecConfig.CODEC_INFO_AVC, ShadowMediaCodecConfig.CODEC_INFO_AAC)
        val server = MockWebServer()
        server.dispatcher = RangeDispatcher(clip, token = "Bearer secret")
        server.start()
        try {
            val client = OkHttpClient.Builder()
                .addInterceptor { chain -> chain.proceed(chain.request().newBuilder().header("Authorization", "Bearer secret").build()) }
                .build()
            val engine = engine(client = client)
            engine.prepare(EngineSource(server.url("/api/files/1/stream").toString(), hls = false), startMs = 5_000, playWhenReady = false)
            TestPlayerRunHelper.run(engine.player).untilState(Player.STATE_READY)
            // A resume starts at the key frame before its place (the clip's are 2 s apart).
            assertEquals(4_000L, engine.player.currentPosition)
            // The header, then from the 5 s point on.
            assertEquals(null, server.takeRequest().headers["Range"])
            assertTrue(server.takeRequest().headers["Range"]!!.startsWith("bytes="))
        } finally {
            server.close()
        }
    }

    /**
     * A resume starts at the key frame before its place, so no frame is
     * decoded only to be dropped; once a frame shows, seeks are exact. The
     * clip's key frames are 2 s apart.
     */
    @Test
    fun aResumeStartsAtAKeyFrameAndSeeksStayExact() {
        codecs.addDecoders(ShadowMediaCodecConfig.CODEC_INFO_AVC, ShadowMediaCodecConfig.CODEC_INFO_AAC)
        val engine = engine()
        engine.player.setVideoSurface(Surface(SurfaceTexture(1)))
        engine.prepare(EngineSource(file(), hls = false, mimeType = "video/mp4"), startMs = 5_000, playWhenReady = false)
        TestPlayerRunHelper.run(engine.player).untilState(Player.STATE_READY)
        assertEquals(4_000L, engine.player.currentPosition)

        engine.player.play()
        TestPlayerRunHelper.run(engine.player).untilPositionAtLeast(4_500)
        engine.player.pause()
        engine.player.seekTo(7_000)
        TestPlayerRunHelper.run(engine.player).untilState(Player.STATE_READY)
        assertEquals(7_000L, engine.player.currentPosition)

        // From the start: nothing to move.
        val fresh = engine()
        fresh.prepare(EngineSource(file(), hls = false, mimeType = "video/mp4"), startMs = 0, playWhenReady = false)
        TestPlayerRunHelper.run(fresh.player).untilState(Player.STATE_READY)
        assertEquals(0L, fresh.player.currentPosition)
    }

    @Test
    fun aDroppedLinkIsWaitedForAndPlaybackGoesOn() {
        codecs.addDecoders(ShadowMediaCodecConfig.CODEC_INFO_AVC, ShadowMediaCodecConfig.CODEC_INFO_AAC)
        // More than the five tries an answer that is an error gets.
        val drops = AtomicInteger(7)
        val ranges = RangeDispatcher(clip, token = "Bearer secret")
        val server = MockWebServer()
        server.dispatcher = object : Dispatcher() {
            override fun dispatch(request: RecordedRequest): MockResponse =
                if (drops.getAndDecrement() > 0) MockResponse.Builder().onResponseStart(SocketEffect.CloseSocket()).build() else ranges.dispatch(request)
        }
        server.start()
        try {
            val client = OkHttpClient.Builder()
                .addInterceptor { chain -> chain.proceed(chain.request().newBuilder().header("Authorization", "Bearer secret").build()) }
                .build()
            // The retries' 90 s on the wall clock: the player's own clock races on while a request is really out.
            val engine = engine(client = client, now = { System.nanoTime() / 1_000_000 })
            engine.prepare(EngineSource(server.url("/api/files/1/stream").toString(), hls = false), startMs = 0, playWhenReady = false)
            RobolectricUtil.runMainLooperUntil { engine.state.value.reconnecting }
            // The failed loads aren't an error: the engine tries again until the server answers.
            TestPlayerRunHelper.run(engine.player).ignoringNonFatalErrors().untilState(Player.STATE_READY)
            RobolectricUtil.runMainLooperUntil { !engine.state.value.reconnecting }
            assertEquals(8, server.requestCount)
        } finally {
            server.close()
        }
    }

    @Test
    fun aMissingSessionFailsAtOnceRatherThanRetrying() {
        val server = MockWebServer()
        server.dispatcher = object : Dispatcher() {
            override fun dispatch(request: RecordedRequest) = MockResponse.Builder().code(404).build()
        }
        server.start()
        try {
            val engine = engine()
            // Not playing: the player's clock moves by itself while the request is really out, and
            // playing, Media3's stuck-buffering check would reach its 10 minutes before a slow 404 came.
            engine.prepare(EngineSource(server.url("/api/playback/s1/index.m3u8").toString(), hls = true), startMs = 0, playWhenReady = false)
            val error = TestPlayerRunHelper.run(engine.player).untilPlayerError()
            assertTrue(MediaRetries.isFinal(error))
            assertEquals(1, server.requestCount)
        } finally {
            server.close()
        }
    }

    @Test
    fun aTvWaitsForItsDisplayModeBeforePlaying() {
        codecs.addDecoders(ShadowMediaCodecConfig.CODEC_INFO_AVC, ShadowMediaCodecConfig.CODEC_INFO_AAC)
        val host = FakeHost(ModeChoiceTest.STREAMER.copy(matchContent = MatchContent.Always))
        val engine = engine(profile = tv, caps = snapshot(DecoderPolicy(tv = true, allowSoftware = true)), host = host)
        val film = MediaStream(index = 0, kind = StreamKind.Video, codec = "h264", frameRate = 23.976, dynamicRange = DynamicRange.Sdr)
        engine.prepare(EngineSource(file(), hls = false, video = film, durationMs = 2 * 60 * 60 * 1000L), startMs = 0, playWhenReady = true)
        // Pressed play at tap time: it buffers, but waits for the window to choose a mode.
        assertFalse(engine.player.playWhenReady)

        val window = Robolectric.buildActivity(Activity::class.java).setup().get().window
        engine.attachWindow(window)
        assertEquals(listOf(46), host.requested)
        assertTrue(engine.state.value.switchingDisplayMode)
        assertEquals("the screen says which rate", 23.976f, engine.state.value.switchingToHz!!, 0.01f)
        assertFalse(engine.player.playWhenReady)

        // Play pressed again while switching: still waits.
        engine.player.play()
        assertFalse(engine.player.playWhenReady)

        shadowOf(Looper.getMainLooper()).idleFor(Duration.ofMillis(FrameRateController.SETTLE_MS))
        assertFalse(engine.state.value.switchingDisplayMode)
        assertNull(engine.state.value.switchingToHz)
        assertTrue(engine.player.playWhenReady)
        assertEquals(46, engine.state.value.stats.displayModeId)

        engine.detachWindow()
        assertEquals("the mode goes back to the system's choice", listOf(46, 0), host.requested)
    }

    @Test
    fun aTvPlaysOnThroughTheHdmiDropOfItsOwnModeChange() {
        codecs.addDecoders(ShadowMediaCodecConfig.CODEC_INFO_AVC, ShadowMediaCodecConfig.CODEC_INFO_AAC)
        val host = FakeHost(ModeChoiceTest.STREAMER.copy(matchContent = MatchContent.Always))
        val engine = engine(profile = tv, caps = snapshot(DecoderPolicy(tv = true, allowSoftware = true)), host = host)
        val film = MediaStream(index = 0, kind = StreamKind.Video, codec = "h264", frameRate = 23.976, dynamicRange = DynamicRange.Sdr)
        engine.prepare(EngineSource(file(), hls = false, video = film, durationMs = 2 * 60 * 60 * 1000L), startMs = 0, playWhenReady = true)
        engine.attachWindow(Robolectric.buildActivity(Activity::class.java).setup().get().window)
        val looper = shadowOf(Looper.getMainLooper())

        // As a Fire TV does while the mode changes: its HDMI audio goes, and Android says the output became noisy.
        context.sendBroadcast(Intent(AudioManager.ACTION_HDMI_AUDIO_PLUG).putExtra(AudioManager.EXTRA_AUDIO_PLUG_STATE, 0))
        context.sendBroadcast(Intent(AudioManager.ACTION_AUDIO_BECOMING_NOISY))
        looper.idle()
        assertFalse(engine.player.playWhenReady)

        looper.idleFor(Duration.ofMillis(FrameRateController.SETTLE_MS))
        assertTrue("the resume plays once the mode settles", engine.player.playWhenReady)

        // The drop's late news doesn't pause it either...
        context.sendBroadcast(Intent(AudioManager.ACTION_HDMI_AUDIO_PLUG).putExtra(AudioManager.EXTRA_AUDIO_PLUG_STATE, 0))
        looper.idle()
        assertTrue(engine.player.playWhenReady)

        // ...but the HDMI output going later still does.
        looper.idleFor(Duration.ofMillis(Media3Engine.SWITCH_QUIET_MS))
        context.sendBroadcast(Intent(AudioManager.ACTION_HDMI_AUDIO_PLUG).putExtra(AudioManager.EXTRA_AUDIO_PLUG_STATE, 0))
        looper.idle()
        assertFalse(engine.player.playWhenReady)
    }

    @Test
    fun aTvLeavesTheModeForShortItemsAndPhonesNeverWait() {
        codecs.addDecoders(ShadowMediaCodecConfig.CODEC_INFO_AVC, ShadowMediaCodecConfig.CODEC_INFO_AAC)
        val host = FakeHost(ModeChoiceTest.STREAMER.copy(matchContent = MatchContent.Always))
        val film = MediaStream(index = 0, kind = StreamKind.Video, codec = "h264", frameRate = 23.976)
        val trailer = engine(profile = tv, caps = snapshot(DecoderPolicy(tv = true, allowSoftware = true)), host = host)
        trailer.prepare(EngineSource(file(), hls = false, video = film, durationMs = 150_000), startMs = 0, playWhenReady = true)
        assertTrue(trailer.player.playWhenReady)

        val off = engine(profile = tv, caps = snapshot(DecoderPolicy(tv = true, allowSoftware = true)), host = host, settings = EngineSettings(refreshRate = RefreshRateMatching.Off))
        off.prepare(EngineSource(file(), hls = false, video = film, durationMs = 7_200_000), startMs = 0, playWhenReady = true)
        assertTrue(off.player.playWhenReady)

        val onPhone = engine()
        onPhone.prepare(EngineSource(file(), hls = false, video = film, durationMs = 7_200_000), startMs = 0, playWhenReady = true)
        assertTrue(onPhone.player.playWhenReady)
        assertEquals(emptyList<Int>(), host.requested)
    }

    /** A TV that shows Dolby Vision at 23.976 Hz but not in its 60 Hz home screen mode. */
    private val dolbyVisionAt24 = DisplayState(
        modes = listOf(
            DisplayModeInfo(1, 3840, 2160, 60f, hdrTypes = listOf(ModeChoice.HDR_HDR10)),
            DisplayModeInfo(2, 3840, 2160, 23.976f, hdrTypes = listOf(ModeChoice.HDR_DOLBY_VISION, ModeChoice.HDR_HDR10)),
        ),
        activeModeId = 1,
        hdrCapabilities = listOf(ModeChoice.HDR_HDR10),
        matchContent = MatchContent.Always,
    )
    private val dolbyVisionTv = CapsSnapshot(
        ClientCaps(),
        emptyList(),
        CapsDetails(
            policy = DecoderPolicy(tv = true),
            screenHdrTypes = listOf(DynamicRange.Hdr10, DynamicRange.DolbyVision),
            dolbyVisionProfiles = listOf(DolbyVisionCaps(profile = 8)),
        ),
    )
    private val dolbyVisionFilm = MediaStream(
        index = 0,
        kind = StreamKind.Video,
        codec = "hevc",
        frameRate = 23.976,
        dynamicRange = DynamicRange.DolbyVision,
        dvProfile = 8,
        dvBlCompat = 1,
    )
    private val twoHours = 2 * 60 * 60 * 1000L

    @Test
    fun dolbyVisionIsJudgedForTheModeTheVideoPlaysIn() = runBlocking {
        val memory = MemoryFrameRate()
        val engine = engine(profile = tv, caps = dolbyVisionTv, host = FakeHost(dolbyVisionAt24), memory = memory)
        engine.attachWindow(Robolectric.buildActivity(Activity::class.java).setup().get().window)
        assertEquals("a film switches to 23.976 Hz, which shows it", true, engine.dolbyVisionShown(dolbyVisionFilm, twoHours))
        assertEquals("a trailer stays in the 60 Hz mode", false, engine.dolbyVisionShown(dolbyVisionFilm, 150_000))
        assertEquals("so does a frame rate unknown", false, engine.dolbyVisionShown(dolbyVisionFilm.copy(frameRate = null), twoHours))
        memory.timeouts = FrameRateController.GIVE_UP_AFTER
        assertEquals("and a film where Auto gave up", false, engine.dolbyVisionShown(dolbyVisionFilm, twoHours))
        val off = engine(profile = tv, caps = dolbyVisionTv, host = FakeHost(dolbyVisionAt24), settings = EngineSettings(refreshRate = RefreshRateMatching.Off))
        off.attachWindow(Robolectric.buildActivity(Activity::class.java).setup().get().window)
        assertEquals("matching turned off", false, off.dolbyVisionShown(dolbyVisionFilm, twoHours))
    }

    @Test
    fun dolbyVisionTurnedOffShowsInTheFilmsModeAndIsRemembered() {
        codecs.addDecoders(ShadowMediaCodecConfig.CODEC_INFO_AVC, ShadowMediaCodecConfig.CODEC_INFO_AAC)
        val memory = MemoryFrameRate()
        val host = FakeHost(dolbyVisionAt24, turnedOff = setOf(ModeChoice.HDR_DOLBY_VISION))
        val engine = engine(profile = tv, caps = dolbyVisionTv, host = host, memory = memory)
        engine.attachWindow(Robolectric.buildActivity(Activity::class.java).setup().get().window)
        // The home screen's mode doesn't list Dolby Vision, so it can't say it is turned off.
        assertEquals(true, runBlocking { engine.dolbyVisionShown(dolbyVisionFilm, twoHours) })

        val source = EngineSource(file(), hls = false, video = dolbyVisionFilm, durationMs = twoHours, dolbyVision = true)
        engine.prepare(source, startMs = 0, playWhenReady = false)
        shadowOf(Looper.getMainLooper()).idleFor(Duration.ofMillis(FrameRateController.SETTLE_MS))
        assertEquals(listOf(2), host.requested)
        assertEquals("seen in the film's mode", setOf(ModeChoice.HDR_DOLBY_VISION), memory.turnedOff[host.displayKey])
        assertEquals(false, runBlocking { engine.dolbyVisionShown(dolbyVisionFilm, twoHours) })
    }

    /** Serves [body] with ranges, as the server's /stream does, to requests carrying [token] only. */
    private class RangeDispatcher(private val body: ByteArray, private val token: String) : Dispatcher() {
        override fun dispatch(request: RecordedRequest): MockResponse {
            if (request.headers["Authorization"] != token) return MockResponse.Builder().code(401).build()
            val range = request.headers["Range"]?.removePrefix("bytes=")?.split('-')
            val start = range?.getOrNull(0)?.toLongOrNull() ?: 0L
            val end = range?.getOrNull(1)?.toLongOrNull() ?: (body.size - 1L)
            val slice = body.copyOfRange(start.toInt(), end.toInt() + 1)
            val headers = Headers.Builder()
                .add("Content-Type", "video/mp4")
                .add("Accept-Ranges", "bytes")
                .apply { if (range != null) add("Content-Range", "bytes $start-$end/${body.size}") }
                .build()
            return MockResponse.Builder()
                .code(if (range != null) 206 else 200)
                .headers(headers)
                .body(Buffer().write(slice))
                .build()
        }
    }
}
