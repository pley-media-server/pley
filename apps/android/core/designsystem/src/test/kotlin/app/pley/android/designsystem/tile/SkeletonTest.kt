package app.pley.android.designsystem.tile

import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshots.Snapshot
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.unit.dp
import app.pley.android.designsystem.LocalReducedMotion
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

/** The skeleton's shimmer: it moves only while a skeleton is drawn with its look, and never on TV. */
@RunWith(RobolectricTestRunner::class)
class SkeletonTest {
    @get:Rule
    val compose = createComposeRule()

    @Test
    fun theShimmerMovesOnlyWhileASkeletonIsDrawn() {
        var loading by mutableStateOf(true)
        lateinit var look: SkeletonState
        compose.mainClock.autoAdvance = false
        compose.setContent {
            CompositionLocalProvider(LocalReducedMotion provides false) {
                look = rememberSkeleton(animated = true)
                if (loading) SkeletonTile(look, TileKind.Poster, TileStyle.Phone, width = 112.dp)
            }
        }
        compose.mainClock.advanceTimeBy(300)
        val earlier = look.shimmer
        compose.mainClock.advanceTimeBy(300)
        assertTrue(look.shown)
        assertNotEquals(earlier, look.shimmer, 0f)

        // The content came, the screen keeps its look: the clock stops with the last skeleton.
        Snapshot.withMutableSnapshot { loading = false }
        compose.mainClock.advanceTimeBy(100)
        assertFalse(look.shown)
        val stopped = look.shimmer
        compose.mainClock.advanceTimeBy(600)
        assertEquals(stopped, look.shimmer, 0f)

        // A part still loading shows skeletons again: it moves again.
        Snapshot.withMutableSnapshot { loading = true }
        compose.mainClock.advanceTimeBy(300)
        assertTrue(look.shown)
        assertNotEquals(stopped, look.shimmer, 0f)
    }

    @Test
    fun aStillLookNeverMoves() {
        lateinit var look: SkeletonState
        compose.mainClock.autoAdvance = false
        compose.setContent {
            look = rememberSkeleton(animated = false)
            SkeletonTile(look, TileKind.Poster, TileStyle.Tv)
        }
        compose.mainClock.advanceTimeBy(1_000)
        assertTrue(look.shown)
        assertEquals(0f, look.shimmer, 0f)
    }
}