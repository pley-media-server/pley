package app.pley.android.data

import android.content.Context
import android.content.pm.ApplicationInfo
import androidx.datastore.core.DataMigration
import androidx.datastore.core.DataStore
import androidx.datastore.core.okio.OkioStorage
import androidx.datastore.preferences.core.PreferenceDataStoreFactory
import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.PreferencesSerializer
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.ProcessLifecycleOwner
import app.pley.android.auth.AuthGraph
import app.pley.android.common.AppLanguage
import app.pley.android.common.DeviceProfile
import app.pley.android.common.Tier
import app.pley.android.network.NetworkGraph
import app.pley.android.network.PleyApi
import app.pley.android.network.ServerEvents
import java.io.File
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.emitAll
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.withContext
import okio.FileSystem
import okio.Path.Companion.toPath

/** Everything the screens read and change, made lazily. */
interface DataGraph {
    val servers: ServersRepository
    val session: SessionRepository
    val snapshots: SnapshotStore
    val libraries: LibrariesRepository
    val hubs: HubsRepository
    val libraryItems: LibraryItemsRepository
    val items: ItemRepository
    val search: SearchRepository
    val people: PeopleRepository
    val collections: CollectionsRepository
    val playlists: PlaylistsRepository
    val history: HistoryRepository
    val watchState: WatchStateRepository
    val watchlist: WatchlistRepository
    val trackDefaults: TrackDefaultsRepository
    val playItem: PlayItem
    val liveUpdates: LiveUpdates
    val settings: AppSettings

    /** Requests and Discover: movies and shows found on TMDB, asked for and followed (none unless the graph has them). */
    val requests: RequestsRepository get() = RequestsRepository.None

    /** How each library was left on this device (nothing kept unless the graph keeps it). */
    val libraryViews: LibraryViews get() = LibraryViews.None

    /** Live TV's channels, guide and watching (none unless the graph has it). */
    val liveTv: LiveTvRepository get() = LiveTvRepository.None

    /** How Live TV was left on this device (nothing kept unless the graph keeps it). */
    val liveTvViews: LiveTvViews get() = LiveTvViews.None

    /** The remote of the profile's other devices (none unless the graph has it). */
    val remote: RemoteRepository get() = RemoteRepository.None

    /** The account's language, and what decides the one shown (nothing unless the graph has it). */
    val language: LanguageRepository get() = LanguageRepository.None

    companion object {
        /**
         * The data side of the app over [network] and [auth]. Making it
         * touches nothing: the disk, the Keystore and the network are first
         * used by whatever asks first, off the main thread.
         */
        fun create(context: Context, network: NetworkGraph, auth: AuthGraph, profile: DeviceProfile): DataGraph {
            val app = context.applicationContext
            return AppDataGraph(
                DataEnvironment(
                    filesDir = { app.filesDir },
                    cacheDir = { app.cacheDir },
                    auth = auth,
                    api = network::api,
                    forgetApis = network::forget,
                    events = network::events,
                    trackDefaults = HttpTrackDefaults { issuer -> network.media(issuer) },
                    profile = profile,
                    legacy = LegacyPreferences.of(app),
                    foreground = processForeground(),
                    debuggable = (app.applicationInfo.flags and ApplicationInfo.FLAG_DEBUGGABLE) != 0,
                    scope = CoroutineScope(SupervisorJob() + Dispatchers.Default),
                    myListTitle = { AppLanguage.resources(app).getString(R.string.data_my_list) },
                ),
            )
        }

        /** Whether the app is in the foreground: some activity of it is started. */
        private fun processForeground(): Flow<Boolean> = flow {
            emitAll(ProcessLifecycleOwner.get().lifecycle.currentStateFlow.map { it.isAtLeast(Lifecycle.State.STARTED) })
        }
    }
}

/**
 * What the data side is made from. The app's comes from
 * [DataGraph.create]; tests give their own: a fake API, folders of their
 * own, a foreground they switch, the test's dispatchers and clock.
 */
class DataEnvironment(
    /** Where snapshots (`snapshots/`) and DataStores (`datastore/`) live: the app's filesDir. */
    val filesDir: () -> File,
    /** Where the HTTP caches live (`http/<server>/<user>`, as NetworkGraph makes them): the app's cacheDir. */
    val cacheDir: () -> File,
    val auth: AuthGraph,
    /** The API of a server as a profile (`null` until GET /api/auth/me says which: no HTTP cache). */
    val api: (issuer: String, userId: Long?) -> PleyApi,
    /** Lets go of a server's APIs once its HTTP caches are deleted (NetworkGraph.forget). */
    val forgetApis: (issuer: String) -> Unit = {},
    val events: (issuer: String) -> ServerEvents,
    val trackDefaults: TrackDefaultsRoute,
    val profile: DeviceProfile,
    val legacy: LegacyPreferences,
    /** Whether the app is in the foreground, where the server's events are followed. */
    val foreground: Flow<Boolean>,
    /** A debug build: software video may be allowed. */
    val debuggable: Boolean,
    /** Where work outlives a screen: requests, snapshot writes, live updates. */
    val scope: CoroutineScope,
    val io: CoroutineDispatcher = Dispatchers.IO,
    /** Decoding and encoding snapshots. */
    val compute: CoroutineDispatcher = Dispatchers.Default,
    /** Monotonic, in ms: how old answers are. */
    val clock: () -> Long = { System.nanoTime() / 1_000_000 },
    /** Unix ms: when a server was last used. */
    val wallClock: () -> Long = System::currentTimeMillis,
    /**
     * What Home's My List is called when the app adds it, until the server,
     * asked again, titles it in the language asked: the app's words, in the
     * language shown.
     */
    val myListTitle: () -> String = { ItemPatch.MY_LIST_TITLE },
)

/**
 * The startup read's files, under filesDir: the active profile's Home
 * (the snapshot's bytes, GET /api/hubs/home as JSON) and the active
 * server's origin. Both are absent when there is none.
 */
object StartupFiles {
    const val HOME = "snapshots/${FileSnapshotStore.STARTUP}/${FileSnapshotStore.STARTUP_HOME}"
    const val SERVER = "snapshots/${FileSnapshotStore.STARTUP}/server"
}

/** The data side of the app, over [env]. Each part is made the first time it is asked for. */
class AppDataGraph(private val env: DataEnvironment) : DataGraph {
    private val snapshotStore = FileSnapshotStore({ File(env.filesDir(), "snapshots") }, env.io)
    override val snapshots: SnapshotStore get() = snapshotStore

    private val serversImpl by lazy {
        DataStoreServers(
            store = preferences(DataStoreServers.FILE, listOf(WebViewMigration(env.legacy))),
            legacy = env.legacy,
            auth = env.auth,
            forgetData = ::forgetData,
            app = env.scope,
            io = env.io,
            wallClock = env.wallClock,
            startupServer = { File(env.filesDir(), StartupFiles.SERVER) },
        )
    }
    override val servers: ServersRepository get() = serversImpl

    private val cacheLazy = lazy {
        ResourceCache(sessionImpl.scopes, { env.api(it.issuer, it.userId) }, snapshotStore, env.scope, env.clock, env.compute)
    }
    private val cache by cacheLazy

    private val sessionImpl: SessionManager by lazy {
        SessionManager(serversImpl, env.auth, { env.api(it, null) }, snapshotStore, ::forgetData, env.scope, env.compute).also {
            it.onScopeChange = { if (cacheLazy.isInitialized()) cache.reset() }
            it.start()
        }
    }
    override val session: SessionRepository get() = sessionImpl

    private val live by lazy {
        ServerLiveUpdates(sessionImpl, env.foreground, env.events, cache, env.scope, receiverId = serversImpl::clientId, liveTv = ::liveTvOpen)
    }

    /** Whether Live TV is open to the profile and has channels, as its summary says; `null` until it does. */
    private fun liveTvOpen(): Flow<Boolean?> = liveTv.summary().map { load ->
        when (load) {
            Loadable.Loading -> null
            is Loadable.Ready -> load.value.available
            is Loadable.Failed -> load.stale?.available == true
        }
    }
    override val liveUpdates: LiveUpdates get() = live

    override val libraries: LibrariesRepository by lazy { CachedLibraries(cache) { live.heard(it) } }
    override val hubs: HubsRepository by lazy { CachedHubs(cache) }
    override val libraryItems: LibraryItemsRepository by lazy {
        CachedLibraryItems(cache, if (env.profile.tier == Tier.T0) LibraryListing.WINDOW_ABOVE_T0 else LibraryListing.WINDOW_ABOVE)
    }
    private val itemsImpl by lazy { CachedItems(cache) }
    override val items: ItemRepository get() = itemsImpl
    override val search: SearchRepository by lazy { CachedSearch(cache) }
    override val people: PeopleRepository by lazy { CachedPeople(cache) }
    override val collections: CollectionsRepository by lazy { CachedCollections(cache) }
    override val playlists: PlaylistsRepository by lazy { CachedPlaylists(cache) }
    override val history: HistoryRepository by lazy { CachedHistory(cache) }
    override val watchState: WatchStateRepository by lazy { CachedWatchState(cache) }
    override val watchlist: WatchlistRepository by lazy { CachedWatchlist(cache, env.myListTitle) }
    override val trackDefaults: TrackDefaultsRepository by lazy { SessionTrackDefaults(sessionImpl, env.trackDefaults, cache) }
    override val playItem: PlayItem by lazy { CachedPlayItem(cache, itemsImpl) }
    override val liveTv: LiveTvRepository by lazy { CachedLiveTv(cache) }
    override val remote: RemoteRepository by lazy {
        ServerRemote(cache, live, env.scope, env.clock, ownReceiver = serversImpl::clientId, hasPin = { sessionImpl.meState.value?.value?.hasPin == true })
    }
    override val language: LanguageRepository by lazy { SessionLanguage(sessionImpl) { if (cacheLazy.isInitialized()) cache else null } }
    override val requests: RequestsRepository by lazy {
        ServerRequests(
            cache, live, env.foreground, sessionImpl.session.map { it?.scopes }, preferences(ServerRequests.FILE), env.scope, tv = env.profile.isTv,
        )
    }

    override val settings: AppSettings by lazy {
        DataStoreSettings(
            store = preferences(DataStoreSettings.FILE),
            tv = env.profile.isTv,
            debuggable = env.debuggable,
        )
    }

    override val libraryViews: LibraryViews by lazy {
        DataStoreLibraryViews(preferences(DataStoreLibraryViews.FILE), sessionImpl.scopes)
    }

    override val liveTvViews: LiveTvViews by lazy {
        DataStoreLiveTvViews(preferences(DataStoreLiveTvViews.FILE), sessionImpl.scopes)
    }

    /** Deletes what was saved from [issuer]: its snapshots, its HTTP caches, and what is held in memory of it. */
    private suspend fun forgetData(issuer: String) {
        val server = ProfileScope.serverKey(issuer)
        if (cacheLazy.isInitialized()) {
            val shown = (cache.scopes.value as? ScopeState.Ready)?.profile
            if (shown == null || shown.server == server) cache.reset()
        }
        snapshotStore.clear(issuer)
        withContext(env.io) { File(env.cacheDir(), "http/$server").deleteRecursively() }
        // Their caches were in that folder: the next ones open theirs again.
        env.forgetApis(issuer)
    }

    /**
     * A DataStore of preferences in filesDir/[file], on [DataEnvironment.io]
     * and ending with the graph's scope (a file has one DataStore at a
     * time). Written through Okio, which replaces the file in one move on
     * every system.
     */
    private fun preferences(file: String, migrations: List<DataMigration<Preferences>> = emptyList()): DataStore<Preferences> =
        PreferenceDataStoreFactory.create(
            storage = OkioStorage(FileSystem.SYSTEM, PreferencesSerializer) { File(env.filesDir(), file).absolutePath.toPath() },
            migrations = migrations,
            scope = CoroutineScope(env.scope.coroutineContext + env.io + SupervisorJob(env.scope.coroutineContext[Job])),
        )
}
