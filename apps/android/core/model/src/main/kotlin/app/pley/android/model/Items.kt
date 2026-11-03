package app.pley.android.model

import kotlinx.serialization.Serializable

/** What an item is (`kind` in apps/server/src/catalog.rs). */
@Serializable(with = ItemKind.Serializer::class)
enum class ItemKind(override val wire: String) : WireEnum {
    Movie("movie"),
    Show("show"),
    Season("season"),
    Episode("episode"),
    Artist("artist"),
    Album("album"),
    Track("track"),

    /** Bonus material of a movie or a show (trailers first): [ItemSummary.extraType] says which. */
    Extra("extra"),
    Unknown("unknown"),
    ;

    /** Kinds that play themselves; the others hold items. */
    val playable: Boolean get() = this == Movie || this == Episode || this == Track || this == Extra

    /** Kinds My List takes: movies and shows (GET /api/watchlist). */
    val listable: Boolean get() = this == Movie || this == Show

    object Serializer : LenientEnumSerializer<ItemKind>("ItemKind", ItemKind.entries, ItemKind.Unknown)
}

/** Which bonus material an [ItemKind.Extra] is (apps/server/src/library/walk.rs, `ExtraKind`). */
@Serializable(with = ExtraType.Serializer::class)
enum class ExtraType(override val wire: String) : WireEnum {
    Trailer("trailer"),
    Featurette("featurette"),
    BehindTheScenes("behind_the_scenes"),
    DeletedScene("deleted_scene"),
    Interview("interview"),
    Scene("scene"),
    Short("short"),
    Other("other"),
    Unknown("unknown"),
    ;

    object Serializer : LenientEnumSerializer<ExtraType>("ExtraType", ExtraType.entries, ExtraType.Unknown)
}

/** A video's dynamic range (packages/transcoder-api, `DynamicRange`). */
@Serializable(with = DynamicRange.Serializer::class)
enum class DynamicRange(override val wire: String) : WireEnum {
    Sdr("sdr"),
    Hdr10("hdr10"),
    Hdr10Plus("hdr10plus"),
    Hlg("hlg"),
    DolbyVision("dolby_vision"),
    Unknown("unknown"),
    ;

    object Serializer : LenientEnumSerializer<DynamicRange>("DynamicRange", DynamicRange.entries, DynamicRange.Unknown)
}

/** The pictures an item has, for GET /api/items/{id}/images/{kind}. Collections have the first two. */
enum class ImageKind(override val wire: String) : WireEnum {
    Poster("poster"),
    Backdrop("backdrop"),

    /** Episode stills and video frame grabs, 16:9. */
    Thumb("thumb"),
}

/** The profile's own state of an item. */
@Serializable
data class UserState(
    /** Where to resume, in ms; 0 when there's nothing to resume. */
    val positionMs: Long = 0,
    /** Shows and seasons: every episode is watched. */
    val watched: Boolean = false,
    val playCount: Int = 0,
    /** Unix seconds. */
    val lastPlayedAt: Long? = null,
    /** The profile's own rating, 0 to 10 (two points per star). */
    val rating: Double? = null,
)

/**
 * What a title's files are like, in library listings: a movie's best
 * file, a show's episodes together (apps/server/src/catalog.rs,
 * `MediaSummary`).
 */
@Serializable
data class MediaSummary(
    /** Bits per second (a show: its episodes' average). */
    val bitrate: Long? = null,
    /** Bytes (a show: all its episodes). */
    val size: Long? = null,
    val width: Int? = null,
    /** A show: its largest episode's. */
    val height: Int? = null,
    /** `null` for shows. */
    val videoCodec: String? = null,
    /** A show: its best episode's (an older server: `null`). */
    val dynamicRange: DynamicRange? = null,
)

/**
 * An item in a listing (apps/server/src/catalog.rs, `ItemSummary`). The
 * server leaves optional fields out when they have no value, to keep big
 * listings small: they read as `null`.
 *
 * Durations are ms, times unix seconds. Picture addresses are the server's
 * own ("/api/items/12/images/poster?v=1729600000"): add `&w=<px>` for a
 * smaller one.
 */
@Serializable
data class ItemSummary(
    val id: Long,
    val libraryId: Long,
    val kind: ItemKind,
    val title: String,
    val sortTitle: String = "",
    val year: Int? = null,
    /** Season number, episode number or track number. */
    val index: Int? = null,
    /** The season of an episode, the show of a season, the album of a track, the artist of an album, the movie or show of an extra. */
    val parentId: Long? = null,
    val parentTitle: String? = null,
    /** An episode's season number; a track's disc number. */
    val parentIndex: Int? = null,
    /** The show of an episode, the artist of a track. */
    val grandparentId: Long? = null,
    val grandparentTitle: String? = null,
    val durationMs: Long? = null,
    /** The critics' rating, 0 to 10. */
    val rating: Double? = null,
    val addedAt: Long = 0,
    val posterUrl: String? = null,
    val backdropUrl: String? = null,
    /** Episode stills and video frame grabs (16:9). */
    val thumbUrl: String? = null,
    val userState: UserState = UserState(),
    /** Shows and seasons: episodes; artists and albums: tracks. */
    val leafCount: Int? = null,
    /** Shows and seasons: watched episodes. */
    val viewedLeafCount: Int? = null,
    /** Shows: seasons; artists: albums. */
    val childCount: Int? = null,
    /** In listings only for episodes; always in [ItemDetail]. */
    val summary: String? = null,
    /** "YYYY-MM-DD". */
    val releaseDate: String? = null,
    /** Kind [ItemKind.Extra] only. */
    val extraType: ExtraType? = null,
    /** Only in library listings (an older server's: only sorted by bitrate). */
    val media: MediaSummary? = null,
)

/**
 * GET /api/items/{id}: an item with everything the server knows about it
 * (apps/server/src/catalog.rs, `ItemDetail`, which is an [ItemSummary]
 * with more fields beside it in the same object). [toSummary] gives the
 * summary back.
 */
@Serializable
data class ItemDetail(
    val id: Long,
    val libraryId: Long,
    val kind: ItemKind,
    val title: String,
    val sortTitle: String = "",
    val year: Int? = null,
    val index: Int? = null,
    val parentId: Long? = null,
    val parentTitle: String? = null,
    val parentIndex: Int? = null,
    val grandparentId: Long? = null,
    val grandparentTitle: String? = null,
    val durationMs: Long? = null,
    val rating: Double? = null,
    val addedAt: Long = 0,
    val posterUrl: String? = null,
    val backdropUrl: String? = null,
    val thumbUrl: String? = null,
    val userState: UserState = UserState(),
    val leafCount: Int? = null,
    val viewedLeafCount: Int? = null,
    val childCount: Int? = null,
    val summary: String? = null,
    val releaseDate: String? = null,
    val extraType: ExtraType? = null,
    val media: MediaSummary? = null,
    // What a listing leaves out.
    val originalTitle: String? = null,
    val tagline: String? = null,
    val genres: List<String> = emptyList(),
    val studio: String? = null,
    /** "PG-13", "15", "FSK 12"… */
    val contentRating: String? = null,
    val tmdbId: Long? = null,
    val imdbId: String? = null,
    /** Empty for shows, seasons, artists and albums. Versions of a movie are several files. */
    val files: List<MediaFile> = emptyList(),
    /** Intro and credits of the files, for Skip Intro and Skip Credits. */
    val markers: List<Marker> = emptyList(),
    /** Fields an administrator edited by hand, which refreshes keep ("title", "summary"…). */
    val lockedFields: List<String> = emptyList(),
    /** Movies and shows: trailers and other bonus material, trailers first. */
    val extras: List<ItemSummary> = emptyList(),
    /** Actors in billing order; episodes: the guest stars; seasons: the show's. */
    val cast: List<Credit> = emptyList(),
    /** Creators, directors, writers and producers. */
    val crew: List<Credit> = emptyList(),
    /** In the profile's My List (movies and shows). */
    val inWatchlist: Boolean = false,
) {
    fun toSummary(): ItemSummary = ItemSummary(
        id = id,
        libraryId = libraryId,
        kind = kind,
        title = title,
        sortTitle = sortTitle,
        year = year,
        index = index,
        parentId = parentId,
        parentTitle = parentTitle,
        parentIndex = parentIndex,
        grandparentId = grandparentId,
        grandparentTitle = grandparentTitle,
        durationMs = durationMs,
        rating = rating,
        addedAt = addedAt,
        posterUrl = posterUrl,
        backdropUrl = backdropUrl,
        thumbUrl = thumbUrl,
        userState = userState,
        leafCount = leafCount,
        viewedLeafCount = viewedLeafCount,
        childCount = childCount,
        summary = summary,
        releaseDate = releaseDate,
        extraType = extraType,
        media = media,
    )
}

/**
 * `{ items }`: GET /api/items/{id}/children (seasons of a show, episodes of
 * a season, albums of an artist, tracks of an album, by index) and
 * GET /api/items/{id}/related ("More Like This", best first; empty for
 * kinds other than movies and shows).
 */
@Serializable
data class ItemList(
    val items: List<ItemSummary> = emptyList(),
)

/**
 * `{ item }`: GET /api/items/{id}/on-deck (the episode Play starts: in
 * progress, else the next unwatched, else the first) and
 * GET /api/items/{id}/next (the episode or track after this one). `null`
 * when there's none.
 */
@Serializable
data class MaybeItem(
    val item: ItemSummary? = null,
)
