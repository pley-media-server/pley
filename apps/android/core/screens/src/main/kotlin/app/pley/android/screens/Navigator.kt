package app.pley.android.screens

import app.pley.android.player.CastFrom
import app.pley.android.player.PlayRequest

/**
 * How a screen's ViewModel moves the app elsewhere. Each interface has its
 * own (the phone's back stacks per tab, the TV's one stack with its rail);
 * ViewModels only say where to.
 */
interface Navigator {
    /** Opens [key] above the current screen (a sheet over it, for sheets). */
    fun open(key: PleyKey)

    /** Back one step: what the system's Back does. */
    fun back()

    /** Starts over at [key], forgetting the way there: after signing in or out, or switching profiles. */
    fun restart(key: PleyKey)
}

/**
 * Starts playback. The request goes out at once, while the player screen
 * opens, so the server and the screen get ready together; the phone's and
 * the TV's player screens differ, the request doesn't.
 */
interface PlayerLauncher {
    fun play(request: PlayRequest)

    /**
     * Opens the live player on [channelId] (Live TV), music paused; the
     * player tunes it as it opens, and changes channel within [category]
     * (a LiveCategory's key: "all", "favorites", "recent", "group:…").
     * [castFrom]: a remote on another device sent it here, which the
     * player says. Launchers without Live TV (tests') do nothing.
     */
    fun watchLive(channelId: Long, category: String = "all", castFrom: CastFrom? = null) = Unit
}
