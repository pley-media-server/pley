package app.pley.android.network

import app.pley.android.model.AddSubtitle
import app.pley.android.model.Approval
import app.pley.android.model.ConversionState
import app.pley.android.model.Decline
import app.pley.android.model.DiscoverList
import app.pley.android.model.MediaRequest
import app.pley.android.model.NewRequest
import app.pley.android.model.RequestList
import app.pley.android.model.RequestQuery
import app.pley.android.model.RequestStatus
import app.pley.android.model.TitleKind
import app.pley.android.model.NetworkReport
import app.pley.android.model.PlaybackNetwork
import app.pley.android.model.SubtitleResult
import app.pley.android.model.EntryIds
import app.pley.android.model.ImageKind
import app.pley.android.model.ItemIds
import app.pley.android.model.DynamicRangeFilter
import app.pley.android.model.LibraryIds
import app.pley.android.model.LibrarySort
import app.pley.android.model.LiveCategory
import app.pley.android.model.ResolutionFilter
import app.pley.android.model.NewPlaylist
import app.pley.android.model.PlaybackRequest
import app.pley.android.model.PlayerCommandRequest
import app.pley.android.model.Control
import app.pley.android.model.PlaylistPatch
import app.pley.android.model.PleyJson
import app.pley.android.model.ProgressReport
import app.pley.android.model.RatingRequest
import app.pley.android.model.SortOrder
import app.pley.android.model.StreamChoice
import app.pley.android.model.WatchRequest
import app.pley.android.testing.Fixtures
import app.pley.android.testing.network.MockPley
import app.pley.android.testing.network.bodyText
import java.util.concurrent.TimeUnit
import kotlinx.coroutines.test.runTest
import kotlinx.serialization.json.Json
import mockwebserver3.MockResponse
import mockwebserver3.MockWebServer
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder

/**
 * A golden request per PleyApi method: the request line (path and query,
 * in their fixed order), the body, and the headers every request carries.
 */
class HttpPleyApiTest {
    private companion object {
        const val CONVERSION = "0123456789abcdef0123456789abcdef"

        /** docs/API.md's examples. */
        const val NETWORK = """{"local":false,"ceiling_kbps":8000,"source":"playback","since":1761600000000,"probe":false}"""
        const val SEARCH = """{"results":[{"id":"opensubtitles:1234567","provider":"opensubtitles","language":"en","name":"Movie.2019.1080p.BluRay.x264-GRP","format":"srt","hearing_impaired":false,"forced":false,"downloads":1234,"matched":"hash"}]}"""
        const val ADDED = """{"index":5000,"codec":"subrip","language":"eng","title":"English","external":true,"text_based":true,"url":"/api/files/601/subtitles/5000.vtt","added":true}"""
    }

    @get:Rule val folder = TemporaryFolder()

    private lateinit var server: MockWebServer
    private lateinit var issuer: String
    private lateinit var api: PleyApi

    @Before
    fun setUp() {
        server = MockPley.server()
        issuer = MockPley.issuerOf(server)
        val tokens = testTokens(issuer)
        api = testGraph(folder.root, { tokens }).api(issuer, userId = 2)
    }

    @After
    fun tearDown() {
        server.close()
    }

    /** One method: what the server answers, what the app sends. */
    private class Golden(
        val name: String,
        val answer: MockResponse,
        val line: String,
        val body: String? = null,
        val call: suspend PleyApi.() -> Any?,
    )

    private fun json(path: String) = MockPley.fixture(path)

    private fun jsonAnswer(text: String, code: Int = 200): MockResponse =
        MockResponse.Builder().code(code).setHeader("Content-Type", "application/json").body(text).build()

    private inline fun <reified T> request(path: String): T = Fixtures.decode(path)

    private fun goldens(): List<Golden> {
        val playback = request<PlaybackRequest>("playback/request.json")
        val progress = request<ProgressReport>("playback/progress_request.json")
        val streams = request<StreamChoice>("playback/streams_request.json")
        val rating = request<RatingRequest>("playback/rating_request.json")
        val create = request<NewPlaylist>("playlists/create_request.json")
        val patch = request<PlaylistPatch>("playlists/patch_request.json")
        val add = request<ItemIds>("playlists/add_items_request.json")
        val order = request<EntryIds>("playlists/order_request.json")
        return listOf(
            Golden("health", json("server/health.json"), "GET /api/health") { health() },
            Golden("server", json("server/info.json"), "GET /api/server") { server() },
            Golden("me", json("me/app_token.json"), "GET /api/auth/me") { me() },
            Golden("libraries", json("libraries/list.json"), "GET /api/libraries") { libraries() },
            Golden("library", json("libraries/library.json"), "GET /api/libraries/2") { library(2) },
            Golden(
                "sortLibraries",
                json("libraries/list.json"),
                "PUT /api/me/libraries/order",
                body = """{"ids":[3,1,2]}""",
            ) { sortLibraries(LibraryIds(listOf(3, 1, 2))) },
            Golden("forgetLibraryOrder", json("libraries/list.json"), "DELETE /api/me/libraries/order") { forgetLibraryOrder() },
            Golden("libraryItems (defaults)", json("libraries/items.json"), "GET /api/libraries/1/items?sort=title&offset=0") {
                libraryItems(1)
            },
            Golden(
                "libraryItems (every parameter)",
                json("libraries/items.json"),
                "GET /api/libraries/1/items?sort=added&order=desc&watched=false&genre=Science%20Fiction&year=2016&offset=200&limit=1000",
            ) {
                libraryItems(1, LibrarySort.Added, SortOrder.Descending, watched = false, genre = "Science Fiction", year = 2016, offset = 200, limit = 1000)
            },
            Golden(
                "libraryItems (resolution and dynamic range)",
                json("libraries/items.json"),
                "GET /api/libraries/1/items?sort=bitrate&resolution=4k&dynamic_range=dolby_vision&offset=0",
            ) {
                libraryItems(1, LibrarySort.Bitrate, resolution = ResolutionFilter.Uhd, dynamicRange = DynamicRangeFilter.DolbyVision)
            },
            Golden("libraryFilters", json("libraries/filters.json"), "GET /api/libraries/1/filters") { libraryFilters(1) },
            Golden("libraryHubs", json("libraries/hubs.json"), "GET /api/libraries/1/hubs") { libraryHubs(1) },
            Golden("libraryCollections", json("libraries/collections.json"), "GET /api/libraries/1/collections") { libraryCollections(1) },
            Golden("collection", json("collections/detail.json"), "GET /api/collections/801") { collection(801) },
            Golden("playlists", json("playlists/list.json"), "GET /api/playlists") { playlists() },
            Golden("playlist", json("playlists/detail.json"), "GET /api/playlists/901") { playlist(901) },
            Golden("item", json("items/movie_detail.json"), "GET /api/items/101") { item(101) },
            Golden("children", json("items/children_seasons.json"), "GET /api/items/201/children") { children(201) },
            Golden("onDeck", json("items/on_deck.json"), "GET /api/items/201/on-deck") { onDeck(201) },
            Golden("next", json("items/next_none.json"), "GET /api/items/203/next") { next(203) },
            Golden("related", json("items/related.json"), "GET /api/items/101/related?limit=20") { related(101) },
            Golden("itemCollections", json("items/collections.json"), "GET /api/items/101/collections") { itemCollections(101) },
            Golden("person", json("people/detail.json"), "GET /api/people/706") { person(706) },
            Golden("history (defaults)", json("history/page.json"), "GET /api/history?offset=0&limit=50") { history() },
            Golden(
                "history (every parameter)",
                json("history/page.json"),
                "GET /api/history?offset=50&limit=200&item_id=201&library_id=2&since=1761000000",
            ) { history(offset = 50, limit = 200, itemId = 201, libraryId = 2, since = 1761000000) },
            Golden("homeHubs", json("hubs/home.json"), "GET /api/hubs/home") { homeHubs() },
            Golden("watchlist", json("watchlist/list.json"), "GET /api/watchlist") { watchlist() },
            Golden("search", json("search/results.json"), "GET /api/search?q=blade%20runner%20%26%202049&limit=200") {
                search("blade runner & 2049")
            },
            Golden(
                "startPlayback",
                json("playback/info_transcode.json"),
                "POST /api/playback",
                body = PleyJson.encodeToString(PlaybackRequest.serializer(), playback),
            ) { startPlayback(playback) },
            Golden(
                "startPlayback (beside the session playing)",
                json("playback/info_transcode.json"),
                "POST /api/playback",
                body = PleyJson.encodeToString(PlaybackRequest.serializer(), playback.copy(keepPrevious = true)),
            ) { startPlayback(playback.copy(keepPrevious = true)) },
            Golden("playbackStatus", json("playback/status_transcode.json"), "GET /api/playback/6f1c2a9e") { playbackStatus("6f1c2a9e") },
            Golden("playingSessions", json("sessions/list.json"), "GET /api/sessions") { playingSessions() },
            Golden(
                "sendCommand",
                MockPley.noContent(),
                "POST /api/sessions/6f1c2a9e/command",
                body = """{"command":"seek","position_ms":600000}""",
            ) { sendCommand("6f1c2a9e", request("sessions/command_request.json")) },
            Golden(
                "sendCommand (no value)",
                MockPley.noContent(),
                "POST /api/sessions/6f1c2a9e/command",
                body = """{"command":"pause"}""",
            ) { sendCommand("6f1c2a9e", PlayerCommandRequest(Control.Pause)) },
            Golden("stopSession", MockPley.noContent(), "DELETE /api/sessions/6f1c2a9e") { stopSession("6f1c2a9e") },
            Golden("remoteDevices", json("remote/devices.json"), "GET /api/remote/devices") { remoteDevices() },
            Golden(
                "sendCommand (a track)",
                MockPley.noContent(),
                "POST /api/sessions/6f1c2a9e/command",
                body = """{"command":"subtitles","stream_index":-1}""",
            ) { sendCommand("6f1c2a9e", PlayerCommandRequest(Control.Subtitles, streamIndex = -1)) },
            Golden("stopPlayback", MockPley.noContent(), "DELETE /api/playback/6f1c2a9e") { stopPlayback("6f1c2a9e") },
            Golden("playbackNetwork", jsonAnswer(NETWORK), "GET /api/playback/network?client_id=android-1") { playbackNetwork("android-1") },
            Golden(
                "reportNetwork (a speed test)",
                jsonAnswer(NETWORK),
                "PUT /api/playback/network",
                body = """{"client_id":"android-1","probe_kbps":52000,"probe_complete":false}""",
            ) { reportNetwork(NetworkReport.Probe("android-1", 52_000, complete = false)) },
            Golden(
                "reportNetwork (a ceiling)",
                jsonAnswer(NETWORK),
                "PUT /api/playback/network",
                body = """{"client_id":"android-1","ceiling_kbps":8000}""",
            ) { reportNetwork(NetworkReport.Ceiling("android-1", 8_000)) },
            Golden(
                "reportNetwork (forgotten)",
                jsonAnswer(NETWORK),
                "PUT /api/playback/network",
                body = """{"client_id":"android-1","ceiling_kbps":null}""",
            ) { reportNetwork(NetworkReport.Ceiling("android-1", null)) },
            Golden(
                "speedTest",
                MockResponse.Builder().setHeader("Content-Type", "application/octet-stream").body(okio.Buffer().write(ByteArray(70_000))).build(),
                "GET /api/playback/speedtest?bytes=70000",
            ) { speedTest(70_000, 2_000, 3_500) },
            Golden(
                "searchSubtitles",
                jsonAnswer(SEARCH),
                "GET /api/items/101/subtitles/search?file_id=601&language=en%2Cfr",
            ) { searchSubtitles(101, 601, listOf("en", "fr")) },
            Golden("searchSubtitles (defaults)", jsonAnswer(SEARCH), "GET /api/items/101/subtitles/search") { searchSubtitles(101, null, null) },
            Golden(
                "addSubtitle",
                jsonAnswer(ADDED, 201),
                "POST /api/items/101/subtitles",
                body = """{"result":"opensubtitles:1234567","file_id":601}""",
            ) { addSubtitle(101, AddSubtitle("opensubtitles:1234567", 601)) },
            Golden(
                "convert",
                json("conversions/direct.json"),
                "POST /api/conversions",
                body = PleyJson.encodeToString(PlaybackRequest.serializer(), playback),
            ) { convert(playback) },
            Golden("conversion", json("conversions/running.json"), "GET /api/conversions/$CONVERSION") { conversion(CONVERSION) },
            Golden("cancelConversion", MockPley.noContent(), "DELETE /api/conversions/$CONVERSION") { cancelConversion(CONVERSION) },
            Golden(
                "subtitles",
                MockResponse.Builder().setHeader("Content-Type", "text/vtt").body("WEBVTT\n").build(),
                "GET /api/files/601/subtitles/3.vtt",
            ) { subtitles(601, 3) },
            Golden("trickplay", json("trickplay/info.json"), "GET /api/files/601/trickplay") { trickplay(601) },
            Golden(
                "reportProgress",
                MockPley.noContent(),
                "POST /api/items/101/progress",
                body = PleyJson.encodeToString(ProgressReport.serializer(), progress),
            ) { reportProgress(101, progress) },
            Golden("markWatched", MockPley.noContent(), "POST /api/items/101/watched", body = "") { markWatched(101) },
            Golden("markUnwatched", MockPley.noContent(), "POST /api/items/101/unwatched", body = "") { markUnwatched(101) },
            Golden(
                "setStreams",
                MockPley.noContent(),
                "PUT /api/items/203/streams",
                body = PleyJson.encodeToString(StreamChoice.serializer(), streams),
            ) { setStreams(203, streams) },
            Golden(
                "setRating",
                MockPley.noContent(),
                "PUT /api/items/101/rating",
                body = PleyJson.encodeToString(RatingRequest.serializer(), rating),
            ) { setRating(101, rating) },
            Golden("deleteHistoryEntry", MockPley.noContent(), "DELETE /api/history/5") { deleteHistoryEntry(5) },
            Golden("hideFromContinueWatching", MockPley.noContent(), "DELETE /api/continue-watching/204") { hideFromContinueWatching(204) },
            Golden("addToWatchlist", MockPley.noContent(), "PUT /api/watchlist/201", body = "") { addToWatchlist(201) },
            Golden("removeFromWatchlist", MockPley.noContent(), "DELETE /api/watchlist/201") { removeFromWatchlist(201) },
            Golden("liveSummary", json("live/summary.json"), "GET /api/live") { liveSummary() },
            Golden("liveChannels", json("live/channels.json"), "GET /api/live/channels") { liveChannels() },
            Golden("liveGuide", json("live/guide.json"), "GET /api/live/guide?from=1761742800&to=1761764400") {
                liveGuide(1761742800, 1761764400)
            },
            Golden("liveGuide (the channels in view)", json("live/guide.json"), "GET /api/live/guide?from=1761742800&to=1761764400&ids=1%2C2") {
                liveGuide(1761742800, 1761764400, listOf(1, 2))
            },
            Golden("liveGroups", json("live/groups.json"), "GET /api/live/groups") { liveGroups() },
            Golden("liveBrowse (defaults)", json("live/browse.json"), "GET /api/live/browse?category=all&offset=0&limit=100") { liveBrowse(LiveCategory.All) },
            Golden(
                "liveBrowse (a group, searched)",
                json("live/browse.json"),
                "GET /api/live/browse?category=group&group=UK%7C%20Sport&q=sky&offset=200&limit=100",
            ) { liveBrowse(LiveCategory.Group("UK| Sport"), " sky ", offset = 200) },
            Golden("liveBrowse (around a channel)", json("live/browse.json"), "GET /api/live/browse?category=recent&limit=100&around=102") {
                liveBrowse(LiveCategory.Recent, around = 102)
            },
            Golden("liveChannel", jsonAnswer("""{"id":1,"name":"Pley News HD","number":"101"}"""), "GET /api/live/channels/1") { liveChannel(1) },
            Golden("setFavoriteChannel", MockPley.noContent(), "PUT /api/live/channels/2/favorite", body = "") { setFavoriteChannel(2, true) },
            Golden("setFavoriteChannel (off)", MockPley.noContent(), "DELETE /api/live/channels/2/favorite") { setFavoriteChannel(2, false) },
            Golden(
                "watchChannel",
                json("live/watching.json"),
                "POST /api/live/channels/1/watch",
                body = """{"client_id":"android-1"}""",
            ) { watchChannel(1, WatchRequest("android-1")) },
            Golden("livePlan", json("live/plan.json"), "GET /api/live/sessions/5d0f7f2a/plan") { livePlan("5d0f7f2a") },
            Golden("livePlan (not known in time)", jsonAnswer("null"), "GET /api/live/sessions/5d0f7f2a/plan") { livePlan("5d0f7f2a") },
            Golden("leaveChannel", MockPley.noContent(), "DELETE /api/live/sessions/5d0f7f2a") { leaveChannel("5d0f7f2a") },
            Golden(
                "leaveChannel (gone already)",
                MockPley.fixture("errors/not_found.json", code = 404),
                "DELETE /api/live/sessions/5d0f7f2a",
            ) { leaveChannel("5d0f7f2a") },
            Golden(
                "createPlaylist",
                json("playlists/detail.json"),
                "POST /api/playlists",
                body = PleyJson.encodeToString(NewPlaylist.serializer(), create),
            ) { createPlaylist(create) },
            Golden(
                "updatePlaylist",
                json("playlists/detail.json"),
                "PATCH /api/playlists/901",
                body = PleyJson.encodeToString(PlaylistPatch.serializer(), patch),
            ) { updatePlaylist(901, patch) },
            Golden("deletePlaylist", MockPley.noContent(), "DELETE /api/playlists/901") { deletePlaylist(901) },
            Golden(
                "addToPlaylist",
                json("playlists/added.json"),
                "POST /api/playlists/901/items",
                body = PleyJson.encodeToString(ItemIds.serializer(), add),
            ) { addToPlaylist(901, add) },
            Golden("removeFromPlaylist", MockPley.noContent(), "DELETE /api/playlists/901/items/7") { removeFromPlaylist(901, 7) },
            Golden(
                "reorderPlaylist",
                json("playlists/detail.json"),
                "PUT /api/playlists/901/order",
                body = PleyJson.encodeToString(EntryIds.serializer(), order),
            ) { reorderPlaylist(901, order) },
            Golden("requestSummary", json("requests/summary.json"), "GET /api/requests/summary") { requestSummary() },
            Golden("discover", json("discover/trending.json"), "GET /api/discover/anime?page=2") { discover(DiscoverList.Anime, 2) },
            Golden("discoverSearch", json("discover/trending.json"), "GET /api/discover/search?q=the%20wild&page=1") { discoverSearch("the wild") },
            Golden("discoverSearch (missing)", json("discover/trending.json"), "GET /api/discover/search?q=dune&page=3&missing=1") {
                discoverSearch("dune", page = 3, missing = true)
            },
            Golden("discoverTitle", json("discover/show.json"), "GET /api/discover/show/1396") { discoverTitle(TitleKind.Show, 1396) },
            Golden("itemRequest", json("requests/item_request.json"), "GET /api/items/201/request") { itemRequest(201) },
            Golden("requests (defaults)", json("requests/list.json"), "GET /api/requests?offset=0") { requests() },
            Golden(
                "requests (every parameter)",
                json("requests/attention.json"),
                "GET /api/requests?status=pending%2Cfailed&attention=1&kind=show&user_id=3&q=arc&order=oldest&mine=1&unseen=1&changed_since=1761600000&offset=20&limit=20",
            ) {
                requests(
                    RequestQuery(
                        statuses = listOf(RequestStatus.Pending, RequestStatus.Failed),
                        attention = true,
                        kind = TitleKind.Show,
                        userId = 3,
                        q = "arc",
                        oldest = true,
                        mine = true,
                        unseen = true,
                        changedSince = 1761600000,
                        offset = 20,
                        limit = 20,
                    ),
                )
            },
            Golden("request", jsonAnswer(oneRequest), "GET /api/requests/44") { request(44) },
            Golden(
                "ask",
                jsonAnswer(oneRequest, code = 201),
                "POST /api/requests",
                body = """{"kind":"show","tmdb_id":1396,"seasons":[3,5]}""",
            ) { ask(NewRequest(TitleKind.Show, 1396, listOf(3, 5))) },
            Golden("ask (the server's seasons)", jsonAnswer(oneRequest, code = 201), "POST /api/requests", body = """{"kind":"movie","tmdb_id":1184918}""") {
                ask(NewRequest(TitleKind.Movie, 1184918))
            },
            Golden("deleteRequest", MockPley.noContent(), "DELETE /api/requests/44") { deleteRequest(44) },
            Golden("unwatchRequest", MockPley.noContent(), "DELETE /api/requests/47/watch") { unwatchRequest(47) },
            Golden("requestsSeen", MockPley.noContent(), "POST /api/requests/seen", body = "") { requestsSeen() },
            Golden(
                "approveRequest",
                jsonAnswer(oneRequest),
                "POST /api/requests/49/approve",
                body = """{"quality_profile":6,"root_folder":"/anime","seasons":[1,2]}""",
            ) { approveRequest(49, Approval(qualityProfile = 6, rootFolder = "/anime", seasons = listOf(1, 2))) },
            Golden("approveRequest (the settings' choices)", jsonAnswer(oneRequest), "POST /api/requests/44/approve", body = "{}") { approveRequest(44) },
            Golden("declineRequest", jsonAnswer(oneRequest), "POST /api/requests/44/decline", body = """{"reason":"Not this one"}""") {
                declineRequest(44, Decline("Not this one"))
            },
            Golden("requestHandled", jsonAnswer(oneRequest), "POST /api/requests/44/handled") { requestHandled(44) },
            Golden("requestOptions", json("requests/options.json"), "GET /api/requests/options") { requestOptions() },
        )
    }

    /** One request, as the routes that change one answer it. */
    private val oneRequest: String
        get() = PleyJson.encodeToString(MediaRequest.serializer(), Fixtures.decode<RequestList>("requests/list.json").requests.first())

    @Test
    fun askingSaysWhetherTheRequestIsNew() = runTest {
        server.enqueue(jsonAnswer(oneRequest, code = 201))
        assertTrue(api.ask(NewRequest(TitleKind.Movie, 1022789)).created)
        // Asked for already by someone else: followed, nothing new.
        server.enqueue(jsonAnswer(oneRequest, code = 200))
        val followed = api.ask(NewRequest(TitleKind.Movie, 1022789))
        assertFalse(followed.created)
        assertEquals(48L, followed.request.id)
    }

    @Test
    fun refusalsOfRequestsSayWhatTheyMean() = runTest {
        server.enqueue(MockPley.fixture("errors/declined.json", code = 409))
        val declined = runCatching { api.ask(NewRequest(TitleKind.Movie, 945961)) }.exceptionOrNull() as ApiException
        assertEquals("declined", declined.code)
        assertEquals(1763992000L, declined.retryAt)

        server.enqueue(MockPley.fixture("errors/conflict_request.json", code = 409))
        val raced = runCatching { api.approveRequest(44) }.exceptionOrNull() as ApiException
        assertEquals(RequestStatus.Approved, raced.details?.request?.status)

        // pley's own 503 is an answer, never the server being away.
        server.enqueue(MockPley.fixture("errors/tmdb_down.json", code = 503))
        val down = runCatching { api.discover(DiscoverList.Trending) }.exceptionOrNull()
        assertTrue("$down", down is ApiException)
        assertEquals("tmdb_down", (down as ApiException).code)
        assertEquals(true, down.details?.setup)
    }

    @Test
    fun everyMethodSendsItsGoldenRequest() = runTest {
        for (golden in goldens()) {
            server.enqueue(golden.answer)
            golden.call(api)
            val request = server.takeRequest(5, TimeUnit.SECONDS) ?: error("${golden.name}: no request")
            assertEquals(golden.name, "${golden.line} HTTP/1.1", request.requestLine)
            assertEquals("${golden.name}: user agent", TEST_AGENT, request.headers["User-Agent"])
            assertEquals("${golden.name}: token", "Bearer pley_at_one", request.headers["Authorization"])
            if (golden.body != null) assertEquals("${golden.name}: body", golden.body, request.bodyText)
            if (golden.body != null && golden.body.isNotEmpty()) {
                assertEquals("${golden.name}: content type", "application/json; charset=utf-8", request.headers["Content-Type"])
            }
        }
    }

    @Test
    fun requestBodiesAreTheFixturesTheyCameFrom() {
        // Decoded from docs/API.md's shapes and written back: nothing lost, nothing renamed.
        for ((path, serializer) in listOf(
            "playback/progress_request.json" to ProgressReport.serializer(),
            "playback/streams_request.json" to StreamChoice.serializer(),
            "playback/rating_request.json" to RatingRequest.serializer(),
            "sessions/command_request.json" to PlayerCommandRequest.serializer(),
            "playlists/add_items_request.json" to ItemIds.serializer(),
            "playlists/order_request.json" to EntryIds.serializer(),
        )) {
            @Suppress("UNCHECKED_CAST")
            val s = serializer as kotlinx.serialization.KSerializer<Any>
            val written = PleyJson.encodeToString(s, PleyJson.decodeFromString(s, Fixtures.text(path)))
            assertEquals(path, Json.parseToJsonElement(Fixtures.text(path)), Json.parseToJsonElement(written))
        }
    }

    @Test
    fun conversionsAreDecoded() = runTest {
        server.enqueue(MockPley.fixture("conversions/direct.json"))
        val direct = api.convert(PlaybackRequest(itemId = 101, fileId = 601))
        assertEquals(Triple(ConversionState.Direct, null, "/api/files/601/stream"), Triple(direct.state, direct.id, direct.url))
        server.enqueue(MockPley.fixture("conversions/running.json"))
        val running = api.conversion(CONVERSION)
        assertEquals(Triple(ConversionState.Running, 0.42, null), Triple(running.state, running.progress, running.size))
    }

    @Test
    fun answersAreDecoded() = runTest {
        server.enqueue(MockPley.fixture("items/movie_detail.json"))
        val movie = api.item(101)
        assertEquals("Arrival", movie.title)

        server.enqueue(MockPley.fixture("libraries/list.json"))
        assertEquals(listOf(1L, 2L, 3L), api.libraries().map { it.id })

        server.enqueue(MockPley.fixture("server/info.json"))
        assertFalse(api.server().twoStepRequired)
        server.enqueue(jsonAnswer("""{"name":"Living Room","version":"0.42.0","two_step_required":true}"""))
        assertTrue(api.server().twoStepRequired)
    }

    @Test
    fun subtitlesSayWhetherMoreAreComing() = runTest {
        server.enqueue(
            MockResponse.Builder().setHeader("Content-Type", "text/vtt").setHeader("X-Pley-Complete", "0").body("WEBVTT\n\npart").build(),
        )
        val partial = api.subtitles(601, 3)
        assertEquals(SubtitleText("WEBVTT\n\npart", complete = false), partial)

        server.enqueue(
            MockResponse.Builder().setHeader("Content-Type", "text/vtt").setHeader("X-Pley-Complete", "1").body("WEBVTT\n\nall").build(),
        )
        assertEquals(true, api.subtitles(601, 3).complete)

        // A sidecar or a cached track: no header, and complete.
        server.enqueue(MockResponse.Builder().setHeader("Content-Type", "text/vtt").body("WEBVTT\n").build())
        assertEquals(true, api.subtitles(601, 3).complete)
    }

    @Test
    fun theNetworkAndSubtitleSearchAreDecoded() = runTest {
        server.enqueue(jsonAnswer(NETWORK))
        assertEquals(PlaybackNetwork(ceilingKbps = 8_000, source = "playback", since = 1_761_600_000_000, probe = false), api.playbackNetwork("a"))
        server.enqueue(jsonAnswer(SEARCH))
        val found = api.searchSubtitles(101, null, null)
        assertTrue(found.setup)
        assertEquals(SubtitleResult("opensubtitles:1234567", "opensubtitles", "en", "Movie.2019.1080p.BluRay.x264-GRP", "srt", downloads = 1234, matched = "hash"), found.results.single())
        server.enqueue(jsonAnswer(ADDED, 201))
        val added = api.addSubtitle(101, AddSubtitle("opensubtitles:1234567"))
        assertTrue(added.added && added.external && added.textBased)
        assertEquals(5000, added.index)
        // Search not set up: said so, not thrown.
        server.enqueue(jsonAnswer("""{"code":"unavailable","message":"Subtitle search isn't set up","setup":false}""", 503))
        assertFalse(api.searchSubtitles(101, null, null).setup)
        // Any other 503 is a refusal.
        server.enqueue(MockPley.error(503, "unavailable", "busy"))
        assertEquals(503, runCatching { api.searchSubtitles(101, null, null) }.exceptionOrNull().let { (it as ApiException).status })
    }

    @Test
    fun seekPreviewsNotMadeYetAreNull() = runTest {
        server.enqueue(MockPley.error(404, "not_found", "seek previews are not ready yet"))
        assertNull(api.trickplay(601))
    }

    @Test
    fun addressesAreAbsoluteAndCarryNoToken() {
        assertEquals("$issuer/api/users/2/avatar?v=abc", api.avatarUrl(2, "abc"))
        assertEquals("$issuer/api/users/2/avatar", api.avatarUrl(2))
        assertEquals("$issuer/api/items/101/images/poster?v=17&w=480", api.itemImageUrl(101, ImageKind.Poster, width = 400, version = "17"))
        assertEquals("$issuer/api/items/203/images/thumb?w=1920", api.itemImageUrl(203, ImageKind.Thumb, width = 5000))
        assertEquals("$issuer/api/collections/801/images/backdrop?v=9&w=1280", api.collectionImageUrl(801, ImageKind.Backdrop, 1280, "9"))
        assertEquals("$issuer/api/people/706/photo?w=160", api.personPhotoUrl(706, width = 100))
        assertEquals("$issuer/api/playback/abc/index.m3u8", api.playbackFileUrl("abc"))
        assertEquals("$issuer/api/playback/abc/7.ts", api.playbackFileUrl("abc", "7.ts"))
        assertEquals("$issuer/api/files/601/stream", api.fileStreamUrl(601))
        assertEquals("$issuer/api/files/601/trickplay/2.jpg?v=t1", api.trickplaySheetUrl(601, 2, "t1"))
        assertEquals("$issuer/api/files/601/chapters/0.jpg", api.chapterImageUrl(601, 0))
        // The server's own addresses, made absolute and sized.
        assertEquals("$issuer/api/items/12/images/poster?v=1729600000", api.url("/api/items/12/images/poster?v=1729600000"))
        assertEquals("$issuer/api/items/12/images/poster?v=1729600000&w=320", api.url("/api/items/12/images/poster?v=1729600000", width = 300))
        assertEquals("https://elsewhere.example/x.jpg", api.url("https://elsewhere.example/x.jpg"))
    }

    @Test(expected = IllegalArgumentException::class)
    fun collectionsHaveNoThumb() {
        api.collectionImageUrl(801, ImageKind.Thumb)
    }
}
