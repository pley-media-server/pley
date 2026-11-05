package app.pley.android.ui.tv.livetv

import android.content.Context
import android.content.Intent
import android.os.Bundle
import android.view.WindowManager
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.viewModels
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.gestures.LocalBringIntoViewSpec
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.lifecycleScope
import androidx.lifecycle.repeatOnLifecycle
import app.pley.android.common.AppLanguage
import app.pley.android.player.CastFrom
import app.pley.android.player.PlayerEngine
import app.pley.android.player.PlayerWindows
import app.pley.android.player.engine.VideoMediaSession
import app.pley.android.player.live.LivePhase
import app.pley.android.player.live.LiveSessionPlayer
import app.pley.android.screens.PleyGraphOwner
import app.pley.android.screens.livetv.LivePlayerViewModel
import app.pley.android.ui.tv.kit.KeyRepeatGate
import app.pley.android.ui.tv.kit.LocalKeyRepeatGate
import app.pley.android.ui.tv.kit.TvCastNotice
import app.pley.android.ui.tv.kit.TvScrolling
import app.pley.android.ui.tv.kit.keyRepeatGate
import app.pley.android.ui.tv.theme.TvTheme
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.launch

/**
 * The TV's live player, in a window of its own as the TV player is:
 * onStart tunes the channel (again, coming back from Home or the screen
 * saver) and starts the media session, so remotes' play and pause and
 * Alexa reach it; onStop lets the provider's connection, the player and
 * its decoders go. Nothing plays behind other apps; the screen stays on
 * only while it plays. One player at a time: opening closes another
 * ([PlayerWindows]). A channel another device sent here says where from
 * as it opens.
 */
class TvLivePlayerActivity : ComponentActivity() {
    private val vm: LivePlayerViewModel by viewModels {
        LivePlayerViewModel.factory(intent.getLongExtra(EXTRA_CHANNEL, 0), intent.getStringExtra(EXTRA_CATEGORY) ?: "all")
    }
    private var attached: PlayerEngine? = null
    private var session: VideoMediaSession? = null
    private var screenOn = false

    override fun attachBaseContext(newBase: Context) {
        super.attachBaseContext(AppLanguage.wrap(newBase))
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        PlayerWindows.opened(this)
        val graph = (application as PleyGraphOwner).screensGraph
        // Said once, as it opens: not again when the window is made anew.
        val cast = castFrom(intent).takeIf { savedInstanceState == null }?.notice(resources)
        setContent {
            TvTheme(tier = graph.device.tier) {
                WindowRules {
                    TvLivePlayerScreen(vm = vm, onExit = ::finish)
                    TvCastNotice(cast)
                }
            }
        }
        lifecycleScope.launch {
            repeatOnLifecycle(Lifecycle.State.STARTED) {
                combine(vm.engine, vm.state) { engine, state -> engine to state }.collect { (engine, state) ->
                    attach(engine)
                    keepScreenOn(state.phase == LivePhase.Playing || state.phase == LivePhase.Tuning)
                }
            }
        }
    }

    override fun onStart() {
        super.onStart()
        vm.resume()
    }

    override fun onDestroy() {
        PlayerWindows.closed(this)
        super.onDestroy()
    }

    override fun onStop() {
        detach()
        if (!isChangingConfigurations) {
            vm.stop()
            keepScreenOn(false)
        }
        super.onStop()
    }

    /** The engine, once there is one: this window for it, and the media session while started. */
    private fun attach(engine: PlayerEngine?) {
        if (attached === engine) return
        detach()
        if (engine == null || !lifecycle.currentState.isAtLeast(Lifecycle.State.STARTED)) return
        attached = engine
        engine.attachWindow(window)
        // Live has no pause and no going back: the session offers none to remotes and assistants.
        session = VideoMediaSession(this, LiveSessionPlayer(engine.player)).also { it.start() }
    }

    private fun detach() {
        session?.stop()
        session = null
        attached?.detachWindow()
        attached = null
    }

    private fun keepScreenOn(on: Boolean) {
        if (on == screenOn) return
        screenOn = on
        if (on) window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON) else window.clearFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
    }

    /**
     * The D-pad rules of the app's own window ([app.pley.android.ui.tv.shell.TvApp]'s) in this
     * one: a held key moves focus at the gate's pace, not on each of the remote's 20 repeats a
     * second, and the channel list scrolls focus into view as the TV's lists do.
     */
    @OptIn(ExperimentalFoundationApi::class)
    @Composable
    private fun WindowRules(content: @Composable BoxScope.() -> Unit) {
        val gate = remember { KeyRepeatGate() }
        val scrolling = remember { TvScrolling.root() }
        CompositionLocalProvider(LocalKeyRepeatGate provides gate, LocalBringIntoViewSpec provides scrolling) {
            Box(Modifier.fillMaxSize().keyRepeatGate(gate)) { content() }
        }
    }

    companion object {
        /** The channel to tune as it opens. */
        const val EXTRA_CHANNEL = "channel"

        /** The category it was chosen in (a LiveCategory's key): channel up and down stay in it. */
        const val EXTRA_CATEGORY = "category"

        /** Sent here by a remote on another device ([CastFrom]): whether, and from which device. */
        const val EXTRA_CAST = "cast"
        const val EXTRA_CAST_FROM = "cast_from"

        fun intent(context: Context, channelId: Long, category: String, castFrom: CastFrom? = null): Intent =
            Intent(context, TvLivePlayerActivity::class.java)
                .putExtra(EXTRA_CHANNEL, channelId)
                .putExtra(EXTRA_CATEGORY, category)
                .putExtra(EXTRA_CAST, castFrom != null)
                .putExtra(EXTRA_CAST_FROM, castFrom?.device)

        /** Where the channel [intent] opens was sent from; `null` when it wasn't. */
        internal fun castFrom(intent: Intent): CastFrom? =
            CastFrom(intent.getStringExtra(EXTRA_CAST_FROM)).takeIf { intent.getBooleanExtra(EXTRA_CAST, false) }
    }
}
