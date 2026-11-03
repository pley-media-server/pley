package app.pley.android.ui.tv.kit

import android.os.SystemClock
import androidx.compose.runtime.Composable
import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.ui.Modifier
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEvent
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.key.type
import app.pley.android.designsystem.PleyMotion
import kotlinx.coroutines.delay

/**
 * D-pad rule 5, held keys (tv-addendum §3.5). A held key repeats after
 * 400 ms and then every 50 ms; moving focus that fast makes weak TVs drop
 * frames and starts a picture load for every tile passed. So while a key
 * repeats:
 * - at most one focus move goes through every [MOVE_INTERVAL_MS];
 * - rows scroll without animation and tiles snap to their focus scale
 *   without glow ([holding]);
 * - backdrops and details wait until focus has rested [REST_MS]
 *   ([awaitRest], [rememberRested]).
 *
 * One per window, at the root ([keyRepeatGate]); [clock] is the uptime in
 * milliseconds, faked in tests.
 */
@Stable
class KeyRepeatGate(private val clock: () -> Long = SystemClock::uptimeMillis) {
    /** Whether a direction key is held down and repeating now. Read it while drawing, or in effects. */
    var holding: Boolean by mutableStateOf(false)
        private set

    /** When focus last moved, or a direction key was let go; [Long.MIN_VALUE] before either. */
    private var lastMove = Long.MIN_VALUE / 2

    /** When a direction key last went down or repeated: a hold whose key-up never came ends on its own. */
    private var lastKey = Long.MIN_VALUE / 2

    /** When any key last went down: what pops up waits for the remote to rest ([quietFor]). */
    private var lastPress = Long.MIN_VALUE / 2

    /**
     * Whether [event] goes through: true lets it move focus, false drops
     * it. Only direction keys are gated; everything else passes.
     */
    fun admit(event: KeyEvent): Boolean {
        if (event.type == KeyEventType.KeyDown) lastPress = clock()
        if (event.key !in DIRECTIONS) return true
        val now = clock()
        return when (event.type) {
            KeyEventType.KeyDown -> {
                lastKey = now
                val repeat = event.nativeKeyEvent.repeatCount
                if (repeat == 0) {
                    holding = false
                    lastMove = now
                    true
                } else {
                    holding = true
                    if (now - lastMove < MOVE_INTERVAL_MS) {
                        false
                    } else {
                        lastMove = now
                        true
                    }
                }
            }
            KeyEventType.KeyUp -> {
                holding = false
                lastMove = now
                true
            }
            else -> true
        }
    }

    /** How long it is, in milliseconds, since a key went down, any key ([touched] counts as one). */
    fun quietFor(): Long = clock() - lastPress

    /**
     * Counts as a key going down now: the app came back to the front (from
     * the player, Home, the assistant), and the person's next press is
     * meant for the page, whatever was pressed elsewhere meanwhile.
     */
    fun touched() {
        lastPress = clock()
    }

    /** Whether focus has rested: no key held, and nothing moved for [REST_MS]. */
    fun rested(): Boolean {
        expireHold()
        return !holding && clock() - lastMove >= REST_MS
    }

    /** Returns once focus has rested ([rested]); at once if it already has. */
    suspend fun awaitRest() {
        while (true) {
            expireHold()
            val left = lastMove + REST_MS - clock()
            if (!holding && left <= 0) return
            delay(if (holding || left > POLL_MS) POLL_MS else left)
        }
    }

    /** The window lost the key-up (focus left the app mid-hold): the hold is over. */
    private fun expireHold() {
        if (holding && clock() - lastKey > LOST_KEY_UP_MS) {
            holding = false
            lastMove = lastKey
        }
    }

    companion object {
        /** At most one move this often while a key repeats (the TV repeats every 50 ms). */
        const val MOVE_INTERVAL_MS = 100L

        /** How long focus rests before backdrops and details follow it. */
        const val REST_MS = PleyMotion.REST_MS.toLong()

        private const val POLL_MS = 50L

        /** Repeats come every 50 ms; this long without one means the key is up. */
        private const val LOST_KEY_UP_MS = 500L

        private val DIRECTIONS = setOf(Key.DirectionUp, Key.DirectionDown, Key.DirectionLeft, Key.DirectionRight)
    }
}

/** The window's gate: the shell provides one at its root. */
val LocalKeyRepeatGate = staticCompositionLocalOf { KeyRepeatGate() }

/** Puts [gate] in front of every key this layout's children get. Once, at the root. */
fun Modifier.keyRepeatGate(gate: KeyRepeatGate): Modifier = onPreviewKeyEvent { !gate.admit(it) }

/**
 * [value], once focus has rested on it: what backdrops and info blocks
 * show, so they never change while a key repeats or focus passes through.
 * The first value shows at once.
 */
@Composable
fun <T> rememberRested(value: T): T {
    val gate = LocalKeyRepeatGate.current
    var rested by remember { mutableStateOf(value) }
    LaunchedEffect(value) {
        if (value != rested) {
            gate.awaitRest()
            rested = value
        }
    }
    return rested
}
