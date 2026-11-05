using System;
using System.Collections.Generic;
using System.Linq;
using System.Threading.Tasks;
using Pley.Tv.Core.Ui;

namespace Pley.Tv.Core.Requests;

/// <summary>A title page's state.</summary>
public abstract record TitleState
{
    public sealed record Loading : TitleState;

    /// <summary>The page can't show: signed in before requests, requests off, TMDB away, not found…</summary>
    public sealed record Problem(RequestProblem Value, RequestSummary? Summary) : TitleState;

    /// <param name="Busy">An ask or an action is on its way: the buttons wait.</param>
    /// <param name="Failed">From what was read before: the server didn't answer this time.</param>
    /// <param name="Summary">The summary the page was drawn with: the limits its words name.</param>
    public sealed record Ready(DiscoverDetail Detail, MainAction Action, Allowance Allowance, bool Manage, bool Busy, RequestProblem? Failed, RequestSummary? Summary) : TitleState;
}

/// <summary>
/// A title found on TMDB (the Android app's DiscoverTitleViewModel): its details, its one main
/// action, its seasons with their states, and the season picker. While it shows, the page reads
/// itself again when one of its requests changes, when many did, and every 10 s while one of them
/// downloads or goes into the library (download progress sends no events).
/// </summary>
public sealed class DiscoverTitleModel : IDisposable
{
    readonly IRequestsSource source;
    readonly IScheduler scheduler;
    readonly PageKey.DiscoverTitle key;
    readonly List<IDisposable> watches = new List<IDisposable>();
    DiscoverDetail? detail;
    RequestProblem? failed;
    bool busy, gone;
    /// <summary>The picker was asked for when the page opened (Request more seasons), and hasn't opened yet.</summary>
    bool pickPending;
    int reads;
    IDisposable? soon, moving;

    public DiscoverTitleModel(IRequestsSource source, IScheduler scheduler, PageKey.DiscoverTitle key)
    {
        this.source = source;
        this.scheduler = scheduler;
        this.key = key;
        pickPending = key.Pick;
        detail = source.CachedTitle(key.Kind, key.TmdbId);
        State = new Signal<TitleState>(new TitleState.Loading());
        watches.Add(source.State.Watch(_ => Gated()));
        source.Changed += Heard;
    }

    public Signal<TitleState> State { get; }

    /// <summary>The season picker while it shows.</summary>
    public Signal<SeasonPicker?> Picker { get; } = new Signal<SeasonPicker?>(null);

    /// <summary>What the page tells once: what asking did, what failed, where to go.</summary>
    public event Action<RequestEvent>? Told;

    public string Origin => source.Origin;

    public void Dispose()
    {
        gone = true;
        foreach (var w in watches) w.Dispose();
        source.Changed -= Heard;
        soon?.Dispose();
        moving?.Dispose();
    }

    bool Open => source.State.Value is RequestsState.Open;

    void Gated()
    {
        if (gone) return;
        if (Open && reads == 0) _ = Read();
        Update();
    }

    /// <summary>Reads the page again after it failed.</summary>
    public void Retry()
    {
        source.Refresh();
        if (Open) _ = Read();
    }

    async Task Read()
    {
        var mine = ++reads;
        try
        {
            var fresh = await source.Title(key.Kind, key.TmdbId);
            if (gone || mine != reads) return;
            detail = fresh;
            failed = null;
        }
        catch (Exception e)
        {
            if (gone || mine != reads) return;
            failed = RequestProblem.Of(e);
        }
        // While one of its requests moves, the page is read again every 10 s.
        moving?.Dispose();
        moving = detail?.Requests.Any(r => r.Moving) == true ? scheduler.After(DiscoverModel.MovingMs, () =>
        {
            moving = null;
            _ = Read();
        }) : null;
        Update();
    }

    /// <summary>
    /// Read again when news may be about the title: one of its own requests, the title named, or
    /// many changed; once changes pause.
    /// </summary>
    void Heard(RequestChange change)
    {
        if (gone || !Open) return;
        var ours = change.About(key.Kind, key.TmdbId) || (change.RequestId is { } id && detail?.Requests.Any(r => r.Id == id) == true);
        if (!ours) return;
        soon?.Dispose();
        soon = scheduler.After(DiscoverModel.ChangesMs, () =>
        {
            soon = null;
            _ = Read();
        });
    }

    void Update()
    {
        if (gone) return;
        var gate = source.State.Value;
        var summary = gate.Summary;
        switch (gate)
        {
            case RequestsState.SignInAgain:
                State.Value = new TitleState.Problem(new RequestProblem.SignInAgain(RequestScope.Requests), summary);
                return;
            case RequestsState.Hidden:
                State.Value = new TitleState.Problem(new RequestProblem.Off(), null);
                return;
            case RequestsState.NoAccess:
                State.Value = new TitleState.Problem(new RequestProblem.NoAccess(), null);
                return;
            case RequestsState.Unknown:
                State.Value = new TitleState.Loading();
                return;
        }
        if (detail == null)
        {
            State.Value = failed != null ? new TitleState.Problem(failed, summary) : new TitleState.Loading();
            return;
        }
        var action = RequestRules.Main(detail, gate);
        var allowance = Allowance.Of(detail, summary);
        // The page first: a screen drawing the picker draws it over the page as it is.
        State.Value = new TitleState.Ready(detail, action, allowance, summary?.Manage == true, busy, failed, summary);
        if (pickPending && failed == null && detail.Kind == TitleKind.Show)
        {
            pickPending = false;
            if (action is MainAction.Request { Ask: Pley.Tv.Core.Requests.Ask.Yes } || action is MainAction.Partial { More: Pley.Tv.Core.Requests.Ask.Yes })
            {
                Picker.Value = SeasonPicker.Of(detail, allowance.SeasonsLeft);
                return;
            }
        }
        // The picker follows the page as it's read again: seasons asked for meanwhile can't be ticked.
        if (Picker.Value is { } open)
        {
            var fresh = SeasonPicker.Of(detail, allowance.SeasonsLeft);
            var askable = new HashSet<int>(fresh.Seasons.Where(s => s.Askable).Select(s => s.Season.Number));
            var picked = open.Picked.Where(askable.Contains).ToList();
            // Only when it changed: the panel isn't made again for nothing.
            if (!picked.OrderBy(n => n).SequenceEqual(open.Picked.OrderBy(n => n)) || !fresh.Seasons.SequenceEqual(open.Seasons) || fresh.Left != open.Left)
                Picker.Value = fresh with { Picked = picked, Start = open.Start };
        }
    }

    TitleState.Ready? Shown => State.Value as TitleState.Ready;

    /// <summary>Request: a movie is asked for; a show's picker opens.</summary>
    public void Request()
    {
        if (Shown is not { Busy: false } page) return;
        if (page.Detail.Kind == TitleKind.Show) OpenPicker();
        else Ask(new NewRequest(page.Detail.Kind, page.Detail.TmdbId), page.Detail.Title);
    }

    /// <summary>Request more seasons: the picker; opened from a season's poster (<paramref name="start"/>), that season ticked too.</summary>
    public void OpenPicker(int? start = null)
    {
        if (Shown is { } page) Picker.Value = SeasonPicker.Of(page.Detail, page.Allowance.SeasonsLeft, start);
    }

    public void Toggle(int season)
    {
        if (Picker.Value is { } open) Picker.Value = open.Toggle(season);
    }

    public void ClosePicker() => Picker.Value = null;

    /// <summary>The picker's Request: the seasons ticked.</summary>
    public void AskPicked()
    {
        if (Shown is not { Busy: false } page || Picker.Value is not { Ready: true } picker) return;
        Ask(new NewRequest(page.Detail.Kind, page.Detail.TmdbId, picker.Asked()), page.Detail.Title, closesPicker: true);
    }

    /// <summary>Tell me too: following the requests others made, which costs nothing.</summary>
    public void TellMeToo()
    {
        if (Shown is { Busy: false } page) Ask(RequestRules.TellMeToo(page.Detail), page.Detail.Title);
    }

    /// <summary>Stop following the request this account follows for it.</summary>
    public void StopFollowing()
    {
        if (Shown is not { Busy: false } page) return;
        var request = (page.Action as MainAction.Following)?.Active ?? (RequestRules.ActiveRequest(page.Detail) is { Watching: true } r ? r : null);
        if (request == null) return;
        Run(() => source.Unwatch(request));
    }

    /// <summary>Where Open goes: the library's title.</summary>
    public PageKey? Opens() => Shown?.Action switch
    {
        MainAction.Open { ItemId: { } id } => new PageKey.Item(id),
        MainAction.Partial { ItemId: { } id } => new PageKey.Item(id),
        _ => null,
    };

    void Ask(NewRequest request, string title, bool closesPicker = false) => Run(async () =>
    {
        var asked = await source.Ask(request);
        if (gone) return;
        if (closesPicker) Picker.Value = null;
        Tell(new RequestEvent.Asked(asked, title));
    });

    void Run(Func<Task> work)
    {
        busy = true;
        Update();
        _ = Running(work);
    }

    async Task Running(Func<Task> work)
    {
        try
        {
            await work();
        }
        catch (Exception e)
        {
            if (gone) return;
            var problem = RequestProblem.Of(e);
            // Someone acted meanwhile: the page is read again, and what happened is told.
            if (problem is RequestProblem.Conflict or RequestProblem.NotFound) _ = Read();
            Tell(new RequestEvent.Failed(problem));
        }
        finally
        {
            busy = false;
            Update();
        }
    }

    void Tell(RequestEvent e) => Told?.Invoke(e);
}

/// <summary>
/// What requests say of a library's movie or show (GET /api/items/{id}/request): the stage pill of
/// its newest active request, and Request more seasons (<paramref name="More"/>) when some can be
/// asked for and asking works.
/// </summary>
public sealed record ItemRequestState(string Kind, long TmdbId, string Status, MediaRequest? Request, bool More, bool Manage)
{
    /// <summary>Where Request more seasons goes: the Discover page, its picker open.</summary>
    public PageKey.DiscoverTitle Picker => new PageKey.DiscoverTitle(Kind, TmdbId, Pick: true);

    public static ItemRequestState Of(ItemRequest answer, bool mayAsk, bool manage) =>
        new ItemRequestState(answer.Kind, answer.TmdbId, answer.Status, answer.Request, mayAsk && answer.Askable is { Count: > 0 }, manage);
}

/// <summary>
/// What requests say of a library's movie or show, for its page (the Android app's
/// ItemViewModel.request): asked for once the page has drawn and its kind is known, and only when the
/// person may ask or has requests; read again every 3 s, 4 times at most, while it fails or TMDB's
/// details haven't come (Request more seasons waits for them), then when one of its requests
/// changes, and every 10 s while its request downloads or goes into the library. Null: nothing to
/// show (not a movie or show, no TMDB match, or requests don't apply).
/// </summary>
public sealed class ItemRequestModel : IDisposable
{
    /// <summary>The server waits for TMDB 2 s at most: a later read has what an earlier one lacked.</summary>
    public const long AgainMs = 3_000;
    public const int AgainReads = 4;

    readonly IRequestsSource source;
    readonly IScheduler scheduler;
    readonly long itemId;
    readonly List<IDisposable> watches = new List<IDisposable>();
    string? kind;
    bool drawn, gone;
    (bool MayAsk, bool Manage)? gate;
    ItemRequest? answer;
    int runs;
    IDisposable? timer;

    public ItemRequestModel(IRequestsSource source, IScheduler scheduler, long itemId)
    {
        this.source = source;
        this.scheduler = scheduler;
        this.itemId = itemId;
        watches.Add(source.State.Watch(_ => Gate()));
        source.Changed += Heard;
    }

    public Signal<ItemRequestState?> State { get; } = new Signal<ItemRequestState?>(null);

    public void Dispose()
    {
        gone = true;
        runs++;
        timer?.Dispose();
        foreach (var w in watches) w.Dispose();
        source.Changed -= Heard;
    }

    /// <summary>The page has drawn, its item of <paramref name="itemKind"/>: requests may be asked now.</summary>
    public void Shown(string itemKind)
    {
        drawn = true;
        kind = itemKind;
        Gate();
    }

    void Gate()
    {
        if (gone) return;
        var s = source.State.Value;
        var wanted = s.MayAsk || (s.Summary is { } summary && summary.Own > 0 && summary.YourRequests);
        (bool, bool)? next = drawn && wanted && kind is "movie" or "show" ? (s.MayAsk, s.Summary?.Manage == true) : null;
        if (next == gate) return;
        gate = next;
        var mine = ++runs;
        timer?.Dispose();
        timer = null;
        answer = null;
        State.Value = null;
        if (next != null) _ = Read(mine, 0);
    }

    async Task Read(int mine, int attempt)
    {
        ItemRequest? read;
        bool failed;
        try
        {
            read = await source.ItemRequest(itemId);
            failed = false;
        }
        catch (Exception)
        {
            read = null;
            failed = true;
        }
        if (gone || mine != runs || gate is not { } g) return;
        // No TMDB match: nothing to show, ever.
        if (!failed && read == null && answer == null)
        {
            State.Value = null;
            return;
        }
        if (read != null)
        {
            answer = read;
            State.Value = ItemRequestState.Of(read, g.MayAsk, g.Manage);
        }
        timer?.Dispose();
        timer = null;
        if ((answer == null || answer.Askable == null) && attempt < AgainReads)
        {
            timer = scheduler.After(AgainMs, () =>
            {
                timer = null;
                _ = Read(mine, attempt + 1);
            });
            return;
        }
        // While its request moves, it's read again every 10 s: download progress sends no events.
        if (answer?.Request?.Moving == true)
            timer = scheduler.After(DiscoverModel.MovingMs, () =>
            {
                timer = null;
                _ = Read(mine, AgainReads);
            });
    }

    void Heard(RequestChange change)
    {
        if (gone || gate == null || answer is not { } shown) return;
        if (!change.About(shown.Kind, shown.TmdbId) && !(change.RequestId != null && change.RequestId == shown.Request?.Id)) return;
        var mine = runs;
        timer?.Dispose();
        timer = scheduler.After(DiscoverModel.ChangesMs, () =>
        {
            timer = null;
            _ = Read(mine, AgainReads);
        });
    }
}
