package app.pley.android.data

import app.pley.android.model.EntryIds
import app.pley.android.model.Hubs
import app.pley.android.model.ItemIds
import app.pley.android.model.ItemKind
import app.pley.android.model.ItemList
import app.pley.android.model.ItemSummary
import app.pley.android.model.Me
import app.pley.android.model.NewPlaylist
import app.pley.android.model.PlaylistDetail
import app.pley.android.model.PlaylistKind
import app.pley.android.model.PlaylistList
import app.pley.android.model.PlaylistPatch
import app.pley.android.model.PlaylistSummary
import app.pley.android.model.RatingRequest
import app.pley.android.model.StreamChoice
import app.pley.android.model.TrackDefaults
import app.pley.android.network.ApiException
import app.pley.android.network.OfflineException
import kotlin.math.roundToInt
import kotlinx.coroutines.async
import kotlinx.coroutines.awaitAll
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.map

/**
 * Playlists, changed at once on screen and put back when the server
 * refuses; what the server answers then replaces the guess.
 */
internal class CachedPlaylists(private val cache: ResourceCache) : PlaylistsRepository {
    private val list = Resource(SnapshotKeys.PLAYLISTS, PlaylistList.serializer()) { api, _ -> api.playlists() }

    private fun detailOf(playlistId: Long) =
        Resource(SnapshotKeys.playlist(playlistId), PlaylistDetail.serializer(), detail = true) { api, _ -> api.playlist(playlistId) }

    override fun playlists(): Flow<Loadable<List<PlaylistSummary>>> = cache.observe(list).map { it.map(PlaylistList::playlists) }

    override fun playlist(playlistId: Long): Flow<Loadable<PlaylistDetail>> = cache.observe(detailOf(playlistId))

    override suspend fun create(title: String, kind: PlaylistKind?, itemIds: List<Long>): Long =
        created(cache.api(cache.profile()).createPlaylist(NewPlaylist(title = title, kind = kind, itemIds = itemIds)))

    override suspend fun createSmart(title: String, libraryId: Long, query: String): Long =
        created(cache.api(cache.profile()).createPlaylist(NewPlaylist(title = title, smartFilter = query, libraryId = libraryId)))

    private fun created(detail: PlaylistDetail): Long {
        cache.put(detailOf(detail.id), detail)
        cache.patchKey(SnapshotKeys.PLAYLISTS) { value ->
            val playlists = (value as PlaylistList).playlists.filterNot { it.id == detail.id } + detail.toSummary()
            PlaylistList(playlists.sortedBy { it.title.lowercase() })
        }
        cache.revalidate { key, _ -> key == SnapshotKeys.PLAYLISTS }
        return detail.id
    }

    override suspend fun rename(playlistId: Long, title: String) {
        val patched = patchBoth(
            playlistId,
            summary = { it.copy(title = title) },
            detail = { it.copy(title = title) },
        )
        val answer = undoing(patched) { cache.api(cache.profile()).updatePlaylist(playlistId, PlaylistPatch(title = title)) }
        cache.put(detailOf(playlistId), answer)
    }

    override suspend fun delete(playlistId: Long) {
        val patched = cache.patchKey(SnapshotKeys.PLAYLISTS) { value ->
            val playlists = (value as PlaylistList).playlists
            if (playlists.none { it.id == playlistId }) null else PlaylistList(playlists.filterNot { it.id == playlistId })
        }
        undoing(patched) { cache.api(cache.profile()).deletePlaylist(playlistId) }
        cache.revalidate { key, _ -> key == SnapshotKeys.playlist(playlistId) }
    }

    override suspend fun add(playlistId: Long, itemIds: List<Long>) {
        cache.api(cache.profile()).addToPlaylist(playlistId, ItemIds(itemIds))
        cache.revalidate { key, _ -> key == SnapshotKeys.playlist(playlistId) || key == SnapshotKeys.PLAYLISTS }
    }

    override suspend fun remove(playlistId: Long, entryIds: List<Long>) {
        val gone = entryIds.toSet()
        // How many go: as many as asked, unless the playlist shown says otherwise.
        var removed = gone.size
        val patched = patchBoth(
            playlistId,
            summary = { it.copy(itemCount = (it.itemCount - removed).coerceAtLeast(0)) },
            detail = { d ->
                val entries = d.entries.filterNot { it.entryId in gone }
                removed = d.entries.size - entries.size
                d.copy(entries = entries, itemCount = (d.itemCount - removed).coerceAtLeast(0))
            },
        )
        try {
            undoing(patched) {
                val api = cache.api(cache.profile())
                coroutineScope { entryIds.map { async { api.removeFromPlaylist(playlistId, it) } }.awaitAll() }
            }
        } finally {
            // Durations and artwork come from the server; after a failure, what was removed before it.
            cache.revalidate { key, _ -> key == SnapshotKeys.playlist(playlistId) || key == SnapshotKeys.PLAYLISTS }
        }
    }

    override suspend fun reorder(playlistId: Long, entryIds: List<Long>) {
        val patched = cache.patchKey(SnapshotKeys.playlist(playlistId)) { value ->
            val detail = value as PlaylistDetail
            detail.copy(entries = reordered(detail, entryIds))
        }
        val answer = undoing(patched) { cache.api(cache.profile()).reorderPlaylist(playlistId, EntryIds(entryIds)) }
        cache.put(detailOf(playlistId), answer)
    }

    /** The detail's and the list's playlist changed together. */
    private fun patchBoth(
        playlistId: Long,
        summary: (PlaylistSummary) -> PlaylistSummary,
        detail: (PlaylistDetail) -> PlaylistDetail,
    ): ResourceCache.Patched {
        val inDetail = cache.patchKey(SnapshotKeys.playlist(playlistId)) { detail(it as PlaylistDetail) }
        val inList = cache.patchKey(SnapshotKeys.PLAYLISTS) { value ->
            val playlists = (value as PlaylistList).playlists
            if (playlists.none { it.id == playlistId }) null else PlaylistList(playlists.map { if (it.id == playlistId) summary(it) else it })
        }
        return ResourceCache.Patched(inDetail.keys + inList.keys, listOf(inDetail::rollback, inList::rollback))
    }

    companion object {
        /** [entryIds] first, in that order, then the rest as they were: what PUT …/order does. */
        fun reordered(detail: PlaylistDetail, entryIds: List<Long>) = run {
            val byId = detail.entries.associateBy { it.entryId }
            val first = entryIds.distinct().mapNotNull { byId[it] }
            val firstIds = first.mapTo(HashSet()) { it.entryId }
            first + detail.entries.filterNot { it.entryId in firstIds }
        }
    }
}

/** Runs [change]; puts [patched] back when it fails. */
internal suspend fun <T> undoing(patched: ResourceCache.Patched, change: suspend () -> T): T = try {
    change()
} catch (e: Throwable) {
    patched.rollback()
    throw e
}

/**
 * Watched marks, ratings and track choices: every list and page that
 * shows the item changes at once; the server is then asked again for what
 * changed with it (Continue Watching, On Deck, counts).
 */
internal class CachedWatchState(private val cache: ResourceCache) : WatchStateRepository {
    override suspend fun setWatched(itemId: Long, watched: Boolean) {
        val target = ItemPatch.find(cache.nodes().map { it.current }, itemId)
        val patch = ItemPatch.watched(itemId, watched, target)
        val patched = cache.patchAll(patch::apply)
        undoing(patched) {
            val api = cache.api(cache.profile())
            if (watched) api.markWatched(itemId) else api.markUnwatched(itemId)
        }
        val touched = patched.keys.toSet()
        // A movie watched to the end leaves My List on the server.
        cache.revalidate { key, _ ->
            key in touched || key == SnapshotKeys.HOME || key.startsWith("libhubs:") || key.startsWith("ondeck:") ||
                key == SnapshotKeys.HISTORY || key == SnapshotKeys.WATCHLIST
        }
    }

    override suspend fun hideFromContinueWatching(itemId: Long) {
        val target = ItemPatch.find(cache.nodes().map { it.current }, itemId)
        val patched = cache.patchAll(ItemPatch.hiddenFromContinueWatching(itemId, target)::apply)
        undoing(patched) { cache.api(cache.profile()).hideFromContinueWatching(itemId) }
        cache.revalidate { key, _ -> key == SnapshotKeys.HOME || key.startsWith("libhubs:") }
    }

    override suspend fun rate(itemId: Long, rating: Double?) {
        // The server keeps half stars, from 0 to 10.
        val kept = rating?.let { ((it * 2).roundToInt() / 2.0).coerceIn(0.0, 10.0) }
        val patched = cache.patchAll(ItemPatch.rating(itemId, kept)::apply)
        undoing(patched) { cache.api(cache.profile()).setRating(itemId, RatingRequest(kept)) }
    }

    override suspend fun chooseStreams(itemId: Long, fileId: Long?, audioIndex: Int?, subtitleIndex: Int?, automaticAudio: Boolean) {
        val patched = cache.patchAll(ItemPatch.streams(itemId, fileId, audioIndex, subtitleIndex, automaticAudio)::apply)
        undoing(patched) { cache.api(cache.profile()).setStreams(itemId, StreamChoice(fileId, audioIndex, subtitleIndex, automaticAudio)) }
        if (automaticAudio) {
            // Without a pick, the details' track is the server's to work out: this page is asked again too.
            cache.revalidate { key, _ -> key.startsWith("item:") }
        } else {
            othersAskedAgain(itemId)
        }
    }

    override fun streamsChosen(itemId: Long, fileId: Long?, audioIndex: Int?, subtitleIndex: Int?) {
        cache.patchAll(ItemPatch.streams(itemId, fileId, audioIndex, subtitleIndex)::apply)
        othersAskedAgain(itemId)
    }

    /** A show's episodes share the choice: their pages are asked again when shown, and a player waits for that. */
    private fun othersAskedAgain(itemId: Long) {
        cache.revalidate { key, _ -> key.startsWith("item:") && key != SnapshotKeys.item(itemId) }
    }
}

/**
 * My List, changed at once in the list, Home's My List and the item's
 * page, and put back when the server refuses; then asked again.
 */
internal class CachedWatchlist(
    private val cache: ResourceCache,
    /** What Home's My List is called until the server titles it ([DataEnvironment.myListTitle]). */
    private val myListTitle: () -> String,
) : WatchlistRepository {
    private val resource = Resource(SnapshotKeys.WATCHLIST, ItemList.serializer()) { api, _ -> api.watchlist() }

    override fun watchlist(): Flow<Loadable<List<ItemSummary>>> = cache.observe(resource).map { it.map(ItemList::items) }

    override suspend fun add(itemId: Long) = change(itemId, inList = true)

    override suspend fun remove(itemId: Long) = change(itemId, inList = false)

    override suspend fun refresh() = cache.refreshQuietly(resource)

    private suspend fun change(itemId: Long, inList: Boolean) {
        val item = if (inList) ItemPatch.find(cache.nodes().map { it.current }, itemId) else null
        val inPages = cache.patchAll(ItemPatch.inWatchlist(itemId, inList)::apply)
        val listed = cache.patchKey(SnapshotKeys.WATCHLIST) { value ->
            val items = (value as ItemList).items
            when {
                !inList -> if (items.none { it.id == itemId }) null else ItemList(items.filterNot { it.id == itemId })
                item == null || items.firstOrNull()?.id == itemId -> null
                else -> ItemList(listOf(item) + items.filterNot { it.id == itemId })
            }
        }
        // Adding something not held anywhere: the row comes with the answer asked for after.
        val inHome = cache.patchKey(SnapshotKeys.HOME) { value ->
            if (inList && item == null) null else ItemPatch.myList(value as Hubs, itemId, item, myListTitle())
        }
        val patched = ResourceCache.Patched(
            inPages.keys + listed.keys + inHome.keys,
            listOf(inPages::rollback, listed::rollback, inHome::rollback),
        )
        undoing(patched) {
            val api = cache.api(cache.profile())
            if (inList) api.addToWatchlist(itemId) else api.removeFromWatchlist(itemId)
        }
        cache.revalidate { key, _ -> key == SnapshotKeys.HOME || key == SnapshotKeys.WATCHLIST }
    }
}

/** PUT /api/me/track-defaults, which PleyApi doesn't carry yet: the profile after the change. */
fun interface TrackDefaultsRoute {
    suspend fun put(issuer: String, defaults: TrackDefaults): Me
}

/** The profile's track defaults, from GET /api/auth/me; saved with [route], at once on screen. */
internal class SessionTrackDefaults(
    private val session: SessionManager,
    private val route: TrackDefaultsRoute,
    /** Titles held start with the tracks the defaults picked: asked again once they change. */
    private val cache: ResourceCache? = null,
) : TrackDefaultsRepository {
    override fun defaults(): Flow<Loadable<TrackDefaults>> = session.meState.map { me ->
        when (me) {
            null -> Loadable.Loading
            else -> Loadable.Ready(me.value.trackDefaults, me.fresh)
        }
    }.distinctUntilChanged()

    override suspend fun save(defaults: TrackDefaults) {
        val issuer = session.issuer() ?: throw IllegalStateException("not signed in")
        val undo = session.patchMe {
            it.copy(audioLanguage = defaults.audioLanguage, subtitleLanguage = defaults.subtitleLanguage, subtitleMode = defaults.subtitleMode)
        }
        val me = try {
            route.put(issuer, defaults)
        } catch (e: ApiException) {
            undo()
            if (e.status == 403 || e.status == 404 || e.status == 405) throw TrackDefaultsUnsupported(e)
            throw e
        } catch (e: Throwable) {
            undo()
            throw e
        }
        session.updateMe(issuer, me)
        cache?.revalidate { key, _ -> key.startsWith("item:") }
    }
}

/** Pressing play as the web does it (apps/web/src/lib/usePlayItem.ts). */
internal class CachedPlayItem(private val cache: ResourceCache, private val items: CachedItems) : PlayItem {
    override suspend fun resolve(item: ItemSummary, fromStart: Boolean): PlayTarget = when (item.kind) {
        ItemKind.Movie, ItemKind.Episode, ItemKind.Extra ->
            PlayTarget.Video(item.id, if (fromStart || item.userState.watched) 0 else item.userState.positionMs)
        ItemKind.Show, ItemKind.Season -> {
            val episode = freshOr(items.onDeckOf(item.id))?.item
            if (episode == null) PlayTarget.NothingToPlay else PlayTarget.Video(episode.id, if (fromStart) 0 else episode.userState.positionMs)
        }
        ItemKind.Album -> music(children(item.id), 0)
        ItemKind.Artist -> {
            val albums = children(item.id)
            val tracks = coroutineScope { albums.map { async { children(it.id) } }.awaitAll() }.flatten()
            music(tracks, 0)
        }
        ItemKind.Track -> {
            val parent = item.parentId
            val tracks = if (parent != null) children(parent).ifEmpty { listOf(item) } else listOf(item)
            music(tracks, tracks.indexOfFirst { it.id == item.id }.coerceAtLeast(0))
        }
        ItemKind.Unknown -> PlayTarget.NothingToPlay
    }

    private fun music(tracks: List<ItemSummary>, start: Int): PlayTarget =
        if (tracks.isEmpty()) PlayTarget.NothingToPlay else PlayTarget.Music(tracks.map { it.id }, start, tracks)

    private suspend fun children(itemId: Long): List<ItemSummary> = freshOr(items.childrenOf(itemId))?.items.orEmpty()

    /** The server's answer, or what is held when it can't be reached. */
    private suspend fun <T> freshOr(resource: Resource<T>): T? = try {
        cache.fresh(resource)
    } catch (e: OfflineException) {
        cache.peek(resource) ?: throw e
    }
}
