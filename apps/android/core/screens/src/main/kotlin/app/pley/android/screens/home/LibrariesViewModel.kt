package app.pley.android.screens.home

import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.viewModelScope
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import app.pley.android.data.LibrariesRepository
import app.pley.android.data.Loadable
import app.pley.android.data.ScanState
import app.pley.android.data.Session
import app.pley.android.data.valueOrNull
import app.pley.android.model.Library
import app.pley.android.network.ApiException
import app.pley.android.screens.account.MeViewModel
import app.pley.android.screens.playlists.Playlists
import app.pley.android.screens.screensGraph
import kotlin.coroutines.cancellation.CancellationException
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.emitAll
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.flowOn
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.receiveAsFlow
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch

/** The list of libraries (the phone's Libraries tab, the TV's Libraries page). */
sealed interface LibrariesState {
    /** Nothing known yet. */
    data object Loading : LibrariesState

    /** The list couldn't be loaded and none was saved: "Couldn't load your libraries". */
    data class Failed(val error: Throwable) : LibrariesState

    /** The libraries in the server's order, with moves not saved yet; empty: "No libraries yet". */
    data class Ready(
        val libraries: List<LibraryEntry>,
        val fresh: Boolean,
        /** The server couldn't be asked again: the list is what was saved. */
        val error: Throwable? = null,
        val refreshing: Boolean = false,
        /** They can be put in another order: there are two or more, and the sign-in may. */
        val sortable: Boolean = false,
    ) : LibrariesState
}

/** Something to tell the person, once. */
sealed interface LibrariesEvent {
    /** A new order wasn't saved: the list is back in the server's. */
    data class Failed(val error: Throwable) : LibrariesEvent

    /** The server is older than libraries' own order: the list is back in its order. */
    data object NeedsNewerServer : LibrariesEvent
}

/**
 * The libraries this profile sees, in the server's order, each with its
 * kind, its name, how much it holds and whether the server is scanning it
 * (the server's events, and the list's own `scanning`).
 *
 * The profile may put them in its own order ([move]), which every list of
 * them follows on all its devices: a move shows at once, and is saved in
 * the order moves were made.
 */
class LibrariesViewModel(
    private val libraries: () -> LibrariesRepository,
    private val scans: () -> StateFlow<List<ScanState>>,
    private val compute: CoroutineDispatcher = Dispatchers.Default,
    private val session: () -> Flow<Session?> = { flowOf(null) },
) : ViewModel() {
    private val refreshing = MutableStateFlow(false)

    /** The libraries' ids in the order shown while moves are made or saved; `null`: the server's. */
    private val order = MutableStateFlow<List<Long>?>(null)

    /** Orders to save, one at a time. */
    private val orders = Channel<List<Long>>(Channel.UNLIMITED)
    private val eventChannel = Channel<LibrariesEvent>(Channel.BUFFERED)
    val events: Flow<LibrariesEvent> = eventChannel.receiveAsFlow()

    val state: StateFlow<LibrariesState> = combine(
        flow { emitAll(libraries().libraries()) },
        flow { emitAll(scans()) },
        refreshing,
        order,
        flow { emitAll(session().map(MeViewModel::mayChange).distinctUntilChanged()) },
    ) { list, scans, refreshing, order, mayChange -> librariesState(list, scans, refreshing, order, mayChange) }
        .distinctUntilChanged()
        .flowOn(compute)
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), LibrariesState.Loading)

    init {
        viewModelScope.launch(compute) {
            for (ids in orders) save(ids)
        }
    }

    /** Asks the server again (pull to refresh, Retry). */
    fun refresh() {
        if (refreshing.value) return
        refreshing.value = true
        viewModelScope.launch(compute) {
            try {
                libraries().refresh()
            } finally {
                refreshing.value = false
            }
        }
    }

    /**
     * Moves the library at [from] to [to]: shown at once, and saved unless
     * [save] is false (a TV's move mode saves once, when it ends).
     */
    fun move(from: Int, to: Int, save: Boolean = true) {
        val ids = shownOrder() ?: return
        if (from !in ids.indices || to !in ids.indices || from == to) return
        val next = Playlists.moved(ids, from, to)
        order.value = next
        if (save) orders.trySend(next)
    }

    /** Moves [libraryId] [by] places, from where it is shown now (moves not yet drawn included). */
    fun moveBy(libraryId: Long, by: Int, save: Boolean = true) {
        val from = shownOrder()?.indexOf(libraryId) ?: return
        if (from >= 0) move(from, from + by, save)
    }

    /** Saves the order shown now (after moves made with `save = false`). */
    fun saveOrder() {
        order.value?.let { orders.trySend(it) }
    }

    /** Forgets moves not saved: the server's order again. */
    fun cancelMoves() {
        order.value = null
    }

    /** The order everyone sees, for this profile again. */
    fun useServerOrder() {
        order.value = null
        viewModelScope.launch(compute) { attempt { libraries().forgetOrder() } }
    }

    /** The libraries' ids as shown, moves the list hasn't drawn yet included; `null` when they can't move. */
    private fun shownOrder(): List<Long>? {
        val ready = state.value as? LibrariesState.Ready ?: return null
        if (!ready.sortable) return null
        val shown = ready.libraries.map { it.library.id }
        // The libraries there are, in the order of the last move.
        return order.value?.let { ids -> ids.filter { it in shown } + shown.filter { it !in ids } } ?: shown
    }

    private suspend fun save(ids: List<Long>) {
        if (attempt { libraries().sort(ids) }) {
            // The server's answer holds this order; a newer move keeps its own.
            order.compareAndSet(ids, null)
        } else {
            order.value = null
        }
    }

    /** Runs [change]; `false` after telling of its failure. */
    private suspend fun attempt(change: suspend () -> Unit): Boolean = try {
        change()
        true
    } catch (e: CancellationException) {
        throw e
    } catch (e: Exception) {
        // A server from before PUT /api/me/libraries/order has no such route.
        val older = e is ApiException && (e.status == 404 || e.status == 405)
        eventChannel.trySend(if (older) LibrariesEvent.NeedsNewerServer else LibrariesEvent.Failed(e))
        false
    }

    override fun onCleared() {
        orders.close()
    }

    companion object {
        val Factory: ViewModelProvider.Factory = viewModelFactory {
            initializer {
                val graph = screensGraph()
                LibrariesViewModel(
                    { graph.data.libraries },
                    { graph.data.liveUpdates.scans },
                    session = { graph.data.session.session },
                )
            }
        }
    }
}

/**
 * The list from what the repository holds and the scans running, in
 * [order] (the libraries' ids, those it doesn't name after) while moves
 * are made or saved. [mayChange]: the sign-in may change the profile's
 * choices, its libraries' order among them.
 */
fun librariesState(
    libraries: Loadable<List<Library>>,
    scans: List<ScanState>,
    refreshing: Boolean = false,
    order: List<Long>? = null,
    mayChange: Boolean = false,
): LibrariesState {
    val list = libraries.valueOrNull
        ?: return if (libraries is Loadable.Failed) LibrariesState.Failed(libraries.error) else LibrariesState.Loading
    val scanning = scans.mapNotNull { it.libraryId }.toSet()
    val shown = if (order == null) {
        list
    } else {
        val at = order.withIndex().associate { (i, id) -> id to i }
        list.sortedBy { at[it.id] ?: Int.MAX_VALUE }
    }
    // A scan the server didn't say the library of is shown on none: the list's own flag says which.
    return LibrariesState.Ready(
        libraries = shown.map { LibraryEntry(it, it.scanning || it.id in scanning) },
        fresh = libraries is Loadable.Ready && libraries.fresh,
        error = (libraries as? Loadable.Failed)?.error,
        refreshing = refreshing,
        sortable = mayChange && list.size > 1,
    )
}
