package app.pley.android.ui.tv.home


import app.pley.android.data.HubsRepository
import app.pley.android.data.ItemRepository
import app.pley.android.data.LibrariesRepository
import app.pley.android.data.Loadable
import app.pley.android.data.PlayItem
import app.pley.android.data.PlayTarget
import app.pley.android.data.ScanState
import app.pley.android.data.Session
import app.pley.android.data.WatchStateRepository
import app.pley.android.data.WatchlistRepository
import app.pley.android.model.CollectionSummary
import app.pley.android.model.Hubs
import app.pley.android.model.ItemDetail
import app.pley.android.model.ItemSummary
import app.pley.android.model.Library
import app.pley.android.player.PlayRequest
import app.pley.android.screens.PlayerLauncher
import app.pley.android.screens.home.HomeSources
import java.util.Collections
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.update

/** Home's sources, every one a fake the test drives and reads. */
class TestHomeSources : HomeSources {
    val home = MutableStateFlow<Loadable<Hubs>>(Loadable.Loading)
    val libraryList = MutableStateFlow<Loadable<List<Library>>>(Loadable.Loading)
    val details = HashMap<Long, MutableStateFlow<Loadable<ItemDetail>>>()
    val prefetched: MutableList<Long> = Collections.synchronizedList(mutableListOf())
    val sessionFlow = MutableStateFlow<Session?>(Session(ORIGIN, null, emptySet()))
    val scanList = MutableStateFlow<List<ScanState>>(emptyList())
    val played: MutableList<PlayRequest> = Collections.synchronizedList(mutableListOf())
    val watched: MutableList<Pair<Long, Boolean>> = Collections.synchronizedList(mutableListOf())
    /** My List as asked: the item, and whether it went on. */
    val listed: MutableList<Pair<Long, Boolean>> = Collections.synchronizedList(mutableListOf())
    val hidden: MutableList<Long> = Collections.synchronizedList(mutableListOf())
    var hubRefreshes = 0
    var libraryRefreshes = 0
    val sorts: MutableList<List<Long>> = Collections.synchronizedList(mutableListOf())
    var sortFailure: Exception? = null
    var orderForgotten = 0

    /** What pressing play on an item resolves to: by default a video from its resume point. */
    var resolve: (ItemSummary, Boolean) -> PlayTarget = { item, fromStart ->
        PlayTarget.Video(item.id, if (fromStart) 0 else item.userState.positionMs)
    }

    fun detail(id: Long): MutableStateFlow<Loadable<ItemDetail>> = details.getOrPut(id) { MutableStateFlow(Loadable.Loading) }

    /** Every library's Recommended rows: none, unless a test says. */
    val libraryHubs = MutableStateFlow<Loadable<Hubs>>(Loadable.Ready(Hubs(emptyList()), fresh = true))

    override val hubs = object : HubsRepository {
        override fun home(): Flow<Loadable<Hubs>> = this@TestHomeSources.home
        override fun library(libraryId: Long): Flow<Loadable<Hubs>> = libraryHubs
        override suspend fun refresh() {
            hubRefreshes++
        }
    }

    override val libraries = object : LibrariesRepository {
        override fun libraries(): Flow<Loadable<List<Library>>> = libraryList
        override suspend fun refresh() {
            libraryRefreshes++
        }

        override suspend fun sort(ids: List<Long>) {
            sortFailure?.let { throw it }
            sorts += ids
            libraryList.update { load ->
                val list = (load as? Loadable.Ready)?.value ?: return@update load
                Loadable.Ready(ids.mapNotNull { id -> list.firstOrNull { it.id == id } } + list.filter { it.id !in ids }, fresh = true)
            }
        }

        override suspend fun forgetOrder() {
            orderForgotten++
        }
    }

    override val items = object : ItemRepository {
        override fun detail(itemId: Long): Flow<Loadable<ItemDetail>> = this@TestHomeSources.detail(itemId)
        // An item's page opened in the frame: nothing more than its details.
        override fun children(itemId: Long): Flow<Loadable<List<ItemSummary>>> = flowOf(Loadable.Ready(emptyList(), fresh = true))
        override fun onDeck(itemId: Long): Flow<Loadable<ItemSummary?>> = flowOf(Loadable.Ready(null, fresh = true))
        override fun related(itemId: Long): Flow<Loadable<List<ItemSummary>>> = flowOf(Loadable.Ready(emptyList(), fresh = true))
        override fun collectionsOf(itemId: Long): Flow<Loadable<List<CollectionSummary>>> = flowOf(Loadable.Ready(emptyList(), fresh = true))
        override suspend fun next(itemId: Long): ItemSummary? = error("not on Home")
        override fun prefetch(itemId: Long) {
            prefetched += itemId
        }
    }

    override val session: Flow<Session?> = sessionFlow
    override val scans: StateFlow<List<ScanState>> = scanList
    override val playItem = PlayItem { item, fromStart -> resolve(item, fromStart) }

    override val watchState = object : WatchStateRepository {
        override suspend fun setWatched(itemId: Long, watched: Boolean) {
            this@TestHomeSources.watched += itemId to watched
        }

        override suspend fun rate(itemId: Long, rating: Double?) = Unit
        override suspend fun chooseStreams(itemId: Long, fileId: Long?, audioIndex: Int?, subtitleIndex: Int?, automaticAudio: Boolean) = Unit
        // As the data side does: out of Continue Watching at once, the row gone with its last tile.
        override suspend fun hideFromContinueWatching(itemId: Long) {
            hidden += itemId
            val shown = (home.value as? Loadable.Ready)?.value ?: return
            val hubs = shown.hubs.mapNotNull { hub ->
                if (!hub.continueWatching) hub else hub.copy(items = hub.items.filterNot { it.id == itemId }).takeIf { it.items.isNotEmpty() }
            }
            home.value = Loadable.Ready(Hubs(hubs), fresh = true)
        }
    }

    // As the data side does: the item's details say at once whether it is on My List.
    override val watchlist = object : WatchlistRepository {
        override fun watchlist(): Flow<Loadable<List<ItemSummary>>> = error("not on Home")
        override suspend fun add(itemId: Long) = list(itemId, true)
        override suspend fun remove(itemId: Long) = list(itemId, false)
        override suspend fun refresh() = Unit

        private fun list(itemId: Long, inList: Boolean) {
            listed += itemId to inList
            val flow = this@TestHomeSources.detail(itemId)
            (flow.value as? Loadable.Ready)?.let { flow.value = Loadable.Ready(it.value.copy(inWatchlist = inList), fresh = true) }
        }
    }

    override val launcher = object : PlayerLauncher {
        override fun play(request: PlayRequest) {
            played += request
        }
    }

    companion object {
        /** Nothing listens there: pictures fail at once and placeholders show. */
        const val ORIGIN = "http://127.0.0.1:9"
    }
}
