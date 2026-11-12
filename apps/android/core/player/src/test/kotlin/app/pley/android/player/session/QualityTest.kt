package app.pley.android.player.session

import app.pley.android.model.MediaFile
import app.pley.android.model.PlaybackMode
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test

/** The menu of docs/QUALITY.md, and the cases of apps/web/src/player/quality.test.ts. */
class QualityTest {
    private fun ids(bitrate: Long?, height: Int?) =
        Qualities.forFile(MediaFile(id = 1, path = "a", bitrate = bitrate, height = height)).map { it.id }

    @Test
    fun `finds them by id, falling back to the original`() {
        assertEquals(4_000, Qualities.byId("4m").maxBitrateKbps)
        assertEquals(720, Qualities.byId("4m").maxHeight)
        assertSame(Qualities.ORIGINAL, Qualities.byId("nope"))
        assertEquals("original", Qualities.byId(null).id)
        assertSame(Qualities.AUTOMATIC, Qualities.byId("auto"))
        // The settings say "Original" for the original.
        assertSame(Qualities.ORIGINAL, Qualities.byIdOrLabel("Original"))
        assertSame(Qualities.AUTOMATIC, Qualities.byIdOrLabel("auto"))
    }

    @Test
    fun `the menu's order is the same on every app`() {
        assertEquals(listOf("original", "auto", "30m", "15m", "20m", "12m", "8m", "4m", "2m"), Qualities.ALL.map { it.id })
        // 15 Mbps 4K under 30 Mbps 4K, then 1080p and below.
        assertEquals(listOf(2160, 2160, 1080, 1080, 1080, 720, 480), Qualities.TRANSCODES.map { it.maxHeight })
        assertSame(Qualities.byId("2m"), Qualities.LOWEST)
    }

    @Test
    fun `Automatic is a choice that never plays as itself`() {
        assertTrue(Qualities.AUTOMATIC.automatic)
        assertFalse(Qualities.AUTOMATIC.original)
        assertFalse(Qualities.ORIGINAL.automatic)
        assertNull(Qualities.AUTOMATIC.maxBitrateKbps)
    }

    @Test
    fun `transcodes every quality but the original and Automatic`() {
        assertEquals(PlaybackMode.Auto, Qualities.byId("original").mode)
        assertEquals(PlaybackMode.Auto, Qualities.byId("auto").mode)
        assertEquals(PlaybackMode.Transcode, Qualities.byId("8m").mode)
    }

    @Test
    fun `offers only the qualities with fewer lines or at most 80 percent of the bitrate, the original and Automatic always`() {
        // 1080p at 9 Mbps: 8 Mbps is barely lighter, and plays no better.
        assertEquals(listOf("original", "auto", "4m", "2m"), ids(9_000_000, 1080))
        // 1080p at 10 Mbps: 8 Mbps is 80 %.
        assertEquals(listOf("original", "auto", "8m", "4m", "2m"), ids(10_000_000, 1080))
        assertEquals(listOf("original", "auto", "2m"), ids(3_000_000, 720))
        assertEquals(listOf("original", "auto"), ids(1_500_000, 480))
        // A 4K file at a low bitrate still gets smaller at 1080p.
        assertEquals(listOf("original", "auto", "20m", "12m", "8m", "4m", "2m"), ids(6_000_000, 2160))
        // A 4K remux: 4K at 30 Mbps too.
        assertEquals(listOf("original", "auto", "30m", "15m", "20m", "12m", "8m", "4m", "2m"), ids(60_000_000, 2160))
        // 4K at 20 Mbps: 4K at 15 is 75 %, 4K at 30 isn't smaller.
        assertEquals(listOf("original", "auto", "15m", "20m", "12m", "8m", "4m", "2m"), ids(20_000_000, 2160))
        // A 720p file at 12 Mbps: 8 Mbps and lower.
        assertEquals(listOf("original", "auto", "8m", "4m", "2m"), ids(12_000_000, 720))
        assertEquals(Qualities.ALL.size, ids(null, null).size)
        assertEquals(Qualities.ALL.size, Qualities.forFile(null).size)
        // A server that converts no video has only the original to give.
        val film = MediaFile(id = 1, path = "a", bitrate = 60_000_000, height = 2160)
        assertEquals(listOf(Qualities.ORIGINAL), Qualities.offered(film, originalOnly = true))
        assertEquals(Qualities.forFile(film), Qualities.offered(film, originalOnly = false))
    }

    @Test
    fun `a file's lines are the server's, so a scope film is 1080p`() {
        assertEquals(1080, Qualities.linesClassOf(MediaFile(id = 1, path = "a", width = 1920, height = 800)))
        assertEquals(2160, Qualities.linesClassOf(MediaFile(id = 1, path = "a", width = 3840, height = 1600)))
        assertEquals(720, Qualities.linesClassOf(MediaFile(id = 1, path = "a", height = 720)))
        assertNull(Qualities.linesClassOf(MediaFile(id = 1, path = "a")))
        // 1920×800 at 30 Mbps: 20 Mbps 1080p has no fewer lines, but is 67 % of the bitrate; 15 Mbps 4K is
        // 15 Mbps 1080p here, and goes under it.
        val scope = MediaFile(id = 1, path = "a", bitrate = 30_000_000, width = 1920, height = 800)
        assertEquals(listOf("original", "auto", "20m", "15m", "12m", "8m", "4m", "2m"), Qualities.forFile(scope).map { it.id })
    }

    @Test
    fun `qualities that come to the same bitrate and lines show once`() {
        assertEquals(1080, Qualities.linesFor(Qualities.byId("15m"), 1080))
        assertEquals(720, Qualities.linesFor(Qualities.byId("4m"), 1080))
        assertEquals(1080, Qualities.linesFor(Qualities.ORIGINAL, 1080))
        val offered = Qualities.forFile(MediaFile(id = 1, path = "a", bitrate = 80_000_000, height = 1080))
        val keys = offered.filter { it.maxBitrateKbps != null }.map { it.maxBitrateKbps to Qualities.linesFor(it, 1080) }
        assertEquals(keys.distinct(), keys)
    }

    @Test
    fun `nothing chosen is Automatic, a remembered quality the file isn't offered plays the original`() {
        val file = MediaFile(id = 1, path = "a", bitrate = 5_000_000, height = 1080)
        assertEquals("4m", Qualities.choose(file, "4m").id)
        assertSame(Qualities.ORIGINAL, Qualities.choose(file, "8m"))
        assertSame(Qualities.AUTOMATIC, Qualities.choose(file, null))
        assertSame(Qualities.AUTOMATIC, Qualities.choose(file, "auto"))
        // The original chosen before stays chosen.
        assertSame(Qualities.ORIGINAL, Qualities.choose(file, "original"))
        assertSame(Qualities.ORIGINAL, Qualities.choose(file, "Original"))
    }
}
