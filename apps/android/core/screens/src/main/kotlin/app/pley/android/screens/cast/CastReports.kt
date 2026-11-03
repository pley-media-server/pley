package app.pley.android.screens.cast

import app.pley.android.model.CastPlayState
import app.pley.android.model.CastReport
import app.pley.android.player.PlayerPhase
import app.pley.android.player.PlayerUiState
import kotlin.math.abs

/**
 * When a TV says what its video player plays (PUT /api/cast/devices/{id}/state),
 * for one player screen, from the player's state as it changes:
 *
 * 1. when what plays or how changes: the title known (buffering, then
 *    playing), a pause, a resume, buffering again, the next episode;
 * 2. after a seek: the position is more than [JUMP_MS] from where it
 *    would be;
 * 3. every [EVERY_MS] while it plays;
 * 4. nothing plays once the title ends or fails, and when the player
 *    closes ([closed]), unless it said so already.
 *
 * The native Samsung app follows the same rules.
 */
class CastReports {
    private var last: CastReport? = null
    private var lastAtMs = 0L
    private var lastSpeed = 1f

    /** What to report for [state] at [nowMs]; `null`: nothing new. */
    fun next(state: PlayerUiState, nowMs: Long): CastReport? {
        val report = reportOf(state) ?: return null
        val before = last
        val due = when {
            before == null -> true
            report.itemId != before.itemId || report.state != before.state -> true
            report.itemId == null -> false
            report.state == CastPlayState.Playing && nowMs - lastAtMs >= EVERY_MS -> true
            else -> abs(report.positionMs - expected(before, nowMs)) > JUMP_MS
        }
        if (!due) return null
        last = report
        lastAtMs = nowMs
        lastSpeed = state.speed
        return report
    }

    /** The player closed: it plays nothing, unless that was said already (or nothing was ever said). */
    fun closed(): CastReport? {
        val before = last ?: return null
        if (before.itemId == null) return null
        return CastReport().also { last = it }
    }

    /** Where the last report's position is at [nowMs]: moved on at the playing speed while it played. */
    private fun expected(before: CastReport, nowMs: Long): Long {
        if (before.state != CastPlayState.Playing) return before.positionMs
        return before.positionMs + ((nowMs - lastAtMs) * lastSpeed).toLong()
    }

    companion object {
        /** Between two reports while playing. */
        const val EVERY_MS = 10_000L

        /** A position this far from where it would be was moved to: a seek. */
        const val JUMP_MS = 3_000L

        /** What [state] says: `null` while the title isn't known yet. */
        fun reportOf(state: PlayerUiState): CastReport? {
            if (state.itemId <= 0) return null
            val playState = when (state.phase) {
                PlayerPhase.Playing -> CastPlayState.Playing
                PlayerPhase.Paused -> CastPlayState.Paused
                PlayerPhase.Preparing, PlayerPhase.Switching, PlayerPhase.Buffering -> CastPlayState.Buffering
                PlayerPhase.Ended, is PlayerPhase.Error -> return CastReport()
            }
            return CastReport(
                itemId = state.itemId,
                positionMs = state.positionMs.coerceAtLeast(0),
                durationMs = state.durationMs.takeIf { it > 0 },
                state = playState,
            )
        }
    }
}
