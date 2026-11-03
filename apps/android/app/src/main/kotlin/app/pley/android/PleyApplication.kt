package app.pley.android

import android.app.Application
import android.os.Bundle
import android.os.StrictMode
import androidx.tracing.trace
import app.pley.android.common.AppBuild
import app.pley.android.common.TraceNames
import app.pley.android.common.perf.FrameRate
import app.pley.android.common.perf.StartupSignals
import app.pley.android.designsystem.PleyImageLoader
import app.pley.android.downloads.AppDownloads
import app.pley.android.downloads.DownloadsOwner
import app.pley.android.screens.PleyGraphOwner
import app.pley.android.screens.ScreensGraph
import coil3.ImageLoader
import coil3.PlatformContext
import coil3.SingletonImageLoader
import java.util.ServiceLoader
import java.util.concurrent.Callable
import java.util.concurrent.atomic.AtomicInteger
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Deferred
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.MainScope
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch

/**
 * The app. Starting builds nothing on the main thread: it starts the one
 * disk read the first screen needs, and the graph's warm-up (AppGraph)
 * beside it, and returns (15 ms at most).
 */
class PleyApplication : Application(), PleyGraphOwner, DownloadsOwner, SingletonImageLoader.Factory {
    val graph: AppGraph by lazy { AppGraph(this) }

    /** What the latest window's first screen was chosen by (StartupRead): read as the process starts, then by [startupFor]. */
    @Volatile
    private lateinit var startup: Deferred<StartupState>

    private val windows = AtomicInteger()

    override val screensGraph: ScreensGraph
        get() = graph

    override val appDownloads: AppDownloads
        get() = graph.appDownloads

    override fun onCreate() {
        trace(TraceNames.APP_ON_CREATE) {
            super.onCreate()
            AppBuild.commit = BuildConfig.COMMIT
            if (BuildConfig.DEBUG) watchForSlowCalls()
            // The frame-rate counter's switch, read in the background; off, it only follows the activities.
            FrameRate.watch(this, shells = setOf(TvActivity::class.java, MainActivity::class.java))
            startup = StartupRead.start(this)
            restartDemoServer()
            warmGraph()
        }
    }

    /**
     * What a new window's first screen is chosen by. The first window
     * takes the read made as the process started. The process outlives its
     * windows (Back on a TV, or on a phone's Home, ends the window only), so
     * a later window starting afresh reads again: a server may have been
     * linked, or signed out of, since. A window brought back with its saved
     * state takes the last read, as its screens come back by themselves.
     */
    fun startupFor(savedInstanceState: Bundle?): Deferred<StartupState> {
        if (windows.getAndIncrement() > 0 && savedInstanceState == null) startup = StartupRead.start(this)
        return startup
    }

    /**
     * Makes, off the main thread, what the first screen and the first play
     * ask for: the data side at once (its HTTP client reads the system's
     * certificates, 300 ms on a Fire TV stick), the player and the device's
     * capabilities once Home is drawn. The graph's parts are made once, so
     * a screen that asks first waits for this instead of making them again.
     *
     * Two first uses that held everything behind them happen here too:
     * - the picture cache's opening: Coil reads its journal (a line per
     *   cached picture) the first time a picture is asked for, holding the
     *   cache, so all of Home's first pictures waited (200 ms on a Google
     *   TV Streamer with a big library);
     * - the server's tokens, unsealed by the Keystore: the first requests
     *   waited for it, one holding the store.
     */
    private fun warmGraph() {
        CoroutineScope(SupervisorJob() + Dispatchers.Default).launch {
            launch(Dispatchers.IO) {
                runCatching { SingletonImageLoader.get(this@PleyApplication).diskCache?.openSnapshot(WARM_UP_KEY)?.close() }
            }
            graph.data
            val start = startup.await()
            val server = start.server
            if (server != null && start.signedIn) {
                launch(Dispatchers.IO) { runCatching { graph.auth.tokens(server).scopes() } }
            }
            StartupSignals.homeDrawn.first { it }
            graph.player
            // The probe itself, not only its holder: the first play needs its answer.
            runCatching { graph.caps.capabilities.warmUp() }
            // The downloads' index, read before a download is played or listed.
            if (!graph.device.isTv) runCatching { graph.downloads }
        }
    }

    /**
     * Builds with the demo server (debug and benchmark builds): it lives in
     * the app's process, so when the device is on it, it starts again with
     * the app, off the main thread, before anything asks it for data.
     */
    private fun restartDemoServer() {
        val type = try {
            Class.forName("app.pley.android.fixtures.DemoServerFactory")
        } catch (_: ClassNotFoundException) {
            return
        }
        val factory = ServiceLoader.load(type, type.classLoader).firstOrNull() as? Callable<*> ?: return
        MainScope().launch(Dispatchers.IO) {
            if (startup.await().server == DEMO_SERVER) runCatching { factory.call() }
        }
    }

    private companion object {
        /** The demo server's address on its usual port (the fixtures' FixtureDemoServerFactory.PORT). */
        const val DEMO_SERVER = "http://127.0.0.1:7740"

        /** A key no picture has: looking it up opens the picture cache and finds nothing. */
        const val WARM_UP_KEY = "pley-warm-up"
    }

    /** Coil's one loader, on the images client; made with the first picture. */
    override fun newImageLoader(context: PlatformContext): ImageLoader =
        PleyImageLoader.create(context, { graph.network.images }, graph.device)

    /** Debug builds log disk and network work on the main thread, and leaks. */
    private fun watchForSlowCalls() {
        StrictMode.setThreadPolicy(StrictMode.ThreadPolicy.Builder().detectAll().penaltyLog().build())
        StrictMode.setVmPolicy(StrictMode.VmPolicy.Builder().detectAll().penaltyLog().build())
    }
}
