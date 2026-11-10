package app.pley.android.screens.item

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
import app.pley.android.data.valueOrNull
import app.pley.android.model.CollectionSummary
import app.pley.android.model.ItemDetail
import app.pley.android.model.ItemKind
import app.pley.android.model.ItemSummary
import app.pley.android.model.MediaFile
import app.pley.android.model.MediaStream
import app.pley.android.model.PersonDetail
import app.pley.android.model.StreamKind
import app.pley.android.model.UserState
import app.pley.android.player.PlayRequest
import app.pley.android.screens.PlayerLauncher
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.map

const val ORIGIN = "http://192.168.1.20:7539"

/** The data side of the screens' tests: items and people the test sets, and what was written. */
class FakeData : DataGraph {
    val details = HashMap<Long, MutableStateFlow<Loadable<ItemDetail>>>()
    val children = HashMap<Long, MutableStateFlow<Loadable<List<ItemSummary>>>>()
    val onDecks = HashMap<Long, MutableStateFlow<Loadable<ItemSummary?>>>()
    val persons = HashMap<Long, MutableStateFlow<Loadable<PersonDetail>>>()

    fun setPerson(id: Long, load: Loadable<PersonDetail>) {
        persons.getOrPut(id) { MutableStateFlow(Loadable.Loading) }.value = load
    }
    val watched = mutableListOf<Pair<Long, Boolean>>()
    val ratings = mutableListOf<Pair<Long, Double?>>()
    val streams = mutableListOf<StreamWrite>()
    val hidden = mutableListOf<Long>()

    /** The items whose children were asked for ahead, in order. */
    val warmed = mutableListOf<Long>()

    /** The items whose pages were got ready from a listing, each with the episode given as its next. */
    val seeded = mutableListOf<Pair<Long, Long?>>()

    /** My List's writes: the item, and whether it went in. */
    val listed = mutableListOf<Pair<Long, Boolean>>()

    /** What My List's writes throw, if anything. */
    var listFailure: Exception? = null
    val active = MutableStateFlow<String?>(ORIGIN)

    data class StreamWrite(val itemId: Long, val fileId: Long?, val audio: Int?, val subtitle: Int?, val automaticAudio: Boolean = false)

    fun detail(id: Long): MutableStateFlow<Loadable<ItemDetail>> = details.getOrPut(id) { MutableStateFlow(Loadable.Loading) }

    fun set(detail: ItemDetail) {
        detail(detail.id).value = Loadable.Ready(detail, fresh = true)
    }

    fun setChildren(id: Long, items: List<ItemSummary>) {
        children.getOrPut(id) { MutableStateFlow(Loadable.Loading) }.value = Loadable.Ready(items, fresh = true)
    }

    fun setOnDeck(id: Long, item: ItemSummary?) {
        onDecks.getOrPut(id) { MutableStateFlow(Loadable.Loading) }.value = Loadable.Ready(item, fresh = true)
    }

    override val items: ItemRepository = object : ItemRepository {
        override fun detail(itemId: Long): Flow<Loadable<ItemDetail>> = this@FakeData.detail(itemId)
        override fun children(itemId: Long): Flow<Loadable<List<ItemSummary>>> = children.getOrPut(itemId) { MutableStateFlow(Loadable.Loading) }
        override fun onDeck(itemId: Long): Flow<Loadable<ItemSummary?>> = onDecks.getOrPut(itemId) { MutableStateFlow(Loadable.Loading) }
        override fun related(itemId: Long): Flow<Loadable<List<ItemSummary>>> = flowOf(Loadable.Ready(emptyList(), true))
        override fun collectionsOf(itemId: Long): Flow<Loadable<List<CollectionSummary>>> = flowOf(Loadable.Ready(emptyList(), true))
        override suspend fun next(itemId: Long): ItemSummary? = null
        override fun prefetch(itemId: Long) = Unit
        override suspend fun warmChildren(itemId: Long) {
            warmed += itemId
        }

        override fun held(itemId: Long): ItemDetail? = details[itemId]?.value?.valueOrNull

        override fun heldChildren(itemId: Long): List<ItemSummary>? = children[itemId]?.value?.valueOrNull

        override fun heldOnDeck(itemId: Long): Loadable<ItemSummary?>? = onDecks[itemId]?.value?.takeIf { it is Loadable.Ready }

        /** As the app's: what the listing says shows while nothing else does. */
        override fun seed(item: ItemSummary, next: ItemSummary?) {
            seeded += item.id to next?.id
            this@FakeData.detail(item.id).compareAndSet(Loadable.Loading, Loadable.Ready(ItemDetail.of(item), fresh = false))
            if (next != null) onDecks.getOrPut(item.id) { MutableStateFlow(Loadable.Loading) }.compareAndSet(Loadable.Loading, Loadable.Ready(next, fresh = false))
        }
    }

    override val watchState: WatchStateRepository = object : WatchStateRepository {
        override suspend fun setWatched(itemId: Long, watched: Boolean) {
            this@FakeData.watched += itemId to watched
        }

        override suspend fun rate(itemId: Long, rating: Double?) {
            ratings += itemId to rating
        }

        override suspend fun chooseStreams(itemId: Long, fileId: Long?, audioIndex: Int?, subtitleIndex: Int?, automaticAudio: Boolean) {
            streams += StreamWrite(itemId, fileId, audioIndex, subtitleIndex, automaticAudio)
        }

        override suspend fun hideFromContinueWatching(itemId: Long) {
            hidden += itemId
        }
    }

    /** As the app's: the page shows the change at once, and takes it back on a failure. */
    override val watchlist: WatchlistRepository = object : WatchlistRepository {
        override fun watchlist(): Flow<Loadable<List<ItemSummary>>> = flowOf(Loadable.Loading)
        override suspend fun add(itemId: Long) = list(itemId, true)
        override suspend fun remove(itemId: Long) = list(itemId, false)
        override suspend fun refresh() = Unit

        private fun list(itemId: Long, inList: Boolean) {
            listed += itemId to inList
            val flow = details[itemId] ?: return
            val shown = (flow.value as? Loadable.Ready)?.value ?: return
            flow.value = Loadable.Ready(shown.copy(inWatchlist = inList), fresh = true)
            listFailure?.let {
                flow.value = Loadable.Ready(shown, fresh = true)
                throw it
            }
        }
    }

    /** As the app's: videos resume unless watched, shows and seasons play their next episode. */
    override val playItem: PlayItem = PlayItem { item, fromStart ->
        when (item.kind) {
            ItemKind.Movie, ItemKind.Episode, ItemKind.Extra ->
                PlayTarget.Video(item.id, if (fromStart || item.userState.watched) 0 else item.userState.positionMs)
            ItemKind.Show, ItemKind.Season -> {
                val next = (onDecks[item.id]?.value as? Loadable.Ready)?.value
                if (next == null) PlayTarget.NothingToPlay else PlayTarget.Video(next.id, if (fromStart) 0 else next.userState.positionMs)
            }
            else -> PlayTarget.NothingToPlay
        }
    }

    override val people: PeopleRepository = object : PeopleRepository {
        override fun person(personId: Long): Flow<Loadable<PersonDetail>> =
            persons.getOrPut(personId) { MutableStateFlow(Loadable.Loading) }
    }

    override val servers: ServersRepository = object : ServersRepository {
        override val saved: Flow<List<SavedServer>> = flowOf(emptyList())
        override val active: StateFlow<String?> = this@FakeData.active
        override suspend fun use(server: SavedServer) = Unit
        override suspend fun forget(origin: String) = Unit
        override suspend fun clientId(): String = "test"
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

    /** Off unless a test says otherwise; set it before the page's model is made. */
    override var requests: RequestsRepository = RequestsRepository.None

    private fun unused(): Nothing = error("item pages don't use this")
}

/** Records what was played. */
class RecordingLauncher : PlayerLauncher {
    val played = mutableListOf<PlayRequest>()

    override fun play(request: PlayRequest) {
        played += request
    }
}

fun stream(index: Int, kind: StreamKind, codec: String, language: String? = null, default: Boolean = false, channels: Int? = null) =
    MediaStream(index = index, kind = kind, codec = codec, language = language, default = default, channels = channels)

fun file(
    id: Long,
    vararg streams: MediaStream,
    selectedAudio: Int? = null,
    selectedSubtitle: Int? = null,
    width: Int = 3840,
    height: Int = 2160,
    audioPicked: Boolean = false,
) =
    MediaFile(
        id = id,
        path = "Movie ($id).mkv",
        size = 1L shl 30,
        container = "mkv",
        width = width,
        height = height,
        videoCodec = "hevc",
        streams = streams.toList(),
        selectedAudio = selectedAudio,
        audioPicked = audioPicked,
        selectedSubtitle = selectedSubtitle,
    )

fun movie(
    id: Long = 1,
    files: List<MediaFile> = listOf(
        file(
            11,
            stream(0, StreamKind.Video, "hevc"),
            stream(1, StreamKind.Audio, "truehd", "eng", default = true, channels = 8),
            stream(2, StreamKind.Audio, "ac3", "fra", channels = 6),
            stream(3, StreamKind.Subtitle, "subrip", "eng"),
        ),
    ),
    positionMs: Long = 0,
    watched: Boolean = false,
    durationMs: Long = 9_060_000,
) = ItemDetail(
    id = id,
    libraryId = 1,
    kind = ItemKind.Movie,
    title = "Saltwind",
    year = 2021,
    durationMs = durationMs,
    rating = 8.1,
    contentRating = "PG-13",
    files = files,
    userState = UserState(positionMs = positionMs, watched = watched),
)

fun summary(id: Long, kind: ItemKind, title: String = "Item $id", parentId: Long? = null, parentIndex: Int? = null, index: Int? = null, positionMs: Long = 0, durationMs: Long? = 3_480_000, grandparentId: Long? = null) =
    ItemSummary(
        id = id,
        libraryId = 2,
        kind = kind,
        title = title,
        parentId = parentId,
        parentIndex = parentIndex,
        index = index,
        grandparentId = grandparentId,
        durationMs = durationMs,
        userState = UserState(positionMs = positionMs),
    )
