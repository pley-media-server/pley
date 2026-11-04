package app.pley.android.testing.network

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
import app.pley.android.model.RequestAction
import app.pley.android.model.RequestList
import app.pley.android.model.RequestOptions
import app.pley.android.model.RequestQuery
import app.pley.android.model.RequestStage
import app.pley.android.model.RequestStatus
import app.pley.android.model.RequestSummary
import app.pley.android.model.TitleKind
import app.pley.android.model.TitleStatus
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
import app.pley.android.model.DynamicRangeFilter
import app.pley.android.model.Library
import app.pley.android.model.LibraryFilters
import app.pley.android.model.LibraryIds
import app.pley.android.model.LibrarySort
import app.pley.android.model.ResolutionFilter
import app.pley.android.model.LiveCategory
import app.pley.android.model.LiveChannel
import app.pley.android.model.LiveChannelPage
import app.pley.android.model.LiveGroups
import app.pley.android.model.LiveGuide
import app.pley.android.model.LiveSummary
import app.pley.android.model.LivePlan
import app.pley.android.model.LiveWatching
import app.pley.android.model.ConversionInfo
import app.pley.android.model.ConversionState
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
import app.pley.android.model.PlaybackMode
import app.pley.android.model.PlaybackRequest
import app.pley.android.model.PlayerCommandRequest
import app.pley.android.model.PlayingSession
import app.pley.android.model.PlaylistDetail
import app.pley.android.model.PlaylistList
import app.pley.android.model.PlaylistPatch
import app.pley.android.model.ProgressReport
import app.pley.android.model.RatingRequest
import app.pley.android.model.SearchResults
import app.pley.android.model.ServerInfo
import app.pley.android.model.SessionStatus
import app.pley.android.model.SortOrder
import app.pley.android.model.StreamChoice
import app.pley.android.model.TrickplayInfo
import app.pley.android.model.WatchRequest
import app.pley.android.network.ApiException
import app.pley.android.network.HttpPleyApi
import app.pley.android.network.PleyApi
import app.pley.android.network.SubtitleText
import app.pley.android.testing.Fixtures
import kotlinx.coroutines.CompletableDeferred

/**
 * [PleyApi] without a network, answering from the JSON fixtures (the
 * household of [Fixtures]: Arrival 101, Severance 201 with season 202 and
 * episodes 203 to 205, the track 303, collections 801 and 803, playlists
 * 901 and 902). Every call is recorded in [calls]; [answer] and [fail]
 * change what a method gives, [hold] keeps calls waiting until released.
 * Addresses are built as [HttpPleyApi] builds them, on [issuer].
 */
class FakePleyApi(val issuer: String = "http://127.0.0.1:7539") : PleyApi {
    /** A call: the method's name (`"libraryItems"`) and its arguments in order. */
    data class Call(val method: String, val args: List<Any?>)

    private val recorded = mutableListOf<Call>()
    private val answers = mutableMapOf<String, Any?>()
    private val failures = mutableMapOf<String, Throwable>()
    private var gate: CompletableDeferred<Unit>? = null

    /** The calls so far, oldest first. */
    val calls: List<Call> get() = synchronized(recorded) { recorded.toList() }

    /** The calls of [method]. */
    fun calls(method: String): List<Call> = calls.filter { it.method == method }

    /** [method] answers [value] from now on (its type must be the method's). */
    fun answer(method: String, value: Any?) = synchronized(recorded) { answers[method] = value }

    /** [method] fails with [error] from now on; `null` for every method. */
    fun fail(method: String?, error: Throwable) = synchronized(recorded) { failures[method ?: ANY] = error }

    /** Answers from the fixtures again, and forgets the calls. */
    fun reset() = synchronized(recorded) {
        recorded.clear()
        answers.clear()
        failures.clear()
        gate?.complete(Unit)
        gate = null
    }

    /** Calls from now on wait until [release]: for what a screen shows while loading. */
    fun hold() = synchronized(recorded) { if (gate == null) gate = CompletableDeferred() }

    fun release() = synchronized(recorded) {
        gate?.complete(Unit)
        gate = null
    }

    private val urls = HttpPleyApi(issuer, { error("FakePleyApi has no network") })

    @Suppress("UNCHECKED_CAST")
    private suspend fun <T> respond(method: String, vararg args: Any?, fixture: () -> T): T {
        val (waitFor, failure, answer) = synchronized(recorded) {
            recorded += Call(method, args.toList())
            Triple(gate, failures[method] ?: failures[ANY], if (method in answers) Answer(answers[method]) else null)
        }
        waitFor?.await()
        if (failure != null) throw failure
        return if (answer != null) answer.value as T else fixture()
    }

    private class Answer(val value: Any?)

    private inline fun <reified T> fixture(path: String): T = Fixtures.decode(path)

    // ---------------------------------------------------------------- server

    override suspend fun health(): Health = respond("health") { fixture("server/health.json") }

    override suspend fun server(): ServerInfo = respond("server") { fixture("server/info.json") }

    override suspend fun me(): Me = respond("me") { fixture("me/app_token.json") }

    override fun avatarUrl(userId: Long, version: String?): String = urls.avatarUrl(userId, version)

    override fun url(path: String, width: Int?): String = urls.url(path, width)

    // ---------------------------------------------------------------- libraries

    override suspend fun libraries(): List<Library> = respond("libraries") { fixture("libraries/list.json") }

    override suspend fun library(id: Long): Library = respond("library", id) {
        fixture<List<Library>>("libraries/list.json").firstOrNull { it.id == id } ?: fixture<Library>("libraries/library.json").copy(id = id)
    }

    override suspend fun sortLibraries(order: LibraryIds): List<Library> = respond("sortLibraries", order) {
        val all = fixture<List<Library>>("libraries/list.json")
        order.ids.mapNotNull { id -> all.firstOrNull { it.id == id } } + all.filter { it.id !in order.ids }
    }

    override suspend fun forgetLibraryOrder(): List<Library> = respond("forgetLibraryOrder") { fixture("libraries/list.json") }

    override suspend fun libraryItems(
        libraryId: Long,
        sort: LibrarySort,
        order: SortOrder?,
        watched: Boolean?,
        genre: String?,
        year: Int?,
        resolution: ResolutionFilter?,
        dynamicRange: DynamicRangeFilter?,
        offset: Int,
        limit: Int?,
    ): ItemsPage = respond("libraryItems", libraryId, sort, order, watched, genre, year, resolution, dynamicRange, offset, limit) {
        val page = fixture<ItemsPage>(if (sort == LibrarySort.Bitrate) "libraries/items_by_bitrate.json" else "libraries/items.json")
        val items = page.items.drop(offset)
        page.copy(items = if (limit != null) items.take(limit) else items)
    }

    override suspend fun libraryFilters(libraryId: Long): LibraryFilters =
        respond("libraryFilters", libraryId) { fixture("libraries/filters.json") }

    override suspend fun libraryHubs(libraryId: Long): Hubs = respond("libraryHubs", libraryId) { fixture("libraries/hubs.json") }

    override suspend fun libraryCollections(libraryId: Long): CollectionList =
        respond("libraryCollections", libraryId) { fixture("libraries/collections.json") }

    override suspend fun collection(id: Long): CollectionDetail = respond("collection", id) {
        fixture(if (id == 803L) "collections/smart_detail.json" else "collections/detail.json")
    }

    override fun collectionImageUrl(id: Long, kind: ImageKind, width: Int?, version: String?): String =
        urls.collectionImageUrl(id, kind, width, version)

    // ---------------------------------------------------------------- playlists (reading)

    override suspend fun playlists(): PlaylistList = respond("playlists") { fixture("playlists/list.json") }

    override suspend fun playlist(id: Long): PlaylistDetail = respond("playlist", id) {
        fixture(if (id == 902L) "playlists/smart_detail.json" else "playlists/detail.json")
    }

    // ---------------------------------------------------------------- items

    override suspend fun item(id: Long): ItemDetail = respond("item", id) {
        when (id) {
            101L -> fixture("items/movie_detail.json")
            102L -> fixture("items/extra.json")
            201L -> fixture("items/show_detail.json")
            203L -> fixture("items/episode_detail.json")
            303L -> fixture("items/track_detail.json")
            else -> fixture<ItemDetail>("items/minimal_detail.json").copy(id = id)
        }
    }

    override suspend fun children(id: Long): ItemList = respond("children", id) {
        when (id) {
            201L -> fixture("items/children_seasons.json")
            202L -> fixture("items/children_episodes.json")
            else -> ItemList()
        }
    }

    override suspend fun onDeck(id: Long): MaybeItem = respond("onDeck", id) { fixture("items/on_deck.json") }

    override suspend fun next(id: Long): MaybeItem = respond("next", id) { fixture("items/next_none.json") }

    /** Every file plays as it is here unless a test answers otherwise. */
    override suspend fun convert(request: PlaybackRequest): ConversionInfo = respond("convert", request) {
        ConversionInfo(state = ConversionState.Direct, progress = 1.0, url = "/api/files/${request.fileId}/stream")
    }

    override suspend fun conversion(id: String): ConversionInfo = respond("conversion", id) {
        ConversionInfo(id = id, state = ConversionState.Done, progress = 1.0, url = "/api/conversions/$id/file")
    }

    override suspend fun cancelConversion(id: String) = respond("cancelConversion", id) { }

    override suspend fun related(id: Long, limit: Int): ItemList = respond("related", id, limit) {
        val list = fixture<ItemList>("items/related.json")
        list.copy(items = list.items.take(limit))
    }

    override fun itemImageUrl(id: Long, kind: ImageKind, width: Int?, version: String?): String =
        urls.itemImageUrl(id, kind, width, version)

    override suspend fun itemCollections(id: Long): ItemCollections =
        respond("itemCollections", id) { fixture("items/collections.json") }

    override suspend fun person(id: Long): PersonDetail = respond("person", id) { fixture("people/detail.json") }

    override fun personPhotoUrl(id: Long, width: Int?, version: String?): String = urls.personPhotoUrl(id, width, version)

    override suspend fun history(offset: Int, limit: Int, itemId: Long?, libraryId: Long?, since: Long?): HistoryPage =
        respond("history", offset, limit, itemId, libraryId, since) { fixture("history/page.json") }

    override suspend fun homeHubs(): Hubs = respond("homeHubs") { fixture("hubs/home.json") }

    override suspend fun watchlist(): ItemList = respond("watchlist") { fixture("watchlist/list.json") }

    override suspend fun search(query: String, limit: Int): SearchResults =
        respond("search", query, limit) { fixture("search/results.json") }

    // ---------------------------------------------------------------- playback

    override suspend fun startPlayback(request: PlaybackRequest): PlaybackInfo = respond("startPlayback", request) {
        val hls = request.mode == PlaybackMode.Remux || request.mode == PlaybackMode.Transcode
        fixture(if (hls) "playback/info_transcode.json" else "playback/info_direct.json")
    }

    override suspend fun playbackStatus(sessionId: String): SessionStatus = respond("playbackStatus", sessionId) {
        val transcode = fixture<SessionStatus>("playback/status_transcode.json")
        if (sessionId == transcode.sessionId) transcode else fixture<SessionStatus>("playback/status_direct.json").copy(sessionId = sessionId)
    }

    override suspend fun stopPlayback(sessionId: String) = respond("stopPlayback", sessionId) { }

    override suspend fun playingSessions(): List<PlayingSession> = respond("playingSessions") { fixture("sessions/list.json") }

    override suspend fun sendCommand(sessionId: String, command: PlayerCommandRequest) = respond("sendCommand", sessionId, command) { }

    override suspend fun stopSession(sessionId: String) = respond("stopSession", sessionId) { }

    /** The server's own network: nothing known, no speed test. */
    override suspend fun playbackNetwork(clientId: String): PlaybackNetwork = respond("playbackNetwork", clientId) { PlaybackNetwork(local = true) }

    override suspend fun reportNetwork(report: NetworkReport): PlaybackNetwork = respond("reportNetwork", report) { PlaybackNetwork() }

    override suspend fun speedTest(bytes: Long, readMs: Long, timeoutMs: Long): SpeedSample =
        respond("speedTest", bytes, readMs, timeoutMs) { SpeedSample(bytes, readMs / 2, complete = true) }

    override suspend fun searchSubtitles(itemId: Long, fileId: Long?, languages: List<String>?): SubtitleResults =
        respond("searchSubtitles", itemId, fileId, languages) { SubtitleResults() }

    override suspend fun addSubtitle(itemId: Long, add: AddSubtitle): SubtitleTrack = respond("addSubtitle", itemId, add) {
        SubtitleTrack(index = 3000, codec = "subrip", language = "eng", external = true, textBased = true, added = true)
    }

    override fun playbackFileUrl(sessionId: String, file: String): String = urls.playbackFileUrl(sessionId, file)

    override fun fileStreamUrl(fileId: Long): String = urls.fileStreamUrl(fileId)

    override suspend fun subtitles(fileId: Long, index: Int): SubtitleText =
        respond("subtitles", fileId, index) { SubtitleText(SUBTITLES, complete = true) }

    override suspend fun trickplay(fileId: Long): TrickplayInfo? = respond("trickplay", fileId) { fixture<TrickplayInfo>("trickplay/info.json") }

    override fun trickplaySheetUrl(fileId: Long, sheet: Int, version: String): String = urls.trickplaySheetUrl(fileId, sheet, version)

    override fun chapterImageUrl(fileId: Long, chapter: Int, version: String?): String = urls.chapterImageUrl(fileId, chapter, version)

    override suspend fun reportProgress(itemId: Long, report: ProgressReport) = respond("reportProgress", itemId, report) { }

    override suspend fun markWatched(itemId: Long) = respond("markWatched", itemId) { }

    override suspend fun markUnwatched(itemId: Long) = respond("markUnwatched", itemId) { }

    override suspend fun setStreams(itemId: Long, choice: StreamChoice) = respond("setStreams", itemId, choice) { }

    override suspend fun setRating(itemId: Long, rating: RatingRequest) = respond("setRating", itemId, rating) { }

    override suspend fun deleteHistoryEntry(id: Long) = respond("deleteHistoryEntry", id) { }

    override suspend fun hideFromContinueWatching(itemId: Long) = respond("hideFromContinueWatching", itemId) { }

    override suspend fun addToWatchlist(itemId: Long) = respond("addToWatchlist", itemId) { }

    override suspend fun removeFromWatchlist(itemId: Long) = respond("removeFromWatchlist", itemId) { }

    // ---------------------------------------------------------------- live TV

    override suspend fun liveSummary(): LiveSummary = respond("liveSummary") { fixture("live/summary.json") }

    override suspend fun liveChannels(): List<LiveChannel> = respond("liveChannels") { fixture("live/channels.json") }

    override suspend fun liveGroups(): LiveGroups = respond("liveGroups") { fixture("live/groups.json") }

    override suspend fun liveBrowse(category: LiveCategory, query: String?, offset: Int, limit: Int, around: Long?): LiveChannelPage =
        respond("liveBrowse", category, query, offset, limit, around) {
            val recent = fixture<LiveSummary>("live/summary.json").recent
            val all = fixture<List<LiveChannel>>("live/channels.json").filter { c ->
                val inCategory = when (category) {
                    LiveCategory.All -> true
                    LiveCategory.Favorites -> c.favorite
                    LiveCategory.Recent -> c.id in recent
                    is LiveCategory.Group -> c.group == category.name
                }
                inCategory && (query.isNullOrBlank() || c.name.contains(query, ignoreCase = true) || c.number?.startsWith(query) == true)
            }
            val start = around?.let { id -> all.indexOfFirst { it.id == id }.coerceAtLeast(0) / limit * limit } ?: offset
            LiveChannelPage(all.size, start, all.drop(start).take(limit))
        }

    override suspend fun liveChannel(id: Long): LiveChannel = respond("liveChannel", id) {
        fixture<List<LiveChannel>>("live/channels.json").firstOrNull { it.id == id } ?: throw ApiException(404, "not_found", "no such channel")
    }

    override suspend fun liveGuide(from: Long, to: Long, ids: List<Long>?): LiveGuide = respond("liveGuide", from, to, ids) {
        val guide = fixture<LiveGuide>("live/guide.json")
        guide.copy(from = from, to = to, programs = if (ids == null) guide.programs else guide.programs.filterKeys { it in ids })
    }

    override suspend fun setFavoriteChannel(channelId: Long, favorite: Boolean) = respond("setFavoriteChannel", channelId, favorite) { }

    override suspend fun watchChannel(channelId: Long, request: WatchRequest): LiveWatching = respond("watchChannel", channelId, request) {
        val watching = fixture<LiveWatching>("live/watching.json")
        watching.copy(sessionId = "live-$channelId", url = "/api/live/sessions/live-$channelId/index.m3u8")
    }

    override suspend fun livePlan(sessionId: String): LivePlan? = respond("livePlan", sessionId) { fixture<LivePlan>("live/plan.json") }

    override fun liveStreamUrl(channelId: Long, clientId: String): String = urls.liveStreamUrl(channelId, clientId)

    override suspend fun leaveChannel(sessionId: String) = respond("leaveChannel", sessionId) { }

    // ---------------------------------------------------------------- playlists (changing)

    override suspend fun createPlaylist(playlist: NewPlaylist): PlaylistDetail = respond("createPlaylist", playlist) {
        val detail = fixture<PlaylistDetail>(if (playlist.smartFilter != null) "playlists/smart_detail.json" else "playlists/detail.json")
        detail.copy(title = playlist.title)
    }

    override suspend fun updatePlaylist(id: Long, patch: PlaylistPatch): PlaylistDetail = respond("updatePlaylist", id, patch) {
        val detail = fixture<PlaylistDetail>(if (id == 902L) "playlists/smart_detail.json" else "playlists/detail.json")
        detail.copy(title = patch.title ?: detail.title, smartFilter = patch.smartFilter ?: detail.smartFilter)
    }

    override suspend fun deletePlaylist(id: Long) = respond("deletePlaylist", id) { }

    override suspend fun addToPlaylist(id: Long, items: ItemIds): Added = respond("addToPlaylist", id, items) { fixture("playlists/added.json") }

    override suspend fun removeFromPlaylist(id: Long, entryId: Long) = respond("removeFromPlaylist", id, entryId) { }

    override suspend fun reorderPlaylist(id: Long, entries: EntryIds): PlaylistDetail = respond("reorderPlaylist", id, entries) {
        val detail = fixture<PlaylistDetail>("playlists/detail.json")
        val first = entries.entryIds.mapNotNull { e -> detail.entries.firstOrNull { it.entryId == e } }
        detail.copy(entries = first + detail.entries.filter { it !in first })
    }

    // ---------------------------------------------------------------- requests

    override suspend fun requestSummary(): RequestSummary = respond("requestSummary") { fixture("requests/summary.json") }

    override suspend fun discover(list: DiscoverList, page: Int): DiscoverPage = respond("discover", list, page) {
        fixture<DiscoverPage>("discover/trending.json").copy(page = page)
    }

    override suspend fun discoverSearch(query: String, page: Int, missing: Boolean): DiscoverPage =
        respond("discoverSearch", query, page, missing) {
            val all = fixture<DiscoverPage>("discover/trending.json")
            all.copy(page = page, results = all.results.filter { !missing || it.status != TitleStatus.Library })
        }

    override suspend fun discoverTitle(kind: TitleKind, tmdbId: Long): DiscoverDetail = respond("discoverTitle", kind, tmdbId) {
        fixture(if (kind == TitleKind.Show) "discover/show.json" else "discover/movie.json")
    }

    override suspend fun itemRequest(itemId: Long): ItemRequest = respond("itemRequest", itemId) { fixture("requests/item_request.json") }

    override suspend fun requests(query: RequestQuery): RequestList = respond("requests", query) {
        val list = fixture<RequestList>(if (query.attention) "requests/attention.json" else "requests/list.json")
        list.copy(
            requests = list.requests.filter { r ->
                (query.kind == null || r.kind == query.kind) && (query.q.isNullOrBlank() || r.title.contains(query.q!!, ignoreCase = true)) &&
                    (query.statuses.isEmpty() || r.status in query.statuses) && (!query.unseen || r.unseen)
            },
        )
    }

    override suspend fun request(id: Long): MediaRequest = respond("request", id) { knownRequest(id) }

    override suspend fun ask(request: NewRequest): Asked = respond("ask", request) {
        Asked(
            MediaRequest(
                id = 60,
                kind = request.kind,
                tmdbId = request.tmdbId,
                title = "Asked for",
                seasons = request.seasons,
                status = RequestStatus.Pending,
                stage = RequestStage.Pending,
                mine = true,
                actions = listOf(RequestAction.Withdraw),
            ),
            created = true,
        )
    }

    override suspend fun deleteRequest(id: Long) = respond("deleteRequest", id) { }

    override suspend fun unwatchRequest(id: Long) = respond("unwatchRequest", id) { }

    override suspend fun requestsSeen() = respond("requestsSeen") { }

    override suspend fun approveRequest(id: Long, approval: Approval): MediaRequest = respond("approveRequest", id, approval) {
        knownRequest(id).copy(
            status = RequestStatus.Approved,
            stage = RequestStage.Approved,
            seasons = approval.seasons ?: knownRequest(id).seasons,
            actions = listOf(RequestAction.Decline, RequestAction.Delete),
        )
    }

    override suspend fun declineRequest(id: Long, decline: Decline): MediaRequest = respond("declineRequest", id, decline) {
        knownRequest(id).copy(
            status = RequestStatus.Declined,
            stage = RequestStage.Declined,
            reason = decline.reason,
            actions = listOf(RequestAction.Approve, RequestAction.Handled, RequestAction.Delete),
        )
    }

    override suspend fun requestHandled(id: Long): MediaRequest = respond("requestHandled", id) {
        knownRequest(id).copy(
            status = RequestStatus.Approved,
            stage = RequestStage.Approved,
            note = "Added by hand",
            actions = listOf(RequestAction.Decline, RequestAction.Delete),
        )
    }

    override suspend fun requestOptions(): RequestOptions = respond("requestOptions") { fixture("requests/options.json") }

    /** Request [id] of the fixtures' lists, else the first one as [id]. */
    private fun knownRequest(id: Long): MediaRequest {
        val all = fixture<RequestList>("requests/list.json").requests + fixture<RequestList>("requests/attention.json").requests
        return all.firstOrNull { it.id == id } ?: all.first().copy(id = id)
    }

    companion object {
        private const val ANY = "*"

        /** What [subtitles] gives: a short WebVTT track. */
        const val SUBTITLES = "WEBVTT\n\n00:00:01.000 --> 00:00:03.000\nThe universe is not made of atoms.\n"
    }
}
