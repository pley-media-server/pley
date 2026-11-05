using System;
using System.Collections.Generic;
using System.Globalization;
using System.Linq;
using Pley.Tv.Core.Api;
using Pley.Tv.Core.Ui;

namespace Pley.Tv.Core.Requests;

/// <summary>
/// Requests' words (the Android app's strings_requests.xml and strings_tv_requests.xml, and its
/// RequestTexts), in the language shown: what the rules decide, said. Buttons say Request; running
/// text says "ask for". Limits are "per 7 days" or "for now", never "this week". Times are relative
/// to <see cref="now"/> (unix seconds) and days are in the TV's time zone: "in 3 days", "tomorrow",
/// "Out Nov 4".
/// </summary>
public sealed class RequestText
{
    public static string Discover => Strings.T("requests.names.discover");
    public static string Yours => Strings.T("requests.names.yours");
    /// <summary>The managers' page: everyone's requests.</summary>
    public static string Manage => Strings.T("requests.names.manage");
    public static string Attention => Strings.T("requests.list.views.attention");
    public static string Open => Strings.T("requests.actions.open");
    public static string Request => Strings.T("requests.actions.request");
    public static string Requested => Strings.T("requests.title.requested");
    public static string Following => Strings.T("requests.title.following");
    public static string StopFollowing => Strings.T("requests.title.stopFollowing");
    public static string TellMeToo => Strings.T("requests.title.tellMeToo");
    public static string AskedAlready => Strings.T("requests.title.askedAlready");
    public static string MoreSeasons => Strings.T("requests.title.moreSeasons");
    public static string ReadyElsewhere => Strings.T("requests.title.readyElsewhere");
    public static string Watch => Strings.T("requests.actions.watch");
    public static string PickerTitle => Strings.T("requests.picker.title");
    public static string PickerNone => Strings.T("requests.picker.none");
    public static string Cancel => Strings.T("requests.actions.cancel");
    public static string Retry => Strings.T("requests.actions.retry");
    public static string SignIn => Strings.T("requests.actions.signIn");
    public static string NoAccessLine => Strings.T("requests.title.noAccess");
    public static string DiscoverFailed => Strings.T("requests.discover.failed");
    public static string SearchFailed => Strings.T("requests.discover.searchFailed");
    public static string TitleFailed => Strings.T("requests.title.failed");
    public static string ListFailed => Strings.T("requests.list.failed");
    public static string SeasonsHeading => Strings.T("requests.title.seasons");
    public static string Cast => Strings.T("requests.title.cast");
    public static string SearchRow => Strings.T("requests.discover.searchRow");
    public static string SeeAll => Strings.T("requests.actions.seeAll");
    public static string Credit => Strings.T("requests.discover.credit");
    public static string CreditAbout => Strings.T("requests.discover.creditAbout");
    public static string DeclineReason => Strings.T("requests.list.declineReason");
    public static string DeclineReasonHint => Strings.T("requests.list.declineReasonHint");

    // The TV's own (strings_tv_requests.xml).
    public static string Loading => Strings.T("requests.discover.loading");
    public static string NothingTitle => Strings.T("requests.discover.nothingTitle");
    public static string NothingBody => Strings.T("requests.discover.nothingBody");
    public static string ListNothingBody => Strings.T("requests.discover.listNothingBody");
    public static string ListFailedShort => Strings.T("requests.discover.listFailed");
    /// <summary>Beside a grid's lone Show more: the pages read so far had nothing.</summary>
    public static string NoneYet => Strings.T("requests.discover.noneYet");
    public static string NoneYetBody => Strings.T("requests.discover.noneYetBody");
    public static string ShowMore => Strings.T("requests.actions.showMore");
    public static string Overview => Strings.T("requests.title.overview");
    public static string Back => Strings.T("requests.actions.back");
    public static string Details => Strings.T("requests.actions.details");
    public static string SignInTitle => Strings.T("requests.signIn.title");
    public static string KindAll => Strings.T("requests.list.kindAll");
    public static string AskedByLabel => Strings.T("requests.list.askedBy");
    public static string Find => Strings.T("requests.list.find");
    public static string Close => Strings.T("requests.actions.close");

    // Lists.
    public static string FilterAll => Strings.T("requests.list.views.all");
    public static string FilterPending => Strings.T("requests.list.views.pending");
    public static string FilterApproved => Strings.T("requests.list.views.approved");
    public static string FilterAvailable => Strings.T("requests.list.views.available");
    public static string FilterDeclined => Strings.T("requests.list.views.declined");
    public static string FilterFailed => Strings.T("requests.list.views.failed");
    public static string FilterMovies => Strings.T("requests.list.movies");
    public static string FilterShows => Strings.T("requests.list.shows");
    public static string FilterEveryone => Strings.T("requests.list.everyone");
    public static string FilterMine => Strings.T("requests.list.mine");
    public static string FilterTitle => Strings.T("requests.list.search");
    public static string NoneTitle => Strings.T("requests.list.noneTitle");
    public static string NoneBody => Strings.T("requests.list.noneBody");
    public static string NoneBodyOff => Strings.T("requests.list.noneBodyOff");
    public static string AttentionNone => Strings.T("requests.list.attentionNone");
    public static string AttentionNoneBody => Strings.T("requests.list.attentionNoneBody");
    public static string FilteredNone => Strings.T("requests.list.filteredNone");
    public static string FilteredNoneBody => Strings.T("requests.list.filteredNoneBody");

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

    /// <summary>What days and times are written in: en-US's in English, as the Android app writes them ("Nov 4", "9:00 PM"), else the language shown's.</summary>
    static CultureInfo Culture => Strings.Language == Languages.English ? English : Strings.Culture;

    // ---- titles

    public static string Badge(TitleBadge badge) => badge switch
    {
        TitleBadge.InLibrary => Strings.T("requests.badge.inLibrary"),
        TitleBadge.PartlyInLibrary => Strings.T("requests.badge.partlyInLibrary"),
        TitleBadge.Requested => Strings.T("requests.badge.requested"),
        _ => Strings.T("requests.badge.declined"),
    };

    public static string Kind(string kind) => kind == TitleKind.Show ? Strings.T("requests.kind.show") : Strings.T("requests.kind.movie");

    public static string Row(DiscoverList list) => list switch
    {
        DiscoverList.Trending => Strings.T("requests.discover.lists.trending"),
        DiscoverList.Movies => Strings.T("requests.discover.lists.movies"),
        DiscoverList.Shows => Strings.T("requests.discover.lists.shows"),
        DiscoverList.Upcoming => Strings.T("requests.discover.lists.upcoming"),
        _ => Strings.T("requests.discover.lists.anime"),
    };

    /// <summary>Over the grid of everything TMDB finds: "Results for “dune”".</summary>
    public static string ResultsFor(string query) => Strings.T("requests.discover.resultsFor", ("query", query));

    public static string SearchNoneYet(string query) => Strings.T("requests.discover.searchNoneYet", ("query", query));

    public static string SearchNone(string query) => Strings.T("requests.discover.searchNone", ("query", query));

    public static string SearchNoneBody => Strings.T("requests.discover.searchNoneBody");

    /// <summary>"This TV shows a code to link it to Home server again."</summary>
    public static string SignInBody(string server) => Strings.T("requests.signIn.body", ("server", server));

    /// <summary>"Declined: “Not for us”. You can ask again in 12 days."</summary>
    public string Declined(MainAction.Declined action)
    {
        var reason = action.Reason?.Trim() is { Length: > 0 } r ? r : null;
        var again = action.AgainAt is { } at ? Until(at) : null;
        if (reason != null && again != null) return Strings.T("requests.declined.reasonAndWhen", ("reason", reason), ("when", again));
        if (again != null) return Strings.T("requests.declined.when", ("when", again));
        if (reason != null) return Strings.T("requests.declined.reason", ("reason", reason));
        return Strings.T("requests.declined.plain");
    }

    // ---- seasons

    /// <summary>"Season 2", "Seasons 1–3, 5", "Seasons 1–2 and specials", "Specials".</summary>
    public static string Seasons(IEnumerable<int> numbers)
    {
        var list = numbers.ToList();
        var specials = list.Contains(0);
        var rest = list.Where(n => n > 0).Distinct().ToList();
        var ranges = RequestRules.SeasonRanges(rest);
        if (rest.Count == 0) return specials ? Strings.T("requests.seasons.specials") : "";
        var seasons = Strings.Count("requests.seasons.list", rest.Count, ("list", ranges));
        return specials ? Strings.T("requests.seasons.andSpecials", ("seasons", seasons)) : seasons;
    }

    /// <summary>A season's name on its own: TMDB's, else "Season 2" or "Specials".</summary>
    public static string SeasonName(DiscoverSeason season) => string.IsNullOrWhiteSpace(season.Name) ? Seasons(new[] { season.Number }) : season.Name;

    /// <summary>How a season stands for this account: "In library", "4 of 10 episodes in library", "Airs Nov 4, 2026", "10 episodes".</summary>
    public static string SeasonState(DiscoverSeason season)
    {
        if (season.Declined) return Strings.T("requests.season.declined");
        switch (season.State)
        {
            case Requests.SeasonState.Library:
                return Strings.T("requests.season.inLibrary");
            case Requests.SeasonState.Requested:
                return Strings.T("requests.season.requested");
            case Requests.SeasonState.Partial:
                return Strings.Count("requests.season.partial", season.InLibrary, ("episodes", Math.Max(season.Episodes, season.InLibrary)));
            case Requests.SeasonState.Unaired:
                return season.AirDate is { Length: > 0 } date
                    ? Strings.T("requests.season.airs", ("date", ItemFormat.Date(date, Culture)))
                    : Strings.T("requests.season.notAired");
            default:
                return Strings.Count("requests.counts.episodes", season.Episodes);
        }
    }

    /// <summary>How many seasons a show has: "3 seasons".</summary>
    public static string SeasonCount(int count) => Strings.Count("requests.counts.seasons", count);

    // ---- stages

    /// <summary>The words of <paramref name="pill"/>: "Waiting for approval", "Downloading 42% · in 20 min", "Out Nov 4".</summary>
    public string Pill(StagePill pill)
    {
        switch (pill)
        {
            case StagePill.Pending:
                return Strings.T("requests.stage.pending");
            case StagePill.Approved:
                return Strings.T("requests.stage.approved");
            case StagePill.Waiting w:
                if (w.At is not { } at) return Strings.T("requests.stage.notOut");
                return w.Show ? Strings.T("requests.stage.airs", ("day", DayOf(at))) : Strings.T("requests.stage.out", ("day", DayOf(at)));
            case StagePill.Searching:
                return Strings.T("requests.stage.searching");
            case StagePill.Downloading d:
                switch (d.State)
                {
                    case DownloadState.Queued:
                        return Strings.T("requests.stage.queued");
                    case DownloadState.Paused:
                        return Strings.T("requests.stage.paused");
                    case DownloadState.Warning:
                        return Strings.T("requests.stage.warning");
                }
                var percent = (int)(d.Progress * 100);
                return d.Eta is { } eta && eta > now()
                    ? Strings.T("requests.stage.downloadingUntil", ("percent", percent), ("when", Until(eta)))
                    : Strings.T("requests.stage.downloading", ("percent", percent));
            case StagePill.Importing:
                return Strings.T("requests.stage.importing");
            case StagePill.Partial p:
                return Strings.T("requests.stage.partial", ("seasons", Seasons(p.Seasons)));
            case StagePill.Available:
                return Strings.T("requests.stage.available");
            case StagePill.Gone:
                return Strings.T("requests.stage.gone");
            case StagePill.Declined:
                return Strings.T("requests.stage.declined");
            case StagePill.CouldNotSend c:
                return c.Reason is { } reason && !string.IsNullOrWhiteSpace(reason)
                    ? Strings.T("requests.stage.couldNotSendBecause", ("reason", reason))
                    : Strings.T("requests.stage.couldNotSend");
            case StagePill.NotSentYet:
                return Strings.T("requests.stage.notSent");
            default:
                return Strings.T("requests.stage.requested");
        }
    }

    /// <summary>The line under a failed request's pill: when the server tries again, or that an administrator will look.</summary>
    public string? PillLine(StagePill pill) => pill switch
    {
        StagePill.NotSentYet n => n.RetryAt is { } at ? Strings.T("requests.stage.retrying", ("when", Until(at))) : Strings.T("requests.stage.notSentLine"),
        StagePill.CouldNotSend c => c.RetryAt is { } at ? Strings.T("requests.stage.retrying", ("when", Until(at))) : null,
        _ => null,
    };

    /// <summary>Who asked and when: "Ann · 3 days ago", "Asked for by someone else · 3 days ago", or just when.</summary>
    public string AskedBy(MediaRequest request, bool manage)
    {
        var at = Ago(request.CreatedAt);
        if (request.Mine && !manage) return at;
        return $"{request.RequestedBy?.Name ?? Strings.T("requests.asked.someone")} · {at}";
    }

    public static string Watchers(int count) => Strings.Count("requests.asked.watchers", count);

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
        var allForNow = Strings.T("requests.allowance.allForNow");
        string AllMovies() => moviesAt != null ? Strings.T("requests.allowance.allMovies", ("when", moviesAt)) : allForNow;
        string AllSeasons() => seasonsAt != null ? Strings.T("requests.allowance.allSeasons", ("when", seasonsAt)) : allForNow;
        if (moviesOut && seasonsOut)
            return new[] { moviesAt != null && seasonsAt != null ? Strings.T("requests.allowance.allBoth", ("movies", moviesAt), ("seasons", seasonsAt)) : allForNow };
        if (moviesOut && !shows) return new[] { AllMovies() };
        if (seasonsOut && !movies) return new[] { AllSeasons() };
        var lines = new List<string>();
        if (moviesOut) lines.Add(AllMovies());
        if (seasonsOut) lines.Add(AllSeasons());
        // Movies first, then seasons, in both lines.
        var left = new List<string>();
        if (movies && allowance.MoviesLeft is > 0 and var ml) left.Add(Strings.Count("requests.allowance.moreMovies", ml));
        if (shows && allowance.SeasonsLeft is > 0 and var sl) left.Add(Strings.Count("requests.allowance.moreSeasons", sl));
        if (left.Count == 2) lines.Add(Strings.T("requests.allowance.canAskBoth", ("movies", left[0]), ("seasons", left[1])));
        else if (left.Count == 1) lines.Add(Strings.T("requests.allowance.canAsk", ("what", left[0])));
        var limits = new List<string>();
        if (movies && allowance.MoviesLimit is { } mlim) limits.Add(Strings.Count("requests.counts.movies", mlim));
        if (shows && allowance.SeasonsLimit is { } slim) limits.Add(SeasonCount(slim));
        if (limits.Count == 2) lines.Add(Strings.T("requests.allowance.limits", ("movies", limits[0]), ("seasons", limits[1])));
        else if (limits.Count == 1) lines.Add(Strings.T("requests.allowance.limit", ("what", limits[0])));
        return lines;
    }

    // ---- asking

    /// <summary>What asking for <paramref name="title"/> did: asked (waiting, approved, or not sent yet), or followed.</summary>
    public static string Asked(Asked asked, string title)
    {
        if (!asked.Created) return Strings.T("requests.asked.following", ("title", title));
        return asked.Request.Status switch
        {
            RequestStatus.Approved or RequestStatus.Available => Strings.T("requests.asked.approved", ("title", title)),
            RequestStatus.Failed => Strings.T("requests.asked.failed", ("title", title)),
            _ => Strings.T("requests.asked.pending", ("title", title)),
        };
    }

    /// <summary>"Request 3 seasons", or "Pick a season." with none picked.</summary>
    public static string PickerButton(SeasonPicker picker) =>
        picker.Count == 0 ? PickerNone : Strings.Count("requests.picker.request", picker.Count);

    // ---- actions

    public static string Action(CardAction action) => action switch
    {
        CardAction.Withdraw => Strings.T("requests.actions.withdraw"),
        CardAction.Remove => Strings.T("requests.actions.remove"),
        CardAction.StopFollowing => StopFollowing,
        CardAction.Approve => Strings.T("requests.actions.approve"),
        CardAction.ApproveWith => Strings.T("requests.actions.approveWith"),
        CardAction.TryAgain => Strings.T("requests.actions.tryAgain"),
        CardAction.Decline => Strings.T("requests.actions.decline"),
        CardAction.DeclineWithReason => Strings.T("requests.actions.declineWithReason"),
        CardAction.Handled => Strings.T("requests.actions.handled"),
        CardAction.Delete => Strings.T("requests.actions.delete"),
        _ => Strings.T("requests.actions.askAgain"),
    };

    /// <summary>The question <paramref name="action"/> asks first, for <paramref name="title"/>; null when it asks none.</summary>
    public static string? Confirm(CardAction action, string title) => action switch
    {
        CardAction.Withdraw => Strings.T("requests.list.confirmWithdraw", ("title", title)),
        CardAction.Remove => Strings.T("requests.list.confirmRemove", ("title", title)),
        CardAction.Delete => Strings.T("requests.list.confirmDelete"),
        _ => null,
    };

    /// <summary>The question over a decline's reason: "Decline “Dune”?".</summary>
    public static string DeclineTitle(string title) => Strings.T("requests.list.declineTitle", ("title", title));

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
        var need = Strings.Count("requests.notices.needYou", entry.Badge);
        return entry.Failed <= 0 ? need : Strings.Count("requests.notices.notSent", entry.Failed, ("need", need));
    }

    static string News(int count) => Strings.Count("requests.notices.news", count);

    public static string Notice(RequestNotice notice)
    {
        switch (notice)
        {
            case RequestNotice.Ready ready:
            {
                var seasons = ready.Seasons ?? Array.Empty<int>();
                if (ready.Complete || seasons.Count == 0) return Strings.T("requests.notices.ready", ("title", ready.Title));
                return Strings.Count("requests.notices.seasonsReady", seasons.Count, ("seasons", Seasons(seasons)), ("title", ready.Title));
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
        RequestProblem.Off => Strings.T("requests.problem.off"),
        RequestProblem.NoAccess => Strings.T("requests.problem.noAccess"),
        RequestProblem.SignInAgain s => s.Scope == RequestScope.Approvals ? Strings.T("requests.problem.signInApprovals") : Strings.T("requests.problem.signIn"),
        RequestProblem.TmdbKey => Strings.T("requests.problem.tmdbKey"),
        RequestProblem.TmdbDown => Strings.T("requests.problem.tmdbDown"),
        RequestProblem.Restricted r => r.Text,
        RequestProblem.Declined d => d.RetryAt is { } at ? Strings.T("requests.problem.askAgain", ("when", Until(at))) : Strings.T("requests.declined.plain"),
        RequestProblem.Limit l => l.RetryAt is { } at ? Strings.T("requests.problem.askMore", ("when", Until(at))) : AtMost(summary),
        RequestProblem.Conflict c => c.Message,
        RequestProblem.NotFound => NotFound,
        RequestProblem.Offline o => Errors.Message(o.Error),
        RequestProblem.Other o => Errors.Message(o.Error),
        _ => "",
    };

    /// <summary>The line under <paramref name="problem"/>'s, where it has one.</summary>
    public static string? ProblemBody(RequestProblem problem, RequestSummary? summary) => problem switch
    {
        RequestProblem.Off => Strings.T("requests.problem.offBody"),
        RequestProblem.NoAccess => Strings.T("requests.problem.noAccessBody"),
        RequestProblem.TmdbKey => summary?.Manage == true ? Strings.T("requests.problem.tmdbKeyManage") : Strings.T("requests.problem.tmdbKeyBody"),
        RequestProblem.TmdbDown => Strings.T("requests.problem.tmdbDownBody"),
        RequestProblem.NotFound => NotFoundBody,
        _ => null,
    };

    /// <summary>A title TMDB doesn't have (any more): its page says so.</summary>
    public static string NotFound => Strings.T("requests.problem.notFound");

    public static string NotFoundBody => Strings.T("requests.problem.notFoundBody");

    /// <summary>Sign-in lacks the approvals scope: the managers' list says so beside Sign in.</summary>
    public static string SignInToDecide => Strings.T("requests.problem.signInApprovals");

    /// <summary>429 with no time: more was asked than the limit allows at all.</summary>
    static string AtMost(RequestSummary? summary)
    {
        if (summary?.SeasonsLimit is { } seasons) return Strings.Count("requests.problem.atMostSeasons", seasons);
        if (summary?.MoviesLimit is { } movies) return Strings.Count("requests.problem.atMostMovies", movies);
        return Strings.T("requests.allowance.allForNow");
    }

    // ---- times

    /// <summary>How long until <paramref name="ts"/>: "in 5 min", "in 3 hours", "tomorrow", "in 12 days"; "in a moment" once it's past.</summary>
    public string Until(long ts)
    {
        var diff = ts - now();
        if (diff <= 0) return Strings.T("requests.time.moment");
        if (diff < Hour)
        {
            var minutes = Math.Max(1, (int)Math.Ceiling(diff / 60.0));
            return Strings.T("requests.time.minutes", ("minutes", minutes));
        }
        if (diff < Day)
        {
            // Kotlin's roundToInt: halves go up.
            var hours = Math.Max(1, Math.Min(23, (int)Math.Floor(diff / (double)Hour + 0.5)));
            return Strings.Count("requests.time.hours", hours);
        }
        var days = Math.Max(1, (int)(LocalDate(ts) - LocalDate(now())).TotalDays);
        return days == 1 ? Strings.T("requests.time.tomorrow") : Strings.Count("requests.time.days", days);
    }

    /// <summary>How long ago <paramref name="ts"/> was: "3 days ago", "yesterday".</summary>
    public string Ago(long ts) => Format.Relative(ts, now());

    /// <summary>
    /// A day, "Nov 4" ("Nov 4, 2027" in another year), with the time within 24 hours: "today, 9:00 PM".
    /// In French and Spanish the day comes first, "4 nov.", and the time is the culture's, "21:00".
    /// </summary>
    public string DayOf(long ts)
    {
        var at = Local(ts);
        var diff = ts - now();
        var culture = Culture;
        var english = ReferenceEquals(culture, English);
        if (diff >= 0 && diff < Day)
        {
            var time = at.ToString(english ? "h:mm tt" : "t", culture);
            return at.Date == LocalDate(now()) ? Strings.T("requests.time.todayAt", ("time", time)) : Strings.T("requests.time.tomorrowAt", ("time", time));
        }
        var thisYear = at.Year == LocalDate(now()).Year;
        if (english) return at.ToString(thisYear ? "MMM d" : "MMM d, yyyy", culture);
        return at.ToString(thisYear ? "d MMM" : "d MMM yyyy", culture);
    }

    DateTime Local(long ts) => TimeZoneInfo.ConvertTime(DateTimeOffset.FromUnixTimeSeconds(ts), zone).DateTime;

    DateTime LocalDate(long ts) => Local(ts).Date;
}
