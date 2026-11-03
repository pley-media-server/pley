package app.pley.android.data

import app.pley.android.model.DynamicRangeFilter
import app.pley.android.model.ItemKind
import app.pley.android.model.ItemSummary
import app.pley.android.model.ItemsPage
import app.pley.android.model.LibrarySort
import app.pley.android.model.ResolutionFilter
import app.pley.android.model.SortOrder
import app.pley.android.network.PleyApi
import app.pley.android.testing.data.TestData
import app.pley.android.testing.network.FakePleyApi
import java.util.Collections
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder

class LibraryItemsTest {
    @get:Rule
    val folder = TemporaryFolder()

    /** A library of [total] movies, ids 1 to [total], over the fixtures' API for the rest. */
    private class BigLibrary(val total: Int, base: FakePleyApi = FakePleyApi(TestData.ISSUER)) : PleyApi by base {
        val pages: MutableList<Pair<Int, Int?>> = Collections.synchronizedList(mutableListOf())

        override suspend fun libraryItems(
            libraryId: Long,
            sort: LibrarySort,
            order: SortOrder?,
            watched: Boolean?,
            genre: String?,
            year: Int?,
            resolution: ResolutionFilter?,
            dynamicRange: DynamicRangeFilter?,
            offset: Int,
            limit: Int?,
        ): ItemsPage {
            pages += offset to limit
            val end = (offset + (limit ?: total)).coerceAtMost(total)
            return ItemsPage(total, (offset until end).map { movie(it + 1L) })
        }

        fun movie(id: Long) = ItemSummary(id = id, libraryId = 1, kind = ItemKind.Movie, title = "Movie $id")
    }

    private suspend fun TestScope.library(total: Int, profile: app.pley.android.common.DeviceProfile = TestData.PHONE): Pair<TestData, BigLibrary> {
        val big = BigLibrary(total)
        val data = TestData(this, folder.newFolder(), profile = profile)
        data.apiOf[2] = big
        data.signIn()
        data.useServer()
        data.graph.session
        testScheduler.runCurrent()
        return data to big
    }

    private val query = LibraryQuery()

    @Test
    fun `the first 200 come first, then pages of 1,000 into one list`() = runTest {
        val (data, big) = library(3_500)
        val seen = collect(data.graph.libraryItems.items(1, query))
        testScheduler.runCurrent()

        val sizes = seen.mapNotNull { it.valueOrNull?.items?.size }
        assertEquals(listOf(200, 2_200, 3_500), sizes)
        assertEquals(listOf(0 to 200, 200 to 1_000, 1_200 to 1_000, 2_200 to 1_000, 3_200 to 1_000), big.pages.sortedBy { it.first })
        val last = seen.last().valueOrNull!!
        assertTrue(last.complete)
        assertEquals((1L..3_500L).toList(), last.items.map { it.id })
        assertEquals(LibraryItems(3_500, last.items), last)
    }

    @Test
    fun `a 50,000-item library stays a window of 2,000 around what the grid shows`() = runTest {
        val (data, big) = library(50_000)
        val seen = collect(data.graph.libraryItems.items(1, query))
        testScheduler.runCurrent()

        val first = seen.last().valueOrNull!!
        assertTrue(first.windowed)
        assertFalse(first.complete)
        assertEquals(50_000, first.total)
        assertEquals(0, first.offset)
        assertEquals(2_000, first.items.size)
        assertEquals(1L, first[0]!!.id)
        assertNull(first[30_000])

        data.graph.libraryItems.showing(1, query, 30_000)
        testScheduler.runCurrent()
        val moved = seen.last().valueOrNull!!
        assertEquals(30_000L + 1, moved[30_000]!!.id)
        assertTrue(moved.offset in 28_001..30_000)
        assertEquals(2_000, moved.items.size)

        data.graph.libraryItems.showing(1, query, 49_999)
        testScheduler.runCurrent()
        val end = seen.last().valueOrNull!!
        assertEquals(50_000L, end[49_999]!!.id)

        // Never more than one window in a list, and only the pages asked for.
        assertTrue(seen.all { (it.valueOrNull?.items?.size ?: 0) <= 2_000 })
        assertTrue(big.pages.size <= 1 + 6)
    }

    @Test
    fun `scrolling back reuses the pages read, within a few`() = runTest {
        val (data, big) = library(10_000)
        collect(data.graph.libraryItems.items(1, query))
        testScheduler.runCurrent()
        data.graph.libraryItems.showing(1, query, 5_200)
        testScheduler.runCurrent()
        val asked = big.pages.size
        data.graph.libraryItems.showing(1, query, 1_200)
        testScheduler.runCurrent()
        data.graph.libraryItems.showing(1, query, 5_200)
        testScheduler.runCurrent()
        // The head, pages 0 and 1, then 4 and 5: nothing asked again on the way back and forth.
        assertEquals(5, asked)
        assertEquals(asked, big.pages.size)
    }

    @Test
    fun `the smallest TVs window above 2,000 items, phones above 5,000`() = runTest {
        val (tv, _) = library(3_000, TestData.FIRE_TV_T0)
        val onTv = collect(tv.graph.libraryItems.items(1, query))
        testScheduler.runCurrent()
        assertTrue(onTv.last().valueOrNull!!.windowed)

        val (phone, _) = library(3_000)
        val onPhone = collect(phone.graph.libraryItems.items(1, query))
        testScheduler.runCurrent()
        assertTrue(onPhone.last().valueOrNull!!.complete)
    }

    @Test
    fun `the query goes to the server as the web sends it`() = runTest {
        val data = signedIn(folder.newFolder())
        val q = LibraryQuery(
            sort = LibrarySort.Added,
            order = SortOrder.Descending,
            unwatched = true,
            genre = "Drama",
            year = 2016,
            resolution = ResolutionFilter.Uhd,
            dynamicRange = DynamicRangeFilter.Hdr,
        )
        collect(data.graph.libraryItems.items(1, q))
        testScheduler.runCurrent()

        val call = data.api.calls("libraryItems").first()
        assertEquals(
            listOf<Any?>(1L, LibrarySort.Added, SortOrder.Descending, false, "Drama", 2016, ResolutionFilter.Uhd, DynamicRangeFilter.Hdr, 0, 200),
            call.args,
        )
    }

    @Test
    fun `the head of a listing is the snapshot of the next start`() = runTest {
        val data = signedIn(folder.newFolder())
        collect(data.graph.libraryItems.items(1, query))
        testScheduler.runCurrent()
        val file = snapshotFile(data.dir, ProfileScope(data.issuer, 2), SnapshotKeys.libraryHead(1, query))
        assertTrue(file.isFile)
        assertEquals("library:1:title.asc:head", SnapshotKeys.libraryHead(1, query))
        // Each filter is a listing of its own.
        val filtered = LibraryQuery(resolution = ResolutionFilter.Uhd, dynamicRange = DynamicRangeFilter.DolbyVision)
        assertEquals("library:1:title.asc.r=4k.d=dolby_vision:head", SnapshotKeys.libraryHead(1, filtered))
    }
}
