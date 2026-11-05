package app.pley.android.player

import android.app.Activity
import java.lang.ref.WeakReference
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.first

/**
 * The player windows open in this process, video and Live TV, the phone's
 * and the TV's: one thing plays at a time, so a player that opens closes
 * the others (an older one in picture in picture, one left under it), and
 * music sent here from a remote closes them all. Called on the main thread.
 */
object PlayerWindows {
    private val open = mutableListOf<WeakReference<Activity>>()

    /** The windows opened that haven't gone yet, those closing too: [gone] waits for none. */
    private val alive = MutableStateFlow(0)

    /** [window] opened: the others close. */
    fun opened(window: Activity) {
        alive.value++
        open.mapNotNull { it.get() }.filter { it !== window && !it.isFinishing }.forEach { it.finish() }
        open.clear()
        open += WeakReference(window)
    }

    /** [window] went. */
    fun closed(window: Activity) {
        open.removeAll { it.get() == null || it.get() === window }
        alive.value = (alive.value - 1).coerceAtLeast(0)
    }

    /** Every player window closes: something else plays now. */
    fun closeAll() {
        open.mapNotNull { it.get() }.filter { !it.isFinishing }.forEach { it.finish() }
        open.clear()
    }

    /**
     * Waits until every player window has gone: what each played has
     * stopped by then, as the windows stop it as they leave the screen.
     */
    suspend fun gone() {
        alive.first { it == 0 }
    }
}
