@file:OptIn(ExperimentalCoroutinesApi::class)

package app.pley.android.screens.collection

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import app.pley.android.data.Loadable
import app.pley.android.data.valueOrNull
import app.pley.android.model.Library
import app.pley.android.model.LibraryKind
import app.pley.android.screens.library.CollectionTile
import app.pley.android.screens.library.LibraryDeps
import app.pley.android.screens.library.LibraryViewModel
import app.pley.android.screens.library.TileFactory
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.flowOn
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

/** One library's collections, as a shelf of the Collections page. */
data class CollectionShelf<T : Any>(
    val libraryId: Long,
    val libraryName: String,
    val kind: LibraryKind,
    val collections: Loadable<List<CollectionTile<T>>>,
)

/**
 * The Collections page: the collections of [only] library (phones open
 * one library's), or of every movie and show library, a shelf each, in
 * the server's order of libraries (the TV rail's Collections). Music has
 * none.
 */
class CollectionsViewModel<T : Any>(
    val only: Long?,
    private val deps: LibraryDeps,
    private val tiles: TileFactory<T>,
) : ViewModel() {
    private val retry = MutableStateFlow(0)

    private val origin: Flow<String> = deps.origin.map { it.orEmpty() }.distinctUntilChanged()

    val state: StateFlow<Loadable<List<CollectionShelf<T>>>> = retry.flatMapLatest {
        deps.libraries.libraries()
    }.flatMapLatest { loadable ->
        val libraries = loadable.valueOrNull
        if (libraries == null) {
            @Suppress("UNCHECKED_CAST")
            return@flatMapLatest flowOf(loadable as Loadable<List<CollectionShelf<T>>>)
        }
        val shown = libraries.filter { it.kind != LibraryKind.Music && (only == null || it.id == only) }
        if (shown.isEmpty()) return@flatMapLatest flowOf(Loadable.Ready(emptyList(), fresh = true))
        origin.flatMapLatest { origin ->
            combine(shown.map { library -> shelf(library, origin) }) { shelves -> Loadable.Ready(shelves.toList(), fresh = true) }
        }
    }.flowOn(deps.compute)
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(LibraryViewModel.STOP_AFTER_MS), Loadable.Loading)

    fun retry() {
        retry.update { it + 1 }
        viewModelScope.launch { runCatching { deps.libraries.refresh() } }
    }

    private fun shelf(library: Library, origin: String): Flow<CollectionShelf<T>> =
        deps.collections.collections(library.id).map { loadable ->
            CollectionShelf(
                libraryId = library.id,
                libraryName = library.name,
                kind = library.kind,
                collections = when (loadable) {
                    is Loadable.Ready -> Loadable.Ready(loadable.value.map { tile(it, library, origin) }, loadable.fresh)
                    is Loadable.Failed -> Loadable.Failed(loadable.error, loadable.stale?.map { tile(it, library, origin) })
                    Loadable.Loading -> Loadable.Loading
                },
            )
        }

    private fun tile(c: app.pley.android.model.CollectionSummary, library: Library, origin: String): CollectionTile<T> =
        c.tile(tiles.collection(c, library.kind, origin))
}
