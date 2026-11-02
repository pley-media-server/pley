package app.pley.android.ui.tv.player

import app.pley.android.player.ChapterMark
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/** Which chapter plays, and which one the chapters' cards open on. */
class TvPlayerTextTest {
    // A gap between the first two, as some files have.
    private val chapters = listOf(
        ChapterMark("Opening", 60_000, 300_000, null),
        ChapterMark("The Records", 320_000, 1_800_000, null),
        ChapterMark("Credits", 1_800_000, 3_600_000, null),
    )

    @Test
    fun `the chapter playing is the one holding the time, none outside them`() {
        assertNull(TvPlayerText.chapterAt(chapters, 30_000))
        assertEquals(0, TvPlayerText.chapterAt(chapters, 60_000))
        assertNull(TvPlayerText.chapterAt(chapters, 310_000))
        assertEquals(1, TvPlayerText.chapterAt(chapters, 320_000))
        assertEquals(2, TvPlayerText.chapterAt(chapters, 1_800_000))
    }

    @Test
    fun `the cards open on the chapter holding the time, else the last before it, else the first`() {
        assertEquals(1, TvPlayerText.chapterNear(chapters, 610_000))
        assertEquals("in the gap: the chapter before", 0, TvPlayerText.chapterNear(chapters, 310_000))
        assertEquals("before them all: the first", 0, TvPlayerText.chapterNear(chapters, 30_000))
        assertEquals("past the end: the last", 2, TvPlayerText.chapterNear(chapters, 4_000_000))
    }
}
