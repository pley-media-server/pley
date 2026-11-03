package app.pley.android.screens.livetv

import app.pley.android.model.LiveChannel
import app.pley.android.model.LiveProgram
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test

/** The TV guide's focus, moved by the guide itself (Samsung's GuideCursorTests, LiveTests.cs). */
class GuideCursorTest {
    /** A half hour. */
    private val start = 1_761_750_000L

    /** 40 minutes in. */
    private val now = start + 2400

    private val channel = LiveChannel(1, name = "One")

    private fun row() = listOf(
        LiveProgram(1, start, start + 1800, "A"),
        LiveProgram(1, start + 1800, start + 5400, "B"),
        LiveProgram(1, start + 5400, start + 12_600, "C"),
    )

    @Test
    fun `the guide opens at now, the grid from the half hour before`() {
        val c = GuideCursor(start, now)
        assertEquals(start, c.left)
        assertEquals("B", c.current(row())?.title)
        assertEquals(GuideOk.Watch, c.ok(channel, row(), now))
        assertFalse(c.awayFromNow(row(), now))
    }

    @Test
    fun `Left and Right go through programmes and the grid follows by half hours`() {
        var c = GuideCursor(start, now).moveRight(row())
        assertEquals("C", c.current(row())?.title)
        assertEquals(GuideOk.Details, c.ok(channel, row(), now))
        assertTrue(c.awayFromNow(row(), now))
        // C starts 90 minutes in: the grid keeps a half hour after it in view.
        assertEquals(start + 3600, c.left)
        c = c.moveLeft(row())!!
        assertEquals("B", c.current(row())?.title)
        c = c.moveLeft(row())!!
        assertEquals("A", c.current(row())?.title)
        assertEquals(start, c.left)
        // From the guide's first half hour, Left goes to the categories.
        assertNull(c.moveLeft(row()))
        assertEquals("B", c.backToNow(now).current(row())?.title)
    }

    @Test
    fun `a channel without programmes moves through time by half hours both ways`() {
        var c = GuideCursor(start, now).moveRight(null)
        assertEquals(GuideCursor.slot(now) + LiveTv.SLOT_S, c.cursor)
        // Left comes back the way Right went, rather than leave the guide from there, and goes on to
        // the guide's first half hour; only from there does it leave for the categories.
        c = c.moveLeft(null)!!
        assertEquals(GuideCursor.slot(now), c.cursor)
        while (true) {
            c = c.moveLeft(null) ?: break
            assertTrue(c.cursor >= start)
        }
        assertEquals(GuideCursor.slot(start), GuideCursor.slot(c.cursor))
        c = c.backToNow(now).moveRight(null)
        assertEquals(GuideOk.Watch, c.ok(channel, null, now))
        assertEquals(GuideOk.None, c.ok(null, null, now))
    }

    @Test
    fun `Left goes back through a long programme before it leaves the guide`() {
        // A five-hour programme from the guide's start, then another.
        val row = listOf(LiveProgram(1, start, start + 18_000, "Long"), LiveProgram(1, start + 18_000, start + 21_600, "Next"))
        var c = GuideCursor(start, now).moveRight(row)
        assertEquals("Next", c.current(row)?.title)
        assertTrue(c.left > start)
        c = c.moveLeft(row)!!
        assertEquals("Long", c.current(row)?.title)
        // Back through the long programme a screen at a time, the grid with it, to the guide's start.
        val lefts = mutableListOf<Long>()
        while (c.left > start) {
            val back = c.moveLeft(row)
            assertNotNull("Left left the guide with the grid scrolled on", back)
            c = back!!
            assertEquals("Long", c.current(row)?.title)
            lefts += c.left
        }
        assertTrue(lefts.size >= 2)
        // Only from there does Left leave for the categories.
        assertNull(c.moveLeft(row))
    }

    @Test
    fun `on a channel without programmes each press moves the grid a half hour`() {
        val opened = GuideCursor(start, now)
        var c = opened.moveRight(null)
        assertEquals(opened.left + LiveTv.SLOT_S, c.left)
        c = c.moveRight(null)
        assertEquals(opened.left + 2 * LiveTv.SLOT_S, c.left)
        assertTrue(c.cursor >= c.left && c.cursor < c.left + GuideCursor.VIEW_S)
        c = c.moveLeft(null)!!
        assertEquals(opened.left + LiveTv.SLOT_S, c.left)
    }

    @Test
    fun `on a channel without programmes Back comes to now once, then leaves`() {
        var c = GuideCursor(start, now)
        // At now on a channel without a guide: not away, so Back leaves the guide.
        assertFalse(c.awayFromNow(null, now))
        c = c.moveRight(null).moveRight(null)
        assertTrue(c.awayFromNow(null, now))
        assertFalse(c.backToNow(now).awayFromNow(null, now))
    }

    @Test
    fun `Up and Down keep the time, and Channel up and down move five rows`() {
        var c = GuideCursor(start, now)
        assertNull(c.moveUp())
        c = c.moveDown(3).moveDown(3).moveDown(3)
        assertEquals(2, c.row)
        c = c.moveUp()!!
        assertEquals(1, c.row)
        c = c.pageDown(30)
        assertEquals(6, c.row)
        c = c.pageUp().pageUp()
        assertEquals(0, c.row)
        c = c.place(50, 10)
        assertEquals(9, c.row)
        assertEquals(now, c.cursor)
        assertEquals(4, c.fit(5).row)
        assertSame(c, c.fit(20))
    }

    @Test
    fun `rewind and fast forward move the grid and the cursor two hours, within the guide`() {
        val opened = GuideCursor(start, now)
        val on = opened.jump(GuideCursor.JUMP_S)
        assertEquals(opened.left + GuideCursor.JUMP_S, on.left)
        assertEquals(now + GuideCursor.JUMP_S, on.cursor)
        assertEquals(opened, on.jump(-GuideCursor.JUMP_S))
        // At the guide's start, back is nothing; at its end, on is.
        assertSame(opened, opened.jump(-GuideCursor.JUMP_S))
        var last = opened
        while (true) last = last.jump(GuideCursor.JUMP_S).takeIf { it != last } ?: break
        assertEquals(last.end - GuideCursor.VIEW_S, last.left)
        assertTrue(last.cursor >= last.left && last.cursor < last.left + GuideCursor.VIEW_S)
    }
}
