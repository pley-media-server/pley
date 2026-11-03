package app.pley.android.network

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
import app.pley.android.model.ProgressReport
import app.pley.android.model.RatingRequest
import app.pley.android.model.SearchResults
import app.pley.android.model.ServerInfo
import app.pley.android.model.SessionStatus
import app.pley.android.model.SortOrder
import app.pley.android.model.StreamChoice
import app.pley.android.model.TrickplayInfo
import app.pley.android.model.WatchRequest

/**
 * A subtitle track as WebVTT (GET /api/files/{id}/subtitles/{index}.vtt).
 * An embedded track still being read out of its file comes with the cues
 * found so far: [complete] is false (the server's `X-Pley-Complete: 0`,
 * packages/transcoder-api `SUBTITLES_COMPLETE_HEADER`), and asking again
 * later gets more.
 */
data class SubtitleText(
    val text: String,
    val complete: Boolean,
)

/**
 * pley's HTTP API as an app's token may use it: one function per route of
 * APP_ROUTES (apps/server/src/auth/scopes.rs), with GET /api/server and
 * GET /api/health, which answer without signing in. docs/API.md has the
 * rules of each.
 *
 * JSON routes are suspending functions answering the model's types; a
 * refusal throws [ApiException], a network failure [OfflineException], an
 * answer that isn't pley's [UnreadableAnswerException] (all IOExceptions).
 * Cancelling the coroutine cancels the request.
 * Routes answering bytes for Coil or Media3 (pictures, streams, HLS files,
 * seek-preview sheets) are address builders instead: absolute addresses
 * on the server in use, without the token, which goes in the
 * `Authorization` header of every request, players' included (docs/API.md:
 * no address pley hands out carries a token).
 *
 * Not here: GET /api/events, the server's events, which ServerEvents
 * streams; GET /api/oauth/profiles, which the sign-in module (:core:auth)
 * asks, as it switches profiles with the token endpoint.
 */
interface PleyApi {
    // ---------------------------------------------------------------- server

    /** GET /api/health: `{ "status": "ok" }` while the server runs. No token needed. */
    suspend fun health(): Health

    /** GET /api/server: the server's name and version. No token needed. */
    suspend fun server(): ServerInfo

    /** GET /api/auth/me: the profile the app acts as, and what it was allowed. Any app token. */
    suspend fun me(): Me

    /**
     * GET /api/users/{id}/avatar: a profile's picture (JPEG, 256 px). Apps
     * see their own profile's and their household's; others answer 404.
     * [version] is the `v` of the profile's `avatar_url`, which changes
     * with the picture.
     */
    fun avatarUrl(userId: Long, version: String? = null): String

    /**
     * Makes one of the server's own addresses absolute: picture addresses
     * in answers (`poster_url`, `photo_url`, `thumb_url`, `artwork`…) and
     * [PlaybackInfo.url]. With [width], a picture is asked for that many
     * pixels wide (`w`, bucketed by the server).
     */
    fun url(path: String, width: Int? = null): String

    // ---------------------------------------------------------------- libraries (scope library)

    /** GET /api/libraries: the libraries the profile may open, in their order. */
    suspend fun libraries(): List<Library>

    /** GET /api/libraries/{id}. */
    suspend fun library(id: Long): Library

    /**
     * PUT /api/me/libraries/order (scope play): the profile's own order of
     * its libraries, [order] naming every one of them once; the libraries
     * in it.
     */
    suspend fun sortLibraries(order: LibraryIds): List<Library>

    /** DELETE /api/me/libraries/order (scope play): the order everyone sees, again; the libraries in it. */
    suspend fun forgetLibraryOrder(): List<Library>

    /**
     * GET /api/libraries/{id}/items: a page of the library's top-level
     * items. [order] `null` is the server's default for [sort]; [watched]
     * true matches shows whose every episode is watched; [resolution] and
     * [dynamicRange] match a title's best file (a show's best episode).
     * Without [limit], every item from [offset].
     */
    suspend fun libraryItems(
        libraryId: Long,
        sort: LibrarySort = LibrarySort.Title,
        order: SortOrder? = null,
        watched: Boolean? = null,
        genre: String? = null,
        year: Int? = null,
        resolution: ResolutionFilter? = null,
        dynamicRange: DynamicRangeFilter? = null,
        offset: Int = 0,
        limit: Int? = null,
    ): ItemsPage

    /** GET /api/libraries/{id}/filters: the genres, years, resolutions and dynamic ranges to filter [libraryItems] by. */
    suspend fun libraryFilters(libraryId: Long): LibraryFilters

    /** GET /api/libraries/{id}/hubs: the library's rows (continue watching, recently added and released, top rated). */
    suspend fun libraryHubs(libraryId: Long): Hubs

    /** GET /api/libraries/{id}/collections: the library's collections, by title. */
    suspend fun libraryCollections(libraryId: Long): CollectionList

    /** GET /api/collections/{id}: a collection and its items, in its order. */
    suspend fun collection(id: Long): CollectionDetail

    /**
     * GET /api/collections/{id}/images/{kind}: a collection's poster or
     * backdrop ([ImageKind.Thumb] isn't one), or the item artwork it falls
     * back to.
     */
    fun collectionImageUrl(id: Long, kind: ImageKind, width: Int? = null, version: String? = null): String

    // ---------------------------------------------------------------- playlists (scope library)

    /** GET /api/playlists: the profile's playlists, by title. */
    suspend fun playlists(): PlaylistList

    /** GET /api/playlists/{id}: a playlist and the entries the profile can see. */
    suspend fun playlist(id: Long): PlaylistDetail

    // ---------------------------------------------------------------- items (scope library)

    /** GET /api/items/{id}: everything about an item, files, markers, extras and credits included. */
    suspend fun item(id: Long): ItemDetail

    /** GET /api/items/{id}/children: seasons, episodes, albums or tracks, by index. */
    suspend fun children(id: Long): ItemList

    /** GET /api/items/{id}/on-deck: the episode Play starts for a show or season. */
    suspend fun onDeck(id: Long): MaybeItem

    /** GET /api/items/{id}/next: the episode or track after this one, for autoplay. */
    suspend fun next(id: Long): MaybeItem

    /** GET /api/items/{id}/related: "More Like This" for movies and shows; [limit] at most 60. */
    suspend fun related(id: Long, limit: Int = 20): ItemList

    /**
     * GET /api/items/{id}/images/{kind}: an item's poster, backdrop or
     * still, falling back to its parent's (a season's to its show's).
     * [version] is the `v` of the item's picture address.
     */
    fun itemImageUrl(id: Long, kind: ImageKind, width: Int? = null, version: String? = null): String

    /** GET /api/items/{id}/collections: the collections a movie or show is in ("Part of …"). */
    suspend fun itemCollections(id: Long): ItemCollections

    /** GET /api/people/{id}: a person and what they are credited in. */
    suspend fun person(id: Long): PersonDetail

    /** GET /api/people/{id}/photo: a person's photo; 404 without one. */
    fun personPhotoUrl(id: Long, width: Int? = null, version: String? = null): String

    /**
     * GET /api/history: the profile's plays, newest first. [itemId] is the
     * item or the episodes and tracks under it; [since] is unix seconds;
     * [limit] at most 200.
     */
    suspend fun history(
        offset: Int = 0,
        limit: Int = 50,
        itemId: Long? = null,
        libraryId: Long? = null,
        since: Long? = null,
    ): HistoryPage

    /** GET /api/hubs/home: Continue Watching, My List, then Recently Added of each library. */
    suspend fun homeHubs(): Hubs

    /** GET /api/watchlist: the profile's My List, movies and shows, newest added first. */
    suspend fun watchlist(): ItemList

    /**
     * GET /api/search: titles by relevance, people and Live TV channels,
     * up to [limit] of each: 200 by default, the most the server gives,
     * so every result shows (the server's own default is 40).
     */
    suspend fun search(query: String, limit: Int = 200): SearchResults

    // ---------------------------------------------------------------- playback (scope play)

    /** POST /api/playback: starts a session; the answer says how and from where it plays. */
    suspend fun startPlayback(request: PlaybackRequest): PlaybackInfo

    /**
     * POST /api/conversions: what a download of [request]'s item takes on
     * this device, decided as a playback would be: the original, or a file
     * the server makes for it.
     */
    suspend fun convert(request: PlaybackRequest): ConversionInfo

    /** GET /api/conversions/{id}: how the server's file for a download goes. */
    suspend fun conversion(id: String): ConversionInfo

    /** DELETE /api/conversions/{id}: the download went before its file was made; the server stops making it. */
    suspend fun cancelConversion(id: String)

    /** GET /api/playback/{session}: how the session plays now, with its transcode's live numbers. */
    suspend fun playbackStatus(sessionId: String): SessionStatus

    /** DELETE /api/playback/{session}: ends the session and any transcode. */
    suspend fun stopPlayback(sessionId: String)

    /** GET /api/playback/network: what the server remembers of this client's network, for Automatic's start. */
    suspend fun playbackNetwork(clientId: String): PlaybackNetwork

    /** PUT /api/playback/network: a speed test's result, or Automatic's new ceiling; the answer is GET's. */
    suspend fun reportNetwork(report: NetworkReport): PlaybackNetwork

    /**
     * GET /api/playback/speedtest?bytes=[bytes], read as it comes: timed from
     * the first byte, and stopped [readMs] after it (the request aborted) or
     * when all arrived. [timeoutMs] bounds the whole request, the wait for
     * the first byte included; a test cut short after its first byte still
     * says what it measured.
     */
    suspend fun speedTest(bytes: Long, readMs: Long, timeoutMs: Long): SpeedSample

    /**
     * GET /api/items/{id}/subtitles/search: subtitles found online for the
     * item's file [fileId] (`null`: its first) in [languages] (`null`: the
     * profile's subtitle language). A server without search set up answers
     * `setup` false.
     */
    suspend fun searchSubtitles(itemId: Long, fileId: Long?, languages: List<String>?): SubtitleResults

    /** POST /api/items/{id}/subtitles: adds a subtitle found by [searchSubtitles]; the new track. */
    suspend fun addSubtitle(itemId: Long, add: AddSubtitle): SubtitleTrack

    /**
     * GET /api/playback/{session}/{file}: a file of the session's HLS
     * stream, "index.m3u8" or a segment "{n}.ts". [PlaybackInfo.url]
     * already names the playlist.
     */
    fun playbackFileUrl(sessionId: String, file: String = "index.m3u8"): String

    /** GET /api/files/{id}/stream: the file as it is, with Range, for direct play. */
    fun fileStreamUrl(fileId: Long): String

    /** GET /api/files/{id}/subtitles/{index}.vtt: a text subtitle track as WebVTT, maybe partial. */
    suspend fun subtitles(fileId: Long, index: Int): SubtitleText

    /**
     * GET /api/files/{id}/trickplay: the file's seek previews; `null`
     * while they are being made (the server answers 404 and queues them).
     */
    suspend fun trickplay(fileId: Long): TrickplayInfo?

    /** GET /api/files/{id}/trickplay/{n}.jpg?v={TrickplayInfo.id}: sheet [sheet] of the seek previews. */
    fun trickplaySheetUrl(fileId: Long, sheet: Int, version: String): String

    /** GET /api/files/{id}/chapters/{n}.jpg: the picture of chapter [chapter] (0-based), once the file has previews. */
    fun chapterImageUrl(fileId: Long, chapter: Int, version: String? = null): String

    /** POST /api/items/{id}/progress: where the player is, every ~10 s, on pause and on closing. */
    suspend fun reportProgress(itemId: Long, report: ProgressReport)

    /** POST /api/items/{id}/watched: watched by hand (containers: all of them). Not a play. */
    suspend fun markWatched(itemId: Long)

    /** POST /api/items/{id}/unwatched. */
    suspend fun markUnwatched(itemId: Long)

    /** PUT /api/items/{id}/streams: remembers the tracks picked, for the whole show or the movie. */
    suspend fun setStreams(itemId: Long, choice: StreamChoice)

    /** PUT /api/items/{id}/rating: the profile's own rating, 0 to 10; `null` clears it. */
    suspend fun setRating(itemId: Long, rating: RatingRequest)

    /** DELETE /api/history/{id}: forgets one of the profile's plays. */
    suspend fun deleteHistoryEntry(id: Long)

    /**
     * DELETE /api/continue-watching/{id}: leaves Continue Watching (an
     * episode: its whole show) until it, or the show, is played again.
     * Progress and watched marks stay.
     */
    suspend fun hideFromContinueWatching(itemId: Long)

    /** PUT /api/watchlist/{id}: adds a movie or show to My List (again is fine). */
    suspend fun addToWatchlist(itemId: Long)

    /** DELETE /api/watchlist/{id}: takes it off My List. */
    suspend fun removeFromWatchlist(itemId: Long)

    // ---------------------------------------------------------------- live TV (docs/API.md, "Live TV")

    /**
     * GET /api/live (scope library): whether Live TV is open to this
     * profile and has channels, and the channels watched last.
     */
    suspend fun liveSummary(): LiveSummary

    /**
     * GET /api/live/channels (scope library): every channel shown, by
     * number, each with what's on now and next. Fine for a few hundred; a
     * provider's tens of thousands are browsed by group ([liveBrowse]).
     */
    suspend fun liveChannels(): List<LiveChannel>

    /** GET /api/live/groups (scope library): the groups in the lineup's order, and how many channels in all, marked and watched lately. */
    suspend fun liveGroups(): LiveGroups

    /**
     * GET /api/live/browse (scope library): a page of [category]'s
     * channels matching [query] (a name, a number's beginning, what's on),
     * [limit] (500 at most) from [offset]; with [around], the page holding
     * that channel instead.
     */
    suspend fun liveBrowse(category: LiveCategory, query: String? = null, offset: Int = 0, limit: Int = 100, around: Long? = null): LiveChannelPage

    /** GET /api/live/channels/{id} (scope library): one channel, with what's on now and next. */
    suspend fun liveChannel(id: Long): LiveChannel

    /**
     * GET /api/live/guide (scope library): the programmes between [from]
     * and [to] (unix seconds, two days at most) of the channels [ids]
     * (every one when `null`), by channel.
     */
    suspend fun liveGuide(from: Long, to: Long, ids: List<Long>? = null): LiveGuide

    /** PUT (or, not [favorite], DELETE) /api/live/channels/{id}/favorite (scope play). */
    suspend fun setFavoriteChannel(channelId: Long, favorite: Boolean)

    /**
     * POST /api/live/channels/{id}/watch (scope play): tunes the channel
     * (unless someone watches it already) and answers its live HLS
     * playlist. A `409` says which channels hold every connection of its
     * source; [WatchRequest.clientId] ends what this device watched
     * before, first.
     */
    suspend fun watchChannel(channelId: Long, request: WatchRequest): LiveWatching

    /**
     * GET /api/live/sessions/{session}/plan (scope play): what the
     * session's channel is made into, waiting a few seconds for it; `null`
     * when it isn't known by then.
     */
    suspend fun livePlan(sessionId: String): LivePlan?

    /** DELETE /api/live/sessions/{session} (scope play): stops watching now; a session already gone is fine. */
    suspend fun leaveChannel(sessionId: String)

    // ---------------------------------------------------------------- playlists (scope playlists)

    /** POST /api/playlists: a new playlist, of items or smart. */
    suspend fun createPlaylist(playlist: NewPlaylist): PlaylistDetail

    /** PATCH /api/playlists/{id}: a new title, or a smart playlist's new filter. */
    suspend fun updatePlaylist(id: Long, patch: PlaylistPatch): PlaylistDetail

    /** DELETE /api/playlists/{id}. */
    suspend fun deletePlaylist(id: Long)

    /** POST /api/playlists/{id}/items: adds items at the end; shows, seasons, albums and artists add what they hold. */
    suspend fun addToPlaylist(id: Long, items: ItemIds): Added

    /** DELETE /api/playlists/{id}/items/{entry_id}: removes one entry. */
    suspend fun removeFromPlaylist(id: Long, entryId: Long)

    /** PUT /api/playlists/{id}/order: these entries first, in this order, the rest after. */
    suspend fun reorderPlaylist(id: Long, entries: EntryIds): PlaylistDetail

    // ---------------------------------------------------------------- requests (any app)

    /**
     * GET /api/requests/summary: what this sign-in may do with requests,
     * and its badge's numbers. Any app may ask; an older server answers 404.
     */
    suspend fun requestSummary(): RequestSummary

    // ---------------------------------------------------------------- requests (scope requests)

    /** GET /api/discover/{list}?page=: a page of one of Discover's lists. */
    suspend fun discover(list: DiscoverList, page: Int = 1): DiscoverPage

    /** GET /api/discover/search?q=&page=&missing=: TMDB's titles matching [query]; [missing] leaves out those in a library. */
    suspend fun discoverSearch(query: String, page: Int = 1, missing: Boolean = false): DiscoverPage

    /** GET /api/discover/movie/{tmdb_id} or /show/{tmdb_id}: a title page. */
    suspend fun discoverTitle(kind: TitleKind, tmdbId: Long): DiscoverDetail

    /** GET /api/items/{id}/request: a library title as Discover sees it; 404 without a TMDB match. */
    suspend fun itemRequest(itemId: Long): ItemRequest

    /** GET /api/requests: requests matching [query], with the counts and people of its filters. */
    suspend fun requests(query: RequestQuery = RequestQuery()): RequestList

    /** GET /api/requests/{id}. */
    suspend fun request(id: Long): MediaRequest

    /**
     * POST /api/requests: asks for a movie or seasons of a show, or follows
     * the requests that ask for them already (then [Asked.created] is false).
     */
    suspend fun ask(request: NewRequest): Asked

    /** DELETE /api/requests/{id}: withdraws this account's request (managers: deletes anyone's). */
    suspend fun deleteRequest(id: Long)

    /** DELETE /api/requests/{id}/watch: stops following someone else's request. */
    suspend fun unwatchRequest(id: Long)

    /** POST /api/requests/seen: this account has looked at its requests' news. */
    suspend fun requestsSeen()

    // ---------------------------------------------------------------- requests (scope approvals)

    /** POST /api/requests/{id}/approve: approves (or, failed, tries again) with [approval]'s choices. */
    suspend fun approveRequest(id: Long, approval: Approval = Approval()): MediaRequest

    /** POST /api/requests/{id}/decline. */
    suspend fun declineRequest(id: Long, decline: Decline = Decline()): MediaRequest

    /** POST /api/requests/{id}/handled: approved without sending, as added by hand. */
    suspend fun requestHandled(id: Long): MediaRequest

    /** GET /api/requests/options: Radarr's and Sonarr's quality profiles and folders, for the approve dialog. */
    suspend fun requestOptions(): RequestOptions
}
