package app.pley.android.screens.library

import app.pley.android.model.DynamicRangeFilter
import app.pley.android.model.LibrarySort
import app.pley.android.model.ResolutionFilter
import app.pley.android.model.SortOrder

/**
 * Smart collection and playlist filters: a library listing in Plex's
 * syntax (`genre=Comedy&unwatched=1&sort=year:desc`, docs/COMPATIBILITY.md), ported
 * from apps/web/src/lib/smartFilter.ts.
 */
object SmartFilter {
    private val PLEX_SORT = mapOf(
        LibrarySort.Title to "titleSort",
        LibrarySort.Year to "year",
        LibrarySort.Added to "addedAt",
        LibrarySort.Rating to "rating",
        LibrarySort.UserRating to "userRating",
        LibrarySort.Release to "originallyAvailableAt",
        LibrarySort.Duration to "duration",
        LibrarySort.LastPlayed to "lastViewedAt",
        LibrarySort.Bitrate to "mediaBitrate",
    )

    private val SORT_NOTES = mapOf(
        "titleSort" to ("By title" to "By title, Z to A"),
        "year" to ("Oldest first" to "Newest first"),
        "originallyAvailableAt" to ("Earliest release first" to "Latest release first"),
        "addedAt" to ("First added first" to "Recently added first"),
        "rating" to ("Lowest rated first" to "Best rated first"),
        "audienceRating" to ("Lowest rated first" to "Best rated first"),
        "userRating" to ("Your lowest rated first" to "Your best rated first"),
        "lastViewedAt" to ("Least recently played first" to "Recently played first"),
        "duration" to ("Shortest first" to "Longest first"),
        "viewCount" to ("Least played first" to "Most played first"),
    )

    /**
     * The library page's filters and order as a smart filter: what "Save
     * as…" saves. Resolution is Plex's own field, dynamic range Pley's
     * (`dynamicRange`), each with the listing's token.
     */
    fun of(params: LibraryParams): String = buildList {
        params.genre?.let { add("genre" to it) }
        params.year?.let { add("year" to it.toString()) }
        params.resolution?.let { add("resolution" to it.wire) }
        params.dynamicRange?.let { add("dynamicRange" to it.wire) }
        params.watched?.let { add("unwatched" to if (it) "0" else "1") }
        val field = PLEX_SORT.getValue(params.sort)
        add("sort" to if (params.effectiveOrder == SortOrder.Descending) "$field:desc" else field)
    }.joinToString("&") { (k, v) -> "${LibraryParams.encode(k)}=${LibraryParams.encode(v)}" }

    /** What a smart filter finds, in words: "Genre: Comedy", "Unwatched", "Newest first"… */
    fun describe(filter: String): List<String> = buildList {
        for (part in filter.split('&')) {
            if (part.isEmpty()) continue
            val at = part.indexOf('=')
            val key = LibraryParams.decode(if (at < 0) part else part.substring(0, at))
            val value = if (at < 0) "" else LibraryParams.decode(part.substring(at + 1))
            when (key) {
                "genre" -> add("Genre: $value")
                "label" -> add("Label: $value")
                "studio" -> add("Studio: $value")
                "contentRating" -> add("Rated $value")
                "unwatched" -> add(if (value == "0" || value == "false") "Watched" else "Unwatched")
                "inProgress" -> add(if (value == "0" || value == "false") "Not in progress" else "In progress")
                "year" -> add("From $value")
                "year>>" -> add("After $value")
                "year<<" -> add("Before $value")
                "decade" -> add("The ${value}s")
                // "4K", "Dolby Vision": the names the library's filters give them.
                "resolution" -> add(ResolutionFilter.of(value)?.label ?: "$key: $value")
                "dynamicRange" -> add(DynamicRangeFilter.of(value)?.label ?: "$key: $value")
                "sort" -> add(sortNote(value))
                "limit" -> add("At most $value")
                // Plumbing, not part of what is found.
                "type", "includeGuids" -> Unit
                else -> add("$key: $value")
            }
        }
    }

    private fun sortNote(value: String): String {
        val first = value.split(',').first()
        val field = first.substringBefore(':')
        val direction = if (':' in first) first.substringAfter(':') else null
        // python-plexapi may name the kind first: `movie.year`.
        val name = if ('.' in field) field.substringAfter('.') else field
        val notes = SORT_NOTES[name] ?: return "Sorted by $name"
        return if (direction == "desc") notes.second else notes.first
    }
}
