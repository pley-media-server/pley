package app.pley.android.screens.home

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
import app.pley.android.model.Hub
import app.pley.android.model.Hubs
import app.pley.android.model.ItemDetail
import app.pley.android.model.ItemKind
import app.pley.android.model.ItemSummary
import app.pley.android.model.Library
import app.pley.android.model.LibraryKind
import app.pley.android.model.UserState
import app.pley.android.player.PlayRequest
import app.pley.android.screens.Navigator
import app.pley.android.screens.PlayerLauncher
import app.pley.android.screens.PleyKey
import java.util.Collections
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.update

/** Home's sources, every one a fake the test drives and reads. */
class FakeHomeSources : HomeSources {
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

    override val hubs = object : HubsRepository {
        override fun home(): Flow<Loadable<Hubs>> = this@FakeHomeSources.home
        override fun library(libraryId: Long): Flow<Loadable<Hubs>> = error("not on Home")
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
        override fun detail(itemId: Long): Flow<Loadable<ItemDetail>> = this@FakeHomeSources.detail(itemId)
        override fun children(itemId: Long): Flow<Loadable<List<ItemSummary>>> = error("not on Home")
        override fun onDeck(itemId: Long): Flow<Loadable<ItemSummary?>> = error("not on Home")
        override fun related(itemId: Long): Flow<Loadable<List<ItemSummary>>> = error("not on Home")
        override fun collectionsOf(itemId: Long): Flow<Loadable<List<CollectionSummary>>> = error("not on Home")
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
            this@FakeHomeSources.watched += itemId to watched
        }

        override suspend fun rate(itemId: Long, rating: Double?) = Unit
        override suspend fun chooseStreams(itemId: Long, fileId: Long?, audioIndex: Int?, subtitleIndex: Int?, automaticAudio: Boolean) = Unit
        override suspend fun hideFromContinueWatching(itemId: Long) {
            hidden += itemId
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
            val flow = this@FakeHomeSources.detail(itemId)
            (flow.value as? Loadable.Ready)?.let { flow.value = Loadable.Ready(it.value.copy(inWatchlist = inList), fresh = true) }
        }
    }

    override val launcher = object : PlayerLauncher {
        override fun play(request: PlayRequest) {
            played += request
        }
    }

    companion object {
        const val ORIGIN = "http://192.168.1.20:7539"
    }
}

/** A navigator that remembers where it was sent. */
class RecordingNavigator : Navigator {
    val opened = mutableListOf<PleyKey>()
    override fun open(key: PleyKey) {
        opened += key
    }

    override fun back() = Unit
    override fun restart(key: PleyKey) = Unit
}

// The test catalogue: the mockups' titles.

val MOVIES = Library(1, "Movies", LibraryKind.Movies, itemCount = 412)
val SHOWS = Library(2, "Shows", LibraryKind.Shows, itemCount = 38)
val MUSIC = Library(3, "Music", LibraryKind.Music, itemCount = 120)

/** An episode 32 of 58 minutes in: "26 min left". */
val GLASSHOUSE = ItemSummary(
    id = 501, libraryId = 2, kind = ItemKind.Episode, title = "Glasshouse", year = 2024, index = 3,
    parentId = 50, parentIndex = 1, grandparentId = 5, grandparentTitle = "The Lantern House",
    durationMs = 58 * 60_000L, userState = UserState(positionMs = 32 * 60_000L),
    summary = "The lamp goes out for the first time in a hundred years.",
)

/** A movie with 1h 12m left. */
val ASHFALL = ItemSummary(
    id = 101, libraryId = 1, kind = ItemKind.Movie, title = "Ashfall", year = 2025, rating = 7.4,
    durationMs = 122 * 60_000L, userState = UserState(positionMs = 50 * 60_000L), addedAt = 1_700_000_000,
)

val SALTWIND = ItemSummary(
    id = 102, libraryId = 1, kind = ItemKind.Movie, title = "Saltwind", year = 2024,
    durationMs = 111 * 60_000L, userState = UserState(positionMs = 73 * 60_000L), addedAt = 1_700_000_100,
)

val PARALLEL = ItemSummary(
    id = 502, libraryId = 2, kind = ItemKind.Episode, title = "Low Tide", index = 2, parentIndex = 4,
    grandparentId = 6, grandparentTitle = "Parallel Coast", durationMs = 45 * 60_000L,
    userState = UserState(positionMs = 14 * 60_000L),
)

val LOW_ORBIT = ItemSummary(id = 111, libraryId = 1, kind = ItemKind.Movie, title = "Low Orbit", year = 2024, addedAt = 1_800_000_000)
val HALCYON = ItemSummary(id = 112, libraryId = 1, kind = ItemKind.Movie, title = "Halcyon", year = 2025, addedAt = 1_800_000_100)
val OVERGROWTH = ItemSummary(
    id = 21, libraryId = 2, kind = ItemKind.Show, title = "Overgrowth", year = 2024, childCount = 2,
    leafCount = 16, viewedLeafCount = 10, addedAt = 1_800_000_200,
)
val ALBUM = ItemSummary(id = 301, libraryId = 3, kind = ItemKind.Album, title = "Tidal", parentTitle = "Harbour Lights", year = 2021, addedAt = 1_900_000_000)

val CONTINUE = Hub("continue_watching", "Continue Watching", null, listOf(GLASSHOUSE, ASHFALL, SALTWIND, PARALLEL))
val ADDED_MOVIES = Hub("recently_added:1", "Recently Added in Movies", 1, listOf(LOW_ORBIT, HALCYON))
val ADDED_SHOWS = Hub("recently_added:2", "Recently Added in Shows", 2, listOf(OVERGROWTH))
val ADDED_MUSIC = Hub("recently_added:3", "Recently Added in Music", 3, listOf(ALBUM))

val HOME = Hubs(listOf(CONTINUE, ADDED_MOVIES, ADDED_SHOWS, ADDED_MUSIC))
