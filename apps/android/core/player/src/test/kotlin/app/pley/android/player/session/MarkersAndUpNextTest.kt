package app.pley.android.player.session

import app.pley.android.data.LocalFile
import app.pley.android.data.LocalMedia
import app.pley.android.fixtures.FixtureIds
import app.pley.android.model.Marker
import app.pley.android.model.MarkerKind
import app.pley.android.model.MarkerSource
import app.pley.android.model.ItemSummary
import app.pley.android.player.PlayQueue
import app.pley.android.player.PlayRequest
import app.pley.android.player.PlayerPhase
import kotlinx.coroutines.runBlocking
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

/** Skip Intro, Skip Credits and Up Next on "Fixture Show", whose 20 s episodes have an intro (2-8 s) and credits (16 s on). */
@RunWith(RobolectricTestRunner::class)
class MarkersAndUpNextTest {
    private var h = SessionHarness()

    @After
    fun close() = h.close()

    private fun episode(startMs: Long, id: Long = FixtureIds.SHOW_S1E1) = h.play(PlayRequest(id, startMs = startMs)).also {
        h.engine.durationMs = 20_000
        h.engine.ready()
    }

    @Test
    fun `Skip Intro shows inside the intro and lands 750 ms before its end`() {
        val c = episode(0)
        h.advance(1_500)
        assertNull(c.state.value.skipLabel)
        h.advance(1_000)
        assertEquals("Skip Intro", c.state.value.skipLabel)
        assertEquals(2, c.state.value.markers.size)
        c.skip()
        assertEquals(FixtureIds.INTRO_END_MS - 750, h.engine.seeks.single())
        h.advance(500)
        // A second before the intro ends, the button goes.
        assertNull(c.state.value.skipLabel)
        // Nothing skips by itself.
        assertEquals(1, h.engine.seeks.size)
    }

    /** The first episode with only an intro, from 2 s to 15 s: long enough for Skip's 8 s by itself. */
    private fun longIntro(): SessionPlaybackController {
        h.editDetail = { d -> d.copy(markers = listOf(Marker(d.files.first().id, MarkerKind.Intro, 2_000, 15_000, MarkerSource.Fingerprint))) }
        return episode(0)
    }

    @Test
    fun `Skip shows by itself for 8 s from entering the intro, then only with the controls, and afresh on entering it again`() {
        val c = longIntro()
        h.advance(2_500)
        assertEquals("Skip Intro", c.state.value.skipLabel)
        assertTrue(c.state.value.skipAlone)
        // It appeared at 2 or 2.5 s: by itself until 8 s after.
        h.advance(7_000)
        assertTrue(c.state.value.skipAlone)
        h.advance(1_500)
        assertEquals("Skip Intro", c.state.value.skipLabel)
        assertFalse(c.state.value.skipAlone)

        // A seek inside the intro: still with the controls only.
        c.seekTo(12_000)
        h.advance(500)
        assertEquals("Skip Intro", c.state.value.skipLabel)
        assertFalse(c.state.value.skipAlone)

        // Out of the intro and back in: 8 s by itself again.
        c.seekTo(16_000)
        h.advance(500)
        assertNull(c.state.value.skipLabel)
        c.seekTo(3_000)
        h.advance(500)
        assertEquals("Skip Intro", c.state.value.skipLabel)
        assertTrue(c.state.value.skipAlone)
        // The 8 s run on the wall clock, paused or not.
        c.pause()
        h.advance(8_500)
        assertEquals("Skip Intro", c.state.value.skipLabel)
        assertFalse(c.state.value.skipAlone)
        // Nothing skips by itself.
        assertEquals(listOf(12_000L, 16_000L, 3_000L), h.engine.seeks)
    }

    @Test
    fun `after Up Next is cancelled, Skip Credits shows by itself for 8 s from then`() {
        val c = episode(15_000)
        h.advance(2_000)
        assertNotNull(c.state.value.upNextCard)
        // Under the card, Skip's 8 s don't run.
        assertFalse(c.state.value.skipAlone)
        c.cancelUpNext()
        h.advance(500)
        assertNull(c.state.value.upNextCard)
        assertEquals("Skip Credits", c.state.value.skipLabel)
        assertTrue(c.state.value.skipAlone)
    }

    @Test
    fun `entering credits that run to the end offers Skip Credits and Up Next, once per segment`() {
        val c = episode(15_000)
        h.advance(1_500)
        val state = c.state.value
        assertEquals("Skip Credits", state.skipLabel)
        val card = state.upNextCard!!
        assertEquals(FixtureIds.SHOW_S1E2, card.itemId)
        assertEquals("Up next · S1 · E2", card.heading)
        assertEquals(card.title, state.upNext)
        assertEquals(10, state.upNextSecondsLeft)
        assertEquals(1, h.api.count("next"))

        // Seeking back hides the card; entering the same credits again doesn't bring it back.
        c.seekTo(10_000)
        h.advance(500)
        assertNull(c.state.value.upNextCard)
        c.seekTo(16_500)
        h.advance(1_000)
        assertEquals("Skip Credits", c.state.value.skipLabel)
        assertNull(c.state.value.upNextCard)
        assertEquals(1, h.api.count("next"))
    }

    /** Plays the first episode from [startMs] with its file [durationMs] long, as the item's files say; its credits stay at 16-20 s. */
    private fun episodeLasting(durationMs: Long, startMs: Long): SessionPlaybackController {
        h.editDetail = { d -> d.copy(files = d.files.map { it.copy(durationMs = durationMs) }) }
        return episode(startMs)
    }

    @Test
    fun `credits that something follows offer Skip Credits only, and Up Next comes at the end`() {
        // The credits end 60 s before the end of the file: an after-credits scene, say.
        val c = episodeLasting(80_000, startMs = 15_000)
        h.advance(1_500)
        assertEquals("Skip Credits", c.state.value.skipLabel)
        assertNull(c.state.value.upNextCard)
        assertEquals(0, h.api.count("next"))

        h.engine.end()
        h.settle()
        assertEquals(FixtureIds.SHOW_S1E2, c.state.value.upNextCard?.itemId)
        assertEquals(10, c.state.value.upNextSecondsLeft)
    }

    @Test
    fun `credits ending 10 s before the end bring Up Next`() {
        val c = episodeLasting(30_000, startMs = 15_000)
        h.advance(1_500)
        assertEquals("Skip Credits", c.state.value.skipLabel)
        assertEquals(FixtureIds.SHOW_S1E2, c.state.value.upNextCard?.itemId)
        assertEquals(10, c.state.value.upNextSecondsLeft)
    }

    @Test
    fun `credits of a file whose length nothing says offer Skip Credits only`() {
        // A download: no session on the server says how long it is either, nor the engine.
        val first = runBlocking { h.api.item(FixtureIds.SHOW_S1E1) }
        h.local = object : LocalMedia {
            override fun file(fileId: Long) = LocalFile("file:///downloads/e1/media.mkv").takeIf { fileId == first.files.first().id }
        }
        h.editDetail = { d -> d.copy(durationMs = null, files = d.files.map { it.copy(durationMs = null) }) }
        val c = h.play(PlayRequest(FixtureIds.SHOW_S1E1, startMs = 15_000))
        h.engine.ready()
        h.advance(1_500)
        assertEquals("Skip Credits", c.state.value.skipLabel)
        assertNull(c.state.value.upNextCard)
    }

    @Test
    fun `a download goes on to the next one downloaded when the server is away`() {
        val first = runBlocking { h.api.item(FixtureIds.SHOW_S1E1) }
        val second: ItemSummary = runBlocking { h.api.item(FixtureIds.SHOW_S1E2) }.toSummary()
        h.local = object : LocalMedia {
            override fun file(fileId: Long) = LocalFile("file:///downloads/e1/media.mkv").takeIf { fileId == first.files.first().id }

            override suspend fun nextAfter(itemId: Long) = second.takeIf { itemId == FixtureIds.SHOW_S1E1 }
        }
        h.api.unreachable += "next"
        val c = episode(19_000)
        assertEquals("file:///downloads/e1/media.mkv", h.engine.source?.url)

        h.engine.end()
        h.advance(500)

        val card = c.state.value.upNextCard!!
        assertEquals(FixtureIds.SHOW_S1E2, card.itemId)
        assertEquals(PlayerPhase.Paused, c.state.value.phase)
    }

    @Test
    fun `the countdown plays the next episode on the same engine, after the one before ended`() {
        val c = episode(15_000)
        h.advance(1_500)
        h.advance(9_000)
        assertEquals(1, c.state.value.upNextSecondsLeft)
        assertEquals(1, h.playbackRequests().size)
        h.advance(1_000)
        val requests = h.playbackRequests()
        assertEquals(listOf(FixtureIds.SHOW_S1E1, FixtureIds.SHOW_S1E2), requests.map { it.itemId })
        assertEquals(listOf("start", "progress stopped", "delete", "start"), h.timeline().filter { it != "progress playing" })
        assertEquals(2, h.engine.prepared.size)
        assertEquals(FixtureIds.SHOW_S1E2, c.state.value.itemId)
        assertEquals("Fixture Show", c.state.value.title)
        assertTrue(c.state.value.subtitle!!.startsWith("S1 · E2 · "))
        assertNull(c.state.value.upNextCard)
        // The next one starts where it resumes, unless watched.
        val next = runBlocking { h.api.item(FixtureIds.SHOW_S1E2) }.userState
        assertEquals(if (next.watched) 0L else next.positionMs, requests.last().startMs)
    }

    @Test
    fun `Play next starts it at once, and says so when there is nothing next`() {
        val c = episode(0)
        c.playNext()
        h.settle()
        assertEquals(FixtureIds.SHOW_S1E2, h.playbackRequests().last().itemId)
        c.close()
        h.settle()

        val last = runBlocking {
            var id = FixtureIds.SHOW_S2E1
            while (true) id = h.api.next(id).item?.id ?: break
            id
        }
        val end = h.play(PlayRequest(last))
        end.playNext()
        h.settle()
        assertEquals("This is the last episode", end.state.value.notice)
        end.dismissNotice()
        assertNull(end.state.value.notice)
    }

    @Test
    fun `without autoplay the card waits, and cancelling it at the end ends playback`() {
        h.close()
        h = SessionHarness(autoplay = false)
        val c = episode(15_500)
        h.advance(1_000)
        assertNotNull(c.state.value.upNextCard)
        assertNull(c.state.value.upNextSecondsLeft)
        h.engine.end()
        h.advance(15_000)
        assertEquals(1, h.playbackRequests().size)
        assertEquals(PlayerPhase.Paused, c.state.value.phase)
        assertNotNull(c.state.value.upNextCard)
        c.cancelUpNext()
        assertNull(c.state.value.upNextCard)
        assertEquals(PlayerPhase.Ended, c.state.value.phase)
    }

    @Test
    fun `a cancelled card comes back with its countdown at the end`() {
        val c = episode(15_500)
        h.advance(1_000)
        c.cancelUpNext()
        assertNull(c.state.value.upNextCard)
        h.engine.end()
        h.settle()
        assertEquals(10, c.state.value.upNextSecondsLeft)
        h.advance(10_000)
        assertEquals(FixtureIds.SHOW_S1E2, h.playbackRequests().last().itemId)
    }

    @Test
    fun `a playlist plays its next video`() {
        val entries = runBlocking { h.api.playlist(FixtureIds.PLAYLIST_VIDEO).entries.map { it.item.id } }
        val c = h.play(PlayRequest(entries[0], queue = PlayQueue.Playlist(FixtureIds.PLAYLIST_VIDEO)))
        h.engine.ready()
        h.engine.end()
        h.settle()
        assertEquals(entries[1], c.state.value.upNextCard?.itemId)
        assertEquals("Up next", c.state.value.upNextCard?.heading)
        c.playNext()
        h.settle()
        assertEquals(entries[1], h.playbackRequests().last().itemId)
    }
}
