using System;
using System.Collections.Generic;
using System.Globalization;
using System.Linq;
using System.Text.Json;
using System.Threading.Tasks;
using Pley.Tv.Core.Api;
using Pley.Tv.Core.Auth;

namespace Pley.Tv.Core.Requests;

/// <summary>
/// Requests and Discover for the profile in use (docs/API.md, "Requests"; the Android app's
/// RequestsRepository): the app's, or a fake in tests. It lives on the UI thread: its signals
/// change there, and its tasks' continuations come back there.
/// </summary>
public interface IRequestsSource
{
    /// <summary>What shows: it follows the summary.</summary>
    Signal<RequestsState> State { get; }

    /// <summary>What to tell the person, oldest first; at most <see cref="AppRequests.Kept"/>.</summary>
    Signal<IReadOnlyList<RequestNotice>> Notices { get; }

    /// <summary>The server's events are connected: without them, open lists are read again every 30 s.</summary>
    Signal<bool> Streaming { get; }

    /// <summary>Requests that changed, as they are heard; nothing is replayed.</summary>
    event Action<RequestChange>? Changed;

    /// <summary>The server's origin, which pictures are made whole against.</summary>
    string Origin { get; }

    /// <summary>Starts following the server's request events, polling while they can't come, and catching up on news, once; started, reads the summary again.</summary>
    void Start();

    /// <summary>Reads the summary again now: a Retry, a sign-in done.</summary>
    void Refresh();

    /// <summary>The first page of <paramref name="list"/> as last read on this TV, to show before the server answers.</summary>
    DiscoverPage? SavedRow(DiscoverList list);

    /// <summary>Page <paramref name="page"/> of <paramref name="list"/>, from the server; the first is kept for <see cref="SavedRow"/>.</summary>
    Task<DiscoverPage> Page(DiscoverList list, int page);

    /// <summary>TMDB's titles matching <paramref name="query"/> (as <see cref="RequestRules.SearchText"/> makes it); <paramref name="missing"/> leaves out those a library has.</summary>
    Task<DiscoverPage> Search(string query, int page = 1, bool missing = false);

    /// <summary>A title page as last read here, held in memory while it is used.</summary>
    DiscoverDetail? CachedTitle(string kind, long tmdbId);

    /// <summary><paramref name="kind"/> <paramref name="tmdbId"/>'s page from the server now, kept for <see cref="CachedTitle"/>.</summary>
    Task<DiscoverDetail> Title(string kind, long tmdbId);

    /// <summary>A library title as Discover sees it; null when it has no TMDB match.</summary>
    Task<ItemRequest?> ItemRequest(long itemId);

    Task<RequestList> List(RequestQuery query);

    /// <summary>One request; null when it is gone (or never was this account's to see).</summary>
    Task<MediaRequest?> Request(long id);

    /// <summary>Asks for <paramref name="request"/> (or follows the requests that ask for it already).</summary>
    Task<Asked> Ask(NewRequest request);

    /// <summary>Withdraws this account's <paramref name="request"/> (Remove, on a declined one); managers delete anyone's.</summary>
    Task Delete(MediaRequest request);

    /// <summary>Stops following someone else's <paramref name="request"/>.</summary>
    Task Unwatch(MediaRequest request);

    Task<MediaRequest> Approve(long id, Approval approval);

    Task<MediaRequest> Decline(long id, string? reason);

    /// <summary>Approves without sending: added by hand.</summary>
    Task<MediaRequest> Handled(long id);

    /// <summary>The person has looked at their requests: their news is seen, and the badge clears.</summary>
    Task Seen();

    /// <summary><paramref name="notice"/> was shown (or is no longer wanted).</summary>
    void Dismiss(RequestNotice notice);
}

/// <summary>What a request action left.</summary>
public abstract record Outcome
{
    /// <summary>The request as it is now.</summary>
    public sealed record Changed(MediaRequest Request) : Outcome;

    /// <summary>It's no longer this account's: withdrawn, removed, deleted, or no longer followed.</summary>
    public sealed record Gone : Outcome;

    /// <summary>Asked for again: a new request (or one now followed).</summary>
    public sealed record AskedAgain(Asked Asked) : Outcome;
}

public static class RequestsSources
{
    /// <summary><paramref name="next"/> once <paramref name="before"/> has ended, however it ended: changes applied in the order they came.</summary>
    public static async Task After(Task before, Func<Task> next)
    {
        try
        {
            await before;
        }
        catch (Exception)
        {
            // Its own failure was dealt with where it ran.
        }
        await next();
    }

    /// <summary>Does <paramref name="action"/> to <paramref name="request"/> (<paramref name="reason"/> for Decline).</summary>
    public static async Task<Outcome> Perform(this IRequestsSource source, MediaRequest request, CardAction action, string? reason = null)
    {
        switch (action)
        {
            case CardAction.Withdraw:
            case CardAction.Remove:
            case CardAction.Delete:
                await source.Delete(request);
                return new Outcome.Gone();
            case CardAction.StopFollowing:
                await source.Unwatch(request);
                return new Outcome.Gone();
            case CardAction.Decline:
            case CardAction.DeclineWithReason:
                return new Outcome.Changed(await source.Decline(request.Id, reason));
            case CardAction.Handled:
                return new Outcome.Changed(await source.Handled(request.Id));
            case CardAction.AskAgain:
                return new Outcome.AskedAgain(await source.Ask(RequestRules.AskAgain(request)));
            default:
                // Approve, Approve… and Try again: the settings' choices, the seasons asked.
                return new Outcome.Changed(await source.Approve(request.Id, new Approval()));
        }
    }

    /// <summary>
    /// How titles already shown change with <paramref name="change"/> (requests_changed): their
    /// badges follow its request, read once (GET /api/requests/{id}). One this account can no
    /// longer read says nothing of its title, nor does a show's request wholly ready (the show may
    /// still lack seasons nobody asked for): the title's page is read instead, for its status (the
    /// request let go of, or followed, should that fail too). Null when none of <paramref name="shown"/>
    /// is touched, many changed at once (rows are never read again for that), or the request couldn't be read.
    /// </summary>
    public static async Task<Func<DiscoverTitle, DiscoverTitle>?> PatchFor(this IRequestsSource source, RequestChange change, IEnumerable<DiscoverTitle> shown)
    {
        if (change.RequestId is not { } id) return null;
        var touched = shown.Where(change.Touches).Select(t => (t.Kind, t.TmdbId)).Distinct().ToList();
        if (touched.Count == 0) return null;
        MediaRequest? request;
        try
        {
            request = await source.Request(id);
        }
        catch (Exception)
        {
            return null;
        }
        // A show whose request is wholly ready may still lack seasons nobody asked for: its page says
        // whether a library has it whole, which its poster opens then.
        if (request != null && !ReadyShow(request)) return title => change.Touches(title) ? RequestRules.Patched(title, request) : title;
        var pages = new Dictionary<(string, long), DiscoverDetail?>();
        foreach (var t in touched)
        {
            try
            {
                pages[t] = await source.Title(t.Kind, t.TmdbId);
            }
            catch (Exception)
            {
                pages[t] = null;
            }
        }
        return title =>
        {
            if (!change.Touches(title)) return title;
            if (pages.TryGetValue((title.Kind, title.TmdbId), out var page) && page != null)
                return title with { Status = page.Status, ItemId = page.ItemId, RequestId = page.RequestId };
            return RequestRules.Patched(title, request);
        };
    }

    static bool ReadyShow(MediaRequest request) => request.Kind == TitleKind.Show && request.Status == RequestStatus.Available;
}

/// <summary>
/// <see cref="IRequestsSource"/> for <see cref="AppState"/>'s server and profile.
///
/// <see cref="State"/> follows the summary, read once <see cref="Start"/> ran: then, after a
/// profile switch or a new sign-in, when the event stream connects, after request events
/// (gathered for a second), and every minute in the foreground while the stream is down. It is
/// kept on the TV per server and profile, so the rail has its entry before the server answers. An
/// older server (404, or a 403 <c>insufficient_scope</c> naming no scope) is taken as requests
/// being off, and asked again only when the stream connects or the app starts.
///
/// On a TV only what became ready to watch is told, as Android TV does: the pop-up takes focus
/// from the page, worth it for a title to open. The rest shows as the number and in the lists.
/// What came while the app wasn't listening is found among the 20 newest unseen requests; the
/// newest <c>updated_at</c> announced is kept per server and profile, so nothing is told twice,
/// across starts too.
/// </summary>
public sealed class AppRequests : IRequestsSource
{
    /// <summary>Notices kept while they can't show.</summary>
    public const int Kept = 3;

    /// <summary>How long request events are gathered before reading the summary again.</summary>
    public const long DebounceMs = 1_000;

    /// <summary>How often the summary is read while the event stream is down, in the foreground.</summary>
    public const long PollMs = 60_000;

    /// <summary>The most news read when catching up.</summary>
    public const int CatchUp = 20;

    /// <summary>Title pages kept in memory; the least recently read go first.</summary>
    const int TitlesKept = 20;

    const string SummaryKey = "pley.requests";
    const string NewsKey = "pley.requestNews";
    const string RowKey = "pley.discover";

    /// <summary>
    /// What is kept per server and profile, and forgotten with their Home (AppState): the summary and
    /// Discover's first pages, which hold the account's requests. The news mark stays, as on Android:
    /// it only keeps what was told from being told again.
    /// </summary>
    public static IEnumerable<string> KeptKeys => new[] { SummaryKey }.Concat(DiscoverLists.All.Select(list => $"{RowKey}.{list.Wire()}"));

    /// <summary>Requests off: what an older server's answer is taken as.</summary>
    static readonly RequestSummary OffSummary = new RequestSummary { Enabled = false };

    readonly AppState app;
    readonly IScheduler scheduler;
    /// <summary>Servers that answered as older ones do: asked again when the stream connects, or at the next start.</summary>
    readonly HashSet<string> older = new HashSet<string>();
    readonly Dictionary<(string, long), DiscoverDetail> titles = new Dictionary<(string, long), DiscoverDetail>();
    readonly LinkedList<(string, long)> titleOrder = new LinkedList<(string, long)>();
    /// <summary>Requests announced in this process, for the profile in use.</summary>
    readonly HashSet<long> announced = new HashSet<long>();
    bool started;
    long keys;
    IDisposable? soon, poll;
    /// <summary>The profile whose notices these are, and the last fresh summary it had.</summary>
    (string Server, long Profile)? owner;
    RequestSummary? last;
    /// <summary>News events heard since the last fresh summary: they explain its <c>unseen</c> rising.</summary>
    int explained;
    /// <summary>
    /// Events told news the saved mark doesn't cover yet: an event doesn't say when its request
    /// changed, so the next fresh summary catches up, which raises the mark past them.
    /// </summary>
    bool unmarked;
    /// <summary>Summaries asked for: only the last one's answer counts.</summary>
    int reads;

    public AppRequests(AppState app, IScheduler scheduler)
    {
        this.app = app;
        this.scheduler = scheduler;
    }

    public Signal<RequestsState> State { get; } = new Signal<RequestsState>(new RequestsState.Unknown());

    public Signal<IReadOnlyList<RequestNotice>> Notices { get; } = new Signal<IReadOnlyList<RequestNotice>>(Array.Empty<RequestNotice>());

    public Signal<bool> Streaming => app.Streaming;

    public event Action<RequestChange>? Changed;

    public string Origin => app.Server.Value ?? "";

    Api.Api Api => app.Api.Value ?? throw new SignedOutException();

    /// <summary>The server and profile in use, when both are known.</summary>
    (string Server, long Profile)? Profile => app.Server.Value is { } server && app.Me.Value?.Id is { } id ? (server, id) : null;

    string? Keyed(string key) => Profile is { } p ? $"{key}.{p.Server}.{p.Profile.ToString(CultureInfo.InvariantCulture)}" : null;

    public void Start()
    {
        if (started)
        {
            // Home again, after a sign-in maybe: signing in again to the same server keeps its API and,
            // its stream already open, nothing else says the summary (sign_in_again with it) is out of date.
            Refresh();
            return;
        }
        started = true;
        app.Me.Watch(_ => Switched());
        app.Api.Changed += _ => Refresh();
        app.Connected += Connected;
        app.OnEvent(Heard);
        app.Streaming.Changed += _ => Polling();
        app.Visible.Changed += _ => Polling();
        Polling();
    }

    /// <summary>A profile came (or went): what was kept for it shows, and the server is asked.</summary>
    void Switched()
    {
        var profile = Profile;
        if (profile == owner)
        {
            return;
        }
        owner = profile;
        last = null;
        explained = 0;
        announced.Clear();
        unmarked = false;
        titles.Clear();
        titleOrder.Clear();
        Notices.Value = Array.Empty<RequestNotice>();
        State.Value = RequestsState.Of(Saved<RequestSummary>(Keyed(SummaryKey)));
        if (profile != null) Refresh();
    }

    /// <summary>The stream connected: nothing is replayed, and the server may have been updated.</summary>
    void Connected()
    {
        older.Clear();
        Refresh();
        Changed?.Invoke(RequestChange.All);
    }

    /// <summary>Every minute in the foreground while the server's events can't come.</summary>
    void Polling()
    {
        var on = started && app.Visible.Value && !app.Streaming.Value;
        if (!on)
        {
            poll?.Dispose();
            poll = null;
            return;
        }
        if (poll != null) return;
        Tick();
    }

    void Tick()
    {
        poll = scheduler.After(PollMs, () =>
        {
            poll = null;
            Refresh();
            Polling();
        });
    }

    public void Refresh()
    {
        if (!started) return;
        _ = Read();
    }

    /// <summary>The summary read again in a second, however many changes come meanwhile.</summary>
    void RefreshSoon()
    {
        soon?.Dispose();
        soon = scheduler.After(DebounceMs, () =>
        {
            soon = null;
            Refresh();
        });
    }

    async Task Read()
    {
        var api = app.Api.Value;
        var profile = Profile;
        if (api == null || profile == null) return;
        var at = ++reads;
        RequestSummary summary;
        try
        {
            if (older.Contains(profile.Value.Server)) summary = OffSummary;
            else summary = await api.RequestSummary();
        }
        catch (ApiException e) when (e.Status == 404 || (e.NeedsScope && e.Scope == null))
        {
            older.Add(profile.Value.Server);
            summary = OffSummary;
        }
        catch (Exception)
        {
            // Shown where the summary is (it keeps what it had); asked again later.
            return;
        }
        if (at != reads || Profile != profile) return;
        Keep(Keyed(SummaryKey), summary);
        State.Value = RequestsState.Of(summary);
        await Fresh(summary, profile.Value);
    }

    /// <summary>A fresh summary: what it says changed is caught up on.</summary>
    async Task Fresh(RequestSummary summary, (string Server, long Profile) profile)
    {
        var before = last;
        last = summary;
        var news = explained;
        explained = 0;
        // Without events, changed_at says when what shows is to be read again.
        if (before != null && before.ChangedAt != summary.ChangedAt && !app.Streaming.Value) Changed?.Invoke(RequestChange.All);
        var rose = before == null ? summary.Unseen > 0 : summary.Unseen > before.Unseen && news == 0;
        if ((rose || unmarked) && summary.Enabled && summary.SignInAgain != RequestScope.Requests) await CatchUpOn(profile);
    }

    /// <summary>
    /// GET /api/requests?unseen=1: one notice for what of it wasn't announced yet and is ready to
    /// watch. Then every request listed has been announced (now, by an event, or before), and the
    /// saved mark is raised past them all.
    /// </summary>
    async Task CatchUpOn((string Server, long Profile) profile)
    {
        // Events heard from here on set it again: their requests may come after this list.
        var pending = unmarked;
        unmarked = false;
        RequestList unseen;
        try
        {
            unseen = await Api.Requests(new RequestQuery { Unseen = true, Limit = CatchUp });
        }
        catch (Exception)
        {
            if (pending) unmarked = true;
            return;
        }
        if (owner != profile) return;
        var key = $"{NewsKey}.{profile.Server}.{profile.Profile.ToString(CultureInfo.InvariantCulture)}";
        var mark = long.TryParse(app.Storage.Get(key), NumberStyles.Integer, CultureInfo.InvariantCulture, out var m) ? m : 0;
        var fresh = unseen.Requests.Where(r => r.UpdatedAt > mark && !announced.Contains(r.Id)).ToList();
        if (fresh.Count > 0)
        {
            var told = fresh.Where(r => r.Stage is RequestStage.Available or RequestStage.Partial).ToList();
            if (told.Count == 1) Post(Ready(told[0]));
            else if (told.Count > 1) Post(new RequestNotice.News(++keys, told.Count));
            foreach (var r in fresh) announced.Add(r.Id);
        }
        if (unseen.Requests.Count == 0) return;
        var newest = unseen.Requests.Max(r => r.UpdatedAt);
        if (newest > mark) app.Storage.Set(key, newest.ToString(CultureInfo.InvariantCulture));
    }

    RequestNotice Ready(MediaRequest request) => new RequestNotice.Ready(
        ++keys, request.Id, request.Title, request.Kind, request.ItemId,
        request.Stage == RequestStage.Partial ? request.AvailableSeasons : null,
        request.Stage == RequestStage.Available, request.PosterUrl);

    void Heard(ServerEvent e)
    {
        if (!started) return;
        switch (e.Type)
        {
            case "requests_changed":
                Change(new RequestChange(e.RequestId, e.Kind, e.TmdbId));
                break;
            case "request_available":
                Told(e.RequestId!.Value);
                Post(new RequestNotice.Ready(++keys, e.RequestId.Value, e.Title ?? "", e.Kind ?? "", e.ItemId, e.Seasons, e.Complete, e.PosterUrl));
                Change(new RequestChange(e.RequestId, e.Kind, e.TmdbId));
                break;
            case "request_decided":
                // On a TV, a decision shows as the number and in the lists.
                if (e.Status == RequestStatus.Declined) Told(e.RequestId!.Value);
                Change(new RequestChange(e.RequestId, e.Kind, e.TmdbId));
                break;
            case "request_created":
                Change(new RequestChange(e.RequestId, e.Kind, e.TmdbId));
                break;
            case "request_failed":
                Change(new RequestChange(e.RequestId, e.Kind, null));
                break;
            case "requests_settings_changed":
                RefreshSoon();
                Changed?.Invoke(RequestChange.All);
                break;
        }
    }

    /// <summary>An event told <paramref name="requestId"/>'s news: it explains <c>unseen</c> rising, and the saved mark is to be raised past it.</summary>
    void Told(long requestId)
    {
        explained++;
        announced.Add(requestId);
        unmarked = true;
    }

    /// <summary>The summary is read again soon, the title's page is let go of, and the screens hear of it.</summary>
    void Change(RequestChange change)
    {
        RefreshSoon();
        if (change.Kind != null && change.TmdbId != null) Forget(change.Kind, change.TmdbId.Value);
        Changed?.Invoke(change);
    }

    void Post(RequestNotice notice)
    {
        var list = Notices.Value.Concat(new[] { notice }).ToList();
        Notices.Value = list.Skip(Math.Max(0, list.Count - Kept)).ToList();
    }

    public void Dismiss(RequestNotice notice) => Notices.Value = Notices.Value.Where(n => n.Key != notice.Key).ToList();

    // ---- Discover

    public DiscoverPage? SavedRow(DiscoverList list) => Saved<DiscoverPage>(Keyed($"{RowKey}.{list.Wire()}"));

    public async Task<DiscoverPage> Page(DiscoverList list, int page)
    {
        var key = Keyed($"{RowKey}.{list.Wire()}");
        var answer = await Api.Discover(list, page);
        if (page == 1) Keep(key, answer);
        return answer;
    }

    public Task<DiscoverPage> Search(string query, int page = 1, bool missing = false) => Api.DiscoverSearch(RequestRules.SearchText(query), page, missing);

    public DiscoverDetail? CachedTitle(string kind, long tmdbId) => titles.TryGetValue((kind, tmdbId), out var detail) ? detail : null;

    public async Task<DiscoverDetail> Title(string kind, long tmdbId)
    {
        var profile = Profile;
        var detail = await Api.DiscoverTitle(kind, tmdbId);
        if (Profile != profile) return detail;
        var key = (kind, tmdbId);
        titles[key] = detail;
        titleOrder.Remove(key);
        titleOrder.AddFirst(key);
        while (titleOrder.Count > TitlesKept)
        {
            titles.Remove(titleOrder.Last.Value);
            titleOrder.RemoveLast();
        }
        return detail;
    }

    void Forget(string kind, long tmdbId)
    {
        titles.Remove((kind, tmdbId));
        titleOrder.Remove((kind, tmdbId));
    }

    public async Task<ItemRequest?> ItemRequest(long itemId)
    {
        try
        {
            return await Api.ItemRequest(itemId);
        }
        catch (ApiException e) when (e.Status == 404)
        {
            return null;
        }
    }

    // ---- requests

    public Task<RequestList> List(RequestQuery query) => Api.Requests(query);

    public async Task<MediaRequest?> Request(long id)
    {
        try
        {
            return await Api.Request(id);
        }
        catch (ApiException e) when (e.Status == 404)
        {
            return null;
        }
    }

    public async Task<Asked> Ask(NewRequest request)
    {
        Asked asked;
        try
        {
            asked = await Api.Ask(request);
        }
        catch (ApiException e) when (e.Status == 409)
        {
            // Someone else asked meanwhile, or it arrived: the page shows it as it is now.
            Forget(request.Kind, request.TmdbId);
            Changed?.Invoke(new RequestChange(null, request.Kind, request.TmdbId));
            throw;
        }
        Acted(asked.Request);
        return asked;
    }

    public async Task Delete(MediaRequest request)
    {
        await Api.DeleteRequest(request.Id);
        Acted(request);
    }

    public async Task Unwatch(MediaRequest request)
    {
        await Api.UnwatchRequest(request.Id);
        Acted(request);
    }

    public async Task<MediaRequest> Approve(long id, Approval approval) => Acted(await Api.ApproveRequest(id, approval));

    public async Task<MediaRequest> Decline(long id, string? reason) =>
        Acted(await Api.DeclineRequest(id, reason?.Trim() is { Length: > 0 } r ? r.Substring(0, RequestRules.DeclineReasonEnd(r)) : null));

    public async Task<MediaRequest> Handled(long id) => Acted(await Api.RequestHandled(id));

    public async Task Seen()
    {
        // The badge clears at once; the server's summary follows.
        if (State.Value is RequestsState.Open { Value.Unseen: > 0 } open) State.Value = new RequestsState.Open(open.Value with { Unseen = 0 });
        try
        {
            await Api.RequestsSeen();
        }
        finally
        {
            RefreshSoon();
        }
    }

    /// <summary>
    /// <paramref name="request"/> changed here: the summary is read now (counts, what's left), the
    /// title's page is let go of, and the screens hear of it at once, before the server's event comes.
    /// </summary>
    MediaRequest Acted(MediaRequest request)
    {
        Refresh();
        Forget(request.Kind, request.TmdbId);
        Changed?.Invoke(new RequestChange(request.Id, request.Kind, request.TmdbId));
        return request;
    }

    // ---- kept on the TV

    T? Saved<T>(string? key) where T : class
    {
        if (key == null || app.Storage.Get(key) is not { Length: > 0 } text) return null;
        try
        {
            return JsonSerializer.Deserialize<T>(text, ApiJson.Options);
        }
        catch (JsonException)
        {
            return null;
        }
    }

    void Keep<T>(string? key, T value)
    {
        if (key != null) app.Storage.Set(key, JsonSerializer.Serialize(value, ApiJson.Options));
    }
}
