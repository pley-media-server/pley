package app.pley.android.ui.tv.livetv

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/** The guide's text over the grid, cut to what its two lines show. */
class TvLiveKitTest {
    @Test
    fun `a short text is left as it is`() {
        val text = "The goals of the weekend."
        assertEquals(text, shortened(text))
        val full = "x".repeat(DESCRIPTION_CHARS)
        assertEquals(full, shortened(full))
    }

    @Test
    fun `a long text is cut at a word, with an ellipsis`() {
        val text = "What the guide says of it, at length. ".repeat(100)
        val cut = shortened(text)
        assertTrue(cut.length <= DESCRIPTION_CHARS + 1)
        assertTrue(cut.endsWith("…"))
        // At the space before the word that would run past the cut.
        assertTrue(text.startsWith(cut.dropLast(1) + " "))
    }

    @Test
    fun `a text without spaces is cut where it must, not inside a character`() {
        assertEquals("x".repeat(DESCRIPTION_CHARS) + "…", shortened("x".repeat(DESCRIPTION_CHARS * 2)))
        val emoji = "📺"
        val text = "x".repeat(DESCRIPTION_CHARS - 1) + emoji.repeat(10)
        assertEquals("x".repeat(DESCRIPTION_CHARS - 1) + "…", shortened(text))
    }
}
