package app.pley.android.ui.phone.library

import android.content.res.Resources
import androidx.annotation.PluralsRes
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import app.pley.android.data.GridDensity
import app.pley.android.designsystem.PleyPhone
import app.pley.android.designsystem.image.ArtworkImage
import app.pley.android.designsystem.tile.ArtworkTileModel
import app.pley.android.designsystem.tile.TileKind
import app.pley.android.model.CollectionSummary
import app.pley.android.model.ItemSummary
import app.pley.android.model.LibraryKind
import app.pley.android.model.LibrarySort
import app.pley.android.model.format.mediaBadges
import app.pley.android.model.format.mediaLine
import app.pley.android.screens.library.TileFactory
import app.pley.android.screens.library.TileLook
import app.pley.android.ui.phone.R
import java.text.NumberFormat
import kotlin.math.floor
import kotlin.math.max

/**
 * The phone's tiles for library pages, made by the screens' ViewModels off
 * the main thread: the design system's models, the collection counts in
 * this app's words.
 */
internal class PhoneTiles(private val resources: Resources) : TileFactory<ArtworkTileModel> {
    override fun item(item: ItemSummary, look: TileLook, origin: String): ArtworkTileModel = when (look) {
        TileLook.Landscape -> ArtworkTileModel.of(item, TileKind.Landscape, origin)
        TileLook.Square -> ArtworkTileModel.of(item, TileKind.Square, origin)
        TileLook.Poster -> ArtworkTileModel.of(item, TileKind.Poster, origin)
        TileLook.Badged -> ArtworkTileModel.of(item, TileKind.Poster, origin).copy(badges = mediaBadges(item.media))
        TileLook.Bitrate -> ArtworkTileModel.of(item, TileKind.Poster, origin).let { model ->
            val line = mediaLine(item.media)
            model.copy(
                subtitle = line.ifEmpty { model.subtitle },
                badges = mediaBadges(item.media),
                description = listOf(model.description, line).filter { it.isNotEmpty() }.joinToString(", "),
            )
        }
    }

    override fun collection(collection: CollectionSummary, kind: LibraryKind?, origin: String): ArtworkTileModel {
        val count = count(resources, if (kind == LibraryKind.Shows) R.plurals.library_count_shows else R.plurals.library_count_movies, collection.itemCount)
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
internal fun count(resources: Resources, @PluralsRes plural: Int, n: Int): String =
    resources.getQuantityString(plural, n, NumberFormat.getIntegerInstance().format(n))

/** "23 of 1,500 movies": [n] of [of] in this locale's digits, with the plural of [of]. */
internal fun countOf(resources: Resources, @PluralsRes plural: Int, n: Int, of: Int): String {
    val digits = NumberFormat.getIntegerInstance()
    return resources.getQuantityString(plural, of, digits.format(n), digits.format(of))
}

/** The noun of a filtered listing of a library of [kind]: "23 of 1,500 movies". */
@PluralsRes
internal fun itemsOfPlural(kind: LibraryKind?): Int = when (kind) {
    LibraryKind.Shows -> R.plurals.library_count_of_shows
    LibraryKind.Music -> R.plurals.library_count_of_artists
    else -> R.plurals.library_count_of_movies
}

/** The listing's noun for a library of [kind]: "movies", "shows", "artists". */
@PluralsRes
internal fun itemsPlural(kind: LibraryKind?): Int = when (kind) {
    LibraryKind.Shows -> R.plurals.library_count_shows
    LibraryKind.Music -> R.plurals.library_count_artists
    else -> R.plurals.library_count_movies
}

/** A sort's name, as the web's select shows it. */
internal fun sortLabel(sort: LibrarySort): Int = when (sort) {
    LibrarySort.Title -> R.string.library_sort_title
    LibrarySort.Year -> R.string.library_sort_year
    LibrarySort.Added -> R.string.library_sort_added
    LibrarySort.Rating -> R.string.library_sort_rating
    LibrarySort.UserRating -> R.string.library_sort_user_rating
    LibrarySort.Release -> R.string.library_sort_release
    LibrarySort.Duration -> R.string.library_sort_duration
    LibrarySort.LastPlayed -> R.string.library_sort_last_played
    LibrarySort.Bitrate -> R.string.library_sort_bitrate
}

/**
 * The narrowest a grid's card gets at each [GridDensity]: Small fits four
 * across a 412 dp phone, Medium (the design's 118 dp poster) three, Large
 * two.
 */
internal fun densityMinWidth(density: GridDensity, kind: TileKind): Dp {
    val medium = kind.phoneMinWidth
    return when (density) {
        GridDensity.Small -> medium * 0.8f
        GridDensity.Medium -> medium
        GridDensity.Large -> medium * 1.3f
    }
}

/**
 * The columns of a library grid [available] wide at [density] (design
 * spec §3.2): as many cards of the density's minimum width as fit, each
 * giving up at most 15 % of it, never fewer than 2.
 */
internal fun libraryColumns(density: GridDensity, kind: TileKind, available: Dp, gap: Dp = PleyPhone.TileGap): Int =
    max(2, floor((available + gap) / (densityMinWidth(density, kind) * (1f - SLACK) + gap)).toInt())

/** The width of each of [columns] cards across [available]. */
internal fun cellWidth(columns: Int, available: Dp, gap: Dp = PleyPhone.TileGap): Dp =
    ((available - gap * (columns - 1)) / columns).coerceAtLeast(1.dp)

private const val SLACK = 0.15f

/**
 * [columns] cells of one width, so every card's art is the same height
 * (fixed cells hand the odd pixels to the first columns, which makes their
 * posters a pixel taller); the pixels left over go to the end margin.
 */
internal class EqualCells(private val columns: Int) : GridCells {
    override fun Density.calculateCrossAxisCellSizes(availableSize: Int, spacing: Int): List<Int> {
        val each = ((availableSize - spacing * (columns - 1)) / columns).coerceAtLeast(1)
        return List(columns) { each }
    }

    override fun equals(other: Any?): Boolean = other is EqualCells && other.columns == columns

    override fun hashCode(): Int = columns
}
