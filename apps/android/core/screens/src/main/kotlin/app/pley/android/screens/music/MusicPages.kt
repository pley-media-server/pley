package app.pley.android.screens.music

import app.pley.android.common.AppLanguage
import app.pley.android.common.UiText
import app.pley.android.model.ItemDetail
import app.pley.android.model.ItemSummary
import app.pley.android.model.format.FormatWords
import app.pley.android.model.format.formatClockMs
import app.pley.android.model.format.formatDuration
import app.pley.android.screens.R
import java.util.Locale

/**
 * An artist's page (apps/web/src/pages/item/MusicViews.tsx, ArtistView):
 * the round picture, "4 albums · 40 tracks", Play and Shuffle, and the
 * albums, newest first.
 */
data class ArtistPage(
    val id: Long,
    val title: String,
    /** The server's address, which the pictures' paths are on. */
    val origin: String,
    val posterUrl: String?,
    val backdropUrl: String?,
    val summary: String?,
    val genres: List<String>,
    /** "4 albums", "40 tracks"; empty when the server doesn't say. */
    val countTexts: List<UiText>,
    /** Newest first, then by title; empty while they load ([albumsLoading]). */
    val albums: List<ItemSummary>,
    val albumsLoading: Boolean,
    /** The artist as a listing has it, for playing all of their tracks. */
    val summaryItem: ItemSummary,
)

/**
 * An album's page (MusicViews.tsx, AlbumView): the art, the artist, the
 * year, the tracks and their length, the profile's rating, Play and
 * Shuffle, and the tracks with disc headers when there are several discs.
 */
data class AlbumPage(
    val id: Long,
    val title: String,
    val origin: String,
    val posterUrl: String?,
    /** The album's artist, a link to their page when [artistId] is known. */
    val artist: String?,
    val artistId: Long?,
    val year: Int?,
    /** "10 tracks", "42m"; the year and the artist go apart. */
    val metaTexts: List<UiText>,
    /** The profile's rating, 0 to 10. */
    val userRating: Int?,
    val summary: String?,
    val genres: List<String>,
    /** The tracks in order, which Play plays. */
    val tracks: List<ItemSummary>,
    /** What the list shows: disc headers, then their tracks. */
    val rows: List<AlbumRow>,
    val tracksLoading: Boolean,
)

/** One line of an album's track list. */
sealed interface AlbumRow {
    val key: String

    /** "Disc 2", above its tracks, on albums of several discs. */
    data class Disc(val number: Int) : AlbumRow {
        override val key: String get() = "disc-$number"
    }

    /**
     * A track: its [number] on the disc, its [title], the track's own
     * [artist] when it isn't the album's, its [duration] ("3:12"), and
     * [position] in the album's tracks (what plays from it).
     */
    data class Track(
        val id: Long,
        val position: Int,
        val number: String,
        val title: String,
        val artist: String?,
        val duration: String,
    ) : AlbumRow {
        override val key: String get() = "track-$id"
    }
}

/** The artist page of [detail] with [albums] (`null` while they load). */
fun artistPage(detail: ItemDetail, albums: List<ItemSummary>?, origin: String, locale: Locale = AppLanguage.locale): ArtistPage {
    val sorted = albums.orEmpty().sortedWith(compareByDescending<ItemSummary> { it.year ?: 0 }.thenBy { it.sortTitle.ifEmpty { it.title }.lowercase(locale) })
    val albumCount = detail.childCount ?: albums?.size
    val countTexts = listOfNotNull(
        albumCount?.let { UiText.plural(R.plurals.music_count_albums, it) },
        detail.leafCount?.let { UiText.plural(R.plurals.music_count_tracks, it) },
    )
    return ArtistPage(
        id = detail.id,
        title = detail.title,
        origin = origin,
        posterUrl = detail.posterUrl?.takeIf { it.isNotEmpty() },
        backdropUrl = detail.backdropUrl?.takeIf { it.isNotEmpty() },
        summary = detail.summary?.takeIf { it.isNotBlank() },
        genres = detail.genres,
        countTexts = countTexts,
        albums = sorted,
        albumsLoading = albums == null,
        summaryItem = detail.toSummary(),
    )
}

/** The album page of [detail] with [tracks] (`null` while they load), its length in [words]. */
fun albumPage(detail: ItemDetail, tracks: List<ItemSummary>?, origin: String, words: FormatWords = AppLanguage.formatWords()): AlbumPage {
    val list = tracks.orEmpty()
    val total = list.sumOf { it.durationMs ?: 0 }
    val length = formatDuration(total, words).takeIf { it.isNotEmpty() }
    val metaTexts = listOfNotNull(
        list.size.takeIf { it > 0 }?.let { UiText.plural(R.plurals.music_count_tracks, it) },
        length?.let(UiText::raw),
    )
    return AlbumPage(
        id = detail.id,
        title = detail.title,
        origin = origin,
        posterUrl = detail.posterUrl?.takeIf { it.isNotEmpty() },
        artist = detail.parentTitle,
        artistId = detail.parentId,
        year = detail.year,
        metaTexts = metaTexts,
        userRating = detail.userState.rating?.let { Math.round(it).toInt() }?.takeIf { it > 0 },
        summary = detail.summary?.takeIf { it.isNotBlank() },
        genres = detail.genres,
        tracks = list,
        rows = albumRows(list, detail.parentTitle),
        tracksLoading = tracks == null,
    )
}

/** Disc headers (when there are several discs) and the tracks, as the web lists them. */
fun albumRows(tracks: List<ItemSummary>, albumArtist: String?): List<AlbumRow> {
    val discs = tracks.map { it.parentIndex ?: 1 }.toSet()
    val several = discs.size > 1
    return buildList {
        var disc: Int? = null
        tracks.forEachIndexed { i, t ->
            val d = t.parentIndex ?: 1
            if (several && d != disc) add(AlbumRow.Disc(d))
            disc = d
            add(
                AlbumRow.Track(
                    id = t.id,
                    position = i,
                    number = t.index?.toString().orEmpty(),
                    title = t.title,
                    artist = t.grandparentTitle?.takeIf { it.isNotBlank() && it != albumArtist },
                    duration = t.durationMs?.takeIf { it > 0 }?.let(::formatClockMs).orEmpty(),
                ),
            )
        }
    }
}
