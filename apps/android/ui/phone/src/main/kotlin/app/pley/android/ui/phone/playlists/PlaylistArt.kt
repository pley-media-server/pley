package app.pley.android.ui.phone.playlists

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawWithCache
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.icon.Icon
import app.pley.android.designsystem.icon.PleyIcon
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.designsystem.image.Artwork
import app.pley.android.designsystem.image.ArtworkImage
import app.pley.android.designsystem.image.MarkGlyph

/**
 * A playlist's square cover (the web's PlaylistArt): a 2 by 2 mosaic of
 * its first four entries, or its first entry's picture, or, empty, its
 * kind's glyph on a [surface3][PleyColors.Surface3] to
 * [surface2][PleyColors.Surface2] gradient. A 1 dp hairline of white at
 * 6 % keeps dark art from melting into the page.
 */
@Composable
internal fun PlaylistArt(
    artwork: List<String>,
    origin: String,
    video: Boolean,
    title: String,
    modifier: Modifier = Modifier,
    shape: Shape = PleyShape.TilePhone,
    /** The empty playlist's glyph: about a third of the cover. */
    glyph: Dp = 40.dp,
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
                    drawRect(PleyColors.TileHairline, style = androidx.compose.ui.graphics.drawscope.Stroke(hair))
                }
            },
    ) {
        when {
            images.size >= 4 -> Column(Modifier.fillMaxSize()) {
                for (r in 0 until 2) {
                    Row(Modifier.fillMaxWidth().weight(1f)) {
                        for (c in 0 until 2) {
                            Artwork(images[r * 2 + c], "", Modifier.weight(1f).fillMaxHeight())
                        }
                    }
                }
            }
            // Should the picture not come, Pley's mark: music's glyph for tracks, play for movies and episodes alike.
            images.isNotEmpty() -> Artwork(images.first(), title, Modifier.fillMaxSize(), mark = if (video) MarkGlyph.Play else MarkGlyph.Music)
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

/** Lucide's trash-2, for Delete: not among the design system's icons yet. */
internal val TrashIcon = PleyIcon(
    "trash-2",
    false,
    "M10 11v6",
    "M14 11v6",
    "M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6",
    "M3 6h18",
    "M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2",
)
