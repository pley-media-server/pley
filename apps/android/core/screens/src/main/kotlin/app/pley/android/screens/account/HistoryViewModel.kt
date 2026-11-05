package app.pley.android.screens.account

import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.viewModelScope
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import app.pley.android.common.AppLanguage
import app.pley.android.data.HistoryRepository
import app.pley.android.data.Loadable
import app.pley.android.data.map
import app.pley.android.data.valueOrNull
import app.pley.android.model.HistoryEntry
import app.pley.android.model.ItemKind
import app.pley.android.model.format.FormatWords
import app.pley.android.model.format.episodeLabel
import app.pley.android.model.format.formatRelative
import app.pley.android.screens.PleyKey
import app.pley.android.screens.screensGraph
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
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

/** A play in the history, as a row shows it. */
data class HistoryRow(
    val entryId: Long,
    val itemId: Long,
    /** The show for an episode, else the item's own title. */
    val title: String,
    /** "S2 · E5 · Glasshouse", "Artist · Album", or the year. */
    val subtitle: String,
    /** "3 hours ago". */
    val time: String,
    /** "Chrome on Windows"; `null` when the server doesn't know. */
    val device: String?,
    /** The picture's paths to try in order, as the server gives them: its own, its parent's, its grandparent's. */
    val artPaths: List<String>,
    /** Albums and tracks have square art; the rest posters. */
    val square: Boolean,
    /** The page the row opens: the album for a track (the track highlighted). */
    val opens: PleyKey,
)

/** The history's state. */
sealed interface HistoryUiState {
    data object Loading : HistoryUiState

    data class Failed(val error: Throwable) : HistoryUiState

    data class Ready(
        val rows: List<HistoryRow>,
        /** Every play there is, however many are loaded. */
        val total: Int,
        val origin: String,
        val loadingMore: Boolean,
        /** Whether it came from the server, for the offline banner. */
        val status: Loadable<Unit>,
    ) : HistoryUiState {
        val canLoadMore: Boolean get() = rows.size < total
    }
}

/**
 * Watch history (the web's HistoryList on the account page): this
 * profile's plays newest first, in pages of 15 with Show more, each with
 * when and where it played; removing one takes it away at once
 * (DELETE /api/history/{id}) and puts it back if the server refuses.
 */
@OptIn(kotlinx.coroutines.ExperimentalCoroutinesApi::class)
class HistoryViewModel(
    private val history: HistoryRepository,
    origin: Flow<String>,
    private val nowSeconds: () -> Long = { System.currentTimeMillis() / 1000 },
    compute: CoroutineDispatcher = Dispatchers.Default,
) : ViewModel() {
    private val loadingMore = MutableStateFlow(false)
    private val channel = Channel<Throwable>(Channel.BUFFERED)

    /** Failures of Show more and of removals, for a toast. */
    val errors: Flow<Throwable> = channel.receiveAsFlow()

    private val retries = MutableStateFlow(0)

    val state: StateFlow<HistoryUiState> = retries.flatMapLatest {
        // Made again when the language shown changes: the rows' times and episode numbers are written in it.
        combine(history.history(), origin, loadingMore, AppLanguage.shown) { load, server, more, _ -> stateOf(load, server, more) }
    }.flowOn(compute).stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), HistoryUiState.Loading)

    private fun stateOf(load: Loadable<app.pley.android.model.HistoryPage>, server: String, more: Boolean): HistoryUiState {
        val page = load.valueOrNull
        return when {
            page != null -> HistoryUiState.Ready(
                rows = page.entries.map { historyRow(it, nowSeconds(), AppLanguage.formatWords()) },
                total = page.total,
                origin = server,
                loadingMore = more,
                status = load.map { },
            )
            load is Loadable.Failed -> HistoryUiState.Failed(load.error)
            else -> HistoryUiState.Loading
        }
    }

    /** Asks for the history again after it failed. */
    fun retry() = retries.update { it + 1 }

    /** The next 15 plays. */
    fun loadMore() {
        if (loadingMore.value) return
        loadingMore.value = true
        viewModelScope.launch {
            try {
                history.loadMore()
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                channel.send(e)
            } finally {
                loadingMore.value = false
            }
        }
    }

    /** Removes a play; the row goes at once, and comes back if the server refuses. */
    fun remove(entryId: Long) {
        viewModelScope.launch {
            try {
                history.remove(entryId)
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                channel.send(e)
            }
        }
    }

    companion object {
        val factory: ViewModelProvider.Factory = viewModelFactory {
            initializer {
                val g = screensGraph()
                HistoryViewModel(g.data.history, g.data.servers.active.map { it.orEmpty() }.distinctUntilChanged())
            }
        }
    }
}

/** [entry] as its row shows it (apps/web/src/components/HistoryList.tsx), its time worded from [now], in [words]. */
fun historyRow(entry: HistoryEntry, now: Long, words: FormatWords = AppLanguage.formatWords()): HistoryRow {
    val item = entry.item
    val (title, subtitle) = when (item.kind) {
        ItemKind.Episode -> (item.grandparentTitle ?: item.title) to
            listOf(episodeLabel(item, words), item.title).filter { it.isNotEmpty() }.joinToString(" · ")
        ItemKind.Track -> item.title to listOfNotNull(item.grandparentTitle, item.parentTitle).filter { it.isNotEmpty() }.joinToString(" · ")
        else -> item.title to (item.year?.toString().orEmpty())
    }
    val opens = when (item.kind) {
        ItemKind.Track -> item.parentId?.let { PleyKey.Album(it, trackId = item.id) } ?: PleyKey.Item(item.id)
        ItemKind.Album -> PleyKey.Album(item.id)
        ItemKind.Artist -> PleyKey.Artist(item.id)
        else -> PleyKey.Item(item.id)
    }
    val art = listOfNotNull(
        item.posterUrl?.takeIf { it.isNotEmpty() },
        item.parentId?.let { "/api/items/$it/images/poster" },
        item.grandparentId?.let { "/api/items/$it/images/poster" },
    )
    return HistoryRow(
        entryId = entry.id,
        itemId = item.id,
        title = title,
        subtitle = subtitle,
        time = formatRelative(entry.viewedAt, now, words),
        device = entry.device?.takeIf { it.isNotBlank() },
        artPaths = art,
        square = item.kind == ItemKind.Track || item.kind == ItemKind.Album || item.kind == ItemKind.Artist,
        opens = opens,
    )
}
