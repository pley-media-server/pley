package app.pley.android.ui.phone.player

import androidx.compose.runtime.Immutable
import androidx.compose.ui.graphics.Color
import app.pley.android.data.Settings
import app.pley.android.data.SubtitleBackground
import app.pley.android.designsystem.PleyColors
import app.pley.android.model.PlaybackMethod
import app.pley.android.player.ChapterMark
import app.pley.android.player.subtitles.CaptionLook
import java.util.Locale
import kotlin.math.abs

/** The method badge's colours: its text and its fill (the text at 20 %), as on the web. */
@Immutable
data class BadgeColors(val text: Color, val fill: Color)

/** The sizes and colours the player draws with (design spec §3.5). */
object PlayerLook {
    /**
     * The method badge (apps/web/src/player/VideoPlayer.tsx): direct play in
     * [PleyColors.Success] (a download plays so too), a direct stream in
     * [PleyColors.Info], a transcode, or a way not known, in [PleyColors.Accent].
     */
    fun badgeColors(method: PlaybackMethod?): BadgeColors {
        val text = when (method) {
            PlaybackMethod.Direct -> PleyColors.Success
            PlaybackMethod.Remux -> PleyColors.Info
            else -> PleyColors.Accent
        }
        return BadgeColors(text, text.copy(alpha = 0.2f))
    }

    /** The subtitle sizes offered, as the web names them, with their share of the height. */
    val subtitleSizes: List<Float> get() = Settings.SUBTITLE_SIZES

    /** Which of [subtitleSizes] [fraction] is (the nearest). */
    fun sizeIndex(fraction: Float): Int =
        subtitleSizes.indices.minBy { abs(subtitleSizes[it] - fraction) }

    /** How subtitles look with these settings: pley's own style applies over the system's once chosen. */
    fun captionLook(settings: Settings): CaptionLook = CaptionLook.of(settings)

    /** The positions offered, as shares of the height raised from the bottom. */
    val subtitleRaises: List<Float> get() = Settings.SUBTITLE_RAISES

    /** Which of [subtitleRaises] [raise] is (the nearest). */
    fun raiseIndex(raise: Float): Int = subtitleRaises.indices.minBy { abs(subtitleRaises[it] - raise) }

    /** The style the sheet shows for [settings]. */
    fun subtitleChoice(settings: Settings): SubtitleChoice = SubtitleChoice(
        sizeIndex = sizeIndex(settings.subtitleSize),
        box = settings.subtitleBackground == SubtitleBackground.DarkBox,
        font = settings.subtitleFont,
        raiseIndex = raiseIndex(settings.subtitleRaise),
    )

    /** "1:02:03" or "4:05", in every language. */
    fun clock(ms: Long): String {
        val total = (ms.coerceAtLeast(0) / 1000)
        val h = total / 3600
        val m = (total % 3600) / 60
        val s = total % 60
        return if (h > 0) String.format(Locale.ROOT, "%d:%02d:%02d", h, m, s) else String.format(Locale.ROOT, "%d:%02d", m, s)
    }

    /** "−25:20": what is left, with a true minus sign. */
    fun remaining(positionMs: Long, durationMs: Long): String = "−" + clock((durationMs - positionMs).coerceAtLeast(0))

    /** The chapter playing at [ms] (chapters are sorted by start), or -1. */
    fun chapterAt(chapters: List<ChapterMark>, ms: Long): Int {
        var found = -1
        for ((i, chapter) in chapters.withIndex()) {
            if (chapter.startMs > ms) break
            found = i
        }
        return found
    }

    /** A chapter's name, or "Chapter 3" when it has none, or only a time ([ChapterMark.name]). */
    fun chapterTitle(chapter: ChapterMark, index: Int, fallback: (Int) -> String): String =
        chapter.name ?: fallback(index + 1)
}
