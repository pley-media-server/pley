package app.pley.android

import android.app.UiModeManager
import android.content.Context
import android.content.Intent
import android.content.res.Configuration
import android.graphics.Color
import android.os.Bundle
import android.os.SystemClock
import androidx.activity.ComponentActivity
import androidx.activity.SystemBarStyle
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.core.splashscreen.SplashScreen.Companion.installSplashScreen
import androidx.lifecycle.lifecycleScope
import app.pley.android.common.AppLanguage
import app.pley.android.ui.phone.shell.PhoneApp
import app.pley.android.ui.phone.shell.PhoneStart
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.launch

/**
 * The native app on phones and tablets. A TV that opens it (the music's
 * notification, a launcher) is sent to TvActivity, the one already open if
 * there is one. The splash screen stays only while the startup read runs,
 * 300 ms at most: nothing here waits for the network. The first screen is
 * chosen from that read alone (PhoneApp's start gate). The window is drawn
 * edge to edge, and Back is predictive (the manifest's
 * enableOnBackInvokedCallback; every screen handles it through the
 * activity's back dispatcher).
 */
class MainActivity : ComponentActivity() {
    override fun attachBaseContext(newBase: Context) {
        super.attachBaseContext(AppLanguage.wrap(newBase))
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        val splash = installSplashScreen()
        super.onCreate(savedInstanceState)
        if (isTelevision()) {
            startActivity(
                Intent(this, TvActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_CLEAR_TOP or Intent.FLAG_ACTIVITY_SINGLE_TOP),
            )
            finish()
            return
        }
        val app = application as PleyApplication
        val startup = app.startupFor(savedInstanceState)
        val shownAt = SystemClock.uptimeMillis()
        splash.setKeepOnScreenCondition {
            !startup.isCompleted && SystemClock.uptimeMillis() - shownAt < SPLASH_LIMIT_MS
        }
        // pley is dark whatever the system's theme: light icons on both bars.
        enableEdgeToEdge(
            statusBarStyle = SystemBarStyle.dark(Color.TRANSPARENT),
            navigationBarStyle = SystemBarStyle.dark(Color.TRANSPARENT),
        )
        setContent {
            PhoneApp(
                start = readNow(startup),
                // A read that failed counts as nothing saved: Servers.
                awaitStart = { runCatching { startup.await().forPhone() }.getOrDefault(PhoneStart(server = null, signedIn = false)) },
                graph = { app.graph },
            )
        }
    }

    /**
     * Downloads that wait go on (Android lets the app start their service
     * while it is in the foreground), and progress kept while offline goes
     * to the server: off the main thread, as the downloads' index is read
     * from disk the first time.
     */
    override fun onStart() {
        super.onStart()
        if (isFinishing) return
        val app = application as PleyApplication
        lifecycleScope.launch(Dispatchers.Default) { app.graph.downloads.resume() }
    }

    /** The startup read's answer if it has finished, without waiting. */
    @OptIn(ExperimentalCoroutinesApi::class)
    private fun readNow(startup: kotlinx.coroutines.Deferred<StartupState>): PhoneStart? =
        if (startup.isCompleted && !startup.isCancelled) runCatching { startup.getCompleted().forPhone() }.getOrNull() else null

    private fun StartupState.forPhone() = PhoneStart(server = server, signedIn = signedIn)

    private fun isTelevision(): Boolean =
        getSystemService(UiModeManager::class.java)?.currentModeType == Configuration.UI_MODE_TYPE_TELEVISION

    private companion object {
        const val SPLASH_LIMIT_MS = 300L
    }
}
