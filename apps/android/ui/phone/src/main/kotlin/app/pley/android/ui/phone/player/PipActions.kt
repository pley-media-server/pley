package app.pley.android.ui.phone.player

import android.app.PendingIntent
import android.app.RemoteAction
import android.content.Context
import android.content.Intent
import android.graphics.drawable.Icon
import app.pley.android.ui.phone.R
import app.pley.android.ui.phone.livetv.LivePlayerActivity

/** The buttons picture in picture shows over the video. */
internal object PipActions {
    /** Pause while [playing], Play otherwise; the player screen's receiver does it. */
    fun playPause(context: Context, playing: Boolean): RemoteAction {
        val title = context.getString(if (playing) R.string.player_pause else R.string.player_play)
        val icon = Icon.createWithResource(context, if (playing) android.R.drawable.ic_media_pause else android.R.drawable.ic_media_play)
        return action(context, icon, title, PhonePlayerActivity.ACTION_PLAY_PAUSE, REQUEST_PLAY_PAUSE)
    }

    /** Channel up, or down; the live player's receiver tunes it. */
    fun channel(context: Context, up: Boolean): RemoteAction {
        val title = context.getString(if (up) R.string.live_player_channel_up else R.string.live_player_channel_down)
        val icon = Icon.createWithResource(context, if (up) R.drawable.pip_channel_up else R.drawable.pip_channel_down)
        return if (up) {
            action(context, icon, title, LivePlayerActivity.ACTION_CHANNEL_UP, REQUEST_CHANNEL_UP)
        } else {
            action(context, icon, title, LivePlayerActivity.ACTION_CHANNEL_DOWN, REQUEST_CHANNEL_DOWN)
        }
    }

    private fun action(context: Context, icon: Icon, title: String, broadcast: String, request: Int): RemoteAction {
        val intent = Intent(broadcast).setPackage(context.packageName)
        val pending = PendingIntent.getBroadcast(context, request, intent, PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
        return RemoteAction(icon, title, title, pending)
    }

    private const val REQUEST_PLAY_PAUSE = 1
    private const val REQUEST_CHANNEL_DOWN = 2
    private const val REQUEST_CHANNEL_UP = 3
}
