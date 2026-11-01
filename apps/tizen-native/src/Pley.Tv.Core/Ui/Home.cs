using System;
using System.Collections.Generic;
using System.Globalization;
using System.Linq;
using Pley.Tv.Core.Api;

namespace Pley.Tv.Core.Ui;

/// <summary>How a row's tiles are shaped: the web's hubAspect (apps/web/src/components/HubRow.tsx).</summary>
public enum RowShape
{
    /// <summary>16:9 stills: Continue Watching, and rows of episodes.</summary>
    Landscape,
    /// <summary>Movies, shows and seasons.</summary>
    Poster,
    /// <summary>Music.</summary>
    Square,
}

/// <summary>Where a row's "See all" goes: a library's items, listed as the row is (<c>sort=added&amp;order=desc</c>).</summary>
public sealed record SeeAll(long LibraryId, string View);

/// <summary>One row of Home: a hub of the server's.</summary>
/// <param name="Key">The hub's key ("continue_watching", "recently_added:3"): stable, the row's key on the page.</param>
/// <param name="Title">"Continue Watching", "Recently Added in Movies", as the server says it.</param>
public sealed record HomeRow(string Key, string Title, RowShape Shape, IReadOnlyList<ItemSummary> Items, SeeAll? SeeAll)
{
    public bool ContinueWatching => Key.StartsWith(HomeRules.ContinuePrefix, StringComparison.Ordinal);

    /// <summary>The tile the row's shape draws on TV.</summary>
    public TileKind Kind => Shape switch
    {
        RowShape.Landscape => TileKind.Landscape,
        RowShape.Square => TileKind.Square,
        _ => TileKind.Poster,
    };

    /// <summary>The <paramref name="count"/> items after <paramref name="item"/>, whose details are asked for ahead.</summary>
    public IReadOnlyList<ItemSummary> After(ItemSummary item, int count = 2)
    {
        var at = IndexOf(item);
        return at < 0 ? Array.Empty<ItemSummary>() : Items.Skip(at + 1).Take(count).ToList();
    }

    int IndexOf(ItemSummary item)
    {
        for (var i = 0; i < Items.Count; i++)
            if (Items[i].Id == item.Id) return i;
        return -1;
    }
}

/// <summary>The metadata line: year · content rating · runtime · ★ rating · quality badges, the parts that are known.</summary>
/// <param name="ContentRating">A badge: "TV-14", "PG-13", "15".</param>
/// <param name="Runtime">"58 min", "2h 02m"; for shows "3 seasons".</param>
/// <param name="Rating">The critics' rating, "7.4".</param>
public sealed record HomeMeta(string Year = "", string? ContentRating = null, string Runtime = "", string? Rating = null, IReadOnlyList<string>? Badges = null)
{
    public IReadOnlyList<string> QualityBadges => Badges ?? Array.Empty<string>();

    public bool IsEmpty => Year.Length == 0 && ContentRating == null && Runtime.Length == 0 && Rating == null && QualityBadges.Count == 0;

    public bool Equals(HomeMeta? other) =>
        other != null && Year == other.Year && ContentRating == other.ContentRating && Runtime == other.Runtime && Rating == other.Rating
        && QualityBadges.SequenceEqual(other.QualityBadges);

    public override int GetHashCode() => (Year, ContentRating, Runtime, Rating, QualityBadges.Count).GetHashCode();
}

/// <summary>
/// What the TV says about one item, in its hero (design spec §2.2) or in the info block over the
/// rows (§4.1), with every label made already.
/// </summary>
/// <param name="Title">The title, or for an episode its show's: the episode is in <paramref name="Context"/>.</param>
/// <param name="Context">Episodes: "S2 · E5 · The Long Night"; empty otherwise.</param>
/// <param name="Progress">Played so far, 0 to 1; 0 when not in progress.</param>
/// <param name="TimeLeft">"26 min left" while in progress; empty otherwise.</param>
/// <param name="Video">Movies, episodes and extras, which play themselves.</param>
/// <param name="Complete">Whether the details (summary, content rating, quality) are in.</param>
public sealed record HomeInfo(ItemSummary Item, string Title, string Context, HomeMeta Meta, string Summary, float Progress, string TimeLeft, bool Video, bool Watched, bool Complete)
{
    /// <summary>Started and not finished: Resume, rather than Play.</summary>
    public bool InProgress => Progress > 0f;
}

/// <summary>Home as the screens draw it (design spec §4.1, §8; the Android app's HomeState).</summary>
public abstract record HomeState
{
    /// <summary>Nothing known yet.</summary>
    public sealed record Loading : HomeState;
    /// <summary>Home couldn't be loaded and nothing was saved: "Couldn't load your home screen".</summary>
    public sealed record Failed(Exception Error) : HomeState;
    /// <summary>The profile sees no library: "No libraries yet".</summary>
    public sealed record NoLibraries : HomeState;
    /// <summary>Libraries, but nothing in them yet: "Nothing here yet", or "Scanning your libraries…".</summary>
    public sealed record Empty(bool Scanning) : HomeState;
    /// <param name="Origin">The server's origin, which the rows' picture addresses are on.</param>
    /// <param name="Hero">What the hero shows; null with nothing to show.</param>
    /// <param name="Error">The server couldn't be asked again: what shows is what was saved.</param>
    public sealed record Ready(string Origin, IReadOnlyList<HomeRow> Rows, HomeInfo? Hero, Exception? Error = null) : HomeState;
}

/// <summary>Home's rules (the Android app's HomeState.kt and homeState).</summary>
public static class HomeRules
{
    public const string ContinuePrefix = "continue";

    static readonly (string Prefix, string View)[] SeeAllViews =
    {
        ("recently_added", "sort=added&order=desc"),
        ("recently_released", "sort=release&order=desc"),
        ("top_rated", "sort=rating&order=desc"),
    };

    static readonly HashSet<string> VideoKinds = new HashSet<string> { "movie", "show", "season", "episode" };

    /// <summary>
    /// Home from what the app holds: the rows when there are some, an empty state when there are
    /// none, a skeleton or an error only when nothing was saved.
    /// </summary>
    public static HomeState StateOf(IReadOnlyList<Hub>? hubs, Exception? error, IReadOnlyList<Library>? libraries, bool scanning, string origin)
    {
        if (hubs == null)
        {
            if (libraries != null && libraries.Count == 0) return new HomeState.NoLibraries();
            return error != null ? new HomeState.Failed(error) : new HomeState.Loading();
        }
        var rows = Rows(hubs, libraries);
        if (rows.Count == 0)
        {
            if (libraries != null && libraries.Count == 0) return new HomeState.NoLibraries();
            return new HomeState.Empty(scanning || (libraries?.Any(l => l.Scanning) ?? false));
        }
        var hero = HeroOf(rows);
        return new HomeState.Ready(origin, rows, hero == null ? null : InfoOf(hero, null), error);
    }

    /// <summary>
    /// The rows of <paramref name="hubs"/> that hold something, shaped as the web does: Continue
    /// Watching as stills, music as squares, the rest by their first item.
    /// </summary>
    public static IReadOnlyList<HomeRow> Rows(IReadOnlyList<Hub> hubs, IReadOnlyList<Library>? libraries) =>
        hubs.Where(h => h.Items.Count > 0)
            .Select(h => new HomeRow(h.Key, h.Title, Shape(h, libraries?.FirstOrDefault(l => l.Id == h.LibraryId)?.Kind), h.Items, SeeAllOf(h)))
            .ToList();

    /// <summary>The shape of <paramref name="hub"/>'s tiles: the web's hubAspect, with a music library's rows square.</summary>
    public static RowShape Shape(Hub hub, string? libraryKind = null)
    {
        if (hub.Key.StartsWith(ContinuePrefix, StringComparison.Ordinal)) return RowShape.Landscape;
        if (libraryKind == "music") return RowShape.Square;
        return hub.Items.FirstOrDefault()?.Kind switch
        {
            "episode" => RowShape.Landscape,
            "artist" or "album" or "track" => RowShape.Square,
            _ => RowShape.Poster,
        };
    }

    /// <summary>Where a row's "See all" goes: its library, listed in the row's order; null for rows without one.</summary>
    public static SeeAll? SeeAllOf(Hub hub)
    {
        if (hub.LibraryId == null) return null;
        foreach (var (prefix, view) in SeeAllViews)
            if (hub.Key.StartsWith(prefix, StringComparison.Ordinal)) return new SeeAll(hub.LibraryId.Value, view);
        return null;
    }

    /// <summary>The item the hero shows: the first of Continue Watching, else the newest video in the rows, else the newest item.</summary>
    public static ItemSummary? HeroOf(IReadOnlyList<HomeRow> rows)
    {
        var continuing = rows.FirstOrDefault(r => r.ContinueWatching)?.Items.FirstOrDefault();
        if (continuing != null) return continuing;
        var all = rows.SelectMany(r => r.Items).ToList();
        return Newest(all.Where(i => VideoKinds.Contains(i.Kind))) ?? Newest(all);
    }

    static ItemSummary? Newest(IEnumerable<ItemSummary> items)
    {
        ItemSummary? best = null;
        foreach (var i in items)
            if (best == null || i.AddedAt > best.AddedAt) best = i;
        return best;
    }

    /// <summary>
    /// What the hero or the info block says about <paramref name="item"/>, with
    /// <paramref name="detail"/> once it is in: a movie's summary, the content rating and the
    /// quality badges come only with it.
    /// </summary>
    public static HomeInfo InfoOf(ItemSummary item, ItemDetail? detail)
    {
        var details = detail?.Id == item.Id ? detail : null;
        var episode = item.Kind == "episode";
        var show = episode && !string.IsNullOrEmpty(item.GrandparentTitle) ? item.GrandparentTitle : null;
        var inProgress = Format.IsInProgress(item);
        var runtime = item.Kind is "show" or "season" or "artist" or "album"
            ? Format.CardSubtitle(item)
            : Format.Runtime(item.DurationMs ?? details?.DurationMs);
        var year = (item.Year ?? details?.Year)?.ToString(CultureInfo.InvariantCulture) ?? "";
        var rating = item.Rating ?? details?.Rating;
        return new HomeInfo(
            item,
            show ?? item.Title,
            episode ? string.Join(" · ", new[] { Format.EpisodeLabel(item), item.Title }.Where(s => s.Length > 0)) : "",
            new HomeMeta(
                year,
                string.IsNullOrWhiteSpace(details?.ContentRating) ? null : details!.ContentRating,
                item.Kind == "show" && runtime == year ? "" : runtime,
                rating > 0 ? rating.Value.ToString("0.0", CultureInfo.InvariantCulture) : null,
                details == null ? null : Format.QualityBadges(details.Media)),
            (details?.Summary ?? item.Summary ?? "").Trim(),
            inProgress ? Format.ProgressFraction(item) : 0f,
            inProgress ? Format.TimeLeft(item.UserState.PositionMs, item.DurationMs) : "",
            item.Kind is "movie" or "episode" or "extra",
            item.UserState.Watched,
            details != null);
    }

    /// <summary>The backdrop behind <paramref name="item"/>: its own, else its show's or parent's.</summary>
    public static IReadOnlyList<string> Backdrop(ItemSummary item, string origin)
    {
        var list = new List<string>();
        if (!string.IsNullOrEmpty(item.BackdropUrl)) list.Add(Artwork.Absolute(item.BackdropUrl!, origin));
        if (item.GrandparentId != null) list.Add(Artwork.Absolute($"/api/items/{item.GrandparentId}/images/backdrop", origin));
        if (item.ParentId != null) list.Add(Artwork.Absolute($"/api/items/{item.ParentId}/images/backdrop", origin));
        return list;
    }
}

/// <summary>
/// Home (the Android app's HomeViewModel, as the TV uses it): <see cref="State"/> from what the
/// app holds, saved first, then from the server; <see cref="Info"/> about the tile focus rests on
/// (400 ms), its details fetched as it rests; the hero's details as they come in.
/// </summary>
public sealed class HomeModel : IDisposable
{
    /// <summary>How long focus rests on a tile before the info block and the backdrop follow it.</summary>
    public const long RestMs = 400;

    readonly AppState app;
    readonly IScheduler scheduler;
    readonly List<IDisposable> watches = new List<IDisposable>();
    IDisposable? rest;
    ItemSummary? focused;
    bool wasScanning;
    bool gone;

    public HomeModel(AppState app, IScheduler scheduler)
    {
        this.app = app;
        this.scheduler = scheduler;
        watches.Add(app.Home.Watch(_ => Compute()));
        watches.Add(app.HomeError.Watch(_ => Compute()));
        watches.Add(app.Libraries.Watch(_ => Compute()));
        watches.Add(app.Scans.Watch(scans =>
        {
            // A scan that ends has changed what Home shows: ask once more, whatever the events said.
            var scanning = scans.Count > 0;
            if (wasScanning && !scanning) _ = app.LoadHome();
            wasScanning = scanning;
            Compute();
        }));
    }

    public Signal<HomeState> State { get; } = new Signal<HomeState>(new HomeState.Loading());

    /// <summary>What the info block shows: the item focus rests on, with its details once in; null at the hero.</summary>
    public Signal<HomeInfo?> Info { get; } = new Signal<HomeInfo?>(null);

    /// <summary>Asks the server again for the rows and the libraries (Retry).</summary>
    public void Refresh()
    {
        _ = app.LoadHome();
        _ = app.LoadAccount();
    }

    /// <summary>Focus moved to <paramref name="item"/> (null: the hero): once it rests, the info block follows, and <paramref name="ahead"/>'s details are asked for.</summary>
    public void Focus(ItemSummary? item, IReadOnlyList<ItemSummary>? ahead = null)
    {
        rest?.Dispose();
        rest = null;
        focused = item;
        if (item == null)
        {
            Info.Value = null;
            return;
        }
        rest = scheduler.After(RestMs, () =>
        {
            rest = null;
            if (focused != item) return;
            Info.Value = HomeRules.InfoOf(item, app.CachedItem(item.Id));
            _ = Detail(item, i => focused == i, detail => Info.Value = HomeRules.InfoOf(item, detail));
            foreach (var next in ahead ?? Array.Empty<ItemSummary>()) _ = Detail(next, _ => true, _ => { });
        });
    }

    /// <summary>Marks <paramref name="item"/> watched or not; Home is asked for again after.</summary>
    public async void SetWatched(ItemSummary item, bool watched)
    {
        var api = app.Api.Value;
        if (api == null) return;
        try
        {
            if (watched) await api.MarkWatched(item.Id);
            else await api.MarkUnwatched(item.Id);
            await app.LoadHome();
        }
        catch (Exception)
        {
            // Home shows as it was; the next refresh says how things are.
        }
    }

    public void Dispose()
    {
        gone = true;
        rest?.Dispose();
        foreach (var w in watches) w.Dispose();
        watches.Clear();
    }

    void Compute()
    {
        if (gone) return;
        var state = HomeRules.StateOf(app.Home.Value, app.HomeError.Value, app.Libraries.Value, app.Scans.Value.Count > 0, app.Server.Value ?? "");
        if (state is HomeState.Ready { Hero: { } hero } ready)
        {
            // The hero's details, kept: a new answer for the rows never draws the hero without them for a moment.
            var cached = app.CachedItem(hero.Item.Id);
            if (cached != null) state = ready with { Hero = HomeRules.InfoOf(hero.Item, cached) };
            if (cached == null || app.CachedItemStale(hero.Item.Id))
                _ = Detail(hero.Item, i => State.Value is HomeState.Ready { Hero: { } h } && h.Item.Id == i.Id, detail =>
                {
                    if (State.Value is HomeState.Ready r && r.Hero != null) State.Value = r with { Hero = HomeRules.InfoOf(r.Hero.Item, detail) };
                });
        }
        State.Value = state;
    }

    /// <summary>The details of <paramref name="item"/>, from what is kept unless stale; <paramref name="use"/> hears them while <paramref name="still"/> holds.</summary>
    async System.Threading.Tasks.Task Detail(ItemSummary item, Func<ItemSummary, bool> still, Action<ItemDetail> use)
    {
        if (!app.CachedItemStale(item.Id)) return;
        try
        {
            var detail = await app.Item(item.Id);
            if (!gone && still(item)) use(detail);
        }
        catch (Exception)
        {
            // The info shows what the row knew.
        }
    }
}
