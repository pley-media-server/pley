package app.pley.android.ui.tv.kit

import android.view.View
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.SemanticsNodeInteraction
import androidx.compose.ui.test.SemanticsNodeInteractionsProvider
import androidx.compose.ui.test.isFocused
import androidx.compose.ui.test.junit4.ComposeContentTestRule
import androidx.compose.ui.test.onRoot
import androidx.compose.ui.test.performKeyInput
import androidx.compose.ui.test.pressKey
import app.pley.android.designsystem.tile.ArtworkTileModel
import java.util.concurrent.TimeUnit
import org.robolectric.shadows.ShadowBuild
import org.robolectric.shadows.ShadowLooper
import org.robolectric.util.ReflectionHelpers

/** Presses [key] on the remote, as a D-pad does: down and up. */
@OptIn(ExperimentalTestApi::class)
internal fun ComposeContentTestRule.press(key: Key, times: Int = 1) {
    repeat(times) {
        onRoot().performKeyInput { pressKey(key) }
        waitForIdle()
    }
}

/**
 * Holds [key] down for [ms], frame by frame, then lets it go: its press,
 * then held moves from 400 ms, for a [KeyRepeatGate] on the test's clock
 * that the content has.
 */
@OptIn(ExperimentalTestApi::class)
internal fun ComposeContentTestRule.hold(key: Key, ms: Long = 600) {
    mainClock.autoAdvance = false
    onRoot().performKeyInput { keyDown(key) }
    mainClock.advanceTimeBy(ms)
    onRoot().performKeyInput { keyUp(key) }
    mainClock.autoAdvance = true
    waitForIdle()
}

/** The node that has focus now. */
internal fun SemanticsNodeInteractionsProvider.focused(): SemanticsNodeInteraction = onNode(isFocused())

/** Made-up tiles [from] to [from] + [count] - 1, titled "<prefix> <id>". */
internal fun tiles(prefix: String, count: Int, from: Long = 1): List<ArtworkTileModel> =
    (from until from + count).map { ArtworkTileModel(id = it, title = "$prefix $it", subtitle = "2024") }

/**
 * Lazy lists prefetch as on a TV: their cache windows compose items off
 * the screen. Compose prefetches nothing under Robolectric, known by its
 * fingerprint, nor for a window that doesn't show, as Robolectric's says.
 * Call it before setContent, then [showWindow] with the content's view.
 */
internal fun prefetchAsOnATv() = ShadowBuild.setFingerprint("pley/tv")

/** The content's window shows, for [prefetchAsOnATv]: [view] is LocalView's. */
internal fun showWindow(view: View) =
    ReflectionHelpers.setField(ReflectionHelpers.getField<Any>(view, "mAttachInfo"), "mWindowVisibility", View.VISIBLE)

/** A frame, with the main looper's work after it: lazy lists' prefetching runs there, as on a TV. */
internal fun ComposeContentTestRule.frameAndPrefetch() {
    mainClock.advanceTimeByFrame()
    ShadowLooper.idleMainLooper(16, TimeUnit.MILLISECONDS)
}
