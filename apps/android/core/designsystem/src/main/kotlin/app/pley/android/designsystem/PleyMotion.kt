package app.pley.android.designsystem

import android.content.Context
import android.provider.Settings
import androidx.compose.animation.core.Easing
import androidx.compose.animation.core.FastOutLinearInEasing
import androidx.compose.animation.core.FastOutSlowInEasing
import androidx.compose.animation.core.FiniteAnimationSpec
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.snap
import androidx.compose.animation.core.tween
import androidx.compose.runtime.Composable
import androidx.compose.runtime.ReadOnlyComposable
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.platform.LocalContext

/**
 * How pley moves (design spec §5.1): short, eased, never bouncing. Font
 * weight never animates; it switches. With reduced motion ([LocalReducedMotion])
 * scale changes snap, the backdrop swaps at once and panels appear in
 * place; focus rings and fills stay.
 */
object PleyMotion {
    /** Scale up, ring and glow in. */
    const val FOCUS_MS = 150

    /** Scale back, ring out. */
    const val UNFOCUS_MS = 120

    /** A pressed scale. */
    const val PRESS_MS = 100

    /** Round-button labels, info blocks, Skip leaving (alpha). */
    const val REVEAL_MS = 150

    /** The backdrop crossfading to follow focus… */
    const val BACKDROP_MS = 300

    /** …once focus has rested this long; info blocks wait as long. Nothing changes while a key repeats. */
    const val REST_MS = 400

    /** Player controls, the drawer, panels. */
    const val OVERLAY_MS = 200

    /** Screen changes. On TV a [PleyColors.Bg] rectangle fades in for half of it, then out. */
    const val SCREEN_MS = 220

    /** Pictures that weren't in the memory cache fade in this long (none on T0). */
    const val IMAGE_FADE_MS = 150

    val FocusEasing: Easing = FastOutSlowInEasing
    val UnfocusEasing: Easing = FastOutLinearInEasing
    val OverlayEasing: Easing = FastOutSlowInEasing

    /**
     * The native Samsung app's curves, as its TV draws them (NUI's EaseOut
     * and EaseIn, both cubic): what opens or takes focus starts quick and
     * settles, what closes or lets focus go starts slow and leaves quick.
     * The TV's rail and side panels move on these.
     */
    val EaseOut: Easing = Easing { fraction ->
        val rest = 1f - fraction
        1f - rest * rest * rest
    }
    val EaseIn: Easing = Easing { fraction -> fraction * fraction * fraction }

    /** Focus in: 150 ms, fast out and slow in; a snap with reduced motion. */
    fun <T> focus(reduced: Boolean = false): FiniteAnimationSpec<T> =
        if (reduced) snap() else tween(FOCUS_MS, easing = FocusEasing)

    /** Focus out: 120 ms, fast out and linear in; a snap with reduced motion. */
    fun <T> unfocus(reduced: Boolean = false): FiniteAnimationSpec<T> =
        if (reduced) snap() else tween(UNFOCUS_MS, easing = UnfocusEasing)

    /** A press: 100 ms, linear. */
    fun <T> press(reduced: Boolean = false): FiniteAnimationSpec<T> =
        if (reduced) snap() else tween(PRESS_MS, easing = LinearEasing)

    /** A label or block fading in or out: 150 ms, linear. Kept with reduced motion: it doesn't move anything. */
    fun <T> reveal(): FiniteAnimationSpec<T> = tween(REVEAL_MS, easing = LinearEasing)

    /** Controls, the drawer and panels: 200 ms; in place with reduced motion. */
    fun <T> overlay(reduced: Boolean = false): FiniteAnimationSpec<T> =
        if (reduced) snap() else tween(OVERLAY_MS, easing = OverlayEasing)

    /**
     * Whether Android asks for no animation: "Remove animations" sets the
     * animator duration scale to 0. Fire OS has no such setting, and Google
     * TV may not, so pley's own switch matters on TVs ([LocalReducedMotion]).
     */
    fun systemReducedMotion(context: Context): Boolean =
        Settings.Global.getFloat(context.contentResolver, Settings.Global.ANIMATOR_DURATION_SCALE, 1f) == 0f
}

/**
 * Whether to move less: the system's "Remove animations" or pley's
 * "Reduce motion" switch. The shells provide it at their root, from both;
 * by default only the system's setting counts.
 */
val LocalReducedMotion = staticCompositionLocalOf<Boolean?> { null }

/** [LocalReducedMotion], falling back to the system's setting where no shell provided it. */
@Composable
@ReadOnlyComposable
fun reducedMotion(): Boolean = LocalReducedMotion.current ?: PleyMotion.systemReducedMotion(LocalContext.current)
