package app.pley.android.designsystem.tile

import androidx.compose.animation.core.withInfiniteAnimationFrameMillis
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.width
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.Stable
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.graphics.drawOutline
import androidx.compose.ui.graphics.drawscope.ContentDrawScope
import androidx.compose.ui.node.DrawModifierNode
import androidx.compose.ui.node.ModifierNodeElement
import androidx.compose.ui.node.invalidateDraw
import androidx.compose.ui.platform.InspectorInfo
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.reducedMotion

/**
 * The loading look of a screen with nothing saved yet (design spec §8):
 * wells and caption bars in the exact shapes of what is coming. On TV they
 * are still; on phones one shimmer runs per screen, shared by every
 * skeleton on it, and stops with reduced motion. Its position is read only
 * while drawing, so the shimmer never recomposes anything.
 *
 * The shimmer runs only while a skeleton is drawn with this look. A screen
 * keeps its look once its content has come (rows and grids that may still
 * load a part), and a clock running under content wakes the main thread
 * every frame for nothing.
 */
@Stable
class SkeletonState internal constructor(internal val animated: Boolean) {
    /** Skeleton shapes drawn with this look now. */
    private var drawn by mutableIntStateOf(0)

    /** Whether any skeleton is drawn with this look: the shimmer runs only then. */
    internal val shown: Boolean by derivedStateOf { drawn > 0 }

    /** Where the shimmer is, from 0 to 1. Read only while drawing. */
    internal var shimmer by mutableFloatStateOf(0f)

    internal fun attach() {
        drawn++
    }

    internal fun detach() {
        drawn--
    }
}

/** One skeleton look per screen. [animated] is false on TV; reduced motion stops it anyway. */
@Composable
fun rememberSkeleton(animated: Boolean): SkeletonState {
    val moving = animated && !reducedMotion()
    val state = remember(moving) { SkeletonState(moving) }
    if (moving) Shimmer(state)
    return state
}

/**
 * Moves [state]'s shimmer across its skeletons, from left to right once
 * every [SHIMMER_MS], while one is drawn. In a scope of its own: a skeleton
 * showing or going starts or stops it without recomposing the screen.
 */
@Composable
private fun Shimmer(state: SkeletonState) {
    if (!state.shown) return
    LaunchedEffect(state) {
        val start = withInfiniteAnimationFrameMillis { it }
        while (true) {
            withInfiniteAnimationFrameMillis { now -> state.shimmer = ((now - start) % SHIMMER_MS).toFloat() / SHIMMER_MS }
        }
    }
}

private const val SHIMMER_MS = 1_200

/** Paints [shape] in [PleyColors.Surface2], with [state]'s shimmer passing over it. */
fun Modifier.skeleton(state: SkeletonState, shape: Shape): Modifier = this then SkeletonElement(state, shape)

/** A tile still loading: its well, and two caption bars of 60 % and 35 % of its width. */
@Composable
fun SkeletonTile(
    state: SkeletonState,
    kind: TileKind,
    style: TileStyle,
    modifier: Modifier = Modifier,
    width: Dp = kind.tvWidth,
) {
    Column(modifier.width(width).clearAndSetSemantics {}) {
        Box(
            Modifier
                .fillMaxWidth()
                .height(width / kind.aspectRatio)
                .skeleton(state, style.shape(kind)),
        )
        Spacer(Modifier.height(style.captionGap))
        val line = style.captionTitle.lineHeight.value.dp
        Bar(state, 0.6f, line)
        Bar(state, 0.35f, line)
    }
}

@Composable
private fun Bar(state: SkeletonState, fraction: Float, line: Dp) {
    Box(Modifier.height(line), propagateMinConstraints = false) {
        Box(
            Modifier
                .fillMaxWidth(fraction)
                .height(line * 0.6f)
                .skeleton(state, PleyShape.Badge),
        )
    }
}

private data class SkeletonElement(val state: SkeletonState, val shape: Shape) : ModifierNodeElement<SkeletonNode>() {
    override fun create() = SkeletonNode(state, shape)

    override fun update(node: SkeletonNode) = node.update(state, shape)

    override fun InspectorInfo.inspectableProperties() {
        name = "skeleton"
    }
}

private class SkeletonNode(private var state: SkeletonState, private var shape: Shape) : Modifier.Node(), DrawModifierNode {
    // Counted while attached: the look's shimmer runs while any of its skeletons is.
    override fun onAttach() = state.attach()

    override fun onDetach() = state.detach()

    fun update(state: SkeletonState, shape: Shape) {
        if (state !== this.state && isAttached) {
            this.state.detach()
            state.attach()
        }
        this.state = state
        this.shape = shape
        invalidateDraw()
    }

    override fun ContentDrawScope.draw() {
        val outline = shape.createOutline(size, layoutDirection, this)
        drawOutline(outline, PleyColors.Surface2)
        if (state.animated) {
            val t = state.shimmer
            val band = size.width.coerceAtLeast(1f)
            val x = -band + t * (size.width + 2 * band)
            drawOutline(
                outline,
                Brush.linearGradient(
                    colors = listOf(PleyColors.Surface2, PleyColors.Surface3, PleyColors.Surface2),
                    start = Offset(x - band / 2f, 0f),
                    end = Offset(x + band / 2f, 0f),
                ),
            )
        }
        drawContent()
    }
}
