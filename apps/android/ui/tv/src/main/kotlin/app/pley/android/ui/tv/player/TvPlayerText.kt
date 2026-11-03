package app.pley.android.ui.tv.player

import app.pley.android.player.ChapterMark
import java.util.Locale

/** How the player writes times and titles. */
internal object TvPlayerText {
    /** "4:05", "32:40", "1:02:03". */
    fun time(ms: Long): String {
        val total = (ms.coerceAtLeast(0) / 1000)
        val h = total / 3600
        val m = (total % 3600) / 60
        val s = total % 60
        return if (h > 0) String.format(Locale.ROOT, "%d:%02d:%02d", h, m, s) else String.format(Locale.ROOT, "%d:%02d", m, s)
    }

    /** What is left, with a real minus: "−25:20". */
    fun remaining(positionMs: Long, durationMs: Long): String = "−" + time((durationMs - positionMs).coerceAtLeast(0))

    /** How screen readers say a time: "32 minutes 40 seconds", "1 hour 2 minutes". */
    fun spoken(ms: Long): String {
        val total = ms.coerceAtLeast(0) / 1000
        val h = total / 3600
        val m = (total % 3600) / 60
        val s = total % 60
        val parts = buildList {
            if (h > 0) add(if (h == 1L) "1 hour" else "$h hours")
            if (m > 0) add(if (m == 1L) "1 minute" else "$m minutes")
            if (s > 0 || isEmpty()) add(if (s == 1L) "1 second" else "$s seconds")
        }
        return parts.joinToString(" ")
    }

    /** An episode's line as the player shows it: "S1 E2 — Pilot" becomes "S1 · E2 · Pilot". */
    fun episodeLine(subtitle: String?): String? {
        if (subtitle.isNullOrBlank()) return null
        return subtitle
            .replace(Regex("""\bS(\d+) E(\d+)\b"""), "S$1 · E$2")
            .replace(" — ", " · ")
    }

    /** The chapter playing at [positionMs]; `null` outside them all. */
    fun chapterAt(chapters: List<ChapterMark>, positionMs: Long): Int? =
        chapters.indexOfLast { positionMs >= it.startMs }.takeIf { it >= 0 && positionMs < chapters[it].endMs.coerceAtLeast(chapters[it].startMs + 1) }

    /** The chapter the cards open on for [positionMs]: the one holding it, else the last to start before it, else the first. */
    fun chapterNear(chapters: List<ChapterMark>, positionMs: Long): Int =
        chapterAt(chapters, positionMs) ?: chapters.indexOfLast { it.startMs <= positionMs }.coerceAtLeast(0)

    /** "−10 s"'s number: whole seconds, never 0 for a real step. */
    fun seconds(ms: Long): Int = ((kotlin.math.abs(ms) + 500) / 1000).toInt()
}
