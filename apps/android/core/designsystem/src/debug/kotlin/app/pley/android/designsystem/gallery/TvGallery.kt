package app.pley.android.designsystem.gallery

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.layout.layout
import androidx.compose.ui.unit.dp
import app.pley.android.common.Tier
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.PleyTv
import app.pley.android.designsystem.PleyType
import app.pley.android.designsystem.image.Artwork
import app.pley.android.designsystem.image.ArtworkImage
import app.pley.android.designsystem.tile.QualityBadge
import app.pley.android.designsystem.tile.SkeletonTile
import app.pley.android.designsystem.tile.TileKind
import app.pley.android.designsystem.tile.TileStyle
import app.pley.android.designsystem.tile.rememberSkeleton

/**
 * Everything P2-DESIGN draws, on the TV's 960 by 540 canvas: the first
 * screen is the design mockup tv-components (tiles at rest, focused with
 * and without the glow, a placeholder; badges; tokens), then real rows of
 * each tile kind to move through with the D-pad, skeletons, type and icons.
 */
@Composable
internal fun TvGallery(tier: Tier) {
    val glow = tier != Tier.T0
    Column(
        Modifier
            .fillMaxSize()
            .background(PleyColors.Bg)
            .verticalScroll(rememberScrollState())
            .padding(top = 30.dp, bottom = 40.dp),
    ) {
        Label(
            "Tiles at rest · focused with glow (T1/T2) · focused on T0 · placeholder",
            PleyType.Tv.Title.copy(fontSize = PleyType.Tv.Section.fontSize),
            modifier = Modifier.padding(start = 48.dp),
        )
        Spacer(Modifier.height(30.dp))
        Row(Modifier.padding(start = 56.dp), horizontalArrangement = Arrangement.spacedBy(22.dp)) {
            GalleryTvTile(GallerySamples.quietHarbor, TileKind.Poster, glow = false, focus = false)
            GalleryTvTile(GallerySamples.glassMeridian, TileKind.Poster, glow = true, focus = true)
            GalleryTvTile(GallerySamples.lowOrbit, TileKind.Poster, glow = false, focus = true)
            GalleryTvTile(GallerySamples.continueWatching[3], TileKind.Landscape, glow = true, focus = true)
            GalleryTvTile(GallerySamples.noPoster, TileKind.Poster, glow = false, focus = false)
        }
        Spacer(Modifier.height(30.dp))
        Row(Modifier.padding(start = 48.dp), verticalAlignment = Alignment.CenterVertically) {
            Box(Modifier.size(171.dp, 44.dp).clip(PleyShape.TileTv)) {
                Artwork(ArtworkImage(listOf("gallery://still/glass-meridian")), "", Modifier.matchParentSize())
                Row(Modifier.align(Alignment.Center), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                    listOf("4K", "DV P8.1", "Atmos").forEach { QualityBadge(it, TileStyle.Tv, onArt = true) }
                }
            }
            Spacer(Modifier.width(16.dp))
            Label("on art", PleyType.Tv.Label, color = PleyColors.Muted)
            Spacer(Modifier.width(16.dp))
            Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                listOf("1080p", "HDR10+", "5.1").forEach { QualityBadge(it, TileStyle.Tv) }
            }
            Spacer(Modifier.width(16.dp))
            Label("on bg", PleyType.Tv.Label, color = PleyColors.Muted)
            Spacer(Modifier.width(16.dp))
            QualityBadge("R", TileStyle.Tv)
            Spacer(Modifier.width(8.dp))
            QualityBadge("PG-13", TileStyle.Tv)
        }
        Spacer(Modifier.height(30.dp))
        Box(Modifier.padding(start = 48.dp)) {
            Swatches(PleyType.Tv.Badge.copy(fontWeight = PleyType.Medium), 80.dp, columns = 10)
        }

        Section("Continue Watching")
        TvRow(GallerySamples.continueWatching, TileKind.Landscape, glow)
        Section("Recently Added in Movies")
        TvRow(GallerySamples.movies, TileKind.Poster, glow)
        Section("Recently Added in Shows")
        TvRow(GallerySamples.shows, TileKind.Poster, glow)
        Section("Season 2")
        TvRow(GallerySamples.episodes, TileKind.Episode, glow)
        Section("Albums")
        TvRow(GallerySamples.albums, TileKind.Square, glow)
        Section("Cast")
        TvRow(GallerySamples.people, TileKind.Person, glow)

        Section("Loading (static on TV)")
        val skeleton = rememberSkeleton(animated = false)
        Row(Modifier.padding(start = PleyTv.Gutter), horizontalArrangement = Arrangement.spacedBy(PleyTv.WideTileGap)) {
            repeat(3) { SkeletonTile(skeleton, TileKind.Landscape, TileStyle.Tv) }
        }

        Section("Type")
        Column(Modifier.padding(start = PleyTv.Gutter), verticalArrangement = Arrangement.spacedBy(10.dp)) {
            Label("Hero title 38", PleyType.Tv.HeroTitle)
            Label("Page title 28", PleyType.Tv.PageTitle)
            Label("Section 18 · Continue Watching", PleyType.Tv.Section)
            Label("Title 16 · Glasshouse", PleyType.Tv.Title)
            Label("Body large 15 · A volcanologist and her estranged brother race the ash cloud.", PleyType.Tv.BodyLarge, color = PleyColors.Fg.copy(alpha = 0.85f))
            Label("Body 14 · Your phone must be on the same Wi-Fi as this TV.", PleyType.Tv.Body, color = PleyColors.Muted)
            Label("Label 14 · 2025 · R · 2h 2m", PleyType.Tv.Label, color = PleyColors.Muted)
            Label("Caption 13 · S4 · E2 · 31 min left", PleyType.Tv.Caption, color = PleyColors.Muted)
            Label("BCDF–GHJK", PleyType.Tv.Code)
        }

        Section("Icons")
        Box(Modifier.padding(start = PleyTv.Gutter)) {
            IconGrid(24.dp, columns = 10, label = PleyType.Tv.Badge.copy(fontWeight = PleyType.Medium), cell = 80.dp)
        }
    }
}

@Composable
private fun Section(title: String) {
    Spacer(Modifier.height(PleyTv.RowGap + 8.dp))
    Label(title, PleyType.Tv.Section, modifier = Modifier.padding(start = PleyTv.Gutter))
    Spacer(Modifier.height(PleyTv.HeaderToTiles))
}

@Composable
private fun TvRow(models: List<app.pley.android.designsystem.tile.ArtworkTileModel>, kind: TileKind, glow: Boolean) {
    // A lazy row clips 15 dp beyond its height; the focused tile's growth,
    // ring and glow reach further, so the row is laid out 28 dp taller on
    // each side and pulled back over its neighbors.
    LazyRow(
        modifier = Modifier.bleed(28.dp),
        contentPadding = PaddingValues(start = PleyTv.Gutter, end = TvSafeEnd + kind.tvWidth, top = 28.dp, bottom = 28.dp),
        horizontalArrangement = Arrangement.spacedBy(kind.tvGap),
    ) {
        items(models, key = { it.id }, contentType = { kind }) { model ->
            GalleryTvTile(model, kind, glow)
        }
    }
}

private val TvSafeEnd = app.pley.android.designsystem.TvSafeArea.Horizontal

/** Lays this out [vertical] taller above and below than the room it takes, for drawing that reaches past it. */
internal fun Modifier.bleed(vertical: androidx.compose.ui.unit.Dp): Modifier = layout { measurable, constraints ->
    val extra = vertical.roundToPx()
    val placeable = measurable.measure(constraints.copy(maxHeight = if (constraints.hasBoundedHeight) constraints.maxHeight + 2 * extra else constraints.maxHeight))
    layout(placeable.width, (placeable.height - 2 * extra).coerceAtLeast(0)) { placeable.place(0, -extra) }
}
