package app.pley.android.designsystem.icon

import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.ContentTransform
import androidx.compose.animation.EnterTransition
import androidx.compose.animation.ExitTransition
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.scaleIn
import androidx.compose.animation.scaleOut
import androidx.compose.animation.togetherWith
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.Dp
import app.pley.android.designsystem.reducedMotion

/**
 * Play or Pause's glyph, one giving way to the other in place, a short
 * fade and scale, as music plays and pauses (Now Playing and the bar of
 * what plays on a phone, Now Playing on a TV). With reduced motion it
 * changes at once.
 */
@Composable
fun PlayPauseGlyph(playing: Boolean, size: Dp, tint: Color) {
    val reduced = reducedMotion()
    AnimatedContent(
        targetState = playing,
        contentAlignment = Alignment.Center,
        transitionSpec = {
            if (reduced) {
                ContentTransform(EnterTransition.None, ExitTransition.None, sizeTransform = null)
            } else {
                (fadeIn(tween(GLYPH_MS)) + scaleIn(tween(GLYPH_MS), initialScale = 0.7f)) togetherWith (fadeOut(tween(GLYPH_MS)) + scaleOut(tween(GLYPH_MS), targetScale = 0.7f))
            }
        },
        label = "play",
    ) { pause -> Icon(if (pause) PleyIcons.Pause else PleyIcons.Play, contentDescription = null, size = size, tint = tint) }
}

private const val GLYPH_MS = 120
