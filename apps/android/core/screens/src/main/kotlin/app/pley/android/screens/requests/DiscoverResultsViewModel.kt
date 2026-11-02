package app.pley.android.screens.requests

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import app.pley.android.data.RequestChange
import app.pley.android.data.RequestsRepository
import app.pley.android.data.RequestsState
import app.pley.android.data.mayAsk
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

/** TMDB's titles for the words searched. */
sealed interface DiscoverSearch {
    data class Loading(val query: String) : DiscoverSearch

    data class Ready(val query: String, val pages: Pages) : DiscoverSearch

    data class Empty(val query: String) : DiscoverSearch

    data class Failed(val query: String, val problem: RequestProblem) : DiscoverSearch
}

/** The page of search's Discover row's See all. */
data class DiscoverResultsUiState(
    /** What is searched for: the words as the server takes them. */
    val query: String,
    val gate: RequestsState = RequestsState.Unknown,
    val origin: String = "",
    /** Why the page can't show: signed in before requests, requests off, no access. */
    val problem: RequestProblem? = null,
    val search: DiscoverSearch = DiscoverSearch.Loading(query),
)

/**
 * The See all of Search's Discover row, on phones and tablets:
 * every title TMDB finds for [words] (cut to what the server takes), each
 * with its badge, those in a library too; read page by page until a
 * screenful, then as the grid scrolls ([more]). Badges follow
 * requests_changed without reading the results again.
 */
class DiscoverResultsViewModel(
    words: String,
    private val deps: RequestsDeps,
) : ViewModel() {
    private val requests = deps.requests
    private val query = RequestsRepository.searchText(words)
    private val retries = MutableStateFlow(0)
    private val scrolls = MutableStateFlow(0)
    private val search = MutableStateFlow<DiscoverSearch>(DiscoverSearch.Loading(query))

    val state: StateFlow<DiscoverResultsUiState> = combine(
        requests.state,
        deps.origin.map { it.orEmpty() }.distinctUntilChanged(),
        search,
    ) { gate, origin, search -> DiscoverResultsUiState(query, gate, origin, RequestProblem.of(gate), search) }
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(DiscoverViewModel.STOP_MS), DiscoverResultsUiState(query))

    init {
        viewModelScope.launch {
            // TMDB is asked only while asking works.
            combine(requests.state.map { it.mayAsk }.distinctUntilChanged(), retries) { open, _ -> open }
                .collectLatest { open -> if (open) searchFor() }
        }
        viewModelScope.launch { requests.changes.gathered().collect { batch -> batch.forEach { patch(it) } } }
    }

    private suspend fun searchFor() {
        if (query.isEmpty()) {
            search.value = DiscoverSearch.Empty(query)
            return
        }
        val scrolled = scrolls.value
        search.value = DiscoverSearch.Loading(query)
        var pages = Pages()
        var auto = 0
        do {
            val answer = try {
                requests.search(query, pages.page + 1)
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                val problem = RequestProblem.of(e)
                search.value = if (pages.titles.isEmpty()) DiscoverSearch.Failed(query, problem) else DiscoverSearch.Ready(query, pages.copy(problem = problem))
                return
            }
            pages = pages.with(answer)
            search.value = if (pages.titles.isEmpty() && !pages.more) DiscoverSearch.Empty(query) else DiscoverSearch.Ready(query, pages)
            // A short page isn't the end: read on until a screenful.
        } while (pages.short(SCREENFUL) && ++auto < DiscoverViewModel.MAX_AUTO_PAGES)
        // Then more as the grid scrolls near its end.
        scrolls.collect { asked ->
            if (asked == scrolled) return@collect
            val shown = search.value as? DiscoverSearch.Ready ?: return@collect
            if (!shown.pages.more || shown.pages.loading) return@collect
            search.value = shown.copy(pages = shown.pages.copy(loading = true, problem = null))
            search.value = try {
                DiscoverSearch.Ready(query, shown.pages.with(requests.search(query, shown.pages.page + 1)))
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                shown.copy(pages = shown.pages.copy(loading = false, problem = RequestProblem.of(e)))
            }
        }
    }

    /** The grid scrolled near its end: its next page, if there is one. */
    fun more() {
        scrolls.update { it + 1 }
    }

    /** Tries again: the summary, and the search from its first page. */
    fun retry() {
        requests.refresh()
        retries.update { it + 1 }
    }

    /** Badges in the results already shown follow [change]'s request. */
    private suspend fun patch(change: RequestChange) {
        val shown = (search.value as? DiscoverSearch.Ready)?.pages?.titles ?: return
        val patch = requests.patchFor(change, shown.asSequence()) ?: return
        search.update { s -> if (s is DiscoverSearch.Ready) s.copy(pages = s.pages.map(patch)) else s }
    }

    companion object {
        /** A screenful: a grid of posters. */
        const val SCREENFUL = 20
    }
}
