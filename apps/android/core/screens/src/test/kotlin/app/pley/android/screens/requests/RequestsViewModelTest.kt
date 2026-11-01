package app.pley.android.screens.requests

import app.pley.android.data.RequestChange
import app.pley.android.data.RequestsRepository
import app.pley.android.model.ApiErrorBody
import app.pley.android.model.MediaRequest
import app.pley.android.model.RequestList
import app.pley.android.model.RequestQuery
import app.pley.android.model.RequestStage
import app.pley.android.model.RequestStatus
import app.pley.android.model.RequestSummary
import app.pley.android.model.TitleKind
import app.pley.android.network.ApiException
import app.pley.android.testing.Fixtures
import app.pley.android.testing.data.FakeRequests
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

/**
 * Your requests and the managers' Requests: Needs attention first, the
 * news seen once shown, one request read again when only it changed, and
 * the cards following what actions did.
 */
@OptIn(ExperimentalCoroutinesApi::class)
class RequestsViewModelTest {
    private val dispatcher = StandardTestDispatcher()
    private val summary: RequestSummary = Fixtures.decode("requests/summary.json")
    private val manager: RequestSummary = Fixtures.decode("requests/summary_manager.json")
    private val attention: RequestList = Fixtures.decode("requests/attention.json")

    @Before
    fun main() = Dispatchers.setMain(dispatcher)

    @After
    fun reset() = Dispatchers.resetMain()

    /** The list's ticks never stop while something downloads: these tests step the clock, never run it out. */
    private fun TestScope.settle() {
        advanceTimeBy(1_000)
        runCurrent()
    }

    private fun TestScope.viewModel(requests: FakeRequests, streaming: Boolean = true): RequestsViewModel {
        val vm = RequestsViewModel(RequestsDeps(requests, MutableStateFlow("http://pley"), MutableStateFlow(streaming), dispatcher))
        backgroundScope.launch { vm.state.collect {} }
        return vm
    }

    private fun FakeRequests.queries(): List<RequestQuery> = api.calls("requests").map { it.args.single() as RequestQuery }

    private fun RequestsViewModel.shown(): List<MediaRequest> = (state.value.list as RequestListState.Ready).requests

    @Test
    fun managersOpenOnNeedsAttentionOldestFirst() = runTest(dispatcher) {
        val requests = FakeRequests(manager)
        val vm = viewModel(requests)
        settle()
        assertTrue(vm.state.value.manage)
        assertEquals(RequestView.Attention, vm.state.value.filter.view)
        val query = requests.queries().first()
        assertTrue(query.attention)
        assertTrue(query.oldest)
        assertEquals(attention.requests.map { it.id }, vm.shown().map { it.id })
        assertEquals(3, vm.state.value.attention)
    }

    @Test
    fun everyoneElseSeesTheirOwnAndTheirNewsIsSeenOnce() = runTest(dispatcher) {
        val requests = FakeRequests(summary)
        val vm = viewModel(requests)
        settle()
        assertEquals(RequestView.All, vm.state.value.filter.view)
        assertFalse(requests.queries().first().attention)
        assertEquals(1, requests.api.calls("requestsSeen").size)
        vm.view(RequestView.Declined)
        settle()
        assertEquals(listOf(RequestStatus.Declined), requests.queries().last().statuses)
        assertEquals(1, requests.api.calls("requestsSeen").size)
    }

    @Test
    fun onlyTheRequestThatChangedIsReadAgain() = runTest(dispatcher) {
        val requests = FakeRequests(summary)
        val vm = viewModel(requests)
        settle()
        val lists = requests.api.calls("requests").size
        val pending = vm.shown().first { it.id == 44L }
        requests.api.answer("request", pending.copy(status = RequestStatus.Approved, stage = RequestStage.Approved))
        requests.change(RequestChange(44, TitleKind.Movie, pending.tmdbId))
        settle()
        assertEquals(lists, requests.api.calls("requests").size)
        assertEquals(listOf(44L), requests.api.calls("request").map { it.args.single() })
        assertEquals(RequestStage.Approved, vm.shown().first { it.id == 44L }.stage)

        // Many changed: the list, once.
        requests.change(RequestChange.All)
        requests.change(RequestChange.All)
        settle()
        assertEquals(lists + 1, requests.api.calls("requests").size)
    }

    @Test
    fun changesHeardWhileOneIsReadComeTogetherEachOnce() = runTest(dispatcher) {
        val requests = FakeRequests(summary)
        viewModel(requests)
        settle()
        requests.api.hold()
        requests.change(RequestChange(44, TitleKind.Movie, 933260))
        runCurrent()
        // While 44 is read, it changes again and again: read once more after.
        repeat(20) { requests.change(RequestChange(44, TitleKind.Movie, 933260)) }
        runCurrent()
        requests.api.release()
        settle()
        assertEquals(listOf(44L, 44L), requests.api.calls("request").map { it.args.single() })
    }

    @Test
    fun approvingTakesACardOutOfNeedsAttention() = runTest(dispatcher) {
        val requests = FakeRequests(manager)
        val vm = viewModel(requests)
        settle()
        val pending = vm.shown().first { it.id == 44L }
        vm.act(pending, CardAction.Approve)
        settle()
        assertEquals(listOf(44L, app.pley.android.model.Approval()), requests.api.calls("approveRequest").single().args)
        assertFalse(vm.shown().any { it.id == 44L })
        assertTrue(vm.state.value.busy.isEmpty())
    }

    @Test
    fun aRaceShowsTheRequestAsItIsNow() = runTest(dispatcher) {
        val requests = FakeRequests(manager)
        val vm = viewModel(requests)
        val events = mutableListOf<RequestEvent>()
        backgroundScope.launch { vm.events.collect { events += it } }
        settle()
        vm.view(RequestView.All)
        settle()
        val pending = vm.shown().first { it.id == 44L }
        val declined = pending.copy(status = RequestStatus.Declined, stage = RequestStage.Declined, actions = emptyList())
        requests.api.fail(
            "approveRequest",
            ApiException(409, "conflict", "Someone decided already", details = ApiErrorBody("conflict", "Someone decided already", request = declined)),
        )
        vm.act(pending, CardAction.Approve)
        settle()
        assertEquals(RequestStatus.Declined, vm.shown().first { it.id == 44L }.status)
        assertEquals(RequestEvent.Failed(RequestProblem.Conflict("Someone decided already", declined)), events.single())
    }

    @Test
    fun withdrawingTakesTheCardAway() = runTest(dispatcher) {
        val requests = FakeRequests(summary)
        val vm = viewModel(requests)
        settle()
        val pending = vm.shown().first { it.id == 44L }
        vm.act(pending, CardAction.Withdraw)
        settle()
        assertEquals(listOf(44L), requests.api.calls("deleteRequest").map { it.args.single() })
        assertFalse(vm.shown().any { it.id == 44L })
    }

    @Test
    fun withdrawingTheLastOfAViewSaysNoneMatch() = runTest(dispatcher) {
        val requests = FakeRequests(summary)
        val vm = viewModel(requests)
        settle()
        val pending = vm.shown().first { it.id == 44L }
        requests.api.answer("requests", RequestList(total = 1, requests = listOf(pending)))
        vm.view(RequestView.Pending)
        settle()
        vm.act(pending, CardAction.Withdraw)
        settle()
        // Others may be approved or ready: not "Nothing asked for yet".
        assertEquals(RequestListState.Empty(filtered = true), vm.state.value.list)
    }

    @Test
    fun listsAreReadAgainWhileSomethingDownloads() = runTest(dispatcher) {
        val requests = FakeRequests(summary)
        val vm = viewModel(requests)
        settle()
        // Breaking Bad (42) downloads: every 10 s.
        assertTrue((vm.state.value.list as RequestListState.Ready).moving)
        val before = requests.api.calls("requests").size
        advanceTimeBy(10_000)
        runCurrent()
        assertEquals(before + 1, requests.api.calls("requests").size)
    }

    @Test
    fun withoutEventsAQuietListIsReadEvery30s() = runTest(dispatcher) {
        val requests = FakeRequests(summary)
        requests.api.answer("requests", RequestList(total = 1, requests = listOf(attention.requests.first())))
        val vm = viewModel(requests, streaming = false)
        settle()
        assertFalse((vm.state.value.list as RequestListState.Ready).moving)
        val before = requests.api.calls("requests").size
        advanceTimeBy(28_000)
        runCurrent()
        assertEquals(before, requests.api.calls("requests").size)
        advanceTimeBy(2_000)
        runCurrent()
        assertEquals(before + 1, requests.api.calls("requests").size)
    }

    @Test
    fun aLongListIsReadAgainInPagesTheServerAllows() = runTest(dispatcher) {
        val fake = FakeRequests(manager)
        val all = (1L..230L).map { attention.requests.first().copy(id = it) }
        val asked = mutableListOf<RequestQuery>()
        val requests = object : RequestsRepository by fake {
            override suspend fun list(query: RequestQuery): RequestList {
                asked += query
                return RequestList(total = all.size, requests = all.drop(query.offset).take(minOf(query.limit ?: 50, 200)))
            }
        }
        val vm = RequestsViewModel(RequestsDeps(requests, MutableStateFlow("http://pley"), MutableStateFlow(true), dispatcher))
        backgroundScope.launch { vm.state.collect {} }
        settle()
        while (vm.shown().size < all.size) {
            vm.more()
            settle()
        }
        asked.clear()
        fake.change(RequestChange.All)
        settle()
        // At most 200 at a time: all 230 still show.
        assertEquals(listOf(0 to 200, 200 to 30), asked.map { it.offset to it.limit })
        assertEquals(all.size, vm.shown().size)
    }

    @Test
    fun pullingToRefreshReadsAgainOnceAndShowsItUntilTheListComes() = runTest(dispatcher) {
        val requests = FakeRequests(summary)
        requests.api.answer("requests", RequestList(total = 1, requests = listOf(attention.requests.first())))
        val vm = viewModel(requests)
        settle()
        val lists = requests.queries().size
        val summaries = requests.refreshed
        requests.api.hold()
        vm.refresh()
        // Pulled again while it reads: nothing more is asked.
        vm.refresh()
        settle()
        assertTrue(vm.state.value.refreshing)
        requests.api.release()
        settle()
        assertFalse(vm.state.value.refreshing)
        assertEquals(lists + 1, requests.queries().size)
        assertEquals(summaries + 1, requests.refreshed)
    }

    @Test
    fun theApproveDialogStartsOnTheSettingsChoices() = runTest(dispatcher) {
        val requests = FakeRequests(manager)
        val vm = viewModel(requests)
        backgroundScope.launch { vm.approving.collect {} }
        settle()
        val arcane = vm.shown().first { it.id == 49L }
        vm.act(arcane, CardAction.ApproveWith)
        settle()
        val dialog = vm.approving.value!!
        assertEquals("/anime", dialog.folder)
        vm.approve(dialog.approval(6, "/anime", setOf(1, 2)))
        settle()
        assertEquals(null, vm.approving.value)
        val sent = requests.api.calls("approveRequest").single().args[1] as app.pley.android.model.Approval
        assertEquals(6L, sent.qualityProfile)
        assertEquals(listOf(1, 2), sent.seasons)
    }
}
