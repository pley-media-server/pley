using System;
using System.Collections.Generic;
using System.Globalization;
using System.Linq;
using Pley.Tv.Core.Api;
using Pley.Tv.Core.Ui;

namespace Pley.Tv.Core.Requests;

/// <summary>
/// Requests' words (the Android app's strings_requests.xml and strings_tv_requests.xml, and its
/// RequestTexts): what the rules decide, said. Buttons say Request; running text says "ask for".
/// Limits are "per 7 days" or "for now", never "this week". Times are relative to
/// <see cref="now"/> (unix seconds) and days are in the TV's time zone: "in 3 days", "tomorrow",
/// "Out Nov 4".
/// </summary>
public sealed class RequestText
{
    public const string Discover = "Discover";
    public const string Yours = "Your requests";
    /// <summary>The managers' page: everyone's requests.</summary>
    public const string Manage = "Requests";
    public const string Attention = "Needs attention";
    public const string Open = "Open";
    public const string Request = "Request";
    public const string Requested = "Requested";
    public const string Following = "Following";
    public const string StopFollowing = "Stop following";
    public const string TellMeToo = "Tell me too";
    public const string AskedAlready = "Someone asked for it already.";
    public const string MoreSeasons = "Request more seasons";
    public const string ReadyElsewhere = "Ready, in a library you can't open: ask an administrator";
    public const string Watch = "Watch";
    public const string PickerTitle = "Which seasons?";
    public const string PickerNone = "Pick a season.";
    public const string Cancel = "Cancel";
    public const string Retry = "Retry";
    public const string SignIn = "Sign in again";
    public const string NoAccessLine = "Requests aren't open to this account.";
    public const string DiscoverFailed = "Couldn't load Discover";
    public const string SearchFailed = "Couldn't search TMDB";
    public const string TitleFailed = "Couldn't load this title";
    public const string ListFailed = "Couldn't load the requests";
    public const string SeasonsHeading = "Seasons";
    public const string Cast = "Cast";
    public const string SearchRow = "Discover";
    public const string SeeAll = "See all";
    public const string Credit = "Details and pictures from TMDB.";
    public const string CreditAbout = "This product uses the TMDB API but is not endorsed or certified by TMDB.";
    public const string DeclineReason = "Reason (optional)";
    public const string DeclineReasonHint = "The person who asked sees it.";

    // The TV's own (strings_tv_requests.xml).
    public const string Loading = "Loading Discover";
    public const string NothingTitle = "Nothing here right now";
    public const string NothingBody = "TMDB's lists have nothing this profile can ask for. Search for a title instead.";
    public const string ListNothingBody = "TMDB's list has nothing this profile can ask for.";
    public const string ListFailedShort = "Couldn't load it";
    /// <summary>Beside a grid's lone Show more: the pages read so far had nothing.</summary>
    public const string NoneYet = "Nothing here yet";
    public const string NoneYetBody = "TMDB's pages read so far have nothing this profile can ask for.";
    public const string ShowMore = "Show more";
    public const string Overview = "Overview";
    public const string Back = "Back";
    public const string SignInTitle = "Sign in again?";
    public const string KindAll = "Movies and shows";
    public const string AskedByLabel = "Asked by";
    public const string Find = "Find something to ask for";
    public const string Close = "Close";

    // Lists.
    public const string FilterAll = "All";
    public const string FilterPending = "Pending";
    public const string FilterApproved = "Approved";
    public const string FilterAvailable = "Available";
    public const string FilterDeclined = "Declined";
    public const string FilterFailed = "Failed";
    public const string FilterMovies = "Movies";
    public const string FilterShows = "Shows";
    public const string FilterEveryone = "Everyone";
    public const string FilterMine = "Yours";
    public const string FilterTitle = "Search requests";
    public const string NoneTitle = "Nothing asked for yet";
    public const string NoneBody = "Find movies and shows in Discover, and ask for what the libraries don't have.";
    public const string NoneBodyOff = "Requests you made or follow show here.";
    public const string AttentionNone = "Nothing needs you";
    public const string AttentionNoneBody = "Requests waiting for a decision, and those that couldn't be sent, show here.";
    public const string FilteredNone = "No requests match";
    public const string FilteredNoneBody = "Try other filters.";

    const long Hour = 3_600, Day = 86_400;
    static readonly CultureInfo English = CultureInfo.GetCultureInfo("en-US");

    readonly Func<long> now;
    readonly TimeZoneInfo zone;

    /// <param name="now">Unix seconds now.</param>
    /// <param name="zone">The TV's time zone; local without.</param>
    public RequestText(Func<long> now, TimeZoneInfo? zone = null)
    {
        this.now = now;
        this.zone = zone ?? TimeZoneInfo.Local;
    }

    // ---- titles

    public static string Badge(TitleBadge badge) => badge switch
    {
        TitleBadge.InLibrary => "In library",
        TitleBadge.PartlyInLibrary => "Partly in library",
        TitleBadge.Requested => "Requested",
        _ => "Declined",
    };

    public static string Kind(string kind) => kind == TitleKind.Show ? "Show" : "Movie";

    public static string Row(DiscoverList list) => list switch
    {
        DiscoverList.Trending => "Trending this week",
        DiscoverList.Movies => "Popular movies",
        DiscoverList.Shows => "Popular shows",
        DiscoverList.Upcoming => "Coming to cinemas",
        _ => "Popular anime",
    };

    /// <summary>Over the grid of everything TMDB finds: "Results for “dune”".</summary>
    public static string ResultsFor(string query) => $"Results for “{query}”";

    public static string SearchNoneYet(string query) => $"Nothing yet for “{query}”";

    public static string SearchNone(string query) => $"Nothing on TMDB matches “{query}”";

    public const string SearchNoneBody = "Try a different spelling, or the title in its own language.";

    /// <summary>"This TV shows a code to link it to Home server again."</summary>
    public static string SignInBody(string server) => $"This TV shows a code to link it to {server} again.";

    /// <summary>"Declined: “Not for us”. You can ask again in 12 days."</summary>
    public string Declined(MainAction.Declined action)
    {
        var reason = action.Reason?.Trim() is { Length: > 0 } r ? r : null;
        var again = action.AgainAt is { } at ? Until(at) : null;
        if (reason != null && again != null) return $"Declined: “{reason}”. You can ask again {again}.";
        if (again != null) return $"Declined. You can ask again {again}.";
        if (reason != null) return $"Declined: “{reason}”.";
        return "Declined.";
    }

    // ---- seasons

    /// <summary>"Season 2", "Seasons 1–3, 5", "Seasons 1–2 and specials", "Specials".</summary>
    public static string Seasons(IEnumerable<int> numbers)
    {
        var list = numbers.ToList();
        var specials = list.Contains(0);
        var rest = list.Where(n => n > 0).Distinct().ToList();
        var ranges = RequestRules.SeasonRanges(rest);
        if (rest.Count == 0) return specials ? "Specials" : "";
        if (rest.Count == 1) return specials ? $"Season {ranges} and specials" : $"Season {ranges}";
        return specials ? $"Seasons {ranges} and specials" : $"Seasons {ranges}";
    }

    /// <summary>A season's name on its own: TMDB's, else "Season 2" or "Specials".</summary>
    public static string SeasonName(DiscoverSeason season) => string.IsNullOrWhiteSpace(season.Name) ? Seasons(new[] { season.Number }) : season.Name;

    /// <summary>How a season stands for this account: "In library", "4 of 10 episodes in library", "Airs Nov 4, 2026", "10 episodes".</summary>
    public static string SeasonState(DiscoverSeason season)
    {
        if (season.Declined) return "Declined";
        switch (season.State)
        {
            case Requests.SeasonState.Library:
                return "In library";
            case Requests.SeasonState.Requested:
                return "Requested";
            case Requests.SeasonState.Partial:
                return $"{season.InLibrary} of {Math.Max(season.Episodes, season.InLibrary)} episodes in library";
            case Requests.SeasonState.Unaired:
                return season.AirDate is { Length: > 0 } date ? $"Airs {ItemFormat.Date(date, English)}" : "Not aired yet";
            default:
                return Format.Pluralize(season.Episodes, "episode");
        }
    }

    // ---- stages

    /// <summary>The words of <paramref name="pill"/>: "Waiting for approval", "Downloading 42% · in 20 min", "Out Nov 4".</summary>
    public string Pill(StagePill pill)
    {
        switch (pill)
        {
            case StagePill.Pending:
                return "Waiting for approval";
            case StagePill.Approved:
                return "Approved";
            case StagePill.Waiting w:
                return w.At is { } at ? (w.Show ? $"Airs {DayOf(at)}" : $"Out {DayOf(at)}") : "Not out yet";
            case StagePill.Searching:
                return "Searching";
            case StagePill.Downloading d:
                switch (d.State)
                {
                    case DownloadState.Queued:
                        return "Waiting to download";
                    case DownloadState.Paused:
                        return "Download paused";
                    case DownloadState.Warning:
                        return "The download has a problem";
                }
                var words = $"Downloading {(int)(d.Progress * 100)}%";
                return d.Eta is { } eta && eta > now() ? $"{words} · {Until(eta)}" : words;
            case StagePill.Importing:
                return "Adding to the library";
            case StagePill.Partial p:
                return $"{Seasons(p.Seasons)} ready";
            case StagePill.Available:
                return "Ready to watch";
            case StagePill.Gone:
                return "No longer in a library";
            case StagePill.Declined:
                return "Declined";
            case StagePill.CouldNotSend c:
                return c.Reason is { } reason && !string.IsNullOrWhiteSpace(reason) ? $"Couldn't send: {reason}" : "Couldn't send";
            case StagePill.NotSentYet:
                return "Not sent yet";
            default:
                return "Requested";
        }
    }

    /// <summary>The line under a failed request's pill: when the server tries again, or that an administrator will look.</summary>
    public string? PillLine(StagePill pill) => pill switch
    {
        StagePill.NotSentYet n => n.RetryAt is { } at ? $"Trying again {Until(at)}." : "It couldn't be sent to download yet: an administrator will look at it.",
        StagePill.CouldNotSend c => c.RetryAt is { } at ? $"Trying again {Until(at)}." : null,
        _ => null,
    };

    /// <summary>Who asked and when: "Ann · 3 days ago", "Asked for by someone else · 3 days ago", or just when.</summary>
    public string AskedBy(MediaRequest request, bool manage)
    {
        var at = Ago(request.CreatedAt);
        if (request.Mine && !manage) return at;
        return $"{request.RequestedBy?.Name ?? "Asked for by someone else"} · {at}";
    }

    public static string Watchers(int count) => count == 1 ? "1 person follows it" : $"{count} people follow it";

    /// <summary>Managers: where it went, "Radarr · HD-1080p · /movies"; null before it was sent.</summary>
    public static string? SentTo(MediaRequest request)
    {
        if (request.SentTo is not { } sent) return null;
        var service = request.Kind == TitleKind.Show ? "Sonarr" : "Radarr";
        return string.Join(" · ", new[] { service, sent.QualityProfile, sent.RootFolder }.Where(s => s != null));
    }

    // ---- allowance

    /// <summary>
    /// The allowance line, for <paramref name="kind"/> (null: both): "You can ask for 2 more movies
    /// and 5 more seasons." with "Limits: 3 movies and 10 seasons per 7 days.", or when more can
    /// be asked for. Empty without limits.
    /// </summary>
    public IReadOnlyList<string> Allowance(Allowance allowance, string? kind = null)
    {
        var movies = kind != TitleKind.Show && allowance.MoviesLeft != null;
        var shows = kind != TitleKind.Movie && allowance.SeasonsLeft != null;
        if (!movies && !shows) return Array.Empty<string>();
        var moviesOut = movies && allowance.MoviesLeft == 0;
        var seasonsOut = shows && allowance.SeasonsLeft == 0;
        var moviesAt = allowance.MoviesNextAt is { } m ? Until(m) : null;
        var seasonsAt = allowance.SeasonsNextAt is { } s ? Until(s) : null;
        const string AllForNow = "You've asked for all you can for now.";
        string AllMovies() => moviesAt != null ? $"You've asked for all the movies you can for now: you can ask for more {moviesAt}." : AllForNow;
        string AllSeasons() => seasonsAt != null ? $"You've asked for all the seasons you can for now: you can ask for more {seasonsAt}." : AllForNow;
        if (moviesOut && seasonsOut)
            return new[] { moviesAt != null && seasonsAt != null ? $"You've asked for all you can for now: more movies {moviesAt}, more seasons {seasonsAt}." : AllForNow };
        if (moviesOut && !shows) return new[] { AllMovies() };
        if (seasonsOut && !movies) return new[] { AllSeasons() };
        var lines = new List<string>();
        if (moviesOut) lines.Add(AllMovies());
        if (seasonsOut) lines.Add(AllSeasons());
        var left = new List<string>();
        if (movies && allowance.MoviesLeft is > 0 and var ml) left.Add(ml == 1 ? "1 more movie" : $"{ml} more movies");
        if (shows && allowance.SeasonsLeft is > 0 and var sl) left.Add(sl == 1 ? "1 more season" : $"{sl} more seasons");
        if (left.Count == 2) lines.Add($"You can ask for {left[0]} and {left[1]}.");
        else if (left.Count == 1) lines.Add($"You can ask for {left[0]}.");
        var limits = new List<string>();
        if (movies && allowance.MoviesLimit is { } mlim) limits.Add(Format.Pluralize(mlim, "movie"));
        if (shows && allowance.SeasonsLimit is { } slim) limits.Add(Format.Pluralize(slim, "season"));
        if (limits.Count == 2) lines.Add($"Limits: {limits[0]} and {limits[1]} per 7 days.");
        else if (limits.Count == 1) lines.Add($"Limit: {limits[0]} per 7 days.");
        return lines;
    }

    // ---- asking

    /// <summary>What asking for <paramref name="title"/> did: asked (waiting, approved, or not sent yet), or followed.</summary>
    public static string Asked(Asked asked, string title)
    {
        if (!asked.Created) return $"You'll hear when “{title}” is ready.";
        return asked.Request.Status switch
        {
            RequestStatus.Approved or RequestStatus.Available => $"“{title}” is approved and on its way.",
            RequestStatus.Failed => $"Asked for “{title}”, but it couldn't be sent to download yet.",
            _ => $"Asked for “{title}”: an administrator decides.",
        };
    }

    /// <summary>"Request 3 seasons", or "Pick a season." with none picked.</summary>
    public static string PickerButton(SeasonPicker picker) =>
        picker.Count == 0 ? PickerNone : picker.Count == 1 ? "Request 1 season" : $"Request {picker.Count} seasons";

    // ---- actions

    public static string Action(CardAction action) => action switch
    {
        CardAction.Withdraw => "Withdraw",
        CardAction.Remove => "Remove",
        CardAction.StopFollowing => StopFollowing,
        CardAction.Approve => "Approve",
        CardAction.ApproveWith => "Approve…",
        CardAction.TryAgain => "Try again",
        CardAction.Decline => "Decline",
        CardAction.DeclineWithReason => "Decline with a reason…",
        CardAction.Handled => "Mark as added by hand",
        CardAction.Delete => "Delete",
        _ => "Request again",
    };

    /// <summary>The question <paramref name="action"/> asks first, for <paramref name="title"/>; null when it asks none.</summary>
    public static string? Confirm(CardAction action, string title) => action switch
    {
        CardAction.Withdraw => $"Withdraw your request for “{title}”?",
        CardAction.Remove => $"Remove “{title}” from your requests?",
        CardAction.Delete => "Delete this request? Radarr and Sonarr keep what they have.",
        _ => null,
    };

    // ---- lists

    public static string View(RequestView view) => view switch
    {
        RequestView.Attention => Attention,
        RequestView.All => FilterAll,
        RequestView.Pending => FilterPending,
        RequestView.Approved => FilterApproved,
        RequestView.Available => FilterAvailable,
        RequestView.Declined => FilterDeclined,
        _ => FilterFailed,
    };

    /// <summary>A view with how many it holds: "Pending · 2".</summary>
    public static string Counted(string label, int count) => $"{label} · {count}";

    // ---- notices

    /// <summary>
    /// The number on the way into requests, said for screen readers: "3 requests need you, 1
    /// couldn't be sent" for managers, "2 of your requests have news" for others.
    /// </summary>
    public static string Count(RequestsEntry entry)
    {
        if (!entry.Manage) return News(entry.Badge);
        var need = entry.Badge == 1 ? "1 request needs you" : $"{entry.Badge} requests need you";
        return entry.Failed <= 0 ? need : $"{need}, {entry.Failed} couldn't be sent";
    }

    static string News(int count) => count == 1 ? "1 of your requests has news" : $"{count} of your requests have news";

    public static string Notice(RequestNotice notice)
    {
        switch (notice)
        {
            case RequestNotice.Ready ready:
            {
                var seasons = ready.Seasons ?? Array.Empty<int>();
                if (ready.Complete || seasons.Count == 0) return $"“{ready.Title}” is ready to watch.";
                return seasons.Count == 1 ? $"{Seasons(seasons)} of “{ready.Title}” is ready to watch." : $"{Seasons(seasons)} of “{ready.Title}” are ready to watch.";
            }
            case RequestNotice.News news:
                return News(news.Count);
            default:
                return "";
        }
    }

    // ---- problems

    /// <summary>What <paramref name="problem"/> says, its first line; <paramref name="summary"/> for the limits and whether this sign-in manages.</summary>
    public string Problem(RequestProblem problem, RequestSummary? summary) => problem switch
    {
        RequestProblem.Off => "Requests are off",
        RequestProblem.NoAccess => "Requests aren't open to this account",
        RequestProblem.SignInAgain s => s.Scope == RequestScope.Approvals ? "Sign in again to approve and decline here." : "Sign in again to ask for movies and shows",
        RequestProblem.TmdbKey => "TMDB refused this server's key",
        RequestProblem.TmdbDown => "TMDB isn't answering",
        RequestProblem.Restricted r => r.Text,
        RequestProblem.Declined d => d.RetryAt is { } at ? $"You can ask again {Until(at)}" : "Declined.",
        RequestProblem.Limit l => l.RetryAt is { } at ? $"You can ask for more {Until(at)}" : AtMost(summary),
        RequestProblem.Conflict c => c.Message,
        RequestProblem.NotFound => "This title isn't on TMDB",
        RequestProblem.Offline o => Errors.Message(o.Error),
        RequestProblem.Other o => Errors.Message(o.Error),
        _ => "",
    };

    /// <summary>The line under <paramref name="problem"/>'s, where it has one.</summary>
    public static string? ProblemBody(RequestProblem problem, RequestSummary? summary) => problem switch
    {
        RequestProblem.Off => "This server doesn't take requests for movies and shows.",
        RequestProblem.NoAccess => "An administrator can let it ask for movies and shows.",
        RequestProblem.SignInAgain s => s.Scope == RequestScope.Approvals ? null : "This app signed in before it could ask. Signing in again lets it.",
        RequestProblem.TmdbKey => summary?.Manage == true ? "Fix it in the server's settings: Settings → Server, on the web." : "An administrator needs to check it.",
        RequestProblem.TmdbDown => "Its details and pictures come back once it answers again.",
        RequestProblem.NotFound => "It may have been taken off since.",
        _ => null,
    };

    /// <summary>429 with no time: more was asked than the limit allows at all.</summary>
    static string AtMost(RequestSummary? summary)
    {
        if (summary?.SeasonsLimit is { } seasons) return $"You can ask for at most {Format.Pluralize(seasons, "season")} per 7 days: pick fewer.";
        if (summary?.MoviesLimit is { } movies) return $"You can ask for at most {Format.Pluralize(movies, "movie")} per 7 days.";
        return "You've asked for all you can for now.";
    }

    // ---- times

    /// <summary>How long until <paramref name="ts"/>: "in 5 min", "in 3 hours", "tomorrow", "in 12 days"; "in a moment" once it's past.</summary>
    public string Until(long ts)
    {
        var diff = ts - now();
        if (diff <= 0) return "in a moment";
        if (diff < Hour)
        {
            var minutes = Math.Max(1, (int)Math.Ceiling(diff / 60.0));
            return $"in {minutes} min";
        }
        if (diff < Day)
        {
            // Kotlin's roundToInt: halves go up.
            var hours = Math.Max(1, Math.Min(23, (int)Math.Floor(diff / (double)Hour + 0.5)));
            return hours == 1 ? "in 1 hour" : $"in {hours} hours";
        }
        var days = Math.Max(1, (int)(LocalDate(ts) - LocalDate(now())).TotalDays);
        return days == 1 ? "tomorrow" : $"in {days} days";
    }

    /// <summary>How long ago <paramref name="ts"/> was: "3 days ago", "yesterday".</summary>
    public string Ago(long ts) => Format.Relative(ts, now());

    /// <summary>A day, "Nov 4" ("Nov 4, 2027" in another year), with the time within 24 hours: "today, 9:00 PM".</summary>
    public string DayOf(long ts)
    {
        var at = Local(ts);
        var diff = ts - now();
        if (diff >= 0 && diff < Day)
        {
            var time = at.ToString("h:mm tt", English);
            return at.Date == LocalDate(now()) ? $"today, {time}" : $"tomorrow, {time}";
        }
        return at.ToString(at.Year == LocalDate(now()).Year ? "MMM d" : "MMM d, yyyy", English);
    }

    DateTime Local(long ts) => TimeZoneInfo.ConvertTime(DateTimeOffset.FromUnixTimeSeconds(ts), zone).DateTime;

    DateTime LocalDate(long ts) => Local(ts).Date;
}
