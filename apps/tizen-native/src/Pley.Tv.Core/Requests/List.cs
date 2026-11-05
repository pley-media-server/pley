using System;
using System.Collections.Generic;
using System.Linq;
using System.Threading.Tasks;
using Pley.Tv.Core.Ui;

namespace Pley.Tv.Core.Requests;

/// <summary>Which requests a list shows: managers' Needs attention, all of them, or those with one status.</summary>
public enum RequestView
{
    /// <summary>Managers: pending ones, and the failed ones the server won't try again (attention=1&amp;order=oldest).</summary>
    Attention,
    All,
    Pending,
    Approved,
    Available,
    Declined,
    Failed,
}

public static class RequestViews
{
    public static readonly IReadOnlyList<RequestView> All = new[] { RequestView.Attention, RequestView.All, RequestView.Pending, RequestView.Approved, RequestView.Available, RequestView.Declined, RequestView.Failed };

    /// <summary>The status the view lists; null for Needs attention and All.</summary>
    public static string? Status(this RequestView view) => view switch
    {
        RequestView.Pending => RequestStatus.Pending,
        RequestView.Approved => RequestStatus.Approved,
        RequestView.Available => RequestStatus.Available,
        RequestView.Declined => RequestStatus.Declined,
        RequestView.Failed => RequestStatus.Failed,
        _ => null,
    };
}

/// <summary>A list's filters: the view, movies or shows, one person's (managers), a title, only this account's.</summary>
public sealed record RequestFilter(RequestView View = RequestView.All, string? Kind = null, long? UserId = null, string Q = "", bool Mine = false)
{
    /// <summary>Some filter narrows the list beyond its view.</summary>
    public bool Narrowed => Kind != null || UserId != null || !string.IsNullOrWhiteSpace(Q) || Mine;

    /// <summary>Less than all shows (or all that needs attention): with none, none match.</summary>
    public bool Filtered => Narrowed || View.Status() != null;

    public RequestQuery Query(int offset, int limit) => new RequestQuery
    {
        Statuses = View.Status() is { } status ? new[] { status } : Array.Empty<string>(),
        Attention = View == RequestView.Attention,
        Kind = Kind,
        UserId = UserId,
        Q = Q.Trim() is { Length: > 0 } q ? q : null,
        Oldest = View == RequestView.Attention,
        Mine = Mine,
        Offset = offset,
        Limit = limit,
    };

    /// <summary>Whether <paramref name="request"/> still belongs in the list, as it is now (after an action on it).</summary>
    public bool Matches(MediaRequest request)
    {
        if (Kind != null && request.Kind != Kind) return false;
        if (UserId != null && request.RequestedBy?.Id != UserId) return false;
        if (!string.IsNullOrWhiteSpace(Q) && request.Title.IndexOf(Q.Trim(), StringComparison.OrdinalIgnoreCase) < 0) return false;
        if (Mine && !request.Mine && !request.Watching) return false;
        if (View == RequestView.Attention) return request.Status == RequestStatus.Pending || (request.Status == RequestStatus.Failed && request.RetryAt == null);
        return View.Status() is not { } status || request.Status == status;
    }
}

/// <summary>A list of requests, as it shows.</summary>
public abstract record RequestListState
{
    public sealed record Loading : RequestListState;

    /// <param name="Counts">By status, with every filter but the view's.</param>
    /// <param name="People">Managers: the people with requests, for the person filter.</param>
    /// <param name="Problem">Reading it again failed: what shows is from before.</param>
    public sealed record Ready(IReadOnlyList<MediaRequest> Requests, int Total, IReadOnlyDictionary<string, int> Counts, IReadOnlyList<RequestUser> People,
        bool LoadingMore = false, RequestProblem? Problem = null) : RequestListState
    {
        public bool More => Requests.Count < Total;

        /// <summary>Something in it downloads or goes into the library: it's read again every 10 s.</summary>
        public bool Moving => Requests.Any(r => r.Moving);
    }

    /// <summary>Nothing to list: none at all, or none matching the filters.</summary>
    public sealed record Empty(bool Filtered) : RequestListState;

    public sealed record Failed(RequestProblem Problem) : RequestListState;
}

/// <summary>Your requests, or for managers everyone's (Requests).</summary>
/// <param name="Manage">Everyone's, with Needs attention, the filters and the managers' actions.</param>
/// <param name="Attention">Managers: how many need them, beside Needs attention.</param>
/// <param name="SignInForApprovals">An administrator's app without approvals: "Sign in again to approve and decline here."</param>
/// <param name="Busy">Requests with an action on its way.</param>
/// <param name="MayAsk">Asking works: titles open their Discover page.</param>
/// <param name="Problem">Requests can't show at all: requests off, signed in before requests.</param>
public sealed record RequestsListState(
    RequestsState Gate,
    bool Manage,
    RequestFilter Filter,
    RequestListState List,
    string Origin,
    int Attention,
    bool SignInForApprovals,
    IReadOnlyCollection<long> Busy,
    bool MayAsk,
    RequestProblem? Problem);

/// <summary>
/// Your requests, and for managers everyone's (the Android app's RequestsViewModel), opening on
/// Needs attention for managers, with filters (movies or shows, a person, a title) and their
/// actions. Each card offers exactly the server's actions. The list follows requests_changed (one
/// request read again when only it changed), every 10 s while something in it downloads, and every
/// 30 s while events can't come. Once its first page shows, the person's news is seen.
/// </summary>
public sealed class RequestsModel : IDisposable
{
    public const int PageSize = 30;

    /// <summary>The most requests the server lists at once.</summary>
    public const int MaxPage = 200;

    /// <summary>While events can't come.</summary>
    public const long PollMs = 30_000;

    /// <summary>How long typing in the title filter pauses before it applies.</summary>
    public const long TypingMs = 300;

    readonly IRequestsSource source;
    readonly IScheduler scheduler;
    readonly List<IDisposable> watches = new List<IDisposable>();
    readonly HashSet<long> busy = new HashSet<long>();
    RequestFilter? filter;
    RequestListState list = new RequestListState.Loading();
    bool seen, gone;
    int loads;
    /// <summary>Readings again of what shows: only the last one's answer counts.</summary>
    int reloads;
    IDisposable? typing, soon, tick;
    /// <summary>Requests read again one at a time, in the order their changes came.</summary>
    Task reading = Task.CompletedTask;

    public RequestsModel(IRequestsSource source, IScheduler scheduler)
    {
        this.source = source;
        this.scheduler = scheduler;
        State = new Signal<RequestsListState>(Build());
        watches.Add(source.State.Watch(gate =>
        {
            // Managers open on Needs attention; everyone else on all of theirs.
            if (filter == null && gate is RequestsState.Open open)
            {
                filter = new RequestFilter(open.Value.Manage ? RequestView.Attention : RequestView.All);
                _ = Load();
            }
            Update();
        }));
        watches.Add(source.Streaming.Watch(_ => Ticking()));
        source.Changed += Heard;
    }

    public Signal<RequestsListState> State { get; }

    /// <summary>What the page tells once: what an action did or why it failed.</summary>
    public event Action<RequestEvent>? Told;

    /// <summary>What is typed in the title filter, as the field shows it.</summary>
    public string TitleQuery { get; private set; } = "";

    public void Dispose()
    {
        gone = true;
        foreach (var w in watches) w.Dispose();
        source.Changed -= Heard;
        typing?.Dispose();
        soon?.Dispose();
        tick?.Dispose();
    }

    void Update()
    {
        if (gone) return;
        State.Value = Build();
        Ticking();
    }

    RequestsListState Build()
    {
        var gate = source.State.Value;
        var summary = gate.Summary;
        RequestProblem? problem = gate switch
        {
            RequestsState.SignInAgain => new RequestProblem.SignInAgain(RequestScope.Requests),
            RequestsState.Hidden => new RequestProblem.Off(),
            RequestsState.NoAccess => new RequestProblem.NoAccess(),
            _ => null,
        };
        return new RequestsListState(
            gate,
            summary?.Manage == true,
            filter ?? new RequestFilter(),
            filter == null ? new RequestListState.Loading() : list,
            source.Origin,
            summary == null ? 0 : summary.Pending + summary.Failed,
            summary?.SignInAgain == RequestScope.Approvals,
            busy.ToList(),
            gate.MayAsk,
            problem);
    }

    /// <summary>Read again while it shows: every 10 s while something moves, every 30 s without events.</summary>
    void Ticking()
    {
        var period = list is RequestListState.Ready { Moving: true } ? DiscoverModel.MovingMs : !source.Streaming.Value ? PollMs : 0;
        tick?.Dispose();
        tick = null;
        if (gone || period == 0 || filter == null) return;
        tick = scheduler.After(period, () =>
        {
            tick = null;
            _ = Reload(quiet: true);
        });
    }

    async Task Load()
    {
        if (filter is not { } f) return;
        var mine = ++loads;
        list = new RequestListState.Loading();
        Update();
        RequestListState next;
        try
        {
            next = Shown(f, await source.List(f.Query(0, PageSize)));
        }
        catch (Exception e)
        {
            next = new RequestListState.Failed(RequestProblem.Of(e));
        }
        if (gone || mine != loads) return;
        list = next;
        Update();
        MarkSeen();
    }

    static RequestListState Shown(RequestFilter f, RequestList answer)
    {
        if (answer.Requests.Count == 0 && answer.Total == 0) return new RequestListState.Empty(f.Filtered);
        var ids = new HashSet<long>();
        return new RequestListState.Ready(answer.Requests.Where(r => ids.Add(r.Id)).ToList(), answer.Total, answer.Counts, answer.People);
    }

    /// <summary>Once the first page has shown: the person's news is seen, and the badge clears.</summary>
    void MarkSeen()
    {
        if (seen || list is not RequestListState.Ready) return;
        seen = true;
        if ((source.State.Value.Summary?.Unseen ?? 0) > 0) _ = Quietly(source.Seen());
    }

    static async Task Quietly(Task task)
    {
        try
        {
            await task;
        }
        catch (Exception)
        {
            // Seen again the next time the list shows.
        }
    }

    /// <summary>Reads what shows again, as long as it is now. <paramref name="quiet"/>: a failure keeps what shows.</summary>
    async Task Reload(bool quiet)
    {
        if (filter is not { } f) return;
        var mine = loads;
        var again = ++reloads;
        var count = (list as RequestListState.Ready)?.Requests.Count ?? 0;
        try
        {
            var answer = await First(f, Math.Max(PageSize, count));
            if (gone || mine != loads || again != reloads || filter != f) return;
            list = Shown(f, answer);
            Update();
            MarkSeen();
        }
        catch (Exception e)
        {
            if (gone || mine != loads || again != reloads) return;
            var problem = RequestProblem.Of(e);
            list = list is RequestListState.Ready ready ? ready with { Problem = problem } : quiet ? list : new RequestListState.Failed(problem);
            Update();
        }
    }

    /// <summary>The first <paramref name="count"/> requests <paramref name="f"/> keeps, read <see cref="MaxPage"/> at a time.</summary>
    async Task<RequestList> First(RequestFilter f, int count)
    {
        var answer = await source.List(f.Query(0, Math.Min(count, MaxPage)));
        while (answer.Requests.Count < Math.Min(count, answer.Total))
        {
            var read = answer.Requests.Count;
            var next = await source.List(f.Query(read, Math.Min(count - read, MaxPage)));
            if (next.Requests.Count == 0) break;
            answer = next with { Requests = answer.Requests.Concat(next.Requests).ToList() };
        }
        return answer;
    }

    /// <summary>Retry: the summary and the list read again.</summary>
    public void Refresh()
    {
        source.Refresh();
        if (filter == null) return;
        if (list is RequestListState.Ready) _ = Reload(quiet: false);
        else _ = Load();
    }

    /// <summary>The list came near its end: the next page.</summary>
    public void More()
    {
        if (filter is not { } f || list is not RequestListState.Ready { More: true, LoadingMore: false } now) return;
        list = now with { LoadingMore = true, Problem = null };
        Update();
        _ = MoreOf(f, now.Requests.Count);
    }

    async Task MoreOf(RequestFilter f, int offset)
    {
        var mine = loads;
        try
        {
            var answer = await source.List(f.Query(offset, PageSize));
            if (gone || mine != loads || filter != f || list is not RequestListState.Ready current) return;
            var ids = new HashSet<long>();
            list = current with { Requests = current.Requests.Concat(answer.Requests).Where(r => ids.Add(r.Id)).ToList(), Total = answer.Total, LoadingMore = false };
        }
        catch (Exception e)
        {
            if (gone || mine != loads || list is not RequestListState.Ready current) return;
            list = current with { LoadingMore = false, Problem = RequestProblem.Of(e) };
        }
        Update();
    }

    // ---- filters

    public void View(RequestView view) => Filter(f => f with { View = view });

    public void Kind(string? kind) => Filter(f => f with { Kind = kind });

    public void Person(long? userId) => Filter(f => f with { UserId = userId });

    /// <summary>The title filter's field changed: it applies once typing pauses.</summary>
    public void TypeTitle(string text)
    {
        TitleQuery = text;
        typing?.Dispose();
        typing = scheduler.After(TypingMs, () =>
        {
            typing = null;
            var q = TitleQuery.Trim();
            Filter(f => f.Q == q ? f : f with { Q = q });
        });
    }

    void Filter(Func<RequestFilter, RequestFilter> change)
    {
        if (filter is not { } f) return;
        var next = change(f);
        if (next == f) return;
        filter = next;
        _ = Load();
    }

    // ---- what changed

    void Heard(RequestChange change)
    {
        if (gone || filter == null) return;
        if (change.RequestId is not { } id || list is not RequestListState.Ready now || now.Requests.All(r => r.Id != id))
        {
            // Many changed, or one this list may now show: the list, once changes pause.
            soon?.Dispose();
            soon = scheduler.After(DiscoverModel.ChangesMs, () =>
            {
                soon = null;
                _ = Reload(quiet: true);
            });
            return;
        }
        // Only that request: it alone is read again.
        reading = RequestsSources.After(reading, () => ReadOne(id));
    }

    async Task ReadOne(long id)
    {
        MediaRequest? request;
        try
        {
            request = await source.Request(id);
        }
        catch (Exception)
        {
            return;
        }
        if (!gone) Replace(id, request);
    }

    /// <summary><paramref name="id"/> as it is now (<paramref name="request"/>), kept where it still belongs; null: gone.</summary>
    void Replace(long id, MediaRequest? request)
    {
        if (filter is not { } f || list is not RequestListState.Ready now) return;
        var kept = request != null && f.Matches(request) ? request : null;
        var requests = kept != null ? now.Requests.Select(r => r.Id == id ? kept : r).ToList() : now.Requests.Where(r => r.Id != id).ToList();
        var removed = now.Requests.Count - requests.Count;
        list = requests.Count == 0 && now.Total - removed <= 0 ? new RequestListState.Empty(f.Filtered) : now with { Requests = requests, Total = now.Total - removed };
        Update();
    }

    // ---- actions

    /// <summary><paramref name="action"/> on <paramref name="request"/> (<paramref name="reason"/> for Decline).</summary>
    public void Act(MediaRequest request, CardAction action, string? reason = null)
    {
        if (gone || !busy.Add(request.Id)) return;
        Update();
        _ = Acting(request, action, reason);
    }

    async Task Acting(MediaRequest request, CardAction action, string? reason)
    {
        try
        {
            switch (await source.Perform(request, action, reason))
            {
                case Outcome.Changed changed:
                    Replace(request.Id, changed.Request);
                    break;
                case Outcome.Gone:
                    Replace(request.Id, null);
                    break;
                case Outcome.AskedAgain again:
                    Told?.Invoke(new RequestEvent.Asked(again.Asked, request.Title));
                    await Reload(quiet: true);
                    break;
            }
        }
        catch (Exception e)
        {
            if (gone) return;
            var problem = RequestProblem.Of(e);
            // A race: the server sends the request as it is now.
            if (problem is RequestProblem.Conflict { Request: { } now }) Replace(request.Id, now);
            if (problem is RequestProblem.NotFound) Replace(request.Id, null);
            Told?.Invoke(new RequestEvent.Failed(problem));
        }
        finally
        {
            busy.Remove(request.Id);
            Update();
        }
    }

    /// <summary>
    /// Where a request card opens: the library's title once it's there, else its Discover page while
    /// asking works; null: nowhere.
    /// </summary>
    public static PageKey? Opens(MediaRequest request, bool mayAsk)
    {
        PageKey? item = request.ItemId is { } id ? new PageKey.Item(id) : null;
        if (item != null && request.Stage is RequestStage.Available or RequestStage.Partial) return item;
        return mayAsk ? new PageKey.DiscoverTitle(request.Kind, request.TmdbId) : item;
    }
}
