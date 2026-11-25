package app.pley.android.player.music.library

import androidx.media3.common.MediaItem
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

/** A car's last searches, kept so one asked again as letters are typed and taken back isn't asked of the server twice. */
@RunWith(RobolectricTestRunner::class)
class KeptSearchesTest {
    private var now = 0L
    private val kept = KeptSearches(clock = { now })

    private fun results(query: String) = listOf(MediaItem.Builder().setMediaId("found:$query").build())

    @Test
    fun theLastThreeSearchesAreKept() {
        for (query in listOf("h", "ha", "har")) kept.put(query, results(query), kept.generation())
        // A letter taken back: "ha" again, from what was kept.
        assertEquals(results("ha"), kept.get("ha"))
        // A fourth pushes out the one used longest ago: "h", "ha" having just been read.
        kept.put("harb", results("harb"), kept.generation())
        assertNull(kept.get("h"))
        assertEquals(results("ha"), kept.get("ha"))
        assertEquals(results("har"), kept.get("har"))
        assertEquals(results("harb"), kept.get("harb"))
    }

    @Test
    fun aSearchIsAskedAgainAfterAMinuteAndForAnotherProfile() {
        kept.put("harbour", results("harbour"), kept.generation())
        now += 59_999
        assertEquals(results("harbour"), kept.get("harbour"))
        now += 1
        assertNull(kept.get("harbour"))

        kept.put("tide", results("tide"), kept.generation())
        kept.clear()
        assertNull(kept.get("tide"))
    }

    @Test
    fun aSearchStartedBeforeAnotherProfileSignedInIsNotKeptForIt() {
        // Started for the profile before; the searches are forgotten as another signs in; then its results come.
        val started = kept.generation()
        kept.clear()
        kept.put("harbour", results("harbour"), started)
        assertNull(kept.get("harbour"))
        // One started after is kept.
        kept.put("harbour", results("harbour"), kept.generation())
        assertEquals(results("harbour"), kept.get("harbour"))
    }
}
