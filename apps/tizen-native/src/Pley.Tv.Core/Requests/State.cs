using System;
using System.Collections.Generic;
using System.Text.Json;
using Pley.Tv.Core.Api;
using Pley.Tv.Core.Net;

namespace Pley.Tv.Core.Requests;

/// <summary>What requests are for the profile in use, from GET /api/requests/summary: whether any of it shows, and what.</summary>
public abstract record RequestsState
{
    /// <summary>Not known yet: starting, or the server never answered.</summary>
    public sealed record Unknown : RequestsState;

    /// <summary>None of it shows: requests are off, or the server is older.</summary>
    public sealed record Hidden : RequestsState;

    /// <summary>None of it shows either: requests are on, but this account may not ask, and has no requests.</summary>
    public sealed record NoAccess : RequestsState;

    /// <summary>Discover shows only "Sign in again to ask for movies and shows": this sign-in came before requests.</summary>
    public sealed record SignInAgain(RequestSummary Value) : RequestsState;

    /// <summary>Requests work: the summary says what shows (Discover, Your requests, the managers' Requests).</summary>
    public sealed record Open(RequestSummary Value) : RequestsState;

    /// <summary>The summary behind the state, where there is one.</summary>
    public RequestSummary? Summary => this switch
    {
        Open o => o.Value,
        SignInAgain s => s.Value,
        _ => null,
    };

    /// <summary>Asking works from here now: Discover's rows, search's Discover row, title pages' Request.</summary>
    public bool MayAsk => this is Open o && o.Value.MayAsk;

    /// <summary>The state <paramref name="summary"/> gives; null: not known.</summary>
    public static RequestsState Of(RequestSummary? summary)
    {
        if (summary == null) return new Unknown();
        if (!summary.Enabled) return new Hidden();
        if (summary.SignInAgain == RequestScope.Requests && summary.Discover) return new SignInAgain(summary);
        if (summary.Discover || summary.RequestsEntry) return new Open(summary);
        return new NoAccess();
    }
}

/// <summary>
/// A request changed, here or for the server (<paramref name="RequestId"/> with its title), or many
/// did (all null): screens read again what they show of it.
/// </summary>
public sealed record RequestChange(long? RequestId, string? Kind, long? TmdbId)
{
    public static readonly RequestChange All = new RequestChange(null, null, null);

    /// <summary>Many changed: whatever shows requests is read again.</summary>
    public bool Many => RequestId == null && TmdbId == null;

    /// <summary>Whether it may be about the title <paramref name="kind"/> <paramref name="tmdbId"/>.</summary>
    public bool About(string kind, long tmdbId) => Many || (TmdbId == tmdbId && (Kind == null || Kind == kind));

    /// <summary>Whether it may be about <paramref name="title"/>: its request, or the title itself.</summary>
    public bool Touches(DiscoverTitle title) =>
        (RequestId != null && title.RequestId == RequestId) || (TmdbId == title.TmdbId && (Kind == null || Kind == title.Kind));
}

/// <summary>
/// Something to tell the person about their requests, once. On a TV only what became ready to
/// watch is told (the pop-up takes focus from the page, worth it for a title to open); the shell
/// shows it when it can (never over playing video), oldest first, the last
/// <see cref="Requests.Kept"/> kept meanwhile. <see cref="Key"/> tells two apart, however alike.
/// </summary>
public abstract record RequestNotice(long Key)
{
    /// <summary>
    /// Ready to watch where <paramref name="ItemId"/> is: the whole of it (<paramref name="Complete"/>),
    /// or <paramref name="Seasons"/> of a show. <paramref name="PosterUrl"/> is the title's poster on the server.
    /// </summary>
    public sealed record Ready(long Key, long RequestId, string Title, string Kind, long? ItemId, IReadOnlyList<int>? Seasons, bool Complete, string? PosterUrl = null)
        : RequestNotice(Key);

    /// <summary>Several of the person's requests have news.</summary>
    public sealed record News(long Key, int Count) : RequestNotice(Key);
}

/// <summary>
/// The ways into requests, for the rail and search: Discover, and Your requests (Requests for
/// managers) carrying one number, in the danger colour while failed requests wait for a manager.
/// </summary>
public sealed record RequestsEntry(bool Discover = false, bool Requests = false, bool Manage = false, int Badge = 0, int Failed = 0)
{
    public static readonly RequestsEntry None = new RequestsEntry();

    /// <summary>The number is in the danger colour.</summary>
    public bool Alarm => Failed > 0;

    public static RequestsEntry Of(RequestsState state)
    {
        if (state.Summary is not { } summary) return None;
        var requests = summary.RequestsEntry;
        return new RequestsEntry(
            summary.Discover,
            requests,
            requests && summary.Manage,
            requests ? summary.Badge : 0,
            requests && summary.Manage ? summary.Failed : 0);
    }
}

/// <summary>
/// Why a request route refused, as the screens say it (the Android app's RequestProblem). A 503
/// with Pley's JSON is an answer, never the server being away: only <see cref="Offline"/> is.
/// </summary>
public abstract record RequestProblem
{
    /// <summary>503 <c>unavailable</c>: "Requests are off".</summary>
    public sealed record Off : RequestProblem;
    /// <summary>403 <c>no_access</c>: "Requests aren't open to this account".</summary>
    public sealed record NoAccess : RequestProblem;
    /// <summary>403 <c>insufficient_scope</c>: this sign-in lacks <paramref name="Scope"/>; signing in again asks for it.</summary>
    public sealed record SignInAgain(string Scope) : RequestProblem;
    /// <summary>503 <c>tmdb_key</c>: TMDB refused the server's key. No Retry: an administrator fixes it.</summary>
    public sealed record TmdbKey : RequestProblem;
    /// <summary>503 <c>tmdb_down</c>: "TMDB isn't answering", with Retry.</summary>
    public sealed record TmdbDown : RequestProblem;
    /// <summary>403 <c>restricted</c>: above the account's age limit; <paramref name="Text"/> says so.</summary>
    public sealed record Restricted(string Text) : RequestProblem;
    /// <summary>409 <c>declined</c>: declined in the last 30 days; asking again works from <paramref name="RetryAt"/>.</summary>
    public sealed record Declined(long? RetryAt) : RequestProblem;
    /// <summary>429 <c>too_many_requests</c>: past the limit until <paramref name="RetryAt"/>; null: what was asked is more than the limit itself.</summary>
    public sealed record Limit(long? RetryAt) : RequestProblem;
    /// <summary>Another 409 (<c>in_library</c>, <c>conflict</c>): the server's message, and the request as it is now, when it sent it.</summary>
    public sealed record Conflict(string Message, MediaRequest? Request) : RequestProblem;
    /// <summary>404: the title or the request isn't there (any more).</summary>
    public sealed record NotFound : RequestProblem;
    /// <summary>The server can't be reached.</summary>
    public sealed record Offline(Exception Error) : RequestProblem;
    /// <summary>Anything else: the error's sentence.</summary>
    public sealed record Other(Exception Error) : RequestProblem;

    /// <summary>Trying again may help: a Retry shows.</summary>
    public bool Retry => this is TmdbDown or Offline or Other;

    /// <summary>Why Discover's pages can't show, from <paramref name="gate"/>: signed in before requests, requests off, no access; null otherwise.</summary>
    public static RequestProblem? Of(RequestsState gate) => gate switch
    {
        RequestsState.SignInAgain => new SignInAgain(RequestScope.Requests),
        RequestsState.Hidden => new Off(),
        RequestsState.NoAccess => new NoAccess(),
        RequestsState.Open open when !open.MayAsk => new NoAccess(),
        _ => null,
    };

    public static RequestProblem Of(Exception error)
    {
        if (error is not ApiException api) return error is NetworkException { Cancelled: false } ? new Offline(error) : new Other(error);
        // The code Pley's JSON gave: a refusal without one (a proxy's 503) is no answer of requests'.
        var body = api.Body;
        var code = body is { } b ? Json.String(b, "code") : null;
        if (api.NeedsScope) return new SignInAgain(api.Scope == RequestScope.Approvals ? RequestScope.Approvals : RequestScope.Requests);
        if (code == "unavailable" || (api.Status == 503 && body is { } s && Json.Bool(s, "setup") == false)) return new Off();
        switch (code)
        {
            case "tmdb_key":
                return new TmdbKey();
            case "tmdb_down":
                return new TmdbDown();
            case "no_access":
                return new NoAccess();
            case "restricted":
                return new Restricted(api.Message);
            case "declined":
                return new Declined(RetryAtOf(api));
        }
        if (api.Status == 429 || code == "too_many_requests") return new Limit(RetryAtOf(api));
        if (api.Status == 409) return new Conflict(api.Message, ConflictRequest(api));
        if (api.Status == 404) return new NotFound();
        // A 503 without a code of these: TMDB or the server busy, worth a Retry.
        return new Other(error);
    }

    static long? RetryAtOf(ApiException api) => api.Body is { } b && Json.Number(b, "retry_at") is { } at ? (long)at : null;

    /// <summary>A 409's request as it is now, when the server sent it.</summary>
    static MediaRequest? ConflictRequest(ApiException api)
    {
        if (api.Body is not { } b || Json.Field(b, "request") is not { ValueKind: JsonValueKind.Object } request) return null;
        try
        {
            return JsonSerializer.Deserialize<MediaRequest>(request.GetRawText(), ApiJson.Options);
        }
        catch (JsonException)
        {
            return null;
        }
    }
}
