package app.pley.android.designsystem.tile

import android.graphics.Canvas
import android.graphics.Paint
import android.text.TextPaint
import android.text.TextUtils
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.drawscope.ContentDrawScope
import androidx.compose.ui.graphics.nativeCanvas
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.layout.Measurable
import androidx.compose.ui.layout.MeasureResult
import androidx.compose.ui.layout.MeasureScope
import androidx.compose.ui.node.CompositionLocalConsumerModifierNode
import androidx.compose.ui.node.DrawModifierNode
import androidx.compose.ui.node.LayoutModifierNode
import androidx.compose.ui.node.ModifierNodeElement
import androidx.compose.ui.node.currentValueOf
import androidx.compose.ui.node.invalidateDraw
import androidx.compose.ui.node.invalidateMeasurement
import androidx.compose.ui.platform.InspectorInfo
import androidx.compose.ui.platform.LocalFontFamilyResolver
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontSynthesis
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.font.resolveAsTypeface
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.constrainHeight
import androidx.compose.ui.unit.constrainWidth
import androidx.compose.ui.unit.LayoutDirection
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyType
import kotlin.math.ceil

/**
 * A TV tile's caption ([TileCaption] on TV): [title] over [subtitle], each
 * one line cut with an ellipsis, laid out and drawn by this one node. A
 * line that never wraps needs no paragraph: it is measured, cut and drawn
 * with the platform's paint, where BasicText lays out a paragraph for each
 * (on a Google TV Streamer half a millisecond a line, a dozen lines in each
 * row a held key brings into a grid). It draws as BasicText did, in the
 * same typeface, size, tracking and line box, which TvCaptionTest checks
 * pixel for pixel (in a right-to-left layout, to within a pixel or two).
 * Only for text in the caption's own font ([inCaptionFont]).
 */
internal fun Modifier.tvCaption(title: String, subtitle: String, emphasized: Boolean): Modifier =
    this then TvCaptionElement(title, subtitle, emphasized)

/**
 * Whether [text] is all in Roboto's own scripts (Latin, Greek and Cyrillic)
 * and punctuation, which [tvCaption] draws in the caption's line box. A
 * script from another font (Arabic, Hebrew, Thai, CJK, emoji) has taller
 * lines, which BasicText makes room for, so those captions stay with it.
 */
internal fun inCaptionFont(text: String): Boolean = text.all { it < '԰' || it in '‐'..'⁞' }

private data class TvCaptionElement(
    val title: String,
    val subtitle: String,
    val emphasized: Boolean,
) : ModifierNodeElement<TvCaptionNode>() {
    override fun create() = TvCaptionNode(title, subtitle, emphasized)

    override fun update(node: TvCaptionNode) = node.update(title, subtitle, emphasized)

    override fun InspectorInfo.inspectableProperties() {
        name = "tvCaption"
        properties["title"] = title
        properties["subtitle"] = subtitle
        properties["emphasized"] = emphasized
    }
}

private class TvCaptionNode(
    private var title: String,
    private var subtitle: String,
    private var emphasized: Boolean,
) : Modifier.Node(), LayoutModifierNode, DrawModifierNode, CompositionLocalConsumerModifierNode {
    private val titleLine = CaptionLine()
    private val subtitleLine = CaptionLine()
    private var lineHeight = 0

    override val shouldAutoInvalidate: Boolean get() = false

    fun update(title: String, subtitle: String, emphasized: Boolean) {
        if (title == this.title && subtitle == this.subtitle && emphasized == this.emphasized) return
        this.title = title
        this.subtitle = subtitle
        this.emphasized = emphasized
        invalidateMeasurement()
        invalidateDraw()
    }

    override fun MeasureScope.measure(measurable: Measurable, constraints: Constraints): MeasureResult {
        val paints = CaptionPaints.of(this, currentValueOf(LocalFontFamilyResolver))
        titleLine.paint = if (emphasized) paints.titleBold else paints.title
        subtitleLine.paint = paints.subtitle
        lineHeight = paints.lineHeight
        val lines = if (subtitle.isEmpty()) 1 else 2
        val width = if (constraints.hasBoundedWidth) {
            constraints.maxWidth
        } else {
            ceil(maxOf(titleLine.paint.paint.measureText(title), if (lines > 1) subtitleLine.paint.paint.measureText(subtitle) else 0f)).toInt()
        }
        val rtl = layoutDirection == LayoutDirection.Rtl
        titleLine.fit(title, width, rtl)
        subtitleLine.fit(subtitle, width, rtl)
        val w = constraints.constrainWidth(width)
        val h = constraints.constrainHeight(lineHeight * lines)
        val placeable = measurable.measure(Constraints.fixed(w, h))
        return layout(w, h) { placeable.place(0, 0) }
    }

    override fun ContentDrawScope.draw() {
        drawContent()
        val canvas = drawContext.canvas.nativeCanvas
        titleLine.draw(canvas, size.width, 0f)
        if (subtitle.isNotEmpty()) subtitleLine.draw(canvas, size.width, lineHeight.toFloat())
    }
}

/** One line of a caption: its paint (shared, [CaptionPaints]), the text as it fits, and where it sits. */
private class CaptionLine {
    lateinit var paint: CaptionPaint
    private var shown: CharSequence = ""
    private var width = 0f
    private var rtl = false

    /** [text] on one line, cut with an ellipsis to [maxWidth], in a layout [rtl] or not. */
    fun fit(text: String, maxWidth: Int, rtl: Boolean) {
        // One line, as BasicText's maxLines = 1 shows: what comes after a line break doesn't.
        val line = text.substringBefore('\n')
        val paint = paint.paint
        // The advance as it is (measureText rounds it up): measured once, and cut only when it doesn't fit.
        val whole = paint.getRunAdvance(line, 0, line.length, 0, line.length, false, line.length)
        if (whole <= maxWidth) {
            shown = line
            width = whole
        } else {
            shown = TextUtils.ellipsize(line, paint, maxWidth.toFloat(), TextUtils.TruncateAt.END)
            width = paint.getRunAdvance(shown, 0, shown.length, 0, shown.length, false, shown.length)
        }
        this.rtl = rtl
    }

    fun draw(canvas: Canvas, boxWidth: Float, top: Float) {
        if (shown.isEmpty()) return
        // At the start: the left, or the right in a right-to-left layout. BasicText sat within a
        // pixel or so of that edge there, its box rounded up from the text's width.
        val x = if (rtl) boxWidth - width else 0f
        canvas.drawText(shown, 0, shown.length, x, top + paint.baseline, paint.paint)
    }
}

/** A caption line's paint and its baseline in the line box. */
private class CaptionPaint(val paint: TextPaint, val baseline: Int)

/**
 * The captions' paints, the same for every tile: made once for the
 * screen's density and font scale, not for each tile a grid brings in.
 * Used on the main thread only, where tiles are measured and drawn.
 */
private class CaptionPaints private constructor(
    val title: CaptionPaint,
    val titleBold: CaptionPaint,
    val subtitle: CaptionPaint,
    val lineHeight: Int,
) {
    companion object {
        private var made: CaptionPaints? = null
        private var density = 0f
        private var fontScale = 0f
        private var resolver: FontFamily.Resolver? = null

        fun of(density: Density, resolver: FontFamily.Resolver): CaptionPaints {
            made?.let {
                if (density.density == this.density && density.fontScale == fontScale && resolver === this.resolver) return it
            }
            val style = TileStyle.Tv
            val lineHeight = with(density) { ceil(style.captionTitle.lineHeight.toPx()).toInt() }
            fun paint(text: TextStyle, weight: FontWeight, color: Int): CaptionPaint {
                val paint = TextPaint(Paint.ANTI_ALIAS_FLAG).apply { this.color = color }
                density.setUp(paint, text, weight, resolver)
                return CaptionPaint(paint, baselineIn(paint.fontMetricsInt, lineHeight))
            }
            val medium = style.captionTitle.fontWeight ?: PleyType.Medium
            return CaptionPaints(
                title = paint(style.captionTitle, medium, PleyColors.Fg.toArgb()),
                titleBold = paint(style.captionTitle, PleyType.Bold, PleyColors.Fg.toArgb()),
                subtitle = paint(style.captionSubtitle, style.captionSubtitle.fontWeight ?: PleyType.Medium, PleyColors.Muted.toArgb()),
                lineHeight = lineHeight,
            ).also {
                made = it
                this.density = density.density
                fontScale = density.fontScale
                this.resolver = resolver
            }
        }
    }
}

/** Sets [paint] to draw [style] in [weight], as Compose's text does. */
private fun Density.setUp(paint: TextPaint, style: TextStyle, weight: FontWeight, resolver: FontFamily.Resolver) {
    paint.textSize = style.fontSize.toPx()
    paint.typeface = resolver.resolveAsTypeface(style.fontFamily, weight, FontStyle.Normal, FontSynthesis.All).value
    // The platform takes tracking in ems: tracking in sp is a fraction of the size.
    val tracking = style.letterSpacing
    paint.letterSpacing = when {
        tracking.isSp -> tracking.toPx() / paint.textSize
        tracking.isEm -> tracking.value
        else -> 0f
    }
}

/**
 * The baseline of one line in a box [lineHeight] tall, as Compose places it
 * for PleyType's line box (LineHeightStyle.Alignment.Center, Trim.None,
 * without the font's padding): the room the line has beyond the font's
 * ascent and descent goes half above and half below, the odd pixel below.
 */
internal fun baselineIn(metrics: Paint.FontMetricsInt, lineHeight: Int): Int {
    val diff = lineHeight - (metrics.descent - metrics.ascent)
    val descent = metrics.descent + ceil(diff / 2f).toInt()
    return lineHeight - descent
}
