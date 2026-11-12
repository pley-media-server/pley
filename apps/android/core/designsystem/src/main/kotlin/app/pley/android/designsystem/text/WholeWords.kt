package app.pley.android.designsystem.text

import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.layout.Layout
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.text
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.TextLayoutResult
import androidx.compose.ui.text.TextMeasurer
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.drawText
import androidx.compose.ui.text.rememberTextMeasurer
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.TextUnit
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.isSpecified
import androidx.compose.ui.unit.sp

/**
 * Text in a narrow slot that never breaks inside a word: at its own size
 * unless one of its words is wider than the slot, then as much smaller as
 * that word needs, down to [MinSize] as drawn. With a large system font,
 * "Libraries" was "Librarie" over "s" and "The Glasshouse" "The Gla" over
 * "sshouse"; Android hyphenates neither. The floor is the drawn size, not
 * sp: at 200 % a 9 sp floor draws about 18 dp, too big for "Playlist" in
 * a fifth of a small phone. Measured and drawn in one layout, so a
 * navigation bar can ask for its intrinsic size.
 */
@Composable
fun WholeWordsText(
    text: String,
    style: TextStyle,
    modifier: Modifier = Modifier,
    maxLines: Int = 2,
) {
    val measurer = rememberTextMeasurer()
    val laid = remember { arrayOfNulls<TextLayoutResult>(1) }
    Layout(
        modifier = modifier
            .semantics { this.text = AnnotatedString(text) }
            .drawBehind { laid[0]?.let { drawText(it) } },
    ) { _, constraints ->
        val fitted = fitted(measurer, text, style, constraints.maxWidth, this)
        val result = measurer.measure(
            text,
            fitted,
            overflow = TextOverflow.Ellipsis,
            maxLines = maxLines,
            constraints = Constraints(maxWidth = constraints.maxWidth),
        )
        laid[0] = result
        layout(
            result.size.width.coerceIn(constraints.minWidth, constraints.maxWidth),
            result.size.height.coerceIn(constraints.minHeight, constraints.maxHeight),
        ) {}
    }
}

/**
 * [style], smaller by half points until the widest words fit [width] each
 * on a line, but not below [MinSize] as [density] draws it.
 */
private fun fitted(measurer: TextMeasurer, text: String, style: TextStyle, width: Int, density: Density): TextStyle {
    if (!style.fontSize.isSpecified || width <= 0 || width == Constraints.Infinity) return style
    val words = text.split(' ', '\n', '\t').filter { it.isNotEmpty() }.sortedByDescending { it.length }.take(3)
    val max = style.fontSize.value
    var size = max
    fun fits(s: Float) = words.all { measurer.measure(it, style.copy(fontSize = s.sp), maxLines = 1, softWrap = false).size.width <= width }
    fun drawn(s: Float) = with(density) { s.sp.toDp() }
    while (!fits(size) && size - Step > 0f && drawn(size - Step) >= MinSize) size -= Step
    if (size == max) return style
    val ratio = size / max
    return style.copy(fontSize = size.sp, lineHeight = style.lineHeight.scaled(ratio))
}

/**
 * One size for [texts] side by side, each in a slot [width] wide: the
 * largest, at most [style]'s, at which every word of every one fits its
 * slot whole, so a row of labels shares a size rather than each
 * shrinking on its own. Null when not even [minSize] is small enough: the
 * slots are too narrow. [minSize] is in sp, so it grows with the system's
 * font size, and a row can choose fewer, wider slots over text smaller
 * than its reader asked for.
 */
fun sharedFit(
    measurer: TextMeasurer,
    texts: List<String>,
    style: TextStyle,
    width: Int,
    minSize: TextUnit = MinSharedSize,
): TextStyle? {
    if (!style.fontSize.isSpecified || width <= 0) return null
    val max = style.fontSize.value
    val words = texts.flatMap { text -> text.split(' ', '\n', '\t').filter { it.isNotEmpty() } }.distinct()
    fun at(s: Float) = if (s == max) style else style.copy(fontSize = s.sp, lineHeight = style.lineHeight.scaled(s / max))
    fun fits(s: Float): Boolean {
        val sized = at(s)
        return words.all { measurer.measure(it, sized, maxLines = 1, softWrap = false).size.width <= width }
    }
    var size = max
    while (!fits(size)) {
        if (size - Step < minSize.value) return null
        size -= Step
    }
    return at(size)
}

/** The smallest a shared size gets: 9 sp, the size [WholeWordsText] stopped at before its floor was drawn dp. */
private val MinSharedSize = 9.sp

private fun TextUnit.scaled(ratio: Float): TextUnit = if (isSpecified) (value * ratio).sp else this

/** The smallest it gets, as drawn: 9 sp at the system's standard font size. */
private val MinSize = 9.dp
private const val Step = 0.5f
