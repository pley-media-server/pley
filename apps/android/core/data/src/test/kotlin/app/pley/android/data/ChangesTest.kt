package app.pley.android.data

import app.pley.android.model.EntryIds
import app.pley.android.model.HistoryPage
import app.pley.android.model.NewPlaylist
import app.pley.android.model.PlaylistDetail
import app.pley.android.model.PlaylistKind
import app.pley.android.model.PlaylistPatch
import app.pley.android.model.SearchResults
import app.pley.android.model.SubtitleMode
import app.pley.android.model.TrackDefaults
import app.pley.android.network.ApiException
import app.pley.android.testing.Fixtures
import app.pley.android.testing.data.TestData
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder

class ChangesTest {
    @get:Rule
    val folder = TemporaryFolder()

    private val refused = ApiException(500, "internal", "the server failed")
    private val detail: PlaylistDetail = Fixtures.decode("playlists/detail.json")

    // ---------------------------------------------------------------- playlists

    @Test
    fun `a rename shows at once in the list and the page`() = runTest {
        val data = signedIn(folder.newFolder())
        val list = collect(data.graph.playlists.playlists())
        val page = collect(data.graph.playlists.playlist(detail.id))
        testScheduler.runCurrent()

        data.api.hold()
        val change = launch { data.graph.playlists.rename(detail.id, "Weekend") }
        testScheduler.runCurrent()
        assertEquals("Weekend", page.last().valueOrNull!!.title)
        assertEquals("Weekend", list.last().valueOrNull!!.first { it.id == detail.id }.title)
        data.api.release()
        change.join()
        assertEquals(listOf<Any?>(detail.id, PlaylistPatch(title = "Weekend")), data.api.calls("updatePlaylist").single().args)
    }

    @Test
    fun `a refused rename is put back`() = runTest {
        val data = signedIn(folder.newFolder())
        val list = collect(data.graph.playlists.playlists())
        val page = collect(data.graph.playlists.playlist(detail.id))
        testScheduler.runCurrent()
        data.api.fail("updatePlaylist", refused)

        try {
            data.graph.playlists.rename(detail.id, "Weekend")
            fail("refused")
        } catch (_: ApiException) {
        }

        assertEquals(detail.title, page.last().valueOrNull!!.title)
        assertEquals(detail.title, list.last().valueOrNull!!.first { it.id == detail.id }.title)
    }

    @Test
    fun `entries move at once, and the server's order is kept`() = runTest {
        val data = signedIn(folder.newFolder())
        val page = collect(data.graph.playlists.playlist(detail.id))
        testScheduler.runCurrent()
        val ids = detail.entries.map { it.entryId }
        val wanted = listOf(ids.last())

        data.graph.playlists.reorder(detail.id, wanted)

        assertEquals(listOf(ids.last()) + ids.dropLast(1), page.last().valueOrNull!!.entries.map { it.entryId })
        assertEquals(listOf<Any?>(detail.id, EntryIds(wanted)), data.api.calls("reorderPlaylist").single().args)
    }

    @Test
    fun `a refused move is put back`() = runTest {
        val data = signedIn(folder.newFolder())
        val page = collect(data.graph.playlists.playlist(detail.id))
        testScheduler.runCurrent()
        data.api.fail("reorderPlaylist", refused)

        runCatching { data.graph.playlists.reorder(detail.id, listOf(detail.entries.last().entryId)) }

        assertEquals(detail.entries, page.last().valueOrNull!!.entries)
    }

    @Test
    fun `removed entries go at once, and come back if refused`() = runTest {
        val data = signedIn(folder.newFolder())
        val page = collect(data.graph.playlists.playlist(detail.id))
        testScheduler.runCurrent()
        val gone = detail.entries.first().entryId

        data.api.hold()
        val change = launch { runCatching { data.graph.playlists.remove(detail.id, listOf(gone)) } }
        testScheduler.runCurrent()
        assertTrue(page.last().valueOrNull!!.entries.none { it.entryId == gone })
        assertEquals(detail.itemCount - 1, page.last().valueOrNull!!.itemCount)

        data.api.fail("removeFromPlaylist", refused)
        data.api.release()
        change.join()
        testScheduler.runCurrent()
        assertTrue(page.last().valueOrNull!!.entries.any { it.entryId == gone })
    }

    @Test
    fun `a deleted playlist leaves the list at once`() = runTest {
        val data = signedIn(folder.newFolder())
        val list = collect(data.graph.playlists.playlists())
        testScheduler.runCurrent()

        data.graph.playlists.delete(detail.id)

        assertTrue(list.last().valueOrNull!!.none { it.id == detail.id })
    }

    @Test
    fun `new playlists, of items or smart`() = runTest {
        val data = signedIn(folder.newFolder())
        val list = collect(data.graph.playlists.playlists())
        testScheduler.runCurrent()

        val id = data.graph.playlists.create("Road trip", PlaylistKind.Audio, listOf(303))
        data.graph.playlists.createSmart("Sci-fi", 1, "genre=Science%20Fiction")
        testScheduler.runCurrent()

        assertEquals(detail.id, id)
        val bodies = data.api.calls("createPlaylist").map { it.args.single() }
        assertEquals(NewPlaylist(title = "Road trip", kind = PlaylistKind.Audio, itemIds = listOf(303)), bodies[0])
        assertEquals(NewPlaylist(title = "Sci-fi", smartFilter = "genre=Science%20Fiction", libraryId = 1), bodies[1])
        assertTrue(list.last() is Loadable.Ready)
    }

    @Test
    fun `reordering keeps the entries not named after the named ones`() {
        val ids = detail.entries.map { it.entryId }
        val moved = CachedPlaylists.reordered(detail, listOf(ids[2], ids[0], 999))
        assertEquals(listOf(ids[2], ids[0]) + ids.filterNot { it == ids[2] || it == ids[0] }, moved.map { it.entryId })
    }

    // ---------------------------------------------------------------- history

    @Test
    fun `history loads more pages into one list`() = runTest {
        val page: HistoryPage = Fixtures.decode("history/page.json")
        val data = signedIn(folder.newFolder())
        val seen = collect(data.graph.history.history())
        testScheduler.runCurrent()
        val more = HistoryPage(total = page.total, entries = page.entries.map { it.copy(id = it.id + 10_000) })
        data.api.answer("history", more)

        data.graph.history.loadMore()

        assertEquals(page.entries.size + more.entries.size, seen.last().valueOrNull!!.entries.size)
        assertEquals(listOf<Any?>(page.entries.size, CachedHistory.PAGE, null, null, null), data.api.calls("history").last().args)
    }

    @Test
    fun `a removed play goes at once, and comes back if refused`() = runTest {
        val data = signedIn(folder.newFolder())
        val seen = collect(data.graph.history.history())
        testScheduler.runCurrent()
        val before = seen.last().valueOrNull!!
        val entry = before.entries.first().id

        data.graph.history.remove(entry)
        assertTrue(seen.last().valueOrNull!!.entries.none { it.id == entry })
        assertEquals(before.total - 1, seen.last().valueOrNull!!.total)

        val next = seen.last().valueOrNull!!.entries.first().id
        data.api.fail("deleteHistoryEntry", refused)
        runCatching { data.graph.history.remove(next) }
        assertTrue(seen.last().valueOrNull!!.entries.any { it.id == next })
    }

    // ---------------------------------------------------------------- search

    @Test
    fun `only the latest search is asked to the end`() = runTest {
        val data = signedIn(folder.newFolder())
        data.api.hold()
        val older = collect(data.graph.search.search("arr"))
        testScheduler.runCurrent()
        val newer = collect(data.graph.search.search("arri"))
        testScheduler.runCurrent()
        data.api.release()
        testScheduler.runCurrent()

        val results: SearchResults = Fixtures.decode("search/results.json")
        assertEquals(listOf<Loadable<SearchResults>>(Loadable.Loading), older)
        assertEquals(Loadable.Ready(results, fresh = true), newer.last())
        assertEquals(listOf("arr", "arri"), data.api.calls("search").map { it.args.first() })
        // Every result there is, up to the server's most: titles, people and channels each.
        assertEquals(listOf(200, 200), data.api.calls("search").map { it.args[1] })
    }

    @Test
    fun `an empty query asks nothing`() = runTest {
        val data = signedIn(folder.newFolder())
        assertEquals(Loadable.Ready(SearchResults(), fresh = true), data.graph.search.search("  ").first())
        assertTrue(data.api.calls("search").isEmpty())
    }

    @Test
    fun `recent searches, newest first, ten at most, per profile`() = runTest {
        val dir = folder.newFolder()
        val first = appRun()
        val before = signedIn(dir, first)
        val search = before.graph.search
        for (q in 1..12) search.remember("query $q")
        search.remember("QUERY 5")
        search.remember("   ")
        val recent = search.recent.first()
        assertEquals(10, recent.size)
        assertEquals("QUERY 5", recent.first())
        assertEquals(listOf("QUERY 5", "query 12", "query 11"), recent.take(3))
        assertEquals(1, recent.count { it.equals("query 5", ignoreCase = true) })
        first.cancel()
        testScheduler.runCurrent()

        // Kept on this device for this profile.
        val after = TestData(this, dir)
        after.signIn()
        assertEquals(recent, after.graph.search.recent.first { it.isNotEmpty() })
        after.graph.search.clearRecent()
        assertTrue(after.graph.search.recent.first().isEmpty())
    }

    // ---------------------------------------------------------------- track defaults

    @Test
    fun `track defaults come with the profile and save at once`() = runTest {
        val data = signedIn(folder.newFolder())
        val seen = collect(data.graph.trackDefaults.defaults())
        testScheduler.runCurrent()
        assertEquals(TrackDefaults("eng", null, SubtitleMode.Foreign), seen.last().valueOrNull)

        val wanted = TrackDefaults("fra", "eng", SubtitleMode.Always)
        data.graph.trackDefaults.save(wanted)

        assertEquals(wanted, seen.last().valueOrNull)
        assertEquals(listOf(data.issuer to wanted), data.trackDefaults.calls)
        assertEquals("fra", data.graph.session.session.value!!.me!!.audioLanguage)
    }

    @Test
    fun `servers without the route are read-only, and nothing changes`() = runTest {
        val data = signedIn(folder.newFolder())
        val seen = collect(data.graph.trackDefaults.defaults())
        testScheduler.runCurrent()
        val before = seen.last().valueOrNull
        data.trackDefaults.failure = ApiException(404, "not_found", "no such route")

        try {
            data.graph.trackDefaults.save(TrackDefaults("fra", null, SubtitleMode.Off))
            fail("unsupported")
        } catch (_: TrackDefaultsUnsupported) {
        }

        assertEquals(before, seen.last().valueOrNull)
        assertFalse(data.graph.session.session.value!!.me!!.audioLanguage == "fra")
    }
}
