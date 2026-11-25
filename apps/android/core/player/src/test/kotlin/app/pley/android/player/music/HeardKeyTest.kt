package app.pley.android.player.music

import app.pley.android.model.ItemKind
import app.pley.android.model.ItemSummary
import app.pley.android.player.MusicQueueState
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertNull
import org.junit.Test

/** When a car's Recent and Books are read again: a new queue playing, a book pausing or going on to its next part. */
class HeardKeyTest {
    private val album = MusicQueueState(tracks = (1..3L).map { track(it, ItemKind.Track) })
    private val book = MusicQueueState(tracks = (11..12L).map { track(it, ItemKind.Part) })

    @Test
    fun aNewQueuePlayingChangesItAndSkippingOrPausingDoesNot() {
        val playing = heardKey(album, playing = true)
        // Paused, nothing changes: the key of the queue playing stays the last one.
        assertNull(heardKey(album, playing = false))
        assertEquals(playing, heardKey(album.copy(index = 2), playing = true))
        // Shuffled, the same tracks.
        assertEquals(playing, heardKey(album.copy(tracks = album.tracks.reversed()), playing = true))
        assertNotEquals(playing, heardKey(album.copy(tracks = album.tracks.drop(1)), playing = true))
        assertNull(heardKey(MusicQueueState(), playing = true))
    }

    @Test
    fun aBookChangesItAsItPausesAndAtItsNextPart() {
        val playing = heardKey(book, playing = true)
        assertNotEquals(playing, heardKey(book, playing = false))
        assertNotEquals(playing, heardKey(book.copy(index = 1), playing = true))
        assertEquals(playing, heardKey(book, playing = true))
    }

    private fun track(id: Long, kind: ItemKind) = ItemSummary(id, 3, kind, "Track $id", parentId = if (kind == ItemKind.Part) 10 else 50)
}
