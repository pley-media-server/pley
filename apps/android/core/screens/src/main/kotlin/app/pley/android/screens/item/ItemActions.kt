package app.pley.android.screens.item

import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.viewModelScope
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import app.pley.android.common.AppLanguage
import app.pley.android.data.DataGraph
import app.pley.android.data.Loadable
import app.pley.android.model.ItemDetail
import app.pley.android.model.ItemKind
import app.pley.android.model.ItemSummary
import app.pley.android.model.format.cardSubtitle
import app.pley.android.model.format.isInProgress
import app.pley.android.network.ApiException
import app.pley.android.screens.PlayerLauncher
import app.pley.android.screens.PleyKey
import app.pley.android.screens.screensGraph
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.flowOn
import kotlinx.coroutines.flow.receiveAsFlow
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch

/**
 * What can be done with an item from its action sheet, its TV panel or a
 * long press on its tile (ItemActions.tsx without the administrator's
 * part): playing, watched marks, leaving Continue Watching, My List,
 * playlists, and going to its show or season. Editing, refreshing and analysing are the web's, for
 * administrators: never offered here, whatever the server says of the
 * profile.
 */
enum class ItemAction {
    Play,
    Resume,
    FromStart,
    MarkWatched,
    MarkUnwatched,

    /** Opened from a Continue Watching tile: it (an episode: its show) leaves the row. */
    RemoveFromContinueWatching,
    AddToMyList,
    RemoveFromMyList,
    AddToPlaylist,
    GoToShow,
    GoToSeason,

    /** Artists, albums and tracks: their music page, which plays them. */
    Open,
}

/**
 * The actions for [item], in the order they are listed. My List is offered
 * for movies and shows when it is known whether the item is in it
 * ([inWatchlist]); leaving Continue Watching when the actions were opened
 * from that row ([continueWatching]).
 */
fun itemActions(item: ItemSummary, inWatchlist: Boolean? = null, continueWatching: Boolean = false): List<ItemAction> = buildList {
    val kind = item.kind
    val music = kind == ItemKind.Artist || kind == ItemKind.Album || kind == ItemKind.Track
    if (music) {
        add(ItemAction.Open)
    } else if (kind != ItemKind.Unknown) {
        if (isInProgress(item)) {
            add(ItemAction.Resume)
            add(ItemAction.FromStart)
        } else {
            add(ItemAction.Play)
        }
    }
    if (kind == ItemKind.Movie || kind == ItemKind.Episode || kind == ItemKind.Show || kind == ItemKind.Season) {
        add(if (watched(item)) ItemAction.MarkUnwatched else ItemAction.MarkWatched)
    }
    if (continueWatching) add(ItemAction.RemoveFromContinueWatching)
    if (inWatchlist != null && kind.listable) add(if (inWatchlist) ItemAction.RemoveFromMyList else ItemAction.AddToMyList)
    if (kind != ItemKind.Extra && kind != ItemKind.Unknown) add(ItemAction.AddToPlaylist)
    if (kind == ItemKind.Episode && item.grandparentId != null) add(ItemAction.GoToShow)
    if (kind == ItemKind.Episode && item.parentId != null) add(ItemAction.GoToSeason)
    if (kind == ItemKind.Season && item.parentId != null) add(ItemAction.GoToShow)
}

/** Watched: a video marked so, or every episode of a show or season. */
fun watched(item: ItemSummary): Boolean = when (item.kind) {
    ItemKind.Show, ItemKind.Season -> {
        val leaves = item.leafCount ?: 0
        leaves > 0 && (item.viewedLeafCount ?: 0) >= leaves
    }
    else -> item.userState.watched
}

/** Where [action] on [item] goes, for the actions that open a page or a sheet; `null` for the others. */
fun destination(action: ItemAction, item: ItemSummary): PleyKey? = when (action) {
    ItemAction.AddToPlaylist -> PleyKey.AddToPlaylist(listOf(item.id))
    ItemAction.GoToShow -> (if (item.kind == ItemKind.Season) item.parentId else item.grandparentId)?.let { PleyKey.Item(it) }
    ItemAction.GoToSeason -> item.parentId?.let { PleyKey.Item(it) }
    ItemAction.Open -> when (item.kind) {
        ItemKind.Artist -> PleyKey.Artist(item.id)
        ItemKind.Album -> PleyKey.Album(item.id)
        else -> item.parentId?.let { PleyKey.Album(it) }
    }
    else -> null
}

/** The sheet's state: the item's name and its actions. */
sealed interface ItemActionsUiState {
    data object Loading : ItemActionsUiState

    data class Failed(val error: Throwable) : ItemActionsUiState

    data class Ready(
        val item: ItemSummary,
        /** The server's origin, for the item's pictures. */
        val origin: String,
        /** "The Office", or an episode's show. */
        val title: String,
        /** "S2 · E5 · Office Olympics", "2019", "3 seasons". */
        val subtitle: String,
        val actions: List<ItemAction>,
    ) : ItemActionsUiState
}

/**
 * The action sheet of an item (PleyKey.ItemActions), opened by a long press
 * on its card or the page's More. Play, the watched marks, Continue
 * Watching and My List are done here; the rest open a page ([destination]).
 * Opened from a Continue Watching tile ([continueWatching]), it offers to
 * take the item out of that row.
 */
class ItemActionsViewModel(
    private val data: DataGraph,
    private val launcher: PlayerLauncher,
    val itemId: Long,
    private val continueWatching: Boolean = false,
    compute: CoroutineDispatcher = Dispatchers.Default,
    /** Where actions run: they outlive the sheet, which closes as soon as one is chosen. */
    private val work: CoroutineScope = OutlivingWork,
) : ViewModel() {
    private val eventChannel = Channel<ItemEvent>(Channel.BUFFERED)

    /** Failures and "nothing to play", told once. */
    val events: Flow<ItemEvent> = eventChannel.receiveAsFlow()

    /** The sheet, made again when the language shown changes: its subtitle's numbers are written in it. */
    val state: StateFlow<ItemActionsUiState> = combine(data.items.detail(itemId), data.servers.active, AppLanguage.shown) { load, origin, _ ->
        when (load) {
            Loadable.Loading -> ItemActionsUiState.Loading
            is Loadable.Failed -> load.stale?.let { ready(it, origin.orEmpty()) } ?: ItemActionsUiState.Failed(load.error)
            is Loadable.Ready -> ready(load.value, origin.orEmpty())
        }
    }
        .flowOn(compute)
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(ItemViewModel.STOP_MS), ItemActionsUiState.Loading)

    private fun ready(detail: ItemDetail, origin: String): ItemActionsUiState.Ready {
        val item = detail.toSummary()
        val episode = item.kind == ItemKind.Episode
        val locale = AppLanguage.locale
        return ItemActionsUiState.Ready(
            item = item,
            origin = origin,
            title = if (episode) item.grandparentTitle?.takeIf { it.isNotEmpty() } ?: item.title else item.title,
            subtitle = if (episode) {
                listOf(cardSubtitle(item, locale), item.title.takeIf { item.grandparentTitle?.isNotEmpty() == true }.orEmpty())
                    .filter { it.isNotEmpty() }.joinToString(" · ")
            } else {
                cardSubtitle(item, locale)
            },
            actions = itemActions(item, detail.inWatchlist, continueWatching),
        )
    }

    /**
     * Does [action] if it is done here (play, from start, the watched
     * marks, Continue Watching, My List): `true` once it is under way, so
     * the sheet can close.
     */
    fun perform(action: ItemAction): Boolean {
        val item = (state.value as? ItemActionsUiState.Ready)?.item ?: return false
        when (action) {
            ItemAction.Play, ItemAction.Resume, ItemAction.FromStart -> work.launch {
                playItem(data, launcher, item, fromStart = action == ItemAction.FromStart)?.let { eventChannel.trySend(it) }
            }
            ItemAction.MarkWatched, ItemAction.MarkUnwatched -> change { data.watchState.setWatched(item.id, action == ItemAction.MarkWatched) }
            ItemAction.RemoveFromContinueWatching -> change { data.watchState.hideFromContinueWatching(item.id) }
            ItemAction.AddToMyList -> change { data.watchlist.add(item.id) }
            ItemAction.RemoveFromMyList -> change { data.watchlist.remove(item.id) }
            else -> return false
        }
        return true
    }

    /** Runs [block] past the sheet; a change the data side undid is told. */
    private fun change(block: suspend () -> Unit) {
        work.launch {
            try {
                block()
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                // The change is undone by then; the sheet, if still open, says why.
                eventChannel.trySend(ItemEvent.Failed(e))
            }
        }
    }

    companion object {
        fun factory(itemId: Long, continueWatching: Boolean = false): ViewModelProvider.Factory = viewModelFactory {
            initializer {
                val graph = screensGraph()
                ItemActionsViewModel(graph.data, graph.playerLauncher, itemId, continueWatching)
            }
        }
    }
}

/**
 * Work started from a sheet that closes at once: a play to launch, a mark
 * to send. It runs to its end on the main thread's loop, whatever happens
 * to the sheet.
 */
internal val OutlivingWork: CoroutineScope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)

/** A 404: the item or person isn't there for this profile. */
internal fun Throwable.isNotFound(): Boolean = (this as? ApiException)?.status == 404
