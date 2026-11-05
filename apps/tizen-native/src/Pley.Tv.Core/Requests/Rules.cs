using System;
using System.Collections.Generic;
using System.Linq;
using Pley.Tv.Core.Ui;

namespace Pley.Tv.Core.Requests;

// Requests' rules, as every client keeps them: pure functions of what the server answered (the
// Android app's RequestRules.kt). Their words are RequestText's.

/// <summary>A title's badge on its poster.</summary>
public enum TitleBadge
{
    InLibrary,
    PartlyInLibrary,
    Requested,
    Declined,
}

/// <summary>A pill's colour: pending amber, on its way blue, ready green, refused red, gone gray.</summary>
public enum PillTone
{
    Amber,
    Blue,
    Green,
    Red,
    Gray,
}

/// <summary>Where a request is, as its pill says it.</summary>
public abstract record StagePill(PillTone Tone)
{
    public sealed record Pending() : StagePill(PillTone.Amber);
    public sealed record Approved() : StagePill(PillTone.Blue);
    /// <summary>Not out yet: out at <paramref name="At"/> (a movie), airing at it (a show), or when isn't known.</summary>
    public sealed record Waiting(bool Show, long? At) : StagePill(PillTone.Blue);
    public sealed record Searching() : StagePill(PillTone.Blue);
    /// <summary><paramref name="Progress"/> 0-1, done at <paramref name="Eta"/>; <paramref name="State"/> says when it waits, is paused or has a problem.</summary>
    public sealed record Downloading(float Progress, long? Eta, string State) : StagePill(PillTone.Blue);
    public sealed record Importing() : StagePill(PillTone.Blue);
    /// <summary>Shows: <paramref name="Seasons"/> are ready.</summary>
    public sealed record Partial(IReadOnlyList<int> Seasons) : StagePill(PillTone.Green);
    public sealed record Available() : StagePill(PillTone.Green);
    public sealed record Gone() : StagePill(PillTone.Gray);
    public sealed record Declined() : StagePill(PillTone.Red);
    /// <summary>Failed, as managers see it: what went wrong, and when the server tries again, if it does.</summary>
    public sealed record CouldNotSend(string? Reason, long? RetryAt) : StagePill(PillTone.Red);
    /// <summary>Failed, as everyone else sees it: the server tries again at <paramref name="RetryAt"/>, or leaves it to an administrator.</summary>
    public sealed record NotSentYet(long? RetryAt) : StagePill(PillTone.Red);
    /// <summary>A stage this app doesn't know.</summary>
    public sealed record Unknown() : StagePill(PillTone.Gray);
}

/// <summary>How much this account may still ask for in the last 7 days, and when more. Nulls: no limit.</summary>
public sealed record Allowance(int? MoviesLeft, int? SeasonsLeft, int? MoviesLimit, int? SeasonsLimit, long? MoviesNextAt, long? SeasonsNextAt)
{
    /// <summary>Some limit applies: the allowance line has something to say.</summary>
    public bool Limited => MoviesLeft != null || SeasonsLeft != null;

    /// <summary>Whether one more of <paramref name="kind"/> may be asked for.</summary>
    public bool Allows(string kind) => kind switch
    {
        TitleKind.Movie => MoviesLeft == null || MoviesLeft > 0,
        TitleKind.Show => SeasonsLeft == null || SeasonsLeft > 0,
        _ => true,
    };

    /// <summary>What's left of <paramref name="kind"/>: movies, or seasons; null: no limit.</summary>
    public int? Left(string kind) => kind == TitleKind.Show ? SeasonsLeft : MoviesLeft;

    public static Allowance Of(RequestSummary summary) =>
        new Allowance(summary.MoviesLeft, summary.SeasonsLeft, summary.MoviesLimit, summary.SeasonsLimit, summary.MoviesNextAt, summary.SeasonsNextAt);

    /// <summary>A title page's, which the server sends with it; the limits from <paramref name="summary"/>.</summary>
    public static Allowance Of(DiscoverDetail detail, RequestSummary? summary) =>
        new Allowance(detail.MoviesLeft, detail.SeasonsLeft, summary?.MoviesLimit, summary?.SeasonsLimit, detail.MoviesNextAt, detail.SeasonsNextAt);
}

/// <summary>Whether asking works for a title now.</summary>
public enum Ask
{
    Yes,
    /// <summary>The account may not ask: "Requests aren't open to this account."</summary>
    NoAccess,
    /// <summary>Nothing left to ask for: the button is disabled, and the allowance line shows.</summary>
    Limit,
}

/// <summary>A title page's one main action, the first that applies.</summary>
public abstract record MainAction
{
    /// <summary>This sign-in came before requests: only "Sign in again".</summary>
    public sealed record SignInAgain : MainAction;
    /// <summary>Above the account's age limit: no button, the restriction's text.</summary>
    public sealed record Restricted(string? Text) : MainAction;
    /// <summary>In a library: Open, where <paramref name="ItemId"/> is known.</summary>
    public sealed record Open(long? ItemId) : MainAction;
    /// <summary>No button: "Declined: “{reason}”. You can ask again in 12 days."</summary>
    public sealed record Declined(string? Reason, long? AgainAt) : MainAction;
    /// <summary>Requested, inert, with <paramref name="Active"/>'s pill.</summary>
    public sealed record Mine(MediaRequest? Active) : MainAction;
    /// <summary>Following, inert, with <paramref name="Active"/>'s pill, and Stop following.</summary>
    public sealed record Following(MediaRequest? Active) : MainAction;
    /// <summary>Someone asked for it already: Tell me too, which costs nothing.</summary>
    public sealed record TellMeToo(Ask Ask) : MainAction;
    /// <summary>Some of it is here or asked for: Open (when <paramref name="ItemId"/> is set), and Request more seasons unless <paramref name="More"/> is null.</summary>
    public sealed record Partial(long? ItemId, Ask? More) : MainAction;
    public sealed record Request(Ask Ask) : MainAction;
}

/// <summary>One season in the picker: tickable when <paramref name="Askable"/>.</summary>
public sealed record PickerSeason(DiscoverSeason Season, bool Askable);

/// <summary>
/// The season picker ("Which seasons?"): every season, specials last; <see cref="Picked"/> the
/// ones ticked. More than <see cref="Left"/> ticked disables the button and shows the allowance
/// line. <see cref="Start"/> is the season it was opened at (a season's poster), where focus starts.
/// </summary>
public sealed record SeasonPicker(IReadOnlyList<PickerSeason> Seasons, IReadOnlyCollection<int> Picked, int? Left, int? Start = null)
{
    public int Count => Picked.Count;

    /// <summary>More ticked than may be asked for.</summary>
    public bool OverLimit => Left != null && Count > Left;

    /// <summary>Request can be pressed.</summary>
    public bool Ready => Count > 0 && !OverLimit;

    /// <summary><paramref name="number"/> ticked or unticked; seasons that can't be asked for stay as they are.</summary>
    public SeasonPicker Toggle(int number)
    {
        if (!Seasons.Any(s => s.Season.Number == number && s.Askable)) return this;
        var picked = new HashSet<int>(Picked);
        if (!picked.Remove(number)) picked.Add(number);
        return this with { Picked = picked };
    }

    /// <summary>What was picked, lowest first, for the request.</summary>
    public IReadOnlyList<int> Asked() => Picked.OrderBy(n => n).ToList();

    /// <summary>
    /// <paramref name="detail"/>'s seasons: those askable and missing or partly here (not the
    /// specials) are ticked lowest first, up to <paramref name="left"/>; unaired seasons and
    /// specials are offered unticked. Opened at <paramref name="start"/>, that season is ticked
    /// too when it may be asked for.
    /// </summary>
    public static SeasonPicker Of(DiscoverDetail detail, int? left, int? start = null)
    {
        var all = (detail.Seasons ?? Array.Empty<DiscoverSeason>()).OrderBy(s => s.Number == 0).ThenBy(s => s.Number);
        var rows = all.Select(s => new PickerSeason(s, RequestRules.Askable(s, detail))).ToList();
        var wanted = rows.Where(r => r.Askable && RequestRules.Wanted(r.Season)).Select(r => r.Season.Number).ToList();
        var picked = new HashSet<int>(left == null ? wanted : wanted.Take(Math.Max(0, left.Value)));
        if (start is { } n && rows.Any(r => r.Askable && r.Season.Number == n)) picked.Add(n);
        return new SeasonPicker(rows, picked, left, start);
    }
}

/// <summary>
/// What a request card offers: exactly the server's actions, in its order, those this app
/// doesn't know left out. Where dialogs can be shown, Approve comes with Approve… and Decline asks
/// for an optional reason in its dialog; on a TV, Decline comes with "Decline with a reason…".
/// </summary>
public enum CardAction
{
    Withdraw,
    Remove,
    StopFollowing,
    Approve,
    ApproveWith,
    TryAgain,
    Decline,
    DeclineWithReason,
    Handled,
    Delete,
    AskAgain,
}

/// <summary>Requests' rules (the Android app's RequestRules.kt).</summary>
public static class RequestRules
{
    /// <summary>The longest reason a decline takes (the server's limit): a longer one keeps its first characters.</summary>
    public const int DeclineReasonMax = 500;

    /// <summary>The longest search the server takes, in characters: it refuses longer ones.</summary>
    public const int SearchMax = 200;

    // ---- badges

    /// <summary><paramref name="title"/>'s badge: none for titles that may be asked for, or can't be.</summary>
    public static TitleBadge? BadgeOf(DiscoverTitle title) => BadgeOf(title.Status, title.ItemId);

    public static TitleBadge? BadgeOf(string status, long? itemId) => status switch
    {
        TitleStatus.Library => TitleBadge.InLibrary,
        TitleStatus.Partial => itemId != null ? TitleBadge.PartlyInLibrary : TitleBadge.Requested,
        TitleStatus.Requested => TitleBadge.Requested,
        TitleStatus.Declined => TitleBadge.Declined,
        _ => null,
    };

    /// <summary>
    /// <paramref name="title"/> as one of its requests now says it is (requests_changed, in rows
    /// already shown): active → requested, declined → declined, available → in the library, gone →
    /// no badge. A request no longer there (null, a 404: no longer followed, withdrawn while others
    /// follow it, deleted) says nothing of the title, which others may still ask for or a library
    /// have: only the request is let go of.
    /// </summary>
    public static DiscoverTitle Patched(DiscoverTitle title, MediaRequest? request)
    {
        if (request == null) return title with { RequestId = null };
        if (request.Stage == RequestStage.Gone) return title with { Status = TitleStatus.None, RequestId = null };
        if (request.Status == RequestStatus.Available) return title with { Status = TitleStatus.Library, ItemId = request.ItemId ?? title.ItemId };
        if (request.Status == RequestStatus.Declined) return title with { Status = TitleStatus.Declined, RequestId = null };
        if (request.Active) return title with { Status = TitleStatus.Requested, RequestId = request.Id };
        return title;
    }

    // ---- where a title opens

    /// <summary>
    /// Where <paramref name="title"/>'s poster opens: the library's title when a library this account
    /// can open has it whole, since its Discover page would offer only Open; else its Discover page.
    /// A show partly there opens its Discover page, where more seasons can be asked for.
    /// </summary>
    public static PageKey Opens(DiscoverTitle title) =>
        title.Status == TitleStatus.Library && title.ItemId is { } id
            ? new PageKey.Item(id)
            : new PageKey.DiscoverTitle(title.Kind, title.TmdbId);

    // ---- stage pills

    /// <summary><paramref name="request"/>'s pill, as a manager (<paramref name="manage"/>) or anyone else sees it.</summary>
    public static StagePill Pill(MediaRequest request, bool manage) => request.Stage switch
    {
        RequestStage.Pending => new StagePill.Pending(),
        RequestStage.Approved => new StagePill.Approved(),
        RequestStage.Waiting => new StagePill.Waiting(request.Kind == TitleKind.Show, request.StageAt),
        RequestStage.Searching => new StagePill.Searching(),
        RequestStage.Downloading => new StagePill.Downloading(DownloadProgress(request) ?? 0f, request.Download?.Eta, request.Download?.State ?? DownloadState.Downloading),
        RequestStage.Importing => new StagePill.Importing(),
        RequestStage.Partial => new StagePill.Partial(request.AvailableSeasons),
        RequestStage.Available => new StagePill.Available(),
        RequestStage.Gone => new StagePill.Gone(),
        RequestStage.Declined => new StagePill.Declined(),
        RequestStage.Failed => manage ? new StagePill.CouldNotSend(request.Reason, request.RetryAt) : new StagePill.NotSentYet(request.RetryAt),
        _ => new StagePill.Unknown(),
    };

    /// <summary>How far <paramref name="request"/>'s download is, 0-1, while there is one: the bar under its pill (a partial one's too).</summary>
    public static float? DownloadProgress(MediaRequest request) =>
        request.Download is { } d ? (float)Math.Max(0, Math.Min(1, d.Progress)) : null;

    /// <summary>Ready (some or all of it) in a library this account can open: Watch opens it there.</summary>
    public static bool ReadyToWatch(MediaRequest request) =>
        request.ItemId != null && request.Stage is RequestStage.Available or RequestStage.Partial;

    /// <summary>Ready for someone who follows it, but in no library they can open: "ask an administrator" instead of Watch.</summary>
    public static bool ReadyElsewhere(MediaRequest request) =>
        request.ItemId == null && request.Stage is RequestStage.Available or RequestStage.Partial;

    // ---- the main action

    /// <summary>Whether asking works for <paramref name="kind"/> under <paramref name="state"/>, with <paramref name="allowance"/> left.</summary>
    public static Ask AskFor(string kind, RequestsState state, Allowance? allowance)
    {
        if (!state.MayAsk || state.Summary?.Access == RequestAccess.Off) return Ask.NoAccess;
        if (allowance != null && !allowance.Allows(kind)) return Ask.Limit;
        return Ask.Yes;
    }

    /// <summary><paramref name="detail"/>'s main action, for what <paramref name="state"/> lets this sign-in do.</summary>
    public static MainAction Main(DiscoverDetail detail, RequestsState state)
    {
        if (state is RequestsState.SignInAgain) return new MainAction.SignInAgain();
        var ask = AskFor(detail.Kind, state, Allowance.Of(detail, state.Summary));
        switch (detail.Status)
        {
            case TitleStatus.Restricted:
                return new MainAction.Restricted(detail.Restriction);
            case TitleStatus.Library:
                return new MainAction.Open(detail.ItemId);
            case TitleStatus.Declined:
                return new MainAction.Declined(detail.Declined?.Reason, detail.Declined?.AgainAt is > 0 ? detail.Declined.AgainAt : null);
            case TitleStatus.Requested:
            {
                var active = ActiveRequest(detail);
                if (active?.Mine == true) return new MainAction.Mine(active);
                if (detail.Watching || active?.Watching == true) return new MainAction.Following(active);
                // Following costs nothing: only access counts.
                return new MainAction.TellMeToo(ask == Ask.NoAccess ? Ask.NoAccess : Ask.Yes);
            }
            case TitleStatus.Partial:
                return new MainAction.Partial(detail.ItemId, MoreSeasons(detail) ? ask : null);
            default:
                return new MainAction.Request(ask);
        }
    }

    /// <summary>
    /// A title's main action from what a row knows of it (the hero's, before its page is read): a
    /// request it has is taken as this account's.
    /// </summary>
    public static MainAction OfTitle(DiscoverTitle title, RequestsState state, Allowance? allowance)
    {
        if (state is RequestsState.SignInAgain) return new MainAction.SignInAgain();
        var ask = AskFor(title.Kind, state, allowance);
        return title.Status switch
        {
            TitleStatus.Restricted => new MainAction.Restricted(null),
            TitleStatus.Library => new MainAction.Open(title.ItemId),
            TitleStatus.Declined => new MainAction.Declined(null, null),
            TitleStatus.Requested => title.RequestId != null ? new MainAction.Mine(null) : new MainAction.TellMeToo(ask == Ask.NoAccess ? Ask.NoAccess : Ask.Yes),
            TitleStatus.Partial => new MainAction.Partial(title.ItemId, ask),
            _ => new MainAction.Request(ask),
        };
    }

    /// <summary>The active request <paramref name="detail"/> names (its <c>request_id</c>), else the newest active one this account sees.</summary>
    public static MediaRequest? ActiveRequest(DiscoverDetail detail) =>
        detail.Requests.FirstOrDefault(r => r.Id == detail.RequestId) ?? detail.Requests.FirstOrDefault(r => r.Active);

    /// <summary>The request <paramref name="season"/> shows the stage of: the one asking for it, among the page's.</summary>
    public static MediaRequest? RequestOf(DiscoverSeason season, DiscoverDetail detail) =>
        season.RequestId is { } id ? detail.Requests.FirstOrDefault(r => r.Id == id) : null;

    /// <summary>Tell me too: the movie, or the seasons others asked for, which costs nothing.</summary>
    public static NewRequest TellMeToo(DiscoverDetail detail)
    {
        if (detail.Kind != TitleKind.Show) return new NewRequest(detail.Kind, detail.TmdbId);
        var asked = (detail.Seasons ?? Array.Empty<DiscoverSeason>()).Where(s => s.State == SeasonState.Requested).Select(s => s.Number).ToList();
        return new NewRequest(detail.Kind, detail.TmdbId, asked.Count > 0 ? asked : null);
    }

    /// <summary>Asking again for <paramref name="request"/> as it was (Request again).</summary>
    public static NewRequest AskAgain(MediaRequest request) =>
        new NewRequest(request.Kind, request.TmdbId, request.Kind == TitleKind.Show && request.Seasons is { Count: > 0 } s ? s : null);

    /// <summary>
    /// Where a decline's <paramref name="reason"/> ends once cut to <see cref="DeclineReasonMax"/>
    /// characters: its length when it fits, else after that many, or one before when the last
    /// would be the first half of a pair (an emoji), which alone would reach the server as "?".
    /// </summary>
    public static int DeclineReasonEnd(string reason)
    {
        if (reason.Length <= DeclineReasonMax) return reason.Length;
        return char.IsHighSurrogate(reason[DeclineReasonMax - 1]) ? DeclineReasonMax - 1 : DeclineReasonMax;
    }

    /// <summary>
    /// <paramref name="query"/> as the server takes it: trimmed, and cut to <see cref="SearchMax"/>
    /// characters, never halfway through one.
    /// </summary>
    public static string SearchText(string query)
    {
        var trimmed = query.Trim();
        // Code points, as the server counts them: a pair is one.
        var points = 0;
        for (var i = 0; i < trimmed.Length; i++)
        {
            if (points == SearchMax) return trimmed.Substring(0, i).Trim();
            if (char.IsHighSurrogate(trimmed[i]) && i + 1 < trimmed.Length && char.IsLowSurrogate(trimmed[i + 1])) i++;
            points++;
        }
        return trimmed;
    }

    // ---- seasons

    /// <summary>Whether <paramref name="season"/> may be asked for: missing, partly in a library or not aired, not declined, the title not restricted.</summary>
    public static bool Askable(DiscoverSeason season, DiscoverDetail detail) =>
        season.State is SeasonState.Missing or SeasonState.Partial or SeasonState.Unaired && !season.Declined && detail.Status != TitleStatus.Restricted;

    /// <summary>Missing or partly here, and not the specials: what the picker ticks first.</summary>
    internal static bool Wanted(DiscoverSeason season) =>
        season.Number != 0 && season.State is SeasonState.Missing or SeasonState.Partial;

    /// <summary>"Request more seasons" shows: a season other than the specials is askable and missing or partly here.</summary>
    public static bool MoreSeasons(DiscoverDetail detail) =>
        (detail.Seasons ?? Array.Empty<DiscoverSeason>()).Any(s => Wanted(s) && Askable(s, detail));

    /// <summary>
    /// Season numbers as ranges: [1, 2, 3, 5] → "1–3, 5" (an en dash), in order, each once. The
    /// specials (0) aren't among them: they're said apart.
    /// </summary>
    public static string SeasonRanges(IEnumerable<int> numbers)
    {
        var sorted = numbers.Where(n => n > 0).Distinct().OrderBy(n => n).ToList();
        if (sorted.Count == 0) return "";
        var parts = new List<string>();
        int start = sorted[0], end = start;
        foreach (var n in sorted.Skip(1))
        {
            if (n == end + 1)
            {
                end = n;
                continue;
            }
            parts.Add(Range(start, end));
            start = end = n;
        }
        parts.Add(Range(start, end));
        return string.Join(", ", parts);
    }

    static string Range(int start, int end) => start == end ? $"{start}" : $"{start}–{end}";

    // ---- actions

    /// <summary>
    /// What a request card offers: exactly the server's actions, in its order, those this app
    /// doesn't know left out; with <paramref name="dialogs"/>, Approve… beside Approve, else
    /// "Decline with a reason…" beside Decline.
    /// </summary>
    public static IReadOnlyList<CardAction> CardActions(MediaRequest request, bool dialogs)
    {
        var actions = new List<CardAction>();
        foreach (var action in request.Actions.Distinct())
        {
            switch (action)
            {
                case RequestAction.Withdraw:
                    actions.Add(request.Status == RequestStatus.Declined ? CardAction.Remove : CardAction.Withdraw);
                    break;
                case RequestAction.Unwatch:
                    actions.Add(CardAction.StopFollowing);
                    break;
                case RequestAction.Approve:
                    actions.Add(CardAction.Approve);
                    if (dialogs) actions.Add(CardAction.ApproveWith);
                    break;
                case RequestAction.Retry:
                    actions.Add(CardAction.TryAgain);
                    break;
                case RequestAction.Decline:
                    actions.Add(CardAction.Decline);
                    if (!dialogs) actions.Add(CardAction.DeclineWithReason);
                    break;
                case RequestAction.Handled:
                    actions.Add(CardAction.Handled);
                    break;
                case RequestAction.Delete:
                    actions.Add(CardAction.Delete);
                    break;
                case RequestAction.AskAgain:
                    actions.Add(CardAction.AskAgain);
                    break;
            }
        }
        return actions;
    }

    /// <summary>Whether <paramref name="action"/> asks first: "Withdraw your request for “X”?", "Remove “X” from your requests?", "Delete this request?".</summary>
    public static bool Confirms(this CardAction action) => action is CardAction.Withdraw or CardAction.Remove or CardAction.Delete;

    /// <summary>Whether <paramref name="action"/> decides for everyone: a manager's, shown after the person's own.</summary>
    public static bool Managing(this CardAction action) =>
        action is CardAction.Approve or CardAction.ApproveWith or CardAction.TryAgain or CardAction.Decline or CardAction.DeclineWithReason or CardAction.Handled or CardAction.Delete;
}
