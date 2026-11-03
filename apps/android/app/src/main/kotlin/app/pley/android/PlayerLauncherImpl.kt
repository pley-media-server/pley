package app.pley.android

import android.content.Context
import android.content.Intent
import app.pley.android.common.DeviceProfile
import app.pley.android.player.PlayRequest
import app.pley.android.player.PlayerGraph
import app.pley.android.screens.PlayerLauncher
import app.pley.android.ui.phone.livetv.LivePlayerActivity
import app.pley.android.ui.phone.player.PhonePlayerActivity
import app.pley.android.ui.tv.livetv.TvLivePlayerActivity
import app.pley.android.ui.tv.player.TvPlayerActivity

/**
 * Starts playback: the request goes out first, so the server answers
 * while the player screen opens; music pauses and ends its session; then
 * the phone's or the TV's player. Live TV opens the phone's or the TV's
 * live player, which tunes the channel itself.
 */
class PlayerLauncherImpl(
    private val context: Context,
    private val player: () -> PlayerGraph,
    private val device: DeviceProfile,
) : PlayerLauncher {
    override fun play(request: PlayRequest) {
        val graph = player()
        val key = graph.services.prepare(request)
        graph.music.pauseForVideo()
        val screen = if (device.isTv) TvPlayerActivity.intent(context, key) else PhonePlayerActivity.intent(context, key)
        context.startActivity(screen.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
    }

    override fun watchLive(channelId: Long, category: String) {
        player().music.pauseForVideo()
        val screen = if (device.isTv) TvLivePlayerActivity.intent(context, channelId, category) else LivePlayerActivity.intent(context, channelId, category)
        context.startActivity(screen.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
    }
}
