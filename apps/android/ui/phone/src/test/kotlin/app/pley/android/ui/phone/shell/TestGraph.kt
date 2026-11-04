package app.pley.android.ui.phone.shell

import android.app.Application
import app.pley.android.auth.AuthGraph
import app.pley.android.caps.CapsGraph
import app.pley.android.caps.DefaultPlaybackCapsPolicy
import app.pley.android.caps.DeviceCapabilities
import app.pley.android.caps.PlaybackCapsPolicy
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
import app.pley.android.data.PlaylistsRepository
import app.pley.android.data.OtherDevice
import app.pley.android.data.RemoteRepository
import app.pley.android.data.RemoteSession
import app.pley.android.data.ScanState
import app.pley.android.data.SearchRepository
import app.pley.android.data.ServersRepository
import app.pley.android.data.Session
import app.pley.android.data.SessionRepository
import app.pley.android.data.SessionState
import app.pley.android.data.SnapshotStore
import app.pley.android.data.TrackDefaultsRepository
import app.pley.android.data.WatchStateRepository
import app.pley.android.model.CastRequest
import app.pley.android.model.CollectionSummary
import app.pley.android.model.Hubs
import app.pley.android.model.PlayerCommandRequest
import app.pley.android.model.PlaylistKind
import app.pley.android.model.PlaylistDetail
import app.pley.android.model.PlaylistSummary
import app.pley.android.model.SearchResults
import app.pley.android.model.ItemDetail
import app.pley.android.model.ItemSummary
import app.pley.android.model.Library
import app.pley.android.model.LibraryKind
import app.pley.android.player.MusicControl
import app.pley.android.player.NowPlaying
import app.pley.android.player.PlaybackServices
import app.pley.android.player.PlayerEngine
import app.pley.android.player.PlayerGraph
import app.pley.android.screens.PlayerLauncher
import app.pley.android.screens.PleyGraphOwner
import app.pley.android.screens.ScreensGraph
import app.pley.android.testing.auth.FakeAuthGraph
import app.pley.android.testing.caps.FakeDeviceCapabilities
import java.util.Collections
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.flowOf

/**
 * The app's graph for the shell's tests: the fake sign-in module, fake
 * capabilities, and data whose servers, live updates and music the test
 * drives. Each part records when it is first asked for ([touched]), so a
 * test can tell what the shell reached before the first frame. Repositories
 * the shell never uses fail when asked.
 */
class TestGraph(server: String? = SERVER) : ScreensGraph {
    val touched: MutableList<String> = Collections.synchronizedList(mutableListOf())
    val fakeAuth = FakeAuthGraph().apply { if (server != null) signedInTo += server }
    val fakeCaps = FakeDeviceCapabilities()
    val active = MutableStateFlow(server)
    val scans = MutableStateFlow<List<ScanState>>(emptyList())
    val music = MutableStateFlow<NowPlaying?>(null)
    val sessionState = MutableStateFlow<SessionState>(SessionState.Starting)

    /** The profile's other devices that are on, for the remote's bar. */
    val remoteDevices = MutableStateFlow<List<OtherDevice>>(emptyList())

    /** What Play on… sent, by receiver id; [castFailure] refuses the next ones. */
    val castsSent: MutableList<Pair<String, CastRequest>> = Collections.synchronizedList(mutableListOf())
    @Volatile var castFailure: Exception? = null

    override val device = DeviceProfile(FormFactor.Phone, fireTv = false, tier = Tier.T2, model = "test", totalRamBytes = 8L shl 30, lowRam = false)

    override val auth: AuthGraph
        get() = fakeAuth.also { touched += "auth" }

    override val caps: CapsGraph
        get() = object : CapsGraph {
            override val capabilities: DeviceCapabilities = fakeCaps
            override val policy: PlaybackCapsPolicy = DefaultPlaybackCapsPolicy
        }.also { touched += "caps" }

    override val player: PlayerGraph
        get() = object : PlayerGraph {
            override val engines = PlayerEngine.Factory { _, _, _ -> error("no engines in shell tests") }
            override val services: PlaybackServices get() = error("no playback in shell tests")
            override val music = object : MusicControl {
                override val nowPlaying: StateFlow<NowPlaying?> = this@TestGraph.music
                override fun pauseForVideo() = Unit
            }
        }.also { touched += "player" }

    override val playerLauncher: PlayerLauncher
        get() = object : PlayerLauncher {
            override fun play(request: app.pley.android.player.PlayRequest) {
                touched += "play"
            }
        }

    override val data: DataGraph
        get() = testData.also { touched += "data" }

    private val testData = object : DataGraph {
        override val servers = object : ServersRepository {
            override val saved: Flow<List<SavedServer>> = flowOf(emptyList())
            override val active: StateFlow<String?> = this@TestGraph.active
            override suspend fun use(server: SavedServer) = Unit
            override suspend fun forget(origin: String) = Unit
            override suspend fun clientId(): String = "android-test"
        }
        override val liveUpdates = object : LiveUpdates {
            override val scans: StateFlow<List<ScanState>> = this@TestGraph.scans
        }
        override val remote = object : RemoteRepository {
            override val sessions: StateFlow<List<RemoteSession>> = MutableStateFlow(emptyList())
            override val devices: StateFlow<List<OtherDevice>> = this@TestGraph.remoteDevices
            override val clock: () -> Long = { 0L }
            override suspend fun refresh(): List<RemoteSession> = remoteDevices.value.mapNotNull { it.session }
            override suspend fun command(sessionId: String, command: PlayerCommandRequest) = Unit
            override suspend fun stop(sessionId: String) = Unit
            override suspend fun cast(receiverId: String, request: CastRequest) {
                castFailure?.let { throw it }
                castsSent += receiverId to request
            }
            override fun here(player: Any, sessionId: String?) = Unit
        }
        override val session = object : SessionRepository {
            override val session: StateFlow<Session?> = MutableStateFlow(null)
            override val state: StateFlow<SessionState> = this@TestGraph.sessionState
            override suspend fun signOut() = Unit
        }
        override val snapshots: SnapshotStore get() = unused("snapshots")

        // Home and the Libraries tab: one library, nothing in it yet.
        override val libraries = object : LibrariesRepository {
            override fun libraries(): Flow<Loadable<List<Library>>> = flowOf(Loadable.Ready(listOf(FILMS), fresh = true))
            override suspend fun refresh() = Unit
            override suspend fun sort(ids: List<Long>) = Unit
            override suspend fun forgetOrder() = Unit
        }
        override val hubs = object : HubsRepository {
            override fun home(): Flow<Loadable<Hubs>> = flowOf(Loadable.Ready(Hubs(), fresh = true))
            override fun library(libraryId: Long): Flow<Loadable<Hubs>> = flowOf(Loadable.Ready(Hubs(), fresh = true))
            override suspend fun refresh() = Unit
        }
        override val libraryItems: LibraryItemsRepository get() = unused("libraryItems")
        // An item page opened in a tab: its details are still on their way.
        override val items = object : ItemRepository {
            override fun detail(itemId: Long): Flow<Loadable<ItemDetail>> = flowOf(Loadable.Loading)
            override fun children(itemId: Long): Flow<Loadable<List<ItemSummary>>> = unused("children")
            override fun onDeck(itemId: Long): Flow<Loadable<ItemSummary?>> = unused("onDeck")
            override fun related(itemId: Long): Flow<Loadable<List<ItemSummary>>> = unused("related")
            override fun collectionsOf(itemId: Long): Flow<Loadable<List<CollectionSummary>>> = unused("collectionsOf")
            override suspend fun next(itemId: Long): ItemSummary? = unused("next")
            override fun prefetch(itemId: Long) = Unit
        }
        // The Search tab: nothing searched yet, no recent searches.
        override val search = object : SearchRepository {
            override fun search(query: String): Flow<Loadable<SearchResults>> = flowOf(Loadable.Ready(SearchResults(), fresh = true))
            override val recent: Flow<List<String>> = flowOf(emptyList())
        }
        override val people: PeopleRepository get() = unused("people")
        override val collections: CollectionsRepository get() = unused("collections")
        // The Playlists tab: none yet.
        override val playlists = object : PlaylistsRepository {
            override fun playlists(): Flow<Loadable<List<PlaylistSummary>>> = flowOf(Loadable.Ready(emptyList(), fresh = true))
            override fun playlist(playlistId: Long): Flow<Loadable<PlaylistDetail>> = flowOf(Loadable.Loading)
            override suspend fun create(title: String, kind: PlaylistKind?, itemIds: List<Long>): Long = unused("create")
            override suspend fun createSmart(title: String, libraryId: Long, query: String): Long = unused("createSmart")
            override suspend fun rename(playlistId: Long, title: String) = unused("rename")
            override suspend fun delete(playlistId: Long) = unused("delete")
            override suspend fun add(playlistId: Long, itemIds: List<Long>) = unused("add")
            override suspend fun remove(playlistId: Long, entryIds: List<Long>) = unused("remove")
            override suspend fun reorder(playlistId: Long, entryIds: List<Long>) = unused("reorder")
        }
        override val history: HistoryRepository get() = unused("history")
        override val watchState: WatchStateRepository get() = unused("watchState")
        override val trackDefaults: TrackDefaultsRepository get() = unused("trackDefaults")
        override val watchlist: app.pley.android.data.WatchlistRepository get() = unused("watchlist")
        override val playItem: PlayItem get() = unused("playItem")
        override val settings: AppSettings get() = unused("settings")
    }

    private fun unused(what: String): Nothing = error("the shell doesn't use $what")

    companion object {
        const val SERVER = "http://192.168.1.20:7539"

        /** The one library Home and the Libraries tab know of. */
        val FILMS = Library(1, "Films", LibraryKind.Movies, itemCount = 12)
    }
}

/**
 * The test's Application: the app's own is the graph's owner, which
 * screens' ViewModels are made from. Each test sets [graph].
 */
class TestApplication : Application(), PleyGraphOwner {
    override val screensGraph: ScreensGraph get() = graph ?: error("no TestGraph set")

    companion object {
        var graph: TestGraph? = null
    }
}
