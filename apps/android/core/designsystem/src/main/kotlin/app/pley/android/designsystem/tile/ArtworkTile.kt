package app.pley.android.designsystem.tile

import androidx.annotation.VisibleForTesting
import androidx.compose.runtime.Composable
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.image.ArtworkBox
import app.pley.android.designsystem.image.ArtworkLoad
import app.pley.android.designsystem.image.artworkPlaceholder

/**
 * A tile's art (design spec §2.3): the picture in its [surface2][PleyColors.Surface2]
 * well, or the title's gradient while there is none (and while the picture
 * fades in), and the overlays: the 4 dp progress bar,
 * the watched tick or the unwatched count, quality badges and the hairline
 * that keeps dark art from dissolving into the page.
 *
 * It has no focus or press of its own: the TV's TvTile wraps it with the
 * focus scale, ring, glow ([tileGlow]) and caption weight, the phone's card
 * with its press. Its size comes from [modifier] ([tvTileSize] on TV).
 *
 * For screen readers it is one node saying [ArtworkTileModel.description];
 * a clickable or focusable parent merges it into its own.
 */
@Composable
fun ArtworkTile(
    model: ArtworkTileModel,
    kind: TileKind,
    style: TileStyle,
    modifier: Modifier = Modifier,
) {
    TileProbe.listener?.let { probe -> SideEffect { probe(model.id) } }
    val load = remember(model.image) { ArtworkLoad() }
    val shape = style.shape(kind)
    ArtworkBox(
        image = model.image,
        load = load,
        modifier = modifier
            .clearAndSetSemantics { contentDescription = model.description }
            .artworkPlaceholder(model.title, if (kind.round) null else style.placeholderText, load, well = PleyColors.Surface2, shape = shape)
            .tileOverlays(model, kind, style, load),
    )
}

/** Counts tile compositions, for the test that tiles skip when an unrelated item changes. */
@VisibleForTesting
internal object TileProbe {
    @Volatile
    var listener: ((Long) -> Unit)? = null
}
