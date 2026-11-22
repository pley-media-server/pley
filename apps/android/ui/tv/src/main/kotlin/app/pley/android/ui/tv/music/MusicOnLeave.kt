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
 * screensaver doesn't give, then onStop): music plays on under the
 * screensaver. A window over the app (the TV's settings panel) gives the
 * hint when it opens, without stopping the app; the app stops later when
 * Home is pressed from it, with no hint of its own, or when the
 * screensaver starts under it. So a hint older than [HINT_MS] waits
 * [DREAM_WAIT_MS] for the screensaver's broadcast ([dreaming]) before the
 * music pauses.
 *
 * The activity calls [leaving], [stopped], [resumed] and [dreaming]; [now]
 * is the uptime clock; [later] runs a block after a delay and returns what
 * cancels it; [music] is `null` until the app's music is made. Main
 * thread.
 */
class MusicOnLeave(
    private val now: () -> Long,
    private val later: (delayMs: Long, block: () -> Unit) -> () -> Unit,
    private val music: () -> MusicControl?,
) {
    /** When the person chose to go, until the activity stops or comes back. */
    private var leftAt: Long? = null

    /** The screensaver is on. */
    private var dreams = false

    /** Cancels the pause waiting to see whether the screensaver started. */
    private var waiting: (() -> Unit)? = null

    /** onUserLeaveHint: Home, or another app, chosen by the person. */
    fun leaving() {
        leftAt = now()
    }

    /** onStop: [finishing] is the last Back. */
    fun stopped(finishing: Boolean) {
        val hint = leftAt
        leftAt = null
        when {
            finishing -> music()?.pauseForLeave(playOnReturn = false)
            hint == null || dreams -> Unit
            now() - hint <= HINT_MS -> music()?.pauseForLeave(playOnReturn = true)
            // The screensaver starting meanwhile cancels it (dreaming).
            else -> waiting = later(DREAM_WAIT_MS) {
                waiting = null
                music()?.pauseForLeave(playOnReturn = true)
            }
        }
    }

    /**
     * onResume: back from Home or another app, or a window over the app
     * closed. [videoNext] when a Watch Next program brought the app back.
     */
    fun resumed(videoNext: Boolean) {
        leftAt = null
        cancelWaiting()
        music()?.returnAfterLeave(playOn = !videoNext)
    }

    /** The screensaver started ([on]) or stopped. */
    fun dreaming(on: Boolean) {
        dreams = on
        if (on) cancelWaiting()
    }

    private fun cancelWaiting() {
        waiting?.invoke()
        waiting = null
    }

    companion object {
        /**
         * How soon onStop follows the hint when the person goes: Android
         * stops the app once the screen that took over settles, which can
         * take its 10 s idle timeout on a slow launcher.
         */
        const val HINT_MS = 15_000L

        /** How long a stop after an old hint waits for the screensaver's broadcast, which comes as it starts. */
        const val DREAM_WAIT_MS = 3_000L
    }
}
