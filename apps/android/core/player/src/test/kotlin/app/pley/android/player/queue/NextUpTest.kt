package app.pley.android.player.queue

import app.pley.android.data.LocalMedia
import app.pley.android.model.ItemKind
import app.pley.android.model.ItemSummary
import app.pley.android.model.MaybeItem
import app.pley.android.player.PlayQueue
import app.pley.android.testing.network.FakePleyApi
import java.io.IOException
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.currentTime
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/** What an episode goes on to, from the server or, with the server away, from downloads. */
@OptIn(ExperimentalCoroutinesApi::class)
class NextUpTest {
    private val api = FakePleyApi()
    private val fromServer = ItemSummary(id = 12, libraryId = 1, kind = ItemKind.Episode, title = "From the server")
    private val downloaded = ItemSummary(id = 13, libraryId = 1, kind = ItemKind.Episode, title = "Downloaded", thumbUrl = "file:///still.jpg")
    private val local = object : LocalMedia {
        override suspend fun nextAfter(itemId: Long): ItemSummary? = downloaded.takeIf { itemId == 11L }
    }
    private val nextUp = NextUp(api, local)

    @Test
    fun theServerSaysWhatComesNextWhenItAnswers() = runTest {
        api.answer("next", MaybeItem(fromServer))
        assertEquals(fromServer, nextUp.after(11, ItemKind.Episode, queue = null, fromDownload = true))
        api.answer("next", MaybeItem(null))
        assertNull("the server's last episode is the last", nextUp.after(11, ItemKind.Episode, queue = null, fromDownload = true))
    }

    @Test
    fun aDownloadGoesOnToTheNextDownloadedWithTheServerAway() = runTest {
        api.fail("next", IOException("unreachable"))
        assertEquals(downloaded, nextUp.after(11, ItemKind.Episode, queue = null, fromDownload = true))
    }

    @Test
    fun aServerThatDoesntAnswerIsWaitedForBriefly() = runTest {
        api.hold()
        assertEquals(downloaded, nextUp.after(11, ItemKind.Episode, queue = null, fromDownload = true))
        assertEquals(NextUp.SERVER_WAIT_MS, currentTime)
        api.release()
    }

    @Test
    fun aMergedRowPlaysThroughItsTitlesInTheRowsOrder() = runTest {
        // collections/merged.json: Dune, then Dune: Part Two, from two libraries' collections.
        val row = PlayQueue.MergedCollection(listOf(801, 901))
        assertEquals(104L, nextUp.after(105, ItemKind.Movie, row)?.id)
        assertNull("the row's last title is the last", nextUp.after(104, ItemKind.Movie, row))
    }

    @Test
    fun streamedEpisodesAskTheServerOnly() = runTest {
        api.fail("next", IOException("unreachable"))
        val failed = runCatching { nextUp.after(11, ItemKind.Episode, queue = null) }
        assertEquals(IOException::class, failed.exceptionOrNull()?.let { it::class })
        assertNull("movies have no next", nextUp.after(11, ItemKind.Movie, queue = null, fromDownload = true))
    }
}
