package app.pley.android.ui.phone.requests

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
import app.pley.android.data.PeopleRepository
import app.pley.android.data.PlayItem
import app.pley.android.data.PlaylistsRepository
import app.pley.android.data.RequestsRepository
import app.pley.android.data.ScanState
import app.pley.android.data.SearchRepository
import app.pley.android.data.ServersRepository
import app.pley.android.data.SessionRepository
import app.pley.android.data.SnapshotStore
import app.pley.android.data.TrackDefaultsRepository
import app.pley.android.data.WatchStateRepository
import app.pley.android.data.WatchlistRepository
import app.pley.android.player.PlayerGraph
import app.pley.android.screens.PlayerLauncher
import app.pley.android.screens.PleyGraphOwner
import app.pley.android.screens.ScreensGraph
import app.pley.android.testing.auth.FakeAuthGraph
import app.pley.android.testing.data.FakeRequests
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.flowOf

/**
 * The app's graph for the request screens' tests: [requests] as the test
 * drives them, a server whose pictures nobody answers for, and nothing
 * else (the rest fails when asked).
 */
class RequestsTestGraph(val requests: FakeRequests = FakeRequests()) : ScreensGraph {
    override val device = DeviceProfile(FormFactor.Phone, fireTv = false, tier = Tier.T2, model = "test", totalRamBytes = 8L shl 30, lowRam = false)
    override val auth: AuthGraph = FakeAuthGraph()
    override val caps: CapsGraph get() = unused("caps")
    override val player: PlayerGraph get() = unused("player")
    override val playerLauncher: PlayerLauncher get() = unused("playerLauncher")

    override val data: DataGraph = object : DataGraph {
        override val requests: RequestsRepository get() = this@RequestsTestGraph.requests
        override val servers = object : ServersRepository {
            override val saved: Flow<List<SavedServer>> = flowOf(emptyList())
            override val active: StateFlow<String?> = MutableStateFlow(SERVER)
            override suspend fun use(server: SavedServer) = Unit
            override suspend fun forget(origin: String) = Unit
            override suspend fun clientId(): String = "android-test"
        }
        override val liveUpdates = object : LiveUpdates {
            override val scans: StateFlow<List<ScanState>> = MutableStateFlow(emptyList())
        }
        override val session: SessionRepository get() = unused("session")
        override val snapshots: SnapshotStore get() = unused("snapshots")
        override val libraries: LibrariesRepository get() = unused("libraries")
        override val hubs: HubsRepository get() = unused("hubs")
        override val libraryItems: LibraryItemsRepository get() = unused("libraryItems")
        override val items: ItemRepository get() = unused("items")
        override val search: SearchRepository get() = unused("search")
        override val people: PeopleRepository get() = unused("people")
        override val collections: CollectionsRepository get() = unused("collections")
        override val playlists: PlaylistsRepository get() = unused("playlists")
        override val history: HistoryRepository get() = unused("history")
        override val watchState: WatchStateRepository get() = unused("watchState")
        override val watchlist: WatchlistRepository get() = unused("watchlist")
        override val trackDefaults: TrackDefaultsRepository get() = unused("trackDefaults")
        override val playItem: PlayItem get() = unused("playItem")
        override val settings: AppSettings get() = unused("settings")
    }

    private fun unused(what: String): Nothing = error("the request screens don't use $what")

    companion object {
        /** Refused at once: the pictures stay placeholders. */
        const val SERVER = "http://127.0.0.1:1"
    }
}

/** The tests' Application, owning the graph the screens' ViewModels come from. Each test sets [graph]. */
class RequestsTestApplication : Application(), PleyGraphOwner {
    override val screensGraph: ScreensGraph get() = graph

    companion object {
        var graph: RequestsTestGraph = RequestsTestGraph()
    }
}
