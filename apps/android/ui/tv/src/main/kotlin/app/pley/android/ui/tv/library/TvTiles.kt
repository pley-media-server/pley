package app.pley.android.ui.tv.library

import android.content.res.Resources
import androidx.annotation.PluralsRes
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import app.pley.android.designsystem.PleyTv
import app.pley.android.designsystem.TvSafeArea
import app.pley.android.designsystem.image.ArtworkImage
import app.pley.android.designsystem.tile.ArtworkTileModel
import app.pley.android.designsystem.tile.TileKind
import app.pley.android.model.CollectionSummary
import app.pley.android.model.ItemSummary
import app.pley.android.model.LibraryKind
import app.pley.android.model.LibrarySort
import app.pley.android.model.format.mediaBadges
import app.pley.android.model.format.mediaLine
import app.pley.android.screens.LibraryTab
import app.pley.android.screens.library.TileFactory
import app.pley.android.screens.library.TileLook
import app.pley.android.ui.tv.R
import java.text.NumberFormat
import kotlin.math.floor
import kotlin.math.max

/**
 * The TV's tiles for library pages, made by the screens' ViewModels off
 * the main thread: the design system's models, the collection counts in
 * this app's words.
 */
internal class TvTiles(private val resources: Resources) : TileFactory<ArtworkTileModel> {
    override fun item(item: ItemSummary, look: TileLook, origin: String): ArtworkTileModel = when (look) {
        TileLook.Landscape -> ArtworkTileModel.of(item, TileKind.Landscape, origin)
        TileLook.Square -> ArtworkTileModel.of(item, TileKind.Square, origin)
        TileLook.Poster -> ArtworkTileModel.of(item, TileKind.LibraryGrid, origin).copy(badges = mediaBadges(item.media))
        TileLook.Bitrate -> ArtworkTileModel.of(item, TileKind.LibraryGrid, origin).let { model ->
            val line = mediaLine(item.media)
            model.copy(
                subtitle = line.ifEmpty { model.subtitle },
                badges = mediaBadges(item.media),
                description = listOf(model.description, line).filter { it.isNotEmpty() }.joinToString(", "),
            )
        }
    }

    override fun collection(collection: CollectionSummary, kind: LibraryKind?, origin: String): ArtworkTileModel {
        val count = tvCount(resources, if (kind == LibraryKind.Shows) R.plurals.tv_library_count_shows else R.plurals.tv_library_count_movies, collection.itemCount)
        return ArtworkTileModel(
            id = collection.id,
            title = collection.title,
            subtitle = count,
            image = ArtworkImage.of(collection.posterUrl, origin),
            description = "${collection.title}, $count",
        )
    }
}

/** "1,500 movies": [n] in this locale's digits, with its plural. */
internal fun tvCount(resources: Resources, @PluralsRes plural: Int, n: Int): String =
    resources.getQuantityString(plural, n, NumberFormat.getIntegerInstance().format(n))

/** "23 of 1,500 movies": [n] of [of] in this locale's digits, with the plural of [of]. */
internal fun tvCountOf(resources: Resources, @PluralsRes plural: Int, n: Int, of: Int): String {
    val digits = NumberFormat.getIntegerInstance()
    return resources.getQuantityString(plural, of, digits.format(n), digits.format(of))
}

/** The noun of a filtered listing of a library of [kind]: "23 of 1,500 movies". */
@PluralsRes
internal fun tvItemsOfPlural(kind: LibraryKind?): Int = when (kind) {
    LibraryKind.Shows -> R.plurals.tv_library_count_of_shows
    LibraryKind.Music -> R.plurals.tv_library_count_of_artists
    else -> R.plurals.tv_library_count_of_movies
}

@PluralsRes
internal fun tvItemsPlural(kind: LibraryKind?): Int = when (kind) {
    LibraryKind.Shows -> R.plurals.tv_library_count_shows
    LibraryKind.Music -> R.plurals.tv_library_count_artists
    else -> R.plurals.tv_library_count_movies
}

internal fun tvSortLabel(sort: LibrarySort): Int = when (sort) {
    LibrarySort.Title -> R.string.tv_library_sort_title
    LibrarySort.Year -> R.string.tv_library_sort_year
    LibrarySort.Added -> R.string.tv_library_sort_added
    LibrarySort.Rating -> R.string.tv_library_sort_rating
    LibrarySort.UserRating -> R.string.tv_library_sort_user_rating
    LibrarySort.Release -> R.string.tv_library_sort_release
    LibrarySort.Duration -> R.string.tv_library_sort_duration
    LibrarySort.LastPlayed -> R.string.tv_library_sort_last_played
    LibrarySort.Bitrate -> R.string.tv_library_sort_bitrate
}

internal fun tvTabLabel(tab: LibraryTab): Int = when (tab) {
    LibraryTab.Recommended -> R.string.tv_library_tab_recommended
    LibraryTab.Library -> R.string.tv_library_tab_library
    LibraryTab.Collections -> R.string.tv_library_tab_collections
}

/**
 * The TV grid's columns (design spec §4.2): as many wells of [kind] as fit
 * between the gutter (x = 96) and [end], 14 dp apart: seven posters of 96,
 * from 96 to 858, which leaves the jump control its room at 872 to 912.
 */
internal fun tvGridColumns(kind: TileKind, end: Dp = GridEnd): Int {
    val width = end - PleyTv.Gutter
    return max(1, floor((width + PleyTv.TileGap) / (kind.tvWidth + PleyTv.TileGap)).toInt())
}

/** Where the grid's last column may end: left of the jump control, 16 dp clear of it. */
internal val GridEnd: Dp get() = PleyTv.CanvasWidth - TvSafeArea.Horizontal - JumpSize - PleyTv.TileGap

/** The jump control's circle. */
internal val JumpSize: Dp = 40.dp
