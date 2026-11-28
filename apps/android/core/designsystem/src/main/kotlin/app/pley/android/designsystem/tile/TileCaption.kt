package app.pley.android.designsystem.tile

import androidx.compose.foundation.border
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.isSpecified
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.PleyType
import app.pley.android.designsystem.text.LocalMinTextSize
import app.pley.android.designsystem.text.WholeWordsText
import app.pley.android.designsystem.text.atLeast

/**
 * The two lines under a tile (design spec §2.3, §3.2): the title in [fg][PleyColors.Fg]
 * and the subtitle in [muted][PleyColors.Muted], one line each with an
 * ellipsis. On TV the title is weight 500, or 700 while the tile has focus
 * ([emphasized]); the weight switches, it never animates. On phones a
 * large font scale lets the title take two lines. Captions always show,
 * so rows never jump. Screen readers hear the tile's description instead.
 * Where text has a smallest size ([LocalMinTextSize], a car's screen),
 * both lines are that size at least, and the title takes two lines there
 * too, never smaller: at 24 sp one line holds a dozen letters of it.
 * On TV, where each line is one line, a single node draws both
 * ([tvCaption]): grids bring in a row of these at each step of a held key.
 */
@Composable
fun TileCaption(
    title: String,
    subtitle: String,
    style: TileStyle,
    modifier: Modifier = Modifier,
    emphasized: Boolean = false,
) {
    val palette = PleyColors.palette
    if (style == TileStyle.Tv && inCaptionFont(title) && inCaptionFont(subtitle)) {
        val colors = remember(palette) { CaptionColors(palette.fg.toArgb(), palette.muted.toArgb()) }
        Spacer(modifier.tvCaption(title, subtitle, emphasized, colors))
        return
    }
    val floor = LocalMinTextSize.current
    val titleLines = if (style == TileStyle.Phone && (LocalDensity.current.fontScale > 1.3f || floor.isSpecified)) 2 else 1
    val titleStyle = remember(style, emphasized, floor, palette) {
        style.captionTitle.atLeast(floor).copy(color = palette.fg, fontWeight = if (emphasized) PleyType.Bold else style.captionTitle.fontWeight)
    }
    val subtitleStyle = remember(style, floor, palette) { style.captionSubtitle.atLeast(floor).copy(color = palette.muted) }
    Column(modifier.clearAndSetSemantics {}) {
        if (titleLines > 1) {
            // On two lines, a word too wide for the tile makes the title smaller rather than break in two; not under a floor.
            WholeWordsText(title, titleStyle, maxLines = titleLines, shrinks = !floor.isSpecified)
        } else {
            BasicText(title, style = titleStyle, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
        if (subtitle.isNotEmpty()) {
            BasicText(subtitle, style = subtitleStyle, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
    }
}

/**
 * A quality or content-rating badge (design spec §2.6): "4K", "DV P8.1",
 * "R". On art it sits on [PleyColors.OnArt] with [fg][PleyColors.Fg] text;
 * on the page it is transparent with a 1 dp [outline][PleyColors.Outline]
 * border and [muted][PleyColors.Muted] text: decoration, never a control.
 * Its text is [LocalMinTextSize] at least, the badge as tall as that takes.
 */
@Composable
fun QualityBadge(
    text: String,
    style: TileStyle,
    modifier: Modifier = Modifier,
    onArt: Boolean = false,
) {
    val floor = LocalMinTextSize.current
    val palette = PleyColors.palette
    val textStyle = remember(style, onArt, floor, palette) { style.badgeText.atLeast(floor).copy(color = if (onArt) palette.fg else palette.muted) }
    val look = if (onArt) {
        Modifier.background(PleyColors.OnArt, PleyShape.Badge)
    } else {
        Modifier.border(1.dp, PleyColors.Outline, PleyShape.Badge)
    }
    Box(
        modifier.heightIn(min = style.badgeHeight).then(look).padding(horizontal = 6.dp),
        contentAlignment = Alignment.Center,
    ) {
        BasicText(text, style = textStyle, maxLines = 1, softWrap = false, overflow = TextOverflow.Ellipsis)
    }
}
