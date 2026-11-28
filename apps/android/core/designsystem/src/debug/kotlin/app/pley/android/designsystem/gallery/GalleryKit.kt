package app.pley.android.designsystem.gallery

import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.focusable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.vector.rememberVectorPainter
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.zIndex
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyFocus
import app.pley.android.designsystem.PleyMotion
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.icon.Icon
import app.pley.android.designsystem.icon.PleyIcon
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.designsystem.icon.PleyMark
import app.pley.android.designsystem.reducedMotion
import app.pley.android.designsystem.theme.PleyPalette
import app.pley.android.designsystem.theme.PleyTheme
import app.pley.android.designsystem.theme.ThemePreview
import app.pley.android.designsystem.theme.nameRes
import app.pley.android.designsystem.tile.ArtworkTile
import app.pley.android.designsystem.tile.ArtworkTileModel
import app.pley.android.designsystem.tile.TileCaption
import app.pley.android.designsystem.tile.TileKind
import app.pley.android.designsystem.tile.TileStyle
import app.pley.android.designsystem.tile.tileGlow
import app.pley.android.designsystem.tile.tvTileSize

/**
 * A stand-in for the TV shell's TvTile (Phase 3), built only from this
 * module's pieces so the gallery shows tiles as the TV will: scale 1.08 in
 * the layer, the accent ring drawn around it, the glow on T1 and T2, the
 * caption's title bold while focused. [focus] forces the look for the
 * still sample row; null follows real focus.
 */
@Composable
internal fun GalleryTvTile(
    model: ArtworkTileModel,
    kind: TileKind,
    glow: Boolean,
    modifier: Modifier = Modifier,
    focus: Boolean? = null,
) {
    var focused by remember { mutableStateOf(false) }
    val on = focus ?: focused
    val reduced = reducedMotion()
    val scale by animateFloatAsState(if (on) PleyFocus.TILE_SCALE else 1f, if (on) PleyMotion.focus(reduced) else PleyMotion.unfocus(reduced), label = "scale")
    val ring by animateFloatAsState(if (on) 1f else 0f, if (on) PleyMotion.focus(reduced) else PleyMotion.unfocus(reduced), label = "ring")
    Column(modifier.width(kind.tvWidth).zIndex(if (on) 1f else 0f)) {
        Box(
            Modifier
                .onFocusChanged { focused = it.isFocused }
                .focusable()
                .graphicsLayer {
                    scaleX = scale
                    scaleY = scale
                }
                .then(if (glow) Modifier.tileGlow(kind) { ring } else Modifier)
                .focusRing(kind) { ring },
        ) {
            ArtworkTile(model, kind, TileStyle.Tv, Modifier.tvTileSize(kind))
        }
        Spacer(Modifier.height(TileStyle.Tv.captionGap))
        TileCaption(model.title, model.subtitle, TileStyle.Tv, emphasized = on)
    }
}

/** The TV focus ring of §1.4, drawn after the tile at [alpha]. */
@Composable
internal fun Modifier.focusRing(kind: TileKind, alpha: () -> Float): Modifier {
    val ring = PleyColors.AccentBright
    return focusRing(kind, ring, alpha)
}

private fun Modifier.focusRing(kind: TileKind, ring: Color, alpha: () -> Float): Modifier = drawWithContent {
    drawContent()
    val a = alpha()
    if (a > 0f) {
        val offset = PleyFocus.RingOffset.toPx()
        val radius = if (kind.round) (size.minDimension / 2f + offset) else PleyFocus.RingRadius.toPx()
        drawRoundRect(
            color = ring.copy(alpha = a),
            topLeft = Offset(-offset, -offset),
            size = Size(size.width + 2 * offset, size.height + 2 * offset),
            cornerRadius = CornerRadius(radius, radius),
            style = Stroke(PleyFocus.RingStroke.toPx()),
        )
    }
}

@Composable
internal fun Label(text: String, style: TextStyle, modifier: Modifier = Modifier, color: Color = PleyColors.Fg) {
    BasicText(text, modifier, style = style.copy(color = color))
}

/** The token swatches, as tv-components shows them. */
@Composable
internal fun Swatches(style: TextStyle, swatch: Dp, columns: Int = 14) {
    val tokens = listOf(
        "bg" to PleyColors.Bg, "surface" to PleyColors.Surface, "surface2" to PleyColors.Surface2,
        "surface3" to PleyColors.Surface3, "surface4" to PleyColors.Surface4, "outline" to PleyColors.Outline,
        "control" to PleyColors.Control, "fg" to PleyColors.Fg, "muted" to PleyColors.Muted, "subtle" to PleyColors.Subtle,
        "accent" to PleyColors.Accent, "accentBright" to PleyColors.AccentBright, "onAccent" to PleyColors.OnAccent,
        "dangerText" to PleyColors.DangerText, "danger" to PleyColors.Danger, "success" to PleyColors.Success,
        "info" to PleyColors.Info, "warning" to PleyColors.Warning, "dangerFg" to PleyColors.DangerFg,
        "brandLight" to PleyColors.BrandLight, "brand" to PleyColors.Brand, "brandDeep" to PleyColors.BrandDeep,
        "brandFar" to PleyColors.BrandFar, "brandTile" to PleyColors.BrandTile, "primaryContainer" to PleyColors.PrimaryContainer,
    )
    Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
        tokens.chunked(columns).forEach { line ->
            Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                line.forEach { (name, colour) ->
                    Column(Modifier.width(swatch)) {
                        Box(
                            Modifier
                                .size(swatch, swatch * 0.45f)
                                .background(colour, PleyShape.Badge)
                                .border(1.dp, PleyColors.Surface4, PleyShape.Badge),
                        )
                        Spacer(Modifier.height(4.dp))
                        Label(name, style, color = PleyColors.Muted)
                        Label("#" + Integer.toHexString(colour.toArgbInt()).takeLast(6), style, color = PleyColors.Muted)
                    }
                }
            }
        }
    }
}

/**
 * The nine themes as Appearance shows them, each previewed in its own
 * colors with the gallery's accent, and its mark on its tile: [columns] to
 * a row, [width] wide.
 */
@Composable
internal fun ThemesGrid(style: TextStyle, width: Dp, columns: Int) {
    val accent = PleyColors.palette.pick
    Column(verticalArrangement = Arrangement.spacedBy(14.dp)) {
        PleyTheme.entries.chunked(columns).forEach { line ->
            Row(horizontalArrangement = Arrangement.spacedBy(14.dp)) {
                line.forEach { theme ->
                    val palette = PleyPalette.of(theme, accent)
                    Column(Modifier.width(width)) {
                        ThemePreview(palette, Modifier.fillMaxWidth())
                        Spacer(Modifier.height(6.dp))
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            Image(rememberVectorPainter(PleyMark.tile(palette)), contentDescription = null, modifier = Modifier.size(20.dp))
                            Spacer(Modifier.width(6.dp))
                            Label(stringResource(theme.nameRes), style)
                        }
                    }
                }
            }
        }
    }
}

private fun Color.toArgbInt(): Int = android.graphics.Color.argb(alpha, red, green, blue)

/** Icons the screens use, with Lucide's names. */
internal val galleryIcons: List<PleyIcon> = with(PleyIcons) {
    listOf(
        Search, House, Clapperboard, Tv, Music, Image, Film, Layers, ListMusic, Settings, UserRound,
        Play, Pause, SkipBack, SkipForward, RotateCcw, RotateCw, Captions, AudioLines, Gauge, SlidersHorizontal,
        PictureInPicture2, Info, Check, Star, Eye, EyeOff, Plus, ListPlus, Ellipsis, Lock, Delete, ArrowRight,
        ChevronRight, X, Server, Wifi, RefreshCw, LogOut, TriangleAlert,
    )
}

@Composable
internal fun IconGrid(size: Dp, columns: Int, label: TextStyle, cell: Dp) {
    Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
        galleryIcons.chunked(columns).forEach { line ->
            Row {
                line.forEach { icon ->
                    Column(Modifier.width(cell), horizontalAlignment = Alignment.CenterHorizontally) {
                        Icon(icon, contentDescription = icon.name, size = size, tint = if (icon.filled) PleyColors.Accent else PleyColors.Fg)
                        Spacer(Modifier.height(4.dp))
                        Label(icon.name, label, Modifier.padding(horizontal = 2.dp), PleyColors.Muted)
                    }
                }
            }
        }
    }
}
