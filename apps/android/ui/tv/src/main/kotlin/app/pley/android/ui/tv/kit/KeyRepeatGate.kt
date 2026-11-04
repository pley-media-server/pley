package app.pley.android.ui.tv.kit

import android.os.SystemClock
import android.view.KeyEvent as NativeKeyEvent
import android.view.View
import android.view.ViewConfiguration
import androidx.compose.animation.core.FiniteAnimationSpec
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.tween
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Modifier
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEvent
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.KeyInputModifierNode
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.type
import androidx.compose.ui.node.ModifierNodeElement
import androidx.compose.ui.node.requireView
import androidx.compose.ui.platform.InspectorInfo
import app.pley.android.designsystem.PleyMotion
import app.pley.android.designsystem.image.ArtworkHold
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

/**
 * D-pad rule 5, held keys (tv-addendum §3.5), as the native Samsung app
 * has them. A press moves focus once. Held for [HOLD_DELAY_MS], the key
 * moves focus by itself on the app's own clock, every [MOVE_INTERVAL_MS]
 * (about 22 a second): sooner and more evenly than the system's repeats,
 * which begin after half a second, come unevenly and only say the key is
 * still down, so the gate drops them. While a key is held ([holding]):
 * - a row slides on under a ring that stays still, and lands where the
 *   key comes up ([TvRow]);
 * - tiles change their focus look evenly over [GLIDE_MS] instead of easing;
 * - backdrops and details wait until focus has rested [REST_MS]
 *   ([awaitRest], [rememberRested]);
 * - pictures wait unless the memory cache has them: the gate is the
 *   window's [ArtworkHold], and where the key comes up loads them.
 *
 * A hold ends when its key comes up, another direction goes down, or no
 * sign of the key comes for [LOST_KEY_UP_MS] (the window lost the key-up):
 * [onRelease] and [awaitRelease] say so. Other keys pass by it, as on the
 * Samsung app: what they do under a slide is the row's ([TvRow]).
 *
 * One per window, at the root ([keyRepeatGate]), which runs its clock.
 * [clock] is the uptime in milliseconds and [firstRepeatMs] how long the
 * system waits before its first repeat (the device's setting), both faked
 * in tests.
 */
@Stable
class KeyRepeatGate(
    private val firstRepeatMs: Long = ViewConfiguration.getKeyRepeatTimeout().toLong(),
    private val clock: () -> Long = SystemClock::uptimeMillis,
) : ArtworkHold {
    /** Whether a direction key is held down and moving focus by itself now. Read it while drawing, or in effects. */
    override var holding: Boolean by mutableStateOf(false)
        private set

    /** The direction key down now, as it went down: what the held moves repeat. */
    private var press: NativeKeyEvent? = null
    private var pressedAt = 0L

    /** Whether the system has repeated [press] yet: until then its first repeat may take [firstRepeatMs]. */
    private var repeated = false

    /** The held moves made since [press]: each is a repeat, counted. */
    private var moves = 0
    private var nextMove = 0L

    /** When focus last moved, or a direction key was let go; [Long.MIN_VALUE] before either. */
    private var lastMove = Long.MIN_VALUE / 2

    /** When a direction key last went down or repeated: a hold whose key-up never came ends on its own. */
    private var lastKey = Long.MIN_VALUE / 2

    /** When any key last went down: what pops up waits for the remote to rest ([quietFor]). */
    private var lastPress = Long.MIN_VALUE / 2

    /** The held move being sent now ([send]), which [admit] lets through. */
    private var sending: NativeKeyEvent? = null

    /** Completed when the hold ends. */
    @Volatile
    private var released: CompletableDeferred<Unit>? = null

    /** What runs as the hold ends ([onRelease]). */
    private val releases = ArrayList<() -> Unit>(2)

    /** Whether a direction key is down, held or not yet: [tick] is worth calling. */
    val down: Boolean get() = press != null

    /**
     * Whether [event] goes through: true lets it act, false drops it. A
     * press of a direction goes through; its repeats don't, as [tick]
     * moves focus while it is held. Only direction keys are gated.
     */
    fun admit(event: KeyEvent): Boolean {
        if (event.type == KeyEventType.KeyDown) lastPress = clock()
        if (event.key !in DIRECTIONS) return true
        val native = event.nativeKeyEvent
        if (native === sending) return true
        val now = clock()
        return when (event.type) {
            KeyEventType.KeyDown -> {
                lastKey = now
                if (native.repeatCount == 0) {
                    // Another direction's press ends the hold before it.
                    endHold(now)
                    press = native
                    pressedAt = now
                    repeated = false
                    moves = 0
                    lastMove = now
                    true
                } else {
                    // A key held down as the window got focus repeats without a press it saw: held already.
                    if (press?.keyCode != native.keyCode) {
                        endHold(now)
                        press = native
                        pressedAt = now - HOLD_DELAY_MS
                        moves = 0
                    }
                    repeated = true
                    // Frames stalled: the repeat starts the hold the clock didn't.
                    if (!holding && now - pressedAt >= HOLD_DELAY_MS) startHold(now)
                    false
                }
            }
            KeyEventType.KeyUp -> {
                if (press == null || press?.keyCode == native.keyCode) {
                    press = null
                    endHold(now)
                    lastMove = now
                }
                true
            }
            else -> true
        }
    }

    /** How long it is, in milliseconds, since a key went down, any key ([touched] counts as one). */
    fun quietFor(): Long = clock() - lastPress

    /**
     * The held move due now, as the key event to send ([send]); null when
     * none is. Called every frame or so while a key is [down]: moves come
     * [MOVE_INTERVAL_MS] apart on average, whenever the frames fall.
     */
    fun tick(): NativeKeyEvent? {
        val now = clock()
        expireHold(now)
        val key = press ?: return null
        if (!holding) {
            // Down long enough: the hold begins, sooner than the system's repeats would.
            if (now - pressedAt < HOLD_DELAY_MS) return null
            startHold(now)
        }
        if (now < nextMove) return null
        // Kept to the pace: a late frame doesn't push the next move later, nor does a stall bring a burst.
        nextMove = maxOf(nextMove + MOVE_INTERVAL_MS, now + MOVE_INTERVAL_MS / 2)
        lastMove = now
        moves++
        return NativeKeyEvent.changeTimeRepeat(key, now, moves)
    }

    /** Sends [move] ([tick]'s) to the window's [view] as the key it repeats, past the gate. */
    fun send(move: NativeKeyEvent, view: View) {
        sending = move
        try {
            view.dispatchKeyEvent(move)
        } finally {
            sending = null
        }
    }

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
        expireHold(clock())
        return !holding && clock() - lastMove >= REST_MS
    }

    /** Returns once focus has rested ([rested]); at once if it already has. */
    suspend fun awaitRest() {
        while (true) {
            expireHold(clock())
            val left = lastMove + REST_MS - clock()
            if (!holding && left <= 0) return
            delay(if (holding || left > POLL_MS) POLL_MS else left)
        }
    }

    /**
     * Returns once the hold is over, the signal that it ended: the key came
     * up, another went down, or its key-up was lost. At once when no key is
     * held.
     */
    override suspend fun awaitRelease() {
        released?.await()
    }

    /**
     * Runs [action] once, as the hold ends: in the key event that ends it,
     * before that event goes on, so another key's press moves focus from
     * where a slide landed. At once when no key is held.
     */
    fun onRelease(action: () -> Unit) {
        if (!holding) action() else if (action !in releases) releases += action
    }

    private fun startHold(now: Long) {
        // Before holding: a picture loader on its own thread that sees the hold waits on this, not on nothing.
        released = CompletableDeferred()
        // The first held move is due at once.
        nextMove = now
        holding = true
    }

    private fun endHold(now: Long) {
        if (!holding) return
        holding = false
        lastMove = now
        released?.complete(Unit)
        released = null
        if (releases.isNotEmpty()) {
            // A copy: what runs may ask again.
            val run = releases.toTypedArray()
            releases.clear()
            run.forEach { it() }
        }
    }

    /**
     * The window lost the key-up (focus left the app mid-hold): the hold is
     * over. The system repeats a held key every 50 ms, after [firstRepeatMs]
     * for the first.
     */
    private fun expireHold(now: Long) {
        if (press == null) return
        val quiet = if (repeated) LOST_KEY_UP_MS else firstRepeatMs + LOST_KEY_UP_MS
        if (now - lastKey > quiet) {
            press = null
            endHold(lastKey)
            lastMove = lastKey
        }
    }

    companion object {
        /**
         * How long a key is down before it moves focus by itself. A remote's
         * press can last 200 ms and more: at 180 ms, such a press moved focus
         * twice. Still sooner than the system's repeats.
         */
        const val HOLD_DELAY_MS = 300L

        /** The time between moves while a key is held: about 22 a second. */
        const val MOVE_INTERVAL_MS = 45L

        /** How long a held move's glide lasts: a little over [MOVE_INTERVAL_MS], so the next starts before it ends. */
        const val GLIDE_MS = (MOVE_INTERVAL_MS * 5 / 4).toInt()

        /** How long focus rests before backdrops and details follow it. */
        const val REST_MS = PleyMotion.REST_MS.toLong()

        private const val POLL_MS = 50L

        /** Repeats come every 50 ms; this long without one means the key is up. */
        private const val LOST_KEY_UP_MS = 500L

        private val DIRECTIONS = setOf(Key.DirectionUp, Key.DirectionDown, Key.DirectionLeft, Key.DirectionRight)

        /** A held move's glide: [GLIDE_MS], evenly. */
        fun <T> glide(): FiniteAnimationSpec<T> = tween(GLIDE_MS, easing = LinearEasing)
    }
}

/** The window's gate: the shell provides one at its root. */
val LocalKeyRepeatGate = staticCompositionLocalOf { KeyRepeatGate() }

/**
 * Puts [gate] in front of every key this layout's children get, and runs
 * its clock: once a frame while a direction key is down, the move due is
 * sent to the window as the key it repeats. Once, at the root.
 */
fun Modifier.keyRepeatGate(gate: KeyRepeatGate): Modifier = this then KeyRepeatGateElement(gate)

private data class KeyRepeatGateElement(val gate: KeyRepeatGate) : ModifierNodeElement<KeyRepeatGateNode>() {
    override fun create() = KeyRepeatGateNode(gate)

    override fun update(node: KeyRepeatGateNode) {
        node.gate = gate
    }

    override fun InspectorInfo.inspectableProperties() {
        name = "keyRepeatGate"
    }
}

private class KeyRepeatGateNode(var gate: KeyRepeatGate) : Modifier.Node(), KeyInputModifierNode {
    private var clock: Job? = null

    override fun onPreKeyEvent(event: KeyEvent): Boolean {
        val admitted = gate.admit(event)
        if (gate.down && clock?.isActive != true) clock = coroutineScope.launch { run(gate) }
        return !admitted
    }

    override fun onKeyEvent(event: KeyEvent): Boolean = false

    /** The gate's clock, while a key is down: the move due each frame, sent to the window. */
    private suspend fun run(gate: KeyRepeatGate) {
        val view = requireView()
        val onFrame: (Long) -> NativeKeyEvent? = { gate.tick() }
        while (gate.down) {
            val move = withFrameNanos(onFrame) ?: continue
            gate.send(move, view)
        }
    }
}

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
