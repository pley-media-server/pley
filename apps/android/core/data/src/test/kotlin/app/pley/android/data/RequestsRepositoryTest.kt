package app.pley.android.data

import app.pley.android.model.NewRequest
import app.pley.android.model.RequestAccess
import app.pley.android.model.RequestList
import app.pley.android.model.RequestQuery
import app.pley.android.model.RequestScope
import app.pley.android.model.RequestStatus
import app.pley.android.model.RequestSummary
import app.pley.android.model.ServerEvent
import app.pley.android.model.TitleKind
import app.pley.android.network.ApiException
import app.pley.android.network.LiveEvent
import app.pley.android.testing.Fixtures
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.Job
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder

class RequestsRepositoryTest {
    @get:Rule
    val folder = TemporaryFolder()

    private fun TestScope.time(ms: Long) {
        testScheduler.advanceTimeBy(ms)
        testScheduler.runCurrent()
    }

    private val summary: RequestSummary = Fixtures.decode("requests/summary.json")

    @Test
    fun `the summary says what shows`() = runTest {
        val data = signedIn(folder.newFolder())
        val requests = data.graph.requests
        // Followed, as the phone's shell follows it.
        collect(requests.state)
        testScheduler.runCurrent()
        assertEquals(RequestsState.Open(summary), requests.state.value)
        assertTrue(requests.state.value.mayAsk)

        data.api.answer("requestSummary", summary.copy(enabled = false, access = RequestAccess.Ask))
        requests.refresh()
        testScheduler.runCurrent()
        assertEquals(RequestsState.Hidden, requests.state.value)

        // Signed in before requests: Discover offers only signing in again.
        val before = summary.copy(signInAgain = RequestScope.Requests, unseen = 0, own = 0)
        data.api.answer("requestSummary", before)
        requests.refresh()
        testScheduler.runCurrent()
        assertEquals(RequestsState.SignInAgain(before), requests.state.value)
        assertTrue(!requests.state.value.mayAsk)

        // May not ask, but has requests: Your requests shows, not Discover.
        val off = summary.copy(access = RequestAccess.Off, own = 2)
        data.api.answer("requestSummary", off)
        requests.refresh()
        testScheduler.runCurrent()
        assertEquals(RequestsState.Open(off), requests.state.value)
        assertTrue(!requests.state.value.mayAsk)
        data.api.answer("requestSummary", off.copy(own = 0))
        requests.refresh()
        testScheduler.runCurrent()
        assertEquals(RequestsState.NoAccess, requests.state.value)
    }

    @Test
    fun `the summary is read only while something follows it`() = runTest {
        val data = signedIn(folder.newFolder())
        val requests = data.graph.requests
        data.graph.liveUpdates.start()
        testScheduler.runCurrent()
        // Made, as a TV's item and search pages make it: nothing is read, then or back in the foreground.
        data.foreground.value = false
        time(10 * 60_000)
        data.foreground.value = true
        testScheduler.runCurrent()
        assertEquals(0, data.api.calls("requestSummary").size)
        assertEquals(RequestsState.Unknown, requests.state.value)

        collect(requests.state)
        testScheduler.runCurrent()
        assertEquals(1, data.api.calls("requestSummary").size)
        assertEquals(RequestsState.Open(summary), requests.state.value)
    }

    @Test
    fun `an older server is taken as requests off, and asked again only when the stream connects`() = runTest {
        val data = signedIn(folder.newFolder())
        data.api.fail("requestSummary", ApiException(404, "not_found", "no route"))
        val requests = data.graph.requests
        collect(requests.state)
        requests.start()
        testScheduler.runCurrent()
        assertEquals(RequestsState.Hidden, requests.state.value)
        assertEquals(1, data.api.calls("requestSummary").size)

        // Refreshes, events and polling don't ask it again.
        requests.refresh()
        data.events.send(LiveEvent.Event(ServerEvent.RequestsSettingsChanged))
        time(5 * 60_000)
        assertEquals(1, data.api.calls("requestSummary").size)

        // Connected again: perhaps updated, so asked.
        data.api.reset()
        data.events.send(LiveEvent.Reconnected)
        testScheduler.runCurrent()
        assertEquals(1, data.api.calls("requestSummary").size)
        assertEquals(RequestsState.Open(summary), requests.state.value)
    }

    @Test
    fun `a build from before scopes for requests is an older server too`() = runTest {
        val data = signedIn(folder.newFolder())
        data.api.fail("requestSummary", ApiException(403, "insufficient_scope", "apps can't", bearerError = "insufficient_scope", scope = null))
        val requests = data.graph.requests
        collect(requests.state)
        testScheduler.runCurrent()
        assertEquals(RequestsState.Hidden, requests.state.value)
    }

    @Test
    fun `while the stream is down the summary is read every minute in the foreground`() = runTest {
        val data = signedIn(folder.newFolder())
        val requests = data.graph.requests
        requests.start()
        testScheduler.runCurrent()
        val first = data.api.calls("requestSummary").size

        time(60_000)
        assertEquals(first + 1, data.api.calls("requestSummary").size)

        // Connected: events tell, nothing polls.
        data.events.send(LiveEvent.Reconnected)
        testScheduler.runCurrent()
        val connected = data.api.calls("requestSummary").size
        time(3 * 60_000)
        assertEquals(connected, data.api.calls("requestSummary").size)

        // Lost: polling again; in the background, not.
        data.events.send(LiveEvent.Dropped)
        time(60_000)
        assertEquals(connected + 1, data.api.calls("requestSummary").size)
        data.foreground.value = false
        time(3 * 60_000)
        assertEquals(connected + 1, data.api.calls("requestSummary").size)
    }

    @Test
    fun `request events are gathered for a second before reading the summary, and screens hear of each`() = runTest {
        val data = signedIn(folder.newFolder())
        val requests = data.graph.requests
        val changes = collect(requests.changes)
        requests.start()
        testScheduler.runCurrent()
        assertTrue(data.events.send(LiveEvent.Reconnected))
        testScheduler.runCurrent()
        val before = data.api.calls("requestSummary").size

        data.events.send(LiveEvent.Event(ServerEvent.RequestsChanged(42, TitleKind.Show, 1396)))
        time(500)
        data.events.send(LiveEvent.Event(ServerEvent.RequestsChanged(44, TitleKind.Movie, 933260)))
        time(500)
        assertEquals(before, data.api.calls("requestSummary").size)
        time(600)
        assertEquals(before + 1, data.api.calls("requestSummary").size)
        assertEquals(
            listOf(RequestChange(42, TitleKind.Show, 1396), RequestChange(44, TitleKind.Movie, 933260)),
            changes.filterNot { it.all },
        )
        assertTrue(changes.first().about(TitleKind.Movie, 1))
        assertTrue(RequestChange(42, TitleKind.Show, 1396).about(TitleKind.Show, 1396))
    }

    @Test
    fun `a screen that falls behind holds up no other`() = runTest {
        val data = signedIn(folder.newFolder())
        val requests = data.graph.requests
        // One screen waits on the server from its first change on.
        val waiting = CompletableDeferred<Unit>()
        backgroundScope.launch(UnconfinedTestDispatcher(testScheduler)) { requests.changes.collect { waiting.await() } }
        val heard = collect(requests.changes)
        requests.start()
        testScheduler.runCurrent()
        for (id in 1L..40L) {
            data.events.send(LiveEvent.Event(ServerEvent.RequestsChanged(id, TitleKind.Movie, 1_000 + id)))
            testScheduler.runCurrent()
        }
        assertEquals((1L..40L).toList(), heard.mapNotNull { it.requestId })
    }

    @Test
    fun `events tell what is ready, decided, asked for and failed, keeping the last three`() = runTest {
        val data = signedIn(folder.newFolder()) { api.answer("requestSummary", summary.copy(unseen = 0)) }
        val requests = data.graph.requests
        requests.start()
        testScheduler.runCurrent()
        assertTrue(data.events.send(LiveEvent.Reconnected))
        testScheduler.runCurrent()

        data.events.send(LiveEvent.Event(Fixtures.decode<ServerEvent>("events/request_available.json")))
        testScheduler.runCurrent()
        val ready = requests.notices.value.single() as RequestNotice.Ready
        assertEquals(listOf("Breaking Bad"), listOf(ready.title))
        assertEquals(listOf(1, 2), ready.seasons)
        assertEquals(230L, ready.itemId)
        assertTrue(!ready.complete)

        // Approved as made: nothing; someone else's approved request: not for managers' notices.
        data.events.send(LiveEvent.Event(ServerEvent.RequestCreated(51, "Dune", TitleKind.Movie, 438631, status = RequestStatus.Approved)))
        testScheduler.runCurrent()
        assertEquals(1, requests.notices.value.size)

        data.events.send(LiveEvent.Event(Fixtures.decode<ServerEvent>("events/request_decided.json")))
        data.events.send(LiveEvent.Event(Fixtures.decode<ServerEvent>("events/request_created.json")))
        data.events.send(LiveEvent.Event(Fixtures.decode<ServerEvent>("events/request_failed.json")))
        testScheduler.runCurrent()
        val notices = requests.notices.value
        assertEquals(3, notices.size)
        assertEquals("We have enough aliens for now", (notices[0] as RequestNotice.Declined).reason)
        assertEquals("Kit", (notices[1] as RequestNotice.Asked).by)
        assertEquals(TitleKind.Movie, (notices[2] as RequestNotice.Failed).kind)

        requests.dismiss(notices[0])
        assertEquals(notices.drop(1), requests.notices.value)
    }

    @Test
    fun `news from before is caught up on once, at start`() = runTest {
        val dir = folder.newFolder()
        val run = appRun()
        val data = signedIn(dir, run)
        data.graph.requests.start()
        testScheduler.runCurrent()
        val notice = data.graph.requests.notices.value.single() as RequestNotice.Ready
        assertEquals("Arrival", notice.title)
        assertEquals(101L, notice.itemId)
        assertTrue(notice.complete)
        assertTrue(data.api.calls("requests").single().args.single().let { it is RequestQuery && it.unseen && it.limit == 20 })
        run.coroutineContext[Job]!!.cancel()
        testScheduler.runCurrent()

        // The next start: announced already.
        val again = signedIn(dir)
        again.graph.requests.start()
        testScheduler.runCurrent()
        assertEquals(1, again.api.calls("requests").size)
        assertEquals(emptyList<RequestNotice>(), again.graph.requests.notices.value)
    }

    @Test
    fun `news no event explained is caught up on, several at once in one notice`() = runTest {
        val data = signedIn(folder.newFolder()) { api.answer("requestSummary", summary.copy(unseen = 0)) }
        val requests = data.graph.requests
        requests.start()
        testScheduler.runCurrent()
        assertEquals(0, data.api.calls("requests").size)

        val list: RequestList = Fixtures.decode("requests/list.json")
        val news = list.requests.take(2).map { it.copy(unseen = true, updatedAt = 1761700000) }
        data.api.answer("requests", list.copy(requests = news))
        data.api.answer("requestSummary", summary.copy(unseen = 2))
        requests.refresh()
        testScheduler.runCurrent()
        assertEquals(2, (requests.notices.value.single() as RequestNotice.News).count)

        // A rise an event explained: told by the event alone. The list is read again only to keep what was told.
        data.events.send(LiveEvent.Reconnected)
        testScheduler.runCurrent()
        val asked = data.api.calls("requests").size
        val event = Fixtures.decode<ServerEvent>("events/request_available.json") as ServerEvent.RequestAvailable
        val ready = list.requests.first { it.id == event.requestId }.copy(unseen = true, updatedAt = 1761700100)
        data.api.answer("requests", list.copy(requests = news + ready))
        data.events.send(LiveEvent.Event(event))
        data.api.answer("requestSummary", summary.copy(unseen = 3))
        time(1_100)
        assertEquals(asked + 1, data.api.calls("requests").size)
        assertEquals(listOf("Breaking Bad"), requests.notices.value.filterIsInstance<RequestNotice.Ready>().map { it.title })
        assertEquals(2, requests.notices.value.size)
    }

    @Test
    fun `news an event told isn't told again at the next start`() = runTest {
        val dir = folder.newFolder()
        val run = appRun()
        val list: RequestList = Fixtures.decode("requests/list.json")
        val ready = list.copy(requests = listOf(list.requests.first { it.id == 42L }.copy(unseen = true, updatedAt = 1761700000)))
        val data = signedIn(dir, run) { api.answer("requestSummary", summary.copy(unseen = 0)) }
        val requests = data.graph.requests
        requests.start()
        testScheduler.runCurrent()
        assertTrue(data.events.send(LiveEvent.Reconnected))
        testScheduler.runCurrent()

        // Breaking Bad is ready: told by its event, and opened from the toast, so its news is never seen.
        data.api.answer("requests", ready)
        data.api.answer("requestSummary", summary.copy(unseen = 1, changedAt = 1761700000))
        data.events.send(LiveEvent.Event(Fixtures.decode<ServerEvent>("events/request_available.json")))
        time(1_100)
        assertEquals(1, requests.notices.value.size)
        run.coroutineContext[Job]!!.cancel()
        testScheduler.runCurrent()

        // The next start: unseen still, and announced already.
        val again = signedIn(dir) { api.answer("requests", ready) }
        again.graph.requests.start()
        testScheduler.runCurrent()
        assertEquals(1, again.api.calls("requests").size)
        assertEquals(emptyList<RequestNotice>(), again.graph.requests.notices.value)
    }

    @Test
    fun `seeing the requests clears the badge at once`() = runTest {
        val data = signedIn(folder.newFolder())
        val requests = data.graph.requests
        collect(requests.state)
        testScheduler.runCurrent()
        assertEquals(1, requests.state.value.summary?.badge)
        data.api.answer("requestSummary", summary.copy(unseen = 0))
        requests.seen()
        testScheduler.runCurrent()
        assertEquals(0, requests.state.value.summary?.badge)
        assertEquals(1, data.api.calls("requestsSeen").size)
    }

    @Test
    fun `asking reads the summary and the title again`() = runTest {
        val data = signedIn(folder.newFolder())
        val requests = data.graph.requests
        val title = collect(requests.title(TitleKind.Movie, 1184918))
        val changes = collect(requests.changes)
        testScheduler.runCurrent()
        val reads = data.api.calls("discoverTitle").size
        val summaries = data.api.calls("requestSummary").size
        val asked = requests.ask(NewRequest(TitleKind.Movie, 1184918))
        testScheduler.runCurrent()
        assertTrue(asked.created)
        assertEquals(reads + 1, data.api.calls("discoverTitle").size)
        assertEquals(summaries + 1, data.api.calls("requestSummary").size)
        assertEquals("The Wild Robot", title.last().valueOrNull?.title)
        // The screens hear of it at once, before the server's event.
        assertEquals(listOf(RequestChange(asked.request.id, TitleKind.Movie, 1184918)), changes)
    }

    @Test
    fun `withdrawing tells the screens which title changed`() = runTest {
        val data = signedIn(folder.newFolder())
        val requests = data.graph.requests
        val changes = collect(requests.changes)
        testScheduler.runCurrent()
        val pending = requests.list(RequestQuery()).requests.first { it.id == 44L }
        requests.delete(pending)
        testScheduler.runCurrent()
        assertEquals(listOf(44L), data.api.calls("deleteRequest").map { it.args.single() })
        assertEquals(listOf(RequestChange(44, pending.kind, pending.tmdbId)), changes)
    }

    @Test
    fun `a title without a TMDB match, or a request gone, is null`() = runTest {
        val data = signedIn(folder.newFolder())
        data.api.fail("itemRequest", ApiException(404, "not_found", "no match"))
        data.api.fail("request", ApiException(404, "not_found", "gone"))
        assertNull(data.graph.requests.itemRequest(201))
        assertNull(data.graph.requests.request(44))
        data.api.reset()
        assertEquals(listOf(2), data.graph.requests.itemRequest(201)?.askable)
        assertEquals(44L, data.graph.requests.request(44)?.id)
    }

    @Test
    fun `the gate follows the summary's rules`() {
        assertEquals(RequestsState.Unknown, RequestsState.of(null))
        assertEquals(RequestsState.Hidden, RequestsState.of(RequestSummary(enabled = false, access = RequestAccess.Auto, manage = false)))
        val manager = RequestSummary(enabled = true, access = RequestAccess.Off, manage = true)
        assertEquals(RequestsState.Open(manager), RequestsState.of(manager))
        val approvals = RequestSummary(enabled = true, access = RequestAccess.Auto, signInAgain = RequestScope.Approvals)
        assertEquals(RequestsState.Open(approvals), RequestsState.of(approvals))
    }
}
