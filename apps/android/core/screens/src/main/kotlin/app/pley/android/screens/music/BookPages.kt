package app.pley.android.screens.music

import app.pley.android.common.UiText
import app.pley.android.model.Chapter
import app.pley.android.model.ItemDetail
import app.pley.android.model.ItemSummary
import app.pley.android.model.format.FormatWords
import app.pley.android.model.format.formatClockMs
import app.pley.android.model.format.formatDuration
import app.pley.android.model.format.formatTimeLeft
import app.pley.android.screens.R
import java.text.NumberFormat
import java.util.Locale

/**
 * An audiobook's page (apps/web/src/pages/item/BookView.tsx): its cover,
 * who wrote and reads it, its series, how far the profile is in it, Play
 * where it was left, and its parts, or the chapters of a book in one file.
 */
data class BookPage(
    val id: Long,
    val title: String,
    /** The server's address, which the pictures' paths are on. */
    val origin: String,
    val posterUrl: String?,
    /** "Series X, book 2", else "Audiobook": the line above the title. */
    val kicker: UiText,
    /** "By A and B"; `null` when nobody is named. */
    val byline: UiText?,
    /** "Read by C"; `null` when nobody is named. */
    val readBy: UiText?,
    /** The year, the length ("11h 20m"), "12 parts" (with several). */
    val metaTexts: List<UiText>,
    val summary: String?,
    val genres: List<String>,
    /** Part way through: how far, 0 to 1, "37 % heard" and "7h 2m left"; `null` otherwise. */
    val progress: BookProgress?,
    /** Every part heard to its end: Play says Listen again. */
    val finished: Boolean,
    /** The parts in order, which Play plays. */
    val parts: List<ItemSummary>,
    /** What the list shows: the parts, or the chapters of a book in one file. */
    val rows: List<BookRow>,
    /** "Up to Part 4", the part Play goes on from, with several parts; `null` otherwise. */
    val upTo: UiText?,
    val partsLoading: Boolean,
) {
    /** Part way through: Play says Resume, and From the start is offered. */
    val started: Boolean get() = progress != null
}

/** How far a book is heard. */
data class BookProgress(val fraction: Float, val heard: UiText, val left: String)

/** A line of a book's list. */
sealed interface BookRow {
    val key: String
    val title: UiText

    /** "1:02:10": a part's length, or where a chapter starts. */
    val time: String

    /** A part: its number, how far it is heard (0 to 1, 0 once heard to its end) and whether it is. */
    data class Part(
        val id: Long,
        val position: Int,
        val number: String,
        override val title: UiText,
        override val time: String,
        val fraction: Float,
        val heard: Boolean,
    ) : BookRow {
        override val key: String get() = "part-$id"
    }

    /** A chapter of the book's one file, from [startMs]. */
    data class Chapter(
        val index: Int,
        override val title: UiText,
        override val time: String,
        val startMs: Long,
        val endMs: Long,
    ) : BookRow {
        override val key: String get() = "chapter-$index"
    }
}

/** A list of names as people say it: "A", "A and B", "A, B and C". */
fun joinNames(names: List<String>, and: String): String = when (names.size) {
    0 -> ""
    1 -> names[0]
    else -> "${names.dropLast(1).joinToString(", ")} $and ${names.last()}"
}

/** How much of a book of [parts] has been heard: the parts heard to the end, and where it is in the others. */
fun heardMs(parts: List<ItemSummary>): Long = parts.sumOf { part ->
    val length = part.durationMs?.coerceAtLeast(0) ?: 0
    if (part.userState.watched) length else part.userState.positionMs.coerceIn(0, length.takeIf { it > 0 } ?: Long.MAX_VALUE)
}

/**
 * The page of book [detail] with its [parts] (`null` while they load), the
 * on-deck part Play goes on from ([next]), and the [chapters] of its one
 * part (empty for a book of several, or a file without). Names are joined
 * with [and]; lengths in [words].
 */
fun bookPage(
    detail: ItemDetail,
    parts: List<ItemSummary>?,
    next: ItemSummary?,
    chapters: List<Chapter>,
    origin: String,
    and: String,
    words: FormatWords,
    locale: Locale = words.locale,
): BookPage {
    val list = parts.orEmpty()
    val book = detail.book
    val total = detail.durationMs?.takeIf { it > 0 } ?: list.sumOf { it.durationMs ?: 0 }
    val heard = if (list.isEmpty()) 0 else heardMs(list)
    val finished = list.isNotEmpty() && list.all { it.userState.watched }
    val started = !finished && heard > 0 && total > 0
    val series = book?.series?.takeIf { it.isNotBlank() }
    val place = book?.seriesIndex
    val kicker = when {
        series != null && place != null -> UiText.res(R.string.book_series_place, series, numberOf(place, locale))
        series != null -> UiText.raw(series)
        else -> UiText.res(R.string.book_kind)
    }
    val authors = book?.authors?.takeIf { it.isNotEmpty() } ?: detail.authors
    val meta = listOfNotNull(
        detail.year?.let { UiText.raw(it.toString()) },
        formatDuration(total.takeIf { it > 0 }, words).takeIf { it.isNotEmpty() }?.let(UiText::raw),
        list.size.takeIf { it > 1 }?.let { UiText.plural(R.plurals.book_count_parts, it) },
    )
    val progress = if (started) {
        val fraction = (heard.toFloat() / total).coerceIn(0f, 1f)
        // Never 100 % before the end, as on the web.
        val percent = NumberFormat.getPercentInstance(locale).format(minOf(0.99, Math.floor(fraction * 100.0) / 100.0))
        BookProgress(fraction, UiText.res(R.string.book_percent_heard, percent), formatTimeLeft(heard, total, words))
    } else {
        null
    }
    val oneFile = list.size == 1 && chapters.size >= 2
    val rows = if (oneFile) chapterRows(chapters) else partRows(list)
    return BookPage(
        id = detail.id,
        title = detail.title,
        origin = origin,
        posterUrl = detail.posterUrl?.takeIf { it.isNotEmpty() },
        kicker = kicker,
        byline = authors.takeIf { it.isNotEmpty() }?.let { UiText.res(R.string.book_by, joinNames(it, and)) },
        readBy = book?.narrators?.takeIf { it.isNotEmpty() }?.let { UiText.res(R.string.book_read_by, joinNames(it, and)) },
        metaTexts = meta,
        summary = detail.summary?.takeIf { it.isNotBlank() },
        genres = detail.genres,
        progress = progress,
        finished = finished,
        parts = list,
        rows = rows,
        upTo = next?.takeIf { started && list.size > 1 }?.let { UiText.res(R.string.book_up_to, it.title) },
        partsLoading = parts == null,
    )
}

/** The parts as the list shows them: number, title, length, how far each is heard. */
fun partRows(parts: List<ItemSummary>): List<BookRow> = parts.mapIndexed { i, p ->
    val length = p.durationMs ?: 0
    BookRow.Part(
        id = p.id,
        position = i,
        number = (p.index ?: (i + 1)).toString(),
        title = UiText.raw(p.title),
        time = length.takeIf { it > 0 }?.let(::formatClockMs).orEmpty(),
        fraction = if (!p.userState.watched && length > 0) (p.userState.positionMs.toFloat() / length).coerceIn(0f, 1f) else 0f,
        heard = p.userState.watched,
    )
}

/** The chapters of a book in one file, named as the web names them ("Chapter 3" without a name of their own). */
fun chapterRows(chapters: List<Chapter>): List<BookRow> = chapters.mapIndexed { i, c ->
    BookRow.Chapter(
        index = i,
        title = c.title?.trim()?.takeIf { it.isNotEmpty() }?.let(UiText::raw) ?: UiText.res(R.string.book_chapter, i + 1),
        time = formatClockMs(c.startMs),
        startMs = c.startMs,
        endMs = c.endMs,
    )
}

/** 2 → "2", 2.5 → "2.5", in [locale]'s digits. */
private fun numberOf(value: Double, locale: Locale): String =
    NumberFormat.getNumberInstance(locale).apply { maximumFractionDigits = 2 }.format(value)
