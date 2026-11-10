package app.pley.android.ui.tv.shell

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
import app.pley.android.data.LibraryItems
import app.pley.android.data.LibraryItemsRepository
import app.pley.android.data.LibraryQuery
import app.pley.android.data.LibraryViews
import app.pley.android.data.LiveUpdates
import app.pley.android.data.Loadable
import app.pley.android.data.PeopleRepository
import app.pley.android.data.PlaylistsRepository
import app.pley.android.data.ScanState
import app.pley.android.data.SearchRepository
import app.pley.android.data.ServersRepository
import app.pley.android.data.Session
import app.pley.android.data.SessionRepository
import app.pley.android.data.SessionState
import app.pley.android.data.Settings
import app.pley.android.data.SnapshotStore
import app.pley.android.data.TrackDefaultsRepository
import app.pley.android.model.CollectionDetail
import app.pley.android.model.CollectionSummary
import app.pley.android.model.Hubs
import app.pley.android.model.LibraryFilters
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.Flow
import app.pley.android.model.PlaylistKind
import app.pley.android.model.PlaylistDetail
import app.pley.android.model.PlaylistSummary
import app.pley.android.model.SearchResults
import app.pley.android.player.PlayerGraph
import app.pley.android.screens.PlayerLauncher
import app.pley.android.screens.PleyGraphOwner
import app.pley.android.screens.ScreensGraph
import app.pley.android.testing.Fixtures
import app.pley.android.ui.tv.home.TestHomeSources
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow

/**
 * The app's graph for the TV frame's tests: Home's data from the JSON
 * fixtures (Continue Watching starts with Severance's "Half Loop"), a
 * library's listing from [listing], every other part failing when asked,
 * as the frame never asks for it.
 */
class TvTestGraph : ScreensGraph {
    val sources = TestHomeSources().apply {
        home.value = Loadable.Ready(Fixtures.decode<Hubs>("hubs/home.json"), fresh = false)
    }

    /** Every library's listing, whatever its sort and filters. */
    val listing = MutableStateFlow<Loadable<LibraryItems>>(Loadable.Loading)

    /** How each library was left on this device (its view, as kept): not at all, unless a test says. */
    val left = HashMap<Long, String>()

    /** What every library can be filtered by: nothing, unless a test says. */
    val filters = MutableStateFlow<Loadable<LibraryFilters>>(Loadable.Ready(LibraryFilters(), fresh = true))

    override val device = DeviceProfile(FormFactor.Tv, fireTv = false, tier = Tier.T2, model = "test", totalRamBytes = 4L shl 30, lowRam = false)
    override val auth: AuthGraph get() = unused("auth")
    override val caps: CapsGraph get() = unused("caps")
    override val player: PlayerGraph get() = unused("player")
    override val playerLauncher: PlayerLauncher get() = sources.launcher

    override val data: DataGraph = object : DataGraph {
        override val session = object : SessionRepository {
            override val session: StateFlow<Session?> = sources.sessionFlow
            override val state: StateFlow<SessionState> = MutableStateFlow(SessionState.Starting)
            override suspend fun signOut() = Unit
        }
        override val libraries = sources.libraries
        override val hubs = sources.hubs
        override val items = sources.items
        override val playItem = sources.playItem
        override val watchState = sources.watchState
        override val liveUpdates = object : LiveUpdates {
            override val scans: StateFlow<List<ScanState>> = sources.scans
        }
        // The playlists page asks which server its pictures are on.
        override val servers = object : ServersRepository {
            override val saved: Flow<List<SavedServer>> = flowOf(emptyList())
            override val active: StateFlow<String?> = MutableStateFlow("http://192.168.1.20:7539")
            override suspend fun use(server: SavedServer) = Unit
            override suspend fun forget(origin: String) = Unit
            override suspend fun clientId(): String = "android-test"
        }
        override val snapshots: SnapshotStore get() = unused("snapshots")
        override val libraryViews = object : LibraryViews {
            override suspend fun of(libraryId: Long): String? = left[libraryId]

            override suspend fun keep(libraryId: Long, view: String) {
                if (view.isEmpty()) left.remove(libraryId) else left[libraryId] = view
            }
        }
        override val libraryItems = object : LibraryItemsRepository {
            override fun items(libraryId: Long, query: LibraryQuery): Flow<Loadable<LibraryItems>> = listing
            override fun filters(libraryId: Long): Flow<Loadable<LibraryFilters>> = filters
        }
        // The Search tab: nothing searched yet, no recent searches.
        override val search = object : SearchRepository {
            override fun search(query: String): Flow<Loadable<SearchResults>> = flowOf(Loadable.Ready(SearchResults(), fresh = true))
            override val recent: Flow<List<String>> = flowOf(emptyList())
        }
        override val people: PeopleRepository get() = unused("people")
        // A library's Collections tab: none.
        override val collections = object : CollectionsRepository {
            override fun collections(libraryId: Long): Flow<Loadable<List<CollectionSummary>>> = flowOf(Loadable.Ready(emptyList(), fresh = true))
            override fun collection(collectionId: Long): Flow<Loadable<CollectionDetail>> = flowOf(Loadable.Loading)
        }
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
        override val trackDefaults: TrackDefaultsRepository get() = unused("trackDefaults")
        override val watchlist: app.pley.android.data.WatchlistRepository get() = unused("watchlist")
        override val settings = object : AppSettings {
            override val settings: Flow<Settings> = MutableStateFlow(Settings())
            override suspend fun update(change: (Settings) -> Settings) = Unit
        }
    }

    private fun unused(what: String): Nothing = error("the TV frame's tests don't use $what")
}

/** The tests' Application: screens' ViewModels are made from its graph, as from the app's. */
class TvTestApplication : Application(), PleyGraphOwner {
    override val screensGraph: ScreensGraph = TvTestGraph()
}
