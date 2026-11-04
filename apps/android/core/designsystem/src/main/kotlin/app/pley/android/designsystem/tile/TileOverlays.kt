package app.pley.android.designsystem.tile

import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.drawscope.ContentDrawScope
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.drawscope.clipRect
import androidx.compose.ui.graphics.drawscope.translate
import androidx.compose.ui.node.CompositionLocalConsumerModifierNode
import androidx.compose.ui.node.DrawModifierNode
import androidx.compose.ui.node.ModifierNodeElement
import androidx.compose.ui.node.currentValueOf
import androidx.compose.ui.node.invalidateDraw
import androidx.compose.ui.platform.InspectorInfo
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalFontFamilyResolver
import androidx.compose.ui.platform.LocalLayoutDirection
import androidx.compose.ui.text.TextLayoutResult
import androidx.compose.ui.text.TextMeasurer
import androidx.compose.ui.text.drawText
import androidx.compose.ui.unit.dp
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.image.ArtworkLoad
import app.pley.android.designsystem.image.ArtworkTrace

/** Draws [model]'s overlays over the art, and counts the tile's first frame for the image budget. */
internal fun Modifier.tileOverlays(model: ArtworkTileModel, kind: TileKind, style: TileStyle, load: ArtworkLoad): Modifier =
    this then TileOverlaysElement(model.progress, model.watched, model.unwatched, model.badges, kind, style, load)

private data class TileOverlaysElement(
    val progress: Float,
    val watched: Boolean,
    val unwatched: Int,
    val badges: List<String>,
    val kind: TileKind,
    val style: TileStyle,
    val load: ArtworkLoad,
) : ModifierNodeElement<TileOverlaysNode>() {
    override fun create() = TileOverlaysNode(this)

    override fun update(node: TileOverlaysNode) = node.update(this)

    override fun InspectorInfo.inspectableProperties() {
        name = "tileOverlays"
        properties["progress"] = progress
        properties["watched"] = watched
        properties["unwatched"] = unwatched
        properties["badges"] = badges
    }
}

private class TileOverlaysNode(var spec: TileOverlaysElement) :
    Modifier.Node(), DrawModifierNode, CompositionLocalConsumerModifierNode {
    private var measurer: TextMeasurer? = null
    private var badgeLayouts: List<TextLayoutResult>? = null
    private var countLayout: TextLayoutResult? = null
    private var tick: Path? = null
    private var tickSize = 0f
    private var counted = false

    override val shouldAutoInvalidate: Boolean get() = false

    fun update(next: TileOverlaysElement) {
        if (next.badges != spec.badges || next.style != spec.style) badgeLayouts = null
        if (next.unwatched != spec.unwatched || next.style != spec.style) countLayout = null
        if (next.load !== spec.load) counted = false
        spec = next
        invalidateDraw()
    }

    override fun ContentDrawScope.draw() {
        drawContent()
        if (!counted) {
            counted = true
            ArtworkTrace.entered(spec.load.loaded)
        }
        val s = spec
        val inset = s.style.inset.toPx()
        if (!s.kind.round) {
            val half = 0.5.dp.toPx()
            val radius = s.style.cornerRadius.toPx() - half
            drawRoundRect(
                color = PleyColors.TileHairline,
                topLeft = Offset(half, half),
                size = Size(size.width - 2 * half, size.height - 2 * half),
                cornerRadius = CornerRadius(radius, radius),
                style = Stroke(width = 1.dp.toPx()),
            )
        }
        val barHeight = if (s.progress > 0f) s.style.progressHeight.toPx() else 0f
        if (barHeight > 0f) {
            // The tile's own rounded rectangle cut to the bar: its bottom corners round with the tile's
            // while the tile isn't clipped (artworkPlaceholder), and a rectangle's clip costs the GPU nothing.
            val top = size.height - barHeight
            val corner = if (s.kind.round) CornerRadius(size.minDimension / 2f) else CornerRadius(s.style.cornerRadius.toPx())
            clipRect(top = top) { drawRoundRect(PleyColors.ProgressTrackOnArt, cornerRadius = corner) }
            clipRect(top = top, right = size.width * s.progress.coerceIn(0f, 1f)) { drawRoundRect(PleyColors.Accent, cornerRadius = corner) }
        }
        when {
            s.unwatched > 0 -> drawCount(s, inset)
            s.watched -> drawTick(s, inset)
        }
        if (s.badges.isNotEmpty()) drawBadges(s, inset, barHeight)
    }

    private fun DrawScope.drawTick(s: TileOverlaysElement, inset: Float) {
        val d = s.style.tickSize.toPx()
        val left = size.width - inset - d
        val r = PleyShape.BadgeRadius.toPx()
        drawRoundRect(PleyColors.OnArt, Offset(left, inset), Size(d, d), CornerRadius(r, r))
        val glyph = s.style.tickGlyph.toPx()
        if (tick == null || tickSize != glyph) {
            // Lucide's check, "M20 6 9 17l-5-5", on its 24 grid.
            val k = glyph / 24f
            tick = Path().apply {
                moveTo(20f * k, 6f * k)
                lineTo(9f * k, 17f * k)
                lineTo(4f * k, 12f * k)
            }
            tickSize = glyph
        }
        translate(left + (d - glyph) / 2f, inset + (d - glyph) / 2f) {
            drawPath(tick!!, PleyColors.Accent, style = Stroke(3f * glyph / 24f, cap = StrokeCap.Round, join = StrokeJoin.Round))
        }
    }

    private fun DrawScope.drawCount(s: TileOverlaysElement, inset: Float) {
        val text = countLayout ?: measure(s.unwatched.toString(), s).also { countLayout = it }
        val h = s.style.countHeight.toPx()
        val w = maxOf(h + 2.dp.toPx(), text.size.width + 2 * 6.dp.toPx())
        val left = size.width - inset - w
        val r = PleyShape.BadgeRadius.toPx()
        drawRoundRect(PleyColors.Accent, Offset(left, inset), Size(w, h), CornerRadius(r, r))
        drawText(
            text,
            color = PleyColors.OnAccent,
            topLeft = Offset(left + (w - text.size.width) / 2f, inset + (h - text.size.height) / 2f),
        )
    }

    private fun DrawScope.drawBadges(s: TileOverlaysElement, inset: Float, barHeight: Float) {
        val layouts = badgeLayouts ?: s.badges.map { measure(it, s) }.also { badgeLayouts = it }
        val h = s.style.badgeHeight.toPx()
        val pad = 6.dp.toPx()
        val gap = 6.dp.toPx()
        val radius = PleyShape.BadgeRadius.toPx()
        val top = size.height - barHeight - inset - h
        var x = inset
        for (text in layouts) {
            val w = text.size.width + 2 * pad
            if (x + w > size.width - inset) break
            drawRoundRect(PleyColors.OnArt, Offset(x, top), Size(w, h), CornerRadius(radius, radius))
            drawText(text, color = PleyColors.Fg, topLeft = Offset(x + pad, top + (h - text.size.height) / 2f))
            x += w + gap
        }
    }

    private fun measure(text: String, s: TileOverlaysElement): TextLayoutResult {
        val m = measurer ?: TextMeasurer(
            defaultFontFamilyResolver = currentValueOf(LocalFontFamilyResolver),
            defaultDensity = currentValueOf(LocalDensity),
            defaultLayoutDirection = currentValueOf(LocalLayoutDirection),
            cacheSize = 0,
        ).also { measurer = it }
        return m.measure(text, s.style.badgeText, maxLines = 1, softWrap = false)
    }
}
