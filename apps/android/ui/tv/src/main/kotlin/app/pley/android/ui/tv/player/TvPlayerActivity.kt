package app.pley.android.ui.tv.player

import android.content.Context
import android.content.Intent
import android.os.Bundle
import android.view.WindowManager
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.lifecycleScope
import androidx.lifecycle.repeatOnLifecycle
import app.pley.android.data.SubtitleBackground
import app.pley.android.player.PlaybackController
import app.pley.android.player.PlayerEngine
import app.pley.android.player.PlayerPhase
import app.pley.android.player.PlayerWindows
import app.pley.android.player.engine.VideoMediaSession
import app.pley.android.player.subtitles.CaptionLook
import app.pley.android.player.session.SavedPlaybackState
import app.pley.android.screens.PleyGraphOwner
import app.pley.android.screens.ScreensGraph
import app.pley.android.ui.tv.theme.TvTheme
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.launch

/**
 * The TV player, in a window of its own (TV addendum §5.1): the display
 * mode follows the video through this window, and Amazon gives the default
 * mode back when it ends.
 *
 * Its life follows the window's:
 * - **onStart** attaches the playback [PlayerLauncher][app.pley.android.screens.PlayerLauncher]
 *   prepared to this window (the display mode is chosen before the first
 *   frame), tells the capability probe which display this is, and starts
 *   the media session, so remotes' media keys and Alexa reach the player.
 * - **onStop** keeps where it was, ends the media session and lets
 *   everything go: the server's session, the player, its decoders and audio
 *   output (Amazon 1.3, Google's TV-NP). Nothing plays behind other apps,
 *   and there is no picture in picture on TV.
 * - **Back to it** (from Home or the screen saver) starts the playback
 *   again where it was, paused if it was.
 *
 * The screen stays on only while it plays (Amazon 3.14, TV-BU/BY). Audio
 * focus, HDMI unplugs and "becoming noisy" are the engine's.
 */
class TvPlayerActivity : ComponentActivity() {
    private val graph: ScreensGraph by lazy { (application as PleyGraphOwner).screensGraph }

    /** The playback shown now; replaced when the screen comes back after onStop. */
    private var controller: PlaybackController? by mutableStateOf(null)
    private var key: String? = null

    /** Where it was when the window went (onStop, or the process was killed). */
    private var saved: SavedPlaybackState? = null
    private var wasPlaying = true

    private var session: VideoMediaSession? = null
    private var attached: PlayerEngine? = null
    private var captionLook by mutableStateOf(CaptionLook())

    /** Settings → Playback details: the readout shows over the video. */
    private var showDetails by mutableStateOf(false)

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        // One player at a time, as one thing plays at a time: one left under this one (Live TV's too) goes.
        PlayerWindows.opened(this)
        key = savedInstanceState?.getString(STATE_KEY) ?: intent.getStringExtra(EXTRA_PLAYBACK)
        saved = SavedPlaybackState.fromBundle(savedInstanceState?.getBundle(STATE_SAVED))
        wasPlaying = savedInstanceState?.getBoolean(STATE_PLAYING, true) ?: true
        controller = key?.let { graph.player.services.controller(it) }
        if (controller == null && saved == null) {
            finish()
            return
        }
        setContent {
            TvTheme(tier = graph.device.tier) {
                val playing = controller
                if (playing != null) {
                    key(playing) {
                        TvPlayerScreen(
                            controller = playing,
                            onExit = ::finish,
                            captionLook = captionLook,
                            onSubtitleStyle = ::chooseStyle,
                            showDetails = showDetails,
                        )
                    }
                }
            }
        }
        followPlayback()
        followSettings()
    }

    override fun onStart() {
        super.onStart()
        if (controller == null) {
            val from = saved ?: return finish()
            val restored = graph.player.services.restore(from)
            key = restored
            controller = graph.player.services.controller(restored)
            if (!wasPlaying) controller?.pause()
        }
        saved = null
        window.decorView.display?.displayId?.let(graph.caps.capabilities::setDisplay)
        attachEngine()
    }

    override fun onStop() {
        if (!isChangingConfigurations) {
            val playing = controller
            if (playing != null) {
                saved = playing.saveState()
                wasPlaying = playing.state.value.phase.wantsToPlay()
            }
            session?.stop()
            session = null
            attached?.detachWindow()
            attached = null
            playing?.close()
            controller = null
            key = null
            window.clearFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
        }
        super.onStop()
    }

    override fun onDestroy() {
        PlayerWindows.closed(this)
        super.onDestroy()
    }

    override fun onSaveInstanceState(outState: Bundle) {
        super.onSaveInstanceState(outState)
        key?.let { outState.putString(STATE_KEY, it) }
        val state = controller?.saveState() ?: saved
        state?.let { outState.putBundle(STATE_SAVED, it.toBundle()) }
        outState.putBoolean(STATE_PLAYING, controller?.state?.value?.phase?.wantsToPlay() ?: wasPlaying)
    }

    /**
     * The engine of the playback showing gets this window (its display mode)
     * and the media session, once there is one: the controller makes its
     * engine as it starts, so usually at once.
     */
    private fun attachEngine() {
        val engine = controller?.engine ?: return
        if (engine === attached) return
        if (!lifecycle.currentState.isAtLeast(Lifecycle.State.STARTED)) return
        attached?.detachWindow()
        session?.stop()
        engine.attachWindow(window)
        attached = engine
        session = VideoMediaSession(this, engine.player).also { it.start() }
    }

    /**
     * Keeps the screen on only while playing or about to; goes back to where
     * playing started when the playback ends with nothing next. Attaches the
     * engine if it came after the window.
     */
    private fun followPlayback() {
        lifecycleScope.launch {
            repeatOnLifecycle(Lifecycle.State.STARTED) {
                val playing = controller ?: return@repeatOnLifecycle
                playing.state.map { it.phase }.distinctUntilChanged().collect { phase ->
                    attachEngine()
                    if (phase.wantsToPlay()) {
                        window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
                    } else {
                        window.clearFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
                    }
                    if (phase == PlayerPhase.Ended) finish()
                }
            }
        }
    }

    /** A size, background, font or position chosen in the Subtitles panel: Settings' own, so every video has it from now on. */
    private fun chooseStyle(look: CaptionLook) {
        lifecycleScope.launch {
            graph.data.settings.update {
                it.copy(
                    subtitleSize = look.textFraction,
                    subtitleBackground = if (look.box) SubtitleBackground.DarkBox else SubtitleBackground.None,
                    subtitleFont = look.font,
                    subtitleRaise = look.raise,
                )
            }
        }
    }

    /** Subtitles as Settings → Subtitles says (size and background), and whether Playback details show. */
    private fun followSettings() {
        lifecycleScope.launch {
            repeatOnLifecycle(Lifecycle.State.STARTED) {
                graph.data.settings.settings.map { CaptionLook.of(it) to it.showPlaybackDetails }.distinctUntilChanged().collect { (look, details) ->
                    captionLook = look
                    showDetails = details
                }
            }
        }
    }

    companion object {
        /** The key PlaybackServices.prepare gave, to find the playback by. */
        const val EXTRA_PLAYBACK = "playback"

        private const val STATE_KEY = "pley.player.key"
        private const val STATE_SAVED = "pley.player.saved"
        private const val STATE_PLAYING = "pley.player.playing"

        fun intent(context: Context, playback: String): Intent =
            Intent(context, TvPlayerActivity::class.java).putExtra(EXTRA_PLAYBACK, playback)
    }
}
