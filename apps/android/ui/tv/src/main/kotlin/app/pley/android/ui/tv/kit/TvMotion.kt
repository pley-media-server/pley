package app.pley.android.ui.tv.kit

import androidx.compose.animation.core.AnimationSpec
import androidx.compose.animation.core.EaseOutCubic
import androidx.compose.animation.core.animate
import androidx.compose.animation.core.tween
import androidx.compose.foundation.gestures.ScrollableState
import androidx.compose.foundation.gestures.animateScrollBy
import androidx.compose.foundation.gestures.scrollBy
import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequesterModifierNode
import androidx.compose.ui.focus.requestFocus
import androidx.compose.ui.node.ModifierNodeElement
import androidx.compose.ui.platform.InspectorInfo
import app.pley.android.designsystem.PleyMotion
import kotlin.math.abs

/**
 * How the TV's rows, pages of rows and grids move to what has focus
 * (design spec §2.4, D-pad rules 4 and 5), as the native Samsung app moves
 * them: over 150 ms, eased out, for a press; evenly over
 * [KeyRepeatGate.GLIDE_MS] for a held key's move, so each runs into the
 * next; at once with reduced motion. Compose's own scrolling to the
 * focused element keeps to no duration, so the kit moves these itself and
 * tells Compose to leave them be ([TvScrolling.itself]).
 */
internal object TvMotion {
    /** A press's move. */
    val Press: AnimationSpec<Float> = tween(PleyMotion.FOCUS_MS, easing = EaseOutCubic)
}

/** Scrolls [by] pixels as the kit moves: a [held] key's glide, a press's ease, or at once when [reduced]. */
internal suspend fun ScrollableState.moveBy(by: Float, held: Boolean, reduced: Boolean) {
    when {
        abs(by) < 0.5f -> Unit
        reduced -> scrollBy(by)
        else -> animateScrollBy(by, if (held) KeyRepeatGate.glide() else TvMotion.Press)
    }
}

/**
 * Tiles keeping their resting look while their row slides, or their grid
 * glides, under a held key (D-pad rule 5): scale 1, no ring, a regular
 * caption, under one ring the row or grid draws still where focus will
 * land. A tile growing and shrinking at every step, its ring with it, would
 * make the page stutter. Tiles read [amount] only in their layer and while
 * drawing; the caption's weight, which composes, switches once at either
 * end ([on]).
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
        animate(amount, 0f, animationSpec = PleyMotion.focus()) { value, _ -> amount = value }
    }
}

/** The quiet of the row or grid a tile is in; none outside them. */
internal val LocalTileQuiet = staticCompositionLocalOf<TileQuiet?> { null }

/**
 * A lazy row's or grid's focusable cells by position, while they are
 * composed: how the kit puts focus on one it picked (where a slide lands,
 * a letter's first title). A node per cell, rather than a requester and an
 * effect.
 */
internal class FocusSlots {
    private val cells = HashMap<Int, FocusSlotNode>()

    /** Focuses the cell at [index], if it is composed; whether it took focus. */
    fun focus(index: Int): Boolean = cells[index]?.let { runCatching { it.requestFocus() }.getOrDefault(false) } == true

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
