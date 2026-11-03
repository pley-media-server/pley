package app.pley.android.ui.tv.livetv

import app.pley.android.model.LiveProgram
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/** What a guide row draws, which says when it is drawn again: not at every tick, guide answer or move. */
class TvGuideCellsTest {
    private val start = 1_761_750_000L

    /** A programme of channel 1 from [from] to [to] minutes after the guide's start. */
    private fun program(from: Int, to: Int, title: String = "At $from") = LiveProgram(1, start + from * 60L, start + to * 60L, title)

    private val programs = listOf(program(0, 30), program(30, 90), program(90, 150), program(150, 240))

    @Test
    fun `a row looks the same at the next tick, and for a new answer of the same programmes`() {
        val now = start + 40 * 60
        val drawn = RowCells.of(programs, start, start, now, focus = null, loaded = true)
        assertEquals(drawn, RowCells.of(programs.map { it.copy() }, start, start, now + 15, focus = null, loaded = true))
        assertEquals(listOf(false, true, false, false), drawn.cells.map { it.airing })
        assertEquals(listOf(true, false, false, false), drawn.cells.map { it.ended })
    }

    @Test
    fun `a programme ending or beginning draws the row again`() {
        val before = RowCells.of(programs, start, start, start + 89 * 60, focus = null, loaded = true)
        val after = RowCells.of(programs, start, start, start + 90 * 60, focus = null, loaded = true)
        assertNotEquals(before, after)
        assertEquals(listOf(false, false, true, false), after.cells.map { it.airing })
    }

    @Test
    fun `the grid moving on leaves a row the same, unless a title moves in or a programme comes into reach`() {
        val now = start + 10 * 60
        // Programmes from an hour on: the grid's half hour moves change nothing of theirs.
        val later = listOf(program(60, 120), program(120, 180))
        assertEquals(
            RowCells.of(later, start, start, now, focus = null, loaded = true),
            RowCells.of(later, start, start + 30 * 60, now, focus = null, loaded = true),
        )
        // One begun before the grid's edge moves its title in to stay in view, as far as its room allows.
        val long = listOf(program(0, 120, "A film"))
        assertEquals(0L, RowCells.of(long, start, start, now, focus = null, loaded = true).cells.single().shift)
        assertEquals(30 * 60L, RowCells.of(long, start, start + 30 * 60, now, focus = null, loaded = true).cells.single().shift)
        assertEquals(95 * 60L, RowCells.of(long, start, start + 120 * 60, now, focus = null, loaded = true).cells.single().shift)
    }

    @Test
    fun `whether the guide has answered matters only to a row without programmes`() {
        val now = start + 10 * 60
        assertEquals(RowCells.of(programs, start, start, now, focus = null, loaded = false), RowCells.of(programs, start, start, now, focus = null, loaded = true))
        val waiting = RowCells.of(null, start, start, now, focus = null, loaded = false)
        val none = RowCells.of(null, start, start, now, focus = null, loaded = true)
        assertFalse(waiting.blank)
        assertTrue(none.blank)
        assertTrue(RowCells.of(null, start, start, now, focus = -1L, loaded = true).blankFocused)
    }

    @Test
    fun `focus marks its programme only`() {
        val now = start + 10 * 60
        val focused = RowCells.of(programs, start, start, now, focus = start + 30 * 60, loaded = true)
        assertEquals(listOf(false, true, false, false), focused.cells.map { it.focused })
        assertNotEquals(RowCells.of(programs, start, start, now, focus = null, loaded = true), focused)
        assertFalse(RowCells.of(programs, start, start, now, focus = -1L, loaded = true).blankFocused)
    }
}
