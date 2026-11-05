@file:OptIn(ExperimentalCoroutinesApi::class, FlowPreview::class)

package app.pley.android.screens.requests

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import app.pley.android.data.RequestChange
import app.pley.android.data.RequestsState
import app.pley.android.data.mayAsk
import app.pley.android.data.summary
import app.pley.android.model.Approval
import app.pley.android.model.DiscoverTitle
import app.pley.android.model.MediaRequest
import app.pley.android.model.RequestList
import app.pley.android.model.RequestQuery
import app.pley.android.model.RequestScope
import app.pley.android.model.RequestStage
import app.pley.android.model.RequestStatus
import app.pley.android.model.RequestUser
import app.pley.android.model.TitleKind
import app.pley.android.model.TitleStatus
import app.pley.android.screens.PleyKey
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.FlowPreview
import kotlinx.coroutines.Job
import kotlinx.coroutines.channels.BufferOverflow
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.debounce
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.receiveAsFlow
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.flow.transformLatest
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

/** Which requests a list shows: managers' Needs attention, all of them, or those with one status. */
enum class RequestView(val status: RequestStatus?) {
    /** Managers: pending ones, and the failed ones the server won't try again (attention=1&order=oldest). */
    Attention(null),
    All(null),
    Pending(RequestStatus.Pending),
    Approved(RequestStatus.Approved),
    Available(RequestStatus.Available),
    Declined(RequestStatus.Declined),
    Failed(RequestStatus.Failed),
}

/** A list's filters: the view, movies or shows, one person's (managers), a title, only this account's. */
data class RequestFilter(
    val view: RequestView = RequestView.All,
    val kind: TitleKind? = null,
    val userId: Long? = null,
    val q: String = "",
    val mine: Boolean = false,
) {
    /** Some filter narrows the list beyond its view. */
    val narrowed: Boolean get() = kind != null || userId != null || q.isNotBlank() || mine

    /** Less than all shows (or all that needs attention): with none, none match. */
    val filtered: Boolean get() = narrowed || view.status != null

    fun query(offset: Int, limit: Int): RequestQuery = RequestQuery(
        statuses = view.status?.let(::listOf).orEmpty(),
        attention = view == RequestView.Attention,
        kind = kind,
        userId = userId,
        q = q.trim().takeIf { it.isNotEmpty() },
        oldest = view == RequestView.Attention,
        mine = mine,
        offset = offset,
        limit = limit,
    )

    /** Whether [request] still belongs in the list, as it is now (after an action on it). */
    fun matches(request: MediaRequest): Boolean = when {
        kind != null && request.kind != kind -> false
        userId != null && request.requestedBy?.id != userId -> false
        q.isNotBlank() && !request.title.contains(q.trim(), ignoreCase = true) -> false
        mine && !request.mine && !request.watching -> false
        view == RequestView.Attention -> request.status == RequestStatus.Pending || (request.status == RequestStatus.Failed && request.retryAt == null)
        view.status != null -> request.status == view.status
        else -> true
    }
}

/** A list of requests, as it shows. */
sealed interface RequestListState {
    data object Loading : RequestListState

    data class Ready(
        val requests: List<MediaRequest>,
        val total: Int,
        /** By status, with every filter but the view's. */
        val counts: Map<String, Int>,
        /** Managers: the people with requests, for the person filter. */
        val people: List<RequestUser>,
        val loadingMore: Boolean = false,
        /** Reading it again failed: what shows is from before. */
        val problem: RequestProblem? = null,
    ) : RequestListState {
        val more: Boolean get() = requests.size < total

        /** Something in it downloads or goes into the library: it's read again every 10 s. */
        val moving: Boolean get() = requests.any { it.moving }

        fun count(status: RequestStatus): Int = counts[status.wire] ?: 0
    }

    /** Nothing to list: none at all, or none matching the filters ([filtered]). */
    data class Empty(val filtered: Boolean) : RequestListState

    data class Failed(val problem: RequestProblem) : RequestListState
}

/** Your requests, or for managers everyone's (Requests). */
data class RequestsUiState(
    val gate: RequestsState = RequestsState.Unknown,
    /** Everyone's, with Needs attention, the filters and the managers' actions. */
    val manage: Boolean = false,
    val filter: RequestFilter = RequestFilter(),
    val list: RequestListState = RequestListState.Loading,
    val origin: String = "",
    /** Managers: how many need them, beside Needs attention. */
    val attention: Int = 0,
    /** An administrator's app without approvals: "Sign in again to approve and decline here." */
    val signInForApprovals: Boolean = false,
    /** Requests with an action on its way. */
    val busy: Set<Long> = emptySet(),
    /** Asking works: titles open their Discover page. */
    val mayAsk: Boolean = false,
    /** Requests can't show at all: requests off, signed in before requests. */
    val problem: RequestProblem? = null,
    /** Pulled to refresh: the list is read again. */
    val refreshing: Boolean = false,
)

/**
 * Your requests, and for managers everyone's,
 * opening on Needs attention (`attention=1&order=oldest`), with filters
 * (movies or shows, a person, a title) and their actions. Each card offers
 * exactly the server's actions. The list follows requests_changed (one
 * request read again when only it changed), every 10 s while something in
 * it downloads, and every 30 s while events can't come. Once its first
 * page shows, the person's news is seen.
 */
class RequestsViewModel(
    private val deps: RequestsDeps,
) : ViewModel() {
    private val requests = deps.requests
    private val gate: StateFlow<RequestsState> = requests.state
    private val eventChannel = Channel<RequestEvent>(Channel.BUFFERED)

    /** What the page tells once: what an action did or why it failed. */
    val events: Flow<RequestEvent> = eventChannel.receiveAsFlow()

    private val filter = MutableStateFlow<RequestFilter?>(null)
    private val list = MutableStateFlow<RequestListState>(RequestListState.Loading)
    private val busy = MutableStateFlow<Set<Long>>(emptySet())
    private val typedTitle = MutableStateFlow("")
    private val refreshing = MutableStateFlow(false)
    private val reloadSoon = MutableSharedFlow<Unit>(extraBufferCapacity = 1, onBufferOverflow = BufferOverflow.DROP_OLDEST)
    private var seen = false
    private var loading: Job? = null

    /** What is typed in the title filter, as the field shows it. */
    val titleQuery: StateFlow<String> = typedTitle.asStateFlow()

    private val _approving = MutableStateFlow<ApproveDialog?>(null)

    /** Managers: the Approve… dialog while it shows. */
    val approving: StateFlow<ApproveDialog?> = _approving.asStateFlow()

    /** Lists are read again while they show: every 10 s while something moves, every 30 s without events. */
    private val ticks: Flow<Unit> = combine(list.map { (it as? RequestListState.Ready)?.moving == true }.distinctUntilChanged(), deps.streaming) { moving, streaming ->
        when {
            moving -> MOVING_MS
            !streaming -> POLL_MS
            else -> 0L
        }
    }.distinctUntilChanged().transformLatest { period ->
        emit(Unit)
        while (period > 0) {
            delay(period)
            reload(quiet = true)
        }
    }

    private data class Head(val gate: RequestsState, val filter: RequestFilter?, val list: RequestListState, val busy: Set<Long>, val origin: String)

    val state: StateFlow<RequestsUiState> = combine(
        combine(gate, filter, list, busy, deps.origin.map { it.orEmpty() }.distinctUntilChanged(), ::Head),
        ticks,
        refreshing,
    ) { head, _, pulled -> ui(head).copy(refreshing = pulled) }
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(STOP_MS), RequestsUiState())

    private fun ui(head: Head): RequestsUiState {
        val summary = head.gate.summary
        val problem = when (head.gate) {
            is RequestsState.SignInAgain -> RequestProblem.SignInAgain(RequestScope.Requests)
            RequestsState.Hidden -> RequestProblem.Off
            RequestsState.NoAccess -> RequestProblem.NoAccess
            else -> null
        }
        return RequestsUiState(
            gate = head.gate,
            manage = summary?.manage == true,
            filter = head.filter ?: RequestFilter(),
            list = if (head.filter == null) RequestListState.Loading else head.list,
            origin = head.origin,
            attention = summary?.let { it.pending + it.failed } ?: 0,
            signInForApprovals = summary?.signInAgain == RequestScope.Approvals,
            busy = head.busy,
            mayAsk = head.gate.mayAsk,
            problem = problem,
        )
    }

    init {
        viewModelScope.launch {
            // Managers open on Needs attention; everyone else on all of theirs.
            val open = gate.first { it is RequestsState.Open }
            filter.value = RequestFilter(view = if (open.summary?.manage == true) RequestView.Attention else RequestView.All)
        }
        viewModelScope.launch { filter.filterNotNull().distinctUntilChanged().collectLatest { load(it) } }
        viewModelScope.launch {
            typedTitle.map { it.trim() }.distinctUntilChanged().debounce(TYPING_MS).collect { q ->
                filter.update { it?.copy(q = q) }
            }
        }
        viewModelScope.launch { requests.changes.gathered().collect { batch -> batch.forEach { changed(it) } } }
        viewModelScope.launch { reloadSoon.debounce(CHANGES_MS).collect { reload(quiet = true) } }
    }

    private suspend fun load(f: RequestFilter) {
        list.value = RequestListState.Loading
        list.value = try {
            shown(f, requests.list(f.query(0, PAGE)))
        } catch (e: CancellationException) {
            throw e
        } catch (e: Exception) {
            RequestListState.Failed(RequestProblem.of(e))
        }
        markSeen()
    }

    private fun shown(f: RequestFilter, answer: RequestList): RequestListState =
        if (answer.requests.isEmpty() && answer.total == 0) {
            RequestListState.Empty(filtered = f.filtered)
        } else {
            RequestListState.Ready(answer.requests.distinctBy { it.id }, answer.total, answer.counts, answer.people)
        }

    /** Once the first page has shown: the person's news is seen, and the badge clears. */
    private fun markSeen() {
        if (seen || list.value !is RequestListState.Ready) return
        seen = true
        if ((gate.value.summary?.unseen ?: 0) > 0) viewModelScope.launch { attempt({}) { requests.seen() } }
    }

    /** Reads what shows again, as long as it is now. [quiet]: a failure keeps what shows. */
    private suspend fun reload(quiet: Boolean) {
        val f = filter.value ?: return
        val shownCount = (list.value as? RequestListState.Ready)?.requests?.size ?: 0
        try {
            val answer = first(f, maxOf(PAGE, shownCount))
            if (filter.value == f) list.value = shown(f, answer)
            markSeen()
        } catch (e: CancellationException) {
            throw e
        } catch (e: Exception) {
            val problem = RequestProblem.of(e)
            list.update { now -> if (now is RequestListState.Ready) now.copy(problem = problem) else if (quiet) now else RequestListState.Failed(problem) }
        }
    }

    /** The first [count] requests [f] keeps, read [MAX_PAGE] at a time. */
    private suspend fun first(f: RequestFilter, count: Int): RequestList {
        var answer = requests.list(f.query(0, minOf(count, MAX_PAGE)))
        while (answer.requests.size < minOf(count, answer.total)) {
            val read = answer.requests.size
            val next = requests.list(f.query(read, minOf(count - read, MAX_PAGE)))
            if (next.requests.isEmpty()) break
            answer = next.copy(requests = answer.requests + next.requests)
        }
        return answer
    }

    /** Pull to refresh, or Retry: the summary and the list read again (once, however often it's pulled meanwhile). */
    fun refresh() {
        if (refreshing.value) return
        requests.refresh()
        val f = filter.value ?: return
        if (list.value !is RequestListState.Ready) return restart(f)
        refreshing.value = true
        viewModelScope.launch {
            try {
                reload(quiet = false)
            } finally {
                refreshing.value = false
            }
        }
    }

    private fun restart(f: RequestFilter) {
        loading?.cancel()
        loading = viewModelScope.launch { load(f) }
    }

    /** The list scrolled near its end: the next page. */
    fun more() {
        val f = filter.value ?: return
        val now = list.value as? RequestListState.Ready ?: return
        if (!now.more || now.loadingMore) return
        list.value = now.copy(loadingMore = true, problem = null)
        viewModelScope.launch {
            try {
                val answer = requests.list(f.query(now.requests.size, PAGE))
                list.update { current ->
                    if (filter.value != f || current !is RequestListState.Ready) current
                    else current.copy(requests = (current.requests + answer.requests).distinctBy { it.id }, total = answer.total, loadingMore = false)
                }
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                list.update { current -> if (current is RequestListState.Ready) current.copy(loadingMore = false, problem = RequestProblem.of(e)) else current }
            }
        }
    }

    // ---------------------------------------------------------------- filters

    fun view(view: RequestView) = filter.update { it?.copy(view = view) }

    fun kind(kind: TitleKind?) = filter.update { it?.copy(kind = kind) }

    fun person(userId: Long?) = filter.update { it?.copy(userId = userId) }

    fun mine(only: Boolean) = filter.update { it?.copy(mine = only) }

    /** The title filter's field changed: it applies once typing pauses. */
    fun onTitleChange(text: String) {
        typedTitle.value = text
    }

    // ---------------------------------------------------------------- what changed

    private suspend fun changed(change: RequestChange) {
        val now = list.value as? RequestListState.Ready
        val id = change.requestId
        if (id == null || now == null || now.requests.none { it.id == id }) {
            // Many changed, or one this list may now show: the list, once changes pause.
            reloadSoon.tryEmit(Unit)
            return
        }
        // Only that request: it alone is read again.
        val request = try {
            requests.request(id)
        } catch (e: CancellationException) {
            throw e
        } catch (_: Exception) {
            return
        }
        replace(id, request)
    }

    /** [id] as it is now ([request]), kept where it still belongs; `null`: gone. */
    private fun replace(id: Long, request: MediaRequest?) {
        val f = filter.value ?: return
        list.update { now ->
            if (now !is RequestListState.Ready) return@update now
            val kept = request?.takeIf { f.matches(it) }
            val requests = if (kept != null) now.requests.map { if (it.id == id) kept else it } else now.requests.filterNot { it.id == id }
            val removed = now.requests.size - requests.size
            if (requests.isEmpty() && now.total - removed <= 0) RequestListState.Empty(f.filtered) else now.copy(requests = requests, total = now.total - removed)
        }
    }

    // ---------------------------------------------------------------- actions

    /** [action] on [request] ([reason] for Decline); Approve… opens its dialog. */
    fun act(request: MediaRequest, action: CardAction, reason: String? = null) {
        if (action == CardAction.ApproveWith) return openApprove(request)
        run(request, action, null, reason)
    }

    fun openApprove(request: MediaRequest) {
        _approving.value = ApproveDialog(request, null, loading = true)
        viewModelScope.launch {
            val options = attempt({}) { requests.options() }
            _approving.update { open -> if (open?.request?.id == request.id) open.copy(options = options?.of(request.kind), loading = false) else open }
        }
    }

    fun closeApprove() {
        _approving.value = null
    }

    /** The dialog's Approve, with what was chosen. */
    fun approve(approval: Approval) {
        val dialog = _approving.value ?: return
        _approving.value = null
        run(dialog.request, CardAction.ApproveWith, approval, null)
    }

    private fun run(request: MediaRequest, action: CardAction, approval: Approval?, reason: String?) {
        if (request.id in busy.value) return
        busy.update { it + request.id }
        viewModelScope.launch {
            try {
                when (val outcome = attempt({ failed(request, it) }) { requests.perform(request, action, approval, reason) }) {
                    is Outcome.Changed -> replace(request.id, outcome.request)
                    Outcome.Gone -> replace(request.id, null)
                    is Outcome.AskedAgain -> {
                        eventChannel.send(RequestEvent.Asked(outcome.asked, request.title))
                        reload(quiet = true)
                    }
                    null -> Unit
                }
            } finally {
                busy.update { it - request.id }
            }
        }
    }

    private suspend fun failed(request: MediaRequest, problem: RequestProblem) {
        // A race: the server sends the request as it is now.
        if (problem is RequestProblem.Conflict && problem.request != null) replace(request.id, problem.request)
        if (problem is RequestProblem.NotFound) replace(request.id, null)
        eventChannel.send(RequestEvent.Failed(problem))
    }

    companion object {
        const val PAGE = 30

        /** The most requests the server lists at once. */
        const val MAX_PAGE = 200

        /** While something downloads or goes into the library. */
        const val MOVING_MS = 10_000L

        /** While events can't come. */
        const val POLL_MS = 30_000L

        /** Changes gathered before the list is read again. */
        const val CHANGES_MS = 500L

        /** How long typing in the title filter pauses before it applies. */
        const val TYPING_MS = 300L

        /** Keeps loading this long after the page stops showing. */
        const val STOP_MS = 5_000L
    }
}

/**
 * Where a request card opens: the library's title once it's there, else
 * its Discover page while asking works; `null`: nowhere.
 */
fun opens(request: MediaRequest, mayAsk: Boolean): PleyKey? {
    val item = request.itemId?.let { PleyKey.Item(it) }
    return when {
        item != null && (request.stage == RequestStage.Available || request.stage == RequestStage.Partial) -> item
        mayAsk -> PleyKey.DiscoverTitle(request.kind, request.tmdbId)
        else -> item
    }
}

/**
 * Where [title]'s poster opens: the library's title when a library this
 * account can open has it whole, since its Discover page would offer only
 * Open; else its Discover page. A show partly there opens its Discover
 * page, where more seasons can be asked for.
 */
fun opens(title: DiscoverTitle): PleyKey {
    val item = title.itemId
    return if (title.status == TitleStatus.Library && item != null) PleyKey.Item(item) else PleyKey.DiscoverTitle(title.kind, title.tmdbId)
}
