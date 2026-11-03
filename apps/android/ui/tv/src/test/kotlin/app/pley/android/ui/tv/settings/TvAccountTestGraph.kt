package app.pley.android.ui.tv.settings

import android.app.Application
import app.pley.android.auth.AuthGraph
import app.pley.android.caps.CapsGraph
import app.pley.android.common.DeviceProfile
import app.pley.android.common.FormFactor
import app.pley.android.common.SavedServer
import app.pley.android.common.Tier
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
import app.pley.android.data.SearchRepository
import app.pley.android.data.ServersRepository
import app.pley.android.data.Session
import app.pley.android.data.SessionRepository
import app.pley.android.data.SessionState
import app.pley.android.data.Settings
import app.pley.android.data.SnapshotStore
import app.pley.android.data.TrackDefaultsRepository
import app.pley.android.data.WatchStateRepository
import app.pley.android.model.CollectionSummary
import app.pley.android.model.HistoryEntry
import app.pley.android.model.HistoryPage
import app.pley.android.model.HistoryUser
import app.pley.android.model.ItemDetail
import app.pley.android.model.ItemKind
import app.pley.android.model.ItemSummary
import app.pley.android.model.Me
import app.pley.android.model.SubtitleMode
import app.pley.android.model.TrackDefaults
import app.pley.android.player.MusicControl
import app.pley.android.player.MusicPosition
import app.pley.android.player.MusicQueueState
import app.pley.android.player.NowPlaying
import app.pley.android.player.sleep.SleepChoice
import app.pley.android.player.sleep.SleepState
import app.pley.android.player.PlaybackServices
import app.pley.android.player.PlayerEngine
import app.pley.android.player.PlayerGraph
import app.pley.android.screens.PlayerLauncher
import app.pley.android.screens.PleyGraphOwner
import app.pley.android.screens.ScreensGraph
import app.pley.android.testing.auth.FakeAuthGraph
import app.pley.android.testing.caps.FakeDeviceCapabilities
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.flowOf

/**
 * A graph for the TV's account and music screens' tests: a signed-in profile,
 * track defaults the test can make the server refuse, settings in memory,
 * 20 plays of history, one album of 12 tracks over two discs, and music
 * that records what it is asked.
 */
class TvAccountTestGraph : ScreensGraph {
    val me = Me(
        id = 7,
        username = "anna",
        name = "Anna",
        managed = false,
        avatarColor = "#a78bfa",
        audioLanguage = "eng",
        subtitleMode = SubtitleMode.Forced,
        scope = "library play playlists profiles",
    )
    val sessionFlow = MutableStateFlow<Session?>(Session(SERVER, me, setOf("library", "play", "playlists", "profiles")))
    val defaults = MutableStateFlow<Loadable<TrackDefaults>>(Loadable.Ready(me.trackDefaults, fresh = true))
    val saved = mutableListOf<TrackDefaults>()
    var refuse: Exception? = null
    val settingsFlow = MutableStateFlow(Settings())
    val music = RecordingMusic()
    val history = MutableStateFlow<Loadable<HistoryPage>>(Loadable.Ready(HistoryPage(20, (1..15L).map(::play)), fresh = true))
    val removed = mutableListOf<Long>()

    override val device = DeviceProfile(FormFactor.Tv, fireTv = false, tier = Tier.T2, model = "test", totalRamBytes = 8L shl 30, lowRam = false)
    override val auth: AuthGraph = FakeAuthGraph()
    override val caps: CapsGraph = object : CapsGraph {
        override val capabilities = FakeDeviceCapabilities()
        override val policy = app.pley.android.caps.DefaultPlaybackCapsPolicy
    }
    override val player: PlayerGraph = object : PlayerGraph {
        override val engines = PlayerEngine.Factory { _, _, _ -> error("no engines here") }
        override val services: PlaybackServices get() = error("no video here")
        override val music: MusicControl = this@TvAccountTestGraph.music
    }
    override val playerLauncher: PlayerLauncher = object : PlayerLauncher {
        override fun play(request: app.pley.android.player.PlayRequest) = error("no video here")
    }

    override val data: DataGraph = object : DataGraph {
        override val session = object : SessionRepository {
            override val session: StateFlow<Session?> = sessionFlow
            override val state: StateFlow<SessionState> = MutableStateFlow(SessionState.Starting)
            override suspend fun signOut() {
                sessionFlow.value = null
            }
        }
        override val trackDefaults = object : TrackDefaultsRepository {
            override fun defaults(): Flow<Loadable<TrackDefaults>> = defaults
            override suspend fun save(defaults: TrackDefaults) {
                refuse?.let { throw it }
                saved += defaults
                this@TvAccountTestGraph.defaults.value = Loadable.Ready(defaults, fresh = true)
            }
        }
        override val settings = object : AppSettings {
            override val settings: Flow<Settings> = settingsFlow
            override suspend fun update(change: (Settings) -> Settings) {
                settingsFlow.value = change(settingsFlow.value)
            }
        }
        override val servers = object : ServersRepository {
            override val saved: Flow<List<SavedServer>> = flowOf(emptyList())
            override val active: StateFlow<String?> = MutableStateFlow(SERVER)
            override suspend fun use(server: SavedServer) = Unit
            override suspend fun forget(origin: String) = Unit
            override suspend fun clientId(): String = "android-test"
        }
        override val watchlist: app.pley.android.data.WatchlistRepository get() = unused()
        override val history = object : HistoryRepository {
            override fun history(): Flow<Loadable<HistoryPage>> = this@TvAccountTestGraph.history
            override suspend fun loadMore() {
                val shown = (this@TvAccountTestGraph.history.value as Loadable.Ready).value
                this@TvAccountTestGraph.history.value = Loadable.Ready(shown.copy(entries = shown.entries + (16..20L).map(::play)), fresh = true)
            }
            override suspend fun remove(entryId: Long) {
                removed += entryId
                val shown = (this@TvAccountTestGraph.history.value as Loadable.Ready).value
                this@TvAccountTestGraph.history.value = Loadable.Ready(shown.copy(total = shown.total - 1, entries = shown.entries.filterNot { it.id == entryId }), fresh = true)
            }
        }
        override val items = object : ItemRepository {
            override fun detail(itemId: Long): Flow<Loadable<ItemDetail>> = flowOf(
                when (itemId) {
                    ALBUM -> Loadable.Ready(ALBUM_DETAIL, fresh = true)
                    ARTIST -> Loadable.Ready(ARTIST_DETAIL, fresh = true)
                    else -> Loadable.Loading
                },
            )
            override fun children(itemId: Long): Flow<Loadable<List<ItemSummary>>> = flowOf(
                when (itemId) {
                    ALBUM -> Loadable.Ready(TRACKS, fresh = true)
                    ARTIST -> Loadable.Ready(listOf(ALBUM_DETAIL.toSummary(), ALBUM_DETAIL.toSummary().copy(id = 51, title = "Shallows", year = 2023)), fresh = true)
                    else -> Loadable.Ready(emptyList(), fresh = true)
                },
            )
            override fun onDeck(itemId: Long): Flow<Loadable<ItemSummary?>> = flowOf(Loadable.Ready(null, true))
            override fun related(itemId: Long): Flow<Loadable<List<ItemSummary>>> = flowOf(Loadable.Ready(emptyList(), true))
            override fun collectionsOf(itemId: Long): Flow<Loadable<List<CollectionSummary>>> = flowOf(Loadable.Ready(emptyList(), true))
            override suspend fun next(itemId: Long): ItemSummary? = null
            override fun prefetch(itemId: Long) = Unit
        }
        override val playItem = PlayItem { _, _ -> PlayTarget.Music(TRACKS.map { it.id }, 0, TRACKS) }
        override val watchState = object : WatchStateRepository {
            override suspend fun setWatched(itemId: Long, watched: Boolean) = Unit
            override suspend fun rate(itemId: Long, rating: Double?) = Unit
            override suspend fun chooseStreams(itemId: Long, fileId: Long?, audioIndex: Int?, subtitleIndex: Int?, automaticAudio: Boolean) = Unit
            override suspend fun hideFromContinueWatching(itemId: Long) = Unit
        }
        override val liveUpdates: LiveUpdates get() = unused()
        override val snapshots: SnapshotStore get() = unused()
        override val libraries: LibrariesRepository get() = unused()
        override val hubs: HubsRepository get() = unused()
        override val libraryItems: LibraryItemsRepository get() = unused()
        override val search: SearchRepository get() = unused()
        override val people: PeopleRepository get() = unused()
        override val collections: CollectionsRepository get() = unused()
        override val playlists: PlaylistsRepository get() = unused()
    }

    private fun unused(): Nothing = error("the account and music screens don't use this")

    /** Music that records what it is asked, its state set by the test. */
    class RecordingMusic : MusicControl {
        val now = MutableStateFlow<NowPlaying?>(null)
        val played = mutableListOf<Pair<List<Long>, Int>>()
        var toggles = 0
        var nexts = 0
        val jumps = mutableListOf<Int>()
        override val nowPlaying: StateFlow<NowPlaying?> = now
        val queueState = MutableStateFlow(MusicQueueState(TRACKS, 0))
        override val queue: StateFlow<MusicQueueState> = queueState
        override fun position(): MusicPosition = MusicPosition(60_000, 185_000, 90_000)
        override fun play(tracks: List<ItemSummary>, startIndex: Int, shuffle: Boolean) {
            played += tracks.map { it.id } to startIndex
        }
        override fun toggle() {
            toggles++
        }
        override fun next() {
            nexts++
        }
        override fun jumpTo(index: Int) {
            jumps += index
        }
        override fun pauseForVideo() = Unit

        val sleepState = MutableStateFlow(SleepState())
        var keeps = 0
        override val sleep: StateFlow<SleepState> = sleepState
        override fun setSleep(choice: SleepChoice) {
            sleepState.value = SleepState(choice)
        }
        override fun keepListening() {
            keeps++
        }
    }

    companion object {
        const val SERVER = "http://192.168.1.20:7539"
        const val ALBUM = 50L
        const val ARTIST = 40L

        val ALBUM_DETAIL = ItemDetail(
            id = ALBUM,
            libraryId = 3,
            kind = ItemKind.Album,
            title = "Deep Water",
            year = 2019,
            parentId = ARTIST,
            parentTitle = "Harbour Lights",
        )

        val ARTIST_DETAIL = ItemDetail(id = ARTIST, libraryId = 3, kind = ItemKind.Artist, title = "Harbour Lights", childCount = 2, leafCount = 24)

        /** Twelve tracks, six on each of two discs. */
        val TRACKS: List<ItemSummary> = (1..12L).map { i ->
            ItemSummary(
                id = 500 + i,
                libraryId = 3,
                kind = ItemKind.Track,
                title = "Song $i",
                index = ((i - 1) % 6 + 1).toInt(),
                parentId = ALBUM,
                parentTitle = "Deep Water",
                parentIndex = if (i <= 6) 1 else 2,
                grandparentId = ARTIST,
                grandparentTitle = "Harbour Lights",
                durationMs = 185_000,
            )
        }

        fun play(id: Long) = HistoryEntry(
            id = id,
            viewedAt = System.currentTimeMillis() / 1000 - id * 3600,
            device = "Pixel 9",
            user = HistoryUser(7, "Anna"),
            item = ItemSummary(900 + id, 1, ItemKind.Movie, "Film $id", year = 2000 + id.toInt()),
        )
    }
}

/** The tests' Application: the screens' ViewModels are made from its [graph]. */
class TvAccountTestApplication : Application(), PleyGraphOwner {
    override val screensGraph: ScreensGraph get() = graph

    companion object {
        var graph: TvAccountTestGraph = TvAccountTestGraph()
    }
}
