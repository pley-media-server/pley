package app.pley.android.model.format

/*
 * Times as people read them, ported from apps/web/src/lib/format.ts so the
 * apps and the web say the same thing. The web's own wording keeps its
 * names (formatDuration, formatClock, formatRemaining); the native screens'
 * wording, from the design spec, has its own (formatRuntime, formatTimeLeft,
 * spokenDuration). Each takes its words last, English when given none.
 */

private const val MINUTE_MS = 60_000L

/** Whole minutes, rounding half a minute up as the web's Math.round does. */
private fun wholeMinutes(ms: Long): Long = Math.round(ms / MINUTE_MS.toDouble())

/**
 * 6720000 → "1h 52m", 2700000 → "45m", 29000 → "29s": whole minutes from
 * 30 s up, seconds below. Empty without a duration.
 */
fun formatDuration(ms: Long?, words: FormatWords = FormatWords.English): String {
    if (ms == null || ms <= 0) return ""
    val totalMin = wholeMinutes(ms)
    if (totalMin < 1) return words.seconds(maxOf(1L, Math.round(ms / 1000.0)))
    val h = totalMin / 60
    val m = totalMin % 60
    if (h == 0L) return words.minutes(m)
    return if (m != 0L) words.hoursMinutes(h, m) else words.hours(h)
}

/** Seconds → "1:02:03" or "2:03"; "0:00" for a missing or invalid time. */
fun formatClock(seconds: Double?): String {
    if (seconds == null || !seconds.isFinite() || seconds < 0) return "0:00"
    val s = seconds.toLong()
    val h = s / 3600
    val m = (s % 3600) / 60
    val sec = s % 60
    val mm = if (h > 0) m.toString().padStart(2, '0') else m.toString()
    return (if (h > 0) "$h:" else "") + mm + ":" + sec.toString().padStart(2, '0')
}

/** Milliseconds → [formatClock] of the whole seconds. */
fun formatClockMs(ms: Long?): String = formatClock(ms?.let { it / 1000.0 })

/** What is left, as the web says it: "34m left"; empty once nothing is. */
fun formatRemaining(positionMs: Long, durationMs: Long?, words: FormatWords = FormatWords.English): String {
    if (durationMs == null || durationMs <= 0) return ""
    val left = formatDuration(maxOf(0L, durationMs - positionMs), words)
    return if (left.isNotEmpty()) words.left(left) else ""
}

/**
 * A runtime as the TV and phone screens show it: "58 min" under an hour,
 * "1h 52m" from an hour (the web's [formatDuration]), "29s" below half a
 * minute. "min" reads better from the sofa than a lone "m".
 */
fun formatRuntime(ms: Long?, words: FormatWords = FormatWords.English): String {
    if (ms == null || ms <= 0) return ""
    val totalMin = wholeMinutes(ms)
    return if (totalMin in 1..59) words.runtimeMinutes(totalMin) else formatDuration(ms, words)
}

/** "26 min left", "1h 12m left"; empty without a duration or once nothing is left. */
fun formatTimeLeft(positionMs: Long, durationMs: Long?, words: FormatWords = FormatWords.English): String {
    if (durationMs == null || durationMs <= 0) return ""
    val left = formatRuntime(maxOf(0L, durationMs - positionMs), words)
    return if (left.isNotEmpty()) words.left(left) else ""
}

/**
 * A duration for screen readers, in words: "34 minutes", "1 hour 12
 * minutes", "2 hours", "45 seconds". Empty without a duration.
 */
fun spokenDuration(ms: Long?, words: FormatWords = FormatWords.English): String {
    if (ms == null || ms <= 0) return ""
    val totalMin = wholeMinutes(ms)
    if (totalMin < 1) return words.spokenSeconds(maxOf(1L, Math.round(ms / 1000.0)))
    val h = totalMin / 60
    val m = totalMin % 60
    return when {
        h == 0L -> words.spokenMinutes(m)
        m == 0L -> words.spokenHours(h)
        else -> words.spokenHoursMinutes(words.spokenHours(h), words.spokenMinutes(m))
    }
}

/** "34 minutes left", for screen readers; empty once nothing is left. */
fun spokenTimeLeft(positionMs: Long, durationMs: Long?, words: FormatWords = FormatWords.English): String {
    if (durationMs == null || durationMs <= 0) return ""
    val left = spokenDuration(maxOf(0L, durationMs - positionMs), words)
    return if (left.isNotEmpty()) words.spokenLeft(left) else ""
}

/** A server's uptime by its two largest units: "1d 1h", "1h 1m", "2m". */
fun formatUptime(secs: Long, words: FormatWords = FormatWords.English): String {
    val d = secs / 86_400
    val h = (secs % 86_400) / 3600
    val m = (secs % 3600) / 60
    return when {
        d > 0 -> words.daysHours(d, h)
        h > 0 -> words.hoursMinutes(h, m)
        else -> words.minutes(maxOf(m, 0L))
    }
}
