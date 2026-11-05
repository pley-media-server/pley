package app.pley.android.model.format

import java.time.Instant
import java.time.LocalDate
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import java.time.format.DateTimeParseException
import java.time.format.FormatStyle
import java.util.Locale
import kotlin.math.abs

/*
 * Dates and relative times. The web leaves these to the browser's Intl in
 * the language shown; here they take a Locale (the device's by default),
 * relative times their words (English by default), and a clock, so tests
 * can pin them.
 */

/** Seconds per unit, and the age below which that unit is used, as the web's table. */
private val RELATIVE_UNITS = listOf(
    Triple(60L, RelativeUnit.Second, 1L),
    Triple(3_600L, RelativeUnit.Minute, 60L),
    Triple(86_400L, RelativeUnit.Hour, 3_600L),
    Triple(86_400L * 7, RelativeUnit.Day, 86_400L),
    Triple(86_400L * 30, RelativeUnit.Week, 86_400L * 7),
    Triple(86_400L * 365, RelativeUnit.Month, 86_400L * 30),
    Triple(Long.MAX_VALUE, RelativeUnit.Year, 86_400L * 365),
)

/**
 * Unix seconds → "3 minutes ago", "in 5 hours", "yesterday", "last week",
 * "now": the largest unit that fits, worded as Intl.RelativeTimeFormat
 * with numeric "auto" words it. "never" without a time.
 */
fun formatRelative(
    ts: Long?,
    nowSeconds: Long = System.currentTimeMillis() / 1000,
    words: FormatWords = FormatWords.English,
): String {
    if (ts == null || ts == 0L) return words.never
    val diff = ts - nowSeconds
    val ageAbs = abs(diff)
    val (_, unit, seconds) = RELATIVE_UNITS.first { ageAbs < it.first }
    val value = Math.round(diff / seconds.toDouble())
    return relativeWords(value, unit, words)
}

private fun relativeWords(value: Long, unit: RelativeUnit, words: FormatWords): String = when {
    value == 0L -> words.thisTime(unit)
    value == -1L -> words.lastTime(unit)
    value == 1L -> words.nextTime(unit)
    value < 0 -> words.ago(-value, unit)
    else -> words.later(value, unit)
}

/** "2023-04-01" → "Apr 1, 2023" in [locale]'s medium style; what doesn't parse comes back as it is. */
fun formatDate(date: String?, locale: Locale = Locale.getDefault()): String {
    if (date.isNullOrEmpty()) return ""
    return try {
        LocalDate.parse(date).format(DateTimeFormatter.ofLocalizedDate(FormatStyle.MEDIUM).withLocale(locale))
    } catch (_: DateTimeParseException) {
        date
    }
}

/** "2024-03-12" → "March 12, 2024" (en-US) or "12 March 2024" (en-GB): air dates on item pages. */
fun formatLongDate(date: String?, locale: Locale = Locale.getDefault()): String {
    if (date.isNullOrEmpty()) return ""
    return try {
        LocalDate.parse(date).format(DateTimeFormatter.ofLocalizedDate(FormatStyle.LONG).withLocale(locale))
    } catch (_: DateTimeParseException) {
        date
    }
}

/** Unix seconds → "Oct 23, 2026" in [zone]. */
fun formatDay(ts: Long?, locale: Locale = Locale.getDefault(), zone: ZoneId = ZoneId.systemDefault()): String {
    if (ts == null || ts == 0L) return ""
    return Instant.ofEpochSecond(ts).atZone(zone).toLocalDate()
        .format(DateTimeFormatter.ofLocalizedDate(FormatStyle.MEDIUM).withLocale(locale))
}

/** Unix seconds → "Oct 23, 2026, 9:41 PM": a date and a time in [zone]. */
fun formatTimestamp(ts: Long?, locale: Locale = Locale.getDefault(), zone: ZoneId = ZoneId.systemDefault()): String {
    if (ts == null || ts == 0L) return ""
    return Instant.ofEpochSecond(ts).atZone(zone)
        .format(DateTimeFormatter.ofLocalizedDateTime(FormatStyle.MEDIUM, FormatStyle.SHORT).withLocale(locale))
}
