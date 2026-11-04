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
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.PleyType
import app.pley.android.designsystem.text.WholeWordsText

/**
 * The two lines under a tile (design spec §2.3, §3.2): the title in [fg][PleyColors.Fg]
 * and the subtitle in [muted][PleyColors.Muted], one line each with an
 * ellipsis. On TV the title is weight 500, or 700 while the tile has focus
 * ([emphasized]); the weight switches, it never animates. On phones a
 * large font scale lets the title take two lines. Captions always show,
 * so rows never jump. Screen readers hear the tile's description instead.
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
    if (style == TileStyle.Tv && inCaptionFont(title) && inCaptionFont(subtitle)) {
        Spacer(modifier.tvCaption(title, subtitle, emphasized))
        return
    }
    val titleLines = if (style == TileStyle.Phone && LocalDensity.current.fontScale > 1.3f) 2 else 1
    val titleStyle = remember(style, emphasized) {
        style.captionTitle.copy(color = PleyColors.Fg, fontWeight = if (emphasized) PleyType.Bold else style.captionTitle.fontWeight)
    }
    val subtitleStyle = remember(style) { style.captionSubtitle.copy(color = PleyColors.Muted) }
    Column(modifier.clearAndSetSemantics {}) {
        if (titleLines > 1) {
            // On two lines, a word too wide for the tile makes the title smaller rather than break in two.
            WholeWordsText(title, titleStyle, maxLines = titleLines)
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
 */
@Composable
fun QualityBadge(
    text: String,
    style: TileStyle,
    modifier: Modifier = Modifier,
    onArt: Boolean = false,
) {
    val textStyle = remember(style, onArt) { style.badgeText.copy(color = if (onArt) PleyColors.Fg else PleyColors.Muted) }
    val look = if (onArt) {
        Modifier.background(PleyColors.OnArt, PleyShape.Badge)
    } else {
        Modifier.border(1.dp, PleyColors.Outline, PleyShape.Badge)
    }
    Box(
        modifier.heightIn(min = style.badgeHeight).then(look).padding(horizontal = 6.dp),
        contentAlignment = Alignment.Center,
    ) {
        BasicText(text, style = textStyle, maxLines = 1, softWrap = false)
    }
}
