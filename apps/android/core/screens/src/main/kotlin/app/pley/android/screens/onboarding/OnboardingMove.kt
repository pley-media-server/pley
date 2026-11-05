package app.pley.android.screens.onboarding

import app.pley.android.screens.Navigator
import app.pley.android.screens.PleyKey

/** Where getting in goes next. The screen's interface hands it to its [Navigator]. */
sealed interface OnboardingMove {
    /** Starts over at [key]: in, signed in, or after a profile switch. */
    data class Restart(val key: PleyKey) : OnboardingMove

    /** Opens [key] above the screen. */
    data class Open(val key: PleyKey) : OnboardingMove
}

/** Goes where [move] says. */
fun Navigator.go(move: OnboardingMove) {
    when (move) {
        is OnboardingMove.Restart -> restart(move.key)
        is OnboardingMove.Open -> open(move.key)
    }
}
