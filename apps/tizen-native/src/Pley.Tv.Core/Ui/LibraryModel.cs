using System;
using System.Collections.Generic;
using System.Globalization;
using System.Linq;
using System.Threading.Tasks;
using Pley.Tv.Core.Api;
using Pley.Tv.Core.Auth;

namespace Pley.Tv.Core.Ui;

/// <summary>What library pages read and change: the app's state, or a fake in tests.</summary>
public interface ILibrarySource
{
    /// <summary>The server's origin, which pictures are made whole against.</summary>
    string Origin { get; }

    /// <summary>The libraries this profile sees, as the app holds them; null until known.</summary>
    Signal<IReadOnlyList<Library>?> Libraries { get; }

    /// <summary>The last try at the libraries failed: with none saved, the page says why.</summary>
    Signal<Exception?> LibrariesError { get; }

    /// <summary>This TV's settings: the grids' density.</summary>
    Signal<Settings> Settings { get; }

    void UpdateSettings(Func<Settings, Settings> change);

    /// <summary>Asks the server for the libraries again (Retry); a failure is kept in <see cref="LibrariesError"/>.</summary>
    Task RefreshLibraries();

    /// <summary>A page of <paramref name="libraryId"/>'s items, listed by <paramref name="query"/> (its offset and limit the page's).</summary>
    Task<ItemsPage> Items(long libraryId, LibraryQuery query);

    /// <summary>The genres and years the library's items can be filtered by.</summary>
    Task<FiltersAnswer> Filters(long libraryId);

    /// <summary>The library's rows, for its Recommended tab.</summary>
    Task<IReadOnlyList<Hub>> Hubs(long libraryId);

    Task<IReadOnlyList<Collection>> Collections(long libraryId);

    /// <summary>The view <paramref name="libraryId"/> was left in on this TV (<see cref="LibraryParams.View"/>); null when it never left the default.</summary>
    string? View(long libraryId);

    /// <summary>Remembers <paramref name="view"/> for <paramref name="libraryId"/> ("" is the default view: forgotten).</summary>
    void KeepView(long libraryId, string view);

    /// <summary>Calls <paramref name="changed"/> when what pages show may have changed (the server's events, a playback that ended).</summary>
    IDisposable OnChange(Action changed);
}

/// <summary>What the Libraries page reads and changes: the app's state, or a fake in tests.</summary>
public interface ILibrariesSource
{
    Signal<IReadOnlyList<Library>?> Libraries { get; }

    Signal<Exception?> LibrariesError { get; }

    /// <summary>The libraries being scanned now.</summary>
    Signal<IReadOnlyList<ScanState>> Scans { get; }

    /// <summary>Who is watching, and what the sign-in may do (<see cref="LibrariesRules.MayChange"/>).</summary>
    Signal<User?> Me { get; }

    /// <summary>Asks the server for the libraries again; a failure is kept in <see cref="LibrariesError"/>.</summary>
    Task RefreshLibraries();

    /// <summary>The profile's own order of its libraries (each once), or the order everyone sees (null); refused, it throws.</summary>
    Task OrderLibraries(IReadOnlyList<long>? ids);
}

/// <summary>Library pages' source in the app: <see cref="AppState"/> and its API.</summary>
public sealed class AppLibrarySource : ILibrarySource, ILibrariesSource
{
    /// <summary>How each library was left on this TV, per server, profile and library.</summary>
    const string ViewKey = "pley.libraryView";

    readonly AppState app;

    public AppLibrarySource(AppState app) => this.app = app;

    Api.Api Api => app.Api.Value ?? throw new SignedOutException();

    public string Origin => app.Server.Value ?? "";

    public Signal<IReadOnlyList<Library>?> Libraries => app.Libraries;

    public Signal<Exception?> LibrariesError => app.AccountError;

    public Signal<Settings> Settings => app.Settings;

    public Signal<IReadOnlyList<ScanState>> Scans => app.Scans;

    public Signal<User?> Me => app.Me;

    public void UpdateSettings(Func<Settings, Settings> change) => app.UpdateSettings(change);

    public async Task RefreshLibraries()
    {
        try
        {
            await app.LoadAccount();
        }
        catch (Exception)
        {
            // Kept in AccountError, which the pages show.
        }
    }

    public Task<ItemsPage> Items(long libraryId, LibraryQuery query) => Answered(() => Api.LibraryItems(libraryId, query));

    public Task<FiltersAnswer> Filters(long libraryId) => Answered(() => Api.LibraryFilters(libraryId));

    public async Task<IReadOnlyList<Hub>> Hubs(long libraryId) => (await Answered(() => Api.LibraryHubs(libraryId))).Hubs;

    public async Task<IReadOnlyList<Collection>> Collections(long libraryId) => (await Answered(() => Api.Collections(libraryId))).Collections;

    public Task OrderLibraries(IReadOnlyList<long>? ids) => app.OrderLibraries(ids);

    public string? View(long libraryId) => ViewKeyOf(libraryId) is { } key ? app.Platform.Storage.Get(key) : null;

    public void KeepView(long libraryId, string view)
    {
        // Without a profile (signed out, not known yet) nothing is kept.
        if (ViewKeyOf(libraryId) is not { } key) return;
        if (view.Length == 0) app.Platform.Storage.Remove(key);
        else app.Platform.Storage.Set(key, view);
    }

    string? ViewKeyOf(long libraryId)
    {
        var server = app.Server.Value;
        var me = app.Me.Value?.Id;
        if (server == null || me == null) return null;
        return string.Join(".", ViewKey, server, me.Value.ToString(CultureInfo.InvariantCulture), libraryId.ToString(CultureInfo.InvariantCulture));
    }

    public IDisposable OnChange(Action changed)
    {
        // Watch calls at once with the count as it is: only the changes after count.
        var first = true;
        return app.Changes.Watch(_ =>
        {
            if (first) first = false;
            else changed();
        });
    }

    /// <summary>
    /// <paramref name="call"/>'s answer; the server out of reach shows as such, and an answer
    /// after it was brings the pages back from what was saved.
    /// </summary>
    async Task<T> Answered<T>(Func<Task<T>> call)
    {
        try
        {
            var value = await call();
            app.ServerAnswered();
            return value;
        }
        catch (Exception e) when (Errors.Unreachable(e))
        {
            app.ServerAway();
            throw;
        }
    }
}

/// <summary>
/// One thing a page reads: asked again, it keeps what it showed until the answer comes, and only
/// the latest ask's answer counts.
/// </summary>
internal sealed class Fetch<T>
{
    readonly IScheduler scheduler;
    readonly Action changed;
    int asks;

    public Fetch(IScheduler scheduler, Action changed)
    {
        this.scheduler = scheduler;
        this.changed = changed;
    }

    public bool Asked { get; private set; }

    public Loadable<T> State { get; private set; } = new Loadable<T>.Loading();

    public void Ask(Func<Task<T>> task)
    {
        Asked = true;
        var at = ++asks;
        Plays.When(scheduler, task, value =>
        {
            if (at != asks) return;
            State = new Loadable<T>.Ready(value);
            changed();
        }, e =>
        {
            if (at != asks) return;
            State = new Loadable<T>.Failed(e, State.Value);
            changed();
        });
    }

    /// <summary>Answers still on their way are dropped.</summary>
    public void Drop() => asks++;
}

/// <summary>
/// One library listed one way (<see cref="Query"/>), as one list for the grid (the Android app's
/// LibraryListing):
///
/// - the first <see cref="Head"/> items first, for the first frame;
/// - then pages of <see cref="Page"/>, <see cref="Parallel"/> at a time, each pair making a new
///   list with everything so far;
/// - above the window's threshold (<see cref="WindowAbove"/>, <see cref="WindowAboveSmall"/> on
///   the smallest TVs), only a window of two pages around what the grid shows
///   (<see cref="Showing"/>), the last <see cref="KeptPages"/> pages read kept for scrolling back.
/// </summary>
internal sealed class LibraryListing : IDisposable
{
    /// <summary>Items asked for first, for the first frame.</summary>
    public const int Head = 200;

    /// <summary>Items per page after the head.</summary>
    public const int Page = 1_000;

    /// <summary>Pages asked for at once.</summary>
    public const int Parallel = 2;

    /// <summary>Above this many items a listing is windowed.</summary>
    public const int WindowAbove = 5_000;

    /// <summary>Above this many on the smallest TVs.</summary>
    public const int WindowAboveSmall = 2_000;

    /// <summary>Pages of a windowed listing kept in memory: the window and the two before it.</summary>
    public const int KeptPages = 4;

    readonly ILibrarySource source;
    readonly long libraryId;
    readonly IScheduler scheduler;
    readonly int windowAbove;
    readonly Action changed;
    readonly Dictionary<int, IReadOnlyList<ItemSummary>> pages = new Dictionary<int, IReadOnlyList<ItemSummary>>();
    /// <summary>The pages held, least recently used first.</summary>
    readonly List<int> recency = new List<int>();
    /// <summary>The position the grid shows.</summary>
    int viewport;
    /// <summary>Set once the listing is known to be windowed: its total, and which load it is of.</summary>
    (int Total, int Generation)? window;
    (int Generation, int First)? shownWindow;
    int loads, windows, moves;
    bool loading, gone;

    public LibraryListing(ILibrarySource source, long libraryId, LibraryQuery query, IScheduler scheduler, int windowAbove, Action changed)
    {
        this.source = source;
        this.libraryId = libraryId;
        Query = query;
        this.scheduler = scheduler;
        this.windowAbove = windowAbove;
        this.changed = changed;
    }

    public LibraryQuery Query { get; }

    public Loadable<LibraryItems> State { get; private set; } = new Loadable<LibraryItems>.Loading();

    /// <summary>Reads the listing, or again: a windowed one keeps its window shown until the new one replaces it.</summary>
    public void Load()
    {
        if (loading || gone) return;
        loading = true;
        var at = ++loads;
        Plays.When(scheduler, () => source.Items(libraryId, Query with { Offset = 0, Limit = Head }), head =>
        {
            if (gone || at != loads) return;
            var total = TotalOf(head);
            if (total > windowAbove)
            {
                pages.Clear();
                recency.Clear();
                windows++;
                // The head shows until the window around the grid's position is read.
                var shown = State.Value;
                if (shown == null || !shown.Windowed || shown.Items.Count <= Head) Set(new Loadable<LibraryItems>.Ready(new LibraryItems(total, head.Items, Windowed: true)));
                window = (total, windows);
                loading = false;
                MoveWindow();
            }
            else
            {
                window = null;
                Set(new Loadable<LibraryItems>.Ready(new LibraryItems(total, head.Items)));
                ReadAll(head.Items, total, head.Items.Count, at);
            }
        }, e =>
        {
            if (gone || at != loads) return;
            loading = false;
            Fail(e);
        });
    }

    /// <summary>The grid shows <paramref name="position"/> now: a windowed listing moves its window there. Cheap.</summary>
    public void Showing(int position)
    {
        viewport = Math.Max(0, position);
        MoveWindow();
    }

    public void Dispose() => gone = true;

    /// <summary>Every item, after the head: pages two at a time, until the listing's end.</summary>
    void ReadAll(IReadOnlyList<ItemSummary> items, int total, int offset, int at)
    {
        if (offset >= total)
        {
            loading = false;
            return;
        }
        var offsets = Enumerable.Range(0, Parallel).Select(i => offset + i * Page).Where(o => o < total).ToList();
        Plays.When(scheduler, () => Task.WhenAll(offsets.Select(o => source.Items(libraryId, Query with { Offset = o, Limit = Page }))), read =>
        {
            if (gone || at != loads) return;
            var next = new List<ItemSummary>(items.Count + read.Sum(p => p.Items.Count));
            next.AddRange(items);
            foreach (var p in read) next.AddRange(p.Items);
            var now = TotalOf(read[read.Length - 1]);
            Set(new Loadable<LibraryItems>.Ready(new LibraryItems(now, next)));
            // The listing got shorter while it was read: what came is all.
            if (read.Any(p => p.Items.Count < Page))
            {
                loading = false;
                return;
            }
            ReadAll(next, now, offsets[offsets.Count - 1] + Page, at);
        }, e =>
        {
            if (gone || at != loads) return;
            loading = false;
            Fail(e);
        });
    }

    void MoveWindow()
    {
        if (gone || window is not { } w) return;
        // A newer position takes over from one still being read.
        var move = ++moves;
        var lastPage = Math.Max(0, (w.Total - 1) / Page);
        var page = Math.Min(viewport / Page, lastPage);
        var first = Math.Max(0, Math.Min(viewport % Page < Page / 2 ? page - 1 : page, Math.Max(0, lastPage - 1)));
        var wanted = new[] { first, first + 1 }.Where(p => p <= lastPage).ToList();
        if (shownWindow == (w.Generation, first) && State is Loadable<LibraryItems>.Ready) return;
        var missing = wanted.Where(p => !pages.ContainsKey(p)).ToList();
        if (missing.Count == 0)
        {
            Publish(w, first, wanted);
            return;
        }
        Plays.When(scheduler, () => Task.WhenAll(missing.Select(p => source.Items(libraryId, Query with { Offset = p * Page, Limit = Page }))), read =>
        {
            if (gone || windows != w.Generation) return;
            for (var i = 0; i < missing.Count; i++)
            {
                pages[missing[i]] = read[i].Items;
                Touch(missing[i]);
            }
            if (move == moves) Publish(w, first, wanted);
        }, e =>
        {
            if (gone || windows != w.Generation || move != moves) return;
            Fail(e);
        });
    }

    void Publish((int Total, int Generation) w, int first, IReadOnlyList<int> wanted)
    {
        var list = new List<ItemSummary>(wanted.Count * Page);
        foreach (var p in wanted)
        {
            Touch(p);
            if (pages.TryGetValue(p, out var items)) list.AddRange(items);
        }
        while (pages.Count > KeptPages)
        {
            var eldest = recency.FirstOrDefault(p => !wanted.Contains(p));
            if (!pages.Remove(eldest)) break;
            recency.Remove(eldest);
        }
        shownWindow = (w.Generation, first);
        Set(new Loadable<LibraryItems>.Ready(new LibraryItems(w.Total, list, first * Page, Windowed: true)));
    }

    void Touch(int page)
    {
        recency.Remove(page);
        recency.Add(page);
    }

    void Set(Loadable<LibraryItems> state)
    {
        State = state;
        changed();
    }

    void Fail(Exception e) => Set(new Loadable<LibraryItems>.Failed(e, State.Value));

    static int TotalOf(ItemsPage page) => (int)Math.Max(0, Math.Min(int.MaxValue, page.Total));
}

/// <summary>
/// One library's page (the Android app's LibraryViewModel, as the TV uses it):
///
/// - **Recommended:** the library's hubs, then its collections.
/// - **Library:** every item, in a grid sized from the listing's total; sorted, ordered and
///   filtered by watched state, genre and year as the web does, with the A to Z when sorted by
///   title. Sorted by bitrate, tiles say what the files are like. A big library is read as a
///   window that follows the grid (<see cref="Showing"/>).
/// - **Collections:** the library's collections (not for music).
///
/// A page opened anew starts with the sort and filters the library was left in on this TV, on
/// its tab, or without one on the tab it was left on (Library the first time); opened with a
/// view (a row's See all), it starts in that instead. A page the back stack brings back hands
/// its own view back as <c>saved</c> (<see cref="LibraryParams.ToQueryString"/> of the state's
/// params). Each tab reads only once it has been shown. The page asks again when the app hears
/// that things changed.
/// </summary>
public sealed class LibraryModel : IDisposable
{
    readonly ILibrarySource source;
    readonly IScheduler scheduler;
    readonly int windowAbove;
    readonly List<IDisposable> watches = new List<IDisposable>();
    readonly HashSet<LibraryTab> visited = new HashSet<LibraryTab>();
    readonly Fetch<FiltersAnswer> filters;
    readonly Fetch<IReadOnlyList<Hub>> hubs;
    readonly Fetch<IReadOnlyList<Collection>> collections;
    readonly LibraryTab openedOn;
    LibraryParams chosen;
    /// <summary>The view kept for the library last: only a change is kept.</summary>
    string kept;
    LibraryListing? listing;
    (Loadable<LibraryItems> From, TileLook Look, string Origin, LibraryGrid Grid)? grid;
    (Loadable<IReadOnlyList<Collection>> From, string? Kind, string Origin, Loadable<IReadOnlyList<CollectionTile>> Tiles)? shelf;
    (Loadable<IReadOnlyList<Hub>> From, Loadable<IReadOnlyList<CollectionTile>> Shelf, string? Kind, string Origin, Loadable<IReadOnlyList<LibraryRow>> Rows)? rows;
    bool computing, again, reloadPosted, gone;

    /// <param name="tab">The tab a route opens on; null opens on the one the library was left on.</param>
    /// <param name="view">A row's See all ("sort=added&amp;order=desc"): the page starts in it, whatever the library was left in.</param>
    /// <param name="saved">The page's own view, when the back stack brings it back.</param>
    /// <param name="windowAbove">Above this many items a listing is read as a window (<see cref="SmallWindowAbove"/> on the smallest TVs).</param>
    public LibraryModel(ILibrarySource source, long libraryId, IScheduler scheduler, LibraryTab? tab = null, string? view = null, string? saved = null,
        int windowAbove = DefaultWindowAbove)
    {
        this.source = source;
        this.scheduler = scheduler;
        this.windowAbove = windowAbove;
        LibraryId = libraryId;
        filters = new Fetch<FiltersAnswer>(scheduler, Compute);
        hubs = new Fetch<IReadOnlyList<Hub>>(scheduler, Compute);
        collections = new Fetch<IReadOnlyList<Collection>>(scheduler, Compute);
        var opening = tab ?? LibraryTab.Library;
        var given = saved ?? view;
        chosen = given != null ? LibraryParams.Parse(given, opening) : new LibraryParams(opening);
        if (given == null && source.View(libraryId) is { Length: > 0 } left)
        {
            // A page opened on a tab stays on it, in the sort and filters the library was left in.
            var leftIn = LibraryParams.Parse(left, opening);
            chosen = tab != null ? leftIn with { Tab = tab.Value } : leftIn;
        }
        openedOn = chosen.Tab;
        kept = chosen.View();
        watches.Add(source.Libraries.Watch(_ => Compute()));
        watches.Add(source.LibrariesError.Watch(_ => Compute()));
        watches.Add(source.Settings.Watch(_ => Compute()));
        watches.Add(source.OnChange(() =>
        {
            // Changes come in bursts: the page asks again once for them.
            if (reloadPosted) return;
            reloadPosted = true;
            scheduler.Post(() =>
            {
                reloadPosted = false;
                Reload();
            });
        }));
    }

    /// <summary>As the key opens it: <see cref="PageKey.Library.Tab"/> and <see cref="PageKey.Library.View"/>.</summary>
    public LibraryModel(ILibrarySource source, PageKey.Library key, IScheduler scheduler, string? saved = null, int windowAbove = DefaultWindowAbove)
        : this(source, key.LibraryId, scheduler, key.Tab, key.View, saved, windowAbove)
    {
    }

    /// <summary>Above this many items a listing is read as a window around what the grid shows.</summary>
    public const int DefaultWindowAbove = LibraryListing.WindowAbove;

    /// <summary>The threshold on the smallest TVs.</summary>
    public const int SmallWindowAbove = LibraryListing.WindowAboveSmall;

    public long LibraryId { get; }

    public Signal<LibraryState> State { get; } = new Signal<LibraryState>(new LibraryState());

    /// <summary>Shows <paramref name="tab"/>; its content starts reading the first time.</summary>
    public void Select(LibraryTab tab) => Choose(p => p with { Tab = tab });

    /// <summary>A Recommended row's See all (<see cref="LibraryRow.SeeAll"/>): the Library tab in that view, without filters, as the web's link to it.</summary>
    public void SeeAll(string view) => Choose(_ => LibraryParams.Parse(view, LibraryTab.Library));

    /// <summary>Sorts by <paramref name="sort"/> in its own order (titles A to Z, the rest newest or best first).</summary>
    public void SortBy(string sort) => Choose(p => p.Sort == sort ? p : p.SortedBy(sort));

    /// <summary>"asc" or "desc".</summary>
    public void SetOrder(string order) => Choose(p => p with { Order = order });

    public void FlipOrder() => Choose(p => p.Flipped());

    /// <summary>False unwatched only, true watched only, null both.</summary>
    public void SetWatched(bool? watched) => Choose(p => p with { Watched = watched });

    /// <summary>The Unwatched chip: unwatched only, or both again.</summary>
    public void ToggleUnwatched() => SetWatched(State.Value.Params.Watched == false ? null : false);

    public void SetGenre(string? genre) => Choose(p => p with { Genre = genre });

    public void SetYear(int? year) => Choose(p => p with { Year = year });

    public void ClearFilters() => Choose(p => p.Cleared());

    /// <summary>Cards smaller or larger, for every grid on this TV.</summary>
    public void SetDensity(GridDensity density) => source.UpdateSettings(s => s with { GridDensity = density });

    /// <summary>Reads what failed again (and everything else the page shows).</summary>
    public void Retry()
    {
        _ = source.RefreshLibraries();
        Reload();
    }

    /// <summary>
    /// The grid shows <paramref name="position"/> now (its first visible cell): a big library's
    /// window follows it. Cheap; call it as the grid scrolls.
    /// </summary>
    public void Showing(int position) => listing?.Showing(position);

    /// <summary>"Save as…": the smart playlist page, with the listing's filters in Plex's syntax.</summary>
    public PageKey SaveAs() => new SaveSmartPlaylistPage(LibraryId, SmartFilter.Of(State.Value.Params));

    /// <summary>A panel's row chosen (<see cref="LibraryText.Panel"/>): applies it, and says what the panel does next.</summary>
    public PanelOutcome Choose(LibraryPanelRow row)
    {
        switch (row.Kind)
        {
            case LibraryRowKind.Sort when row.Value != null:
                SortBy(row.Value);
                return new PanelOutcome.Close();
            case LibraryRowKind.Order when row.Value != null:
                SetOrder(row.Value);
                return new PanelOutcome.Close();
            case LibraryRowKind.Show:
                SetWatched(row.Value switch
                {
                    "true" => true,
                    "false" => false,
                    _ => null,
                });
                return new PanelOutcome.Stay();
            case LibraryRowKind.Genres:
                return new PanelOutcome.Show(LibraryPanel.Genre);
            case LibraryRowKind.Years:
                return new PanelOutcome.Show(LibraryPanel.Year);
            case LibraryRowKind.ClearFilters:
                ClearFilters();
                return new PanelOutcome.Close();
            case LibraryRowKind.SaveAs:
                return new PanelOutcome.Open(SaveAs());
            case LibraryRowKind.Genre:
                SetGenre(row.Value);
                return new PanelOutcome.Close();
            case LibraryRowKind.Year:
                SetYear(int.TryParse(row.Value, NumberStyles.None, CultureInfo.InvariantCulture, out var year) ? year : null);
                return new PanelOutcome.Close();
            default:
                return new PanelOutcome.Stay();
        }
    }

    public void Dispose()
    {
        gone = true;
        foreach (var w in watches) w.Dispose();
        watches.Clear();
        listing?.Dispose();
        filters.Drop();
        hubs.Drop();
        collections.Drop();
    }

    /// <summary>What is chosen is shown, and is how the library opens next time.</summary>
    void Choose(Func<LibraryParams, LibraryParams> change)
    {
        var next = change(chosen);
        if (next == chosen) return;
        chosen = next;
        var view = chosen.View();
        if (view != kept)
        {
            kept = view;
            source.KeepView(LibraryId, view);
        }
        Compute();
    }

    /// <summary>Everything shown is asked for again, while the page shows what it has.</summary>
    void Reload()
    {
        if (gone) return;
        listing?.Load();
        if (filters.Asked) filters.Ask(() => source.Filters(LibraryId));
        if (hubs.Asked) hubs.Ask(() => source.Hubs(LibraryId));
        if (collections.Asked) collections.Ask(() => source.Collections(LibraryId));
        Compute();
    }

    /// <summary>Works the state out again; asked while it already is (an answer that came at once), it goes round once more.</summary>
    void Compute()
    {
        if (gone) return;
        if (computing)
        {
            again = true;
            return;
        }
        computing = true;
        try
        {
            do
            {
                again = false;
                State.Value = StateNow();
            }
            while (again);
        }
        finally
        {
            computing = false;
        }
    }

    LibraryState StateNow()
    {
        var libraries = source.Libraries.Value;
        var error = source.LibrariesError.Value;
        Library? lib = null;
        foreach (var l in libraries ?? Array.Empty<Library>())
            if (l.Id == LibraryId) lib = l;
        var kind = lib?.Kind;
        var shown = lib != null ? chosen.ValidFor(kind) : chosen;
        visited.Add(shown.Tab);
        var origin = source.Origin;
        var music = kind == "music";

        var grid = new LibraryGrid(new Loadable<GridCells>.Loading(), TileLook.Poster, null);
        if (lib != null && visited.Contains(LibraryTab.Library))
        {
            var query = shown.Query();
            if (listing == null || listing.Query != query)
            {
                listing?.Dispose();
                listing = new LibraryListing(source, LibraryId, query, scheduler, windowAbove, Compute);
                this.grid = null;
                listing.Load();
            }
            grid = GridOf(listing.State, LibraryRules.GridLook(kind, shown.Sort), origin, shown.Sort);
            if (!music && !filters.Asked) filters.Ask(() => source.Filters(LibraryId));
        }

        Loadable<IReadOnlyList<CollectionTile>> shelfNow = new Loadable<IReadOnlyList<CollectionTile>>.Loading();
        if (lib != null && music) shelfNow = new Loadable<IReadOnlyList<CollectionTile>>.Ready(Array.Empty<CollectionTile>());
        else if (lib != null && (visited.Contains(LibraryTab.Collections) || visited.Contains(LibraryTab.Recommended)))
        {
            if (!collections.Asked) collections.Ask(() => source.Collections(LibraryId));
            shelfNow = ShelfOf(collections.State, kind, origin);
        }

        Loadable<IReadOnlyList<LibraryRow>> recommended = new Loadable<IReadOnlyList<LibraryRow>>.Loading();
        if (lib != null && visited.Contains(LibraryTab.Recommended))
        {
            if (!hubs.Asked) hubs.Ask(() => source.Hubs(LibraryId));
            recommended = RowsOf(hubs.State, shelfNow, kind, origin);
        }

        return new LibraryState
        {
            LibraryId = LibraryId,
            Library = lib,
            Missing = libraries != null && lib == null && error == null,
            LibraryError = lib == null ? error : null,
            Params = shown,
            OpenedOn = openedOn,
            Tabs = LibraryParams.TabsFor(kind),
            Sorts = LibraryParams.SortsFor(kind),
            Filters = !music && filters.State.Value is { } f ? f : new LibraryState().Filters,
            Grid = grid,
            Density = source.Settings.Value.GridDensity,
            Recommended = recommended,
            Collections = shelfNow,
        };
    }

    /// <summary>The grid of <paramref name="from"/>, made again only when the listing, the look or the server changed.</summary>
    LibraryGrid GridOf(Loadable<LibraryItems> from, TileLook look, string origin, string sort)
    {
        if (grid is { } g && ReferenceEquals(g.From, from) && g.Look == look && g.Origin == origin) return g.Grid;
        var before = grid?.Grid.Cells.Value;
        var items = from.Value;
        var cells = items == null ? null : GridCells.Of(items, look, origin, before);
        Loadable<GridCells> loadable = from switch
        {
            Loadable<LibraryItems>.Ready => new Loadable<GridCells>.Ready(cells!),
            Loadable<LibraryItems>.Failed f => new Loadable<GridCells>.Failed(f.Error, cells),
            _ => new Loadable<GridCells>.Loading(),
        };
        // Only a whole listing by title has its A to Z: a window's titles aren't all known.
        var index = sort == LibrarySort.Title && items is { Complete: true } && items.Total > AlphaIndex.MinItems ? AlphaIndex.Of(items.Items) : null;
        var made = new LibraryGrid(loadable, look, index);
        grid = (from, look, origin, made);
        return made;
    }

    Loadable<IReadOnlyList<CollectionTile>> ShelfOf(Loadable<IReadOnlyList<Collection>> from, string? kind, string origin)
    {
        if (shelf is { } s && ReferenceEquals(s.From, from) && s.Kind == kind && s.Origin == origin) return s.Tiles;
        IReadOnlyList<CollectionTile>? Tiles(IReadOnlyList<Collection>? list) =>
            list?.Select(c => new CollectionTile(c.Id, c.ItemCount, LibraryText.CollectionTile(c, kind, origin))).ToList();
        Loadable<IReadOnlyList<CollectionTile>> tiles = from switch
        {
            Loadable<IReadOnlyList<Collection>>.Ready r => new Loadable<IReadOnlyList<CollectionTile>>.Ready(Tiles(r.Data)!),
            Loadable<IReadOnlyList<Collection>>.Failed f => new Loadable<IReadOnlyList<CollectionTile>>.Failed(f.Error, Tiles(f.Stale)),
            _ => new Loadable<IReadOnlyList<CollectionTile>>.Loading(),
        };
        shelf = (from, kind, origin, tiles);
        return tiles;
    }

    Loadable<IReadOnlyList<LibraryRow>> RowsOf(Loadable<IReadOnlyList<Hub>> from, Loadable<IReadOnlyList<CollectionTile>> shelfNow, string? kind, string origin)
    {
        if (rows is { } r && ReferenceEquals(r.From, from) && ReferenceEquals(r.Shelf, shelfNow) && r.Kind == kind && r.Origin == origin) return r.Rows;
        IReadOnlyList<LibraryRow>? Rows(IReadOnlyList<Hub>? list)
        {
            if (list == null) return null;
            var made = list.Where(h => h.Items.Count > 0).Select(h => LibraryRules.Row(h, kind, origin)).ToList();
            var shelfTiles = shelfNow.Value;
            if (shelfTiles is { Count: > 0 })
                made.Add(new LibraryRow(
                    LibraryRow.CollectionsKey,
                    LibraryText.TabCollections,
                    TileLook.Poster,
                    shelfTiles.Select(t => t.Tile).ToList(),
                    shelfTiles.Select(t => (PageKey)new PageKey.Collection(t.Id)).ToList()));
            return made;
        }
        Loadable<IReadOnlyList<LibraryRow>> built = from switch
        {
            Loadable<IReadOnlyList<Hub>>.Ready ready => new Loadable<IReadOnlyList<LibraryRow>>.Ready(Rows(ready.Data)!),
            Loadable<IReadOnlyList<Hub>>.Failed f => new Loadable<IReadOnlyList<LibraryRow>>.Failed(f.Error, Rows(f.Stale)),
            _ => new Loadable<IReadOnlyList<LibraryRow>>.Loading(),
        };
        rows = (from, shelfNow, kind, origin, built);
        return built;
    }
}

/// <summary>
/// The libraries this profile sees (the TV's Libraries page; the Android app's
/// LibrariesViewModel), in the server's order, each with its kind, its name, how much it holds
/// and whether the server is scanning it.
///
/// The profile may put them in its own order, which every list of them follows on all its
/// devices: a move shows at once, and is saved in the order moves were made. On TV, Sort (or
/// Move in a card's actions) starts sorting: OK lifts a card, the arrows move it through the
/// grid, OK drops it there and saves the order, Back puts it back; Done or Back ends sorting.
/// </summary>
public sealed class LibrariesModel : IDisposable
{
    readonly ILibrariesSource source;
    readonly IScheduler scheduler;
    readonly List<IDisposable> watches = new List<IDisposable>();
    /// <summary>The libraries' ids in the order shown while moves are made or saved; null: the server's.</summary>
    IReadOnlyList<long>? order;
    bool refreshing, gone;

    public LibrariesModel(ILibrariesSource source, IScheduler scheduler)
    {
        this.source = source;
        this.scheduler = scheduler;
        watches.Add(source.Libraries.Watch(_ => Compute()));
        watches.Add(source.LibrariesError.Watch(_ => Compute()));
        watches.Add(source.Scans.Watch(_ => Compute()));
        watches.Add(source.Me.Watch(_ => Compute()));
    }

    public Signal<LibrariesState> State { get; } = new Signal<LibrariesState>(new LibrariesState.Loading());

    /// <summary>The page is sorting: the title says so, cards show their handles, OK lifts one.</summary>
    public Signal<bool> Sorting { get; } = new Signal<bool>(false);

    /// <summary>The library lifted while sorting; null with none.</summary>
    public Signal<long?> Moving { get; } = new Signal<long?>(null);

    /// <summary>What the page says once: shown as a toast.</summary>
    public event Action<LibrariesEvent>? Told;

    bool Sortable => State.Value is LibrariesState.Ready { Sortable: true };

    /// <summary>Asks the server again (Retry).</summary>
    public void Refresh()
    {
        if (refreshing) return;
        refreshing = true;
        Compute();
        Plays.When(scheduler, () => source.RefreshLibraries(), Done, _ => Done());
        void Done()
        {
            refreshing = false;
            Compute();
        }
    }

    /// <summary>
    /// Moves the library at <paramref name="from"/> to <paramref name="to"/>: shown at once, and
    /// saved unless <paramref name="save"/> is false (sorting on TV saves once, when a card is dropped).
    /// </summary>
    public void Move(int from, int to, bool save = true)
    {
        var ids = ShownOrder();
        if (ids == null || from < 0 || from >= ids.Count || to < 0 || to >= ids.Count || from == to) return;
        var next = ids.ToList();
        var moved = next[from];
        next.RemoveAt(from);
        next.Insert(to, moved);
        order = next;
        Compute();
        if (save) Save(next);
    }

    /// <summary>Moves <paramref name="libraryId"/> <paramref name="by"/> places from where it is shown now.</summary>
    public void MoveBy(long libraryId, int by, bool save = true)
    {
        var from = ShownOrder()?.ToList().IndexOf(libraryId) ?? -1;
        if (from >= 0) Move(from, from + by, save);
    }

    /// <summary>Saves the order shown now (after moves made without saving).</summary>
    public void SaveOrder()
    {
        if (order != null) Save(order);
    }

    /// <summary>Forgets moves not saved: the server's order again.</summary>
    public void CancelMoves()
    {
        order = null;
        Compute();
    }

    /// <summary>The order everyone sees, for this profile again.</summary>
    public void UseServerOrder()
    {
        order = null;
        Compute();
        Plays.When(scheduler, () => source.OrderLibraries(null), () => { }, Refused);
    }

    // ---- sorting on TV

    /// <summary>Sort: the page sorts, when the libraries can be (focus goes to the first card, ready to lift).</summary>
    public void StartSorting()
    {
        if (Sortable) Sorting.Value = true;
    }

    /// <summary>
    /// OK on a card: drops the lifted one; while sorting, lifts it; else the library to open.
    /// </summary>
    public PageKey? Select(long libraryId)
    {
        if (Moving.Value != null)
        {
            Drop();
            return null;
        }
        if (Sorting.Value && Sortable)
        {
            Moving.Value = libraryId;
            return null;
        }
        return new PageKey.Library(libraryId);
    }

    /// <summary>
    /// An arrow while a card is lifted: <paramref name="by"/> is ±1, or ±<see cref="LibrariesRules.Columns"/>
    /// for a row (down onto a shorter last row it lands at its end). Shown at once, saved when dropped.
    /// </summary>
    public void MoveLifted(int by)
    {
        if (Moving.Value is not { } id || ShownOrder() is not { } ids) return;
        var index = ids.ToList().IndexOf(id);
        if (index < 0) return;
        var to = LibrariesRules.MoveTarget(index, by, ids.Count);
        if (to != index) MoveBy(id, to - index, save: false);
    }

    /// <summary>OK on the lifted card: it stays there, and the order is saved.</summary>
    public void Drop()
    {
        SaveOrder();
        Moving.Value = null;
    }

    /// <summary>Back: a lifted card goes back where it was, then sorting ends. False when Back is the page's to handle.</summary>
    public bool Back()
    {
        if (Moving.Value != null)
        {
            CancelMoves();
            Moving.Value = null;
            return true;
        }
        if (Sorting.Value)
        {
            Sorting.Value = false;
            return true;
        }
        return false;
    }

    /// <summary>Done: sorting ends.</summary>
    public void Done() => Sorting.Value = false;

    /// <summary>"Use the server's order" while sorting.</summary>
    public void ServerOrder()
    {
        Moving.Value = null;
        UseServerOrder();
    }

    /// <summary>Move, in a card's actions: the page sorts with that card lifted.</summary>
    public void MoveFromActions(long libraryId)
    {
        if (!Sortable) return;
        Sorting.Value = true;
        Moving.Value = libraryId;
    }

    public void Dispose()
    {
        gone = true;
        foreach (var w in watches) w.Dispose();
        watches.Clear();
    }

    /// <summary>The libraries' ids as shown; null when they can't move.</summary>
    IReadOnlyList<long>? ShownOrder()
    {
        if (State.Value is not LibrariesState.Ready { Sortable: true } ready) return null;
        return ready.Libraries.Select(e => e.Library.Id).ToList();
    }

    void Save(IReadOnlyList<long> ids)
    {
        Plays.When(scheduler, () => source.OrderLibraries(ids), () =>
        {
            // The server's answer holds this order; a newer move keeps its own.
            if (order != null && order.SequenceEqual(ids)) order = null;
            Compute();
        }, e =>
        {
            order = null;
            Compute();
            Refused(e);
        });
    }

    void Refused(Exception e)
    {
        if (gone) return;
        // A server from before PUT /api/me/libraries/order has no such route.
        Told?.Invoke(e is ApiException { Status: 404 or 405 } ? new LibrariesEvent.NeedsNewerServer() : new LibrariesEvent.Failed(e));
    }

    void Compute()
    {
        if (gone) return;
        State.Value = LibrariesRules.StateOf(source.Libraries.Value, source.LibrariesError.Value, source.Scans.Value, refreshing, order,
            LibrariesRules.MayChange(source.Me.Value));
        // Nothing left to sort (a library went, or this sign-in may not): the page as it is.
        if (!Sortable)
        {
            Sorting.Value = false;
            Moving.Value = null;
        }
    }
}
