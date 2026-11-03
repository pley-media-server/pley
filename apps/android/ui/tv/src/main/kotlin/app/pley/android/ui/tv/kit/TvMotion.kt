package app.pley.android.ui.tv.kit

import androidx.compose.animation.core.Easing
import androidx.compose.animation.core.FiniteAnimationSpec
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.animate
import androidx.compose.animation.core.tween
import androidx.compose.foundation.gestures.ScrollableState
import androidx.compose.foundation.gestures.scrollBy
import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusEventModifierNode
import androidx.compose.ui.focus.FocusRequesterModifierNode
import androidx.compose.ui.focus.FocusState
import androidx.compose.ui.focus.requestFocus
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.layout.LayoutCoordinates
import androidx.compose.ui.node.ModifierNodeElement
import androidx.compose.ui.node.requireLayoutCoordinates
import androidx.compose.ui.platform.InspectorInfo
import app.pley.android.designsystem.PleyMotion
import kotlin.math.abs
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.launch

/**
 * The kit's curves, the native Samsung app's (its NUI's built-in alpha
 * functions, Kit/TileView.cs and RowView.cs): a focus look comes in over
 * 150 ms eased out and goes over 120 ms eased in; rows, pages and grids
 * move to a press over 150 ms eased out; under a held key everything
 * moves evenly ([KeyRepeatGate.GLIDE_MS]).
 */
internal object TvMotion {
    /** NUI's EaseOut, a cubic: fast, then slowing to rest. */
    val EaseOut: Easing = Easing { val left = 1f - it; 1f - left * left * left }

    /** NUI's EaseIn, a cubic: slow, then fast. */
    val EaseIn: Easing = Easing { it * it * it }

    /** A focus look coming in. */
    val FocusIn: FiniteAnimationSpec<Float> = tween(PleyMotion.FOCUS_MS, easing = EaseOut)

    /** A focus look going. */
    val FocusOut: FiniteAnimationSpec<Float> = tween(PleyMotion.UNFOCUS_MS, easing = EaseIn)
}

/**
 * How the TV's rows, pages of rows and grids move to what has focus
 * (design spec §2.4, D-pad rules 4 and 5), as the native Samsung app moves
 * them: over 150 ms, eased out, for a press; evenly over
 * [KeyRepeatGate.GLIDE_MS] for a held key's step; at once with reduced
 * motion. Compose's own scrolling to the focused element keeps to no
 * duration, so the kit moves these itself and tells Compose to leave them
 * be ([TvScrolling.itself]). One per row, page or grid.
 *
 * Each move goes from where the last one has got to, in the same run of
 * frames: a Compose animation started again would spend its first frame
 * standing still, a stutter at every held step, where these run into each
 * other as NUI's do on the native Samsung app.
 */
internal class TvGlide(private val state: ScrollableState, private val scope: CoroutineScope) {
    private var distance = 0f
    private var covered = 0f
    private var elapsed = 0L
    private var duration = 1L
    private var easing: Easing = LinearEasing
    private var then: (() -> Unit)? = null

    /** Whether a move asked for is still to run its course. */
    private var asked = false
    private var job: Job? = null

    /** Whether it is moving now. */
    val moving: Boolean get() = asked && job?.isActive == true

    /** Moves [by] pixels from where it is now: a [held] key's step or a press's, or at once when [reduced]. */
    fun move(by: Float, held: Boolean, reduced: Boolean) {
        when {
            abs(by) < 0.5f -> stop()
            reduced -> {
                stop()
                scope.launch { state.scrollBy(by) }
            }
            held -> start(by, KeyRepeatGate.GLIDE_MS, LinearEasing, null)
            else -> start(by, PleyMotion.FOCUS_MS, TvMotion.EaseOut, null)
        }
    }

    /** Moves [by] pixels evenly over [ms] (a row's slide), then calls [then] unless another move took over. */
    fun slide(by: Float, ms: Int, then: () -> Unit) = start(by, ms, LinearEasing, then)

    /** Stops where it is. */
    fun stop() {
        asked = false
        then = null
        job?.cancel()
        job = null
    }

    private fun start(by: Float, ms: Int, easing: Easing, then: (() -> Unit)?) {
        distance = by
        covered = 0f
        elapsed = 0L
        duration = ms * NANOS_PER_MS
        this.easing = easing
        this.then = then
        asked = true
        if (job?.isActive != true) job = scope.launch { run() }
    }

    private suspend fun run() {
        state.scroll {
            var last = -1L
            while (asked) {
                val now = withFrameNanos(FrameTime)
                // The first frame counts as one: a move asked for in a frame shows in the next.
                elapsed += if (last < 0) FRAME_NANOS else now - last
                last = now
                val fraction = if (elapsed >= duration) 1f else easing.transform(elapsed.toFloat() / duration)
                val step = distance * fraction - covered
                covered += step
                val stopped = abs(scrollBy(step) - step) > 0.5f
                if (fraction < 1f && !stopped) continue
                // Done, or at the end: what follows may ask for another move, which goes on from here.
                asked = false
                val next = then
                then = null
                next?.invoke()
            }
        }
    }

    private companion object {
        const val NANOS_PER_MS = 1_000_000L
        const val FRAME_NANOS = 16_666_667L
        val FrameTime: (Long) -> Long = { it }
    }
}

/**
 * Tiles keeping their resting look while their row slides, or their grid
 * glides, under a held key (D-pad rule 5): scale 1, no ring, a regular
 * caption, under one ring the row or grid draws still where focus will
 * land while they are quiet ([on]). A tile growing and shrinking at every
 * step, its ring with it, would make the page stutter. As on the native
 * Samsung app, the still ring goes at once as the key comes up, and the
 * focused tile's look comes in as a focus look does (150 ms, eased out).
 * Tiles read [amount] only in their layer and while drawing; the caption's
 * weight, which composes, switches once at either end ([on]).
 */
@Stable
internal class TileQuiet {
    /** 1 while quiet, 0 not, between them as the looks come back where the key came up. */
    var amount by mutableFloatStateOf(0f)
        private set

    /** Whether the tiles are quiet: from the slide's start until it lands. */
    var on by mutableStateOf(false)
        private set

    /** The tiles go quiet, at once. */
    fun start() {
        on = true
        amount = 1f
    }

    /** The tiles take their looks back, as a focus look comes in; at once with [reduced] motion. */
    suspend fun end(reduced: Boolean) {
        on = false
        if (reduced) {
            amount = 0f
            return
        }
        animate(amount, 0f, animationSpec = TvMotion.FocusIn) { value, _ -> amount = value }
    }
}

/** The quiet of the row or grid a tile is in; none outside them. */
internal val LocalTileQuiet = staticCompositionLocalOf<TileQuiet?> { null }

/**
 * How a row of chips moves (recent searches), as the native Samsung app's
 * SearchScreen moves them: not while the focused chip shows between
 * [start] from the row's leading edge and [end] from its trailing one;
 * past them, just far enough, as the kit moves rows ([TvGlide]). The row
 * provides [TvScrolling.itself], tells where it is laid out ([row]), and
 * each chip carries [keptWithin].
 */
@Stable
internal class WithinMotion(state: ScrollableState, scope: CoroutineScope) {
    private val glide = TvGlide(state, scope)
    var row: LayoutCoordinates? = null
    var start = 0f
    var end = 0f
    var gate: KeyRepeatGate? = null
    var reduced = false

    /** The chip laid out at [chip] took focus. */
    fun focused(chip: LayoutCoordinates) {
        val row = row ?: return
        if (!row.isAttached || !chip.isAttached) return
        val left = row.localPositionOf(chip, Offset.Zero).x
        val right = left + chip.size.width
        val room = row.size.width - end
        val by = when {
            left < start -> left - start
            // Past the edge: its end just inside it, unless it is too wide to fit, when its start.
            right > room -> minOf(right - room, left - start)
            else -> return
        }
        glide.move(by, gate?.holding == true, reduced)
    }
}

/** This chip keeps within its row as [motion] says, when it takes focus. */
internal fun Modifier.keptWithin(motion: WithinMotion): Modifier = this then KeptWithinElement(motion)

private data class KeptWithinElement(val motion: WithinMotion) : ModifierNodeElement<KeptWithinNode>() {
    override fun create() = KeptWithinNode(motion)

    override fun update(node: KeptWithinNode) {
        node.motion = motion
    }

    override fun InspectorInfo.inspectableProperties() {
        name = "keptWithin"
    }
}

private class KeptWithinNode(var motion: WithinMotion) : Modifier.Node(), FocusEventModifierNode {
    private var hadFocus = false

    override fun onFocusEvent(focusState: FocusState) {
        if (focusState.hasFocus == hadFocus) return
        hadFocus = focusState.hasFocus
        if (hadFocus) motion.focused(requireLayoutCoordinates())
    }
}

/**
 * A lazy row's or grid's focusable cells by position, while they are
 * composed: how the kit puts focus on one it picked (where a slide lands,
 * a letter's first title) and finds where one is (to bring it to its
 * line). A node per cell, rather than a requester and an effect.
 */
internal class FocusSlots {
    private val cells = HashMap<Int, FocusSlotNode>()

    /** Focuses the cell at [index], if it is composed; whether it took focus. */
    fun focus(index: Int): Boolean = cells[index]?.let { runCatching { it.requestFocus() }.getOrDefault(false) } == true

    /** Where the cell at [index] is laid out, unscaled by its focus look; null when it isn't composed. */
    fun coordinates(index: Int): LayoutCoordinates? = cells[index]?.takeIf { it.isAttached }?.requireLayoutCoordinates()

    internal fun attach(index: Int, node: FocusSlotNode) {
        cells[index] = node
    }

    internal fun detach(index: Int, node: FocusSlotNode) {
        if (cells[index] === node) cells.remove(index)
    }
}

/** This focusable is [slots]' cell [index]. Before the focusable, as a FocusRequester goes. */
internal fun Modifier.focusSlot(slots: FocusSlots, index: Int): Modifier = this then FocusSlotElement(slots, index)

private data class FocusSlotElement(val slots: FocusSlots, val index: Int) : ModifierNodeElement<FocusSlotNode>() {
    override fun create() = FocusSlotNode(slots, index)

    override fun update(node: FocusSlotNode) = node.update(slots, index)

    override fun InspectorInfo.inspectableProperties() {
        name = "focusSlot"
        properties["index"] = index
    }
}

internal class FocusSlotNode(private var slots: FocusSlots, private var index: Int) : Modifier.Node(), FocusRequesterModifierNode {
    override fun onAttach() = slots.attach(index, this)

    override fun onDetach() = slots.detach(index, this)

    fun update(slots: FocusSlots, index: Int) {
        if (slots === this.slots && index == this.index) return
        if (isAttached) this.slots.detach(this.index, this)
        this.slots = slots
        this.index = index
        if (isAttached) slots.attach(index, this)
    }
}
