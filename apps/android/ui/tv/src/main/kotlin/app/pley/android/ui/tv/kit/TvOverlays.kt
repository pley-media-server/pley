package app.pley.android.ui.tv.kit

import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.setValue
import androidx.compose.runtime.staticCompositionLocalOf

/**
 * What covers the page and holds focus now: a side panel, a dialog, a
 * field the keyboard types into. Requests' news waits until none does, as
 * the Samsung and LG apps' does: popping up, it would take focus from it.
 * The shell provides one; each of those counts itself in while it shows
 * ([OverlayShowing]).
 */
@Stable
class TvOverlays {
    private var open by mutableIntStateOf(0)

    /** Whether one shows now. */
    val any: Boolean get() = open > 0

    internal fun opened() {
        open++
    }

    internal fun closed() {
        open--
    }
}

/** The shell's; outside it (a screen on its own, in tests) none, and nothing is counted. */
val LocalTvOverlays = staticCompositionLocalOf<TvOverlays?> { null }

/** Counts the overlay that calls it in [LocalTvOverlays] while [shown]. */
@Composable
fun OverlayShowing(shown: Boolean = true) {
    val overlays = LocalTvOverlays.current
    if (overlays == null || !shown) return
    DisposableEffect(overlays) {
        overlays.opened()
        onDispose { overlays.closed() }
    }
}
