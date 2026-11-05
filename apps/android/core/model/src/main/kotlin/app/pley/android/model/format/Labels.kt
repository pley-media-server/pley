package app.pley.android.model.format

import app.pley.android.model.ItemKind
import app.pley.android.model.ItemSummary
import java.text.NumberFormat
import java.util.Locale

/*
 * Titles, counts and episode numbers, as apps/web/src/lib/format.ts makes
 * them, plus the spoken descriptions screen readers get for tiles. Each
 * takes its words last, English when given none.
 */

/** "1 movie", "0 movies", "12,345 movies", "3 children": the count in [locale]'s digits. */
fun pluralize(n: Int, one: String, many: String = "${one}s", locale: Locale = Locale.getDefault()): String =
    "${NumberFormat.getIntegerInstance(locale).format(n)} ${if (n == 1) one else many}"

/** "S1 · E3", "E3", or "" with neither number. */
fun episodeLabel(seasonNumber: Int?, episodeNumber: Int?, words: FormatWords = FormatWords.English): String = when {
    seasonNumber != null && episodeNumber != null -> words.seasonEpisode(seasonNumber, episodeNumber)
    seasonNumber != null -> words.seasonShort(seasonNumber)
    episodeNumber != null -> words.episodeShort(episodeNumber)
    else -> ""
}

/** [episodeLabel] of an episode: its season is [ItemSummary.parentIndex]. */
fun episodeLabel(item: ItemSummary, words: FormatWords = FormatWords.English): String =
    episodeLabel(item.parentIndex, item.index, words)

/** A season's own title, else "Specials" for season 0, else "Season 2". */
fun seasonTitle(title: String?, index: Int?, words: FormatWords = FormatWords.English): String = when {
    !title.isNullOrEmpty() -> title
    index == 0 -> words.specials
    index != null -> words.season(index)
    else -> words.seasonUnnumbered
}

/** "Season 2 · Episode 5", for the episode lines of item pages; the parts that are known. */
fun episodeLongLabel(seasonNumber: Int?, episodeNumber: Int?, words: FormatWords = FormatWords.English): String = listOfNotNull(
    seasonNumber?.let { if (it == 0) words.specials else words.season(it) },
    episodeNumber?.let { words.episode(it) },
).joinToString(" · ")

/** The short line under a card, by kind: a year, "3 seasons", "S2 · E5", "Daft Punk · 2001"… */
fun cardSubtitle(item: ItemSummary, locale: Locale = Locale.getDefault()): String =
    cardSubtitle(item, FormatWords.English, locale)

/** [cardSubtitle] in [words], its counts in [locale]'s digits. */
fun cardSubtitle(item: ItemSummary, words: FormatWords, locale: Locale = words.locale): String = when (item.kind) {
    ItemKind.Movie -> item.year?.toString().orEmpty()
    ItemKind.Show -> when {
        item.childCount != null && item.childCount > 0 -> formatCount(Counted.Seasons, item.childCount, words, locale)
        item.leafCount != null -> formatCount(Counted.Episodes, item.leafCount, words, locale)
        else -> item.year?.toString().orEmpty()
    }
    ItemKind.Season -> item.leafCount?.let { formatCount(Counted.Episodes, it, words, locale) }.orEmpty()
    ItemKind.Episode -> episodeLabel(item, words)
    ItemKind.Artist -> item.childCount?.let { formatCount(Counted.Albums, it, words, locale) }.orEmpty()
    ItemKind.Album -> listOfNotNull(item.parentTitle?.takeIf { it.isNotEmpty() }, item.year?.toString()).joinToString(" · ")
    ItemKind.Track -> listOfNotNull(
        item.grandparentTitle?.takeIf { it.isNotEmpty() },
        item.parentTitle?.takeIf { it.isNotEmpty() },
    ).joinToString(" · ")
    else -> ""
}

/** "3 seasons", "12,345 episodes": [n] of [what] in [words], the number in [locale]'s digits. */
fun formatCount(what: Counted, n: Int, words: FormatWords, locale: Locale = words.locale): String =
    words.count(what, n, NumberFormat.getIntegerInstance(locale).format(n))

/** A title with its context: "The Office — S2 · E3 — Office Olympics", "The Office — Season 2", "Heat (1995)". */
fun fullTitle(item: ItemSummary, words: FormatWords = FormatWords.English): String = when (item.kind) {
    ItemKind.Episode -> listOf(item.grandparentTitle.orEmpty(), episodeLabel(item, words), item.title)
        .filter { it.isNotEmpty() }.joinToString(" — ")
    ItemKind.Season -> listOf(item.parentTitle.orEmpty(), seasonTitle(item.title, item.index, words))
        .filter { it.isNotEmpty() }.joinToString(" — ")
    ItemKind.Track -> listOf(item.title, item.grandparentTitle.orEmpty()).filter { it.isNotEmpty() }.joinToString(" — ")
    else -> if (item.year != null) "${item.title} (${item.year})" else item.title
}

/** How much of an item has been played, 0 to 1. */
fun progressFraction(positionMs: Long, durationMs: Long?): Float {
    if (positionMs <= 0 || durationMs == null || durationMs <= 0) return 0f
    return (positionMs.toDouble() / durationMs).coerceIn(0.0, 1.0).toFloat()
}

/** [progressFraction] of an item's resume point. */
fun progressFraction(item: ItemSummary): Float = progressFraction(item.userState.positionMs, item.durationMs)

/** Episodes of a show or season not watched yet; 0 for other kinds. */
fun unwatchedCount(item: ItemSummary): Int {
    val leaves = item.leafCount ?: return 0
    return maxOf(0, leaves - (item.viewedLeafCount ?: 0))
}

/** A video the player plays: a movie, an episode or an extra (not a show, a season or music). */
fun isPlayableVideo(kind: ItemKind): Boolean = kind == ItemKind.Movie || kind == ItemKind.Episode || kind == ItemKind.Extra

/** An item's resume point is worth showing: started, not finished, not marked watched. */
fun isInProgress(item: ItemSummary): Boolean {
    val duration = item.durationMs ?: return false
    return !item.userState.watched && item.userState.positionMs > 0 && item.userState.positionMs < duration
}

/**
 * What a screen reader says for a tile: the title, then what places it,
 * then its state. "Glasshouse, Season 2 Episode 5, 26 minutes left";
 * "Northern Lights, 2024, watched"; "Severance, 2 seasons, 6 unwatched".
 */
fun tileDescription(item: ItemSummary, locale: Locale = Locale.getDefault()): String =
    tileDescription(item, FormatWords.English, locale)

/** [tileDescription] in [words], its counts in [locale]'s digits. */
fun tileDescription(item: ItemSummary, words: FormatWords, locale: Locale = words.locale): String {
    val context = when (item.kind) {
        ItemKind.Episode -> listOfNotNull(
            item.grandparentTitle?.takeIf { it.isNotEmpty() && it != item.title },
            episodeLongLabel(item.parentIndex, item.index, words).replace(" · ", " ").takeIf { it.isNotEmpty() },
        ).joinToString(", ")
        else -> cardSubtitle(item, words, locale)
    }
    val unwatched = unwatchedCount(item)
    val state = when {
        isInProgress(item) -> spokenTimeLeft(item.userState.positionMs, item.durationMs, words)
        item.userState.watched -> words.watched
        (item.kind == ItemKind.Show || item.kind == ItemKind.Season) && unwatched > 0 -> words.unwatched(unwatched)
        else -> ""
    }
    return listOf(item.title, context, state).filter { it.isNotEmpty() }.joinToString(", ")
}
