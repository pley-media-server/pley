package app.pley.android.ui.phone.library

import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.detectVerticalDragGestures
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.requiredSize
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.grid.LazyGridState
import androidx.compose.foundation.systemGestureExclusion
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.reducedMotion
import app.pley.android.screens.library.AlphaIndex
import kotlin.math.roundToInt
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.distinctUntilChanged

/**
 * The grid's fast-scroll handle (design spec §4.2): a 48 dp handle on the
 * right edge, showing while the grid scrolls. Only the handle takes
 * touches, 48 dp square around it and only while it shows, so a swipe or
 * a tap near the edge still reaches the grid; the system's back gesture
 * stays clear of it. Dragging it moves through the whole listing at once;
 * when the listing has its A to Z ([index]), a 56 dp bubble beside the
 * finger shows the letter it is at, and each new letter ticks.
 *
 * The handle's place is read while laying out, so scrolling never
 * recomposes it.
 */
@Composable
internal fun FastScroller(
    grid: LazyGridState,
    total: Int,
    columns: Int,
    index: AlphaIndex?,
    modifier: Modifier = Modifier,
) {
    val density = LocalDensity.current
    val haptics = LocalHapticFeedback.current
    val reduced = reducedMotion()
    var height by remember { mutableIntStateOf(0) }
    var dragging by remember { mutableStateOf(false) }
    var dragFraction by remember { mutableFloatStateOf(0f) }
    var letter by remember { mutableIntStateOf(-1) }
    val alpha = remember { Animatable(0f) }
    val shown by remember { derivedStateOf { alpha.value > 0f } }
    val handle = with(density) { HANDLE.roundToPx() }

    LaunchedEffect(grid) {
        snapshotFlow { grid.isScrollInProgress || dragging }.distinctUntilChanged().collect { active ->
            if (active) {
                alpha.snapTo(1f)
            } else {
                delay(HIDE_AFTER_MS)
                if (reduced) alpha.snapTo(0f) else alpha.animateTo(0f, tween(FADE_MS))
            }
        }
    }

    fun span(): Int = (height - handle).coerceAtLeast(1)

    fun fraction(): Float {
        if (dragging) return dragFraction
        val info = grid.layoutInfo
        val visible = info.visibleItemsInfo.size.coerceAtLeast(1)
        val last = (total - visible).coerceAtLeast(1)
        return (grid.firstVisibleItemIndex.toFloat() / last).coerceIn(0f, 1f)
    }

    // [top] is where the finger has moved the handle's top to, so the handle stays where it was taken hold of.
    fun jump(top: Float) {
        val f = (top / span()).coerceIn(0f, 1f)
        dragFraction = f
        val target = (f * (total - 1)).roundToInt()
        val rowStart = target - target % columns.coerceAtLeast(1)
        grid.requestScrollToItem(rowStart)
        val at = index?.letterAt(target) ?: -1
        if (at != letter) {
            if (letter >= 0 && at >= 0) haptics.performHapticFeedback(HapticFeedbackType.SegmentTick)
            letter = at
        }
    }

    // The strip only measures the height and places the handle: it takes no touches of its own.
    Box(
        modifier
            .fillMaxHeight()
            .width(TOUCH)
            .onSizeChanged { height = it.height }
            .clearAndSetSemantics { },
    ) {
        val place: () -> Int = { (fraction() * (height - handle).coerceAtLeast(0)).roundToInt() }
        Box(
            Modifier
                .align(Alignment.TopEnd)
                .offset { IntOffset(0, place()) }
                .size(TOUCH, HANDLE)
                .then(
                    if (shown) {
                        Modifier
                            .systemGestureExclusion()
                            .pointerInput(total, columns, index) {
                                var top = 0f
                                detectVerticalDragGestures(
                                    onDragStart = {
                                        top = fraction() * span()
                                        dragFraction = top / span()
                                        dragging = true
                                    },
                                    onDragEnd = { dragging = false },
                                    onDragCancel = { dragging = false },
                                    onVerticalDrag = { change, dy ->
                                        change.consume()
                                        top += dy
                                        jump(top)
                                    },
                                )
                            }
                    } else {
                        Modifier
                    },
                ),
        ) {
            Box(
                Modifier
                    .align(Alignment.TopEnd)
                    .padding(end = 4.dp)
                    .size(6.dp, HANDLE)
                    .graphicsLayer { this.alpha = alpha.value }
                    .background(if (dragging) PleyColors.Muted else PleyColors.Control, PleyShape.Control),
            )
        }
        if (dragging && index != null && letter >= 0) {
            Box(
                Modifier
                    .align(Alignment.TopEnd)
                    // Centered on the touch strip, then moved left of the handle, clear of the finger.
                    .offset { IntOffset(-BUBBLE_SHIFT.roundToPx(), place() + (handle - BUBBLE.roundToPx()) / 2) }
                    .requiredSize(BUBBLE)
                    .background(PleyColors.Surface3, PleyShape.Circle),
                contentAlignment = Alignment.Center,
            ) {
                Text(AlphaIndex.LETTERS[letter], style = MaterialTheme.typography.headlineSmall, color = PleyColors.Fg)
            }
        }
    }
}

private val HANDLE = 48.dp
private val TOUCH = 48.dp
private val BUBBLE = 56.dp
private val BUBBLE_SHIFT = 36.dp
private const val HIDE_AFTER_MS = 1_200L
private const val FADE_MS = 300
