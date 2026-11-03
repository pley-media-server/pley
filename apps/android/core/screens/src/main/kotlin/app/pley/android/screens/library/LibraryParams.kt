package app.pley.android.screens.library

import app.pley.android.data.LibraryQuery
import app.pley.android.model.DynamicRangeFilter
import app.pley.android.model.LibraryKind
import app.pley.android.model.LibrarySort
import app.pley.android.model.ResolutionFilter
import app.pley.android.model.SortOrder
import app.pley.android.screens.LibraryTab
import java.net.URLDecoder
import java.net.URLEncoder

/**
 * How a library page is shown: its tab, and the Library tab's sort, order
 * and filters, as the web keeps them in the page's address
 * (apps/web/src/pages/LibraryPage.tsx). [toQueryString] and [parse] turn
 * them into that address's query and back ("tab=collections",
 * "sort=year&watched=false&genre=Comedy&resolution=4k"), which is also how
 * a page keeps them across process death.
 *
 * [order] `null` is the sort's own order ([defaultOrder]); [watched] is the
 * web's: `false` lists unwatched titles only, `true` watched ones, `null`
 * both.
 */
data class LibraryParams(
    val tab: LibraryTab = LibraryTab.Library,
    val sort: LibrarySort = LibrarySort.Title,
    val order: SortOrder? = null,
    val watched: Boolean? = null,
    val genre: String? = null,
    val year: Int? = null,
    val resolution: ResolutionFilter? = null,
    val dynamicRange: DynamicRangeFilter? = null,
) {
    /** The order the listing is in: the one chosen, else the sort's own. */
    val effectiveOrder: SortOrder get() = order ?: defaultOrder(sort)

    /** A filter narrows the listing (the sort doesn't count). */
    val filtered: Boolean
        get() = watched != null || genre != null || year != null || resolution != null || dynamicRange != null

    /**
     * Posters carry their quality badges ("4K", "DV"): only where they say
     * something, sorted by bitrate or filtered by what they show. Every
     * listing has what they are made of.
     */
    val badges: Boolean get() = sort == LibrarySort.Bitrate || resolution != null || dynamicRange != null

    /** The listing as the data side asks for it. */
    fun query(): LibraryQuery = LibraryQuery(
        sort = sort,
        order = effectiveOrder,
        unwatched = watched?.not(),
        genre = genre,
        year = year,
        resolution = resolution,
        dynamicRange = dynamicRange,
    )

    /** The tab, sort, order and filters: how a library is left, to open so again. */
    fun view(): String = toQueryString()

    /** Without any filter: "Clear filters". */
    fun cleared(): LibraryParams = copy(watched = null, genre = null, year = null, resolution = null, dynamicRange = null)

    /** Sorted by [sort] in its own order, as choosing a sort does on the web. */
    fun sortedBy(sort: LibrarySort): LibraryParams = copy(sort = sort, order = null)

    /** The other order. */
    fun flipped(): LibraryParams = copy(order = if (effectiveOrder == SortOrder.Ascending) SortOrder.Descending else SortOrder.Ascending)

    /**
     * What a library of [kind] can show: a sort it offers (else Title),
     * and no filters for music, which has none on the web, nor a
     * Collections tab.
     */
    fun validFor(kind: LibraryKind): LibraryParams {
        val sorts = sortsFor(kind)
        val sort = if (sort in sorts) sort else LibrarySort.Title
        val base = copy(sort = sort, order = if (sort != this.sort) null else order)
        return if (kind == LibraryKind.Music) {
            base.copy(tab = if (tab == LibraryTab.Collections) LibraryTab.Library else tab).cleared()
        } else {
            base
        }
    }

    /**
     * The web's query: parameters at their default are left out, so a
     * fresh page is "" and the Library tab is never named.
     */
    fun toQueryString(): String = buildList {
        if (tab != LibraryTab.Library) add("tab" to tab.wire)
        if (sort != LibrarySort.Title) add("sort" to sort.wire)
        if (order != null && order != defaultOrder(sort)) add("order" to order.wire)
        if (watched != null) add("watched" to watched.toString())
        if (genre != null) add("genre" to genre)
        if (year != null) add("year" to year.toString())
        if (resolution != null) add("resolution" to resolution.wire)
        if (dynamicRange != null) add("dynamic_range" to dynamicRange.wire)
    }.joinToString("&") { (k, v) -> "$k=${encode(v)}" }

    companion object {
        /** Sorts the web offers each kind of library, in its order. */
        fun sortsFor(kind: LibraryKind): List<LibrarySort> = when (kind) {
            LibraryKind.Music -> listOf(LibrarySort.Title, LibrarySort.Added, LibrarySort.UserRating, LibrarySort.LastPlayed)
            LibraryKind.Shows -> listOf(
                LibrarySort.Title, LibrarySort.Year, LibrarySort.Added, LibrarySort.Rating, LibrarySort.UserRating,
                LibrarySort.Release, LibrarySort.LastPlayed, LibrarySort.Bitrate,
            )
            else -> listOf(
                LibrarySort.Title, LibrarySort.Year, LibrarySort.Added, LibrarySort.Rating, LibrarySort.UserRating,
                LibrarySort.Release, LibrarySort.Duration, LibrarySort.LastPlayed, LibrarySort.Bitrate,
            )
        }

        /** The tabs of a library of [kind]: music has no collections. */
        fun tabsFor(kind: LibraryKind?): List<LibraryTab> =
            if (kind == LibraryKind.Music) listOf(LibraryTab.Recommended, LibraryTab.Library) else LibraryTab.entries

        /** Titles A to Z; everything else newest, biggest or best first, as on the web. */
        fun defaultOrder(sort: LibrarySort): SortOrder = if (sort == LibrarySort.Title) SortOrder.Ascending else SortOrder.Descending

        /**
         * [query] as the web's page reads it: unknown values fall back to
         * the defaults, a page opened on [tab] (a route's) unless the query
         * names one.
         */
        fun parse(query: String, tab: LibraryTab = LibraryTab.Library): LibraryParams {
            val values = query.split('&').filter { it.isNotEmpty() }.associate { part ->
                val at = part.indexOf('=')
                if (at < 0) decode(part) to "" else decode(part.substring(0, at)) to decode(part.substring(at + 1))
            }
            val sort = LibrarySort.entries.firstOrNull { it.wire == values["sort"] } ?: LibrarySort.Title
            return LibraryParams(
                tab = values["tab"]?.let { t -> LibraryTab.entries.firstOrNull { it.wire == t } } ?: tab,
                sort = sort,
                order = SortOrder.entries.firstOrNull { it.wire == values["order"] },
                watched = when (values["watched"]) {
                    "true" -> true
                    "false" -> false
                    else -> null
                },
                genre = values["genre"]?.takeIf { it.isNotEmpty() },
                year = values["year"]?.toIntOrNull(),
                // A value the server wouldn't take (an older app's, a hand-made address) is dropped.
                resolution = ResolutionFilter.of(values["resolution"]),
                dynamicRange = DynamicRangeFilter.of(values["dynamic_range"]),
            )
        }

        internal fun encode(value: String): String = URLEncoder.encode(value, Charsets.UTF_8.name())

        internal fun decode(value: String): String = try {
            URLDecoder.decode(value, Charsets.UTF_8.name())
        } catch (_: IllegalArgumentException) {
            value
        }
    }
}

/** The tab's name in the web's address. */
val LibraryTab.wire: String
    get() = when (this) {
        LibraryTab.Recommended -> "recommended"
        LibraryTab.Library -> "library"
        LibraryTab.Collections -> "collections"
    }
