@file:OptIn(ExperimentalCoroutinesApi::class)

package app.pley.android.data

import app.pley.android.model.ItemDetail
import app.pley.android.model.ItemKind
import app.pley.android.model.ItemSummary
import app.pley.android.network.OfflineException
import java.io.IOException
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.cancel
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder

/**
 * An item's page got ready from a listing ([ItemRepository.seed]): what
 * the listing says stands in for its details until its snapshot or the
 * server's answer comes, and the server is asked at once.
 */
class SeededItemsTest {
    @get:Rule
    val folder = TemporaryFolder()

    private val season = ItemSummary(id = 202, libraryId = 2, kind = ItemKind.Season, title = "Season 1", index = 1, parentId = 201, parentTitle = "Severance", leafCount = 3)
    private val episode = ItemSummary(id = 204, libraryId = 2, kind = ItemKind.Episode, title = "Half Loop", index = 2, parentId = 202, parentIndex = 1)
    private val standIn = Loadable.Ready(ItemDetail.of(season), fresh = false)

    @Test
    fun `a seeded season shows at once and is asked for, then the server's details show`() = runTest {
        val data = signedIn(folder.newFolder())
        data.api.hold()
        data.graph.items.seed(season, episode)
        testScheduler.runCurrent()
        // Asked for as it is seeded, before any page shows it.
        assertEquals(listOf(202L), data.api.calls("item").map { it.args.single() })
        assertEquals(listOf(202L), data.api.calls("onDeck").map { it.args.single() })

        val details = collect(data.graph.items.detail(202))
        val next = collect(data.graph.items.onDeck(202))
        testScheduler.runCurrent()
        assertEquals(listOf<Loadable<ItemDetail>>(standIn), details)
        assertEquals(listOf<Loadable<ItemSummary?>>(Loadable.Ready(episode, fresh = false)), next)

        data.api.release()
        testScheduler.runCurrent()
        val answer = details.last() as Loadable.Ready<ItemDetail>
        assertTrue(answer.fresh)
        assertEquals(202L, answer.value.id)
        assertTrue((next.last() as Loadable.Ready).fresh)
        // One request each: the page's collecting shares the one made as it was seeded.
        assertEquals(1, data.api.calls("item").size)
        assertEquals(1, data.api.calls("onDeck").size)
    }

    @Test
    fun `a seed replaces nothing that is held`() = runTest {
        val data = signedIn(folder.newFolder())
        val details = collect(data.graph.items.detail(202))
        testScheduler.runCurrent()
        val held = details.last()
        assertTrue(held is Loadable.Ready && held.fresh)

        data.graph.items.seed(season)
        testScheduler.runCurrent()
        assertEquals(held, details.last())
        // Fresh, so not asked for again either.
        assertEquals(1, data.api.calls("item").size)
    }

    @Test
    fun `the snapshot of an earlier visit takes a seed's place, and a seed is never saved`() = runTest {
        val dir = folder.newFolder()
        // A first run: the season's page is shown, and saved.
        val first = appRun()
        val before = signedIn(dir, first)
        val saved = collect(before.graph.items.detail(202), first)
        testScheduler.runCurrent()
        val answer = (saved.last() as Loadable.Ready<ItemDetail>).value
        first.cancel()
        testScheduler.runCurrent()
        val file = snapshotFile(dir, ProfileScope(before.issuer, 2), SnapshotKeys.item(202))
        assertTrue(file.isFile)
        val written = file.readBytes()

        // The next: seeded while the server holds its answers. The snapshot says more than the listing.
        val after = signedIn(dir)
        after.api.hold()
        after.graph.items.seed(season.copy(title = "From the listing"))
        val details = collect(after.graph.items.detail(202))
        testScheduler.runCurrent()
        assertEquals(Loadable.Ready(answer, fresh = false), details.last())
        assertTrue(written.contentEquals(file.readBytes()))
    }

    @Test
    fun `a seed stays shown when the server can't be reached, and nothing is saved of it`() = runTest {
        val dir = folder.newFolder()
        val data = signedIn(dir)
        val offline = OfflineException(IOException("no route"))
        data.api.fail("item", offline)
        data.graph.items.seed(season)
        val details = collect(data.graph.items.detail(202))
        testScheduler.runCurrent()
        assertEquals(Loadable.Failed(offline, stale = standIn.value), details.last())
        assertFalse(snapshotFile(dir, ProfileScope(data.issuer, 2), SnapshotKeys.item(202)).exists())
    }
}
