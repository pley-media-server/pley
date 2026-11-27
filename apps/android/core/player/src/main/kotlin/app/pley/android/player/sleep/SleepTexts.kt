package app.pley.android.player.sleep

import android.content.res.Resources
import app.pley.android.player.R

/** How the sleep timer's menus say things, for the phone and the TV alike. */
object SleepTexts {
    /** A choice: "Off", "30 min", "End of this track". */
    fun choice(res: Resources, choice: SleepChoice): String = when (choice) {
        SleepChoice.Off -> res.getString(R.string.sleep_off)
        is SleepChoice.Minutes -> res.getString(R.string.sleep_minutes, choice.minutes)
        SleepChoice.EndOfItem -> res.getString(R.string.sleep_end_of_track)
    }

    /** What the menu row says now: "Off", "42 min left", "End of this track". */
    fun summary(res: Resources, state: SleepState): String {
        val minutes = state.minutesLeft
        return if (state.choice is SleepChoice.Minutes && minutes != null) {
            res.getQuantityString(R.plurals.sleep_minutes_left, minutes, minutes)
        } else {
            choice(res, state.choice)
        }
    }

    fun title(res: Resources): String = res.getString(R.string.sleep_timer)

    /** "Pausing in 30 s". */
    fun notice(res: Resources, secondsLeft: Int): String = res.getString(R.string.sleep_notice, secondsLeft)

    /** "Keep listening". */
    fun keep(res: Resources): String = res.getString(R.string.sleep_keep_listening)
}
