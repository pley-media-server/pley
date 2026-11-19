package app.pley.android.ui.tv.kit

import androidx.compose.foundation.gestures.animateScrollBy
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusDirection
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import app.pley.android.designsystem.TvSafeArea
import kotlinx.coroutines.launch

/**
 * Lets the D-pad read text that doesn't take focus (details, licenses):
 * Down where focus has nowhere to go scrolls [state] on while there is
 * more below, until its end is [endRoom] above the screen's bottom: first
 * the last item's top up to [line], so what had focus goes whole, then a
 * step at a time. Up takes those steps back before focus moves again.
 * Text never needs a focus look to be reached. [onReading] hears whether
 * the list is read on past what has focus (a page can give the text the
 * whole screen then).
 */
@Composable
fun Modifier.readOn(
    state: LazyListState,
    line: Dp = TvSafeArea.Vertical,
    endRoom: Dp = TvSafeArea.Vertical,
    onReading: (Boolean) -> Unit = {},
): Modifier {
    val focus = LocalFocusManager.current
    val scope = rememberCoroutineScope()
    val density = LocalDensity.current
    val read = remember(state) { ReadOn() }
    fun stepped() = onReading(read.steps.isNotEmpty())
    return onKeyEvent { event ->
        if (event.type != KeyEventType.KeyDown) return@onKeyEvent false
        when (event.key) {
            Key.DirectionDown -> {
                if (focus.moveFocus(FocusDirection.Down)) {
                    read.steps.clear()
                    stepped()
                    return@onKeyEvent true
                }
                val left = below(state, with(density) { endRoom.toPx() })
                if (left < 1f) return@onKeyEvent false
                val toLine = lastToLine(state, with(density) { line.toPx() })
                val by = if (toLine >= 1f) toLine else minOf(with(density) { ReadStep.toPx() }, left)
                read.steps += by
                stepped()
                scope.launch { state.animateScrollBy(by) }
                true
            }
            Key.DirectionUp -> {
                val by = read.steps.removeLastOrNull() ?: return@onKeyEvent false
                stepped()
                scope.launch { state.animateScrollBy(-by) }
                true
            }
            Key.DirectionLeft, Key.DirectionRight -> {
                read.steps.clear()
                stepped()
                false
            }
            else -> false
        }
    }
}

/** The steps the list has been read on past what has focus, in px, to take back in turn. */
private class ReadOn {
    val steps = ArrayList<Float>()
}

/** How far [state]'s last item reaches below the screen's bottom less [room], in px; more than a step while it isn't laid out. */
private fun below(state: LazyListState, room: Float): Float {
    val info = state.layoutInfo
    val last = info.visibleItemsInfo.lastOrNull() ?: return 0f
    if (last.index < info.totalItemsCount - 1) return Float.MAX_VALUE
    return (last.offset + last.size - (info.viewportEndOffset - room)).coerceAtLeast(0f)
}

/** How far [state]'s last item's top is below [line] (from the list's top), in px; 0 while it isn't laid out. */
private fun lastToLine(state: LazyListState, line: Float): Float {
    val info = state.layoutInfo
    val last = info.visibleItemsInfo.lastOrNull()?.takeIf { it.index == info.totalItemsCount - 1 } ?: return 0f
    return (last.offset + info.beforeContentPadding - line).coerceAtLeast(0f)
}

/** A step: a few lines of text, about a third of the screen. */
private val ReadStep = 160.dp
