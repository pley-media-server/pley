package app.pley.android.player.engine

import android.os.Build
import android.os.StrictMode
import android.os.SystemClock
import android.os.strictmode.Violation
import android.util.Log
import android.view.SurfaceView
import androidx.activity.ComponentActivity
import androidx.media3.common.Player
import androidx.test.core.app.ActivityScenario
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import app.pley.android.caps.CapsDetails
import app.pley.android.caps.CapsSnapshot
import app.pley.android.caps.DecoderPolicy
import app.pley.android.common.DeviceProfile
import app.pley.android.model.caps.ClientCaps
import app.pley.android.player.EngineSource
import mockwebserver3.Dispatcher
import mockwebserver3.MockResponse
import mockwebserver3.MockWebServer
import mockwebserver3.RecordedRequest
import okhttp3.Headers
import okhttp3.OkHttpClient
import okio.Buffer
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import java.util.concurrent.CopyOnWriteArrayList
import java.util.concurrent.TimeUnit

/**
 * The engine on a real device or emulator: a clip from a server with a
 * token reaches its first frame on a SurfaceView, nothing dropped, nothing
 * read or written on the main thread. On emulators software decoders are
 * allowed (the debug switch); on devices the real probe decides.
 */
@RunWith(AndroidJUnit4::class)
class EngineOnDeviceTest {
    private val instrumentation = InstrumentationRegistry.getInstrumentation()
    private val context = instrumentation.targetContext
    private val profile = DeviceProfile.of(context)
    private lateinit var scenario: ActivityScenario<ComponentActivity>
    private lateinit var surface: SurfaceView
    private var engine: Media3Engine? = null
    private val server = MockWebServer()

    @Before
    fun showASurface() {
        scenario = ActivityScenario.launch(ComponentActivity::class.java)
        scenario.onActivity { activity ->
            // Shown over a lock screen, as a player is; the device is woken for the run.
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O_MR1) {
                activity.setShowWhenLocked(true)
                activity.setTurnScreenOn(true)
            }
            surface = SurfaceView(activity)
            activity.setContentView(surface)
        }
    }

    @After
    fun tearDown() {
        instrumentation.runOnMainSync { engine?.release() }
        scenario.close()
        server.close()
    }

    private fun serve(resource: String, token: String) {
        val bytes = javaClass.classLoader!!.getResourceAsStream("app/pley/android/fixtures/media/$resource")!!.use { it.readBytes() }
        server.dispatcher = RangeDispatcher(bytes, token)
        server.start()
    }

    private fun emulator(): Boolean = Build.FINGERPRINT.contains("generic") || Build.HARDWARE.contains("ranchu") || Build.HARDWARE.contains("goldfish")

    /** The real probe on devices; on emulators, software decoders allowed. */
    private fun caps(): CapsSnapshot = if (emulator()) {
        CapsSnapshot(ClientCaps(), emptyList(), CapsDetails(policy = DecoderPolicy(tv = profile.isTv, allowSoftware = true)))
    } else {
        realProbe(context, profile)
    }

    private fun makeEngine(caps: CapsSnapshot, client: OkHttpClient): Media3Engine {
        var made: Media3Engine? = null
        instrumentation.runOnMainSync {
            made = Media3Engine(
                context = context,
                profile = profile,
                music = false,
                caps = { caps },
                mediaClient = { client },
                settings = { EngineSettings(refreshRate = RefreshRateMatching.Off) },
                frameRateMemory = PreferencesFrameRateMemory(context),
                focusPort = AndroidFocusPort(context),
            ).also { it.player.setVideoSurfaceView(surface) }
        }
        return made!!.also { engine = it }
    }

    private fun bearer(token: String) = OkHttpClient.Builder()
        .addInterceptor { chain -> chain.proceed(chain.request().newBuilder().header("Authorization", token).build()) }
        .build()

    private fun <T> onMain(block: () -> T): T {
        var out: T? = null
        instrumentation.runOnMainSync { out = block() }
        @Suppress("UNCHECKED_CAST")
        return out as T
    }

    private fun waitFor(timeoutMs: Long, what: String, condition: () -> Boolean) {
        val end = SystemClock.elapsedRealtime() + timeoutMs
        while (!onMain(condition)) {
            check(SystemClock.elapsedRealtime() < end) { "Timed out waiting for $what" }
            SystemClock.sleep(50)
        }
    }

    @Test
    fun aClipFromTheServerReachesItsFirstFrameWithNothingDroppedAndNoMainThreadIo() {
        assumeTrue("StrictMode's listener needs Android 9", Build.VERSION.SDK_INT >= Build.VERSION_CODES.P)
        serve("sample-1080p.mp4", "Bearer secret")
        val caps = caps()
        val violations = CopyOnWriteArrayList<Violation>()
        instrumentation.runOnMainSync {
            StrictMode.setThreadPolicy(
                StrictMode.ThreadPolicy.Builder().detectDiskReads().detectDiskWrites().detectNetwork()
                    .penaltyListener(Runnable::run) { violations += it }
                    .build(),
            )
        }
        // Measured from a surface on screen (a locked device shows it once awake).
        waitFor(10_000, "the surface") { surface.holder.surface.isValid }
        val engine = makeEngine(caps, bearer("Bearer secret"))
        instrumentation.runOnMainSync {
            engine.prepare(EngineSource(server.url("/api/files/1/stream").toString(), hls = false, mimeType = "video/mp4"), 0, playWhenReady = true)
        }
        waitFor(10_000, "the first frame") { engine.state.value.stats.firstFrameMs != null || engine.player.playerError != null }
        assertEquals(null, onMain { engine.player.playerError })
        waitFor(15_000, "5 s of playback") { engine.player.currentPosition >= 5_000 }
        instrumentation.runOnMainSync { engine.player.pause() }
        SystemClock.sleep(1_200)
        val stats = onMain { engine.state.value.stats }
        instrumentation.runOnMainSync { StrictMode.setThreadPolicy(StrictMode.ThreadPolicy.LAX) }
        Log.i(TAG, "first frame ${stats.firstFrameMs} ms; $stats")

        // Waited for with a limit, as :core:testing's asked() does: the device tests don't build on that module.
        val asked = server.takeRequest(5, TimeUnit.SECONDS) ?: throw AssertionError("the server got no request for the clip")
        assertEquals("Bearer secret", asked.headers["Authorization"])
        assertNotNull(stats.videoDecoder)
        assertTrue("rendered ${stats.renderedFrames}", stats.renderedFrames > 100)
        assertEquals("dropped buffers", 0, stats.droppedFrames)
        assertEquals("main-thread I/O: ${violations.joinToString { it.toString() + it.stackTrace.take(8).joinToString() }}", 0, violations.size)
    }

    @Test
    fun theHdr10ClipPlaysOnAHardwareDecoderOnHdrScreens() {
        assumeTrue("an HDR screen and hardware HEVC Main 10", !emulator())
        val caps = caps()
        assumeTrue("an HDR10 screen", caps.details.screenHdrTypes.isNotEmpty())
        serve("sample-2160p-hdr10.mkv", "Bearer secret")
        // Measured from a surface on screen (a locked device shows it once awake).
        waitFor(10_000, "the surface") { surface.holder.surface.isValid }
        val engine = makeEngine(caps, bearer("Bearer secret"))
        instrumentation.runOnMainSync {
            engine.prepare(EngineSource(server.url("/api/files/2/stream").toString(), hls = false, mimeType = "video/x-matroska"), 0, playWhenReady = true)
        }
        waitFor(15_000, "the first frame") { engine.state.value.stats.firstFrameMs != null || engine.player.playerError != null }
        waitFor(20_000, "8 s of playback") { engine.player.currentPosition >= 8_000 || engine.player.playerError != null }
        val stats = onMain { engine.state.value.stats }
        Log.i(TAG, "HDR10 2160p23.976: $stats; error ${onMain { engine.player.playerError }}")
        assertEquals(null, onMain { engine.player.playerError })
        assertEquals(true, stats.videoDecoderHardware)
        assertEquals("HDR10", stats.dynamicRange)
        assertEquals(Player.STATE_READY, onMain { engine.player.playbackState })
    }

    /** Serves a file with ranges, as pley's /stream does, to requests with the token only. */
    private class RangeDispatcher(private val body: ByteArray, private val token: String) : Dispatcher() {
        override fun dispatch(request: RecordedRequest): MockResponse {
            if (request.headers["Authorization"] != token) return MockResponse.Builder().code(401).build()
            val range = request.headers["Range"]?.removePrefix("bytes=")?.split('-')
            val start = range?.getOrNull(0)?.toLongOrNull() ?: 0L
            val end = range?.getOrNull(1)?.toLongOrNull() ?: (body.size - 1L)
            val headers = Headers.Builder()
                .add("Content-Type", "application/octet-stream")
                .add("Accept-Ranges", "bytes")
                .apply { if (range != null) add("Content-Range", "bytes $start-$end/${body.size}") }
                .build()
            return MockResponse.Builder()
                .code(if (range != null) 206 else 200)
                .headers(headers)
                .body(Buffer().write(body, start.toInt(), (end - start + 1).toInt()))
                .build()
        }
    }

    private companion object {
        const val TAG = "EngineOnDeviceTest"
    }
}
