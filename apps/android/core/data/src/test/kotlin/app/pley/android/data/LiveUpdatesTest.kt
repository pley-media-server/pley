package app.pley.android.data

import app.pley.android.model.ItemKind
import app.pley.android.model.ItemList
import app.pley.android.model.ItemSummary
import app.pley.android.model.Library
import app.pley.android.model.LibraryKind
import app.pley.android.model.ScanPhase
import app.pley.android.model.ServerEvent
import app.pley.android.network.LiveEvent
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder

class LiveUpdatesTest {
    @get:Rule
    val folder = TemporaryFolder()

    private fun TestScope.time(ms: Long) {
        testScheduler.advanceTimeBy(ms)
        testScheduler.runCurrent()
    }

    @Test
    fun `the stream is open only in the foreground and signed in`() = runTest {
        val data = signedIn(folder.newFolder())
        data.foreground.value = false
        data.graph.liveUpdates.start()
        testScheduler.runCurrent()
        assertEquals(0, data.events.connected.value)

        data.foreground.value = true
        testScheduler.runCurrent()
        assertEquals(1, data.events.connected.value)

        data.foreground.value = false
        testScheduler.runCurrent()
        assertEquals(0, data.events.connected.value)

        data.foreground.value = true
        testScheduler.runCurrent()
        data.graph.session.signOut()
        testScheduler.runCurrent()
        assertEquals(0, data.events.connected.value)
        assertEquals(2, data.events.connections)
    }

    @Test
    fun `while something plays the stream stays open in the background, and closes when it ends`() = runTest {
        val data = signedIn(folder.newFolder())
        val live = data.graph.liveUpdates
        live.start()
        testScheduler.runCurrent()
        val music = Any()
        val video = Any()
        live.playing(music, true)
        live.playing(video, true)
        data.foreground.value = false
        testScheduler.runCurrent()
        // Kept, not opened again: a remote's command can't fall in between.
        assertEquals(1, data.events.connected.value)
        assertEquals(1, data.events.connections)

        live.playing(video, false)
        testScheduler.runCurrent()
        assertEquals(1, data.events.connected.value)
        // Saying it twice changes nothing.
        live.playing(music, true)
        live.playing(music, false)
        live.playing(music, false)
        testScheduler.runCurrent()
        assertEquals(0, data.events.connected.value)

        // A playback that starts in the background opens it.
        live.playing(music, true)
        testScheduler.runCurrent()
        assertEquals(1, data.events.connected.value)
        assertEquals(2, data.events.connections)
    }

    @Test
    fun `signing out closes the stream something playing kept open`() = runTest {
        val data = signedIn(folder.newFolder())
        val live = data.graph.liveUpdates
        live.start()
        live.playing(this, true)
        data.foreground.value = false
        testScheduler.runCurrent()
        assertEquals(1, data.events.connected.value)
        data.graph.session.signOut()
        testScheduler.runCurrent()
        assertEquals(0, data.events.connected.value)
        live.playing(this, false)
    }

    @Test
    fun `coming back while something kept the stream open asks again for what is old`() = runTest {
        val data = signedIn(folder.newFolder())
        collect(data.graph.hubs.home())
        val live = data.graph.liveUpdates
        live.start()
        live.playing(this, true)
        testScheduler.runCurrent()
        data.foreground.value = false
        time(60_000)

        data.foreground.value = true
        testScheduler.runCurrent()
        assertEquals(1, data.events.connections)
        assertEquals(2, data.api.calls("homeHubs").size)
        live.playing(this, false)
    }

    @Test
    fun `in the background, events and reconnections only mark what they touch, asked for when next shown`() = runTest {
        val data = signedIn(folder.newFolder())
        collect(data.graph.hubs.home())
        val live = data.graph.liveUpdates
        live.start()
        live.playing(this, true)
        testScheduler.runCurrent()
        data.foreground.value = false
        time(60_000)

        data.events.send(LiveEvent.Dropped)
        data.events.send(LiveEvent.Reconnected)
        data.events.send(LiveEvent.Event(ServerEvent.WatchlistChanged))
        data.events.send(LiveEvent.Event(ServerEvent.LibraryChanged(1)))
        time(2_000)
        assertEquals(1, data.api.calls("homeHubs").size)

        data.foreground.value = true
        testScheduler.runCurrent()
        assertEquals(2, data.api.calls("homeHubs").size)
        live.playing(this, false)
    }

    @Test
    fun `a stream refused while something plays opens again on coming back`() = runTest {
        val data = signedIn(folder.newFolder())
        val live = data.graph.liveUpdates
        live.start()
        live.playing(this, true)
        testScheduler.runCurrent()
        data.foreground.value = false
        testScheduler.runCurrent()
        data.events.refuse(403)
        time(120_000)
        assertEquals(1, data.events.connections)

        data.foreground.value = true
        testScheduler.runCurrent()
        assertEquals(2, data.events.connections)
        live.playing(this, false)
    }

    @Test
    fun `reading the scans starts following the server, as the shells do`() = runTest {
        val data = signedIn(folder.newFolder())
        testScheduler.runCurrent()
        assertEquals(0, data.events.connected.value)

        data.graph.liveUpdates.scans
        testScheduler.runCurrent()
        assertEquals(1, data.events.connected.value)
    }

    @Test
    fun `starting twice opens one stream`() = runTest {
        val data = signedIn(folder.newFolder())
        data.graph.liveUpdates.start()
        data.graph.liveUpdates.start()
        testScheduler.runCurrent()
        assertEquals(1, data.events.connected.value)
    }

    @Test
    fun `a library's changes are gathered for 1,5 s, then what shows it is asked again`() = runTest {
        val data = signedIn(folder.newFolder())
        collect(data.graph.hubs.library(1))
        data.graph.liveUpdates.start()
        testScheduler.runCurrent()
        val before = data.api.calls("libraryHubs").size

        repeat(5) {
            data.events.send(LiveEvent.Event(ServerEvent.LibraryChanged(1)))
            time(500)
        }
        // Still within 1.5 s of the last change.
        assertEquals(before, data.api.calls("libraryHubs").size)

        time(1_500)
        val asked = data.api.calls("libraryHubs").drop(before).map { it.args.single() }
        assertEquals(listOf<Any?>(1L), asked)
    }

    @Test
    fun `changes to the libraries this profile sees ask again for everything shown, and who it is`() = runTest {
        val data = signedIn(folder.newFolder())
        collect(data.graph.libraries.libraries())
        collect(data.graph.hubs.home())
        collect(data.graph.hubs.library(1))
        collect(data.graph.items.detail(101))
        data.graph.liveUpdates.start()
        testScheduler.runCurrent()
        val me = data.api.calls("me").size

        // A kid's age limit lifted (or a library added, or their order): their Home fills at once.
        data.events.send(LiveEvent.Event(ServerEvent.LibrariesChanged))
        testScheduler.runCurrent()

        assertEquals(2, data.api.calls("libraries").size)
        assertEquals(2, data.api.calls("homeHubs").size)
        assertEquals(2, data.api.calls("libraryHubs").size)
        assertEquals(2, data.api.calls("item").size)
        assertEquals(me + 1, data.api.calls("me").size)
    }

    @Test
    fun `reconnecting asks again for what is older than 30 s`() = runTest {
        val data = signedIn(folder.newFolder())
        collect(data.graph.hubs.home())
        data.graph.liveUpdates.start()
        testScheduler.runCurrent()
        data.events.send(LiveEvent.Reconnected)
        testScheduler.runCurrent()
        assertEquals(1, data.api.calls("homeHubs").size)

        time(31_000)
        data.events.send(LiveEvent.Reconnected)
        testScheduler.runCurrent()
        assertEquals(2, data.api.calls("homeHubs").size)
    }

    @Test
    fun `coming back to the foreground asks again for what is old`() = runTest {
        val data = signedIn(folder.newFolder())
        collect(data.graph.hubs.home())
        data.graph.liveUpdates.start()
        testScheduler.runCurrent()
        data.foreground.value = false
        time(60_000)

        data.foreground.value = true
        testScheduler.runCurrent()

        assertEquals(2, data.api.calls("homeHubs").size)
    }

    @Test
    fun `a refused stream stays closed until the next start`() = runTest {
        val data = signedIn(folder.newFolder())
        data.graph.liveUpdates.start()
        testScheduler.runCurrent()
        data.events.refuse(403)
        time(120_000)
        assertEquals(0, data.events.connected.value)
        assertEquals(1, data.events.connections)

        data.foreground.value = false
        testScheduler.runCurrent()
        data.foreground.value = true
        testScheduler.runCurrent()
        assertEquals(2, data.events.connections)
    }

    @Test
    fun `scans fill the scan state`() = runTest {
        val data = signedIn(folder.newFolder())
        val live = data.graph.liveUpdates
        live.start()
        testScheduler.runCurrent()

        data.events.send(LiveEvent.Event(ServerEvent.ScanStarted(1)))
        testScheduler.runCurrent()
        assertEquals(listOf(ScanState(1, null)), live.scans.value)

        data.events.send(LiveEvent.Event(ServerEvent.ScanProgress(1, ScanPhase.Probing, processed = 25, total = 100)))
        testScheduler.runCurrent()
        assertEquals(listOf(ScanState(1, 0.25f)), live.scans.value)

        data.events.send(LiveEvent.Event(ServerEvent.ScanFinished(1)))
        testScheduler.runCurrent()
        assertTrue(live.scans.value.isEmpty())
    }

    @Test
    fun `libraries the server says are scanning show as scans`() = runTest {
        val data = signedIn(folder.newFolder()) {
            api.answer("libraries", listOf(Library(1, "Movies", LibraryKind.Movies, scanning = true), Library(2, "TV", LibraryKind.Shows)))
        }
        data.graph.liveUpdates.start()
        collect(data.graph.libraries.libraries())
        testScheduler.runCurrent()

        assertEquals(listOf(ScanState(1, null)), data.graph.liveUpdates.scans.value)
    }

    @Test
    fun `watch state changed elsewhere, Home and what shows those items are asked again 300 ms later`() = runTest {
        val data = signedIn(folder.newFolder())
        collect(data.graph.hubs.home())
        collect(data.graph.hubs.library(2))
        collect(data.graph.items.detail(203))
        collect(data.graph.items.children(202))
        collect(data.graph.items.detail(101))
        data.graph.liveUpdates.start()
        testScheduler.runCurrent()
        val home = data.api.calls("homeHubs").size
        val items = data.api.calls("item").size

        // An episode watched on another device: it, its season and its show.
        data.events.send(LiveEvent.Event(ServerEvent.UserStateChanged(listOf(203, 202, 201))))
        time(299)
        assertEquals(home, data.api.calls("homeHubs").size)

        time(1)
        assertEquals(home + 1, data.api.calls("homeHubs").size)
        assertEquals(listOf<Any?>(2L), data.api.calls("libraryHubs").drop(1).map { it.args.single() })
        // The episode's page and its season's episodes; not the film's page.
        assertEquals(listOf<Any?>(203L), data.api.calls("item").drop(items).map { it.args.single() })
        assertEquals(listOf<Any?>(202L), data.api.calls("children").drop(1).map { it.args.single() })
    }

    @Test
    fun `a library change heard with a watch state change is asked for in the same round`() = runTest {
        val data = signedIn(folder.newFolder())
        collect(data.graph.hubs.home())
        collect(data.graph.hubs.library(1))
        data.graph.liveUpdates.start()
        testScheduler.runCurrent()

        // A play that ends: its watch state, then its library's change.
        data.events.send(LiveEvent.Event(ServerEvent.UserStateChanged(listOf(101))))
        data.events.send(LiveEvent.Event(ServerEvent.LibraryChanged(1)))
        time(300)
        assertEquals(2, data.api.calls("homeHubs").size)
        assertEquals(2, data.api.calls("libraryHubs").size)

        // Not again when the library's 1,5 s would have ended.
        time(1_500)
        assertEquals(2, data.api.calls("homeHubs").size)
        assertEquals(2, data.api.calls("libraryHubs").size)
    }

    @Test
    fun `a change made here is asked for once, not again when the server tells of it`() = runTest {
        val data = signedIn(folder.newFolder())
        collect(data.graph.hubs.home())
        collect(data.graph.items.detail(101))
        data.graph.liveUpdates.start()
        testScheduler.runCurrent()

        data.api.hold()
        val change = launch { data.graph.watchState.setWatched(101, true) }
        testScheduler.runCurrent()
        // The server tells every device of it, this one too, as it answers.
        data.events.send(LiveEvent.Event(ServerEvent.UserStateChanged(listOf(101))))
        time(100)
        data.api.release()
        testScheduler.runCurrent()
        change.join()
        assertEquals(2, data.api.calls("homeHubs").size)
        assertEquals(2, data.api.calls("item").size)

        time(200)
        assertEquals(2, data.api.calls("homeHubs").size)
        assertEquals(2, data.api.calls("item").size)
    }

    @Test
    fun `a show marked watched elsewhere asks again for the pages of its episodes`() = runTest {
        val data = signedIn(folder.newFolder())
        collect(data.graph.items.detail(203))
        collect(data.graph.items.children(202))
        collect(data.graph.items.detail(101))
        data.graph.liveUpdates.start()
        testScheduler.runCurrent()
        val items = data.api.calls("item").size

        data.events.send(LiveEvent.Event(ServerEvent.UserStateChanged(listOf(201), children = true)))
        time(300)

        assertEquals(listOf<Any?>(203L), data.api.calls("item").drop(items).map { it.args.single() })
        assertEquals(listOf<Any?>(202L), data.api.calls("children").drop(1).map { it.args.single() })
    }

    @Test
    fun `watch state heard as the app leaves is asked for on coming back, not in the background`() = runTest {
        val data = signedIn(folder.newFolder())
        collect(data.graph.hubs.home())
        data.graph.liveUpdates.start()
        testScheduler.runCurrent()

        data.events.send(LiveEvent.Event(ServerEvent.UserStateChanged(listOf(101))))
        time(100)
        data.foreground.value = false
        time(5_000)
        assertEquals(1, data.api.calls("homeHubs").size)

        // Back well within 30 s: asked again all the same.
        data.foreground.value = true
        testScheduler.runCurrent()
        assertEquals(2, data.api.calls("homeHubs").size)
    }

    @Test
    fun `what a watch state change touches`() {
        val ids = setOf(203L, 202L, 201L)
        assertTrue(ServerLiveUpdates.touches(ids, emptySet(), SnapshotKeys.HOME, null))
        assertTrue(ServerLiveUpdates.touches(ids, emptySet(), SnapshotKeys.libraryHubs(7), null))
        assertTrue(ServerLiveUpdates.touches(ids, emptySet(), SnapshotKeys.item(203), null))
        assertTrue(ServerLiveUpdates.touches(ids, emptySet(), SnapshotKeys.children(202), null))
        assertTrue(ServerLiveUpdates.touches(ids, emptySet(), SnapshotKeys.onDeck(201), null))
        assertFalse(ServerLiveUpdates.touches(ids, emptySet(), SnapshotKeys.item(101), null))
        assertFalse(ServerLiveUpdates.touches(ids, emptySet(), SnapshotKeys.PLAYLISTS, null))

        // Lists holding one of them, wherever they are.
        val next = ItemSummary(id = 204, libraryId = 2, kind = ItemKind.Episode, title = "Next", parentId = 202, grandparentId = 201)
        val film = ItemSummary(id = 101, libraryId = 1, kind = ItemKind.Movie, title = "Film")
        assertTrue(ServerLiveUpdates.touches(ids, emptySet(), SnapshotKeys.search("next"), ItemList(listOf(film, next.copy(id = 203)))))
        assertFalse(ServerLiveUpdates.touches(ids, emptySet(), SnapshotKeys.search("film"), ItemList(listOf(film))))
        // What is under a show marked as a whole.
        assertFalse(ServerLiveUpdates.touches(setOf(201), emptySet(), SnapshotKeys.related(9), ItemList(listOf(next))))
        assertTrue(ServerLiveUpdates.touches(setOf(201), setOf(201), SnapshotKeys.related(9), ItemList(listOf(next))))
    }

    @Test
    fun `what a library change touches`() {
        assertTrue(ServerLiveUpdates.affects(1, "libhubs:1", null))
        assertTrue(ServerLiveUpdates.affects(1, "library:1:title.asc:head", null))
        assertTrue(ServerLiveUpdates.affects(1, "collections:1", null))
        assertTrue(ServerLiveUpdates.affects(1, SnapshotKeys.HOME, null))
        assertFalse(ServerLiveUpdates.affects(1, "libhubs:2", null))
        assertFalse(ServerLiveUpdates.affects(1, "library:11:title.asc:head", null))
        assertFalse(ServerLiveUpdates.affects(1, SnapshotKeys.PLAYLISTS, null))
    }
}
