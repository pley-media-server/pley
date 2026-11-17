@file:OptIn(ExperimentalCoroutinesApi::class)

package app.pley.android.screens.collection

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import app.pley.android.data.Loadable
import app.pley.android.data.PlayTarget
import app.pley.android.data.map
import app.pley.android.data.valueOrNull
import app.pley.android.model.ItemKind
import app.pley.android.model.MergedCollection
import app.pley.android.player.PlayQueue
import app.pley.android.player.PlayRequest
import app.pley.android.screens.PleyKey
import app.pley.android.screens.library.LibraryViewModel
import app.pley.android.screens.library.TileFactory
import app.pley.android.screens.library.TileLook
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.channels.BufferOverflow
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOn
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

/** One of a merged page's collections: its library, whose name opens that collection alone. */
data class MergedSource(val collectionId: Long, val libraryName: String)

/**
 * A Home row of several libraries' collections of one title, as its page
 * shows them (apps/web/src/pages/collections/MergedCollectionPage.tsx):
 * the title, how many movies and shows, the libraries they come from, and
 * every title in the row's order.
 */
data class MergedCollectionPage<T : Any>(
    val title: String,
    val movies: Int,
    val shows: Int,
    /** In the row's order; those of libraries not known yet left out. */
    val sources: List<MergedSource>,
    val tiles: List<T>,
    val itemIds: List<Long>,
) {
    val canPlay: Boolean get() = itemIds.isNotEmpty()
    val canShuffle: Boolean get() = itemIds.size >= 2
}

/**
 * The merged page of [collectionIds], shared by phones and TVs. Play plays
 * its movies one after another in the row's order (a
 * [PlayQueue.MergedCollection]), Shuffle from a random one; without
 * movies, the first show's next episode, or a random show's.
 */
class MergedCollectionViewModel<T : Any>(
    val collectionIds: List<Long>,
    private val deps: CollectionDeps,
    private val tiles: TileFactory<T>,
) : ViewModel() {
    private val retry = MutableStateFlow(0)
    private val _events = MutableSharedFlow<CollectionEvent>(extraBufferCapacity = 1, onBufferOverflow = BufferOverflow.DROP_OLDEST)

    /** Toasts to show: nothing to play, a failure. */
    val events: SharedFlow<CollectionEvent> = _events

    @Volatile
    private var merged: MergedCollection? = null

    val state: StateFlow<Loadable<MergedCollectionPage<T>>> = retry.flatMapLatest {
        combine(
            deps.collections.merged(collectionIds),
            deps.libraries.libraries().map { it.valueOrNull.orEmpty() },
            deps.origin.map { it.orEmpty() }.distinctUntilChanged(),
        ) { loadable, libraries, origin ->
            loadable.map { merged ->
                this.merged = merged
                MergedCollectionPage(
                    title = merged.title,
                    movies = merged.items.count { it.kind == ItemKind.Movie },
                    shows = merged.items.count { it.kind == ItemKind.Show },
                    sources = merged.collections.mapNotNull { part ->
                        libraries.firstOrNull { it.id == part.libraryId }?.let { MergedSource(part.id, it.name) }
                    },
                    tiles = merged.items.map { tiles.item(it, TileLook.Poster, origin) },
                    itemIds = merged.items.map { it.id },
                )
            }
        }
    }.flowOn(deps.compute)
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(LibraryViewModel.STOP_AFTER_MS), Loadable.Loading)

    fun retry() = retry.update { it + 1 }

    /** Play ([shuffle] false) or Shuffle. */
    fun play(shuffle: Boolean) {
        val items = merged?.items.orEmpty()
        if (items.isEmpty()) return
        val movies = items.filter { it.kind == ItemKind.Movie }
        if (movies.isNotEmpty()) {
            val start = if (shuffle) movies[deps.random.nextInt(movies.size)] else movies.first()
            deps.player.play(PlayRequest(itemId = start.id, queue = PlayQueue.MergedCollection(collectionIds)))
            return
        }
        val pick = if (shuffle) items[deps.random.nextInt(items.size)] else items.first()
        viewModelScope.launch {
            try {
                when (val target = deps.playItem.resolve(pick, fromStart = false)) {
                    is PlayTarget.Video -> deps.player.play(PlayRequest(itemId = target.itemId, startMs = target.startMs))
                    else -> _events.tryEmit(CollectionEvent.NothingToPlay)
                }
            } catch (e: kotlin.coroutines.cancellation.CancellationException) {
                throw e
            } catch (e: Exception) {
                _events.tryEmit(CollectionEvent.Failed(e))
            }
        }
    }

    /** Where opening the item at [position] goes. */
    fun opens(position: Int): PleyKey? = merged?.items?.getOrNull(position)?.let { LibraryViewModel.opens(it) }
}
