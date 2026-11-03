package app.pley.android.player

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/** What chapters are called where they show (the native Samsung app's PlayerRulesTests, the same cases). */
class ChapterMarkTest {
    private fun chapter(title: String?) = ChapterMark(title, 379_212, 500_000, null)

    @Test
    fun `a chapter is called by its title, trimmed`() {
        assertEquals("Opening", chapter("Opening").name)
        assertEquals("The Heist", chapter("  The Heist ").name)
    }

    @Test
    fun `a chapter without a title has no name, and is numbered`() {
        assertNull(chapter(null).name)
        assertNull(chapter("").name)
        assertNull(chapter("   ").name)
    }

    @Test
    fun `a chapter named only for where it starts has no name, and is numbered`() {
        for (title in listOf("00:06:19.212", "00:06:19", "6:19", " 01:02:03,500 ")) {
            assertNull(title, chapter(title).name)
        }
    }

    @Test
    fun `a time inside a real name keeps the name`() {
        assertEquals("Act 2: 12:00 at Noon", chapter("Act 2: 12:00 at Noon").name)
        assertEquals("Chapter 6:19", chapter("Chapter 6:19").name)
    }
}
