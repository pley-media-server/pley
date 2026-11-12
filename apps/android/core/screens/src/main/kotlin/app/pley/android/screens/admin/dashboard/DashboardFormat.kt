package app.pley.android.screens.admin.dashboard

import app.pley.android.model.ItemKind
import app.pley.android.model.ItemSummary
import app.pley.android.model.StreamAction
import app.pley.android.model.admin.DashboardTranscode
import app.pley.android.model.format.FormatWords
import java.util.Locale

/*
 * How the dashboard writes what it shows, as the web's
 * (apps/web/src/lib/format.ts and images.ts).
 */

/**
 * What a transcode does, in one line, as the web's transcodeSummary:
 * "HEVC 3840×2160 → H.264 1920×1080 · EAC3 5.1 (copy) · h264_nvenc ·
 * cuda decode · libplacebo · size 2 of 3 · 1.4×". [size] words which of
 * a transcode's sizes the player takes ("size 2 of 3").
 */
fun dashboardTranscodeSummary(t: DashboardTranscode, words: FormatWords, size: (Int, Int) -> String): String {
    val parts = mutableListOf<String>()
    when (t.video) {
        StreamAction.Transcode -> parts += listOfNotNull(t.videoIn, t.videoOut).filter { it.isNotEmpty() }
            .joinToString(" → ").ifEmpty { words.videoTranscode }
        StreamAction.Copy -> parts += words.copied(t.videoIn?.takeIf { it.isNotEmpty() } ?: words.video)
        else -> Unit
    }
    when (t.audio) {
        StreamAction.Transcode -> listOfNotNull(t.audioIn, t.audioOut).filter { it.isNotEmpty() }
            .joinToString(" → ").takeIf { it.isNotEmpty() }?.let { parts += it }
        StreamAction.Copy -> t.audioIn?.takeIf { it.isNotEmpty() }?.let { parts += words.copied(it) }
        else -> Unit
    }
    val transcoding = t.video == StreamAction.Transcode
    if (transcoding) t.encoder?.takeIf { it.isNotEmpty() }?.let { parts += it }
    // The CPU decoding a job whose GPU does the rest is worth saying: the GPU couldn't decode it.
    val decoded = t.decoder != "cpu" || t.pipeline == "hybrid"
    if (transcoding && decoded) t.decoder?.takeIf { it.isNotEmpty() }?.let { parts += words.decoded(it) }
    t.tonemap?.takeIf { it.isNotEmpty() }?.let { parts += it }
    if (t.variants > 0) parts += size(t.rendition + 1, t.variants + 1)
    if (t.subtitleBurn) parts += words.burnIn
    t.speed?.takeIf { it > 0 }?.let { parts += "${oneDecimal(it, words)}×" }
    return parts.joinToString(" · ")
}

/** [value] with one decimal, in the words' language: "1.4", "1,4". */
fun oneDecimal(value: Double, words: FormatWords): String =
    String.format(Locale.ROOT, "%.1f", value).replace('.', words.decimalSeparator)

/** "1.2.0 (abc1234)": a version and the commit it was built from, when it says. */
fun versionLabel(version: String, commit: String?): String =
    if (!commit.isNullOrEmpty() && commit != "unknown") "$version ($commit)" else version

/**
 * The 16:9 pictures of what a session plays, to try in order, as the
 * web's thumbCandidates: a movie's backdrop, the item's frame, an
 * episode's show's backdrop, then the item's backdrop.
 */
fun sessionArt(item: ItemSummary): List<String> {
    val out = mutableListOf<String>()
    val backdrop = item.backdropUrl?.takeIf { it.isNotEmpty() }
    if (item.kind == ItemKind.Movie && backdrop != null) out += backdrop
    item.thumbUrl?.takeIf { it.isNotEmpty() }?.let { out += it }
    if (item.kind == ItemKind.Episode && item.grandparentId != null) out += "/api/items/${item.grandparentId}/images/backdrop"
    if (item.kind != ItemKind.Episode && backdrop != null && backdrop !in out) out += backdrop
    return out
}

/**
 * The share of a measure, as the web's charts write it: one decimal
 * below 10 % ("4.5 %"), none from there or at 0 ("12 %").
 */
fun percentText(percent: Double, locale: Locale): String {
    val digits = if (percent == 0.0 || percent >= 10) 0 else 1
    val format = java.text.NumberFormat.getPercentInstance(locale)
    format.minimumFractionDigits = digits
    format.maximumFractionDigits = digits
    return format.format(percent / 100)
}
