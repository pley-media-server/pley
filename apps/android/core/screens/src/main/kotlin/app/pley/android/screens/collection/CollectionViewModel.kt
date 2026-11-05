@file:OptIn(ExperimentalCoroutinesApi::class)

package app.pley.android.screens.collection

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import app.pley.android.common.UiText
import app.pley.android.data.CollectionsRepository
import app.pley.android.data.LibrariesRepository
import app.pley.android.data.Loadable
import app.pley.android.data.PlayItem
import app.pley.android.data.PlayTarget
import app.pley.android.data.map
import app.pley.android.data.valueOrNull
import app.pley.android.model.CollectionDetail
import app.pley.android.model.CollectionSort
import app.pley.android.model.ItemKind
import app.pley.android.model.LibraryKind
import app.pley.android.player.PlayQueue
import app.pley.android.player.PlayRequest
import app.pley.android.screens.PlayerLauncher
import app.pley.android.screens.PleyKey
import app.pley.android.screens.R
import app.pley.android.screens.ScreensGraph
import app.pley.android.screens.library.LibraryViewModel
import app.pley.android.screens.library.SmartFilter
import app.pley.android.screens.library.TileFactory
import app.pley.android.screens.library.TileLook
import kotlin.random.Random
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.channels.BufferOverflow
import kotlinx.coroutines.flow.Flow
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

/** What a collection's page reads and does. */
class CollectionDeps(
    val collections: CollectionsRepository,
    val libraries: LibrariesRepository,
    val playItem: PlayItem,
    val player: PlayerLauncher,
    val origin: Flow<String?>,
    val compute: CoroutineDispatcher = Dispatchers.Default,
    val random: Random = Random.Default,
) {
    companion object {
        fun of(graph: ScreensGraph): CollectionDeps = CollectionDeps(
            collections = graph.data.collections,
            libraries = graph.data.libraries,
            playItem = graph.data.playItem,
            player = graph.playerLauncher,
            origin = graph.data.servers.active,
        )
    }
}

/**
 * A collection as its page shows it (apps/web/src/pages/collections/CollectionPage.tsx):
 * the header's facts and the tiles of its items, in its order.
 */
data class CollectionPage<T : Any>(
    val id: Long,
    val title: String,
    val summary: String?,
    /** Mirrors a TMDB collection: the "TMDB" badge. */
    val automatic: Boolean,
    /** Filled by a filter: the "Smart" badge. */
    val smart: Boolean,
    val count: Int,
    val libraryId: Long,
    /** `null` while the libraries aren't known. */
    val libraryName: String?,
    val kind: LibraryKind?,
    /** [noteTexts] in English: the TV's, until it shows them. */
    val notes: List<String>,
    /** What it holds and in which order, in words: its filter's ("Genre: Comedy", "Newest first"), else its order ("By release date"). */
    val noteTexts: List<UiText>,
    /** The server's addresses for its pictures, made absolute by the interface ([origin]). */
    val posterUrl: String?,
    val backdropUrl: String?,
    val origin: String,
    val tiles: List<T>,
    val itemIds: List<Long>,
    /** Its items are movies, played in its order; else shows, which play their next episode. */
    val movies: Boolean,
) {
    val canPlay: Boolean get() = itemIds.isNotEmpty()
    val canShuffle: Boolean get() = itemIds.size >= 2
}

/** What happened when Play or Shuffle was pressed, for a toast. */
sealed interface CollectionEvent {
    /** The show picked has nothing to play yet. */
    data object NothingToPlay : CollectionEvent

    data class Failed(val error: Throwable) : CollectionEvent
}

/**
 * One collection's page (design spec §4.9), shared by phones and TVs:
 * the backdrop and poster, the TMDB and Smart badges, what it holds in
 * words, Play and Shuffle, then its items.
 *
 * Play plays a collection of movies from its first, through the rest in
 * order (a [PlayQueue.Collection]); Shuffle from a random one. A
 * collection of shows plays the first show's next episode; Shuffle a
 * random show's, as the web does.
 */
class CollectionViewModel<T : Any>(
    val collectionId: Long,
    private val deps: CollectionDeps,
    private val tiles: TileFactory<T>,
) : ViewModel() {
    private val retry = MutableStateFlow(0)
    private val _events = MutableSharedFlow<CollectionEvent>(extraBufferCapacity = 1, onBufferOverflow = BufferOverflow.DROP_OLDEST)

    /** Toasts to show: nothing to play, a failure. */
    val events: SharedFlow<CollectionEvent> = _events

    val state: StateFlow<Loadable<CollectionPage<T>>> = retry.flatMapLatest {
        combine(
            deps.collections.collection(collectionId),
            deps.libraries.libraries().map { it.valueOrNull.orEmpty() },
            deps.origin.map { it.orEmpty() }.distinctUntilChanged(),
        ) { loadable, libraries, origin ->
            loadable.map { detail ->
                val library = libraries.firstOrNull { it.id == detail.libraryId }
                page(detail, library?.name, library?.kind, origin)
            }
        }
    }.flowOn(deps.compute)
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(LibraryViewModel.STOP_AFTER_MS), Loadable.Loading)

    fun retry() = retry.update { it + 1 }

    /** Play ([shuffle] false) or Shuffle. */
    fun play(shuffle: Boolean) {
        val page = state.value.valueOrNull ?: return
        val detail = details ?: return
        if (detail.items.isEmpty()) return
        if (page.movies) {
            val videos = detail.items.filter { it.kind == ItemKind.Movie || it.kind == ItemKind.Episode || it.kind == ItemKind.Extra }
            val start = if (shuffle) videos[deps.random.nextInt(videos.size)] else videos.first()
            deps.player.play(PlayRequest(itemId = start.id, queue = PlayQueue.Collection(collectionId)))
            return
        }
        val pick = if (shuffle) detail.items[deps.random.nextInt(detail.items.size)] else detail.items.first()
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
    fun opens(position: Int): PleyKey? = details?.items?.getOrNull(position)?.let { LibraryViewModel.opens(it) }

    @Volatile
    private var details: CollectionDetail? = null

    private fun page(detail: CollectionDetail, libraryName: String?, kind: LibraryKind?, origin: String): CollectionPage<T> {
        details = detail
        val movies = detail.items.any { it.kind == ItemKind.Movie } || (detail.items.isEmpty() && kind != LibraryKind.Shows)
        val filter = detail.smartFilter?.takeIf { it.isNotEmpty() }
        val notes = filter?.let(SmartFilter::describe) ?: listOfNotNull(sortNote(detail.sort))
        val noteTexts = filter?.let(SmartFilter::words) ?: listOfNotNull(sortNoteText(detail.sort))
        return CollectionPage(
            id = detail.id,
            title = detail.title,
            summary = detail.summary?.takeIf { it.isNotBlank() },
            automatic = detail.automatic,
            smart = !detail.smartFilter.isNullOrEmpty(),
            count = detail.itemCount,
            libraryId = detail.libraryId,
            libraryName = libraryName,
            kind = kind,
            notes = notes,
            noteTexts = noteTexts,
            posterUrl = detail.posterUrl,
            backdropUrl = detail.backdropUrl,
            origin = origin,
            tiles = detail.items.map { tiles.item(it, TileLook.Poster, origin) },
            itemIds = detail.items.map { it.id },
            movies = movies,
        )
    }

    companion object {
        /** The web's words for a collection's order, in English: [sortNoteText]'s. */
        fun sortNote(sort: CollectionSort): String? = when (sort) {
            CollectionSort.Release -> "By release date"
            CollectionSort.Title -> "By title"
            CollectionSort.Custom -> "Custom order"
            CollectionSort.Unknown -> null
        }

        /** The web's words for a collection's order: "By release date". */
        fun sortNoteText(sort: CollectionSort): UiText? = when (sort) {
            CollectionSort.Release -> UiText.res(R.string.collection_order_release)
            CollectionSort.Title -> UiText.res(R.string.collection_order_title)
            CollectionSort.Custom -> UiText.res(R.string.collection_order_custom)
            CollectionSort.Unknown -> null
        }
    }
}
