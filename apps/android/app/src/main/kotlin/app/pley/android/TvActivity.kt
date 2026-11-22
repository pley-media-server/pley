package app.pley.android

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.runtime.CompositionLocalProvider
import androidx.core.content.ContextCompat
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.lifecycleScope
import androidx.lifecycle.repeatOnLifecycle
import app.pley.android.common.AppLanguage
import app.pley.android.common.Tier
import app.pley.android.common.perf.FrameRate
import app.pley.android.data.SessionState
import app.pley.android.designsystem.image.LocalArtworkImageLoader
import app.pley.android.player.PlayRequest
import app.pley.android.player.music.MusicHub
import app.pley.android.screens.onboarding.Household
import app.pley.android.ui.tv.music.MusicOnLeave
import app.pley.android.ui.tv.shell.TvApp
import app.pley.android.ui.tv.shell.TvShellDemo
import app.pley.android.ui.tv.shell.TvShellSource
import app.pley.android.watchnext.WatchNextLink
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/**
 * The native app on TVs (Android TV, Google TV, Fire TV). Back goes
 * through the activity's back dispatcher only (Compose's BackHandler; the
 * manifest turns on the system's back callbacks), which Fire OS 16 needs:
 * it can skip onBackPressed. The last Back finishes the activity, not the
 * process: the next one reads again what its first screen is chosen by
 * (PleyApplication.startupFor).
 *
 * Debug builds take two extras, for trying the frame on an emulator:
 * `--es tier T0|T1|T2` overrides the device's tier, and `--ez demo true`
 * shows made-up libraries and rows instead of the app's data.
 *
 * ```
 * adb shell am start -n app.pley.android/.TvActivity --ez demo true --es tier T2
 * ```
 *
 * Every build takes `--ez frames true` (or `false`), the frame-rate
 * counter for this run (FrameRate), also when the app is already open.
 *
 * A program of the home screen's Watch Next row opens it with its server,
 * profile and item ([WatchNextLink]): when that profile is the one signed
 * in, the item's page opens and it plays from where it was; otherwise the
 * app just opens. While it is in the foreground, Android TV and Google TV
 * get the profile's Continue Watching in that row (WatchNext).
 *
 * Home, another app or the last Back pauses the music, which plays on
 * when the app is back after Home or another app ([MusicOnLeave]).
 */
class TvActivity : ComponentActivity() {
    /** The Watch Next program that opened the app, until it is played: not checked yet ([WatchNextLink.verified]). */
    private val link = MutableStateFlow<WatchNextLink.Link?>(null)

    /** The item of [link], once its profile is the one signed in. */
    private val opened = MutableStateFlow<Long?>(null)

    /** A Watch Next program brought the app back, until it resumes: its video plays, not the music. */
    private var linkArrived = false

    private val main = Handler(Looper.getMainLooper())

    // The app's music once it is made (MusicHub): leaving before then makes no player.
    private val musicOnLeave = MusicOnLeave(
        now = SystemClock::uptimeMillis,
        later = { delayMs, block ->
            main.postDelayed(block, delayMs)
            ({ main.removeCallbacks(block) })
        },
        music = { MusicHub.playback?.let { (application as PleyApplication).graph.player.music } },
    )

    /** The screensaver starting and stopping, for [musicOnLeave]: the system's own broadcasts. */
    private val dreams = object : BroadcastReceiver() {
        override fun onReceive(context: Context, intent: Intent) {
            musicOnLeave.dreaming(intent.action == Intent.ACTION_DREAMING_STARTED)
        }
    }

    override fun attachBaseContext(newBase: Context) {
        super.attachBaseContext(AppLanguage.wrap(newBase))
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val app = application as PleyApplication
        val device = app.graph.device
        // Not again when the activity is made anew (a rotation, a process brought back).
        if (savedInstanceState == null) {
            link.value = WatchNextLink.read(intent)
            linkArrived = link.value != null
        }
        followWatchNext(app)
        val dreamFilter = IntentFilter(Intent.ACTION_DREAMING_STARTED).apply { addAction(Intent.ACTION_DREAMING_STOPPED) }
        ContextCompat.registerReceiver(this, dreams, dreamFilter, ContextCompat.RECEIVER_NOT_EXPORTED)
        val tier = debugTier() ?: device.tier
        val demo = BuildConfig.DEBUG && intent.getBooleanExtra(EXTRA_DEMO, false)
        // Made before the content, so what the first screen is chosen by is read
        // (in the background) while the window comes up: the first frame shows it.
        val source = if (demo) {
            null
        } else {
            val read = app.startupFor(savedInstanceState)
            TvShellSource.of(
                app.graph,
                startup = { read.await().let { it.server to it.signedIn } },
                // "Who's watching?" at a cold start needs the household's size, saved on this device.
                householdSize = {
                    val startup = read.await()
                    Household.sizeAtStart(app, { app.graph }, startup.server, startup.signedIn)
                },
            )
        }
        setContent {
            if (source == null) {
                CompositionLocalProvider(LocalArtworkImageLoader provides TvShellDemo.imageLoader(this, device.copy(tier = tier))) {
                    TvApp(TvShellDemo.source, onExit = ::finish, tier = tier, placeholder = TvShellDemo.placeholder)
                }
            } else {
                TvApp(source, onExit = ::finish, tier = tier, opened = opened, onOpened = ::play)
            }
        }
    }

    override fun onUserLeaveHint() {
        super.onUserLeaveHint()
        musicOnLeave.leaving()
    }

    override fun onResume() {
        super.onResume()
        musicOnLeave.resumed(videoNext = linkArrived)
        linkArrived = false
    }

    override fun onStop() {
        musicOnLeave.stopped(finishing = isFinishing)
        super.onStop()
    }

    override fun onDestroy() {
        unregisterReceiver(dreams)
        super.onDestroy()
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        FrameRate.read(intent)
        WatchNextLink.read(intent)?.let {
            link.value = it
            linkArrived = true
        }
    }

    /**
     * The link's item, once its profile is signed in; and on Android TV and
     * Google TV (not Fire TV: no such row), the Watch Next row kept while
     * the app is in the foreground.
     */
    private fun followWatchNext(app: PleyApplication) {
        val graph = app.graph
        if (!graph.device.isTv) return
        lifecycleScope.launch {
            // The data graph is made off the main thread (the start reads it there too).
            val data = withContext(Dispatchers.Default) { graph.data }
            // Any app may send this activity a link: only one this installation signed plays.
            val key = withContext(Dispatchers.IO) { WatchNextLink.key(app) }
            combine(link, data.session.state) { link, state ->
                val session = (state as? SessionState.SignedIn)?.session
                val wanted = link?.let { WatchNextLink.verified(it, key) }
                wanted?.takeIf { (owner, _) -> session != null && session.issuer == owner.issuer && session.me?.id == owner.profileId }?.second
            }.collect { opened.value = it }
        }
        val row = graph.watchNext ?: return
        lifecycleScope.launch { repeatOnLifecycle(Lifecycle.State.STARTED) { row.follow() } }
    }

    /** The Watch Next program's item: it plays, from where it was. */
    private fun play(itemId: Long) {
        link.value = null
        opened.value = null
        (application as PleyApplication).graph.playerLauncher.play(PlayRequest(itemId))
    }

    /** Debug builds: the tier asked for with `--es tier`, or `null`. */
    private fun debugTier(): Tier? {
        if (!BuildConfig.DEBUG) return null
        return intent.getStringExtra(EXTRA_TIER)?.let { name -> Tier.entries.firstOrNull { it.name == name } }
    }

    private companion object {
        const val EXTRA_TIER = "tier"
        const val EXTRA_DEMO = "demo"
    }
}
