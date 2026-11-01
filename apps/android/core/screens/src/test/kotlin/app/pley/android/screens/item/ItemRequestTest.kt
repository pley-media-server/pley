package app.pley.android.screens.item

import app.pley.android.data.RequestChange
import app.pley.android.model.ItemKind
import app.pley.android.model.ItemRequest
import app.pley.android.model.RequestAccess
import app.pley.android.model.RequestList
import app.pley.android.model.RequestSummary
import app.pley.android.model.TitleKind
import app.pley.android.model.TitleStatus
import app.pley.android.network.ApiException
import app.pley.android.screens.PleyKey
import app.pley.android.testing.Fixtures
import app.pley.android.testing.data.FakeRequests
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

/** What requests say of a library's movie or show: asked once the page has drawn, never before. */
@OptIn(ExperimentalCoroutinesApi::class)
class ItemRequestTest {
    private val dispatcher = UnconfinedTestDispatcher()
    private val data = FakeData()
    private val summary: RequestSummary = Fixtures.decode("requests/summary.json")

    @Before
    fun setUp() = Dispatchers.setMain(dispatcher)

    @After
    fun tearDown() = Dispatchers.resetMain()

    private fun TestScope.model(requests: FakeRequests): ItemViewModel {
        data.requests = requests
        val model = ItemViewModel(data, RecordingLauncher(), 1, compute = dispatcher)
        backgroundScope.launch(dispatcher) { model.state.collect { } }
        backgroundScope.launch(dispatcher) { model.request.collect { } }
        return model
    }

    private val show = movie().copy(kind = ItemKind.Show, title = "Severance", files = emptyList())

    @Test
    fun `a show's request state is asked for once the page has drawn`() = runTest(dispatcher) {
        val requests = FakeRequests(summary)
        val model = model(requests)
        data.set(show)
        assertNull(model.request.value)
        assertTrue(requests.api.calls("itemRequest").isEmpty())
        model.onShown()
        assertEquals(listOf<Any?>(1L), requests.api.calls("itemRequest").map { it.args.single() })
        val state = model.request.value!!
        assertEquals(TitleStatus.Partial, state.status)
        // Season 2 may still be asked for: Request more seasons opens its picker.
        assertTrue(state.more)
        assertEquals(PleyKey.DiscoverTitle(TitleKind.Show, 95396, pick = true), state.picker)

        // Its requests changed twice in a moment: read again, once.
        requests.change(RequestChange(70, TitleKind.Show, 95396))
        requests.change(RequestChange(70, TitleKind.Show, 95396))
        advanceTimeBy(600)
        assertEquals(2, requests.api.calls("itemRequest").size)
        // Another title's change isn't read for.
        requests.change(RequestChange(71, TitleKind.Movie, 1184918))
        advanceTimeBy(600)
        assertEquals(2, requests.api.calls("itemRequest").size)
    }

    @Test
    fun `nothing is asked for someone who may not ask and has no requests`() = runTest(dispatcher) {
        val requests = FakeRequests(summary.copy(access = RequestAccess.Off, own = 0, unseen = 0))
        val model = model(requests)
        data.set(show)
        model.onShown()
        assertNull(model.request.value)
        assertTrue(requests.api.calls("itemRequest").isEmpty())
    }

    @Test
    fun `someone who may no longer ask still sees their request, without Request more seasons`() = runTest(dispatcher) {
        val requests = FakeRequests(summary.copy(access = RequestAccess.Off))
        val model = model(requests)
        data.set(show)
        model.onShown()
        val state = model.request.value!!
        assertEquals(TitleStatus.Partial, state.status)
        assertTrue(!state.more)
    }

    @Test
    fun `while its request downloads it's read again every 10 s`() = runTest(dispatcher) {
        val requests = FakeRequests(summary)
        val downloading = Fixtures.decode<RequestList>("requests/list.json").requests.first { it.id == 42L }
        requests.api.answer("itemRequest", ItemRequest(TitleKind.Show, 1396, TitleStatus.Requested, downloading, askable = emptyList()))
        val model = model(requests)
        data.set(show)
        model.onShown()
        assertEquals(1, requests.api.calls("itemRequest").size)
        // Its progress sends no events.
        advanceTimeBy(10_001)
        assertEquals(2, requests.api.calls("itemRequest").size)

        // Imported: read only as its requests change from then on.
        requests.api.answer("itemRequest", ItemRequest(TitleKind.Show, 1396, TitleStatus.Library, null, askable = emptyList()))
        advanceTimeBy(10_000)
        assertEquals(3, requests.api.calls("itemRequest").size)
        advanceTimeBy(60_000)
        assertEquals(3, requests.api.calls("itemRequest").size)
        assertEquals(TitleStatus.Library, model.request.value?.status)
    }

    @Test
    fun `a first read without TMDB's details, or failing, is read again until they come`() = runTest(dispatcher) {
        val requests = FakeRequests(summary)
        requests.api.fail("itemRequest", ApiException(503, "tmdb_down", "TMDB isn't answering"))
        val model = model(requests)
        data.set(show)
        model.onShown()
        assertNull(model.request.value)

        // TMDB answered, but not with the details yet: the stage shows, Request more seasons waits.
        requests.api.reset()
        requests.api.answer("itemRequest", ItemRequest(TitleKind.Show, 95396, TitleStatus.Partial, null, askable = null))
        advanceTimeBy(3_001)
        assertEquals(TitleStatus.Partial, model.request.value?.status)
        assertFalse(model.request.value!!.more)
        requests.api.answer("itemRequest", ItemRequest(TitleKind.Show, 95396, TitleStatus.Partial, null, askable = listOf(2)))
        advanceTimeBy(3_000)
        assertTrue(model.request.value!!.more)
        // Then only as its requests change.
        val reads = requests.api.calls("itemRequest").size
        advanceTimeBy(60_000)
        assertEquals(reads, requests.api.calls("itemRequest").size)
    }

    @Test
    fun `TMDB away for long is read again four times, then left`() = runTest(dispatcher) {
        val requests = FakeRequests(summary)
        requests.api.fail("itemRequest", ApiException(503, "tmdb_down", "TMDB isn't answering"))
        val model = model(requests)
        data.set(show)
        model.onShown()
        advanceTimeBy(60_000)
        assertEquals(5, requests.api.calls("itemRequest").size)
        assertNull(model.request.value)
    }

    @Test
    fun `music isn't asked about`() = runTest(dispatcher) {
        val requests = FakeRequests(summary)
        val model = model(requests)
        data.set(movie().copy(kind = ItemKind.Album, title = "Blue"))
        model.onShown()
        assertNull(model.request.value)
        assertTrue(requests.api.calls("itemRequest").isEmpty())
    }
}
