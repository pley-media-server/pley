package app.pley.android.screens.library

import app.pley.android.common.UiText
import app.pley.android.model.DynamicRangeFilter
import app.pley.android.model.LibrarySort
import app.pley.android.model.ResolutionFilter
import app.pley.android.model.SortOrder
import app.pley.android.screens.R

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

    /** [SORT_WORDS] in English, for [describe]. */
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

    /** Each field's order in words: first, then reversed. */
    private val SORT_WORDS = mapOf(
        "titleSort" to (R.string.library_filter_title_asc to R.string.library_filter_title_desc),
        "year" to (R.string.library_filter_year_asc to R.string.library_filter_year_desc),
        "originallyAvailableAt" to (R.string.library_filter_release_asc to R.string.library_filter_release_desc),
        "addedAt" to (R.string.library_filter_added_asc to R.string.library_filter_added_desc),
        "rating" to (R.string.library_filter_rating_asc to R.string.library_filter_rating_desc),
        "audienceRating" to (R.string.library_filter_rating_asc to R.string.library_filter_rating_desc),
        "userRating" to (R.string.library_filter_user_rating_asc to R.string.library_filter_user_rating_desc),
        "lastViewedAt" to (R.string.library_filter_played_asc to R.string.library_filter_played_desc),
        "duration" to (R.string.library_filter_duration_asc to R.string.library_filter_duration_desc),
        "viewCount" to (R.string.library_filter_plays_asc to R.string.library_filter_plays_desc),
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

    /**
     * What a smart filter finds, in English: "Genre: Comedy", "Unwatched",
     * "Newest first"… The TV's, until it shows [words].
     */
    fun describe(filter: String): List<String> = buildList {
        for ((key, value) in parts(filter)) {
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

    /** What a smart filter finds, in words: "Genre: Comedy", "Unwatched", "Newest first"… */
    fun words(filter: String): List<UiText> = buildList {
        for ((key, value) in parts(filter)) {
            when (key) {
                "genre" -> add(UiText.res(R.string.library_filter_genre, value))
                "label" -> add(UiText.res(R.string.library_filter_label, value))
                "studio" -> add(UiText.res(R.string.library_filter_studio, value))
                "contentRating" -> add(UiText.res(R.string.library_filter_rated, value))
                "unwatched" -> add(UiText.res(if (value == "0" || value == "false") R.string.library_filter_watched else R.string.library_filter_unwatched))
                "inProgress" -> add(UiText.res(if (value == "0" || value == "false") R.string.library_filter_not_in_progress else R.string.library_filter_in_progress))
                "year" -> add(UiText.res(R.string.library_filter_from, value))
                "year>>" -> add(UiText.res(R.string.library_filter_after, value))
                "year<<" -> add(UiText.res(R.string.library_filter_before, value))
                "decade" -> add(UiText.res(R.string.library_filter_decade, value))
                // "4K", "Dolby Vision": the names the library's filters give them.
                "resolution" -> add(ResolutionFilter.of(value)?.let { UiText.raw(it.label) } ?: other(key, value))
                "dynamicRange" -> add(DynamicRangeFilter.of(value)?.labelText ?: other(key, value))
                "sort" -> add(sortWords(value))
                "limit" -> add(UiText.res(R.string.library_filter_at_most, value))
                // Plumbing, not part of what is found.
                "type", "includeGuids" -> Unit
                else -> add(other(key, value))
            }
        }
    }

    /** [filter]'s fields and their values, decoded, in its order. */
    private fun parts(filter: String): List<Pair<String, String>> = filter.split('&').filter { it.isNotEmpty() }.map { part ->
        val at = part.indexOf('=')
        val key = LibraryParams.decode(if (at < 0) part else part.substring(0, at))
        val value = if (at < 0) "" else LibraryParams.decode(part.substring(at + 1))
        key to value
    }

    /** A field Pley has no words for, as it is: "country: France". */
    private fun other(key: String, value: String): UiText = UiText.res(R.string.library_filter_other, key, value)

    private fun sortNote(value: String): String {
        val (name, descending) = sortOf(value)
        val notes = SORT_NOTES[name] ?: return "Sorted by $name"
        return if (descending) notes.second else notes.first
    }

    private fun sortWords(value: String): UiText {
        val (name, descending) = sortOf(value)
        val words = SORT_WORDS[name] ?: return UiText.res(R.string.library_filter_sorted_by, name)
        return UiText.res(if (descending) words.second else words.first)
    }

    /** A sort's field and whether it is reversed: "year:desc", or "movie.addedAt". */
    private fun sortOf(value: String): Pair<String, Boolean> {
        val first = value.split(',').first()
        val field = first.substringBefore(':')
        val direction = if (':' in first) first.substringAfter(':') else null
        // python-plexapi may name the kind first: `movie.year`.
        val name = if ('.' in field) field.substringAfter('.') else field
        return name to (direction == "desc")
    }
}

/**
 * A dynamic range's name, on the library's filters and in a smart
 * filter's words: its [label][DynamicRangeFilter.label] ("Dolby Vision",
 * "HDR+"), and any HDR in words.
 */
val DynamicRangeFilter.labelText: UiText
    get() = if (this == DynamicRangeFilter.Hdr) UiText.res(R.string.library_filter_any_hdr) else UiText.raw(label)
