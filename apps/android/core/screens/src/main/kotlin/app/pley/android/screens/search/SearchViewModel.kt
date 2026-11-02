@file:OptIn(ExperimentalCoroutinesApi::class, FlowPreview::class)

package app.pley.android.screens.search

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import app.pley.android.data.Loadable
import app.pley.android.data.RequestsRepository
import app.pley.android.data.SearchRepository
import app.pley.android.data.mayAsk
import app.pley.android.data.summary
import app.pley.android.model.DiscoverTitle
import app.pley.android.model.ItemKind
import app.pley.android.model.ItemSummary
import app.pley.android.model.LiveChannel
import app.pley.android.model.PersonSummary
import app.pley.android.model.RequestSummary
import app.pley.android.model.SearchResults
import app.pley.android.screens.PlayerLauncher
import app.pley.android.screens.PleyKey
import app.pley.android.screens.ScreensGraph
import app.pley.android.screens.library.LibraryViewModel
import app.pley.android.screens.library.TileFactory
import app.pley.android.screens.library.TileLook
import app.pley.android.screens.livetv.ChannelHandoff
import app.pley.android.screens.requests.Pages
import app.pley.android.screens.requests.RequestProblem
import app.pley.android.screens.requests.gathered
import app.pley.android.screens.requests.patchFor
import java.util.IdentityHashMap
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.FlowPreview
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.debounce
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.flowOn
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.onStart
import kotlinx.coroutines.flow.runningFold
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

/**
 * What search reads: the server's answers, recent searches, the server's
 * address for pictures, and [requests] for what TMDB has beside them;
 * [launcher] plays a Live TV channel found.
 */
class SearchDeps(
    val search: SearchRepository,
    val origin: Flow<String?>,
    val compute: CoroutineDispatcher = Dispatchers.Default,
    val launcher: PlayerLauncher? = null,
    val requests: RequestsRepository = RequestsRepository.None,
) {
    companion object {
        fun of(graph: ScreensGraph): SearchDeps =
            SearchDeps(graph.data.search, graph.data.servers.active, launcher = graph.playerLauncher, requests = graph.data.requests)
    }
}

/**
 * The Discover row: the first of TMDB's titles for the query
 * that no library has, after the libraries' results, while asking works.
 */
sealed interface TmdbSection {
    data object Hidden : TmdbSection

    data class Loading(val query: String) : TmdbSection

    /** [titles], their pictures on the server at [origin]. */
    data class Ready(val query: String, val titles: List<DiscoverTitle>, val origin: String = "") : TmdbSection

    /** TMDB couldn't be searched: [problem] says why, in words for [summary]'s account (an administrator fixes a refused key). */
    data class Failed(val query: String, val problem: RequestProblem, val summary: RequestSummary? = null) : TmdbSection
}

/**
 * The tiles of search results: [TileFactory]'s for items, and a person's
 * (a circle with their photo and name).
 */
interface SearchTiles<T : Any> {
    fun item(item: ItemSummary, look: TileLook, origin: String): T

    fun person(person: PersonSummary, origin: String): T

    companion object {
        /** Items from [items], people from [person]. */
        fun <T : Any> of(items: TileFactory<T>, person: (PersonSummary, String) -> T): SearchTiles<T> = object : SearchTiles<T> {
            override fun item(item: ItemSummary, look: TileLook, origin: String): T = items.item(item, look, origin)

            override fun person(person: PersonSummary, origin: String): T = person(person, origin)
        }
    }
}

/**
 * The groups results come in, in the order they show (the web's
 * SearchPage): people first, then each kind of item. [look] is how its
 * items are drawn; people are circles.
 */
enum class SearchGroupKind(val look: TileLook, internal val kind: ItemKind?) {
    People(TileLook.Poster, null),
    Movies(TileLook.Poster, ItemKind.Movie),
    Shows(TileLook.Poster, ItemKind.Show),
    Seasons(TileLook.Poster, ItemKind.Season),
    Episodes(TileLook.Landscape, ItemKind.Episode),
    Artists(TileLook.Square, ItemKind.Artist),
    Albums(TileLook.Square, ItemKind.Album),
    Tracks(TileLook.Square, ItemKind.Track),
}

/** One group of results: its [tiles], [ids] beside them. */
data class SearchGroup<T : Any>(
    val kind: SearchGroupKind,
    val tiles: List<T>,
    val ids: List<Long>,
) {
    val count: Int get() = tiles.size
}

/** What the results part of the page shows. */
sealed interface SearchResultsState<out T : Any> {
    /** Nothing typed: recent searches, or what search finds. */
    data object Idle : SearchResultsState<Nothing>

    /** The first answer for [query] is on its way, with nothing earlier to keep showing. */
    data class Loading(val query: String) : SearchResultsState<Nothing>

    /**
     * Results for [query], grouped, with the Live TV [channels] found (shown
     * after people, as on the web; their logos are on [origin]). While
     * [loading], a newer query's answer is on its way and these stay until
     * it comes.
     */
    data class Ready<T : Any>(
        val query: String,
        val groups: List<SearchGroup<T>>,
        val loading: Boolean,
        val channels: List<LiveChannel> = emptyList(),
        val origin: String = "",
    ) : SearchResultsState<T>

    /** `No results for "north"`. */
    data class Empty(val query: String) : SearchResultsState<Nothing>

    /** "Search failed", with the error's sentence. */
    data class Failed(val query: String, val error: Throwable) : SearchResultsState<Nothing>
}

/** The page: what is typed, the profile's recent searches (newest first) and the results. */
data class SearchUiState<T : Any>(
    val query: String,
    val recent: List<String>,
    val results: SearchResultsState<T>,
)

/**
 * Search (apps/web/src/pages/SearchPage.tsx; design spec §4.5), shared by
 * phones and TVs.
 *
 * What is typed is asked for once typing has paused 250 ms, and only the
 * latest query's answer counts: a newer one replaces a request still on
 * its way. The results already shown stay while the next ones load.
 * Results come in groups: People, Live TV, Movies, Shows, Seasons,
 * Episodes, Artists, Albums, Tracks, each with its count. A query is kept among the
 * profile's last 10 searches when it is submitted or one of its results
 * is opened.
 */
class SearchViewModel<T : Any>(
    initialQuery: String,
    private val deps: SearchDeps,
    private val tiles: SearchTiles<T>,
) : ViewModel() {
    private val typed = MutableStateFlow(initialQuery)

    /** Retry after a failed search: the same query, asked again. */
    private val retries = MutableStateFlow(0)

    /** What is typed now, as the field shows it. */
    val query: StateFlow<String> = typed.asStateFlow()

    /** The items and people of the results shown, by id, for opening them. */
    @Volatile
    private var shown: Shown = Shown("", emptyMap(), emptySet())

    private class Shown(val query: String, val items: Map<Long, ItemSummary>, val people: Set<Long>)

    private val results: Flow<SearchResultsState<T>> = typed
        .map { it.trim() }
        .distinctUntilChanged()
        .debounce { if (it.isEmpty()) 0L else DEBOUNCE_MS }
        .combine(retries) { q, _ -> q }
        .flatMapLatest { q ->
            if (q.isEmpty()) {
                flowOf<Step<T>>(Step.Idle)
            } else {
                combine(deps.search.search(q), deps.origin.map { it.orEmpty() }.distinctUntilChanged()) { load, origin -> step(q, load, origin) }
                    .onStart { emit(Step.Asking(q)) }
            }
        }
        .runningFold<Step<T>, SearchResultsState<T>>(SearchResultsState.Idle) { before, step -> next(before, step) }
        .distinctUntilChanged()
        .flowOn(deps.compute)

    val state: StateFlow<SearchUiState<T>> = combine(typed, deps.search.recent, results) { q, recent, results ->
        SearchUiState(q, recent, results)
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(STOP_AFTER_MS), SearchUiState(initialQuery, emptyList(), SearchResultsState.Idle))

    /** Retry after the Discover row failed: TMDB asked again. */
    private val tmdbRetries = MutableStateFlow(0)

    /**
     * The Discover row, asked for beside the libraries' search, never before it
     * nor holding it up: TMDB's first titles that no library has
     * (`missing=1`), their badges following requests_changed. The query is
     * cut to what the server takes. Hidden when asking doesn't work or TMDB
     * has none; when TMDB couldn't be searched, it says why.
     */
    val tmdb: StateFlow<TmdbSection> = combine(
        typed.map { RequestsRepository.searchText(it) }.distinctUntilChanged().debounce { if (it.isEmpty()) 0L else DEBOUNCE_MS },
        deps.requests.state.map { it.mayAsk }.distinctUntilChanged(),
    ) { q, asking -> if (asking) q else "" }
        .distinctUntilChanged()
        .combine(tmdbRetries) { q, _ -> q }
        .flatMapLatest { q -> if (q.isEmpty()) flowOf(TmdbSection.Hidden) else onTmdb(q) }
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(STOP_AFTER_MS), TmdbSection.Hidden)

    private fun onTmdb(q: String): Flow<TmdbSection> = flow {
        emit(TmdbSection.Loading(q))
        // A short page, even an empty one (its titles all in a library), isn't the end: read on for enough to show.
        var pages = Pages()
        var read = 0
        try {
            do {
                pages = pages.with(deps.requests.search(q, pages.page + 1, missing = true))
            } while (pages.short(TMDB_SHOWN) && ++read < TMDB_PAGES)
        } catch (e: CancellationException) {
            throw e
        } catch (e: Exception) {
            // What came before shows; with nothing, why.
            if (pages.titles.isEmpty()) {
                emit(TmdbSection.Failed(q, RequestProblem.of(e), deps.requests.state.value.summary))
                return@flow
            }
        }
        var titles = pages.titles.take(TMDB_SHOWN)
        if (titles.isEmpty()) {
            emit(TmdbSection.Hidden)
            return@flow
        }
        val origin = deps.origin.first().orEmpty()
        emit(TmdbSection.Ready(q, titles, origin))
        deps.requests.changes.gathered().collect { batch ->
            val before = titles
            for (change in batch) {
                val patch = deps.requests.patchFor(change, titles.asSequence()) ?: continue
                titles = titles.map(patch)
            }
            if (titles != before) emit(TmdbSection.Ready(q, titles, origin))
        }
    }

    /** The field changed. */
    fun onQueryChange(text: String) {
        typed.value = text
    }

    /** Asks for the query again, after it failed. */
    fun retry() = retries.update { it + 1 }

    /** Asks TMDB again, after the Discover row failed. */
    fun retryTmdb() = tmdbRetries.update { it + 1 }

    /** Search was pressed on the keyboard: the query is kept among the recent ones. */
    fun submit() {
        val q = typed.value.trim()
        if (q.isNotEmpty()) viewModelScope.launch { deps.search.remember(q) }
    }

    /** A recent search was chosen: it is searched again, and moves to the top. */
    fun searchAgain(query: String) {
        typed.value = query
        submit()
    }

    fun clearRecent() {
        viewModelScope.launch { deps.search.clearRecent() }
    }

    /**
     * Where opening the result [id] of [group] goes; the query is kept
     * among the recent searches, since one of its results was wanted.
     */
    fun open(group: SearchGroupKind, id: Long): PleyKey? {
        val now = shown
        val key = if (group == SearchGroupKind.People) {
            PleyKey.Person(id).takeIf { id in now.people }
        } else {
            now.items[id]?.let(::opens)
        }
        if (key != null && now.query.isNotEmpty()) viewModelScope.launch { deps.search.remember(now.query) }
        return key
    }

    /**
     * Plays the Live TV [channel] found, changing channel among every
     * channel (as a search in Live TV does); the query is kept among the
     * recent searches.
     */
    fun watch(channel: LiveChannel) {
        val q = shown.query
        if (q.isNotEmpty()) viewModelScope.launch { deps.search.remember(q) }
        ChannelHandoff.give(channel)
        deps.launcher?.watchLive(channel.id, "all")
    }

    // The steps from what is typed to what shows.

    private sealed interface Step<out T : Any> {
        data object Idle : Step<Nothing>

        data class Asking(val query: String) : Step<Nothing>

        data class Answer<T : Any>(
            val query: String,
            val groups: List<SearchGroup<T>>,
            val stillLoading: Boolean,
            val channels: List<LiveChannel> = emptyList(),
            val origin: String = "",
        ) : Step<T>

        data class Failed(val query: String, val error: Throwable) : Step<Nothing>
    }

    private fun step(q: String, load: Loadable<SearchResults>, origin: String): Step<T> = when (load) {
        Loadable.Loading -> Step.Asking(q)
        is Loadable.Ready -> answer(q, load.value, origin)
        is Loadable.Failed -> load.stale?.let { answer(q, it, origin) } ?: Step.Failed(q, load.error)
    }

    private fun answer(q: String, results: SearchResults, origin: String): Step<T> =
        Step.Answer(q, groups(q, results, origin), stillLoading = false, channels = results.channels, origin = origin)

    private fun next(before: SearchResultsState<T>, step: Step<T>): SearchResultsState<T> = when (step) {
        Step.Idle -> SearchResultsState.Idle
        is Step.Asking -> when (before) {
            // Keep showing what is there while the new answer comes.
            is SearchResultsState.Ready -> before.copy(loading = true)
            else -> SearchResultsState.Loading(step.query)
        }
        is Step.Answer -> if (step.groups.isEmpty() && step.channels.isEmpty()) {
            SearchResultsState.Empty(step.query)
        } else {
            SearchResultsState.Ready(step.query, step.groups, loading = step.stillLoading, channels = step.channels, origin = step.origin)
        }
        is Step.Failed -> SearchResultsState.Failed(step.query, step.error)
    }

    /** Tiles already made, by the item or person they show, so a new answer only makes what is new. */
    private var made = IdentityHashMap<Any, T>()
    private var madeFor = ""

    private fun groups(q: String, results: SearchResults, origin: String): List<SearchGroup<T>> {
        if (madeFor != origin) {
            made = IdentityHashMap()
            madeFor = origin
        }
        val next = IdentityHashMap<Any, T>()
        val grouped = searchGroups(results)
        val out = grouped.map { (kind, entries) ->
            val ids = ArrayList<Long>(entries.size)
            val built = entries.map { entry ->
                val tile = made[entry] ?: when (entry) {
                    is PersonSummary -> tiles.person(entry, origin)
                    is ItemSummary -> tiles.item(entry, kind.look, origin)
                    else -> error("not a result: $entry")
                }
                next[entry] = tile
                ids += if (entry is PersonSummary) entry.id else (entry as ItemSummary).id
                tile
            }
            SearchGroup(kind, built, ids)
        }
        made = next
        shown = Shown(q, results.items.associateBy { it.id }, results.people.mapTo(HashSet()) { it.id })
        return out
    }

    companion object {
        /** How long typing pauses before what is typed is searched. */
        const val DEBOUNCE_MS = 250L

        /** How long the results keep being followed after the page stops showing. */
        const val STOP_AFTER_MS = 5_000L

        /** How many of TMDB's titles the Discover row shows; its See all has the rest. */
        const val TMDB_SHOWN = 12

        /** The most of TMDB's pages the Discover row reads for them. */
        const val TMDB_PAGES = 3

        /**
         * [results] in their groups and the groups' order (people, movies,
         * shows, seasons, episodes, artists, albums, tracks), each keeping
         * the server's order of relevance; empty groups left out. Items of
         * other kinds (extras) aren't shown, as on the web.
         */
        fun searchGroups(results: SearchResults): List<Pair<SearchGroupKind, List<Any>>> = buildList {
            if (results.people.isNotEmpty()) add(SearchGroupKind.People to results.people)
            val byKind = results.items.groupBy { it.kind }
            for (kind in SearchGroupKind.entries) {
                val items = kind.kind?.let(byKind::get).orEmpty()
                if (items.isNotEmpty()) add(kind to items)
            }
        }

        /** Where a result opens: artists and albums their music pages, a track its album, the rest their item page. */
        fun opens(item: ItemSummary): PleyKey = when (item.kind) {
            ItemKind.Track -> item.parentId?.let { PleyKey.Album(it) } ?: PleyKey.Item(item.id)
            else -> LibraryViewModel.opens(item)
        }
    }
}
