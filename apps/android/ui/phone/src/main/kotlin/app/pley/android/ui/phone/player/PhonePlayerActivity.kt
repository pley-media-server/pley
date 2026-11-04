package app.pley.android.ui.phone.player

import android.app.AppOpsManager
import android.app.PictureInPictureParams
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.content.pm.ActivityInfo
import android.content.pm.PackageManager
import android.content.res.Configuration
import android.graphics.Rect
import android.media.AudioManager
import android.os.Build
import android.os.Bundle
import android.os.Process
import android.provider.Settings as SystemSettings
import android.view.WindowManager
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.core.content.ContextCompat
import androidx.core.content.edit
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.WindowInsetsControllerCompat
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.lifecycleScope
import app.pley.android.data.AppSettings
import app.pley.android.data.SessionState
import app.pley.android.data.Settings
import app.pley.android.data.SubtitleBackground
import app.pley.android.player.PlaybackController
import app.pley.android.player.PlaybackServices
import app.pley.android.player.PlayerEngine
import app.pley.android.player.PlayerPhase
import app.pley.android.player.PlayerUiState
import app.pley.android.player.PlayerWindows
import app.pley.android.player.engine.VideoMediaSession
import app.pley.android.player.session.SavedPlaybackState
import app.pley.android.screens.PleyGraphOwner
import app.pley.android.ui.phone.theme.PhoneTheme
import kotlin.math.roundToInt
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeoutOrNull

/**
 * The phone and tablet player, in a window of its own:
 * - immersive and black, landscape on phones (either way up), the screen
 *   kept on only while playing, never rebuilt by turning, folding, a
 *   keyboard or a dark-mode change;
 * - it finds the playback PlayerLauncher started (the server's answer
 *   overlaps this screen's start), or starts it again where it was after
 *   Android killed the app ([SavedPlaybackState]);
 * - picture in picture by itself when leaving while it plays (Android 12
 *   and later; Home on older ones), growing from the video, with play and
 *   pause; never while paused;
 * - leaving otherwise pauses, reports and lets go of the decoders, and
 *   the remote isn't obeyed meanwhile; back within 90 s it carries on with
 *   the same server session, later with a new one at the same place.
 */
class PhonePlayerActivity : ComponentActivity(), PlayerHost {
    private lateinit var services: PlaybackServices
    private var settings: AppSettings? = null
    private var key: String? = null
    private val controller = mutableStateOf<PlaybackController?>(null)
    private val inPip = mutableStateOf(false)
    private val picture = PictureState()
    private var watching: Job? = null
    private var attached: PlayerEngine? = null
    private var mediaSession: VideoMediaSession? = null
    private var pipKey: Any? = null
    private var screenOn = false

    /** Decoders let go in onStop; prepared again in onStart. */
    private var decodersReleased = false
    private var backgroundClose: Job? = null

    /** What to start again with, after 90 s in the background closed the playback. */
    private var resumeLater: SavedPlaybackState? = null

    private val pipReceiver = object : BroadcastReceiver() {
        override fun onReceive(context: Context, intent: Intent) {
            if (intent.action != ACTION_PLAY_PAUSE) return
            val c = controller.value ?: return
            if (isPlaying(c.state.value.phase)) c.pause() else c.play()
        }
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        // One player at a time, as one thing plays at a time: an older one (in picture in picture, or Live TV's) goes.
        PlayerWindows.opened(this)

        immersive()
        if (resources.configuration.smallestScreenWidthDp < TABLET_DP) {
            requestedOrientation = ActivityInfo.SCREEN_ORIENTATION_SENSOR_LANDSCAPE
        }
        val graph = (application as PleyGraphOwner).screensGraph
        services = graph.player.services
        settings = runCatching { graph.data.settings }.getOrNull()

        key = savedInstanceState?.getString(STATE_KEY) ?: intent.getStringExtra(EXTRA_PLAYBACK)
        val found = key?.let(services::controller)
        val saved = SavedPlaybackState.fromBundle(savedInstanceState?.getBundle(STATE_SAVED))?.takeIf { it.itemId > 0 }
        when {
            found != null -> bind(found)
            saved != null -> lifecycleScope.launch {
                // After process death the sign-in is still being read from disk: the playback waits for it.
                withTimeoutOrNull(SIGN_IN_WAIT_MS) { graph.data.session.state.first { it !is SessionState.Starting } }
                restore(saved)?.let(::bind) ?: finish()
            }
            else -> {
                finish()
                return
            }
        }
        loadFill()
        ContextCompat.registerReceiver(this, pipReceiver, IntentFilter(ACTION_PLAY_PAUSE), ContextCompat.RECEIVER_NOT_EXPORTED)

        val settingsFlow: Flow<Settings> = settings?.settings ?: flowOf(Settings())
        setContent {
            PhoneTheme {
                val current = controller.value
                val chosen by settingsFlow.collectAsStateWithLifecycle(Settings())
                if (current == null) {
                    Box(Modifier.fillMaxSize().background(Color.Black))
                } else {
                    key(current) {
                        PhonePlayerScreen(
                            controller = current,
                            host = this@PhonePlayerActivity,
                            picture = picture,
                            captions = PlayerLook.captionLook(chosen),
                            style = PlayerLook.subtitleChoice(chosen),
                            onStyle = ::chooseStyle,
                            inPip = inPip.value,
                            showDetails = chosen.showPlaybackDetails,
                        )
                    }
                }
            }
        }
    }

    /** Edge to edge, the system bars hidden until swiped in, the picture beside the camera cutout too. */
    private fun immersive() {
        WindowCompat.setDecorFitsSystemWindows(window, false)
        WindowInsetsControllerCompat(window, window.decorView).apply {
            hide(WindowInsetsCompat.Type.systemBars())
            systemBarsBehavior = WindowInsetsControllerCompat.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
        }
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
            window.attributes = window.attributes.apply {
                layoutInDisplayCutoutMode = WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_SHORT_EDGES
            }
        }
    }

    override fun onWindowFocusChanged(hasFocus: Boolean) {
        super.onWindowFocusChanged(hasFocus)
        if (hasFocus) immersive()
    }

    /** Starts [saved] again: a new playback where it was. */
    private fun restore(saved: SavedPlaybackState): PlaybackController? {
        if (saved.itemId <= 0) return null
        val restored = services.restore(saved)
        key = restored
        return services.controller(restored)
    }

    /** Shows [c] and follows it: the screen kept on, picture in picture's settings, the end. */
    private fun bind(c: PlaybackController) {
        controller.value = c
        watching?.cancel()
        watching = lifecycleScope.launch {
            c.state.collect { s ->
                keepScreenOn(isPlaying(s.phase))
                if (lifecycle.currentState.isAtLeast(Lifecycle.State.STARTED)) attachEngine()
                updatePip(s)
                if (s.phase == PlayerPhase.Ended) finish()
            }
        }
    }

    private fun keepScreenOn(on: Boolean) {
        if (on == screenOn) return
        screenOn = on
        if (on) window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON) else window.clearFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
    }

    /** The engine, once there is one: this window for it, and the media session while started. */
    private fun attachEngine() {
        val engine = controller.value?.engine ?: return
        if (attached === engine) return
        detachEngine()
        attached = engine
        engine.attachWindow(window)
        mediaSession = VideoMediaSession(this, engine.player).also { it.start() }
    }

    private fun detachEngine() {
        mediaSession?.stop()
        mediaSession = null
        attached?.detachWindow()
        attached = null
    }

    override fun onStart() {
        super.onStart()
        backgroundClose?.cancel()
        backgroundClose = null
        val later = resumeLater
        if (later != null) {
            resumeLater = null
            restore(later)?.let(::bind)
        } else if (decodersReleased) {
            // Within 90 s: the same session, at the same place, still paused.
            controller.value?.engine?.player?.prepare()
        }
        controller.value?.setAway(false)
        decodersReleased = false
        attachEngine()
        immersive()
    }

    override fun onStop() {
        super.onStop()
        detachEngine()
        if (isFinishing || isChangingConfigurations) return
        // Stopped in picture in picture too: the screen turned off, and it is
        // there again, paused, once unlocked. Closed, it ends as it leaves
        // picture in picture (onPictureInPictureModeChanged).
        val c = controller.value ?: return
        c.pause()
        c.setAway(true)
        // Lets go of the decoders and the buffers; the session stays for 90 s.
        c.engine?.player?.let {
            it.stop()
            decodersReleased = true
        }
        backgroundClose = lifecycleScope.launch {
            delay(BACKGROUND_KEEP_MS)
            resumeLater = c.saveState()
            c.close()
            if (controller.value === c) controller.value = null
            decodersReleased = false
        }
    }

    override fun onSaveInstanceState(outState: Bundle) {
        super.onSaveInstanceState(outState)
        outState.putString(STATE_KEY, key)
        val saved = resumeLater ?: controller.value?.saveState()
        if (saved != null && saved.itemId > 0) outState.putBundle(STATE_SAVED, saved.toBundle())
    }

    override fun onDestroy() {
        PlayerWindows.closed(this)
        runCatching { unregisterReceiver(pipReceiver) }
        detachEngine()
        if (isFinishing) {
            backgroundClose?.cancel()
            controller.value?.close()
        }
        super.onDestroy()
    }

    // ---------------------------------------------------------------- picture in picture

    override val pipSupported: Boolean by lazy {
        if (!packageManager.hasSystemFeature(PackageManager.FEATURE_PICTURE_IN_PICTURE)) return@lazy false
        val ops = getSystemService(AppOpsManager::class.java) ?: return@lazy true
        @Suppress("DEPRECATION")
        val mode = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            ops.unsafeCheckOpNoThrow(AppOpsManager.OPSTR_PICTURE_IN_PICTURE, Process.myUid(), packageName)
        } else {
            ops.checkOpNoThrow(AppOpsManager.OPSTR_PICTURE_IN_PICTURE, Process.myUid(), packageName)
        }
        mode == AppOpsManager.MODE_ALLOWED
    }

    override fun enterPip() {
        if (!pipSupported) return
        runCatching { enterPictureInPictureMode(pipParams(isPlaying(controller.value?.state?.value?.phase))) }
    }

    override fun onUserLeaveHint() {
        super.onUserLeaveHint()
        // Android 12 and later enter by themselves (setAutoEnterEnabled).
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.S && isPlaying(controller.value?.state?.value?.phase)) enterPip()
    }

    override fun onPictureInPictureModeChanged(isInPictureInPictureMode: Boolean, newConfig: Configuration) {
        super.onPictureInPictureModeChanged(isInPictureInPictureMode, newConfig)
        inPip.value = isInPictureInPictureMode
        picture.inPip = isInPictureInPictureMode
        // Left picture in picture while stopped: its window was closed, and the playback ends.
        if (!isInPictureInPictureMode && lifecycle.currentState == Lifecycle.State.CREATED) finish()
    }

    /** Picture in picture's settings, set again only when they change. */
    private fun updatePip(s: PlayerUiState) {
        if (!pipSupported) return
        val playing = isPlaying(s.phase)
        val next = listOf(playing, aspect(), sourceHint())
        if (next == pipKey) return
        pipKey = next
        runCatching { setPictureInPictureParams(pipParams(playing)) }
    }

    private fun pipParams(playing: Boolean): PictureInPictureParams {
        val builder = PictureInPictureParams.Builder()
            .setAspectRatio(aspect())
            .setActions(listOf(PipActions.playPause(this, playing)))
        sourceHint()?.let(builder::setSourceRectHint)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            // Only while playing: a paused video doesn't shrink into a corner.
            builder.setAutoEnterEnabled(playing)
            builder.setSeamlessResizeEnabled(false)
        }
        return builder.build()
    }

    /**
     * Where picture in picture grows from: the whole picture, fitted to this
     * window (picture in picture shows all of it, however it was zoomed).
     */
    private fun sourceHint(): Rect? {
        // In picture in picture the window is the small one: the hint for coming back stays as it was.
        if (inPip.value) return lastHint
        val root = window.decorView
        if (root.width == 0 || root.height == 0) return null
        val r = PictureGeometry.place(root.width, root.height, picture.videoAspect, 1f, keepBottom = false)
        return Rect(r.left, r.top, r.right, r.bottom).takeIf { !it.isEmpty }.also { lastHint = it }
    }

    private var lastHint: Rect? = null

    /** The video's shape, within what picture in picture allows (2.39:1 either way). */
    private fun aspect(): android.util.Rational {
        val video = picture.videoAspect.takeIf { it > 0f } ?: (16f / 9f)
        val clamped = video.coerceIn(1f / 2.39f + 0.001f, 2.39f - 0.001f)
        return android.util.Rational((clamped * 1000).roundToInt(), 1000)
    }

    // ---------------------------------------------------------------- the host's other parts

    override fun back() = finish()

    override fun volume(): Float {
        val audio = getSystemService(AudioManager::class.java) ?: return 0f
        val max = audio.getStreamMaxVolume(AudioManager.STREAM_MUSIC).coerceAtLeast(1)
        return audio.getStreamVolume(AudioManager.STREAM_MUSIC).toFloat() / max
    }

    override fun setVolume(level: Float) {
        val audio = getSystemService(AudioManager::class.java) ?: return
        val max = audio.getStreamMaxVolume(AudioManager.STREAM_MUSIC)
        audio.setStreamVolume(AudioManager.STREAM_MUSIC, (level * max).roundToInt().coerceIn(0, max), 0)
    }

    override fun brightness(): Float {
        val own = window.attributes.screenBrightness
        if (own >= 0f) return own
        val system = runCatching { SystemSettings.System.getInt(contentResolver, SystemSettings.System.SCREEN_BRIGHTNESS) }.getOrDefault(128)
        return (system / 255f).coerceIn(0f, 1f)
    }

    /** This window's brightness only: the system's setting is left alone, and comes back on leaving. */
    override fun setBrightness(level: Float) {
        window.attributes = window.attributes.apply { screenBrightness = level.coerceIn(MIN_BRIGHTNESS, 1f) }
    }

    private fun chooseStyle(choice: SubtitleChoice) {
        val store = settings ?: return
        lifecycleScope.launch {
            store.update {
                it.copy(
                    subtitleSize = PlayerLook.subtitleSizes[choice.sizeIndex.coerceIn(0, PlayerLook.subtitleSizes.lastIndex)],
                    subtitleBackground = if (choice.box) SubtitleBackground.DarkBox else SubtitleBackground.None,
                    subtitleFont = choice.font,
                    subtitleRaise = PlayerLook.subtitleRaises[choice.raiseIndex.coerceIn(0, PlayerLook.subtitleRaises.lastIndex)],
                )
            }
        }
    }

    /** Fill or Fit, as last chosen on this device. */
    private fun loadFill() {
        picture.onChosen = { mode ->
            val fill = mode != FitMode.Fit
            lifecycleScope.launch(Dispatchers.IO) { prefs().edit { putBoolean(PREF_FILL, fill) } }
        }
        lifecycleScope.launch {
            val fill = withContext(Dispatchers.IO) { prefs().getBoolean(PREF_FILL, false) }
            if (picture.mode == FitMode.Fit) picture.restore(fill)
        }
    }

    private fun prefs() = getSharedPreferences(PREFS, Context.MODE_PRIVATE)

    companion object {
        /** The key PlaybackServices.prepare gave, to find the playback by. */
        const val EXTRA_PLAYBACK = "playback"

        private const val STATE_KEY = "pley.player.key"
        private const val STATE_SAVED = "pley.player.saved"
        private const val PREFS = "phone_player"
        private const val PREF_FILL = "fill"
        private const val TABLET_DP = 600

        /** The longest a playback restored after process death waits for the sign-in to be read. */
        private const val SIGN_IN_WAIT_MS = 5_000L

        /** How long a playback left in the background keeps its server session. */
        const val BACKGROUND_KEEP_MS = 90_000L

        /** Never quite black: a drag to the bottom leaves the picture visible. */
        private const val MIN_BRIGHTNESS = 0.01f

        internal const val ACTION_PLAY_PAUSE = "app.pley.android.player.PLAY_PAUSE"

        fun intent(context: Context, playback: String): Intent =
            Intent(context, PhonePlayerActivity::class.java).putExtra(EXTRA_PLAYBACK, playback)

        internal fun isPlaying(phase: PlayerPhase?): Boolean =
            phase == PlayerPhase.Playing || phase == PlayerPhase.Buffering || phase == PlayerPhase.Preparing || phase == PlayerPhase.Switching
    }
}
