@file:OptIn(ExperimentalCoroutinesApi::class, FlowPreview::class)

package app.pley.android.screens.requests

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import app.pley.android.data.Loadable
import app.pley.android.data.RequestsState
import app.pley.android.data.mayAsk
import app.pley.android.data.summary
import app.pley.android.model.Approval
import app.pley.android.model.DiscoverDetail
import app.pley.android.model.MediaRequest
import app.pley.android.model.NewRequest
import app.pley.android.model.RequestScope
import app.pley.android.model.RequestSummary
import app.pley.android.model.TitleKind
import app.pley.android.screens.PleyKey
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.FlowPreview
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.debounce
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.filter
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.onEach
import kotlinx.coroutines.flow.onStart
import kotlinx.coroutines.flow.receiveAsFlow
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.flow.transformLatest
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

/** A title page's state. */
sealed interface TitleUiState {
    data object Loading : TitleUiState

    /** The page can't show: signed in before requests, requests off, TMDB away, not found… [origin]: the server, to sign in to again. */
    data class Problem(val problem: RequestProblem, val summary: RequestSummary?, val origin: String = "") : TitleUiState

    data class Ready(
        val detail: DiscoverDetail,
        val action: MainAction,
        val allowance: Allowance,
        val manage: Boolean,
        val origin: String,
        /** An ask or an action is on its way: the buttons wait. */
        val busy: Boolean,
        /** From what was read before: the server didn't answer this time ([problem]). */
        val problem: RequestProblem?,
        /** The summary the page was drawn with: the limits its words name. */
        val summary: RequestSummary? = null,
    ) : TitleUiState
}

/**
 * A title found on TMDB: its details, its one main
 * action, its seasons with their states, and the season picker; the
 * requests for it (this account's and those it follows; everyone's for
 * managers) with their actions. While it shows, the page reads itself
 * again when one of its requests changes, when many did, and every 10 s
 * while one of them downloads or goes into the library (download
 * progress sends no events).
 */
class DiscoverTitleViewModel(
    private val key: PleyKey.DiscoverTitle,
    private val deps: RequestsDeps,
) : ViewModel() {
    private val requests = deps.requests
    private val eventChannel = Channel<RequestEvent>(Channel.BUFFERED)
    private val busy = MutableStateFlow(false)
    private val retries = MutableStateFlow(0)

    /** What the page tells once: what asking did, what failed, where to go. */
    val events: Flow<RequestEvent> = eventChannel.receiveAsFlow()

    private val _picker = MutableStateFlow<SeasonPicker?>(null)

    /** The season picker while it shows. */
    val picker: StateFlow<SeasonPicker?> = _picker.asStateFlow()

    private val _approving = MutableStateFlow<ApproveDialog?>(null)

    /** Managers: the Approve… dialog while it shows. */
    val approving: StateFlow<ApproveDialog?> = _approving.asStateFlow()

    /** The picker was asked for when the page opened (Request more seasons), and hasn't opened yet. */
    private var pickPending = key.pick

    /** One of the page's requests downloads or goes into the library. */
    private val downloading = MutableStateFlow(false)

    /** The page's requests, for news that doesn't name its title. */
    @Volatile
    private var onPage: Set<Long> = emptySet()

    /** While the page shows and something on it moves, it's read again every 10 s. */
    private val ticks: Flow<Unit> = downloading.transformLatest { on ->
        emit(Unit)
        while (on) {
            delay(RequestsViewModel.MOVING_MS)
            requests.refreshTitle(key.kind, key.tmdbId)
        }
    }

    /**
     * While the page shows, it's read again when many requests changed (or,
     * without events, the server says something did), or when one of its
     * own did and the news doesn't name the title (one that failed). News
     * naming the title reads it again in the repository.
     */
    private val news: Flow<Unit> = requests.changes
        .filter { it.all || (it.tmdbId == null && it.requestId != null && it.requestId in onPage) }
        .debounce(RequestsViewModel.CHANGES_MS)
        .onEach { requests.refreshTitle(key.kind, key.tmdbId) }
        .map { }
        .onStart { emit(Unit) }

    /** The server's address: pictures come from it, and signing in again goes to it. */
    private val server: Flow<String> = deps.origin.map { it.orEmpty() }.distinctUntilChanged()

    val state: StateFlow<TitleUiState> = combine(requests.state, retries) { gate, _ -> gate }
        .flatMapLatest { gate ->
            when {
                gate is RequestsState.SignInAgain -> server.map { TitleUiState.Problem(RequestProblem.SignInAgain(RequestScope.Requests), gate.summary, it) }
                gate == RequestsState.Hidden -> flowOf(TitleUiState.Problem(RequestProblem.Off, null))
                gate == RequestsState.NoAccess -> flowOf(TitleUiState.Problem(RequestProblem.NoAccess, null))
                gate == RequestsState.Unknown -> flowOf(TitleUiState.Loading)
                else -> combine(
                    requests.title(key.kind, key.tmdbId),
                    server,
                    busy,
                    ticks,
                    news,
                ) { load, origin, waiting, _, _ -> page(gate, load, origin, waiting) }
            }
        }
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(STOP_MS), TitleUiState.Loading)

    private fun page(gate: RequestsState, load: Loadable<DiscoverDetail>, origin: String, waiting: Boolean): TitleUiState {
        val summary = gate.summary
        val detail = when (load) {
            Loadable.Loading -> return TitleUiState.Loading
            is Loadable.Ready -> load.value
            is Loadable.Failed -> load.stale ?: return TitleUiState.Problem(RequestProblem.of(load.error), summary, origin)
        }
        onPage = detail.requests.mapTo(HashSet()) { it.id }
        downloading.value = detail.requests.any { it.moving }
        if (pickPending && detail.kind == TitleKind.Show) {
            pickPending = false
            val action = mainAction(detail, gate)
            if (action is MainAction.Request && action.ask == Ask.Yes || action is MainAction.Partial && action.more == Ask.Yes) {
                _picker.value = SeasonPicker.of(detail, Allowance.of(detail, summary).seasonsLeft)
            }
        }
        // The picker follows the page as it's read again: seasons asked for meanwhile can't be ticked.
        _picker.update { open -> open?.let { refreshed(it, detail, summary) } }
        return TitleUiState.Ready(
            detail = detail,
            action = mainAction(detail, gate),
            allowance = Allowance.of(detail, summary),
            manage = summary?.manage == true,
            origin = origin,
            busy = waiting,
            problem = (load as? Loadable.Failed)?.let { RequestProblem.of(it.error) },
            summary = summary,
        )
    }

    private fun refreshed(open: SeasonPicker, detail: DiscoverDetail, summary: RequestSummary?): SeasonPicker {
        val fresh = SeasonPicker.of(detail, Allowance.of(detail, summary).seasonsLeft)
        val askable = fresh.seasons.filter { it.askable }.mapTo(HashSet()) { it.season.number }
        return fresh.copy(picked = open.picked.filterTo(HashSet()) { it in askable }, start = open.start)
    }

    private fun shown(): TitleUiState.Ready? = state.value as? TitleUiState.Ready

    /** Reads the page again after it failed. */
    fun retry() {
        requests.refresh()
        requests.refreshTitle(key.kind, key.tmdbId)
        retries.update { it + 1 }
    }

    /** Request: a movie is asked for; a show's picker opens. */
    fun request() {
        val page = shown() ?: return
        if (page.busy) return
        if (page.detail.kind == TitleKind.Show) {
            openPicker()
        } else {
            ask(NewRequest(page.detail.kind, page.detail.tmdbId), page.detail.title)
        }
    }

    /** Request more seasons: the picker; opened from a season's poster ([start], on TVs), that season ticked too. */
    fun openPicker(start: Int? = null) {
        val page = shown() ?: return
        _picker.value = SeasonPicker.of(page.detail, page.allowance.seasonsLeft, start)
    }

    fun toggle(season: Int) {
        _picker.update { it?.toggle(season) }
    }

    fun closePicker() {
        _picker.value = null
    }

    /** The picker's Request: the seasons ticked. */
    fun askPicked() {
        val page = shown() ?: return
        val picker = _picker.value ?: return
        if (!picker.ready || page.busy) return
        ask(NewRequest(page.detail.kind, page.detail.tmdbId, picker.asked()), page.detail.title, closesPicker = true)
    }

    /** Tell me too: following the requests others made, which costs nothing. */
    fun tellMeToo() {
        val page = shown() ?: return
        if (page.busy) return
        ask(tellMeToo(page.detail), page.detail.title)
    }

    /** Stop following the request this account follows for it. */
    fun stopFollowing() {
        val page = shown() ?: return
        val request = (page.action as? MainAction.Following)?.request ?: activeRequest(page.detail)?.takeIf { it.watching } ?: return
        act(request, CardAction.StopFollowing)
    }

    /** [action] on one of the page's requests ([reason] for Decline). */
    fun act(request: MediaRequest, action: CardAction, reason: String? = null) {
        if (action == CardAction.ApproveWith) return openApprove(request)
        run(request, action, null, reason)
    }

    /** Approve…: the dialog, with the service's choices once read. */
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
        viewModelScope.launch {
            busy.value = true
            try {
                val outcome = attempt(::failed) { requests.perform(request, action, approval, reason) }
                if (outcome is Outcome.AskedAgain) eventChannel.send(RequestEvent.Asked(outcome.asked, request.title))
            } finally {
                busy.value = false
            }
        }
    }

    private fun ask(request: NewRequest, title: String, closesPicker: Boolean = false) {
        viewModelScope.launch {
            busy.value = true
            try {
                val asked = attempt(::failed) { requests.ask(request) } ?: return@launch
                if (closesPicker) _picker.value = null
                eventChannel.send(RequestEvent.Asked(asked, title))
            } finally {
                busy.value = false
            }
        }
    }

    private suspend fun failed(problem: RequestProblem) {
        // Someone acted meanwhile: the page is read again, and what happened is told.
        if (problem is RequestProblem.Conflict || problem is RequestProblem.NotFound) requests.refreshTitle(key.kind, key.tmdbId)
        eventChannel.send(RequestEvent.Failed(problem))
    }

    /** Where Open goes: the library's title. */
    fun open(): PleyKey? = when (val action = shown()?.action) {
        is MainAction.Open -> action.itemId?.let { PleyKey.Item(it) }
        is MainAction.Partial -> action.itemId?.let { PleyKey.Item(it) }
        else -> null
    }

    /** Whether asking works here now, for the interface's own checks. */
    val mayAsk: Boolean get() = requests.state.value.mayAsk

    companion object {
        /** Keeps loading this long after the page stops showing. */
        const val STOP_MS = 5_000L
    }
}
