package app.pley.android.designsystem

import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshots.Snapshot
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onAllNodesWithText
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

/** The words of what plays give way to the next track's over a moment, not in one frame; at once with reduced motion. */
@RunWith(RobolectricTestRunner::class)
class TrackChangeTest {
    @get:Rule
    val compose = createComposeRule()

    private var words by mutableStateOf(TrackWords(1L to 0, 0, "Song 1", "Harbour Lights"))

    private fun show(reduced: Boolean) {
        compose.setContent {
            CompositionLocalProvider(LocalReducedMotion provides reduced) {
                TrackChange(words) { shown -> BasicText(shown.title) }
            }
        }
        compose.waitForIdle()
    }

    /** The next [to] words, on the paused clock: composed on the next frame. */
    private fun change(to: TrackWords) {
        compose.runOnUiThread {
            words = to
            Snapshot.sendApplyNotifications()
        }
        compose.mainClock.advanceTimeByFrame()
    }

    private fun shown(title: String) = compose.onAllNodesWithText(title).fetchSemanticsNodes().size

    @Test
    fun theNextTracksWordsComeInAsTheLastOnesGo() {
        show(reduced = false)
        compose.mainClock.autoAdvance = false
        change(TrackWords(2L to 1, 1, "Song 2", "Harbour Lights"))
        compose.mainClock.advanceTimeBy(50)
        assertEquals(1, shown("Song 1"))
        assertEquals(1, shown("Song 2"))
        compose.mainClock.advanceTimeBy(500)
        assertEquals(0, shown("Song 1"))
        assertEquals(1, shown("Song 2"))
    }

    @Test
    fun withReducedMotionTheyChangeInPlace() {
        show(reduced = true)
        compose.mainClock.autoAdvance = false
        change(TrackWords(2L to 1, 1, "Song 2", "Harbour Lights"))
        compose.mainClock.advanceTimeByFrame()
        assertEquals(0, shown("Song 1"))
        assertEquals(1, shown("Song 2"))
    }

    @Test
    fun theSameTrackChangesNothing() {
        show(reduced = false)
        compose.mainClock.autoAdvance = false
        // A new reading of the same track (its byline filled in): no slide.
        change(TrackWords(1L to 0, 0, "Song 1", "Harbour Lights · Deep Water"))
        compose.mainClock.advanceTimeBy(50)
        assertEquals(1, shown("Song 1"))
    }
}
