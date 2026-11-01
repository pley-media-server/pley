package app.pley.android.model

import kotlinx.serialization.Serializable

/*
 * Requests: movies and shows found on TMDB that people ask for,
 * administrators approve, and Radarr and Sonarr fetch
 * (apps/server/src/requests, docs/API.md "Requests"). Times are unix
 * seconds; pictures are the server's own paths
 * (`/api/discover/images/{kind}/{file}`), sized with `w` as library
 * artwork is.
 */

/** What TMDB has: a movie or a show. */
@Serializable(with = TitleKind.Serializer::class)
enum class TitleKind(override val wire: String) : WireEnum {
    Movie("movie"),
    Show("show"),
    Unknown("unknown"),
    ;

    object Serializer : LenientEnumSerializer<TitleKind>("TitleKind", TitleKind.entries, TitleKind.Unknown)

    companion object {
        /** A library item's kind as a title's: movies and shows; `null` for the rest. */
        fun of(kind: ItemKind): TitleKind? = when (kind) {
            ItemKind.Movie -> Movie
            ItemKind.Show -> Show
            else -> null
        }
    }
}

/** What asking does for this sign-in's account. */
@Serializable(with = RequestAccess.Serializer::class)
enum class RequestAccess(override val wire: String) : WireEnum {
    /** It may not ask: Discover doesn't show. */
    Off("off"),

    /** Its requests wait for an administrator. */
    Ask("ask"),

    /** Its requests are approved as they are made. */
    Auto("auto"),
    Unknown("unknown"),
    ;

    object Serializer : LenientEnumSerializer<RequestAccess>("RequestAccess", RequestAccess.entries, RequestAccess.Unknown)
}

/** A scope this sign-in lacks for requests: signing in again would ask for it. */
@Serializable(with = RequestScope.Serializer::class)
enum class RequestScope(override val wire: String) : WireEnum {
    /** Asking and following: without it, Discover shows only a way to sign in again. */
    Requests("requests"),

    /** Approving and declining, on an administrator's account. */
    Approvals("approvals"),
    Unknown("unknown"),
    ;

    object Serializer : LenientEnumSerializer<RequestScope>("RequestScope", RequestScope.entries, RequestScope.Unknown)
}

/**
 * GET /api/requests/summary: what this sign-in may do with requests, and
 * the numbers its badge shows. Any signed-in app gets it; while requests
 * are off every number is 0 and [manage] is false.
 */
@Serializable
data class RequestSummary(
    /** Requests are on, and TMDB has a key. */
    val enabled: Boolean = false,
    val access: RequestAccess = RequestAccess.Off,
    /** Movies it may still ask for in the last 7 days; `null`: no limit. */
    val moviesLeft: Int? = null,
    val seasonsLeft: Int? = null,
    /** The limits per 7 days; `null`: none. */
    val moviesLimit: Int? = null,
    val seasonsLimit: Int? = null,
    /** With none left: when one more may be asked for. */
    val moviesNextAt: Long? = null,
    val seasonsNextAt: Long? = null,
    /** This sign-in approves and declines: an administrator's, with `approvals` for an app. */
    val manage: Boolean = false,
    /** For managers: requests waiting for a decision. */
    val pending: Int = 0,
    /** For managers: failed requests the server won't try again by itself. */
    val failed: Int = 0,
    /** This account's requests (made or followed) with news since it last looked. */
    val unseen: Int = 0,
    /** This account's requests, made or followed, whatever their status. */
    val own: Int = 0,
    /** The newest change among the requests this sign-in sees: polled, it says when to read them again. */
    val changedAt: Long? = null,
    val signInAgain: RequestScope? = null,
) {
    /** Discover shows: requests are on and this account may ask. */
    val discover: Boolean get() = enabled && access != RequestAccess.Off

    /** Asking works from this sign-in: Discover shows, and the sign-in has `requests`. */
    val mayAsk: Boolean get() = discover && signInAgain != RequestScope.Requests

    /** "Your requests" shows. */
    val yourRequests: Boolean
        get() = enabled && signInAgain != RequestScope.Requests && (access != RequestAccess.Off || own > 0)

    /** The way into requests shows: Your requests, or for managers Requests. */
    val requestsEntry: Boolean get() = yourRequests || (enabled && manage)

    /** The number on the way into requests: for managers, what waits for them too. */
    val badge: Int get() = if (manage) pending + failed + unseen else unseen

    /** The badge is in the danger colour: requests failed and wait for a manager. */
    val badgeAlarm: Boolean get() = manage && failed > 0
}

/** What a title is for this account, the first of these that holds (docs/API.md "Requests"). */
@Serializable(with = TitleStatus.Serializer::class)
enum class TitleStatus(override val wire: String) : WireEnum {
    /** A library it can open has it whole. */
    Library("library"),

    /** Above its age limit: it can't ask. */
    Restricted("restricted"),

    /** Active requests cover everything it lacks. */
    Requested("requested"),

    /** Its last request for it was declined in the last 30 days. */
    Declined("declined"),

    /** Shows: some of it is in a library or asked for, and more seasons can be. */
    Partial("partial"),

    /** It may be asked for. */
    None("none"),
    Unknown("unknown"),
    ;

    object Serializer : LenientEnumSerializer<TitleStatus>("TitleStatus", TitleStatus.entries, TitleStatus.Unknown)
}

/** A title of Discover's lists and search, and the start of a title page. */
@Serializable
data class DiscoverTitle(
    val kind: TitleKind,
    val tmdbId: Long,
    val title: String,
    val year: Int? = null,
    val overview: String = "",
    val posterUrl: String? = null,
    val backdropUrl: String? = null,
    /** TMDB users', 0-10; `null` with too few votes. */
    val rating: Double? = null,
    val status: TitleStatus = TitleStatus.None,
    /** The library's title, where this account can open it. */
    val itemId: Long? = null,
    /** An active request for it this account made or follows (any, for managers): never a status. */
    val requestId: Long? = null,
)

/**
 * `{ page, total_pages, results }`: a page of a list or of search. A page
 * may hold fewer than 20 titles, even none, before the last one.
 */
@Serializable
data class DiscoverPage(
    val page: Int = 1,
    val totalPages: Int = 0,
    val results: List<DiscoverTitle> = emptyList(),
)

/** Discover's lists, by their path. */
enum class DiscoverList(override val wire: String) : WireEnum {
    Trending("trending"),
    Movies("movies"),
    Shows("shows"),
    Upcoming("upcoming"),
    Anime("anime"),
}

/** One of a title's cast, in billing order. */
@Serializable
data class CastMember(
    val name: String,
    val character: String? = null,
    val photoUrl: String? = null,
)

/** A season's state for this account, by the server's rule for a whole season. */
@Serializable(with = SeasonState.Serializer::class)
enum class SeasonState(override val wire: String) : WireEnum {
    Library("library"),
    Partial("partial"),
    Requested("requested"),
    Missing("missing"),
    Unaired("unaired"),
    Unknown("unknown"),
    ;

    object Serializer : LenientEnumSerializer<SeasonState>("SeasonState", SeasonState.entries, SeasonState.Unknown)
}

/** A season of a show on its title page; number 0 is the specials. */
@Serializable
data class DiscoverSeason(
    val number: Int,
    val name: String = "",
    /** Episodes TMDB lists. */
    val episodes: Int = 0,
    /** Of those, aired by today. */
    val aired: Int = 0,
    val airDate: String? = null,
    val posterUrl: String? = null,
    /** Episodes in a library this account can open. */
    val inLibrary: Int = 0,
    val requested: Boolean = false,
    val state: SeasonState = SeasonState.Unknown,
    /** A request for it this account made or followed was declined in the last 30 days. */
    val declined: Boolean = false,
    /** The active request asking for it, where this account made or follows it (any, for managers). */
    val requestId: Long? = null,
)

/** A decline that still holds for this account: it may ask again at [againAt]. */
@Serializable
data class DeclinedTitle(
    /** The administrator's reason; `null` without one, and for a follower. */
    val reason: String? = null,
    val at: Long = 0,
    val againAt: Long = 0,
)

/** A title page: GET /api/discover/movie/{tmdb_id} and /show/{tmdb_id}. */
@Serializable
data class DiscoverDetail(
    val kind: TitleKind,
    val tmdbId: Long,
    val title: String,
    val year: Int? = null,
    val overview: String = "",
    val posterUrl: String? = null,
    val backdropUrl: String? = null,
    val rating: Double? = null,
    val status: TitleStatus = TitleStatus.None,
    val itemId: Long? = null,
    val requestId: Long? = null,
    /** Only where it differs from [title]. */
    val originalTitle: String? = null,
    val tagline: String? = null,
    val genres: List<String> = emptyList(),
    /** A movie's release date, a show's first air date: "2008-01-20". */
    val releaseDate: String? = null,
    val runtimeMinutes: Int? = null,
    /** For the server's region: "PG-13", "TV-MA". */
    val certification: String? = null,
    /** A show's first network, a movie's first studio. */
    val network: String? = null,
    /** Shows: "Returning Series", "Ended". */
    val showStatus: String? = null,
    val anime: Boolean = false,
    val cast: List<CastMember> = emptyList(),
    /** Shows, in TMDB's order (specials first when there are some); `null` for movies. */
    val seasons: List<DiscoverSeason>? = null,
    val imdbId: String? = null,
    val declined: DeclinedTitle? = null,
    /** Why it can't be asked for: "Rated R: above this account's age limit". */
    val restriction: String? = null,
    /** This account follows an active request for it. */
    val watching: Boolean = false,
    /** This account's requests for it and those it follows (everyone's, for managers), newest first. */
    val requests: List<MediaRequest> = emptyList(),
    val moviesLeft: Int? = null,
    val seasonsLeft: Int? = null,
    val moviesNextAt: Long? = null,
    val seasonsNextAt: Long? = null,
) {
    /** The title as lists show it. */
    fun toTitle(): DiscoverTitle = DiscoverTitle(
        kind = kind,
        tmdbId = tmdbId,
        title = title,
        year = year,
        overview = overview,
        posterUrl = posterUrl,
        backdropUrl = backdropUrl,
        rating = rating,
        status = status,
        itemId = itemId,
        requestId = requestId,
    )
}

/** The decision on a request. */
@Serializable(with = RequestStatus.Serializer::class)
enum class RequestStatus(override val wire: String) : WireEnum {
    Pending("pending"),
    Approved("approved"),
    Declined("declined"),
    Failed("failed"),
    Available("available"),
    Unknown("unknown"),
    ;

    object Serializer : LenientEnumSerializer<RequestStatus>("RequestStatus", RequestStatus.entries, RequestStatus.Unknown)
}

/** Where a request is, from its decision to the library. */
@Serializable(with = RequestStage.Serializer::class)
enum class RequestStage(override val wire: String) : WireEnum {
    /** It waits for an administrator. */
    Pending("pending"),
    Declined("declined"),

    /** It couldn't be sent to Radarr or Sonarr (see [MediaRequest.retryAt]). */
    Failed("failed"),

    /** Approved, not sent: no Radarr or Sonarr for its kind, sending, or added by hand. */
    Approved("approved"),

    /** Sent, not out yet: [MediaRequest.stageAt] says when. */
    Waiting("waiting"),

    /** Sent and out, nothing downloading yet. */
    Searching("searching"),

    /** See [MediaRequest.download]. */
    Downloading("downloading"),

    /** Downloaded, going into the library. */
    Importing("importing"),

    /** Shows: some seasons asked for are ready ([MediaRequest.availableSeasons]). */
    Partial("partial"),

    /** Ready to watch. */
    Available("available"),

    /** It was available; no library has it any more. */
    Gone("gone"),
    Unknown("unknown"),
    ;

    object Serializer : LenientEnumSerializer<RequestStage>("RequestStage", RequestStage.entries, RequestStage.Unknown)
}

/** What this sign-in may do to a request now; apps list exactly these, in their order. */
@Serializable(with = RequestAction.Serializer::class)
enum class RequestAction(override val wire: String) : WireEnum {
    /** Its maker: DELETE /api/requests/{id} (Remove, on a declined one). */
    Withdraw("withdraw"),

    /** A follower: DELETE /api/requests/{id}/watch. */
    Unwatch("unwatch"),
    Approve("approve"),

    /** A failed one, approved again. */
    Retry("retry"),
    Decline("decline"),

    /** Approved without sending: added by hand. */
    Handled("handled"),

    /** Managers: removed for everyone. */
    Delete("delete"),

    /** Asked for again, as it was. */
    AskAgain("ask_again"),
    Unknown("unknown"),
    ;

    object Serializer : LenientEnumSerializer<RequestAction>("RequestAction", RequestAction.entries, RequestAction.Unknown)
}

/** Someone who asked for, or decided on, a request. */
@Serializable
data class RequestUser(
    val id: Long,
    val name: String,
    val avatarUrl: String? = null,
    /** "#38bdf8": the circle behind the initial when there's no picture. */
    val avatarColor: String? = null,
)

/** What Radarr's or Sonarr's queue says of a download. */
@Serializable(with = DownloadState.Serializer::class)
enum class DownloadState(override val wire: String) : WireEnum {
    Queued("queued"),
    Downloading("downloading"),
    Paused("paused"),
    Importing("importing"),
    Warning("warning"),
    Unknown("unknown"),
    ;

    object Serializer : LenientEnumSerializer<DownloadState>("DownloadState", DownloadState.entries, DownloadState.Unknown)
}

/** A request's download, while Radarr or Sonarr has it queued. */
@Serializable
data class RequestDownload(
    /** 0-1, over everything queued for it. */
    val progress: Double = 0.0,
    val state: DownloadState = DownloadState.Unknown,
    /** When it should be done. */
    val eta: Long? = null,
    /** Shows: episodes queued; movies: 1. */
    val episodes: Int = 0,
)

/** Where a request went. */
@Serializable(with = ArrService.Serializer::class)
enum class ArrService(override val wire: String) : WireEnum {
    Radarr("radarr"),
    Sonarr("sonarr"),
    Unknown("unknown"),
    ;

    object Serializer : LenientEnumSerializer<ArrService>("ArrService", ArrService.entries, ArrService.Unknown)
}

/** For managers: the service a request was sent to, with the quality profile's name and the folder. */
@Serializable
data class SentTo(
    val service: ArrService = ArrService.Unknown,
    val qualityProfile: String? = null,
    val rootFolder: String? = null,
)

/** A request for a movie, or for seasons of a show. */
@Serializable
data class MediaRequest(
    val id: Long,
    val kind: TitleKind,
    val tmdbId: Long,
    val title: String,
    val year: Int? = null,
    val posterUrl: String? = null,
    val backdropUrl: String? = null,
    /** Shows: the seasons asked for; `null` for movies. */
    val seasons: List<Int>? = null,
    /** Shows: those ready. */
    val availableSeasons: List<Int> = emptyList(),
    val anime: Boolean = false,
    val status: RequestStatus = RequestStatus.Unknown,
    val stage: RequestStage = RequestStage.Unknown,
    /** Waiting: when it comes out (a release known by its day is that day at 12:00 UTC). */
    val stageAt: Long? = null,
    /** Declined: the administrator's (`null` for followers); failed: what went wrong (managers only). */
    val reason: String? = null,
    /** Managers: what to know about it ("Added by hand"). */
    val note: String? = null,
    /** This account made it. */
    val mine: Boolean = false,
    /** This account follows it. */
    val watching: Boolean = false,
    /** Managers: how many follow it. */
    val watchers: Int = 0,
    /** It has news this account hasn't seen: available, partly available or declined. */
    val unseen: Boolean = false,
    val actions: List<RequestAction> = emptyList(),
    /** `null` for someone else's request seen by someone who doesn't manage. */
    val requestedBy: RequestUser? = null,
    val decidedBy: RequestUser? = null,
    val decidedAt: Long? = null,
    val createdAt: Long = 0,
    val updatedAt: Long = 0,
    val availableAt: Long? = null,
    /** Failed: when the server tries again by itself; `null`: it won't. */
    val retryAt: Long? = null,
    val sentTo: SentTo? = null,
    /** The library's title once there, where this account can open it. */
    val itemId: Long? = null,
    val download: RequestDownload? = null,
)

/**
 * GET /api/requests: a page of requests, how many match ([total]), how
 * many have each status with every filter but `status` ([counts], by
 * status), and for managers the people with requests matching every
 * filter but the person ([people], by name).
 */
@Serializable
data class RequestList(
    val total: Int = 0,
    val counts: Map<String, Int> = emptyMap(),
    val requests: List<MediaRequest> = emptyList(),
    val people: List<RequestUser> = emptyList(),
) {
    /** How many have [status]. */
    fun count(status: RequestStatus): Int = counts[status.wire] ?: 0
}

/** Which requests GET /api/requests lists, as its query. */
data class RequestQuery(
    /** One status or several. */
    val statuses: List<RequestStatus> = emptyList(),
    /** Managers: pending ones and the failed ones the server won't retry. */
    val attention: Boolean = false,
    val kind: TitleKind? = null,
    /** Managers: one person's. */
    val userId: Long? = null,
    /** Titles containing this. */
    val q: String? = null,
    /** Oldest first, instead of newest. */
    val oldest: Boolean = false,
    /** Only this account's and those it follows (managers see everyone's without it). */
    val mine: Boolean = false,
    /** Only those with news, newest news first. */
    val unseen: Boolean = false,
    val changedSince: Long? = null,
    val offset: Int = 0,
    val limit: Int? = null,
)

/** The body of POST /api/requests: a movie, or seasons of a show (`null`: those the server picks). */
@Serializable
data class NewRequest(
    val kind: TitleKind,
    val tmdbId: Long,
    val seasons: List<Int>? = null,
)

/** What POST /api/requests answered: a new request ([created], 201), or one this account now follows (200). */
data class Asked(
    val request: MediaRequest,
    val created: Boolean,
)

/** The body of POST /api/requests/{id}/approve: the choices of the dialog; `null`s take the settings'. */
@Serializable
data class Approval(
    /** A quality profile's id ([ArrOptions.qualityProfiles]). */
    val qualityProfile: Long? = null,
    val rootFolder: String? = null,
    /** Only these of the seasons asked for. */
    val seasons: List<Int>? = null,
)

/** The body of POST /api/requests/{id}/decline. */
@Serializable
data class Decline(
    val reason: String? = null,
)

/** A quality profile of Radarr or Sonarr. */
@Serializable
data class QualityProfile(
    val id: Long,
    val name: String,
)

/** A root folder of Radarr or Sonarr. */
@Serializable
data class RootFolder(
    val path: String,
    val freeBytes: Long? = null,
)

/**
 * What a manager may pick when approving, from one service: its quality
 * profiles and root folders, with the settings' choices (anime's for
 * anime). [error]: it didn't answer, and these are the last lists known.
 */
@Serializable
data class ArrOptions(
    val qualityProfiles: List<QualityProfile> = emptyList(),
    val rootFolders: List<RootFolder> = emptyList(),
    val qualityProfile: Long? = null,
    val rootFolder: String? = null,
    val animeQualityProfile: Long? = null,
    val animeRootFolder: String? = null,
    val error: String? = null,
)

/** GET /api/requests/options: each service's choices, `null` where it isn't connected. */
@Serializable
data class RequestOptions(
    val radarr: ArrOptions? = null,
    val sonarr: ArrOptions? = null,
) {
    /** The service a request of [kind] goes to. */
    fun of(kind: TitleKind): ArrOptions? = if (kind == TitleKind.Show) sonarr else radarr
}

/**
 * GET /api/items/{id}/request: a library title as Discover sees it, with
 * the newest active request for it this account made or follows (any, for
 * managers). [askable]: the seasons the picker would offer without a
 * limit (`[]` for movies; `null` while the server has no details for it).
 */
@Serializable
data class ItemRequest(
    val kind: TitleKind,
    val tmdbId: Long,
    val status: TitleStatus = TitleStatus.None,
    val request: MediaRequest? = null,
    val askable: List<Int>? = null,
)
