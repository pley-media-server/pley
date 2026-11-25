package app.pley.android.player.music

import android.content.Context
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
import app.pley.android.data.SearchRepository
import app.pley.android.data.ServersRepository
import app.pley.android.data.Session
import app.pley.android.data.SessionRepository
import app.pley.android.data.SessionState
import app.pley.android.data.SnapshotStore
import app.pley.android.data.TrackDefaultsRepository
import app.pley.android.data.WatchStateRepository
import app.pley.android.data.WatchlistRepository
import app.pley.android.model.CollectionSummary
import app.pley.android.model.Hubs
import app.pley.android.model.ItemDetail
import app.pley.android.model.ItemSummary
import app.pley.android.model.Library
import app.pley.android.model.PlaylistDetail
import app.pley.android.model.PlaylistKind
import app.pley.android.model.PlaylistSummary
import app.pley.android.network.Discovery
import app.pley.android.network.LocalNetworkAccess
import app.pley.android.network.NetworkGraph
import app.pley.android.network.NetworkSweep
import app.pley.android.network.PleyApi
import app.pley.android.network.ServerEvents
import app.pley.android.network.ServerProbe
import app.pley.android.player.MusicControl
import app.pley.android.player.PlaybackServices
import app.pley.android.player.PlayerEngine
import app.pley.android.player.PlayerGraph
import app.pley.android.player.music.library.MusicLibraryGraph
import app.pley.android.testing.data.TestData
import app.pley.android.testing.network.FakePleyApi
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.flow
import okhttp3.OkHttpClient

/**
 * The player graph as [MusicService] reads it, for a test that stands the
 * real service up: [playback] as the app's music, and the real music
 * library over a profile signed in to a server that answers as [api] does
 * (the fixtures' household, or what a test has it answer). Nothing reaches
 * a network.
 */
internal class MusicServiceGraph(context: Context, playback: MusicPlayback, api: FakePleyApi) : PlayerGraph {
    override val engines: PlayerEngine.Factory get() = unused()
    override val services: PlaybackServices get() = unused()
    override val music: MusicControl = ServiceMusicControl(context, playback)
    override val musicLibrary: MusicLibraryGraph = MusicLibraryGraph(context, SignedInData(api, api.issuer), OneServer(api))
}

/** The app's data as the music library reads it: a profile signed in, each list the server's own answer ([api]'s). */
private class SignedInData(private val api: PleyApi, issuer: String) : DataGraph {
    private val signedIn = Session(issuer, me = null, scopes = setOf("library", "play", "playlists"))

    override val session: SessionRepository = object : SessionRepository {
        override val session: StateFlow<Session?> = MutableStateFlow(signedIn)
        override val state: StateFlow<SessionState> = MutableStateFlow(SessionState.SignedIn(signedIn))

        override suspend fun signOut() = Unit
    }

    override val libraries: LibrariesRepository = object : LibrariesRepository {
        override fun libraries(): Flow<Loadable<List<Library>>> = fresh { api.libraries() }

        override suspend fun refresh() = Unit

        override suspend fun sort(ids: List<Long>) = Unit

        override suspend fun forgetOrder() = Unit
    }

    override val hubs: HubsRepository = object : HubsRepository {
        override fun home(): Flow<Loadable<Hubs>> = fresh { api.homeHubs() }

        override fun library(libraryId: Long): Flow<Loadable<Hubs>> = fresh { api.libraryHubs(libraryId) }

        override suspend fun refresh() = Unit
    }

    override val items: ItemRepository = object : ItemRepository {
        override fun detail(itemId: Long): Flow<Loadable<ItemDetail>> = fresh { api.item(itemId) }

        override fun children(itemId: Long): Flow<Loadable<List<ItemSummary>>> = fresh { api.children(itemId).items }

        override fun onDeck(itemId: Long): Flow<Loadable<ItemSummary?>> = fresh { api.onDeck(itemId).item }

        override fun related(itemId: Long): Flow<Loadable<List<ItemSummary>>> = unused()

        override fun collectionsOf(itemId: Long): Flow<Loadable<List<CollectionSummary>>> = unused()

        override suspend fun next(itemId: Long): ItemSummary? = unused()

        override fun prefetch(itemId: Long) = Unit
    }

    override val playlists: PlaylistsRepository = object : PlaylistsRepository {
        override fun playlists(): Flow<Loadable<List<PlaylistSummary>>> = fresh { api.playlists().playlists }

        override fun playlist(playlistId: Long): Flow<Loadable<PlaylistDetail>> = fresh { api.playlist(playlistId) }

        override suspend fun create(title: String, kind: PlaylistKind?, itemIds: List<Long>): Long = unused()

        override suspend fun createSmart(title: String, libraryId: Long, query: String): Long = unused()

        override suspend fun rename(playlistId: Long, title: String) = unused()

        override suspend fun delete(playlistId: Long) = unused()

        override suspend fun add(playlistId: Long, itemIds: List<Long>) = unused()

        override suspend fun remove(playlistId: Long, entryIds: List<Long>) = unused()

        override suspend fun reorder(playlistId: Long, entryIds: List<Long>) = unused()
    }

    override val servers: ServersRepository get() = unused()
    override val snapshots: SnapshotStore get() = unused()
    override val libraryItems: LibraryItemsRepository get() = unused()
    override val search: SearchRepository get() = unused()
    override val people: PeopleRepository get() = unused()
    override val collections: CollectionsRepository get() = unused()
    override val history: HistoryRepository get() = unused()
    override val watchState: WatchStateRepository get() = unused()
    override val watchlist: WatchlistRepository get() = unused()
    override val trackDefaults: TrackDefaultsRepository get() = unused()
    override val playItem: PlayItem get() = unused()
    override val liveUpdates: LiveUpdates get() = unused()
    override val settings: AppSettings get() = unused()
}

/** The network as the music library reads it: one server, [api], and pictures that are never a picture. */
private class OneServer(private val api: PleyApi) : NetworkGraph {
    override val root: OkHttpClient = TestData.offlineHttp()
    override val images: OkHttpClient get() = root

    override fun api(issuer: String, userId: Long?): PleyApi = api

    override fun media(issuer: String): OkHttpClient = root

    override fun subtitles(issuer: String): OkHttpClient = root

    override fun events(issuer: String): ServerEvents = unused()

    override val discovery: Discovery get() = unused()
    override val sweep: NetworkSweep get() = unused()
    override val probe: ServerProbe get() = unused()
    override val localNetwork: LocalNetworkAccess get() = unused()
}

/** What [read] gives, as the server's own answer. */
private fun <T> fresh(read: suspend () -> T): Flow<Loadable<T>> = flow { emit(Loadable.Ready(read(), fresh = true)) }

private fun unused(): Nothing = error("The music service doesn't read this")
