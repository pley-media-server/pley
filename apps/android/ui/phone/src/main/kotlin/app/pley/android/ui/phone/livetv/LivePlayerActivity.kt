package app.pley.android.ui.phone.livetv

import android.app.PictureInPictureParams
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.content.pm.ActivityInfo
import android.content.res.Configuration
import android.graphics.Rect
import android.os.Build
import android.os.Bundle
import android.view.KeyEvent
import android.view.WindowManager
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.viewModels
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.core.content.ContextCompat
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.WindowInsetsControllerCompat
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.lifecycleScope
import androidx.lifecycle.repeatOnLifecycle
import androidx.media3.common.Player
import androidx.media3.common.VideoSize
import app.pley.android.common.AppLanguage
import app.pley.android.player.CastFrom
import app.pley.android.player.PlayerEngine
import app.pley.android.player.PlayerWindows
import app.pley.android.player.engine.VideoMediaSession
import app.pley.android.player.live.LivePhase
import app.pley.android.player.live.LiveSessionPlayer
import app.pley.android.screens.livetv.LivePlayerViewModel
import app.pley.android.screens.PleyGraphOwner
import app.pley.android.ui.phone.player.PictureInPicture
import app.pley.android.ui.phone.player.PipActions
import app.pley.android.ui.phone.remote.CastNotice
import app.pley.android.ui.phone.remote.CastsSent
import app.pley.android.ui.phone.remote.PlayOnWindow
import app.pley.android.ui.phone.theme.PhoneTheme
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.launch

/**
 * The phone and tablet live player, in a window of its own, as the
 * phone's player: immersive and black, landscape on phones, the screen
 * kept on while it plays. Live has no pause: the media session offers
 * none either ([LiveSessionPlayer]). Picture in picture as the video
 * player's: by itself when leaving while a channel plays or tunes (Android
 * 12 and later; Home on older ones), growing from the picture, with channel
 * down and up. Leaving otherwise, or the screen turned off, lets the
 * channel's connection and the decoders go; back to it, the channel is
 * tuned again at the live edge. One player at a time: opening closes
 * another ([PlayerWindows]). A channel another device sent here says where
 * from as it opens.
 */
class LivePlayerActivity : ComponentActivity() {
    private val vm: LivePlayerViewModel by viewModels {
        LivePlayerViewModel.factory(intent.getLongExtra(EXTRA_CHANNEL, 0), intent.getStringExtra(EXTRA_CATEGORY) ?: "all")
    }
    private var attached: PlayerEngine? = null
    private var mediaSession: VideoMediaSession? = null
    private var screenOn = false
    private val inPip = mutableStateOf(false)

    /** The picture's shape, width over height: 0 until it is known (and for radio). */
    private var videoAspect = 0f
    private var pipKey: Any? = null
    private var lastHint: Rect? = null

    private val pipReceiver = object : BroadcastReceiver() {
        override fun onReceive(context: Context, intent: Intent) {
            when (intent.action) {
                ACTION_CHANNEL_UP -> vm.step(1)
                ACTION_CHANNEL_DOWN -> vm.step(-1)
            }
        }
    }

    private val shapeListener = object : Player.Listener {
        override fun onVideoSizeChanged(videoSize: VideoSize) {
            videoAspect = aspectOf(videoSize)
            updatePip()
        }
    }

    override fun attachBaseContext(newBase: Context) {
        super.attachBaseContext(AppLanguage.wrap(newBase))
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        PlayerWindows.opened(this)
        immersive()
        if (resources.configuration.smallestScreenWidthDp < TABLET_DP) {
            requestedOrientation = ActivityInfo.SCREEN_ORIENTATION_SENSOR_LANDSCAPE
        }
        ContextCompat.registerReceiver(
            this,
            pipReceiver,
            IntentFilter().apply {
                addAction(ACTION_CHANNEL_UP)
                addAction(ACTION_CHANNEL_DOWN)
            },
            ContextCompat.RECEIVER_NOT_EXPORTED,
        )
        // Said once, as it opens: not again when the window is made anew.
        val cast = castFrom(intent).takeIf { savedInstanceState == null }?.notice(resources)
        val remote = (application as PleyGraphOwner).screensGraph.data.remote
        setContent {
            PhoneTheme {
                // Watch on…: the channel goes to the device, and the app's shell says so as this closes.
                PlayOnWindow(
                    remote,
                    onCast = { device, name ->
                        CastsSent.tell(device.deviceId, name)
                        finish()
                    },
                ) {
                    Box(Modifier.fillMaxSize()) {
                        val small = inPip.value
                        LivePlayerScreen(vm = vm, onBack = ::finish, inPip = small, onPip = if (pipSupported) ::enterPip else null)
                        if (!small) CastNotice(cast, Modifier.align(Alignment.TopCenter).windowInsetsPadding(WindowInsets.safeDrawing).padding(top = 64.dp))
                    }
                }
            }
        }
        // Where picture in picture grows from moves with the window (a tablet turned, a split screen), which
        // changes neither the channel's state nor its picture: worked out again as it is laid out.
        window.decorView.addOnLayoutChangeListener { _, left, top, right, bottom, oldLeft, oldTop, oldRight, oldBottom ->
            if (!isInPictureInPictureMode && (right - left != oldRight - oldLeft || bottom - top != oldBottom - oldTop)) updatePip()
        }
        lifecycleScope.launch {
            repeatOnLifecycle(Lifecycle.State.STARTED) {
                combine(vm.engine, vm.state) { engine, state -> engine to state }.collect { (engine, _) ->
                    attach(engine)
                    keepScreenOn(watching())
                    updatePip()
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

    override fun onStart() {
        super.onStart()
        vm.resume()
        immersive()
    }

    override fun onStop() {
        super.onStop()
        detach()
        // Out of sight, the connection goes: in picture in picture too, once the screen turns off.
        if (!isChangingConfigurations) vm.stop()
    }

    override fun onDestroy() {
        PlayerWindows.closed(this)
        runCatching { unregisterReceiver(pipReceiver) }
        detach()
        super.onDestroy()
    }

    // ---------------------------------------------------------------- picture in picture

    private val pipSupported: Boolean by lazy { PictureInPicture.allowed(this) }

    private fun enterPip() {
        if (!pipSupported) return
        runCatching { enterPictureInPictureMode(pipParams()) }
    }

    override fun onUserLeaveHint() {
        super.onUserLeaveHint()
        // Android 12 and later enter by themselves (setAutoEnterEnabled).
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.S && watching()) enterPip()
    }

    override fun onPictureInPictureModeChanged(isInPictureInPictureMode: Boolean, newConfig: Configuration) {
        super.onPictureInPictureModeChanged(isInPictureInPictureMode, newConfig)
        inPip.value = isInPictureInPictureMode
        // Left picture in picture while stopped: its window was closed, and the player goes.
        if (!isInPictureInPictureMode && lifecycle.currentState == Lifecycle.State.CREATED) finish()
    }

    /** A channel plays or is being tuned: the screen stays on, and leaving shrinks it into picture in picture. */
    private fun watching(): Boolean = vm.state.value.phase.let { it == LivePhase.Playing || it == LivePhase.Tuning }

    /** Picture in picture's settings, set again only when they change. */
    private fun updatePip() {
        if (!pipSupported) return
        val next = listOf(watching(), PictureInPicture.aspect(videoAspect), sourceHint())
        if (next == pipKey) return
        pipKey = next
        runCatching { setPictureInPictureParams(pipParams()) }
    }

    // A channel that doesn't play doesn't shrink into a corner by itself.
    private fun pipParams(): PictureInPictureParams = PictureInPicture.params(
        videoAspect,
        sourceHint(),
        listOf(PipActions.channel(this, up = false), PipActions.channel(this, up = true)),
        autoEnter = watching(),
    )

    /** Where picture in picture grows from: the whole picture, fitted to this window. */
    private fun sourceHint(): Rect? {
        // In picture in picture the window is the small one: the hint for coming back stays as it was.
        if (inPip.value) return lastHint
        return PictureInPicture.hint(window.decorView, videoAspect).also { lastHint = it }
    }

    private fun aspectOf(size: VideoSize): Float =
        if (size.width > 0 && size.height > 0) size.width * size.pixelWidthHeightRatio / size.height else 0f

    /** A keyboard's or a remote's channel keys and digits on a tablet, where the screen didn't take them. */
    override fun onKeyDown(keyCode: Int, event: KeyEvent): Boolean {
        when (keyCode) {
            KeyEvent.KEYCODE_CHANNEL_UP, KeyEvent.KEYCODE_PAGE_UP -> {
                vm.step(1)
                return true
            }
            KeyEvent.KEYCODE_CHANNEL_DOWN, KeyEvent.KEYCODE_PAGE_DOWN -> {
                vm.step(-1)
                return true
            }
            // Live has no pause, no going back: these do nothing here.
            KeyEvent.KEYCODE_MEDIA_PLAY_PAUSE, KeyEvent.KEYCODE_MEDIA_PLAY, KeyEvent.KEYCODE_MEDIA_PAUSE, KeyEvent.KEYCODE_SPACE,
            KeyEvent.KEYCODE_MEDIA_REWIND, KeyEvent.KEYCODE_MEDIA_FAST_FORWARD,
            -> return true
        }
        val digit = event.unicodeChar.toChar()
        if (digit.isDigit()) {
            vm.digit(digit)
            return true
        }
        return super.onKeyDown(keyCode, event)
    }

    /** The engine, once there is one: this window for it, and the media session while started. */
    private fun attach(engine: PlayerEngine?) {
        if (attached === engine) return
        detach()
        if (engine == null || !lifecycle.currentState.isAtLeast(Lifecycle.State.STARTED)) return
        attached = engine
        engine.attachWindow(window)
        mediaSession = VideoMediaSession(this, LiveSessionPlayer(engine.player)).also { it.start() }
        engine.player.addListener(shapeListener)
        videoAspect = aspectOf(engine.player.videoSize)
    }

    private fun detach() {
        mediaSession?.stop()
        mediaSession = null
        attached?.player?.removeListener(shapeListener)
        attached?.detachWindow()
        attached = null
    }

    private fun keepScreenOn(on: Boolean) {
        if (on == screenOn) return
        screenOn = on
        if (on) window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON) else window.clearFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
    }

    companion object {
        /** The channel to tune as it opens. */
        const val EXTRA_CHANNEL = "channel"

        /** The category it was chosen in (a LiveCategory's key): channel up and down stay in it. */
        const val EXTRA_CATEGORY = "category"

        /** Sent here by a remote on another device ([CastFrom]): whether, and from which device. */
        const val EXTRA_CAST = "cast"
        const val EXTRA_CAST_FROM = "cast_from"

        /** Picture in picture's channel buttons ([PipActions.channel]). */
        internal const val ACTION_CHANNEL_UP = "app.pley.android.livetv.CHANNEL_UP"
        internal const val ACTION_CHANNEL_DOWN = "app.pley.android.livetv.CHANNEL_DOWN"

        private const val TABLET_DP = 600

        fun intent(context: Context, channelId: Long, category: String, castFrom: CastFrom? = null): Intent =
            Intent(context, LivePlayerActivity::class.java)
                .putExtra(EXTRA_CHANNEL, channelId)
                .putExtra(EXTRA_CATEGORY, category)
                .putExtra(EXTRA_CAST, castFrom != null)
                .putExtra(EXTRA_CAST_FROM, castFrom?.device)

        /** Where the channel [intent] opens was sent from; `null` when it wasn't. */
        internal fun castFrom(intent: Intent): CastFrom? =
            CastFrom(intent.getStringExtra(EXTRA_CAST_FROM)).takeIf { intent.getBooleanExtra(EXTRA_CAST, false) }
    }
}
