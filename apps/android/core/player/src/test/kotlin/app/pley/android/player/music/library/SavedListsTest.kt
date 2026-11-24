@file:OptIn(ExperimentalCoroutinesApi::class)

package app.pley.android.player.music.library

import android.net.Uri
import androidx.media3.common.MediaItem
import androidx.media3.common.MediaMetadata
import app.pley.android.data.Loadable
import java.io.IOException
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.withContext
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

/**
 * A car's list shown from what was saved, the server slow: the car is told to
 * read it again once the server's answer turns out different, and only then.
 */
@RunWith(RobolectricTestRunner::class)
class SavedListsTest {
    /** The source's answer for every list: saved, then the server's. */
    private val answer = MutableStateFlow<Loadable<List<String>>>(Loadable.Ready(listOf("Deep Water", "Second Tide"), fresh = false))

    /** The lists the cars were told to read again. */
    private val told = mutableListOf<String>()

    /** Reads a page as the library does, through the source's answer. */
    private suspend fun read(parentId: String, page: Int): List<MediaItem> =
        answer.freshOrSaved().drop(page * PAGE).take(PAGE).mapIndexed { i, title -> row("$parentId/${page * PAGE + i}", title) }

    private fun TestScope.lists() = SavedLists(backgroundScope, { parentId, page, _ -> read(parentId, page) }) { told += it }

    /** Page [page] of [parentId] as a car is given it, and whether any of it was what was saved. */
    private suspend fun TestScope.shown(lists: SavedLists, parentId: String, page: Int = 0): Boolean {
        val saved = SavedAnswers()
        val rows = withContext(saved) { read(parentId, page) }
        lists.shown(parentId, page, PAGE, rows, saved)
        runCurrent()
        return saved.any
    }

    private fun TestScope.serverAnswers(value: Loadable<List<String>>) {
        answer.value = value
        advanceTimeBy(1_000)
        runCurrent()
    }

    @Test
    fun aListShownFromWhatWasSavedIsReadAgainWhenTheServersDiffers() = runTest {
        val lists = lists()
        assertTrue(shown(lists, "artist:40"))
        assertEquals(emptyList<String>(), told)

        serverAnswers(Loadable.Ready(listOf("Deep Water", "Third Wave"), fresh = true))
        assertEquals(listOf("artist:40"), told)
    }

    @Test
    fun theSameAnswerTellsNothing() = runTest {
        val lists = lists()
        assertTrue(shown(lists, "artist:40"))
        serverAnswers(Loadable.Ready(listOf("Deep Water", "Second Tide"), fresh = true))
        assertEquals(emptyList<String>(), told)
    }

    @Test
    fun theServersAnswerInTimeIsWhatIsShownAndNothingFollows() = runTest {
        answer.value = Loadable.Ready(listOf("Deep Water"), fresh = true)
        val lists = lists()
        assertFalse(shown(lists, "artist:40"))
        serverAnswers(Loadable.Ready(listOf("Third Wave"), fresh = true))
        assertEquals(emptyList<String>(), told)
    }

    @Test
    fun aServerThatFailsLeavesWhatIsShown() = runTest {
        val lists = lists()
        assertTrue(shown(lists, "artist:40"))
        serverAnswers(Loadable.Failed(IOException("Out of reach"), stale = listOf("Deep Water", "Second Tide")))
        assertEquals(emptyList<String>(), told)
    }

    @Test
    fun theChangedPagesOfOneListAreToldOnce() = runTest {
        answer.value = Loadable.Ready(listOf("A", "B", "C", "D"), fresh = false)
        val lists = lists()
        assertTrue(shown(lists, "playlists", page = 0))
        assertTrue(shown(lists, "playlists", page = 1))
        assertTrue(shown(lists, "recent"))
        serverAnswers(Loadable.Ready(listOf("A", "B2", "C", "D2"), fresh = true))
        assertEquals(listOf("playlists", "recent"), told.sorted())
    }

    @Test
    fun aRowPicturedOtherwiseIsAChange() = runTest {
        val lists = SavedLists(backgroundScope, { parentId, _, _ -> read(parentId, 0).map { it.pictured("new") } }) { told += it }
        val saved = SavedAnswers()
        val rows = withContext(saved) { read("recent", 0).map { it.pictured("old") } }
        lists.shown("recent", 0, PAGE, rows, saved)
        serverAnswers(Loadable.Ready(listOf("Deep Water", "Second Tide"), fresh = true))
        assertEquals(listOf("recent"), told)
    }

    private fun row(id: String, title: String): MediaItem =
        MediaItem.Builder().setMediaId(id).setMediaMetadata(MediaMetadata.Builder().setTitle(title).build()).build()

    private fun MediaItem.pictured(version: String): MediaItem =
        buildUpon().setMediaMetadata(mediaMetadata.buildUpon().setArtworkUri(Uri.parse("content://art/$mediaId?v=$version")).build()).build()

    private companion object {
        const val PAGE = 2
    }
}
