package app.pley.android.network

import app.pley.android.common.Addresses
import app.pley.android.common.ImageUrls
import app.pley.android.model.Added
import app.pley.android.model.Approval
import app.pley.android.model.Asked
import app.pley.android.model.CollectionDetail
import app.pley.android.model.Decline
import app.pley.android.model.DiscoverDetail
import app.pley.android.model.DiscoverList
import app.pley.android.model.DiscoverPage
import app.pley.android.model.ItemRequest
import app.pley.android.model.MediaRequest
import app.pley.android.model.NewRequest
import app.pley.android.model.RequestList
import app.pley.android.model.RequestOptions
import app.pley.android.model.RequestQuery
import app.pley.android.model.RequestSummary
import app.pley.android.model.TitleKind
import app.pley.android.model.CollectionList
import app.pley.android.model.EntryIds
import app.pley.android.model.Health
import app.pley.android.model.HistoryPage
import app.pley.android.model.Hubs
import app.pley.android.model.ImageKind
import app.pley.android.model.ItemCollections
import app.pley.android.model.ItemDetail
import app.pley.android.model.ItemIds
import app.pley.android.model.ItemList
import app.pley.android.model.ItemsPage
import app.pley.android.model.Library
import app.pley.android.model.LibraryFilters
import app.pley.android.model.LibraryIds
import app.pley.android.model.LibrarySort
import app.pley.android.model.LiveCategory
import app.pley.android.model.LiveChannel
import app.pley.android.model.LiveChannelPage
import app.pley.android.model.LiveGroups
import app.pley.android.model.LiveGuide
import app.pley.android.model.LiveSummary
import app.pley.android.model.LivePlan
import app.pley.android.model.LiveWatching
import app.pley.android.model.ConversionInfo
import app.pley.android.model.AddSubtitle
import app.pley.android.model.MaybeItem
import app.pley.android.model.NetworkReport
import app.pley.android.model.PlaybackNetwork
import app.pley.android.model.SpeedSample
import app.pley.android.model.SubtitleResults
import app.pley.android.model.SubtitleTrack
import app.pley.android.model.Me
import app.pley.android.model.NewPlaylist
import app.pley.android.model.PersonDetail
import app.pley.android.model.PlaybackInfo
import app.pley.android.model.PlaybackRequest
import app.pley.android.model.PlaylistDetail
import app.pley.android.model.PlaylistList
import app.pley.android.model.PlaylistPatch
import app.pley.android.model.PleyJson
import app.pley.android.model.ProgressReport
import app.pley.android.model.RatingRequest
import app.pley.android.model.SearchResults
import app.pley.android.model.ServerInfo
import app.pley.android.model.SessionStatus
import app.pley.android.model.SortOrder
import app.pley.android.model.StreamChoice
import app.pley.android.model.TrickplayInfo
import app.pley.android.model.WatchRequest
import java.io.IOException
import java.util.concurrent.TimeUnit
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.job
import kotlinx.coroutines.withContext
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.DeserializationStrategy
import kotlinx.serialization.SerializationStrategy
import kotlinx.serialization.builtins.ListSerializer
import kotlinx.serialization.builtins.nullable
import okhttp3.HttpUrl
import okhttp3.HttpUrl.Companion.toHttpUrl
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody
import okhttp3.RequestBody.Companion.toRequestBody

/**
 * [PleyApi] over HTTP, for the server at [issuer] (its origin). [client]
 * gives the client with the server's tokens; it is made the first time a
 * request goes out, off the main thread, as making it reads the tokens.
 *
 * Query parameters always go in the same order, defaults included where
 * the server's answer depends on them: a library listing's ETag is made
 * from its query (apps/server/src/api/libraries.rs, listing_etag), so the
 * same listing asked twice revalidates instead of downloading again.
 */
class HttpPleyApi(
    issuer: String,
    client: () -> OkHttpClient,
    private val io: CoroutineDispatcher = Dispatchers.IO,
) : PleyApi {
    /** The server's origin: `http://nas:7539`. */
    val issuer: String = Addresses.origin(issuer) ?: issuer.trimEnd('/')

    private val base: HttpUrl = this.issuer.toHttpUrl()
    private val lazyClient = lazy(client)

    private suspend fun http(): OkHttpClient =
        if (lazyClient.isInitialized()) lazyClient.value else withContext(io) { lazyClient.value }

    // ---------------------------------------------------------------- server

    override suspend fun health(): Health = get(Health.serializer(), "api", "health")

    override suspend fun server(): ServerInfo = get(ServerInfo.serializer(), "api", "server")

    override suspend fun me(): Me = get(Me.serializer(), "api", "auth", "me")

    override fun avatarUrl(userId: Long, version: String?): String =
        picture(listOf("api", "users", userId, "avatar"), width = null, version = version)

    override fun url(path: String, width: Int?): String {
        val absolute = when {
            path.startsWith("http://", ignoreCase = true) || path.startsWith("https://", ignoreCase = true) -> path
            path.startsWith("/") -> issuer + path
            else -> "$issuer/$path"
        }
        return if (width != null) ImageUrls.sized(absolute, width) else absolute
    }

    // ---------------------------------------------------------------- libraries

    override suspend fun libraries(): List<Library> = get(ListSerializer(Library.serializer()), "api", "libraries")

    override suspend fun library(id: Long): Library = get(Library.serializer(), "api", "libraries", id)

    override suspend fun sortLibraries(order: LibraryIds): List<Library> =
        send("PUT", ListSerializer(Library.serializer()), json(LibraryIds.serializer(), order), "api", "me", "libraries", "order")

    override suspend fun forgetLibraryOrder(): List<Library> =
        http().fetch(jsonRequest(address(listOf("api", "me", "libraries", "order"))).delete().build()) {
            it.decode(ListSerializer(Library.serializer()))
        }

    override suspend fun libraryItems(
        libraryId: Long,
        sort: LibrarySort,
        order: SortOrder?,
        watched: Boolean?,
        genre: String?,
        year: Int?,
        offset: Int,
        limit: Int?,
    ): ItemsPage = get(
        ItemsPage.serializer(),
        "api", "libraries", libraryId, "items",
        query = listOf(
            "sort" to sort.wire,
            "order" to order?.wire,
            "watched" to watched?.toString(),
            "genre" to genre,
            "year" to year?.toString(),
            "offset" to offset.toString(),
            "limit" to limit?.toString(),
        ),
    )

    override suspend fun libraryFilters(libraryId: Long): LibraryFilters =
        get(LibraryFilters.serializer(), "api", "libraries", libraryId, "filters")

    override suspend fun libraryHubs(libraryId: Long): Hubs = get(Hubs.serializer(), "api", "libraries", libraryId, "hubs")

    override suspend fun libraryCollections(libraryId: Long): CollectionList =
        get(CollectionList.serializer(), "api", "libraries", libraryId, "collections")

    override suspend fun collection(id: Long): CollectionDetail = get(CollectionDetail.serializer(), "api", "collections", id)

    override fun collectionImageUrl(id: Long, kind: ImageKind, width: Int?, version: String?): String {
        require(kind != ImageKind.Thumb) { "collections have a poster and a backdrop only" }
        return picture(listOf("api", "collections", id, "images", kind.wire), width, version)
    }

    // ---------------------------------------------------------------- playlists (reading)

    override suspend fun playlists(): PlaylistList = get(PlaylistList.serializer(), "api", "playlists")

    override suspend fun playlist(id: Long): PlaylistDetail = get(PlaylistDetail.serializer(), "api", "playlists", id)

    // ---------------------------------------------------------------- items

    override suspend fun item(id: Long): ItemDetail = get(ItemDetail.serializer(), "api", "items", id)

    override suspend fun children(id: Long): ItemList = get(ItemList.serializer(), "api", "items", id, "children")

    override suspend fun onDeck(id: Long): MaybeItem = get(MaybeItem.serializer(), "api", "items", id, "on-deck")

    override suspend fun next(id: Long): MaybeItem = get(MaybeItem.serializer(), "api", "items", id, "next")

    override suspend fun related(id: Long, limit: Int): ItemList =
        get(ItemList.serializer(), "api", "items", id, "related", query = listOf("limit" to limit.toString()))

    override fun itemImageUrl(id: Long, kind: ImageKind, width: Int?, version: String?): String =
        picture(listOf("api", "items", id, "images", kind.wire), width, version)

    override suspend fun itemCollections(id: Long): ItemCollections =
        get(ItemCollections.serializer(), "api", "items", id, "collections")

    override suspend fun person(id: Long): PersonDetail = get(PersonDetail.serializer(), "api", "people", id)

    override fun personPhotoUrl(id: Long, width: Int?, version: String?): String =
        picture(listOf("api", "people", id, "photo"), width, version)

    override suspend fun history(offset: Int, limit: Int, itemId: Long?, libraryId: Long?, since: Long?): HistoryPage = get(
        HistoryPage.serializer(),
        "api", "history",
        query = listOf(
            "offset" to offset.toString(),
            "limit" to limit.toString(),
            "item_id" to itemId?.toString(),
            "library_id" to libraryId?.toString(),
            "since" to since?.toString(),
        ),
    )

    override suspend fun homeHubs(): Hubs = get(Hubs.serializer(), "api", "hubs", "home")

    override suspend fun watchlist(): ItemList = get(ItemList.serializer(), "api", "watchlist")

    override suspend fun search(query: String, limit: Int): SearchResults = get(
        SearchResults.serializer(),
        "api", "search",
        query = listOf("q" to query, "limit" to limit.toString()),
    )

    // ---------------------------------------------------------------- playback

    override suspend fun startPlayback(request: PlaybackRequest): PlaybackInfo =
        send("POST", PlaybackInfo.serializer(), json(PlaybackRequest.serializer(), request), "api", "playback")

    override suspend fun convert(request: PlaybackRequest): ConversionInfo =
        send("POST", ConversionInfo.serializer(), json(PlaybackRequest.serializer(), request), "api", "conversions")

    override suspend fun conversion(id: String): ConversionInfo = get(ConversionInfo.serializer(), "api", "conversions", id)

    override suspend fun cancelConversion(id: String) = call("DELETE", null, "api", "conversions", id)

    override suspend fun playbackStatus(sessionId: String): SessionStatus =
        get(SessionStatus.serializer(), "api", "playback", sessionId)

    override suspend fun stopPlayback(sessionId: String) = call("DELETE", null, "api", "playback", sessionId)

    override suspend fun playbackNetwork(clientId: String): PlaybackNetwork =
        get(PlaybackNetwork.serializer(), "api", "playback", "network", query = listOf("client_id" to clientId))

    override suspend fun reportNetwork(report: NetworkReport): PlaybackNetwork {
        // Exactly one of the two, and a ceiling of null is sent as null (it forgets).
        val fields = buildMap {
            put("client_id", JsonPrimitive(report.clientId))
            when (report) {
                is NetworkReport.Probe -> {
                    put("probe_kbps", JsonPrimitive(report.kbps))
                    put("probe_complete", JsonPrimitive(report.complete))
                }
                is NetworkReport.Ceiling -> put("ceiling_kbps", report.kbps?.let(::JsonPrimitive) ?: JsonNull)
            }
        }
        return send("PUT", PlaybackNetwork.serializer(), JsonObject(fields).toString().toRequestBody(JSON), "api", "playback", "network")
    }

    override suspend fun speedTest(bytes: Long, readMs: Long, timeoutMs: Long): SpeedSample {
        val client = http().newBuilder().callTimeout(timeoutMs, TimeUnit.MILLISECONDS).build()
        val request = Request.Builder()
            .url(address(listOf("api", "playback", "speedtest"), listOf("bytes" to bytes.toString())))
            // The bytes as they travel: nothing to unpack, nothing counted twice.
            .header("Accept-Encoding", "identity")
            .header("Cache-Control", "no-store")
            .build()
        return withContext(io) {
            val call = client.newCall(request)
            val cancel = coroutineContext.job.invokeOnCompletion { call.cancel() }
            try {
                call.execute().use { response ->
                    response.expectSuccess()
                    SpeedReader(bytes, readMs).read(response.body.source())
                }
            } catch (e: IOException) {
                throw failure(e)
            } finally {
                cancel.dispose()
            }
        }
    }

    override suspend fun searchSubtitles(itemId: Long, fileId: Long?, languages: List<String>?): SubtitleResults {
        val query = listOf("file_id" to fileId?.toString(), "language" to languages?.takeIf { it.isNotEmpty() }?.joinToString(","))
        val request = jsonRequest(address(listOf("api", "items", itemId, "subtitles", "search"), query)).get().build()
        return http().fetch(request) { response ->
            if (response.code == 503 && notSetUp(response)) SubtitleResults(setup = false) else response.decode(SubtitleResults.serializer())
        }
    }

    override suspend fun addSubtitle(itemId: Long, add: AddSubtitle): SubtitleTrack =
        send("POST", SubtitleTrack.serializer(), json(AddSubtitle.serializer(), add), "api", "items", itemId, "subtitles")

    override fun playbackFileUrl(sessionId: String, file: String): String =
        address(listOf("api", "playback", sessionId, file)).toString()

    override fun fileStreamUrl(fileId: Long): String = address(listOf("api", "files", fileId, "stream")).toString()

    override suspend fun subtitles(fileId: Long, index: Int): SubtitleText {
        val request = Request.Builder()
            .url(address(listOf("api", "files", fileId, "subtitles", "$index.vtt")))
            .header("Accept", "text/vtt")
            .build()
        return http().fetch(request) { response ->
            response.expectSuccess()
            val complete = response.header(SUBTITLES_COMPLETE_HEADER)?.trim() != "0"
            SubtitleText(response.body.string(), complete)
        }
    }

    override suspend fun trickplay(fileId: Long): TrickplayInfo? {
        val request = jsonRequest(address(listOf("api", "files", fileId, "trickplay"))).build()
        return http().fetch(request) { response ->
            // Not made yet: the server queued them, and says 404 meanwhile.
            if (response.code == 404) null else response.decode(TrickplayInfo.serializer())
        }
    }

    override fun trickplaySheetUrl(fileId: Long, sheet: Int, version: String): String =
        picture(listOf("api", "files", fileId, "trickplay", "$sheet.jpg"), width = null, version = version)

    override fun chapterImageUrl(fileId: Long, chapter: Int, version: String?): String =
        picture(listOf("api", "files", fileId, "chapters", "$chapter.jpg"), width = null, version = version)

    override suspend fun reportProgress(itemId: Long, report: ProgressReport) =
        call("POST", json(ProgressReport.serializer(), report), "api", "items", itemId, "progress")

    override suspend fun markWatched(itemId: Long) = call("POST", EMPTY, "api", "items", itemId, "watched")

    override suspend fun markUnwatched(itemId: Long) = call("POST", EMPTY, "api", "items", itemId, "unwatched")

    override suspend fun setStreams(itemId: Long, choice: StreamChoice) =
        call("PUT", json(StreamChoice.serializer(), choice), "api", "items", itemId, "streams")

    override suspend fun setRating(itemId: Long, rating: RatingRequest) =
        call("PUT", json(RatingRequest.serializer(), rating), "api", "items", itemId, "rating")

    override suspend fun deleteHistoryEntry(id: Long) = call("DELETE", null, "api", "history", id)

    override suspend fun hideFromContinueWatching(itemId: Long) = call("DELETE", null, "api", "continue-watching", itemId)

    override suspend fun addToWatchlist(itemId: Long) = call("PUT", EMPTY, "api", "watchlist", itemId)

    override suspend fun removeFromWatchlist(itemId: Long) = call("DELETE", null, "api", "watchlist", itemId)

    // ---------------------------------------------------------------- live TV

    override suspend fun liveSummary(): LiveSummary = get(LiveSummary.serializer(), "api", "live")

    override suspend fun liveChannels(): List<LiveChannel> = get(ListSerializer(LiveChannel.serializer()), "api", "live", "channels")

    override suspend fun liveGroups(): LiveGroups = get(LiveGroups.serializer(), "api", "live", "groups")

    override suspend fun liveBrowse(category: LiveCategory, query: String?, offset: Int, limit: Int, around: Long?): LiveChannelPage = get(
        LiveChannelPage.serializer(),
        "api", "live", "browse",
        query = listOf(
            "category" to category.wire,
            "group" to (category as? LiveCategory.Group)?.name,
            "q" to query?.trim()?.ifEmpty { null },
            "offset" to offset.takeIf { around == null }?.toString(),
            "limit" to limit.toString(),
            "around" to around?.toString(),
        ),
    )

    override suspend fun liveChannel(id: Long): LiveChannel = get(LiveChannel.serializer(), "api", "live", "channels", id)

    override suspend fun liveGuide(from: Long, to: Long, ids: List<Long>?): LiveGuide = get(
        LiveGuide.serializer(),
        "api", "live", "guide",
        query = listOf("from" to from.toString(), "to" to to.toString(), "ids" to ids?.joinToString(",")),
    )

    override suspend fun setFavoriteChannel(channelId: Long, favorite: Boolean) =
        if (favorite) {
            call("PUT", EMPTY, "api", "live", "channels", channelId, "favorite")
        } else {
            call("DELETE", null, "api", "live", "channels", channelId, "favorite")
        }

    override suspend fun watchChannel(channelId: Long, request: WatchRequest): LiveWatching =
        send("POST", LiveWatching.serializer(), json(WatchRequest.serializer(), request), "api", "live", "channels", channelId, "watch")

    override suspend fun livePlan(sessionId: String): LivePlan? = get(LivePlan.serializer().nullable, "api", "live", "sessions", sessionId, "plan")

    override suspend fun leaveChannel(sessionId: String) {
        val request = jsonRequest(address(listOf("api", "live", "sessions", sessionId))).delete().build()
        // Gone already (left by itself, or stopped by an administrator): left all the same.
        http().fetch(request) { if (it.code != 404) it.expectSuccess() }
    }

    // ---------------------------------------------------------------- playlists (changing)

    override suspend fun createPlaylist(playlist: NewPlaylist): PlaylistDetail =
        send("POST", PlaylistDetail.serializer(), json(NewPlaylist.serializer(), playlist), "api", "playlists")

    override suspend fun updatePlaylist(id: Long, patch: PlaylistPatch): PlaylistDetail =
        send("PATCH", PlaylistDetail.serializer(), json(PlaylistPatch.serializer(), patch), "api", "playlists", id)

    override suspend fun deletePlaylist(id: Long) = call("DELETE", null, "api", "playlists", id)

    override suspend fun addToPlaylist(id: Long, items: ItemIds): Added =
        send("POST", Added.serializer(), json(ItemIds.serializer(), items), "api", "playlists", id, "items")

    override suspend fun removeFromPlaylist(id: Long, entryId: Long) =
        call("DELETE", null, "api", "playlists", id, "items", entryId)

    override suspend fun reorderPlaylist(id: Long, entries: EntryIds): PlaylistDetail =
        send("PUT", PlaylistDetail.serializer(), json(EntryIds.serializer(), entries), "api", "playlists", id, "order")

    // ---------------------------------------------------------------- requests

    override suspend fun requestSummary(): RequestSummary = get(RequestSummary.serializer(), "api", "requests", "summary")

    override suspend fun discover(list: DiscoverList, page: Int): DiscoverPage =
        get(DiscoverPage.serializer(), "api", "discover", list.wire, query = listOf("page" to page.toString()))

    override suspend fun discoverSearch(query: String, page: Int, missing: Boolean): DiscoverPage = get(
        DiscoverPage.serializer(),
        "api", "discover", "search",
        query = listOf("q" to query, "page" to page.toString(), "missing" to if (missing) "1" else null),
    )

    override suspend fun discoverTitle(kind: TitleKind, tmdbId: Long): DiscoverDetail =
        get(DiscoverDetail.serializer(), "api", "discover", kind.wire, tmdbId)

    override suspend fun itemRequest(itemId: Long): ItemRequest = get(ItemRequest.serializer(), "api", "items", itemId, "request")

    override suspend fun requests(query: RequestQuery): RequestList = get(
        RequestList.serializer(),
        "api", "requests",
        query = listOf(
            "status" to query.statuses.takeIf { it.isNotEmpty() }?.joinToString(",") { it.wire },
            "attention" to if (query.attention) "1" else null,
            "kind" to query.kind?.wire,
            "user_id" to query.userId?.toString(),
            "q" to query.q?.takeIf { it.isNotBlank() },
            "order" to if (query.oldest) "oldest" else null,
            "mine" to if (query.mine) "1" else null,
            "unseen" to if (query.unseen) "1" else null,
            "changed_since" to query.changedSince?.toString(),
            "offset" to query.offset.toString(),
            "limit" to query.limit?.toString(),
        ),
    )

    override suspend fun request(id: Long): MediaRequest = get(MediaRequest.serializer(), "api", "requests", id)

    override suspend fun ask(request: NewRequest): Asked {
        val call = jsonRequest(address(listOf("api", "requests"))).post(json(NewRequest.serializer(), request)).build()
        // 201: a new request; 200: what was asked for is asked for already, and now followed.
        return http().fetch(call) { response -> Asked(response.decode(MediaRequest.serializer()), created = response.code == 201) }
    }

    override suspend fun deleteRequest(id: Long) = call("DELETE", null, "api", "requests", id)

    override suspend fun unwatchRequest(id: Long) = call("DELETE", null, "api", "requests", id, "watch")

    override suspend fun requestsSeen() = call("POST", EMPTY, "api", "requests", "seen")

    override suspend fun approveRequest(id: Long, approval: Approval): MediaRequest =
        send("POST", MediaRequest.serializer(), json(Approval.serializer(), approval), "api", "requests", id, "approve")

    override suspend fun declineRequest(id: Long, decline: Decline): MediaRequest =
        send("POST", MediaRequest.serializer(), json(Decline.serializer(), decline), "api", "requests", id, "decline")

    override suspend fun requestHandled(id: Long): MediaRequest =
        send("POST", MediaRequest.serializer(), EMPTY, "api", "requests", id, "handled")

    override suspend fun requestOptions(): RequestOptions = get(RequestOptions.serializer(), "api", "requests", "options")

    // ---------------------------------------------------------------- plumbing

    /** An address on the server: each segment encoded, then the query in the order given, `null`s left out. */
    private fun address(segments: List<Any>, query: List<Pair<String, String?>> = emptyList()): HttpUrl {
        val builder = base.newBuilder()
        for (segment in segments) builder.addPathSegment(segment.toString())
        for ((name, value) in query) if (value != null) builder.addQueryParameter(name, value)
        return builder.build()
    }

    /** A picture's address: `v` (its version, which never goes stale) then `w` (a bucket of ImageUrls). */
    private fun picture(segments: List<Any>, width: Int?, version: String?): String = address(
        segments,
        listOf("v" to version, "w" to width?.let { ImageUrls.bucket(it) }?.toString()),
    ).toString()

    private fun jsonRequest(url: HttpUrl): Request.Builder = Request.Builder().url(url).header("Accept", "application/json")

    private suspend fun <T> get(
        deserializer: DeserializationStrategy<T>,
        vararg segments: Any,
        query: List<Pair<String, String?>> = emptyList(),
    ): T = http().fetch(jsonRequest(address(segments.toList(), query)).get().build()) { it.decode(deserializer) }

    private suspend fun <T> send(method: String, deserializer: DeserializationStrategy<T>, body: RequestBody, vararg segments: Any): T =
        http().fetch(jsonRequest(address(segments.toList())).method(method, body).build()) { it.decode(deserializer) }

    private suspend fun call(method: String, body: RequestBody?, vararg segments: Any) {
        val request = jsonRequest(address(segments.toList())).method(method, body ?: if (method == "DELETE") null else EMPTY).build()
        http().fetch(request) { it.expectSuccess() }
    }

    /** A 503 that says subtitle search isn't set up (`"setup": false`). */
    private fun notSetUp(response: okhttp3.Response): Boolean {
        val text = try {
            response.peekBody(MAX_ERROR_BODY).string()
        } catch (_: IOException) {
            return false
        }
        val fields = runCatching { PleyJson.parseToJsonElement(text) as? JsonObject }.getOrNull() ?: return false
        return (fields["setup"] as? JsonPrimitive)?.booleanOrNull == false
    }

    private fun <T> json(serializer: SerializationStrategy<T>, value: T): RequestBody =
        PleyJson.encodeToString(serializer, value).toRequestBody(JSON)

    companion object {
        private val JSON = "application/json".toMediaType()
        private val EMPTY = ByteArray(0).toRequestBody(null)
        private const val MAX_ERROR_BODY = 16L * 1024

        /**
         * The header of a subtitle track still being read out of its file:
         * `0` while more cues are coming (packages/transcoder-api,
         * SUBTITLES_COMPLETE_HEADER).
         */
        const val SUBTITLES_COMPLETE_HEADER = "X-Pley-Complete"
    }
}
