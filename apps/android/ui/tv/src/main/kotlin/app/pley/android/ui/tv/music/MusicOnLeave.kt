package app.pley.android.ui.tv.music

import app.pley.android.player.MusicControl

/**
 * Music when the person leaves the TV app. Android TV's guidelines let
 * audio play on after Home only with the system's media controls on the
 * home screen, and Google TV's has none: music playing there could only
 * be stopped by opening the app again. So Home or another app pauses
 * music that plays, keeping the queue and the place, and it plays on from
 * there when the app is back, as on Samsung TVs. The last Back pauses it
 * for good, and a Watch Next program that brings the app back plays its
 * video instead ([MusicControl.pauseForLeave]). What is left to do is
 * kept with the music, so an activity made anew meanwhile still plays it
 * on.
 *
 * Only a person's choice to go counts (onUserLeaveHint, which the
 * screensaver doesn't give, then onStop within [HINT_MS]): music plays on
 * under the screensaver, also one that starts while a window is left open
 * over the app (the hint came then, with no onStop after it).
 *
 * The activity calls [leaving], [stopped] and [resumed]; [now] is the
 * uptime clock; [music] is `null` until the app's music is made. Main
 * thread.
 */
class MusicOnLeave(private val now: () -> Long, private val music: () -> MusicControl?) {
    /** When the person chose to go, until the activity stops or comes back. */
    private var leftAt: Long? = null

    /** onUserLeaveHint: Home, or another app, chosen by the person. */
    fun leaving() {
        leftAt = now()
    }

    /** onStop: [finishing] is the last Back. */
    fun stopped(finishing: Boolean) {
        val chose = leftAt?.let { now() - it <= HINT_MS } == true
        leftAt = null
        if (chose || finishing) music()?.pauseForLeave(playOnReturn = !finishing)
    }

    /**
     * onResume: back from Home or another app, or a window over the app
     * closed. [videoNext] when a Watch Next program brought the app back.
     */
    fun resumed(videoNext: Boolean) {
        leftAt = null
        music()?.returnAfterLeave(playOn = !videoNext)
    }

    companion object {
        /**
         * How soon onStop follows the hint when the person goes: Android
         * stops the app once the screen that took over settles, which can
         * take its 10 s idle timeout on a slow launcher. The screensaver
         * comes minutes later.
         */
        const val HINT_MS = 15_000L
    }
}
