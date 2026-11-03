package app.pley.android.screens.library

import app.pley.android.data.AppSettings
import app.pley.android.data.CollectionsRepository
import app.pley.android.data.HubsRepository
import app.pley.android.data.LibrariesRepository
import app.pley.android.data.LibraryItems
import app.pley.android.data.LibraryItemsRepository
import app.pley.android.data.LibraryQuery
import app.pley.android.data.LibraryViews
import app.pley.android.data.Loadable
import app.pley.android.data.Settings
import app.pley.android.model.CollectionDetail
import app.pley.android.model.CollectionSummary
import app.pley.android.model.Hubs
import app.pley.android.model.ItemKind
import app.pley.android.model.ItemSummary
import app.pley.android.model.Library
import app.pley.android.model.LibraryFilters
import app.pley.android.model.LibraryKind
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.onStart
import kotlinx.coroutines.flow.update

/** A tile as a test sees it: what it was made from. */
data class FakeTile(val id: Long, val title: String, val look: TileLook, val subtitle: String = "")

object FakeTiles : TileFactory<FakeTile> {
    override fun item(item: ItemSummary, look: TileLook, origin: String) = FakeTile(item.id, item.title, look, "$origin")

    override fun collection(collection: CollectionSummary, kind: LibraryKind?, origin: String) =
        FakeTile(collection.id, collection.title, TileLook.Poster, "${collection.itemCount} ${if (kind == LibraryKind.Shows) "shows" else "movies"}")
}

fun movie(id: Long, title: String = "Movie $id", kind: ItemKind = ItemKind.Movie, libraryId: Long = 1) =
    ItemSummary(id = id, libraryId = libraryId, kind = kind, title = title, sortTitle = title)

class FakeLibraries(vararg libraries: Library) : LibrariesRepository {
    val state = MutableStateFlow<Loadable<List<Library>>>(Loadable.Ready(libraries.toList(), fresh = true))
    var refreshes = 0

    override fun libraries(): Flow<Loadable<List<Library>>> = state

    override suspend fun refresh() {
        refreshes++
    }

    override suspend fun sort(ids: List<Long>) = Unit

    override suspend fun forgetOrder() = Unit
}

/** Listings by query; [showings] records what the grid said it shows. */
class FakeLibraryItems : LibraryItemsRepository {
    val listings = MutableStateFlow<Map<LibraryQuery, Loadable<LibraryItems>>>(emptyMap())
    val asked = mutableListOf<LibraryQuery>()
    val showings = mutableListOf<Pair<LibraryQuery, Int>>()
    var filters = MutableStateFlow<Loadable<LibraryFilters>>(
        Loadable.Ready(LibraryFilters(listOf("Comedy", "Drama"), listOf(2024, 1999), listOf("4k", "1080"), listOf("hdr", "dolby_vision", "hdr10", "sdr")), true),
    )

    fun set(query: LibraryQuery, items: LibraryItems) = listings.update { it + (query to Loadable.Ready(items, fresh = true)) }

    override fun items(libraryId: Long, query: LibraryQuery): Flow<Loadable<LibraryItems>> =
        listings.map { it[query] ?: Loadable.Loading }.onStart { asked += query }

    override fun filters(libraryId: Long): Flow<Loadable<LibraryFilters>> = filters

    override fun showing(libraryId: Long, query: LibraryQuery, position: Int) {
        showings += query to position
    }
}

class FakeHubs : HubsRepository {
    val library = MutableStateFlow<Loadable<Hubs>>(Loadable.Loading)
    var opened = 0

    override fun home(): Flow<Loadable<Hubs>> = MutableStateFlow(Loadable.Loading)

    override fun library(libraryId: Long): Flow<Loadable<Hubs>> = library.onStart { opened++ }

    override suspend fun refresh() = Unit
}

class FakeCollections : CollectionsRepository {
    val lists = MutableStateFlow<Map<Long, List<CollectionSummary>>>(emptyMap())
    val details = MutableStateFlow<Map<Long, CollectionDetail>>(emptyMap())

    override fun collections(libraryId: Long): Flow<Loadable<List<CollectionSummary>>> =
        lists.map { all -> all[libraryId]?.let { Loadable.Ready(it, fresh = true) } ?: Loadable.Loading }

    override fun collection(collectionId: Long): Flow<Loadable<CollectionDetail>> =
        details.map { all -> all[collectionId]?.let { Loadable.Ready(it, fresh = true) } ?: Loadable.Loading }
}

/** Views kept by library, as a test sets and reads them. */
class FakeLibraryViews(vararg left: Pair<Long, String>) : LibraryViews {
    val kept = mutableMapOf(*left)

    override suspend fun of(libraryId: Long): String? = kept[libraryId]

    override suspend fun keep(libraryId: Long, view: String) {
        if (view.isEmpty()) kept.remove(libraryId) else kept[libraryId] = view
    }
}

class FakeSettings : AppSettings {
    val state = MutableStateFlow(Settings())

    override val settings: Flow<Settings> = state

    override suspend fun update(change: (Settings) -> Settings) = state.update(change)
}
