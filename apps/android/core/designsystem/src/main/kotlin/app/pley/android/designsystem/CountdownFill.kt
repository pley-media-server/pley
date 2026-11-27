package app.pley.android.designsystem

import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.tween
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.State
import androidx.compose.runtime.remember

/**
 * How much of a countdown of [total] seconds has gone, from 0 to 1, for a
 * ring or a sweep to draw. The player counts whole seconds; through each
 * one the fill glides, linearly, to where it stands at that second's end,
 * so it moves without steps and is full as the countdown reaches 0. A
 * countdown that starts again jumps back at once rather than sliding back.
 * Without [smooth] (reduced motion) it steps once a second.
 *
 * Read the value while drawing, so that only the drawing follows it and
 * nothing is composed again 60 times a second.
 */
@Composable
fun countdownFill(secondsLeft: Int?, total: Int, smooth: Boolean = true): State<Float> {
    val fill = remember { Animatable(0f) }
    LaunchedEffect(secondsLeft, smooth) {
        if (secondsLeft == null) {
            fill.snapTo(0f)
            return@LaunchedEffect
        }
        val start = ((total - secondsLeft).toFloat() / total).coerceIn(0f, 1f)
        if (!smooth) {
            fill.snapTo(start)
            return@LaunchedEffect
        }
        val end = ((total - secondsLeft + 1).toFloat() / total).coerceIn(0f, 1f)
        // Ahead of this second (a countdown started again) or far behind it: from its place.
        if (fill.value > end || fill.value < start - 0.5f / total) fill.snapTo(start)
        fill.animateTo(end, tween(SECOND_MS, easing = LinearEasing))
    }
    return fill.asState()
}

private const val SECOND_MS = 1_000
