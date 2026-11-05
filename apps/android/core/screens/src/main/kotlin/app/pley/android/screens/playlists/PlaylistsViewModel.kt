package app.pley.android.screens.playlists

import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.viewModelScope
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import app.pley.android.common.AppLanguage
import app.pley.android.data.Loadable
import app.pley.android.data.PlaylistsRepository
import app.pley.android.data.map
import app.pley.android.model.PlaylistKind
import app.pley.android.screens.ScreensGraph
import app.pley.android.screens.screensGraph
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOn
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.receiveAsFlow
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

/** What the playlists' pages and sheets read and change. */
class PlaylistDeps(
    val playlists: PlaylistsRepository,
    val origin: Flow<String?>,
    val compute: CoroutineDispatcher = Dispatchers.Default,
    /** Where changes run: they outlive the page or sheet that asked for them. */
    val work: CoroutineScope = PlaylistWork,
) {
    companion object {
        fun of(graph: ScreensGraph): PlaylistDeps = PlaylistDeps(graph.data.playlists, graph.data.servers.active)
    }
}

/** A playlist was made: open it. Or making it failed. */
sealed interface CreateEvent {
    data class Created(val playlistId: Long, val title: String) : CreateEvent

    data class Failed(val error: Throwable) : CreateEvent
}

/**
 * The profile's playlists (apps/web/src/pages/playlists/PlaylistsPage.tsx):
 * a card each, by title, and New playlist (a title of at most 200
 * characters, for Movies & TV or Music), which opens the playlist made.
 */
@OptIn(ExperimentalCoroutinesApi::class)
class PlaylistsViewModel(private val deps: PlaylistDeps) : ViewModel() {
    private val retries = MutableStateFlow(0)
    private val eventChannel = Channel<CreateEvent>(Channel.BUFFERED)
    private val creating = MutableStateFlow(false)

    /** A playlist made (open it), or a failure to tell. */
    val events: Flow<CreateEvent> = eventChannel.receiveAsFlow()

    /** Whether a New playlist is on its way to the server. */
    val busy: StateFlow<Boolean> = creating

    val state: StateFlow<Loadable<List<PlaylistCard>>> = retries
        .flatMapLatest {
            // Made again when the language shown changes: the cards' durations are written in it.
            combine(deps.playlists.playlists(), deps.origin.map { it.orEmpty() }.distinctUntilChanged(), AppLanguage.shown) { load, origin, _ ->
                val words = AppLanguage.formatWords()
                load.map { list -> list.map { Playlists.card(it, origin, words) } }
            }
        }
        .flowOn(deps.compute)
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(STOP_AFTER_MS), Loadable.Loading)

    fun retry() = retries.update { it + 1 }

    /**
     * Makes a playlist called [title] for [kind]; `false` (and nothing
     * sent) when the title won't do or one is already being made.
     */
    fun create(title: String, kind: PlaylistKind): Boolean {
        if (Playlists.check(title) != null || !creating.compareAndSet(false, true)) return false
        val name = title.trim()
        deps.work.launch {
            try {
                val id = deps.playlists.create(name, kind)
                eventChannel.trySend(CreateEvent.Created(id, name))
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                eventChannel.trySend(CreateEvent.Failed(e))
            } finally {
                creating.value = false
            }
        }
        return true
    }

    companion object {
        const val STOP_AFTER_MS = 5_000L

        val Factory: ViewModelProvider.Factory = viewModelFactory {
            initializer { PlaylistsViewModel(PlaylistDeps.of(screensGraph())) }
        }
    }
}
