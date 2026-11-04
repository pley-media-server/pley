package app.pley.android.player

import android.app.Activity
import java.lang.ref.WeakReference

/**
 * The player windows open in this process, video and Live TV, the phone's
 * and the TV's: one thing plays at a time, so a player that opens closes
 * the others (an older one in picture in picture, one left under it), and
 * music sent here from a remote closes them all. Called on the main thread.
 */
object PlayerWindows {
    private val open = mutableListOf<WeakReference<Activity>>()

    /** [window] opened: the others close. */
    fun opened(window: Activity) {
        open.mapNotNull { it.get() }.filter { it !== window && !it.isFinishing }.forEach { it.finish() }
        open.clear()
        open += WeakReference(window)
    }

    /** [window] went. */
    fun closed(window: Activity) {
        open.removeAll { it.get() == null || it.get() === window }
    }

    /** Every player window closes: something else plays now. */
    fun closeAll() {
        open.mapNotNull { it.get() }.filter { !it.isFinishing }.forEach { it.finish() }
        open.clear()
    }
}
