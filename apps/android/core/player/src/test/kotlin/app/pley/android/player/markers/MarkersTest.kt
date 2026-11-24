package app.pley.android.player.markers

import app.pley.android.model.ItemDetail
import app.pley.android.model.ItemKind
import app.pley.android.model.Marker
import app.pley.android.model.MarkerKind
import app.pley.android.model.MarkerSource
import app.pley.android.model.MediaFile
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test

/** The cases of apps/web/src/player/SkipButton.test.tsx. */
class MarkersTest {
    private fun marker(kind: MarkerKind, start: Long, end: Long, file: Long = 1) = Marker(file, kind, start, end, MarkerSource.Chapter)

    private val intro = marker(MarkerKind.Intro, 10_000, 70_000)
    private val credits = marker(MarkerKind.Credits, 1_200_000, 1_300_000)

    @Test
    fun `takes the markers of the file being played, else the first file`() {
        val other = marker(MarkerKind.Intro, 0, 5_000, file = 2)
        val item = ItemDetail(
            id = 1, libraryId = 1, kind = ItemKind.Movie, title = "A",
            files = listOf(MediaFile(id = 1, path = "a.mkv"), MediaFile(id = 2, path = "b.mkv")),
            markers = listOf(intro, other),
        )
        assertEquals(listOf(other), Markers.forFile(item, 2))
        assertEquals(listOf(intro), Markers.forFile(item, null))
        assertEquals(emptyList<Marker>(), Markers.forFile(item.copy(markers = emptyList()), 1))
    }

    @Test
    fun `finds the segment the playhead is in, until a second before it ends`() {
        val markers = listOf(intro, credits)
        assertNull(Markers.active(markers, 9_900))
        assertSame(intro, Markers.active(markers, 10_000))
        assertSame(intro, Markers.active(markers, 68_900))
        assertNull(Markers.active(markers, 69_000))
        assertSame(credits, Markers.active(markers, 1_250_000))
    }

    @Test
    fun `lands just before an intro ends, so its last moment plays, and at the end of credits`() {
        assertEquals(69_250L, Markers.skipTarget(intro))
        assertEquals(1_300_000L, Markers.skipTarget(credits))
    }

    @Test
    fun `stays inside a very short intro`() {
        assertEquals(500L, Markers.skipTarget(marker(MarkerKind.Intro, 0, 1_000)))
    }

    @Test
    fun `announces the credits once, even when seeking back into them`() {
        val gate = CreditsGate()
        assertNull(gate.entered(null, 1_300_000))
        assertNull(gate.entered(intro, 1_300_000))
        assertSame(credits, gate.entered(credits, 1_300_000))
        assertNull(gate.entered(credits, 1_300_000))
        // Other credits (another file's, or another segment) are announced.
        val other = marker(MarkerKind.Credits, 1_200_000, 1_300_000, file = 2)
        assertSame(other, gate.entered(other, 1_300_000))
        gate.reset()
        assertSame(credits, gate.entered(credits, 1_300_000))
    }

    @Test
    fun `credits run to the end when they end at most 15 s before it`() {
        assertTrue(Markers.reachesEnd(credits, 1_300_000))
        assertTrue(Markers.reachesEnd(credits, 1_310_000))
        assertTrue(Markers.reachesEnd(credits, 1_315_000))
        assertFalse(Markers.reachesEnd(credits, 1_315_001))
        assertFalse(Markers.reachesEnd(credits, 1_360_000))
        // With no length known, they don't.
        assertFalse(Markers.reachesEnd(credits, null))
        assertFalse(Markers.reachesEnd(credits, 0))
        assertFalse(Markers.reachesEnd(intro, 70_000))
    }

    @Test
    fun `announces only credits that run to the end`() {
        val gate = CreditsGate()
        // An after-credits scene follows: Skip Credits only.
        assertNull(gate.entered(credits, 1_360_000))
        assertNull(gate.entered(credits, null))
        // Turned down, they are still announced once known to run to the end.
        assertSame(credits, gate.entered(credits, 1_310_000))
    }
}
