package app.pley.android.ui.tv.library

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.lazy.grid.LazyGridState
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.assertCountEquals
import androidx.compose.ui.test.isFocused
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onRoot
import androidx.compose.ui.test.performKeyInput
import androidx.compose.ui.test.pressKey
import app.pley.android.designsystem.tile.ArtworkTileModel
import app.pley.android.designsystem.tile.TileKind
import app.pley.android.designsystem.tile.rememberSkeleton
import app.pley.android.ui.tv.kit.KeyRepeatGate
import app.pley.android.ui.tv.kit.LocalKeyRepeatGate
import app.pley.android.ui.tv.kit.keyRepeatGate
import app.pley.android.ui.tv.kit.rememberFocusMemory
import app.pley.android.ui.tv.theme.TvTheme
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

/**
 * A TV grid's focus while its pages load, and how it moves: a cell
 * focused as a skeleton keeps focus when its title arrives, and the D-pad
 * goes on from it; a press glides the grid a row over 150 ms, and a held
 * Down glides it on a row a step, on the gate's clock, without a stop or
 * a jump (rule 5).
 */
@OptIn(ExperimentalTestApi::class)
@RunWith(RobolectricTestRunner::class)
@Config(qualifiers = "w960dp-h540dp-xhdpi")
class TvGridLoadFocusTest {
    @get:Rule
    val compose = createComposeRule()

    private val tiles = List(120) { ArtworkTileModel(id = it + 1L, title = "Title ${it + 1}", subtitle = "2024") }

    /** On the test's clock. The test sends no repeats, as the system would: none is waited for. */
    private val gate = KeyRepeatGate(firstRepeatMs = 60_000) { compose.mainClock.currentTime }

    private lateinit var focus: TvGridFocus
    private lateinit var state: LazyGridState

    private fun grid(reduced: Boolean? = null, tileAt: (Int) -> ArtworkTileModel? = { tiles[it] }) {
        compose.setContent {
            TvTheme(reducedMotion = reduced) {
                focus = remember { TvGridFocus() }
                state = rememberTvGridState()
                CompositionLocalProvider(LocalKeyRepeatGate provides gate) {
                    Box(Modifier.fillMaxSize().keyRepeatGate(gate)) {
                        TvPosterGrid(
                            total = tiles.size,
                            tileAt = tileAt,
                            keyAt = { it },
                            kind = TileKind.LibraryGrid,
                            onOpen = { _, _ -> },
                            memory = rememberFocusMemory(),
                            memoryPrefix = "grid",
                            skeleton = rememberSkeleton(animated = false),
                            state = state,
                            focus = focus,
                            modifier = Modifier.fillMaxSize(),
                        )
                    }
                }
            }
        }
        compose.waitForIdle()
    }

    /** From one row's top to the next's, in pixels: they all have the one height. */
    private fun rowPitch(): Float = state.layoutInfo.visibleItemsInfo.let { (it[Columns].offset.y - it[0].offset.y).toFloat() }

    /** How far the grid has scrolled, in pixels. */
    private fun scrolled(): Float = state.firstVisibleItemIndex / Columns * rowPitch() + state.firstVisibleItemScrollOffset

    @Test
    fun aFocusedSkeletonKeepsFocusWhenItsPageLoads() {
        var loaded by mutableStateOf(false)
        grid(reduced = true, tileAt = { if (loaded) tiles[it] else null })
        compose.onRoot().performKeyInput { repeat(3) { pressKey(Key.DirectionDown) } }
        compose.waitForIdle()
        val before = focus.focused
        assertEquals(Columns * 3, before)

        loaded = true
        compose.waitForIdle()
        compose.onAllNodes(isFocused()).assertCountEquals(1)
        assertEquals(before, focus.focused)

        compose.onRoot().performKeyInput { pressKey(Key.DirectionRight) }
        compose.waitForIdle()
        assertEquals(before + 1, focus.focused)
    }

    @Test
    fun aPressGlidesTheGridARowOver150MsEasedOut() {
        grid()
        val pitch = rowPitch()
        compose.mainClock.autoAdvance = false
        compose.onRoot().performKeyInput { pressKey(Key.DirectionDown) }
        // Eased out: well over half the way a third of the way through.
        compose.mainClock.advanceTimeBy(80)
        val partway = scrolled()
        assertTrue("$partway of $pitch", partway > pitch / 2 && partway < pitch)
        compose.mainClock.advanceTimeBy(128)
        assertEquals(pitch, scrolled(), 1f)
        assertEquals(Columns, focus.focused)
    }

    @Test
    fun aHeldDownGlidesTheGridOnARowAStep() {
        grid()
        val pitch = rowPitch()
        compose.mainClock.autoAdvance = false
        compose.onRoot().performKeyInput { keyDown(Key.DirectionDown) }
        var last = scrolled()
        // 25 frames, 400 ms: the press, then a step every 45 ms from 180 ms, each glide running into the next.
        for (frame in 1..25) {
            compose.mainClock.advanceTimeByFrame()
            val moved = scrolled() - last
            last += moved
            assertTrue("frame $frame moved $moved of $pitch", moved < pitch / 2)
            if (frame > 13) assertTrue("frame $frame stood still", moved > 0f)
        }
        compose.onRoot().performKeyInput { keyUp(Key.DirectionDown) }
        compose.mainClock.autoAdvance = true
        compose.waitForIdle()
        // The press and five held steps: six rows down, that row at the top.
        assertEquals(Columns * 6, focus.focused)
        assertEquals(6 * pitch, scrolled(), 1f)
    }

    private companion object {
        /** The library grid's columns on TV. */
        val Columns = tvGridColumns(TileKind.LibraryGrid)
    }
}
