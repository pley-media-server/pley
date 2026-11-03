package app.pley.android.screens.requests

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import app.pley.android.data.RequestChange
import app.pley.android.data.RequestsRepository
import app.pley.android.data.RequestsState
import app.pley.android.data.mayAsk
import app.pley.android.model.DiscoverList
import app.pley.android.model.DiscoverPage
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

/** TMDB's titles for the words searched, or of one of Discover's lists. */
sealed interface DiscoverSearch {
    data class Loading(val query: String) : DiscoverSearch

    data class Ready(val query: String, val pages: Pages) : DiscoverSearch

    data class Empty(val query: String) : DiscoverSearch

    data class Failed(val query: String, val problem: RequestProblem) : DiscoverSearch
}

/** What a grid of TMDB's titles shows: what TMDB finds for some words, or one of Discover's lists whole. */
sealed interface DiscoverSource {
    /** Every title TMDB finds for [words], those a library has too (search's Discover row's See all). */
    data class Words(val words: String) : DiscoverSource

    /** Every title of [list] (a Discover row's See all, on TVs). */
    data class Row(val list: DiscoverList) : DiscoverSource
}

/** The page of a grid of TMDB's titles. */
data class DiscoverResultsUiState(
    /** What is searched for: the words as the server takes them; empty for a list. */
    val query: String,
    val gate: RequestsState = RequestsState.Unknown,
    val origin: String = "",
    /** Why the page can't show: signed in before requests, requests off, no access. */
    val problem: RequestProblem? = null,
    val search: DiscoverSearch = DiscoverSearch.Loading(query),
    /** The list shown whole, for a Discover row's See all; `null` for search's. */
    val list: DiscoverList? = null,
)

/**
 * A grid of TMDB's titles: the See all of Search's Discover row, every
 * title TMDB finds for the words (cut to what the server takes), or on
 * TVs the See all of a Discover row ([DiscoverSource]); each with its
 * badge, those in a library too. It reads page by page until a
 * [screenful], [fill] pages at most, then as the grid scrolls: a page at
 * a time ([more], phones), or [fill] pages at most until the grid has
 * what it wants ([want], TVs), stopping short then until Show more
 * ([showMore]). Badges follow requests_changed without reading the
 * results again.
 */
class DiscoverResultsViewModel(
    private val source: DiscoverSource,
    private val deps: RequestsDeps,
    private val screenful: Int = SCREENFUL,
    private val fill: Int = DiscoverViewModel.MAX_AUTO_PAGES,
) : ViewModel() {
    /** Search's See all on phones and tablets: every title TMDB finds for [words]. */
    constructor(words: String, deps: RequestsDeps) : this(DiscoverSource.Words(words), deps)

    private val requests = deps.requests
    private val query = (source as? DiscoverSource.Words)?.let { RequestsRepository.searchText(it.words) }.orEmpty()
    private val list = (source as? DiscoverSource.Row)?.list
    private val retries = MutableStateFlow(0)
    private val scrolls = MutableStateFlow(0)
    private val wanted = MutableStateFlow(screenful)
    private val goOns = MutableStateFlow(0)
    private val search = MutableStateFlow<DiscoverSearch>(DiscoverSearch.Loading(query))

    val state: StateFlow<DiscoverResultsUiState> = combine(
        requests.state,
        deps.origin.map { it.orEmpty() }.distinctUntilChanged(),
        search,
    ) { gate, origin, search -> DiscoverResultsUiState(query, gate, origin, RequestProblem.of(gate), search, list) }
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(DiscoverViewModel.STOP_MS), DiscoverResultsUiState(query, list = list))

    init {
        viewModelScope.launch {
            // TMDB is asked only while asking works.
            combine(requests.state.map { it.mayAsk }.distinctUntilChanged(), retries) { open, _ -> open }
                .collectLatest { open -> if (open) searchFor() }
        }
        viewModelScope.launch { requests.changes.gathered().collect { batch -> batch.forEach { patch(it) } } }
    }

    private suspend fun page(n: Int): DiscoverPage = when (source) {
        is DiscoverSource.Words -> requests.search(query, n)
        is DiscoverSource.Row -> requests.page(source.list, n)
    }

    private suspend fun searchFor() {
        if (source is DiscoverSource.Words && query.isEmpty()) {
            search.value = DiscoverSearch.Empty(query)
            return
        }
        var scrolled = scrolls.value
        var goneOn = goOns.value
        search.value = DiscoverSearch.Loading(query)
        // A short page isn't the end: read on until a screenful.
        if (!fillTo(Pages(), screenful)) return
        // Then more as the grid scrolls near its end, and past a stop when asked to.
        combine(scrolls, wanted, goOns) { asked, want, goOn -> Triple(asked, want, goOn) }.collect { (asked, want, goOn) ->
            val shown = search.value as? DiscoverSearch.Ready ?: return@collect
            when {
                goOn != goneOn -> {
                    goneOn = goOn
                    fillTo(shown.pages.copy(stopped = false, problem = null), maxOf(want, shown.pages.titles.size + 1))
                }
                asked != scrolled -> {
                    scrolled = asked
                    if (!shown.pages.loading) fillTo(shown.pages.copy(stopped = false, problem = null), shown.pages.titles.size + 1, pages = 1)
                }
                !shown.pages.stopped && shown.pages.problem == null -> fillTo(shown.pages, want)
            }
        }
    }

    /**
     * Reads the pages after [start]'s while fewer than [want] titles came,
     * [pages] of them at most; short of [want] with more to read, it stops
     * there ([Pages.stopped]). Whether the grid shows: false when a page
     * failed before any title came.
     */
    private suspend fun fillTo(start: Pages, want: Int, pages: Int = fill): Boolean {
        var shown = start
        var read = 0
        while (shown.more && shown.titles.size < want) {
            if (read == pages) {
                shown = shown.copy(stopped = true)
                break
            }
            if (shown.page > 0) search.value = DiscoverSearch.Ready(query, shown.copy(loading = true))
            val answer = try {
                page(shown.page + 1)
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                // What came before shows, with why the rest didn't; with nothing, why.
                val problem = RequestProblem.of(e)
                search.value = if (shown.titles.isEmpty()) DiscoverSearch.Failed(query, problem) else DiscoverSearch.Ready(query, shown.copy(problem = problem))
                return shown.titles.isNotEmpty()
            }
            shown = shown.with(answer)
            read++
            search.value = shown(shown)
        }
        search.value = shown(shown)
        return true
    }

    private fun shown(pages: Pages): DiscoverSearch =
        if (pages.titles.isEmpty() && !pages.more) DiscoverSearch.Empty(query) else DiscoverSearch.Ready(query, pages)

    /** The grid scrolled near its end: its next page, if there is one. */
    fun more() {
        scrolls.update { it + 1 }
    }

    /** The grid wants [count] titles (focus came near its end): pages are read until it has them, unless the reading stopped short. */
    fun want(count: Int) {
        wanted.update { maxOf(it, count) }
    }

    /** Show more, or Retry, at the grid's end: the reading goes on past where it stopped short or failed. */
    fun showMore() {
        goOns.update { it + 1 }
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
        /** A screenful: a phone's grid of posters. */
        const val SCREENFUL = 20

        /** Pages a TV's grid reads at a time, at most, before it offers Show more (as the Samsung and LG apps do). */
        const val TV_FILL = 5
    }
}
