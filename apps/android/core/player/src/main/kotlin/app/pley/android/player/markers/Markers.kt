package app.pley.android.player.markers

import app.pley.android.model.ItemDetail
import app.pley.android.model.Marker
import app.pley.android.model.MarkerKind

/**
 * Skip Intro and Skip Credits, as the web does them
 * (apps/web/src/player/SkipButton.tsx). Nothing is skipped by itself: the
 * button only offers it (automatic skipping was removed from the web in
 * ba6efb7).
 */
object Markers {
    /** The button goes this long before the segment ends. */
    const val TAIL_MS = 1_000L

    /** Skip Intro lands this long before the intro ends, so its last moment plays. */
    const val INTRO_LEAD_MS = 750L

    /**
     * How long Skip shows by itself from the moment it appears ([SkipWindow]);
     * after that it shows only with the player's controls.
     */
    const val SKIP_ALONE_MS = 8_000L

    /**
     * Credits ending this close to the end of their file run to the end: what
     * follows them is a logo, a promo or black (apps/server/src/markers/mod.rs,
     * `CREDITS_TAIL_MS`).
     */
    const val CREDITS_TAIL_MS = 15_000L

    /** The markers of the file playing (the item's first file when it isn't known yet). */
    fun forFile(item: ItemDetail, fileId: Long?): List<Marker> {
        val id = fileId ?: item.files.firstOrNull()?.id ?: return emptyList()
        return item.markers.filter { it.fileId == id && it.kind != MarkerKind.Unknown }
    }

    /** The marker the playhead is in: from its start until a second before its end. */
    fun active(markers: List<Marker>, positionMs: Long): Marker? =
        markers.firstOrNull { positionMs >= it.startMs && positionMs < it.endMs - TAIL_MS }

    /** Where skipping [marker] lands: just before an intro ends, at the end of credits. */
    fun skipTarget(marker: Marker): Long {
        val lead = if (marker.kind == MarkerKind.Intro) minOf(INTRO_LEAD_MS, (marker.endMs - marker.startMs) / 2) else 0
        return marker.endMs - lead
    }

    /** Identifies a segment, to tell entering it again apart from entering another. */
    fun keyOf(marker: Marker): String = "${marker.fileId}:${marker.kind.wire}:${marker.startMs}"

    /**
     * Whether [marker] is credits that run to the end of their file, [durationMs]
     * long (`null` when it isn't known), as the server's `Marker::reaches_end` has it.
     */
    fun reachesEnd(marker: Marker, durationMs: Long?): Boolean =
        marker.kind == MarkerKind.Credits && durationMs != null && durationMs > 0 && durationMs - marker.endMs <= CREDITS_TAIL_MS
}

/**
 * Skip shows by itself for [Markers.SKIP_ALONE_MS] on the wall clock from
 * the moment it appears, then only while the player's controls show, so it
 * doesn't sit over a whole intro or the credits. Entering a segment again
 * (seeking out and back in, or the next one) starts a fresh window; seeking
 * inside the same segment doesn't.
 */
class SkipWindow(private val aloneMs: Long = Markers.SKIP_ALONE_MS) {
    private var key: String? = null
    private var since = 0L

    /**
     * Whether Skip, offered for [marker] at [nowMs], shows by itself. [marker]
     * is `null` while Skip isn't offered, or is hidden under something that
     * takes its place (Up Next's card): it starts again when it appears.
     */
    fun alone(marker: Marker?, nowMs: Long): Boolean {
        if (marker == null) {
            key = null
            return false
        }
        val entered = Markers.keyOf(marker)
        if (entered != key) {
            key = entered
            since = nowMs
        }
        return nowMs - since < aloneMs
    }

    /** A new item: its segments start fresh windows, whatever they're called. */
    fun reset() {
        key = null
    }
}

/**
 * Entering credits that run to the end of their file brings up Up Next, once
 * per segment: seeking back into them later lets them play (SkipButton.tsx,
 * useSkipMarkers). Credits that something follows (an after-credits scene, a
 * show's last minutes) only offer Skip Credits, and Up Next waits for the
 * end: its countdown would skip what follows them.
 */
class CreditsGate {
    private val announced = mutableSetOf<String>()

    /**
     * The credits [marker] the playhead just entered for the first time, when
     * they run to the end of their file, [durationMs] long; `null` otherwise.
     */
    fun entered(marker: Marker?, durationMs: Long?): Marker? {
        if (marker == null || !Markers.reachesEnd(marker, durationMs)) return null
        return if (announced.add(Markers.keyOf(marker))) marker else null
    }

    /** A new item: every segment may be announced again. */
    fun reset() = announced.clear()
}
