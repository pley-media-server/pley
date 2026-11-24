package app.pley.android.designsystem

import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.ContentTransform
import androidx.compose.animation.EnterTransition
import androidx.compose.animation.ExitTransition
import androidx.compose.animation.SizeTransform
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInHorizontally
import androidx.compose.animation.slideOutHorizontally
import androidx.compose.animation.togetherWith
import androidx.compose.runtime.Composable
import androidx.compose.runtime.Immutable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier

/** The words that change with the track: [key] says which track of the queue they are, [index] where it is. */
@Immutable
data class TrackWords(val key: Any, val index: Int, val title: String, val byline: String)

/**
 * [words] drawn by [content], changing with the track (Now Playing and
 * the bar of what plays on a phone, Now Playing on a TV): the next track's
 * come in from the side it is on in the queue (from the right going on,
 * from the left going back) as the last one's fade out, a short way, and
 * the block's height follows. With reduced motion they change in place.
 */
@Composable
fun TrackChange(words: TrackWords, modifier: Modifier = Modifier, alignment: Alignment = Alignment.TopStart, content: @Composable (TrackWords) -> Unit) {
    val reduced = reducedMotion()
    AnimatedContent(
        targetState = words,
        modifier = modifier,
        contentKey = { it.key },
        contentAlignment = alignment,
        transitionSpec = {
            if (reduced) {
                ContentTransform(EnterTransition.None, ExitTransition.None, sizeTransform = null)
            } else {
                val forward = targetState.index >= initialState.index
                val enter = fadeIn(tween(TRACK_IN_MS, delayMillis = TRACK_OUT_MS / 2)) +
                    slideInHorizontally(tween(TRACK_IN_MS, easing = PleyMotion.EaseOut)) { width -> (if (forward) width else -width) / TRACK_SLIDE }
                val exit = fadeOut(tween(TRACK_OUT_MS)) +
                    slideOutHorizontally(tween(TRACK_OUT_MS, easing = PleyMotion.EaseIn)) { width -> (if (forward) -width else width) / TRACK_SLIDE }
                (enter togetherWith exit).using(SizeTransform(clip = false))
            }
        },
        label = "track",
    ) { shown -> content(shown) }
}

private const val TRACK_IN_MS = 260
private const val TRACK_OUT_MS = 140

/** The words move this share of their width. */
private const val TRACK_SLIDE = 10
