package app.pley.android.ui.tv.player

import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import kotlin.math.pow

/**
 * Left and Right in the player (TV addendum §5.2, design spec §2.7):
 * - **A press** moves the target 10 s. Presses in a row add up ("+30 s")
 *   and seek once, [SETTLE_MS] after the last one ([settle]), so a few
 *   quick presses don't start the server's conversion again for each.
 * - **A held key** scrubs: the target runs faster the longer the key is
 *   held, from [HOLD_START_RATE], doubling every [HOLD_DOUBLES_MS], the
 *   preview follows it, and nothing seeks until the key is let go; then it
 *   seeks once ([release]). That keeps within the TV memory guide's 5 s of
 *   pre-buffer while seeking.
 *
 * [position] and [duration] are the playback's now; [seek] makes the seek.
 * Main thread only; times are the key events' (uptime ms).
 */
@Stable
internal class TvSeeker(
    private val position: () -> Long,
    private val duration: () -> Long,
    private val seek: (Long) -> Unit,
) {
    /** Where the seek will land, while one is coming: the scrubber and its preview show it. */
    var target: Long? by mutableStateOf(null)
        private set

    /** A key is held: scrubbing through the previews. */
    var holding: Boolean by mutableStateOf(false)
        private set

    /** How far the presses in a row go, for the seek badge ("+30 s"); `null` when none. */
    var badgeMs: Long? by mutableStateOf(null)
        private set

    /** Changes with every press: the badge and the settling wait start again from it. */
    var presses: Int by mutableIntStateOf(0)
        private set

    /** Changes with every seek made: OK confirms it for [CONFIRM_MS] after. */
    var seeks: Int by mutableIntStateOf(0)
        private set

    private var confirmable = false
    private var asked = 0L
    private var holdStart = 0L
    private var lastEvent = 0L

    /** A press of Left ([direction] −1) or Right (+1), or a jump of [stepMs] (fast forward and rewind). */
    fun press(direction: Int, stepMs: Long = STEP_MS) {
        val base = target ?: position().also { asked = 0 }
        // The badge says what was asked for ("−20 s"), even where the start or end cuts it short.
        asked += direction * stepMs
        target = clamp(base + direction * stepMs)
        badgeMs = asked
        presses++
    }

    /** The key repeats ([eventTime], ms): it's held, and the target runs on. */
    fun repeat(direction: Int, eventTime: Long) {
        if (!holding) {
            if (target == null) target = position()
            holding = true
            holdStart = eventTime
            lastEvent = eventTime
            badgeMs = null
            return
        }
        val dt = (eventTime - lastEvent).coerceIn(0, MAX_GAP_MS)
        lastEvent = eventTime
        val rate = rateAt(eventTime - holdStart)
        target = clamp((target ?: position()) + direction * (rate * dt).toLong())
    }

    /** The key is let go: a scrub seeks now. Returns whether one did. */
    fun release(): Boolean {
        if (!holding) return false
        holding = false
        commit()
        return true
    }

    /** [SETTLE_MS] after press number [press]: seeks, unless more presses came or a key is held. */
    fun settle(press: Int) {
        if (press != presses || holding) return
        commit()
    }

    /** The badge has shown long enough. */
    fun hideBadge(press: Int) {
        if (press == presses) badgeMs = null
    }

    /**
     * OK pressed while a seek is coming, or just after one: as on most TV
     * players, OK after Left and Right says "go there", not "pause". A
     * coming seek is made now. Returns whether OK confirmed a seek; if not,
     * it plays or pauses as usual.
     */
    fun confirm(): Boolean {
        if (target != null) {
            holding = false
            commit()
            // Confirmed: the next OK pauses.
            confirmable = false
            return true
        }
        val confirmed = confirmable
        confirmable = false
        return confirmed
    }

    /** [CONFIRM_MS] after seek number [seek]: OK plays and pauses again. */
    fun settled(seek: Int) {
        if (seek == seeks) confirmable = false
    }

    /** Forgets a seek on its way (the playback changed under it). */
    fun cancel() {
        target = null
        holding = false
        badgeMs = null
        confirmable = false
    }

    private fun commit() {
        val to = target ?: return
        target = null
        seek(to)
        confirmable = true
        seeks++
    }

    private fun clamp(ms: Long): Long {
        val d = duration()
        return if (d > 0) ms.coerceIn(0, (d - END_MARGIN_MS).coerceAtLeast(0)) else ms.coerceAtLeast(0)
    }

    /**
     * How fast a held key scrubs, in ms of video per ms held: smoothly
     * faster the longer it's held, up to a top that grows with the video.
     */
    internal fun rateAt(heldMs: Long): Double =
        minOf(HOLD_START_RATE * 2.0.pow(heldMs / HOLD_DOUBLES_MS.toDouble()), maxOf(HOLD_TOP_RATE, duration() / FULL_SWEEP_MS.toDouble()))

    companion object {
        /** A press of Left or Right. */
        const val STEP_MS = 10_000L

        /** Presses in a row seek once, this long after the last. */
        const val SETTLE_MS = 400L

        /** The seek badge shows this long after the last press. */
        const val BADGE_MS = 600L

        /** OK this soon after a seek confirms it instead of pausing: a seek far off takes a moment to show. */
        const val CONFIRM_MS = 2_000L

        /** Seeks stop short of the end, so the end isn't reached by seeking. */
        const val END_MARGIN_MS = 1_000L

        /** A held key's speed as it starts, in ms of video per ms held. */
        const val HOLD_START_RATE = 20.0

        /** A held key's speed doubles this often. */
        const val HOLD_DOUBLES_MS = 1_500L

        /** The speed it stops at: at least this, or a whole video in [FULL_SWEEP_MS]. */
        const val HOLD_TOP_RATE = 300.0

        /** At its top speed, a whole video goes by in about this long. */
        const val FULL_SWEEP_MS = 10_000L

        /** A gap between repeats longer than this counts as this (a stalled frame isn't a jump). */
        private const val MAX_GAP_MS = 200L
    }
}
