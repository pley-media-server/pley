package app.pley.android.ui.tv.playlists

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsFocusedAsState
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawWithCache
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusProperties
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.PleyTv
import app.pley.android.designsystem.PleyType
import app.pley.android.designsystem.icon.Icon
import app.pley.android.designsystem.icon.PleyIcon
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.designsystem.image.Artwork
import app.pley.android.designsystem.image.ArtworkImage
import app.pley.android.designsystem.image.MarkGlyph

/**
 * A playlist's square cover on TV (the web's PlaylistArt): a 2 by 2
 * mosaic of its first four entries, or its first entry's picture, or,
 * empty, its kind's glyph on a [surface3][PleyColors.Surface3] to
 * [surface2][PleyColors.Surface2] gradient, with the tile's 1 dp
 * hairline.
 */
@Composable
internal fun TvPlaylistArt(
    artwork: List<String>,
    origin: String,
    video: Boolean,
    title: String,
    modifier: Modifier = Modifier,
    shape: Shape = PleyShape.TileTv,
    glyph: Dp = 48.dp,
) {
    val images = remember(artwork, origin) { artwork.mapNotNull { ArtworkImage.of(it, origin) } }
    Box(
        modifier
            .clip(shape)
            .background(PleyColors.Surface2)
            .drawWithCache {
                val hair = 1.dp.toPx()
                onDrawWithContent {
                    drawContent()
                    drawRect(PleyColors.TileHairline, style = Stroke(hair))
                }
            },
    ) {
        when {
            images.size >= 4 -> Column(Modifier.fillMaxSize()) {
                for (r in 0 until 2) {
                    Row(Modifier.fillMaxWidth().weight(1f)) {
                        for (c in 0 until 2) Artwork(images[r * 2 + c], "", Modifier.weight(1f).fillMaxHeight())
                    }
                }
            }
            // Should the picture not come, Pley's mark: music's glyph for tracks, play for movies and episodes alike. The
            // title isn't on it: the caption or the row's line beside the cover names it.
            images.isNotEmpty() -> Artwork(
                images.first(),
                title,
                Modifier.fillMaxSize(),
                mark = if (video) MarkGlyph.Play else MarkGlyph.Music,
            )
            else -> Box(
                Modifier
                    .fillMaxSize()
                    .drawWithCache {
                        val brush = Brush.linearGradient(listOf(PleyColors.Surface3, PleyColors.Surface2))
                        onDrawBehind { drawRect(brush) }
                    },
                contentAlignment = Alignment.Center,
            ) {
                Icon(if (video) PleyIcons.ListVideo else PleyIcons.ListMusic, contentDescription = null, size = glyph, tint = PleyColors.Subtle)
            }
        }
    }
}

/**
 * A row of a side panel (design spec §2.7): 44 tall, its [label] in
 * tvBody with an optional 20 dp [icon] and a [supporting] line; a
 * [checked] row has an [accent][PleyColors.Accent] check. Focused, it is
 * an accent pill with [onAccent][PleyColors.OnAccent] content, never
 * scaled. A [danger] row reads in [dangerText][PleyColors.DangerText].
 */
@Composable
internal fun TvPanelRow(
    label: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    icon: PleyIcon? = null,
    supporting: String? = null,
    checked: Boolean = false,
    danger: Boolean = false,
    enabled: Boolean = true,
    focusRequester: FocusRequester? = null,
    leading: (@Composable () -> Unit)? = null,
) {
    val interactions = remember { MutableInteractionSource() }
    val focused by interactions.collectIsFocusedAsState()
    val rest = if (danger) PleyColors.DangerText else PleyColors.Fg
    val content = if (focused) PleyColors.OnAccent else rest
    Row(
        modifier
            .fillMaxWidth()
            .heightIn(min = PleyTv.ListRowHeight)
            .alpha(if (enabled) 1f else 0.4f)
            .then(if (focusRequester != null) Modifier.focusRequester(focusRequester) else Modifier)
            .focusProperties { canFocus = enabled }
            .background(if (focused) PleyColors.Accent else Color.Transparent, PleyShape.Pill)
            .semantics { selected = checked }
            .clickable(interactionSource = interactions, indication = null, enabled = enabled, role = Role.Button, onClick = onClick)
            .padding(horizontal = 16.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (leading != null) {
            leading()
            Spacer(Modifier.width(12.dp))
        } else if (icon != null) {
            Icon(icon, contentDescription = null, size = PleyTv.IconSize, tint = content)
            Spacer(Modifier.width(12.dp))
        }
        Column(Modifier.weight(1f)) {
            BasicText(label, style = PleyType.Tv.Body.copy(color = content), maxLines = 2, overflow = TextOverflow.Ellipsis)
            if (!supporting.isNullOrEmpty()) {
                BasicText(
                    supporting,
                    style = PleyType.Tv.Caption.copy(color = if (focused) PleyColors.OnAccent else PleyColors.Muted),
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }
        if (checked) {
            Spacer(Modifier.width(12.dp))
            Icon(PleyIcons.Check, contentDescription = null, size = PleyTv.IconSize, tint = if (focused) PleyColors.OnAccent else PleyColors.Accent)
        }
    }
}

/** A side panel's title in tvTitle, with what it is for under it. */
@Composable
internal fun TvPanelTitle(title: String, subtitle: String? = null) {
    Column(Modifier.padding(start = 16.dp, top = 8.dp, bottom = 12.dp)) {
        BasicText(title, style = PleyType.Tv.Title.copy(color = PleyColors.Fg), maxLines = 2, overflow = TextOverflow.Ellipsis)
        if (!subtitle.isNullOrEmpty()) {
            BasicText(subtitle, style = PleyType.Tv.Caption.copy(color = PleyColors.Muted), maxLines = 2, overflow = TextOverflow.Ellipsis)
        }
    }
}

/** A heading inside a side panel: "Your playlists", "For". */
@Composable
internal fun TvPanelHeading(text: String) {
    BasicText(text, style = PleyType.Tv.Label.copy(color = PleyColors.Muted), modifier = Modifier.padding(start = 16.dp, top = 16.dp, bottom = 6.dp))
}

/** Lucide's trash-2, for Delete: not among the design system's icons yet. */
internal val TvTrashIcon = PleyIcon(
    "trash-2",
    false,
    "M10 11v6",
    "M14 11v6",
    "M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6",
    "M3 6h18",
    "M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2",
)
