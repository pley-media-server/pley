package app.pley.android.player.subtitles

import androidx.media3.common.text.Cue
import app.pley.android.fixtures.PgsMks
import java.io.File
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

/** Subtitles moved in time, text and pictures alike. */
@RunWith(RobolectricTestRunner::class)
class SubtitleSyncTest {
    private val a = Cue.Builder().setText("A").build()
    private val b = Cue.Builder().setText("B").build()
    private val index = SubtitleIndex(listOf(TimedCues(1_000, 4_000, listOf(a)), TimedCues(6_000, 8_000, listOf(b))))

    /** What shows with the playhead at [positionMs] and the subtitles moved by [offsetMs]. */
    private fun showing(positionMs: Long, offsetMs: Long): List<String> = runBlocking {
        index.cuesAt(SubtitleSync.trackTime(positionMs, offsetMs)).map { it.text.toString() }
    }

    @Test
    fun `a positive offset shows the subtitles later`() {
        assertEquals(emptyList<String>(), showing(1_400, 500))
        assertEquals(listOf("A"), showing(1_500, 500))
        assertEquals(listOf("A"), showing(4_499, 500))
        assertEquals(emptyList<String>(), showing(4_500, 500))
        assertEquals(listOf("B"), showing(6_500, 500))
        // The next change is counted from the moved time: 1.0 s of the track, 0.5 s later on screen.
        assertEquals(500L, SubtitleSync.untilChange(index, 1_000, 500))
    }

    @Test
    fun `a negative offset shows them earlier, even before the playhead reached them`() {
        assertEquals(listOf("A"), showing(0, -1_000))
        assertEquals(listOf("A"), showing(2_999, -1_000))
        assertEquals(emptyList<String>(), showing(3_000, -1_000))
        assertEquals(listOf("B"), showing(5_000, -1_000))
        assertEquals(1_000L, SubtitleSync.untilChange(index, 2_000, -1_000))
        // At 0 nothing moves.
        assertEquals(listOf("A"), showing(1_000, 0))
        assertEquals(emptyList<String>(), showing(999, 0))
    }

    @Test
    fun `offsets go by 0,1 s within 10 s either way, and read as on the web`() {
        assertEquals(10_000L, SubtitleSync.clamp(12_345))
        assertEquals(-10_000L, SubtitleSync.clamp(-99_999))
        assertEquals(0L, SubtitleSync.clamp(-40))
        assertEquals(100L, SubtitleSync.clamp(149))
        assertEquals(200L, SubtitleSync.clamp(150))
        assertEquals("+0.5 s", SubtitleSync.label(500))
        assertEquals("−1.2 s", SubtitleSync.label(-1_200))
        assertEquals("0.0 s", SubtitleSync.label(0))
        assertEquals("+10.0 s", SubtitleSync.label(10_000))
    }

    @Test
    fun `offsets are remembered per file, and 0 forgets`() = runBlocking {
        val offsets = SubtitleOffsets.memory()
        offsets.remember("http://nas/api/files/7", 1_200)
        offsets.remember("http://nas/api/files/8", -300)
        assertEquals(1_200L, offsets.offset("http://nas/api/files/7"))
        assertEquals(-300L, offsets.offset("http://nas/api/files/8"))
        offsets.remember("http://nas/api/files/7", 0)
        assertEquals(0L, offsets.offset("http://nas/api/files/7"))
    }

    @Test
    fun `a picture track is read out of its mks and decoded when it shows, moved like text`() = runBlocking {
        val dir = File(System.getProperty("java.io.tmpdir"), "pley-sync-test-${System.nanoTime()}").apply { mkdirs() }
        val mks = File(dir, "track.mks").apply { writeBytes(PgsMks.build(listOf(Triple(1_000L, 3_000L, "PGS 1"), Triple(5_000L, 7_000L, "PGS 2")), 10_000, "eng")) }
        val track = PictureTrack.read(mks, SampleStore(File(dir, "samples")), Dispatchers.Unconfined)
        try {
            // A display set to show each cue and one to clear it.
            assertEquals(4, track.size)
            assertEquals(emptyList<Cue>(), track.cuesAt(500))
            val shown = track.cuesAt(1_500).single()
            assertNotNull(shown.bitmap)
            val bitmap = shown.bitmap!!
            assertTrue(bitmap.width > 100)
            // Centered, near the bottom of the 1080p plane.
            assertEquals((PgsMks.WIDTH - bitmap.width) / 2f / PgsMks.WIDTH, shown.position, 0.001f)
            assertTrue(shown.line > 0.7f)
            assertEquals(emptyList<Cue>(), track.cuesAt(3_500))
            assertEquals(1, track.cuesAt(5_200).size)
            assertEquals(3_000L, track.nextChangeAfter(1_500))
            assertNull(track.nextChangeAfter(7_500))
            // Moved 2 s later: the first picture shows from 3 s on screen.
            assertEquals(0, track.cuesAt(SubtitleSync.trackTime(2_900, 2_000)).size)
            assertEquals(1, track.cuesAt(SubtitleSync.trackTime(3_100, 2_000)).size)
            // And 0.5 s earlier.
            assertEquals(1, track.cuesAt(SubtitleSync.trackTime(600, -500)).size)
        } finally {
            track.close()
            dir.deleteRecursively()
        }
    }
}
