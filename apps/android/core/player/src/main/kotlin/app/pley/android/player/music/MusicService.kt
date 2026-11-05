package app.pley.android.player.music

import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.view.KeyEvent
import androidx.annotation.OptIn
import androidx.core.content.IntentCompat
import androidx.media3.common.util.UnstableApi
import androidx.media3.session.MediaNotification
import androidx.media3.session.MediaSession
import androidx.media3.session.MediaSessionService
import app.pley.android.common.AppLanguage

/**
 * Music in the background (Android 17 plays background audio only from a
 * foreground service): a Media3 media session service, foreground with
 * type mediaPlayback while music plays, which shows the notification and
 * the lock screen's controls. It plays the app's one [MusicPlayback]
 * ([MusicHub]); with nothing to play, it takes no connections and stops.
 *
 * Only the app itself, the system (its media controls, the notification,
 * Bluetooth and the lock screen) and Media3's notification controller may
 * connect: no other app drives Pley's music. A remote's Stop key pauses
 * the music ([StopKey]).
 */
@OptIn(markerClass = [UnstableApi::class])
class MusicService : MediaSessionService() {
    private var session: MediaSession? = null

    // The notification's words, Media3's included, in the language shown.
    override fun attachBaseContext(base: Context) {
        super.attachBaseContext(AppLanguage.wrap(base))
    }

    override fun onCreate() {
        super.onCreate()
        sessionOrMake()
    }

    override fun onGetSession(controllerInfo: MediaSession.ControllerInfo): MediaSession? = sessionOrMake()

    /**
     * The session over the process's music, made again when music plays
     * after a stop (the service may still be bound then); `null` with
     * nothing to play.
     */
    private fun sessionOrMake(): MediaSession? {
        session?.let { return it }
        val playback = MusicHub.playback ?: return null
        if (!playback.active) return null
        val output = playback.output() as? EngineMusicOutput ?: return null
        val made = MediaSession.Builder(this, QueuePlayer(output.player, playback))
            .setId(SESSION_ID)
            .setCallback(OnlyOurs(packageName, StopKey(playback::pause)))
            .apply { openApp()?.let(::setSessionActivity) }
            .build()
        session = made
        playback.onStopped = {
            playback.onStopped = null
            if (session === made) session = null
            made.release()
            stopSelf()
        }
        return made
    }

    override fun onDestroy() {
        MusicHub.playback?.onStopped = null
        session?.release()
        session = null
        super.onDestroy()
    }

    /**
     * Tapping the notification opens the app as its icon does: the app's
     * task comes back as it was, with no second window over it (on a TV,
     * MainActivity hands over to the TV's window already open).
     */
    private fun openApp(): PendingIntent? = packageManager.getLaunchIntentForPackage(packageName)?.let {
        PendingIntent.getActivity(this, 0, it, PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
    }

    /** Accepts the app, the system's controllers and Media3's notification; refuses everyone else. */
    private class OnlyOurs(private val own: String, private val stop: StopKey) : MediaSession.Callback {
        override fun onConnect(session: MediaSession, controller: MediaSession.ControllerInfo): MediaSession.ConnectionResult =
            if (allowed(session, controller)) {
                super.onConnect(session, controller)
            } else {
                MediaSession.ConnectionResult.reject()
            }

        override fun onMediaButtonEvent(session: MediaSession, controllerInfo: MediaSession.ControllerInfo, intent: Intent): Boolean =
            stop.handle(intent) || super.onMediaButtonEvent(session, controllerInfo, intent)

        private fun allowed(session: MediaSession, controller: MediaSession.ControllerInfo): Boolean =
            controller.packageName == own ||
                session.isMediaNotificationController(controller) ||
                controller.isTrusted
    }

    private companion object {
        const val SESSION_ID = "pley.music"
    }
}

/**
 * The Stop key of a TV remote or a headset, as the Samsung app has it: the
 * music pauses and keeps its queue, where Media3 would stop it and forget
 * the queue (Now Playing would leave the rail). The notification swiped
 * away comes as Stop too, marked as its dismissal: that still ends the music.
 */
@OptIn(markerClass = [UnstableApi::class])
internal class StopKey(private val pause: () -> Unit) {
    /** Whether [intent], a media button, is the Stop key, handled here. */
    fun handle(intent: Intent): Boolean {
        val key = IntentCompat.getParcelableExtra(intent, Intent.EXTRA_KEY_EVENT, KeyEvent::class.java) ?: return false
        if (key.keyCode != KeyEvent.KEYCODE_MEDIA_STOP) return false
        if (intent.getBooleanExtra(MediaNotification.NOTIFICATION_DISMISSED_EVENT_KEY, false)) return false
        // The key's release and its repeats are handled too, and change nothing.
        if (key.action == KeyEvent.ACTION_DOWN && key.repeatCount == 0) pause()
        return true
    }
}

/**
 * The process's music, for the service: the player graph puts its
 * [MusicPlayback] here when it makes it. A service the system starts in a
 * process without one (after process death) has nothing to play.
 */
object MusicHub {
    @Volatile
    var playback: MusicPlayback? = null
}
