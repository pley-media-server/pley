package app.pley.android.designsystem.gallery

import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.systemBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.rememberVectorPainter
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyPhone
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.PleyType
import app.pley.android.designsystem.icon.Icon
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.designsystem.icon.PleyMark
import app.pley.android.designsystem.tile.ArtworkTile
import app.pley.android.designsystem.tile.ArtworkTileModel
import app.pley.android.designsystem.tile.QualityBadge
import app.pley.android.designsystem.tile.SkeletonTile
import app.pley.android.designsystem.tile.TileCaption
import app.pley.android.designsystem.tile.TileKind
import app.pley.android.designsystem.tile.TileStyle
import app.pley.android.designsystem.tile.rememberSkeleton

/**
 * Everything P2-DESIGN draws, on a phone: the first screen follows the
 * design mockup phone-home (the bar, Continue Watching as 16:9 stills,
 * posters), then shows, the episode list's thumbnails, albums, people,
 * the shared shimmer, badges, tokens, type and icons.
 */
@Composable
internal fun PhoneGallery() {
    Column(
        Modifier
            .fillMaxSize()
            .background(PleyColors.Bg)
            .systemBarsPadding()
            .verticalScroll(rememberScrollState())
            .padding(bottom = 32.dp),
    ) {
        Row(
            Modifier.fillMaxWidth().height(64.dp).padding(horizontal = PleyPhone.MarginCompact),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Image(rememberVectorPainter(PleyMark.Tile), contentDescription = null, modifier = Modifier.size(24.dp))
            Spacer(Modifier.width(10.dp))
            Label("Pley", PleyType.Phone.HeadlineSmall)
            Spacer(Modifier.weight(1f))
            Box(Modifier.size(32.dp).background(Color(0xFFE0735A), PleyShape.Circle), contentAlignment = Alignment.Center) {
                Label("A", PleyType.Phone.TitleSmall.copy(fontWeight = PleyType.Bold), color = Color.White)
            }
        }
        PhoneSection("Continue Watching")
        PhoneRow(GallerySamples.continueWatching, TileKind.Landscape, 236.dp)
        PhoneSection("Recently Added in Movies")
        PhoneRow(GallerySamples.movies, TileKind.Poster, 118.dp)
        PhoneSection("Recently Added in Shows")
        PhoneRow(GallerySamples.shows, TileKind.Poster, 118.dp)

        PhoneSection("Season 2 · 8 episodes")
        Column(Modifier.padding(horizontal = PleyPhone.MarginCompact), verticalArrangement = Arrangement.spacedBy(16.dp)) {
            GallerySamples.episodes.take(4).forEach { episode ->
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Box {
                        ArtworkTile(episode, TileKind.Episode, TileStyle.Phone, Modifier.size(128.dp, 72.dp))
                        Box(
                            Modifier.align(Alignment.Center).size(32.dp).background(PleyColors.OnArt, PleyShape.Circle),
                            contentAlignment = Alignment.Center,
                        ) { Icon(PleyIcons.Play, null, size = 16.dp) }
                    }
                    Spacer(Modifier.width(12.dp))
                    Column {
                        Label(episode.title, PleyType.Phone.TitleMedium)
                        Label("${episode.subtitle} · Mar 12, 2024", PleyType.Phone.BodySmall, color = PleyColors.Muted)
                    }
                }
            }
        }

        PhoneSection("Albums")
        PhoneRow(GallerySamples.albums, TileKind.Square, 132.dp)
        PhoneSection("Cast")
        PhoneRow(GallerySamples.people, TileKind.Person, 88.dp)

        PhoneSection("Loading (one shimmer per screen)")
        val skeleton = rememberSkeleton(animated = true)
        Row(Modifier.padding(horizontal = PleyPhone.MarginCompact), horizontalArrangement = Arrangement.spacedBy(PleyPhone.TileGap)) {
            repeat(3) { SkeletonTile(skeleton, TileKind.Poster, TileStyle.Phone, width = 118.dp) }
        }

        PhoneSection("Badges")
        Row(Modifier.padding(horizontal = PleyPhone.MarginCompact), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            listOf("4K", "DV P8.1", "Atmos", "PG-13").forEach { QualityBadge(it, TileStyle.Phone) }
        }

        PhoneSection("Colours")
        Box(Modifier.padding(horizontal = PleyPhone.MarginCompact)) {
            Swatches(PleyType.Phone.LabelSmall.copy(fontWeight = PleyType.Medium), 80.dp, columns = 4)
        }

        PhoneSection("Themes")
        Box(Modifier.padding(horizontal = PleyPhone.MarginCompact)) {
            ThemesGrid(PleyType.Phone.LabelMedium, 160.dp, columns = 2)
        }

        PhoneSection("Type")
        Column(Modifier.padding(horizontal = PleyPhone.MarginCompact), verticalArrangement = Arrangement.spacedBy(6.dp)) {
            Label("Display small 32", PleyType.Phone.DisplaySmall)
            Label("Headline medium 26", PleyType.Phone.HeadlineMedium)
            Label("Headline small 22", PleyType.Phone.HeadlineSmall)
            Label("Title large 19", PleyType.Phone.TitleLarge)
            Label("Title medium 16 · 5. Glasshouse", PleyType.Phone.TitleMedium)
            Label("Title small 14 · Low Orbit", PleyType.Phone.TitleSmall)
            Label("Body large 16 · A volcanologist races the ash cloud.", PleyType.Phone.BodyLarge)
            Label("Body medium 14 · Choose your server.", PleyType.Phone.BodyMedium, color = PleyColors.Muted)
            Label("Body small 12 · S2 · E5 · 26 min left", PleyType.Phone.BodySmall, color = PleyColors.Muted)
            Label("Label large 15 · Resume", PleyType.Phone.LabelLarge)
            Label("Label medium 13 · 2025 · R · 2h 2m", PleyType.Phone.LabelMedium, color = PleyColors.Muted)
        }

        PhoneSection("Icons")
        Box(Modifier.padding(horizontal = PleyPhone.MarginCompact)) {
            IconGrid(24.dp, columns = 5, label = PleyType.Phone.LabelSmall.copy(fontWeight = PleyType.Medium), cell = 76.dp)
        }
    }
}

@Composable
private fun PhoneSection(title: String) {
    Spacer(Modifier.height(PleyPhone.SectionGap - 8.dp))
    Label(title, PleyType.Phone.TitleLarge, modifier = Modifier.padding(horizontal = PleyPhone.MarginCompact))
    Spacer(Modifier.height(PleyPhone.HeaderToContent + 4.dp))
}

@Composable
private fun PhoneRow(models: List<ArtworkTileModel>, kind: TileKind, width: Dp) {
    LazyRow(
        contentPadding = PaddingValues(horizontal = PleyPhone.MarginCompact),
        horizontalArrangement = Arrangement.spacedBy(PleyPhone.TileGap),
    ) {
        items(models, key = { it.id }, contentType = { kind }) { model ->
            Column(Modifier.width(width)) {
                ArtworkTile(model, kind, TileStyle.Phone, Modifier.size(width, width / kind.aspectRatio))
                Spacer(Modifier.height(TileStyle.Phone.captionGap))
                TileCaption(model.title, model.subtitle, TileStyle.Phone)
            }
        }
    }
}
