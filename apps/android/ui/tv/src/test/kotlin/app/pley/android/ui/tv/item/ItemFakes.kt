package app.pley.android.ui.tv.item

import app.pley.android.common.SavedServer
import app.pley.android.data.AppSettings
import app.pley.android.data.CollectionsRepository
import app.pley.android.data.DataGraph
import app.pley.android.data.HistoryRepository
import app.pley.android.data.HubsRepository
import app.pley.android.data.ItemRepository
import app.pley.android.data.LibrariesRepository
import app.pley.android.data.LibraryItemsRepository
import app.pley.android.data.LiveUpdates
import app.pley.android.data.Loadable
import app.pley.android.data.PeopleRepository
import app.pley.android.data.PlayItem
import app.pley.android.data.PlayTarget
import app.pley.android.data.PlaylistsRepository
import app.pley.android.data.RequestsRepository
import app.pley.android.data.SearchRepository
import app.pley.android.data.ServersRepository
import app.pley.android.data.SessionRepository
import app.pley.android.data.SnapshotStore
import app.pley.android.data.TrackDefaultsRepository
import app.pley.android.data.WatchStateRepository
import app.pley.android.data.WatchlistRepository
import app.pley.android.model.CollectionSummary
import app.pley.android.model.ItemDetail
import app.pley.android.model.ItemKind
import app.pley.android.model.ItemSummary
import app.pley.android.model.MediaFile
import app.pley.android.model.MediaStream
import app.pley.android.model.PersonDetail
import app.pley.android.model.StreamKind
import app.pley.android.player.PlayRequest
import app.pley.android.screens.PlayerLauncher
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.flowOf

/**
 * An item page's data for the TV's tests: the items set, what was written,
 * and [requests] and each item's [children] (a show's seasons, a season's
 * episodes) where a test gives them.
 */
internal class ItemData(
    vararg details: ItemDetail,
    override val requests: RequestsRepository = RequestsRepository.None,
    private val children: Map<Long, List<ItemSummary>> = emptyMap(),
) : DataGraph {
    private val held = details.associate { it.id to MutableStateFlow<Loadable<ItemDetail>>(Loadable.Ready(it, true)) }
    val streams = mutableListOf<List<Long?>>()
    val played = mutableListOf<PlayRequest>()
    val hidden = mutableListOf<Long>()

    /** My List's writes: the item, and whether it went in. */
    val listed = mutableListOf<Pair<Long, Boolean>>()

    val launcher = object : PlayerLauncher {
        override fun play(request: PlayRequest) {
            played += request
        }
    }

    override val items: ItemRepository = object : ItemRepository {
        override fun detail(itemId: Long): Flow<Loadable<ItemDetail>> = held[itemId] ?: flowOf(Loadable.Loading)
        override fun children(itemId: Long): Flow<Loadable<List<ItemSummary>>> = flowOf(Loadable.Ready(children[itemId].orEmpty(), true))
        override fun onDeck(itemId: Long): Flow<Loadable<ItemSummary?>> = flowOf(Loadable.Ready(null, true))
        override fun related(itemId: Long): Flow<Loadable<List<ItemSummary>>> = flowOf(Loadable.Ready(emptyList(), true))
        override fun collectionsOf(itemId: Long): Flow<Loadable<List<CollectionSummary>>> = flowOf(Loadable.Ready(emptyList(), true))
        override suspend fun next(itemId: Long): ItemSummary? = null
        override fun prefetch(itemId: Long) = Unit
    }

    override val watchState: WatchStateRepository = object : WatchStateRepository {
        override suspend fun setWatched(itemId: Long, watched: Boolean) = Unit
        override suspend fun rate(itemId: Long, rating: Double?) = Unit
        override suspend fun chooseStreams(itemId: Long, fileId: Long?, audioIndex: Int?, subtitleIndex: Int?, automaticAudio: Boolean) {
            streams += listOf(itemId, fileId, audioIndex?.toLong(), subtitleIndex?.toLong())
        }

        override suspend fun hideFromContinueWatching(itemId: Long) {
            hidden += itemId
        }
    }

    override val watchlist: WatchlistRepository = object : WatchlistRepository {
        override fun watchlist(): Flow<Loadable<List<ItemSummary>>> = flowOf(Loadable.Loading)
        override suspend fun add(itemId: Long) = list(itemId, true)
        override suspend fun remove(itemId: Long) = list(itemId, false)
        override suspend fun refresh() = Unit

        private fun list(itemId: Long, inList: Boolean) {
            listed += itemId to inList
            held[itemId]?.let { flow -> (flow.value as? Loadable.Ready)?.let { flow.value = it.copy(value = it.value.copy(inWatchlist = inList)) } }
        }
    }

    override val playItem: PlayItem = PlayItem { item, fromStart -> PlayTarget.Video(item.id, if (fromStart) 0 else item.userState.positionMs) }

    override val servers: ServersRepository = object : ServersRepository {
        override val saved: Flow<List<SavedServer>> = flowOf(emptyList())
        override val active: StateFlow<String?> = MutableStateFlow("http://127.0.0.1:1")
        override suspend fun use(server: SavedServer) = Unit
        override suspend fun forget(origin: String) = Unit
        override suspend fun clientId(): String = "test"
    }

    override val people: PeopleRepository = object : PeopleRepository {
        override fun person(personId: Long): Flow<Loadable<PersonDetail>> = flowOf(Loadable.Loading)
    }
    override val session: SessionRepository get() = unused()
    override val snapshots: SnapshotStore get() = unused()
    override val libraries: LibrariesRepository get() = unused()
    override val hubs: HubsRepository get() = unused()
    override val libraryItems: LibraryItemsRepository get() = unused()
    override val search: SearchRepository get() = unused()
    override val collections: CollectionsRepository get() = unused()
    override val playlists: PlaylistsRepository get() = unused()
    override val history: HistoryRepository get() = unused()
    override val trackDefaults: TrackDefaultsRepository get() = unused()
    override val liveUpdates: LiveUpdates get() = unused()
    override val settings: AppSettings get() = unused()

    private fun unused(): Nothing = error("item pages don't use this")
}

/** A movie with two audio tracks and a subtitle track, [positionMs] in. */
internal fun movie(positionMs: Long = 0) = ItemDetail(
    id = 1,
    libraryId = 1,
    kind = ItemKind.Movie,
    title = "Saltwind",
    year = 2021,
    durationMs = 9_060_000,
    summary = "Three salt traders cross the Danakil in the last caravan of the season.",
    userState = app.pley.android.model.UserState(positionMs = positionMs),
    files = listOf(
        MediaFile(
            id = 11,
            path = "Saltwind.mkv",
            width = 3840,
            height = 2160,
            streams = listOf(
                MediaStream(0, StreamKind.Video, "hevc"),
                MediaStream(1, StreamKind.Audio, "truehd", language = "eng", default = true, channels = 8),
                MediaStream(2, StreamKind.Audio, "ac3", language = "fra", channels = 6),
                MediaStream(3, StreamKind.Subtitle, "subrip", language = "eng"),
            ),
        ),
    ),
)
