package app.pley.android.designsystem.theme

import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawWithCache
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.FilterQuality
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.dp
import kotlin.math.roundToInt

/**
 * A small Pley in [palette]'s colors, as the web's picker shows each theme
 * (ThemeCard.tsx): its sidebar, a heading with a button, a line of text and
 * a row of posters, the first one focused, over the theme's page and glow.
 * 16:9, drawn in one node.
 */
@Composable
fun ThemePreview(palette: PleyPalette, modifier: Modifier = Modifier, corner: Dp = 6.dp) {
    // The page's glow, drawn small: the preview is a 16:9 window.
    val glow = remember(palette.theme) { Backdrop.page(palette.theme, palette.bg, 160, 90)?.asImageBitmap() }
    val sidebar = remember(palette.theme) { Backdrop.sidebar(palette.theme) }
    Spacer(
        modifier.aspectRatio(16f / 9f).clip(RoundedCornerShape(corner)).drawWithCache {
            // The web's preview is about 200 px wide: its sizes, in that unit.
            val u = size.width / 200f
            val radius = CornerRadius(corner.toPx())
            onDrawBehind {
                drawRect(palette.bg)
                if (glow != null) {
                    drawImage(glow, IntOffset.Zero, IntSize(glow.width, glow.height), IntOffset.Zero, IntSize(size.width.roundToInt(), size.height.roundToInt()), filterQuality = FilterQuality.Low)
                }
                val side = size.width / 4f
                // The sidebar: its surface (Sunshine's rainbow over it), a line on its edge, an accent bar and three of text.
                drawRect(palette.surface, size = Size(side, size.height))
                if (sidebar != null) drawRect(Brush.verticalGradient(*sidebar.toTypedArray()), size = Size(side, size.height))
                drawRect(palette.border, Offset(side - u, 0f), Size(u, size.height))
                val pad = 8 * u
                val inner = side - 2 * pad
                listOf(0.75f to palette.accent, 1f to palette.fg.copy(alpha = 0.15f), 0.67f to palette.fg.copy(alpha = 0.15f), 0.83f to palette.fg.copy(alpha = 0.15f))
                    .forEachIndexed { i, (share, color) -> bar(color, Offset(pad, pad + i * 12 * u), Size(inner * share, 6 * u)) }
                // The page: a heading and its button, a line, and four posters at the foot.
                val left = side + pad
                val width = size.width - left - pad
                bar(palette.fg.copy(alpha = 0.8f), Offset(left, pad + u), Size(width / 2f, 8 * u))
                bar(palette.accent, Offset(left + width * 0.75f, pad), Size(width / 4f, 10 * u))
                bar(palette.muted.copy(alpha = 0.5f), Offset(left, pad + 16 * u), Size(width / 3f, 6 * u))
                val gap = 4 * u
                val poster = (width - 3 * gap) / 4f
                val tall = poster * 1.5f
                val top = size.height - pad - tall
                for (i in 0 until 4) {
                    val at = Offset(left + i * (poster + gap), top)
                    drawRoundRect(palette.surface3, at, Size(poster, tall), CornerRadius(2 * u))
                    if (i == 0) drawRoundRect(palette.accent, at, Size(poster, tall), CornerRadius(2 * u), style = Stroke(u))
                }
                // Its edge: the words' color at 10 %, as the web's ring.
                drawRoundRect(palette.fg.copy(alpha = 0.1f), cornerRadius = radius, style = Stroke(2 * u))
            }
        },
    )
}

/** A rounded bar, as the web's `rounded-full` spans. */
private fun DrawScope.bar(color: Color, at: Offset, size: Size) =
    drawRoundRect(color, at, size, CornerRadius(size.height / 2f))
