package app.pley.android.ui.phone.theme

import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.displayCutout
import androidx.compose.foundation.layout.systemBars
import androidx.compose.foundation.layout.union
import androidx.compose.runtime.Composable
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.remember
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalLayoutDirection
import androidx.compose.ui.platform.LocalWindowInfo
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.ui.unit.dp
import app.pley.android.designsystem.PleyPhone

/** Material's window width classes, by the window's width. */
enum class WidthClass {
    /** Under 600 dp: phones held upright. The navigation bar. */
    Compact,

    /** 600 to 840 dp: small tablets, phones on their side, unfolded foldables. The rail. */
    Medium,

    /** 840 dp and up: tablets. The rail. */
    Expanded,
    ;

    companion object {
        fun of(windowWidth: Dp): WidthClass = when {
            windowWidth < 600.dp -> Compact
            windowWidth < 840.dp -> Medium
            else -> Expanded
        }
    }
}

/**
 * The page's measures for this window (design spec §1.3): the side margin
 * by width class, and how wide the page's content is, beside the rail
 * when there is one. Rows and grids size their cards from [contentWidth]
 * without measuring themselves, so a screen of rows lays out in one pass.
 *
 * On its side a phone has its navigation buttons or its camera's cutout
 * at a side of the page: [insetStart] and [insetEnd] keep the page's
 * content clear of them, inside [marginStart] and [marginEnd].
 */
@Immutable
data class PhoneMetrics(
    val widthClass: WidthClass,
    /** The page's content width: the window's, less the rail's and the safe area's insets at the sides. */
    val contentWidth: Dp,
    /** What the page keeps clear of at its start: the safe area's inset there, unless the rail covers it. */
    val insetStart: Dp = 0.dp,
    /** What the page keeps clear of at its end: the safe area's inset there. */
    val insetEnd: Dp = 0.dp,
) {
    /** The side margin: 16 compact, 24 medium, 32 expanded. */
    val margin: Dp
        get() = when (widthClass) {
            WidthClass.Compact -> PleyPhone.MarginCompact
            WidthClass.Medium -> PleyPhone.MarginMedium
            WidthClass.Expanded -> PleyPhone.MarginExpanded
        }

    /** The page's padding at its start: the margin past [insetStart]. */
    val marginStart: Dp get() = margin + insetStart

    /** The page's padding at its end: the margin past [insetEnd]. */
    val marginEnd: Dp get() = margin + insetEnd

    /** The page's whole width, insets included, for what bleeds to its edges. */
    val pageWidth: Dp get() = contentWidth + insetStart + insetEnd

    /** Whether navigation is a rail beside the page rather than a bar under it. */
    val rail: Boolean get() = widthClass != WidthClass.Compact

    companion object {
        /** The navigation rail (NavigationRail), its icons alone. */
        val RailWidth = 80.dp

        /**
         * A window [windowWidth] wide whose safe area stops [safeStart] and
         * [safeEnd] short of its sides. The rail, at the start, is wider by
         * [railStart], the system bars' inset it pads itself by there.
         */
        fun of(windowWidth: Dp, safeStart: Dp = 0.dp, safeEnd: Dp = 0.dp, railStart: Dp = 0.dp): PhoneMetrics {
            val widthClass = WidthClass.of(windowWidth)
            val rail = if (widthClass == WidthClass.Compact) 0.dp else RailWidth + railStart
            val start = (safeStart - rail).coerceAtLeast(0.dp)
            val content = windowWidth - rail - start - safeEnd
            return PhoneMetrics(widthClass, content.coerceAtLeast(0.dp), start, safeEnd)
        }
    }
}

/** The page's [PhoneMetrics]; [PhoneTheme] provides them for the window. */
val LocalPhoneMetrics = staticCompositionLocalOf { PhoneMetrics.of(412.dp) }

@Composable
internal fun rememberPhoneMetrics(): PhoneMetrics {
    val width = LocalWindowInfo.current.containerSize.width
    val density = LocalDensity.current
    val direction = LocalLayoutDirection.current
    // The safe area's sides: the system bars and the cutout. The keyboard only ever rises from the bottom.
    val bars = WindowInsets.systemBars
    val safe = bars.union(WindowInsets.displayCutout)
    val ltr = direction == LayoutDirection.Ltr
    val safeStart = if (ltr) safe.getLeft(density, direction) else safe.getRight(density, direction)
    val safeEnd = if (ltr) safe.getRight(density, direction) else safe.getLeft(density, direction)
    val barStart = if (ltr) bars.getLeft(density, direction) else bars.getRight(density, direction)
    return remember(width, density, safeStart, safeEnd, barStart) {
        with(density) { PhoneMetrics.of(width.toDp(), safeStart.toDp(), safeEnd.toDp(), barStart.toDp()) }
    }
}
