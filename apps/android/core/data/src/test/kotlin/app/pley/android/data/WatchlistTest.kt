package app.pley.android.data

import app.pley.android.model.Hub
import app.pley.android.model.HubKeys
import app.pley.android.model.Hubs
import app.pley.android.model.ItemKind
import app.pley.android.model.ItemSummary
import app.pley.android.model.ServerEvent
import app.pley.android.network.ApiException
import app.pley.android.network.LiveEvent
import app.pley.android.testing.Fixtures
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder

/** Leaving Continue Watching, and My List: shown at once, put back when refused. */
class WatchlistTest {
    @get:Rule
    val folder = TemporaryFolder()

    private val refused = ApiException(500, "internal", "the server failed")

    private fun Hubs.keys(): List<String> = hubs.map { it.key }

    private fun Hubs.row(key: String): Hub? = hubs.firstOrNull { it.key == key }

    @Test
    fun `an episode leaves Continue Watching with its show, on Home and in the libraries, at once`() = runTest {
        val data = signedIn(folder.newFolder())
        val home = collect(data.graph.hubs.home())
        val library = collect(data.graph.hubs.library(1))
        testScheduler.runCurrent()

        data.api.hold()
        val change = launch { data.graph.watchState.hideFromContinueWatching(204) }
        testScheduler.runCurrent()

        // Before the server answers: Half Loop is out, Arrival stays, the show's other rows too.
        val shown = home.last().valueOrNull!!
        assertEquals(listOf(101L), shown.row(HubKeys.CONTINUE_WATCHING)!!.items.map { it.id })
        assertTrue(shown.row("recently_added:2")!!.items.any { it.id == 205L })

        data.api.release()
        testScheduler.runCurrent()
        change.join()
        assertEquals(listOf<Any?>(204L), data.api.calls("hideFromContinueWatching").single().args)
        // Then Home and the libraries' rows are asked again.
        assertEquals(2, data.api.calls("homeHubs").size)
        assertEquals(2, data.api.calls("libraryHubs").size)
        assertTrue(library.isNotEmpty())
    }

    @Test
    fun `a movie leaving the libraries' Continue Watching takes the row with it when it was the last`() = runTest {
        val data = signedIn(folder.newFolder())
        val library = collect(data.graph.hubs.library(1))
        testScheduler.runCurrent()
        assertTrue(library.last().valueOrNull!!.row("continue_watching:1") != null)
        data.api.hold()

        launch { data.graph.watchState.hideFromContinueWatching(101) }
        testScheduler.runCurrent()

        assertNull(library.last().valueOrNull!!.row("continue_watching:1"))
        data.api.release()
    }

    @Test
    fun `a refused removal is put back`() = runTest {
        val data = signedIn(folder.newFolder())
        val home = collect(data.graph.hubs.home())
        testScheduler.runCurrent()
        val before = home.last().valueOrNull
        data.api.fail("hideFromContinueWatching", refused)

        try {
            data.graph.watchState.hideFromContinueWatching(101)
            fail("refused")
        } catch (e: ApiException) {
            assertSame(refused, e)
        }
        testScheduler.runCurrent()

        assertEquals(before, home.last().valueOrNull)
    }

    @Test
    fun `added to My List, the page, the list and Home show it at once`() = runTest {
        val data = signedIn(folder.newFolder())
        val home = collect(data.graph.hubs.home())
        val detail = collect(data.graph.items.detail(101))
        val list = collect(data.graph.watchlist.watchlist())
        testScheduler.runCurrent()
        assertFalse(detail.last().valueOrNull!!.inWatchlist)
        assertNull(home.last().valueOrNull!!.row(HubKeys.MY_LIST))

        data.api.hold()
        val change = launch { data.graph.watchlist.add(101) }
        testScheduler.runCurrent()

        assertTrue(detail.last().valueOrNull!!.inWatchlist)
        assertEquals(101L, list.last().valueOrNull!!.first().id)
        // Home gets My List, after Continue Watching.
        val keys = home.last().valueOrNull!!.keys()
        assertEquals(listOf(HubKeys.CONTINUE_WATCHING, HubKeys.MY_LIST), keys.take(2))
        assertEquals(listOf(101L), home.last().valueOrNull!!.row(HubKeys.MY_LIST)!!.items.map { it.id })

        data.api.release()
        testScheduler.runCurrent()
        change.join()
        assertEquals(listOf<Any?>(101L), data.api.calls("addToWatchlist").single().args)
        assertEquals(2, data.api.calls("watchlist").size)
        assertEquals(2, data.api.calls("homeHubs").size)
    }

    @Test
    fun `removed from My List, it leaves the list and Home's row`() = runTest {
        val data = signedIn(folder.newFolder())
        val list = collect(data.graph.watchlist.watchlist())
        testScheduler.runCurrent()
        assertEquals(listOf(201L, 104L), list.last().valueOrNull!!.map { it.id })
        data.api.hold()

        launch { data.graph.watchlist.remove(201) }
        testScheduler.runCurrent()

        assertEquals(listOf(104L), list.last().valueOrNull!!.map { it.id })
        data.api.release()
        testScheduler.runCurrent()
        assertEquals(listOf<Any?>(201L), data.api.calls("removeFromWatchlist").single().args)
    }

    @Test
    fun `a refused add is put back`() = runTest {
        val data = signedIn(folder.newFolder())
        val home = collect(data.graph.hubs.home())
        val detail = collect(data.graph.items.detail(101))
        testScheduler.runCurrent()
        val homeBefore = home.last().valueOrNull
        data.api.fail("addToWatchlist", refused)

        runCatching { data.graph.watchlist.add(101) }
        testScheduler.runCurrent()

        assertFalse(detail.last().valueOrNull!!.inWatchlist)
        assertEquals(homeBefore, home.last().valueOrNull)
    }

    @Test
    fun `My List changed on another device, the list, Home and the pages are asked again`() = runTest {
        val data = signedIn(folder.newFolder())
        collect(data.graph.hubs.home())
        collect(data.graph.watchlist.watchlist())
        collect(data.graph.items.detail(101))
        data.graph.liveUpdates.start()
        testScheduler.runCurrent()
        val home = data.api.calls("homeHubs").size
        val list = data.api.calls("watchlist").size
        val item = data.api.calls("item").size

        data.events.send(LiveEvent.Event(ServerEvent.WatchlistChanged))
        testScheduler.runCurrent()

        assertEquals(home + 1, data.api.calls("homeHubs").size)
        assertEquals(list + 1, data.api.calls("watchlist").size)
        assertEquals(item + 1, data.api.calls("item").size)
    }

    @Test
    fun `Home's My List puts the newest first, comes after Continue Watching and goes when empty`() {
        val home: Hubs = Fixtures.decode("hubs/home.json")
        val dune = home.hubs.flatMap { it.items }.first { it.id == 104L }
        val show = ItemSummary(id = 201, libraryId = 2, kind = ItemKind.Show, title = "Severance")

        val one = ItemPatch.myList(home, 104, dune)!!
        assertEquals(listOf(HubKeys.CONTINUE_WATCHING, HubKeys.MY_LIST, "recently_added:1"), one.keys().take(3))
        assertEquals("My List", one.row(HubKeys.MY_LIST)!!.title)
        // In the language shown, until the server titles it.
        assertEquals("Ma liste", ItemPatch.myList(home, 104, dune, "Ma liste")!!.row(HubKeys.MY_LIST)!!.title)
        val two = ItemPatch.myList(one, 201, show)!!
        assertEquals(listOf(201L, 104L), two.row(HubKeys.MY_LIST)!!.items.map { it.id })
        // Already first: nothing changes.
        assertNull(ItemPatch.myList(two, 201, show))

        val left = ItemPatch.myList(two, 201, null)!!
        assertEquals(listOf(104L), left.row(HubKeys.MY_LIST)!!.items.map { it.id })
        assertNull(ItemPatch.myList(left, 201, null))
        assertNull(ItemPatch.myList(left, 104, null)!!.row(HubKeys.MY_LIST))
    }
}
