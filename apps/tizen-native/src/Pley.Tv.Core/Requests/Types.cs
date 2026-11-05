using System;
using System.Collections.Generic;
using System.Linq;

namespace Pley.Tv.Core.Requests;

// Requests: movies and shows found on TMDB that people ask for, administrators approve, and
// Radarr and Sonarr fetch (docs/API.md, "Requests"; the Android app's model/Requests.kt). Times
// are unix seconds; pictures are the server's own paths (/api/discover/images/{kind}/{file}),
// sized with `w` as library artwork is. Kinds, statuses and stages stay strings, as the server
// writes them, so one a newer server adds reads as one this app doesn't know.

/// <summary>What TMDB has: a movie or a show.</summary>
public static class TitleKind
{
    public const string Movie = "movie";
    public const string Show = "show";

    /// <summary>A library item's kind as a title's: movies and shows; null for the rest.</summary>
    public static string? Of(string itemKind) => itemKind switch
    {
        "movie" => Movie,
        "show" => Show,
        _ => null,
    };
}

/// <summary>What asking does for this sign-in's account.</summary>
public static class RequestAccess
{
    /// <summary>It may not ask: Discover doesn't show.</summary>
    public const string Off = "off";
    /// <summary>Its requests wait for an administrator.</summary>
    public const string Ask = "ask";
    /// <summary>Its requests are approved as they are made.</summary>
    public const string Auto = "auto";
}

/// <summary>A scope a sign-in lacks for requests: signing in again would ask for it.</summary>
public static class RequestScope
{
    /// <summary>Asking and following: without it, Discover shows only a way to sign in again.</summary>
    public const string Requests = "requests";
    /// <summary>Approving and declining, on an administrator's account.</summary>
    public const string Approvals = "approvals";
}

/// <summary>
/// GET /api/requests/summary: what this sign-in may do with requests, and the numbers its badge
/// shows. Any signed-in app gets it; while requests are off every number is 0 and
/// <see cref="Manage"/> is false.
/// </summary>
public sealed record RequestSummary
{
    /// <summary>Requests are on, and TMDB has a key.</summary>
    public bool Enabled { get; init; }
    public string Access { get; init; } = RequestAccess.Off;
    /// <summary>Movies it may still ask for in the last 7 days; null: no limit.</summary>
    public int? MoviesLeft { get; init; }
    public int? SeasonsLeft { get; init; }
    /// <summary>The limits per 7 days; null: none.</summary>
    public int? MoviesLimit { get; init; }
    public int? SeasonsLimit { get; init; }
    /// <summary>With none left: when one more may be asked for.</summary>
    public long? MoviesNextAt { get; init; }
    public long? SeasonsNextAt { get; init; }
    /// <summary>This sign-in approves and declines: an administrator's, with <c>approvals</c> for an app.</summary>
    public bool Manage { get; init; }
    /// <summary>For managers: requests waiting for a decision.</summary>
    public int Pending { get; init; }
    /// <summary>For managers: failed requests the server won't try again by itself.</summary>
    public int Failed { get; init; }
    /// <summary>This account's requests (made or followed) with news since it last looked.</summary>
    public int Unseen { get; init; }
    /// <summary>This account's requests, made or followed, whatever their status.</summary>
    public int Own { get; init; }
    /// <summary>The newest change among the requests this sign-in sees: polled, it says when to read them again.</summary>
    public long? ChangedAt { get; init; }
    public string? SignInAgain { get; init; }

    /// <summary>Discover shows: requests are on and this account may ask.</summary>
    public bool Discover => Enabled && Access != RequestAccess.Off;

    /// <summary>Asking works from this sign-in: Discover shows, and the sign-in has <c>requests</c>.</summary>
    public bool MayAsk => Discover && SignInAgain != RequestScope.Requests;

    /// <summary>"Your requests" shows.</summary>
    public bool YourRequests => Enabled && SignInAgain != RequestScope.Requests && (Access != RequestAccess.Off || Own > 0);

    /// <summary>The way into requests shows: Your requests, or for managers Requests.</summary>
    public bool RequestsEntry => YourRequests || (Enabled && Manage);

    /// <summary>The number on the way into requests: for managers, what waits for them too.</summary>
    public int Badge => Manage ? Pending + Failed + Unseen : Unseen;

    /// <summary>The badge is in the danger colour: requests failed and wait for a manager.</summary>
    public bool BadgeAlarm => Manage && Failed > 0;
}

/// <summary>What a title is for this account, the first of these that holds (docs/API.md, "Requests").</summary>
public static class TitleStatus
{
    /// <summary>A library it can open has it whole.</summary>
    public const string Library = "library";
    /// <summary>Above its age limit: it can't ask.</summary>
    public const string Restricted = "restricted";
    /// <summary>Active requests cover everything it lacks.</summary>
    public const string Requested = "requested";
    /// <summary>Its last request for it was declined in the last 30 days.</summary>
    public const string Declined = "declined";
    /// <summary>Shows: some of it is in a library or asked for, and more seasons can be.</summary>
    public const string Partial = "partial";
    /// <summary>It may be asked for.</summary>
    public const string None = "none";
}

/// <summary>A title of Discover's lists and search, and the start of a title page.</summary>
public sealed record DiscoverTitle
{
    public string Kind { get; init; } = "";
    public long TmdbId { get; init; }
    public string Title { get; init; } = "";
    public int? Year { get; init; }
    public string Overview { get; init; } = "";
    public string? PosterUrl { get; init; }
    public string? BackdropUrl { get; init; }
    /// <summary>TMDB users', 0-10; null with too few votes.</summary>
    public double? Rating { get; init; }
    public string Status { get; init; } = TitleStatus.None;
    /// <summary>The library's title, where this account can open it.</summary>
    public long? ItemId { get; init; }
    /// <summary>An active request for it this account made or follows (any, for managers): never a status.</summary>
    public long? RequestId { get; init; }

    /// <summary>The key telling titles apart in lists: a movie and a show may share a TMDB id.</summary>
    public string Key => $"{Kind}:{TmdbId}";
}

/// <summary>
/// <c>{ page, total_pages, results }</c>: a page of a list or of search. A page may hold fewer
/// than 20 titles, even none, before the last one.
/// </summary>
public sealed record DiscoverPage
{
    public int Page { get; init; } = 1;
    public int TotalPages { get; init; }
    public IReadOnlyList<DiscoverTitle> Results { get; init; } = Array.Empty<DiscoverTitle>();
}

/// <summary>Discover's lists, by their path, in their order.</summary>
public enum DiscoverList
{
    Trending,
    Movies,
    Shows,
    Upcoming,
    Anime,
}

public static class DiscoverLists
{
    public static readonly IReadOnlyList<DiscoverList> All = new[] { DiscoverList.Trending, DiscoverList.Movies, DiscoverList.Shows, DiscoverList.Upcoming, DiscoverList.Anime };

    /// <summary>The list's path: /api/discover/{wire}.</summary>
    public static string Wire(this DiscoverList list) => list switch
    {
        DiscoverList.Trending => "trending",
        DiscoverList.Movies => "movies",
        DiscoverList.Shows => "shows",
        DiscoverList.Upcoming => "upcoming",
        _ => "anime",
    };
}

/// <summary>One of a title's cast, in billing order.</summary>
public sealed record CastMember
{
    public string Name { get; init; } = "";
    public string? Character { get; init; }
    public string? PhotoUrl { get; init; }
}

/// <summary>A season's state for this account, by the server's rule for a whole season.</summary>
public static class SeasonState
{
    public const string Library = "library";
    public const string Partial = "partial";
    public const string Requested = "requested";
    public const string Missing = "missing";
    public const string Unaired = "unaired";
}

/// <summary>A season of a show on its title page; number 0 is the specials.</summary>
public sealed record DiscoverSeason
{
    public int Number { get; init; }
    public string Name { get; init; } = "";
    /// <summary>Episodes TMDB lists.</summary>
    public int Episodes { get; init; }
    /// <summary>Of those, aired by today.</summary>
    public int Aired { get; init; }
    public string? AirDate { get; init; }
    public string? PosterUrl { get; init; }
    /// <summary>Episodes in a library this account can open.</summary>
    public int InLibrary { get; init; }
    public bool Requested { get; init; }
    public string State { get; init; } = "";
    /// <summary>A request for it this account made or followed was declined in the last 30 days.</summary>
    public bool Declined { get; init; }
    /// <summary>The active request asking for it, where this account made or follows it (any, for managers).</summary>
    public long? RequestId { get; init; }
}

/// <summary>A decline that still holds for this account: it may ask again at <see cref="AgainAt"/>.</summary>
public sealed record DeclinedTitle
{
    /// <summary>The administrator's reason; null without one, and for a follower.</summary>
    public string? Reason { get; init; }
    public long At { get; init; }
    public long AgainAt { get; init; }
}

/// <summary>A title page: GET /api/discover/movie/{tmdb_id} and /show/{tmdb_id}.</summary>
public sealed record DiscoverDetail
{
    public string Kind { get; init; } = "";
    public long TmdbId { get; init; }
    public string Title { get; init; } = "";
    public int? Year { get; init; }
    public string Overview { get; init; } = "";
    public string? PosterUrl { get; init; }
    public string? BackdropUrl { get; init; }
    public double? Rating { get; init; }
    public string Status { get; init; } = TitleStatus.None;
    public long? ItemId { get; init; }
    public long? RequestId { get; init; }
    /// <summary>Only where it differs from <see cref="Title"/>.</summary>
    public string? OriginalTitle { get; init; }
    public string? Tagline { get; init; }
    public IReadOnlyList<string> Genres { get; init; } = Array.Empty<string>();
    /// <summary>A movie's release date, a show's first air date: "2008-01-20".</summary>
    public string? ReleaseDate { get; init; }
    public int? RuntimeMinutes { get; init; }
    /// <summary>For the server's region: "PG-13", "TV-MA".</summary>
    public string? Certification { get; init; }
    /// <summary>A show's first network, a movie's first studio.</summary>
    public string? Network { get; init; }
    /// <summary>Shows: "Returning Series", "Ended".</summary>
    public string? ShowStatus { get; init; }
    public bool Anime { get; init; }
    public IReadOnlyList<CastMember> Cast { get; init; } = Array.Empty<CastMember>();
    /// <summary>Shows, in TMDB's order (specials first when there are some); null for movies.</summary>
    public IReadOnlyList<DiscoverSeason>? Seasons { get; init; }
    public string? ImdbId { get; init; }
    public DeclinedTitle? Declined { get; init; }
    /// <summary>Why it can't be asked for: "Rated R: above this account's age limit".</summary>
    public string? Restriction { get; init; }
    /// <summary>This account follows an active request for it.</summary>
    public bool Watching { get; init; }
    /// <summary>This account's requests for it and those it follows (everyone's, for managers), newest first.</summary>
    public IReadOnlyList<MediaRequest> Requests { get; init; } = Array.Empty<MediaRequest>();
    public int? MoviesLeft { get; init; }
    public int? SeasonsLeft { get; init; }
    public long? MoviesNextAt { get; init; }
    public long? SeasonsNextAt { get; init; }

    /// <summary>The title as lists show it.</summary>
    public DiscoverTitle ToTitle() => new DiscoverTitle
    {
        Kind = Kind,
        TmdbId = TmdbId,
        Title = Title,
        Year = Year,
        Overview = Overview,
        PosterUrl = PosterUrl,
        BackdropUrl = BackdropUrl,
        Rating = Rating,
        Status = Status,
        ItemId = ItemId,
        RequestId = RequestId,
    };
}

/// <summary>The decision on a request.</summary>
public static class RequestStatus
{
    public const string Pending = "pending";
    public const string Approved = "approved";
    public const string Declined = "declined";
    public const string Failed = "failed";
    public const string Available = "available";
}

/// <summary>Where a request is, from its decision to the library.</summary>
public static class RequestStage
{
    /// <summary>It waits for an administrator.</summary>
    public const string Pending = "pending";
    public const string Declined = "declined";
    /// <summary>It couldn't be sent to Radarr or Sonarr (see <see cref="MediaRequest.RetryAt"/>).</summary>
    public const string Failed = "failed";
    /// <summary>Approved, not sent: no Radarr or Sonarr for its kind, sending, or added by hand.</summary>
    public const string Approved = "approved";
    /// <summary>Sent, not out yet: <see cref="MediaRequest.StageAt"/> says when.</summary>
    public const string Waiting = "waiting";
    /// <summary>Sent and out, nothing downloading yet.</summary>
    public const string Searching = "searching";
    /// <summary>See <see cref="MediaRequest.Download"/>.</summary>
    public const string Downloading = "downloading";
    /// <summary>Downloaded, going into the library.</summary>
    public const string Importing = "importing";
    /// <summary>Shows: some seasons asked for are ready (<see cref="MediaRequest.AvailableSeasons"/>).</summary>
    public const string Partial = "partial";
    /// <summary>Ready to watch.</summary>
    public const string Available = "available";
    /// <summary>It was available; no library has it any more.</summary>
    public const string Gone = "gone";
}

/// <summary>What this sign-in may do to a request now; apps list exactly these, in their order.</summary>
public static class RequestAction
{
    /// <summary>Its maker: DELETE /api/requests/{id} (Remove, on a declined one).</summary>
    public const string Withdraw = "withdraw";
    /// <summary>A follower: DELETE /api/requests/{id}/watch.</summary>
    public const string Unwatch = "unwatch";
    public const string Approve = "approve";
    /// <summary>A failed one, approved again.</summary>
    public const string Retry = "retry";
    public const string Decline = "decline";
    /// <summary>Approved without sending: added by hand.</summary>
    public const string Handled = "handled";
    /// <summary>Managers: removed for everyone.</summary>
    public const string Delete = "delete";
    /// <summary>Asked for again, as it was.</summary>
    public const string AskAgain = "ask_again";
}

/// <summary>Someone who asked for, or decided on, a request.</summary>
public sealed record RequestUser
{
    public long Id { get; init; }
    public string Name { get; init; } = "";
    public string? AvatarUrl { get; init; }
    /// <summary>"#38bdf8": the circle behind the initial when there's no picture.</summary>
    public string? AvatarColor { get; init; }
}

/// <summary>What Radarr's or Sonarr's queue says of a download.</summary>
public static class DownloadState
{
    public const string Queued = "queued";
    public const string Downloading = "downloading";
    public const string Paused = "paused";
    public const string Importing = "importing";
    public const string Warning = "warning";
}

/// <summary>A request's download, while Radarr or Sonarr has it queued.</summary>
public sealed record RequestDownload
{
    /// <summary>0-1, over everything queued for it.</summary>
    public double Progress { get; init; }
    public string State { get; init; } = "";
    /// <summary>When it should be done.</summary>
    public long? Eta { get; init; }
    /// <summary>Shows: episodes queued; movies: 1.</summary>
    public int Episodes { get; init; }
}

/// <summary>For managers: the service a request was sent to, with the quality profile's name and the folder.</summary>
public sealed record SentTo
{
    /// <summary>"radarr" or "sonarr".</summary>
    public string Service { get; init; } = "";
    public string? QualityProfile { get; init; }
    public string? RootFolder { get; init; }
}

/// <summary>A request for a movie, or for seasons of a show.</summary>
public sealed record MediaRequest
{
    public long Id { get; init; }
    public string Kind { get; init; } = "";
    public long TmdbId { get; init; }
    public string Title { get; init; } = "";
    public int? Year { get; init; }
    public string? PosterUrl { get; init; }
    public string? BackdropUrl { get; init; }
    /// <summary>Shows: the seasons asked for; null for movies.</summary>
    public IReadOnlyList<int>? Seasons { get; init; }
    /// <summary>Shows: those ready.</summary>
    public IReadOnlyList<int> AvailableSeasons { get; init; } = Array.Empty<int>();
    public bool Anime { get; init; }
    public string Status { get; init; } = "";
    public string Stage { get; init; } = "";
    /// <summary>Waiting: when it comes out (a release known by its day is that day at 12:00 UTC).</summary>
    public long? StageAt { get; init; }
    /// <summary>Declined: the administrator's (null for followers); failed: what went wrong (managers only).</summary>
    public string? Reason { get; init; }
    /// <summary>Managers: what to know about it ("Added by hand").</summary>
    public string? Note { get; init; }
    /// <summary>This account made it.</summary>
    public bool Mine { get; init; }
    /// <summary>This account follows it.</summary>
    public bool Watching { get; init; }
    /// <summary>Managers: how many follow it.</summary>
    public int Watchers { get; init; }
    /// <summary>It has news this account hasn't seen: available, partly available or declined.</summary>
    public bool Unseen { get; init; }
    public IReadOnlyList<string> Actions { get; init; } = Array.Empty<string>();
    /// <summary>Null for someone else's request seen by someone who doesn't manage.</summary>
    public RequestUser? RequestedBy { get; init; }
    public RequestUser? DecidedBy { get; init; }
    public long? DecidedAt { get; init; }
    public long CreatedAt { get; init; }
    public long UpdatedAt { get; init; }
    public long? AvailableAt { get; init; }
    /// <summary>Failed: when the server tries again by itself; null: it won't.</summary>
    public long? RetryAt { get; init; }
    public SentTo? SentTo { get; init; }
    /// <summary>The library's title once there, where this account can open it.</summary>
    public long? ItemId { get; init; }
    public RequestDownload? Download { get; init; }

    /// <summary>Pending, approved or failed: it still asks for something.</summary>
    public bool Active => Status is RequestStatus.Pending or RequestStatus.Approved or RequestStatus.Failed;

    /// <summary>Something of it downloads or goes into the library: worth reading again every 10 s.</summary>
    public bool Moving => Download != null || Stage is RequestStage.Downloading or RequestStage.Importing;
}

/// <summary>
/// GET /api/requests: a page of requests, how many match (<see cref="Total"/>), how many have each
/// status with every filter but <c>status</c> (<see cref="Counts"/>), and for managers the people
/// with requests matching every filter but the person (<see cref="People"/>, by name).
/// </summary>
public sealed record RequestList
{
    public int Total { get; init; }
    public IReadOnlyDictionary<string, int> Counts { get; init; } = new Dictionary<string, int>();
    public IReadOnlyList<MediaRequest> Requests { get; init; } = Array.Empty<MediaRequest>();
    public IReadOnlyList<RequestUser> People { get; init; } = Array.Empty<RequestUser>();

    /// <summary>How many have <paramref name="status"/>.</summary>
    public int Count(string status) => Counts.TryGetValue(status, out var n) ? n : 0;
}

/// <summary>Which requests GET /api/requests lists, as its query.</summary>
public sealed record RequestQuery
{
    /// <summary>One status or several.</summary>
    public IReadOnlyList<string> Statuses { get; init; } = Array.Empty<string>();
    /// <summary>Managers: pending ones and the failed ones the server won't retry.</summary>
    public bool Attention { get; init; }
    public string? Kind { get; init; }
    /// <summary>Managers: one person's.</summary>
    public long? UserId { get; init; }
    /// <summary>Titles containing this.</summary>
    public string? Q { get; init; }
    /// <summary>Oldest first, instead of newest.</summary>
    public bool Oldest { get; init; }
    /// <summary>Only this account's and those it follows (managers see everyone's without it).</summary>
    public bool Mine { get; init; }
    /// <summary>Only those with news, newest news first.</summary>
    public bool Unseen { get; init; }
    public long? ChangedSince { get; init; }
    public int Offset { get; init; }
    public int? Limit { get; init; }
}

/// <summary>The body of POST /api/requests: a movie, or seasons of a show (null: those the server picks).</summary>
public sealed record NewRequest(string Kind, long TmdbId, IReadOnlyList<int>? Seasons = null)
{
    /// <summary>The body as the server reads it.</summary>
    public IReadOnlyDictionary<string, object?> Body()
    {
        var body = new Dictionary<string, object?> { ["kind"] = Kind, ["tmdb_id"] = TmdbId };
        if (Seasons != null) body["seasons"] = Seasons.ToList();
        return body;
    }
}

/// <summary>What POST /api/requests answered: a new request (<paramref name="Created"/>, 201), or one this account now follows (200).</summary>
public sealed record Asked(MediaRequest Request, bool Created);

/// <summary>The body of POST /api/requests/{id}/approve: the choices of the dialog; nulls take the settings'.</summary>
public sealed record Approval(long? QualityProfile = null, string? RootFolder = null, IReadOnlyList<int>? Seasons = null)
{
    public IReadOnlyDictionary<string, object?> Body()
    {
        var body = new Dictionary<string, object?>();
        if (QualityProfile != null) body["quality_profile"] = QualityProfile;
        if (RootFolder != null) body["root_folder"] = RootFolder;
        if (Seasons != null) body["seasons"] = Seasons.ToList();
        return body;
    }
}

/// <summary>A quality profile of Radarr or Sonarr.</summary>
public sealed record QualityProfile
{
    public long Id { get; init; }
    public string Name { get; init; } = "";
}

/// <summary>A root folder of Radarr or Sonarr.</summary>
public sealed record RootFolder
{
    public string Path { get; init; } = "";
    public long? FreeBytes { get; init; }
}

/// <summary>
/// What a manager may pick when approving, from one service: its quality profiles and root
/// folders, with the settings' choices (anime's for anime). <see cref="Error"/>: it didn't
/// answer, and these are the last lists known.
/// </summary>
public sealed record ArrOptions
{
    public IReadOnlyList<QualityProfile> QualityProfiles { get; init; } = Array.Empty<QualityProfile>();
    public IReadOnlyList<RootFolder> RootFolders { get; init; } = Array.Empty<RootFolder>();
    public long? QualityProfile { get; init; }
    public string? RootFolder { get; init; }
    public long? AnimeQualityProfile { get; init; }
    public string? AnimeRootFolder { get; init; }
    public string? Error { get; init; }
}

/// <summary>GET /api/requests/options: each service's choices, null where it isn't connected.</summary>
public sealed record RequestOptions
{
    public ArrOptions? Radarr { get; init; }
    public ArrOptions? Sonarr { get; init; }

    /// <summary>The service a request of <paramref name="kind"/> goes to.</summary>
    public ArrOptions? Of(string kind) => kind == TitleKind.Show ? Sonarr : Radarr;
}

/// <summary>
/// GET /api/items/{id}/request: a library title as Discover sees it, with the newest active
/// request for it this account made or follows (any, for managers). <see cref="Askable"/>: the
/// seasons the picker would offer without a limit (empty for movies; null while the server has
/// no details for it).
/// </summary>
public sealed record ItemRequest
{
    public string Kind { get; init; } = "";
    public long TmdbId { get; init; }
    public string Status { get; init; } = TitleStatus.None;
    public MediaRequest? Request { get; init; }
    public IReadOnlyList<int>? Askable { get; init; }
}
