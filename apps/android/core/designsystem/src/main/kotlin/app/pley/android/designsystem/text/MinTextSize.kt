package app.pley.android.designsystem.text

import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.unit.TextUnit
import androidx.compose.ui.unit.isSpecified
import androidx.compose.ui.unit.sp

/**
 * The smallest text drawn here, when there is one: a car's screen, read
 * at arm's length and at a glance, takes none under 24 sp. What draws
 * text in a size of its own (a tile's caption, a badge), rather than the
 * theme's typography, which such a screen raises itself, reads this.
 */
val LocalMinTextSize = staticCompositionLocalOf { TextUnit.Unspecified }

/** This style at [min] at least, its line height grown with it; as it is when [min] isn't specified. */
fun TextStyle.atLeast(min: TextUnit): TextStyle {
    if (!min.isSpecified || !fontSize.isSpecified || fontSize.value >= min.value) return this
    val ratio = if (lineHeight.isSpecified) lineHeight.value / fontSize.value else 1.25f
    return copy(fontSize = min, lineHeight = (min.value * ratio).sp)
}
