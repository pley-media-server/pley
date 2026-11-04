using System;
using System.Collections.Generic;
using System.Linq;
using System.Threading.Tasks;
using Pley.Tv.Core.Ui;

namespace Pley.Tv.Core.Requests;

/// <summary>Something a request screen tells once (a toast), or a place to go.</summary>
public abstract record RequestEvent
{
    /// <summary>Asking for <paramref name="Title"/> did what <paramref name="Value"/> says: "Asked for “X”: an administrator decides."</summary>
    public sealed record Asked(Requests.Asked Value, string Title) : RequestEvent;

    /// <summary>It didn't work: <paramref name="Problem"/> says why.</summary>
    public sealed record Failed(RequestProblem Problem) : RequestEvent;

    /// <summary>Go to <paramref name="Key"/>: a show's picker, a library title.</summary>
    public sealed record Open(PageKey Key) : RequestEvent;
}

/// <summary>
/// Titles of a list or of search as they're read page by page: each title once, more to read while
/// <see cref="Page"/> &lt; <see cref="TotalPages"/> (a short page, even an empty one, isn't the end).
/// </summary>
/// <param name="Page">The last page read; 0 before the first.</param>
/// <param name="Stopped">
/// The last pages read on their own brought fewer titles than wanted, and more are read only when
/// asked for (a grid's Show more): for an account with an age limit, page after page can come
/// empty. It holds until a page is read (<see cref="With"/> starts afresh) or the reading is asked
/// to go on.
/// </param>
public sealed record Pages(IReadOnlyList<DiscoverTitle> Titles, int Page = 0, int TotalPages = 1, bool Loading = false, RequestProblem? Problem = null, bool Stopped = false)
{
    public static readonly Pages None = new Pages(Array.Empty<DiscoverTitle>());

    public bool More => Page < TotalPages;

    /// <summary>Nothing more is read unless asked for: every page read, or the reading stopped short or failed.</summary>
    public bool Settled => !Loading && (Problem != null || (Page > 0 && (!More || Stopped)));

    /// <summary><paramref name="answer"/> added: the first page replaces what was there; later ones add what's new.</summary>
    public Pages With(DiscoverPage answer)
    {
        var all = answer.Page <= 1 ? answer.Results : Titles.Concat(answer.Results);
        var seen = new HashSet<string>();
        var titles = all.Where(t => seen.Add(t.Key)).ToList();
        return new Pages(titles, Math.Max(1, answer.Page), answer.TotalPages);
    }

    /// <summary>Another page is wanted without scrolling: fewer than a <paramref name="screenful"/>, and more to read.</summary>
    public bool Short(int screenful) => More && !Loading && Problem == null && !Stopped && Titles.Count < screenful;

    public Pages Map(Func<DiscoverTitle, DiscoverTitle> transform) => this with { Titles = Titles.Select(transform).ToList() };
}

/// <summary>One of Discover's rows: its titles as read so far.</summary>
public sealed record DiscoverRow(DiscoverList List, Pages Pages);

/// <summary>
/// Discover's top: the first title of its rows (trending's, unless it has none for this account),
/// with its main action, and its page once read (<paramref name="Detail"/>: the seasons Tell me too follows).
/// </summary>
public sealed record DiscoverHero(DiscoverTitle Title, MainAction Action, DiscoverDetail? Detail = null);

/// <summary>Discover's page.</summary>
/// <param name="Rows">In their order; a row shows once it has titles, or is reading them.</param>
/// <param name="Problem">
/// Why the page can't show: the gate first (sign in again, off, no access), then the trending
/// row's failure, then, with no title in any row, the first row's that failed.
/// </param>
/// <param name="Stale">Shown from what was saved: the server didn't answer.</param>
/// <param name="Yours">"Your requests", newest first, for those who have any.</param>
/// <param name="Asking">The hero's ask is on its way: its button waits.</param>
/// <param name="Nothing">Every row was read, and none has anything this account can ask for: "Nothing here right now".</param>
public sealed record DiscoverState(
    RequestsState Gate,
    string Origin,
    DiscoverHero? Hero,
    IReadOnlyList<DiscoverRow> Rows,
    RequestProblem? Problem,
    bool Stale,
    IReadOnlyList<MediaRequest> Yours,
    bool Manage,
    Allowance? Allowance,
    bool Asking,
    bool Nothing);

/// <summary>
/// Discover (the Android app's DiscoverViewModel): a hero for the first trending title (the first
/// title of the next row with any, when trending has none for this account: then every row is read
/// at once, to find one), then rows (Trending this week, Popular movies, Popular shows, Coming to
/// cinemas, Popular anime) read as they come into view (<see cref="ShowRow"/>), page by page until a
/// screenful and then as they scroll (<see cref="More"/>), each title once; then the person's
/// requests (read again as requests change, and every 10 s while one downloads). Badges follow
/// requests_changed in rows already shown, without reading the rows again.
/// </summary>
public sealed class DiscoverModel : IDisposable
{
    /// <summary>Changes gathered before "Your requests" is read again.</summary>
    public const long ChangesMs = 500;

    /// <summary>A row's screenful, and a little more.</summary>
    public const int RowScreenful = 12;

    /// <summary>Pages read on their own, one after another, before waiting for a scroll.</summary>
    public const int MaxAutoPages = 4;

    /// <summary>"Your requests" on Discover: the newest.</summary>
    public const int YoursCount = 12;

    /// <summary>While something downloads or goes into the library.</summary>
    public const long MovingMs = 10_000;

    readonly IRequestsSource source;
    readonly IScheduler scheduler;
    readonly Dictionary<DiscoverList, Pages> rows = DiscoverLists.All.ToDictionary(l => l, _ => Pages.None);
    /// <summary>Rows asked for, with the reading they're on: a Retry starts a row's afresh.</summary>
    readonly Dictionary<DiscoverList, int> started = new Dictionary<DiscoverList, int>();
    readonly List<IDisposable> watches = new List<IDisposable>();
    IReadOnlyList<MediaRequest> yours = Array.Empty<MediaRequest>();
    bool stale, asking, gone, allRows, hasYours;
    int readings;
    (string Kind, long TmdbId)? heroKey;
    DiscoverDetail? heroDetail;
    IDisposable? yoursSoon, moving;
    /// <summary>Badges follow changes one at a time, in the order they came: an older answer never lands last.</summary>
    Task patching = Task.CompletedTask;

    public DiscoverModel(IRequestsSource source, IScheduler scheduler)
    {
        this.source = source;
        this.scheduler = scheduler;
        State = new Signal<DiscoverState>(Build());
        watches.Add(source.State.Watch(Gated));
        source.Changed += Heard;
    }

    public Signal<DiscoverState> State { get; }

    /// <summary>What the page tells once: what asking did, what failed, where to go.</summary>
    public event Action<RequestEvent>? Told;

    public void Dispose()
    {
        gone = true;
        foreach (var w in watches) w.Dispose();
        source.Changed -= Heard;
        yoursSoon?.Dispose();
        moving?.Dispose();
    }

    void Gated(RequestsState gate)
    {
        if (gone) return;
        // Rows are read only while asking works; the trending row at once, for the hero.
        if (gate.MayAsk)
        {
            ShowRow(DiscoverList.Trending);
            foreach (var list in started.Keys.ToList()) Begin(list);
        }
        var any = gate.Summary is { } s && s.Own > 0 && s.YourRequests;
        if (any != hasYours)
        {
            hasYours = any;
            if (any) _ = ReadYours();
            else yours = Array.Empty<MediaRequest>();
        }
        Update();
    }

    // ---- rows

    /// <summary><paramref name="list"/>'s row came into view: its first page is read (from what was saved first), then more until a screenful.</summary>
    public void ShowRow(DiscoverList list)
    {
        if (gone || started.ContainsKey(list)) return;
        started[list] = 0;
        Begin(list);
    }

    /// <summary>The reading <paramref name="list"/>'s row is on; 0 before it starts.</summary>
    int Reading(DiscoverList list) => started.TryGetValue(list, out var n) ? n : 0;

    /// <summary>Starts reading a row asked for, once asking works.</summary>
    void Begin(DiscoverList list)
    {
        if (!source.State.Value.MayAsk || started[list] != 0) return;
        var reading = ++readings;
        started[list] = reading;
        var pages = rows[list];
        pages = pages with { Loading = pages.Titles.Count == 0, Problem = null };
        var saved = source.SavedRow(list);
        if (saved != null) pages = pages.With(saved) with { Loading = false };
        rows[list] = pages;
        Update();
        _ = First(list, reading, saved);
    }

    async Task First(DiscoverList list, int reading, DiscoverPage? saved)
    {
        DiscoverPage answer;
        try
        {
            answer = await source.Page(list, 1);
        }
        catch (Exception e)
        {
            if (gone || Reading(list) != reading) return;
            var problem = RequestProblem.Of(e);
            if (list == DiscoverList.Trending) stale = saved != null && problem is RequestProblem.Offline;
            rows[list] = rows[list] with { Loading = false, Problem = problem };
            Update();
            return;
        }
        if (gone || Reading(list) != reading) return;
        // A newer first page: what came after it is read again.
        rows[list] = rows[list].With(answer);
        if (list == DiscoverList.Trending) stale = false;
        Update();
        await Fill(list, reading);
    }

    /// <summary>Reads on while <paramref name="list"/>'s row is short of a screenful; still short, it waits for a scroll (<see cref="Pages.Stopped"/>).</summary>
    async Task Fill(DiscoverList list, int reading)
    {
        var auto = 0;
        while (rows[list].Short(RowScreenful) && auto++ < MaxAutoPages)
            if (!await Next(list, reading)) break;
        if (gone || Reading(list) != reading) return;
        if (rows[list].Short(RowScreenful)) rows[list] = rows[list] with { Stopped = true };
        Update();
    }

    /// <summary><paramref name="list"/>'s row scrolled near its end: its next page, if there is one.</summary>
    public void More(DiscoverList list)
    {
        var pages = rows[list];
        if (gone || !pages.More || pages.Loading || pages.Page == 0) return;
        _ = Next(list, Reading(list));
    }

    /// <summary>Reads <paramref name="list"/>'s next page; whether it came.</summary>
    async Task<bool> Next(DiscoverList list, int reading)
    {
        var before = rows[list];
        if (before.Loading) return false;
        rows[list] = before with { Loading = true, Problem = null };
        Update();
        try
        {
            var answer = await source.Page(list, before.Page + 1);
            if (gone || Reading(list) != reading) return false;
            rows[list] = rows[list].With(answer);
            Update();
            return true;
        }
        catch (Exception e)
        {
            if (gone || Reading(list) != reading) return false;
            rows[list] = rows[list] with { Loading = false, Problem = RequestProblem.Of(e) };
            Update();
            return false;
        }
    }

    /// <summary>Tries again what failed: the summary, and the rows that couldn't be read.</summary>
    public void Retry()
    {
        source.Refresh();
        foreach (var list in DiscoverLists.All)
        {
            if (rows[list].Problem == null || !started.ContainsKey(list)) continue;
            rows[list] = rows[list] with { Problem = null, Loading = false };
            started[list] = 0;
            Begin(list);
        }
        Update();
    }

    // ---- what changed

    void Heard(RequestChange change)
    {
        if (gone) return;
        patching = RequestsSources.After(patching, () => Patch(change));
        if (heroKey is { } h && change.About(h.Kind, h.TmdbId)) _ = ReadHero(h);
        // "Your requests", once changes pause.
        if (!hasYours) return;
        yoursSoon?.Dispose();
        yoursSoon = scheduler.After(ChangesMs, () =>
        {
            yoursSoon = null;
            _ = ReadYours();
        });
    }

    /// <summary>Badges in rows already shown follow <paramref name="change"/>'s request.</summary>
    async Task Patch(RequestChange change)
    {
        var shown = rows.Values.SelectMany(p => p.Titles).ToList();
        var patch = await source.PatchFor(change, shown);
        if (patch == null || gone) return;
        foreach (var list in DiscoverLists.All) rows[list] = rows[list].Map(patch);
        Update();
    }

    async Task ReadYours()
    {
        try
        {
            var list = await source.List(new RequestQuery { Mine = true, Limit = YoursCount });
            if (gone || !hasYours) return;
            yours = list.Requests;
        }
        catch (Exception)
        {
            // A shortcut: it stays as it was.
            return;
        }
        // While one of them downloads, they're read again every 10 s: download progress sends no events.
        moving?.Dispose();
        moving = yours.Any(r => r.Moving) ? scheduler.After(MovingMs, () =>
        {
            moving = null;
            _ = ReadYours();
        }) : null;
        Update();
    }

    async Task ReadHero((string Kind, long TmdbId) key)
    {
        try
        {
            var detail = await source.Title(key.Kind, key.TmdbId);
            if (gone || heroKey != key) return;
            heroDetail = detail;
            Update();
        }
        catch (Exception)
        {
            // The hero's action stays as the row says it.
        }
    }

    // ---- the page

    void Update()
    {
        if (gone) return;
        // Trending has nothing this account can ask for: every row is read at once, for a hero.
        var trending = rows[DiscoverList.Trending];
        if (!allRows && trending.Titles.Count == 0 && trending.Settled && trending.Problem == null)
        {
            allRows = true;
            foreach (var list in DiscoverLists.All) ShowRow(list);
        }
        var hero = HeroTitle();
        var key = hero == null ? ((string, long)?)null : (hero.Kind, hero.TmdbId);
        if (key != heroKey)
        {
            heroKey = key;
            heroDetail = key is { } k ? source.CachedTitle(k.Item1, k.Item2) : null;
            if (key is { } read) _ = ReadHero(read);
        }
        State.Value = Build();
    }

    DiscoverState Build()
    {
        var gate = source.State.Value;
        var summary = gate.Summary;
        var allowance = summary == null ? null : Allowance.Of(summary);
        var list = DiscoverLists.All.Select(l => new DiscoverRow(l, rows[l])).ToList();
        var trending = rows[DiscoverList.Trending];
        // Every row came, none with a title, and some failed: no hero can be found, so the page says why, with Retry.
        var failed = list.All(r => r.Pages.Titles.Count == 0 && r.Pages.Settled) ? list.Select(r => r.Pages.Problem).FirstOrDefault(p => p != null) : null;
        var problem = RequestProblem.Of(gate) ?? (trending.Titles.Count == 0 ? trending.Problem : null) ?? failed;
        DiscoverHero? hero = null;
        if (HeroTitle() is { } title)
        {
            var page = heroDetail is { } d && d.Kind == title.Kind && d.TmdbId == title.TmdbId ? d : null;
            hero = new DiscoverHero(title, page != null ? RequestRules.Main(page, gate) : RequestRules.OfTitle(title, gate, allowance), page);
        }
        return new DiscoverState(
            gate,
            source.Origin,
            hero,
            list,
            problem,
            stale,
            yours,
            summary?.Manage == true,
            allowance,
            asking,
            problem == null && list.All(r => r.Pages.Titles.Count == 0 && r.Pages.Settled && r.Pages.Problem == null));
    }

    /// <summary>The hero's title: the first of the first row with any, once the rows before it have come with none, or failed.</summary>
    DiscoverTitle? HeroTitle()
    {
        foreach (var list in DiscoverLists.All)
        {
            var pages = rows[list];
            if (pages.Titles.Count > 0) return pages.Titles[0];
            if (!pages.Settled) return null;
        }
        return null;
    }

    // ---- the hero's action

    /// <summary>The hero's Request more seasons, beside Open: the show's page, with Which seasons? open.</summary>
    public void HeroMore()
    {
        if (State.Value.Hero?.Title is { } title) Tell(new RequestEvent.Open(new PageKey.DiscoverTitle(title.Kind, title.TmdbId, Pick: true)));
    }

    /// <summary>
    /// The hero's Tell me too: following what others asked for, here, as the title's page does; a show
    /// whose page isn't read yet (which says the seasons asked for) opens it instead.
    /// </summary>
    public void HeroTellMeToo()
    {
        if (State.Value.Hero is not { } hero) return;
        var title = hero.Title;
        if (hero.Detail != null) Ask(RequestRules.TellMeToo(hero.Detail), title.Title);
        else if (title.Kind == TitleKind.Movie) Ask(new NewRequest(TitleKind.Movie, title.TmdbId), title.Title);
        else Tell(new RequestEvent.Open(new PageKey.DiscoverTitle(title.Kind, title.TmdbId)));
    }

    /// <summary>The hero's Stop following: the request this account follows for it, as the title's page does.</summary>
    public void HeroStopFollowing()
    {
        if ((State.Value.Hero?.Action as MainAction.Following)?.Active is not { } request || asking) return;
        asking = true;
        Update();
        _ = Run(() => source.Unwatch(request));
    }

    /// <summary>
    /// The hero's main action was pressed: a movie is asked for here; a show opens its page with the
    /// picker; what a library has (some of it) opens there; the rest open the title.
    /// </summary>
    public void HeroAction()
    {
        if (State.Value.Hero is not { } hero) return;
        var title = hero.Title;
        var page = new PageKey.DiscoverTitle(title.Kind, title.TmdbId);
        switch (hero.Action)
        {
            case MainAction.Open open:
                Tell(new RequestEvent.Open(open.ItemId is { } id ? new PageKey.Item(id) : page));
                break;
            case MainAction.Request request:
                if (request.Ask != Pley.Tv.Core.Requests.Ask.Yes) Tell(new RequestEvent.Open(page));
                else if (title.Kind == TitleKind.Movie) Ask(new NewRequest(TitleKind.Movie, title.TmdbId), title.Title);
                else Tell(new RequestEvent.Open(page with { Pick = true }));
                break;
            case MainAction.Partial partial:
                Tell(new RequestEvent.Open(partial.ItemId is { } item ? new PageKey.Item(item) : page with { Pick = partial.More == Pley.Tv.Core.Requests.Ask.Yes }));
                break;
            case MainAction.TellMeToo tell:
                if (tell.Ask == Pley.Tv.Core.Requests.Ask.Yes && title.Kind == TitleKind.Movie) Ask(new NewRequest(TitleKind.Movie, title.TmdbId), title.Title);
                else Tell(new RequestEvent.Open(page));
                break;
            default:
                Tell(new RequestEvent.Open(page));
                break;
        }
    }

    /// <summary>Asks for <paramref name="request"/>, once at a time: a second press while it's on its way does nothing.</summary>
    void Ask(NewRequest request, string title)
    {
        if (asking) return;
        asking = true;
        Update();
        _ = Run(async () =>
        {
            var asked = await source.Ask(request);
            if (!gone) Tell(new RequestEvent.Asked(asked, title));
        });
    }

    async Task Run(Func<Task> work)
    {
        try
        {
            await work();
        }
        catch (Exception e)
        {
            if (!gone) Tell(new RequestEvent.Failed(RequestProblem.Of(e)));
        }
        finally
        {
            asking = false;
            Update();
        }
    }

    void Tell(RequestEvent e) => Told?.Invoke(e);
}

/// <summary>What a grid of TMDB's titles shows: what TMDB finds for some words, or one of Discover's lists whole.</summary>
public abstract record DiscoverSource
{
    /// <summary>Every title TMDB finds for <paramref name="Words"/>, those a library has too (search's Discover row's See all).</summary>
    public sealed record Words(string Value) : DiscoverSource;

    /// <summary>Every title of <paramref name="List"/> (a Discover row's See all).</summary>
    public sealed record Row(DiscoverList List) : DiscoverSource;
}

/// <summary>TMDB's titles for the words searched, or of one of Discover's lists.</summary>
public abstract record DiscoverSearch
{
    public sealed record Loading : DiscoverSearch;
    public sealed record Ready(Pages Pages) : DiscoverSearch;
    public sealed record Empty : DiscoverSearch;
    public sealed record Failed(RequestProblem Problem) : DiscoverSearch;
}

/// <summary>The page of a grid of TMDB's titles.</summary>
/// <param name="Query">What is searched for: the words as the server takes them; empty for a list.</param>
/// <param name="Problem">Why the page can't show: signed in before requests, requests off, no access.</param>
/// <param name="List">The list shown whole, for a Discover row's See all; null for search's.</param>
public sealed record DiscoverGridState(string Query, RequestsState Gate, string Origin, RequestProblem? Problem, DiscoverSearch Search, DiscoverList? List);

/// <summary>
/// A grid of TMDB's titles (the Android app's DiscoverResultsViewModel, as the TV uses it): every
/// title TMDB finds for the words, or a Discover row's whole list, each with its badge. It reads
/// page by page until a screenful, <see cref="TvFill"/> pages at most, then as focus nears the
/// grid's end (<see cref="Want"/>) the same, stopping short until Show more
/// (<see cref="ShowMore"/>). Badges follow requests_changed without reading the results again.
/// </summary>
public sealed class DiscoverGridModel : IDisposable
{
    /// <summary>Pages a TV's grid reads at a time, at most, before it offers Show more.</summary>
    public const int TvFill = 5;

    readonly IRequestsSource source;
    readonly DiscoverSource what;
    readonly int screenful;
    readonly List<IDisposable> watches = new List<IDisposable>();
    DiscoverSearch search = new DiscoverSearch.Loading();
    int wanted, run;
    Task patching = Task.CompletedTask;
    bool gone, open;

    /// <param name="screenful">The titles read before the first answer shows whole: four rows of the grid.</param>
    public DiscoverGridModel(IRequestsSource source, DiscoverSource what, int screenful)
    {
        this.source = source;
        this.what = what;
        this.screenful = screenful;
        wanted = screenful;
        Query = what is DiscoverSource.Words w ? RequestRules.SearchText(w.Value) : "";
        State = new Signal<DiscoverGridState>(Build());
        watches.Add(source.State.Watch(gate =>
        {
            // TMDB is asked only while asking works.
            if (gate.MayAsk && !open)
            {
                open = true;
                _ = SearchFor();
            }
            else if (!gate.MayAsk) open = false;
            Update();
        }));
        source.Changed += Heard;
    }

    public string Query { get; }

    public Signal<DiscoverGridState> State { get; }

    public void Dispose()
    {
        gone = true;
        run++;
        foreach (var w in watches) w.Dispose();
        source.Changed -= Heard;
    }

    Task<DiscoverPage> PageAt(int n) => what is DiscoverSource.Row row ? source.Page(row.List, n) : source.Search(Query, n);

    async Task SearchFor()
    {
        var mine = ++run;
        if (what is DiscoverSource.Words && Query.Length == 0)
        {
            Set(new DiscoverSearch.Empty());
            return;
        }
        wanted = screenful;
        Set(new DiscoverSearch.Loading());
        await FillTo(Pages.None, screenful, TvFill, mine);
    }

    /// <summary>
    /// Reads the pages after <paramref name="start"/>'s while fewer than <paramref name="want"/>
    /// titles came, <paramref name="pages"/> of them at most; short of it with more to read, it
    /// stops there (<see cref="Pages.Stopped"/>).
    /// </summary>
    async Task FillTo(Pages start, int want, int pages, int mine)
    {
        var shown = start;
        var read = 0;
        while (shown.More && shown.Titles.Count < want)
        {
            if (read == pages)
            {
                shown = shown with { Stopped = true };
                break;
            }
            if (shown.Page > 0) Set(new DiscoverSearch.Ready(shown with { Loading = true }));
            DiscoverPage answer;
            try
            {
                answer = await PageAt(shown.Page + 1);
            }
            catch (Exception e)
            {
                if (mine != run) return;
                // What came before shows, with why the rest didn't; with nothing, why.
                var problem = RequestProblem.Of(e);
                Set(shown.Titles.Count == 0 ? new DiscoverSearch.Failed(problem) : new DiscoverSearch.Ready(shown with { Problem = problem }));
                return;
            }
            if (mine != run) return;
            // Badges that followed a request while the page was read: the titles as they show now.
            if (search is DiscoverSearch.Ready { Pages: { } now } && now.Page == shown.Page) shown = shown with { Titles = now.Titles };
            shown = shown.With(answer);
            read++;
        }
        Set(shown.Titles.Count == 0 && !shown.More ? new DiscoverSearch.Empty() : new DiscoverSearch.Ready(shown));
    }

    /// <summary>The grid wants <paramref name="count"/> titles (focus came near its end): pages are read until it has them, unless the reading stopped short.</summary>
    public void Want(int count)
    {
        wanted = Math.Max(wanted, count);
        if (search is DiscoverSearch.Ready { Pages: { Loading: false, Stopped: false, Problem: null } pages } && pages.More && pages.Titles.Count < wanted)
            _ = FillTo(pages, wanted, TvFill, run);
    }

    /// <summary>Show more, or Retry, at the grid's end: the reading goes on past where it stopped short or failed.</summary>
    public void ShowMore()
    {
        if (search is not DiscoverSearch.Ready { Pages: { Loading: false } pages }) return;
        wanted = Math.Max(wanted, pages.Titles.Count + 1);
        _ = FillTo(pages with { Stopped = false, Problem = null }, wanted, TvFill, run);
    }

    /// <summary>Tries again: the summary, and the search from its first page.</summary>
    public void Retry()
    {
        source.Refresh();
        if (source.State.Value.MayAsk) _ = SearchFor();
    }

    void Heard(RequestChange change)
    {
        if (!gone) patching = RequestsSources.After(patching, () => Patch(change));
    }

    /// <summary>Badges in the results already shown follow <paramref name="change"/>'s request.</summary>
    async Task Patch(RequestChange change)
    {
        if (search is not DiscoverSearch.Ready ready) return;
        var patch = await source.PatchFor(change, ready.Pages.Titles);
        if (patch == null || gone) return;
        if (search is DiscoverSearch.Ready now) Set(new DiscoverSearch.Ready(now.Pages.Map(patch)));
    }

    void Set(DiscoverSearch next)
    {
        search = next;
        Update();
    }

    void Update()
    {
        if (!gone) State.Value = Build();
    }

    DiscoverGridState Build()
    {
        var gate = source.State.Value;
        return new DiscoverGridState(Query, gate, source.Origin, RequestProblem.Of(gate), search, (what as DiscoverSource.Row)?.List);
    }
}
