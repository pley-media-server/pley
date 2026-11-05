package app.pley.android.model

import kotlinx.serialization.Serializable

/** What a library holds (apps/server/src/library/mod.rs, `LibraryKind`). */
@Serializable(with = LibraryKind.Serializer::class)
enum class LibraryKind(override val wire: String) : WireEnum {
    Movies("movies"),
    Shows("shows"),
    Music("music"),
    Unknown("unknown"),
    ;

    object Serializer : LenientEnumSerializer<LibraryKind>("LibraryKind", LibraryKind.entries, LibraryKind.Unknown)
}

/**
 * A library: GET /api/libraries (in the profile's own order, else the one
 * administrators chose) and GET /api/libraries/{id}
 * (apps/server/src/api/libraries.rs, `LibraryDto`). Times are unix seconds.
 */
@Serializable
data class Library(
    val id: Long,
    val name: String,
    val kind: LibraryKind,
    /** Where the server keeps the media: always empty for an app's token. */
    val paths: List<String> = emptyList(),
    /** Top-level items: movies, shows or artists. */
    val itemCount: Int = 0,
    val createdAt: Long = 0,
    val scannedAt: Long? = null,
    /** A scan is queued or running right now. */
    val scanning: Boolean = false,
)

/** `{ ids }`: the body of PUT /api/me/libraries/order, every library the profile may open once, in its order. */
@Serializable
data class LibraryIds(
    val ids: List<Long>,
)

/**
 * A page of GET /api/libraries/{id}/items: top-level items only. [total]
 * counts every match, not just this page. Items leave out `backdrop_url`
 * and `thumb_url` (grids show posters), and carry [ItemSummary.media]
 * (an older server's only sorted by [LibrarySort.Bitrate]).
 */
@Serializable
data class ItemsPage(
    val total: Int = 0,
    val items: List<ItemSummary> = emptyList(),
)

/**
 * GET /api/libraries/{id}/filters: what a library's listing can be
 * filtered by. [resolutions] and [dynamicRanges] are the tokens of
 * [ResolutionFilter] and [DynamicRangeFilter] the titles have, best first,
 * and only when they have two or more (one would keep every title); an
 * older server, and music, has none.
 */
@Serializable
data class LibraryFilters(
    val genres: List<String> = emptyList(),
    val years: List<Int> = emptyList(),
    val resolutions: List<String> = emptyList(),
    val dynamicRanges: List<String> = emptyList(),
) {
    /** [resolutions] this app knows, in the server's order. */
    val resolutionChoices: List<ResolutionFilter> get() = resolutions.mapNotNull(ResolutionFilter::of)

    /** [dynamicRanges] this app knows, in the server's order. */
    val dynamicRangeChoices: List<DynamicRangeFilter> get() = dynamicRanges.mapNotNull(DynamicRangeFilter::of)
}

/**
 * The `resolution` of GET /api/libraries/{id}/items: a title's best
 * file's (a show's best episode's), on the lines its poster's "4K"
 * follows. [label] is how every app names it.
 */
enum class ResolutionFilter(override val wire: String, val label: String) : WireEnum {
    Uhd("4k", "4K"),

    /** Up to 1440p. */
    FullHd("1080", "1080p"),
    Hd("720", "720p"),
    Sd("sd", "SD"),
    ;

    companion object {
        /** The filter [wire] names; `null` for anything else. */
        fun of(wire: String?): ResolutionFilter? = entries.firstOrNull { it.wire == wire }
    }
}

/**
 * The `dynamic_range` of GET /api/libraries/{id}/items: a title's best
 * file's (a show's best episode's). [Hdr] is any but SDR. [label] is how
 * every app names it: HDR10 and HDR10+ as plain "HDR" and "HDR+", as on
 * the posters; [Hdr]'s in English (dynamicRangeFilterLabel words it in
 * the language shown).
 */
enum class DynamicRangeFilter(override val wire: String, val label: String) : WireEnum {
    Hdr("hdr", "Any HDR"),
    DolbyVision("dolby_vision", "Dolby Vision"),
    Hdr10Plus("hdr10plus", "HDR+"),
    Hdr10("hdr10", "HDR"),
    Hlg("hlg", "HLG"),
    Sdr("sdr", "SDR"),
    ;

    companion object {
        /** The filter [wire] names; `null` for anything else. */
        fun of(wire: String?): DynamicRangeFilter? = entries.firstOrNull { it.wire == wire }
    }
}

/**
 * The `sort` of GET /api/libraries/{id}/items (apps/server/src/catalog.rs,
 * `SortKey`). A query value, not JSON: [wire] is what goes in the address.
 */
enum class LibrarySort(override val wire: String) : WireEnum {
    /** The server's default, ascending. */
    Title("title"),
    Year("year"),

    /** Newest first by default; shows by their newest episode. */
    Added("added"),
    Rating("rating"),
    UserRating("user_rating"),
    Release("release"),
    Duration("duration"),
    LastPlayed("last_played"),

    /** The best file's bitrate (a show's: its episodes' average). */
    Bitrate("bitrate"),
}

/**
 * The `order` of a listing. Left out, the server sorts titles, years and
 * durations ascending and the rest descending.
 */
enum class SortOrder(override val wire: String) : WireEnum {
    Ascending("asc"),
    Descending("desc"),
}
