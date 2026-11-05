@file:OptIn(ExperimentalCoroutinesApi::class)

package app.pley.android.data

import app.pley.android.model.Hub
import app.pley.android.model.Hubs
import app.pley.android.model.ItemDetail
import app.pley.android.network.OfflineException
import app.pley.android.testing.Fixtures
import app.pley.android.testing.data.TestData
import java.io.IOException
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder

class SnapshotFirstTest {
    @get:Rule
    val folder = TemporaryFolder()

    private val home: Hubs = Fixtures.decode("hubs/home.json")

    @Test
    fun `Home shows the snapshot at once, then the server's answer`() = runTest {
        val dir = folder.newFolder()
        // The first run: the server answers, and Home is saved.
        val first = appRun()
        val before = signedIn(dir, first)
        val firstSeen = collect(before.graph.hubs.home(), first)
        testScheduler.runCurrent()
        assertEquals(Loadable.Ready(home, fresh = true), firstSeen.last())
        first.cancel()
        testScheduler.runCurrent()

        // The next start: the server holds every answer, GET /api/auth/me included.
        val after = TestData(this, dir)
        after.signIn()
        after.api.hold()
        val seen = collect(after.graph.hubs.home())
        testScheduler.runCurrent()
        assertEquals(listOf(Loadable.Loading, Loadable.Ready(home, fresh = false)), seen)

        after.api.release()
        testScheduler.runCurrent()
        assertEquals(listOf(Loadable.Loading, Loadable.Ready(home, fresh = false), Loadable.Ready(home, fresh = true)), seen)
    }

    @Test
    fun `the server's answer is saved, and an unchanged one isn't written again`() = runTest {
        val dir = folder.newFolder()
        val data = signedIn(dir)
        val job = launch { data.graph.hubs.home().first { it is Loadable.Ready && it.fresh } }
        testScheduler.runCurrent()
        job.join()
        val file = snapshotFile(dir, ProfileScope(data.issuer, 2), SnapshotKeys.HOME)
        assertTrue(file.isFile)
        val written = file.lastModified()
        file.setLastModified(1_000)

        data.graph.hubs.refresh()
        testScheduler.runCurrent()
        assertEquals(1_000, file.lastModified())
        assertTrue(written > 0)
    }

    @Test
    fun `screens showing the same answer share one request`() = runTest {
        val data = signedIn(folder.newFolder())
        data.api.hold()
        val a = collect(data.graph.hubs.home())
        val b = collect(data.graph.hubs.home())
        testScheduler.runCurrent()
        data.api.release()
        testScheduler.runCurrent()

        assertEquals(1, data.api.calls("homeHubs").size)
        assertEquals(Loadable.Ready(home, fresh = true), a.last())
        assertEquals(Loadable.Ready(home, fresh = true), b.last())
    }

    @Test
    fun `a failure keeps what was shown`() = runTest {
        val data = signedIn(folder.newFolder())
        val seen = collect(data.graph.hubs.home())
        testScheduler.runCurrent()
        val offline = OfflineException(IOException("no route"))
        data.api.fail("homeHubs", offline)

        data.graph.hubs.refresh()
        testScheduler.runCurrent()

        assertEquals(Loadable.Failed(offline, stale = home), seen.last())
    }

    @Test
    fun `an answer shown again is asked again only when 30 seconds old`() = runTest {
        val data = signedIn(folder.newFolder())
        val first = launch { data.graph.items.detail(101).collect {} }
        testScheduler.runCurrent()
        first.cancel()
        assertEquals(1, data.api.calls("item").size)

        advanceTimeBy(10_000)
        val again = launch { data.graph.items.detail(101).collect {} }
        testScheduler.runCurrent()
        again.cancel()
        assertEquals(1, data.api.calls("item").size)

        advanceTimeBy(31_000)
        val later = launch { data.graph.items.detail(101).collect {} }
        testScheduler.runCurrent()
        later.cancel()
        assertEquals(2, data.api.calls("item").size)
    }

    @Test
    fun `item pages beyond the 50 last are read again from the snapshot and the server`() = runTest {
        val data = signedIn(folder.newFolder())
        for (id in 1L..60L) {
            val job = launch { data.graph.items.detail(id).collect {} }
            testScheduler.runCurrent()
            job.cancel()
        }
        assertEquals(60, data.api.calls("item").size)

        // The newest is held: no request.
        val newest = collect(data.graph.items.detail(60))
        testScheduler.runCurrent()
        assertEquals(60, data.api.calls("item").size)
        assertEquals(60L, (newest.last() as Loadable.Ready<ItemDetail>).value.id)

        // The oldest went from memory: its snapshot shows first, then the server is asked.
        data.api.hold()
        val oldest = collect(data.graph.items.detail(1))
        testScheduler.runCurrent()
        val saved = oldest.last() as Loadable.Ready<ItemDetail>
        assertEquals(1L, saved.value.id)
        assertTrue(!saved.fresh)
        data.api.release()
        testScheduler.runCurrent()
        assertEquals(61, data.api.calls("item").size)
    }

    @Test
    fun `before GET auth me answers, no request goes out as a profile`() = runTest {
        val data = TestData(this, folder.newFolder())
        data.signIn()
        data.api.hold()
        data.useServer()
        val seen = collect(data.graph.hubs.home())
        testScheduler.runCurrent()
        assertEquals(listOf<Loadable<Hubs>>(Loadable.Loading), seen)
        assertEquals(listOf(data.issuer to null), data.apis.distinct())

        data.api.release()
        testScheduler.runCurrent()
        assertEquals(Loadable.Ready(home, fresh = true), seen.last())
        // The listings' requests carry the profile, so they get its HTTP cache.
        assertTrue(data.apis.contains(data.issuer to 2L))
        // GET /api/server, for the server's language, goes beside GET /api/auth/me.
        assertEquals(listOf("me", "server", "homeHubs"), data.api.calls.map { it.method })
    }

    @Test
    fun `offline with nothing saved says why`() = runTest {
        val data = TestData(this, folder.newFolder())
        data.signIn()
        val offline = OfflineException(IOException("no route"))
        data.api.fail(null, offline)
        data.useServer()
        val seen = collect(data.graph.hubs.home())
        testScheduler.runCurrent()

        assertEquals(Loadable.Failed<Hubs>(offline), seen.last())
    }

    @Test
    fun `the library rows of each library are kept apart`() = runTest {
        val data = signedIn(folder.newFolder())
        val one = Hubs(listOf(Hub("recently_added:1", "Recently Added", 1)))
        data.api.answer("libraryHubs", one)
        val seen = collect(data.graph.hubs.library(1))
        testScheduler.runCurrent()
        assertEquals(Loadable.Ready(one, fresh = true), seen.last())
        assertEquals(1L, data.api.calls("libraryHubs").single().args.single())
    }
}
