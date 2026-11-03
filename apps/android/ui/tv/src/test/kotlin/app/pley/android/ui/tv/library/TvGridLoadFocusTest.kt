package app.pley.android.ui.tv.library

import android.view.View
import androidx.compose.foundation.focusable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.grid.LazyGridState
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.assertCountEquals
import androidx.compose.ui.test.assertIsFocused
import androidx.compose.ui.test.assertIsNotFocused
import androidx.compose.ui.test.isFocused
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
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
import java.util.concurrent.TimeUnit
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.shadows.ShadowBuild
import org.robolectric.shadows.ShadowLooper
import org.robolectric.util.ReflectionHelpers

/**
 * A TV grid's focus while its pages load, and how it moves: a cell
 * focused as a skeleton keeps focus when its title arrives, and the D-pad
 * goes on from it; a press glides the grid a row over 150 ms, and a held
 * Down glides it on a row a step, on the gate's clock, without a stop or
 * a jump (rule 5); a held Up stays in the grid, and in its column, on
 * cells laid out on the screen: the grid composes rows off the screen as
 * a TV's prefetching does, and focus never goes to one of those.
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
    private lateinit var view: View

    /** The grid, with something focusable above it when [above] (the library's chips). */
    private fun grid(reduced: Boolean? = null, above: Boolean = false, tileAt: (Int) -> ArtworkTileModel? = { tiles[it] }) {
        // Compose prefetches nothing under Robolectric, known by its fingerprint, nor for a window that
        // doesn't show, as Robolectric's says; a TV composes the cache window's rows off the screen.
        ShadowBuild.setFingerprint("pley/tv")
        compose.setContent {
            TvTheme(reducedMotion = reduced) {
                focus = remember { TvGridFocus() }
                state = rememberTvGridState()
                view = LocalView.current
                CompositionLocalProvider(LocalKeyRepeatGate provides gate) {
                    Column(Modifier.fillMaxSize().keyRepeatGate(gate)) {
                        if (above) Box(Modifier.size(40.dp).testTag(ABOVE).focusable())
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
                            modifier = Modifier.weight(1f).fillMaxWidth(),
                        )
                    }
                }
            }
        }
        ReflectionHelpers.setField(ReflectionHelpers.getField<Any>(view, "mAttachInfo"), "mWindowVisibility", View.VISIBLE)
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

    @Test
    fun aHeldUpStaysInTheGridAndAPressLeavesIt() {
        grid(above = true)
        compose.onRoot().performKeyInput { pressKey(Key.DirectionDown) }
        compose.waitForIdle()
        assertEquals(Columns, focus.focused)
        compose.mainClock.autoAdvance = false
        compose.onRoot().performKeyInput { keyDown(Key.DirectionUp) }
        // The press moves up to the first row; held from 180 ms, Up goes no further.
        compose.mainClock.advanceTimeBy(600)
        compose.onRoot().performKeyInput { keyUp(Key.DirectionUp) }
        compose.mainClock.autoAdvance = true
        compose.waitForIdle()
        assertEquals(0, focus.focused)
        compose.onNodeWithTag(ABOVE).assertIsNotFocused()
        compose.onRoot().performKeyInput { pressKey(Key.DirectionUp) }
        compose.waitForIdle()
        compose.onNodeWithTag(ABOVE).assertIsFocused()
    }

    /**
     * A frame, with the main looper's work after it: the grid's prefetching
     * runs there, as on a TV, composing the cache window's rows off the
     * screen without laying them out.
     */
    private fun frame() {
        compose.mainClock.advanceTimeByFrame()
        ShadowLooper.idleMainLooper(16, TimeUnit.MILLISECONDS)
    }

    /** Whether the focused cell is laid out on the grid, not only composed ahead of it. */
    private fun focusLaidOut(): Boolean = state.layoutInfo.visibleItemsInfo.any { it.key == focus.focused }

    /** Focus on row 8, column 3, with the rows the cache window composes around it. */
    private fun deepInTheGrid() {
        grid(above = true)
        compose.onRoot().performKeyInput {
            repeat(9) { pressKey(Key.DirectionDown) }
            repeat(3) { pressKey(Key.DirectionRight) }
        }
        compose.waitForIdle()
        // Up once, so the cache window reaches up the grid.
        compose.onRoot().performKeyInput { pressKey(Key.DirectionUp) }
        compose.waitForIdle()
        repeat(3) { frame() }
        compose.waitForIdle()
        assertEquals(Columns * 8 + 3, focus.focused)
        // As on a TV, the row above the screen's is composed (the window ahead of it) but not laid out.
        val above = state.layoutInfo.visibleItemsInfo.minOf { it.index } / Columns * Columns - Columns + 3
        val laidOut = state.layoutInfo.visibleItemsInfo.map { it.key }
        assertTrue("$above composed; laid out: $laidOut", focus.slots.has(above))
        assertTrue("$above not laid out: $laidOut", above !in laidOut)
    }

    @Test
    fun aHeldUpKeepsItsColumn() {
        deepInTheGrid()
        compose.mainClock.autoAdvance = false
        compose.onRoot().performKeyInput { keyDown(Key.DirectionUp) }
        // The press and five held steps, a frame at a time: six rows up, in the same column all the way,
        // on a cell laid out on the grid.
        for (frame in 1..25) {
            frame()
            assertEquals("frame $frame", 3, focus.focused % Columns)
            assertTrue("frame $frame: ${focus.focused} laid out", focusLaidOut())
        }
        compose.onRoot().performKeyInput { keyUp(Key.DirectionUp) }
        compose.mainClock.autoAdvance = true
        compose.waitForIdle()
        assertEquals(Columns * 2 + 3, focus.focused)
        // Held on to the top, it stays in the column there too.
        compose.mainClock.autoAdvance = false
        compose.onRoot().performKeyInput { keyDown(Key.DirectionUp) }
        compose.mainClock.advanceTimeBy(600)
        compose.onRoot().performKeyInput { keyUp(Key.DirectionUp) }
        compose.mainClock.autoAdvance = true
        compose.waitForIdle()
        assertEquals(3, focus.focused)
    }

    @Test
    fun aLongHeldUpEndsAtTheTopInItsColumn() {
        deepInTheGrid()
        compose.mainClock.autoAdvance = false
        compose.onRoot().performKeyInput { keyDown(Key.DirectionUp) }
        // Three seconds: up to the first row, and held there.
        for (frame in 1..180) {
            frame()
            assertEquals("frame $frame", 3, focus.focused % Columns)
            assertTrue("frame $frame: ${focus.focused} laid out", focusLaidOut())
        }
        compose.onRoot().performKeyInput { keyUp(Key.DirectionUp) }
        compose.mainClock.autoAdvance = true
        compose.waitForIdle()
        assertEquals(3, focus.focused)
        compose.onAllNodes(isFocused()).assertCountEquals(1)
        compose.onNodeWithTag(ABOVE).assertIsNotFocused()
    }

    @Test
    fun pressesUpKeepTheirColumn() {
        deepInTheGrid()
        for (row in 7 downTo 0) {
            compose.onRoot().performKeyInput { pressKey(Key.DirectionUp) }
            compose.waitForIdle()
            frame()
            assertEquals(Columns * row + 3, focus.focused)
            assertTrue("row $row laid out", focusLaidOut())
        }
    }

    private companion object {
        /** The library grid's columns on TV. */
        val Columns = tvGridColumns(TileKind.LibraryGrid)

        const val ABOVE = "above"
    }
}
