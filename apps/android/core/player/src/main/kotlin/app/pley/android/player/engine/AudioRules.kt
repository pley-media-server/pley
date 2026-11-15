package app.pley.android.player.engine

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.media.AudioDeviceCallback
import android.media.AudioDeviceInfo
import android.media.AudioFocusRequest
import android.media.AudioManager
import android.os.Handler
import android.os.Looper
import androidx.annotation.OptIn
import androidx.core.content.ContextCompat
import androidx.media3.common.PlaybackException
import androidx.media3.common.util.UnstableApi
import androidx.media3.exoplayer.audio.AudioSink

/** What the focus rules do to the player. */
interface FocusTarget {
    fun pause()

    fun play()

    fun setVolume(volume: Float)
}

/** Android's audio focus, behind a seam for tests. */
interface FocusPort {
    /** Asks for focus; `true` when granted. Changes come to [onChange] (AudioManager.AUDIOFOCUS_*). */
    fun request(onChange: (Int) -> Unit): Boolean

    fun abandon()
}

/**
 * Audio focus for video, handled by pley rather than Media3, because Media3
 * ducks where passthrough can't: encoded audio (AC3, E-AC3, TrueHD, DTS)
 * goes to the receiver as it is, and its volume can't be lowered. So while
 * it passes through, a "may duck" loss pauses instead, and playback comes
 * back with the focus (Amazon's multimedia guideline 4.7). Focus is asked
 * for right before playing, kept through pauses, and given back on release
 * or for good when another app takes it.
 *
 * A live channel has no pause, and its player plays on whatever paused it:
 * paused for another app's sound, it played over it at once, and took the
 * focus back from music or a video. It goes quiet instead, its sound back
 * with the focus, or, when another app took it for good, as a channel is
 * tuned.
 */
class AudioFocusRules(private val port: FocusPort, private val target: FocusTarget) {
    private var held = false
    private var resumeOnGain = false
    private var ducked = false

    /** A live channel, quiet without the focus. */
    private var quiet = false

    /** Encoded audio goes to the output as it is. */
    var passthrough = false

    /** A live channel plays: losing the focus quiets it rather than pausing it. */
    var live = false
        set(value) {
            field = value
            if (!value && quiet) restoreVolume()
        }

    /**
     * Before playing: `false` when focus was refused (a phone call), and it
     * shouldn't play. A live channel always plays, heard only with the focus.
     */
    fun beforePlay(): Boolean {
        if (live) {
            // Lost for now (a call): quiet until the focus comes back.
            if (resumeOnGain) return true
            if (!held) held = port.request(::onFocusChange)
            if (!held) goQuiet() else if (quiet) restoreVolume()
            return true
        }
        resumeOnGain = false
        if (held) return true
        held = port.request(::onFocusChange)
        return held
    }

    /** Gives the focus back (the player closes). */
    fun release() {
        if (held) port.abandon()
        held = false
        resumeOnGain = false
        restoreVolume()
    }

    fun onFocusChange(change: Int) {
        when (change) {
            AudioManager.AUDIOFOCUS_GAIN -> {
                restoreVolume()
                if (resumeOnGain) {
                    resumeOnGain = false
                    if (!live) target.play()
                }
            }
            AudioManager.AUDIOFOCUS_LOSS -> {
                held = false
                resumeOnGain = false
                port.abandon()
                if (live) goQuiet() else target.pause()
            }
            AudioManager.AUDIOFOCUS_LOSS_TRANSIENT -> pauseForNow()
            AudioManager.AUDIOFOCUS_LOSS_TRANSIENT_CAN_DUCK -> if (passthrough) {
                pauseForNow()
            } else {
                ducked = true
                if (!quiet) target.setVolume(DUCKED_VOLUME)
            }
        }
    }

    /** The person paused: no coming back on its own when focus returns. A live channel plays on, and stays quiet until then. */
    fun onPausedByPerson() {
        if (!live) resumeOnGain = false
    }

    private fun pauseForNow() {
        resumeOnGain = true
        if (live) goQuiet() else target.pause()
    }

    private fun goQuiet() {
        if (quiet) return
        quiet = true
        target.setVolume(0f)
    }

    private fun restoreVolume() {
        if (ducked || quiet) {
            ducked = false
            quiet = false
            target.setVolume(1f)
        }
    }

    companion object {
        const val DUCKED_VOLUME = 0.2f
    }
}

/** [FocusPort] on AudioManager, for movies; ducking is pley's to do (willPauseWhenDucked). */
class AndroidFocusPort(context: Context) : FocusPort {
    private val audio = context.applicationContext.getSystemService(AudioManager::class.java)
    private val main = Handler(Looper.getMainLooper())
    private var request: AudioFocusRequest? = null

    override fun request(onChange: (Int) -> Unit): Boolean {
        val manager = audio ?: return true
        val attributes = android.media.AudioAttributes.Builder()
            .setUsage(android.media.AudioAttributes.USAGE_MEDIA)
            .setContentType(android.media.AudioAttributes.CONTENT_TYPE_MOVIE)
            .build()
        val focus = AudioFocusRequest.Builder(AudioManager.AUDIOFOCUS_GAIN)
            .setAudioAttributes(attributes)
            .setWillPauseWhenDucked(true)
            .setOnAudioFocusChangeListener({ onChange(it) }, main)
            .build()
        request = focus
        return manager.requestAudioFocus(focus) == AudioManager.AUDIOFOCUS_REQUEST_GRANTED
    }

    override fun abandon() {
        request?.let { audio?.abandonAudioFocusRequest(it) }
        request = null
    }
}

/**
 * Pauses, keeping the position, when the HDMI output goes: the HDMI audio
 * plug intent says so on TVs (its sticky first delivery aside), and the
 * device callback on everything else.
 */
class HdmiUnplugWatcher(context: Context, private val onUnplugged: () -> Unit) {
    private val app = context.applicationContext
    private val audio = app.getSystemService(AudioManager::class.java)
    private val main = Handler(Looper.getMainLooper())
    private var started = false

    private val plug = object : BroadcastReceiver() {
        override fun onReceive(context: Context, intent: Intent) {
            if (isInitialStickyBroadcast) return
            if (intent.getIntExtra(AudioManager.EXTRA_AUDIO_PLUG_STATE, 1) == 0) onUnplugged()
        }
    }

    private val devices = object : AudioDeviceCallback() {
        override fun onAudioDevicesRemoved(removedDevices: Array<out AudioDeviceInfo>) {
            if (removedDevices.any { it.isSink && it.type in HDMI }) onUnplugged()
        }
    }

    fun start() {
        if (started) return
        started = true
        ContextCompat.registerReceiver(app, plug, IntentFilter(AudioManager.ACTION_HDMI_AUDIO_PLUG), ContextCompat.RECEIVER_NOT_EXPORTED)
        audio?.registerAudioDeviceCallback(devices, main)
    }

    fun stop() {
        if (!started) return
        started = false
        app.unregisterReceiver(plug)
        audio?.unregisterAudioDeviceCallback(devices)
    }

    private companion object {
        // TYPE_HDMI, TYPE_HDMI_ARC, TYPE_HDMI_EARC (29, Android 12).
        val HDMI = setOf(AudioDeviceInfo.TYPE_HDMI, AudioDeviceInfo.TYPE_HDMI_ARC, 29)
    }
}

/**
 * When the audio output fails under the player (an AudioTrack write
 * returning -32, dead object, before the route change is reported; a
 * passthrough track Fire OS 16 invalidated), the engine prepares again at
 * the same position, a few times a minute at most.
 */
@OptIn(markerClass = [UnstableApi::class])
class AudioRestarts(private val now: () -> Long, private val perMinute: Int = 3) {
    private val recent = ArrayDeque<Long>()

    /** Whether to prepare again after [error]; counts the restart when so. */
    fun shouldRestart(error: PlaybackException): Boolean {
        if (!isAudioOutputFailure(error)) return false
        val t = now()
        while (recent.isNotEmpty() && t - recent.first() > WINDOW_MS) recent.removeFirst()
        if (recent.size >= perMinute) return false
        recent.addLast(t)
        return true
    }

    companion object {
        const val WINDOW_MS = 60_000L

        /** AudioTrack's ERROR_DEAD_OBJECT. */
        const val DEAD_OBJECT = -32

        fun isAudioOutputFailure(error: PlaybackException): Boolean {
            if (error.errorCode == PlaybackException.ERROR_CODE_AUDIO_TRACK_INIT_FAILED) return true
            var cause: Throwable? = error
            while (cause != null) {
                if (cause is AudioSink.WriteException) return cause.errorCode == DEAD_OBJECT || cause.isRecoverable
                cause = cause.cause
            }
            return error.errorCode == PlaybackException.ERROR_CODE_AUDIO_TRACK_WRITE_FAILED
        }
    }
}
