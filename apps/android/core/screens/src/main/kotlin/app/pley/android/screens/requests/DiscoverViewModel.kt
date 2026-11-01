@file:OptIn(ExperimentalCoroutinesApi::class, FlowPreview::class)

package app.pley.android.screens.requests

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import app.pley.android.data.Loadable
import app.pley.android.data.RequestChange
import app.pley.android.data.RequestsState
import app.pley.android.data.mayAsk
import app.pley.android.data.summary
import app.pley.android.data.valueOrNull
import app.pley.android.model.DiscoverDetail
import app.pley.android.model.DiscoverList
import app.pley.android.model.DiscoverTitle
import app.pley.android.model.MediaRequest
import app.pley.android.model.NewRequest
import app.pley.android.model.RequestQuery
import app.pley.android.model.RequestScope
import app.pley.android.model.TitleKind
import app.pley.android.screens.PleyKey
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.FlowPreview
import kotlinx.coroutines.Job
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.debounce
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.merge
import kotlinx.coroutines.flow.receiveAsFlow
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.flow.transformLatest
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

/** One of Discover's rows: its titles as read so far. */
data class DiscoverRow(val list: DiscoverList, val pages: Pages)

/** Discover's top: the first trending title, with its main action. */
data class DiscoverHero(val title: DiscoverTitle, val action: MainAction)

/** Discover's page, apart from its search. */
data class DiscoverUiState(
    val gate: RequestsState = RequestsState.Unknown,
    val origin: String = "",
    val hero: DiscoverHero? = null,
    /** In their order; a row shows once it has titles, or is reading them. */
    val rows: List<DiscoverRow> = DiscoverList.entries.map { DiscoverRow(it, Pages()) },
    /** Why the page can't show: the gate first (sign in again, off, no access), then the trending row's failure. */
    val problem: RequestProblem? = null,
    /** Shown from what was saved: the server didn't answer. */
    val stale: Boolean = false,
    /** "Your requests", newest first, for those who have any. */
    val yours: List<MediaRequest> = emptyList(),
    /** This sign-in manages: pills say so. */
    val manage: Boolean = false,
    val allowance: Allowance? = null,
    /** The hero's ask is on its way: its button waits. */
    val asking: Boolean = false,
)

/** Discover's search: what was typed, and TMDB's titles for it. */
sealed interface DiscoverSearch {
    data object Idle : DiscoverSearch

    data class Loading(val query: String) : DiscoverSearch

    data class Ready(val query: String, val pages: Pages) : DiscoverSearch

    data class Empty(val query: String) : DiscoverSearch

    data class Failed(val query: String, val problem: RequestProblem) : DiscoverSearch
}

/**
 * Discover, on phones and tablets: a hero for the
 * first trending title, then rows (Trending this week, Popular movies,
 * Popular shows, Coming to cinemas, Popular anime) read as they come into
 * view ([showRow]), page by page until a screenful and then as they
 * scroll ([more]), each title once; then the person's requests (read
 * again as requests change, and every 10 s while one downloads); a search
 * at the top. Badges follow requests_changed in rows already shown,
 * without reading the rows again.
 */
class DiscoverViewModel(
    initialQuery: String,
    private val deps: RequestsDeps,
) : ViewModel() {
    private val requests = deps.requests
    private val eventChannel = Channel<RequestEvent>(Channel.BUFFERED)

    /** What the page tells once: what asking did, what failed, where to go. */
    val events: Flow<RequestEvent> = eventChannel.receiveAsFlow()

    private val rows = DiscoverList.entries.associateWith { MutableStateFlow(Pages()) }
    private val rowJobs = HashMap<DiscoverList, Job>()
    private val stale = MutableStateFlow(false)
    private val yours = MutableStateFlow<List<MediaRequest>>(emptyList())
    private val retries = MutableStateFlow(0)
    private val asking = MutableStateFlow(false)
    private val gate: StateFlow<RequestsState> = requests.state

    /** The hero's page, read for its exact main action (and there at once when it opens). */
    private val heroDetail: Flow<DiscoverDetail?> = rows.getValue(DiscoverList.Trending)
        .map { it.titles.firstOrNull()?.let { t -> t.kind to t.tmdbId } }
        .distinctUntilChanged()
        .flatMapLatest { id -> if (id == null) flowOf(null) else requests.title(id.first, id.second).map { it.valueOrNull } }

    private val rowStates: Flow<List<DiscoverRow>> =
        combine(rows.map { (list, flow) -> flow.map { DiscoverRow(list, it) } }) { it.toList() }

    private data class Head(val gate: RequestsState, val origin: String, val stale: Boolean, val yours: List<MediaRequest>, val asking: Boolean)

    /** While Discover shows and one of "Your requests" downloads, they're read again every 10 s: download progress sends no events. */
    private val ticks: Flow<Unit> = yours.map { list -> list.any { it.moving } }.distinctUntilChanged().transformLatest { on ->
        emit(Unit)
        while (on) {
            delay(RequestsViewModel.MOVING_MS)
            readYours()
        }
    }

    val state: StateFlow<DiscoverUiState> = combine(
        combine(gate, deps.origin.map { it.orEmpty() }.distinctUntilChanged(), stale, yours, asking, ::Head),
        rowStates,
        heroDetail,
        ticks,
    ) { head, rowList, detail, _ -> page(head, rowList, detail) }
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(STOP_MS), DiscoverUiState())

    private fun page(head: Head, rowList: List<DiscoverRow>, detail: DiscoverDetail?): DiscoverUiState {
        val gate = head.gate
        val summary = gate.summary
        val allowance = summary?.let(Allowance::of)
        val trending = rowList.first().pages
        val problem = when {
            gate is RequestsState.SignInAgain -> RequestProblem.SignInAgain(RequestScope.Requests)
            gate == RequestsState.Hidden -> RequestProblem.Off
            gate == RequestsState.NoAccess -> RequestProblem.NoAccess
            gate is RequestsState.Open && !gate.mayAsk -> RequestProblem.NoAccess
            else -> trending.problem.takeIf { trending.titles.isEmpty() }
        }
        val hero = trending.titles.firstOrNull()?.let { title ->
            val action = detail?.takeIf { it.kind == title.kind && it.tmdbId == title.tmdbId }?.let { mainAction(it, gate) }
                ?: titleAction(title, gate, allowance)
            DiscoverHero(title, action)
        }
        return DiscoverUiState(
            gate = gate,
            origin = head.origin,
            hero = hero,
            rows = rowList,
            problem = problem,
            stale = head.stale,
            yours = head.yours,
            manage = summary?.manage == true,
            allowance = allowance,
            asking = head.asking,
        )
    }

    // ---------------------------------------------------------------- search

    private val typed = MutableStateFlow(initialQuery)
    private val searchMore = MutableStateFlow(0)

    /** What is typed in Discover's search, as the field shows it. */
    val query: StateFlow<String> = typed.asStateFlow()

    private val _search = MutableStateFlow<DiscoverSearch>(DiscoverSearch.Idle)

    /** TMDB's titles for what is typed: asked for once typing pauses 250 ms, page by page as they scroll. */
    val search: StateFlow<DiscoverSearch> = _search.asStateFlow()

    init {
        viewModelScope.launch {
            // Rows are read only while asking works; the trending row at once, for the hero.
            gate.map { it.mayAsk }.distinctUntilChanged().collect { open -> if (open) showRow(DiscoverList.Trending) }
        }
        viewModelScope.launch {
            combine(gate.map { it.mayAsk }.distinctUntilChanged(), typed.map { it.trim() }.distinctUntilChanged()) { open, q -> if (open) q else "" }
                .distinctUntilChanged()
                .debounce { if (it.isEmpty()) 0L else DEBOUNCE_MS }
                .combine(retries) { q, _ -> q }
                .collectLatest(::searchFor)
        }
        viewModelScope.launch { requests.changes.gathered().collect { batch -> batch.forEach { patch(it) } } }
        viewModelScope.launch {
            // "Your requests" for those who have any, read again as requests change.
            val any = gate.map { it.summary?.let { s -> s.own > 0 && s.yourRequests } == true }.distinctUntilChanged()
            val reads = merge(flowOf(Unit), requests.changes.debounce(CHANGES_MS).map { })
            combine(any, reads) { has, _ -> has }.collectLatest { has ->
                if (has) {
                    readYours()
                } else {
                    yours.value = emptyList()
                }
            }
        }
    }

    private suspend fun searchFor(q: String) {
        if (q.isEmpty()) {
            _search.value = DiscoverSearch.Idle
            return
        }
        val scrolled = searchMore.value
        _search.value = DiscoverSearch.Loading(q)
        var pages = Pages()
        var auto = 0
        do {
            val answer = try {
                requests.search(q, pages.page + 1)
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                val problem = RequestProblem.of(e)
                _search.value = if (pages.titles.isEmpty()) DiscoverSearch.Failed(q, problem) else DiscoverSearch.Ready(q, pages.copy(problem = problem))
                return
            }
            pages = pages.with(answer)
            _search.value = if (pages.titles.isEmpty() && !pages.more) DiscoverSearch.Empty(q) else DiscoverSearch.Ready(q, pages)
            // A short page isn't the end: read on until a screenful.
        } while (pages.short(SEARCH_SCREENFUL) && ++auto < MAX_AUTO_PAGES)
        // Then more as the results scroll near their end.
        searchMore.collect { asked ->
            if (asked == scrolled) return@collect
            val shown = _search.value as? DiscoverSearch.Ready ?: return@collect
            if (!shown.pages.more || shown.pages.loading) return@collect
            _search.value = shown.copy(pages = shown.pages.copy(loading = true, problem = null))
            _search.value = try {
                DiscoverSearch.Ready(q, shown.pages.with(requests.search(q, shown.pages.page + 1)))
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                shown.copy(pages = shown.pages.copy(loading = false, problem = RequestProblem.of(e)))
            }
        }
    }

    /** The field changed. */
    fun onQueryChange(text: String) {
        typed.value = text
    }

    /** The search results scrolled near their end. */
    fun moreResults() {
        searchMore.update { it + 1 }
    }

    // ---------------------------------------------------------------- rows

    /** [list]'s row came into view: its first page is read (from what was saved first), then more until a screenful. */
    fun showRow(list: DiscoverList) {
        // Once per row: it follows what was saved and the server's answer from then on.
        if (list in rowJobs) return
        val pages = rows.getValue(list)
        rowJobs[list] = viewModelScope.launch {
            gate.first { it.mayAsk }
            pages.update { it.copy(loading = it.titles.isEmpty(), problem = null) }
            requests.row(list).collect { load ->
                when (load) {
                    Loadable.Loading -> Unit
                    is Loadable.Ready -> {
                        // A newer first page: what came after it is read again.
                        pages.update { it.with(load.value) }
                        if (load.fresh) {
                            if (list == DiscoverList.Trending) stale.value = false
                            fill(list)
                        }
                    }
                    is Loadable.Failed -> {
                        val saved = load.stale
                        val problem = RequestProblem.of(load.error)
                        if (list == DiscoverList.Trending) stale.value = saved != null && problem is RequestProblem.Offline
                        pages.update { if (saved != null) it.with(saved).copy(problem = problem) else it.copy(loading = false, problem = problem) }
                    }
                }
            }
        }
    }

    /** Reads on while [list]'s row is short of a screenful. */
    private suspend fun fill(list: DiscoverList) {
        var auto = 0
        while (rows.getValue(list).value.short(ROW_SCREENFUL) && auto++ < MAX_AUTO_PAGES) {
            if (!next(list)) break
        }
    }

    /** [list]'s row scrolled near its end: its next page, if there is one. */
    fun more(list: DiscoverList) {
        val pages = rows.getValue(list).value
        if (!pages.more || pages.loading || pages.page == 0) return
        viewModelScope.launch { next(list) }
    }

    /** Reads [list]'s next page; whether it came. */
    private suspend fun next(list: DiscoverList): Boolean {
        val pages = rows.getValue(list)
        val before = pages.value
        if (before.loading) return false
        pages.value = before.copy(loading = true, problem = null)
        return try {
            val answer = requests.page(list, before.page + 1)
            pages.update { it.with(answer) }
            true
        } catch (e: CancellationException) {
            pages.update { it.copy(loading = false) }
            throw e
        } catch (e: Exception) {
            pages.update { it.copy(loading = false, problem = RequestProblem.of(e)) }
            false
        }
    }

    /** Tries again what failed: the summary, the rows that couldn't be read, the search. */
    fun retry() {
        requests.refresh()
        for ((list, pages) in rows) {
            if (pages.value.problem == null) continue
            rowJobs.remove(list)?.cancel()
            pages.update { it.copy(problem = null, loading = false) }
            showRow(list)
        }
        retries.update { it + 1 }
    }

    // ---------------------------------------------------------------- what changed

    /** Badges in rows and results already shown follow [change]'s request. */
    private suspend fun patch(change: RequestChange) {
        val shown = sequence {
            for (pages in rows.values) yieldAll(pages.value.titles)
            (_search.value as? DiscoverSearch.Ready)?.let { yieldAll(it.pages.titles) }
        }
        val patch = requests.patchFor(change, shown) ?: return
        for (pages in rows.values) pages.update { it.map(patch) }
        _search.update { s -> if (s is DiscoverSearch.Ready) s.copy(pages = s.pages.map(patch)) else s }
    }

    private suspend fun readYours() {
        try {
            yours.value = requests.list(RequestQuery(mine = true, limit = YOURS)).requests
        } catch (e: CancellationException) {
            throw e
        } catch (_: Exception) {
            // A shortcut: it stays as it was.
        }
    }

    // ---------------------------------------------------------------- the hero's action

    /**
     * The hero's main action was pressed: a movie is asked for here; a
     * show opens its page with the picker; what a library has (some of it)
     * opens there; the rest open the title.
     */
    fun heroAction() {
        val hero = state.value.hero ?: return
        val title = hero.title
        val page = PleyKey.DiscoverTitle(title.kind, title.tmdbId)
        when (val action = hero.action) {
            is MainAction.Open -> send(RequestEvent.Open(action.itemId?.let { PleyKey.Item(it) } ?: page))
            is MainAction.Request -> when {
                action.ask != Ask.Yes -> send(RequestEvent.Open(page))
                title.kind == TitleKind.Movie -> ask(NewRequest(TitleKind.Movie, title.tmdbId), title.title)
                else -> send(RequestEvent.Open(page.copy(pick = true)))
            }
            is MainAction.Partial -> send(RequestEvent.Open(action.itemId?.let { PleyKey.Item(it) } ?: page.copy(pick = action.more == Ask.Yes)))
            is MainAction.TellMeToo ->
                if (action.ask == Ask.Yes && title.kind == TitleKind.Movie) ask(NewRequest(TitleKind.Movie, title.tmdbId), title.title)
                else send(RequestEvent.Open(page))
            else -> send(RequestEvent.Open(page))
        }
    }

    /** Asks for [request], once at a time: a second press while it's on its way does nothing. */
    private fun ask(request: NewRequest, title: String) {
        if (!asking.compareAndSet(false, true)) return
        viewModelScope.launch {
            try {
                val asked = attempt({ eventChannel.send(RequestEvent.Failed(it)) }) { requests.ask(request) } ?: return@launch
                eventChannel.send(RequestEvent.Asked(asked, title))
            } finally {
                asking.value = false
            }
        }
    }

    private fun send(event: RequestEvent) {
        eventChannel.trySend(event)
    }

    companion object {
        /** How long typing pauses before Discover's search asks. */
        const val DEBOUNCE_MS = 250L

        /** Changes gathered before "Your requests" is read again. */
        const val CHANGES_MS = 500L

        /** Keeps loading this long after the page stops showing. */
        const val STOP_MS = 5_000L

        /** A row's screenful: what fills a phone's or a tablet's width, and a little more. */
        const val ROW_SCREENFUL = 12

        /** Search's screenful: a grid of posters. */
        const val SEARCH_SCREENFUL = 20

        /** Pages read on their own, one after another, before waiting for a scroll. */
        const val MAX_AUTO_PAGES = 4

        /** "Your requests" on Discover: the newest. */
        const val YOURS = 12
    }
}
