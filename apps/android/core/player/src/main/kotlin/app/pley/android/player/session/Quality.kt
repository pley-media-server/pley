package app.pley.android.player.session

import app.pley.android.model.MediaFile
import app.pley.android.model.PlaybackMode
import app.pley.android.model.StreamKind

/**
 * A quality the player offers (docs/QUALITY.md): the original, Automatic,
 * or a transcode capped at [maxBitrateKbps] and [maxHeight]. [id] is what
 * the server's `auto_quality` and the settings use ("8m").
 *
 * Automatic is a choice, never what plays: it plays the original or one of
 * the transcodes ([AutoQuality] decides which), so [original] stays true
 * only for the file as it is.
 */
data class QualityPreset(
    val id: String,
    val maxBitrateKbps: Int?,
    val maxHeight: Int?,
) {
    val original: Boolean get() = id == ORIGINAL_ID

    /** Automatic: the original, lower only when it doesn't play well. */
    val automatic: Boolean get() = id == AUTOMATIC_ID

    /**
     * How the quality is asked for: the original lets the server choose
     * (direct play when the device can), every other quality is a transcode.
     */
    val mode: PlaybackMode get() = if (original || automatic) PlaybackMode.Auto else PlaybackMode.Transcode

    companion object {
        const val ORIGINAL_ID = "original"
        const val AUTOMATIC_ID = "auto"
    }
}

/** The menu's qualities, in its order, and which of them a file is offered. */
object Qualities {
    val ORIGINAL = QualityPreset(QualityPreset.ORIGINAL_ID, null, null)
    val AUTOMATIC = QualityPreset(QualityPreset.AUTOMATIC_ID, null, null)

    /** The transcodes, in the menu's order: the two 4K ones, then 1080p and below. */
    val TRANSCODES: List<QualityPreset> = listOf(
        QualityPreset("30m", 30_000, 2160),
        QualityPreset("15m", 15_000, 2160),
        QualityPreset("20m", 20_000, 1080),
        QualityPreset("12m", 12_000, 1080),
        QualityPreset("8m", 8_000, 1080),
        QualityPreset("4m", 4_000, 720),
        QualityPreset("2m", 2_000, 480),
    )

    /** The menu, as on every app: Original, Automatic, then the transcodes. */
    val ALL: List<QualityPreset> = listOf(ORIGINAL, AUTOMATIC) + TRANSCODES

    /** A quality is offered for a file's bitrate when it is at most this share of it. */
    const val LIGHTER = 0.8

    /** The lowest quality Automatic plays. */
    val LOWEST: QualityPreset = TRANSCODES.last()

    /** The preset with [id], else the original. */
    fun byId(id: String?): QualityPreset = ALL.firstOrNull { it.id == id } ?: ORIGINAL

    /**
     * The qualities worth offering for [file] (docs/QUALITY.md): the
     * original and Automatic, then those that make it smaller, with fewer
     * lines or at most 80 % of its bitrate (a copy a little lighter plays no
     * better). Two that come to the same bitrate and lines for it show once,
     * and they go by the lines they give the file, then bitrate.
     * Every one when its bitrate and size are unknown.
     */
    fun forFile(file: MediaFile?): List<QualityPreset> {
        val kbps = file?.bitrate?.takeIf { it > 0 }?.let { it / 1000.0 }
        val lines = linesClassOf(file)
        if (kbps == null && lines == null) return ALL
        val seen = HashSet<Pair<Int, Int?>>()
        val smaller = TRANSCODES.filter { q ->
            ((kbps != null && (q.maxBitrateKbps ?: 0) <= kbps * LIGHTER) || (lines != null && (q.maxHeight ?: 0) < lines)) &&
                seen.add((q.maxBitrateKbps ?: 0) to linesFor(q, lines))
        }
        // By the lines they give the file, then bitrate: a 4K file keeps the table's order.
        return listOf(ORIGINAL, AUTOMATIC) + smaller.sortedWith(
            compareByDescending<QualityPreset> { linesFor(it, lines) ?: 0 }.thenByDescending { it.maxBitrateKbps ?: 0 },
        )
    }

    /**
     * The qualities the menu offers for [file]: those worth offering
     * ([forFile]), or the original alone on a server that converts no video
     * (`PlaybackInfo.originalOnly`), which has no other to give.
     */
    fun offered(file: MediaFile?, originalOnly: Boolean): List<QualityPreset> = if (originalOnly) listOf(ORIGINAL) else forFile(file)

    /** The lines [preset] gives a file of [fileLines]: never more than the file's. */
    fun linesFor(preset: QualityPreset, fileLines: Int?): Int? {
        val most = preset.maxHeight ?: return fileLines
        return if (fileLines != null && fileLines < most) fileLines else most
    }

    /**
     * The quality chosen for [file]: [remembered] when the file is offered
     * it, Automatic when nothing was ever chosen, else the original (an
     * 8 Mbps cap on a 5 Mbps file plays the original, as on the web).
     */
    fun choose(file: MediaFile?, remembered: String?): QualityPreset {
        if (remembered.isNullOrBlank()) return AUTOMATIC
        val wanted = byIdOrLabel(remembered)
        return forFile(file).firstOrNull { it.id == wanted.id } ?: ORIGINAL
    }

    /** A setting's value: a preset's id ("8m", "auto"), or "Original" as the settings say it. */
    fun byIdOrLabel(value: String?): QualityPreset {
        if (value == null) return ORIGINAL
        return ALL.firstOrNull { it.id.equals(value, ignoreCase = true) } ?: ORIGINAL
    }

    /** The file's bitrate in kbps; `null` when unknown. */
    fun fileKbps(file: MediaFile?): Int? = file?.bitrate?.takeIf { it > 0 }?.let { (it / 1000).toInt() }

    /** The file's lines; `null` when unknown. */
    fun linesOf(file: MediaFile?): Int? =
        file?.height?.takeIf { it > 0 } ?: file?.streams?.firstOrNull { it.kind == StreamKind.Video }?.height?.takeIf { it > 0 }

    /**
     * The file's lines as qualities name them (2160, 1080, 720 or 480), by
     * its width or height as the server tells them apart: a 1920×800 film
     * is 1080p. `null` when neither is known.
     */
    fun linesClassOf(file: MediaFile?): Int? {
        val video = file?.streams?.firstOrNull { it.kind == StreamKind.Video }
        val w = file?.width ?: video?.width ?: 0
        val h = linesOf(file) ?: 0
        if (w <= 0 && h <= 0) return null
        return when {
            w >= 3800 || h >= 2100 -> 2160
            w >= 1800 || h >= 1000 -> 1080
            w >= 1200 || h >= 700 -> 720
            else -> 480
        }
    }

    /**
     * What the server converts [file] at when it converts the original
     * (apps/server/src/playback.rs, closest_quality): its own size's
     * quality, with the highest bitrate up to 1.5 times the file's, or
     * that size's lowest. The server says so itself (`auto_quality`) only
     * when it converts; Automatic needs it before.
     */
    fun closest(file: MediaFile?): QualityPreset {
        val lines = linesClassOf(file) ?: 1080
        val cap = file?.bitrate?.takeIf { it > 0 }?.let { it * 1.5 / 1000.0 }
        val tier = TRANSCODES.filter { it.maxHeight == lines }.sortedByDescending { it.maxBitrateKbps }
        return tier.firstOrNull { cap == null || (it.maxBitrateKbps ?: 0) <= cap } ?: tier.lastOrNull() ?: byId("8m")
    }
}
