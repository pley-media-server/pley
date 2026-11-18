using System;
using System.Collections.Generic;
using System.Linq;
using System.Threading.Tasks;
using Pley.Tv.Core.Api;
using Pley.Tv.Core.Auth;

namespace Pley.Tv.Core.Ui;

/// <summary>One library's collections, as a row of the Collections page.</summary>
/// <param name="Kind">"movies" or "shows".</param>
public sealed record CollectionShelf(long LibraryId, string LibraryName, string Kind, Loadable<IReadOnlyList<CollectionTile>> Collections)
{
    /// <summary>The row's key, for remembering focus: "library:3".</summary>
    public string Key => $"library:{LibraryId}";

    /// <summary>The tile the row draws: posters.</summary>
    public TileKind TileKind => TileKind.Poster;
}

/// <summary>
/// A collection as its page shows it (apps/web/src/pages/collections/CollectionPage.tsx; the
/// Android app's CollectionPage): the header's facts and the tiles of its items, in its order.
/// </summary>
public sealed record CollectionPage
{
    public long Id { get; init; }
    public string Title { get; init; } = "";
    public string? Summary { get; init; }
    /// <summary>Mirrors a TMDB collection: the "TMDB" badge.</summary>
    public bool Automatic { get; init; }
    /// <summary>Its category's name, a badge after the "Collection" label; null without one, or for Other.</summary>
    public string? Category { get; init; }
    /// <summary>Filled by a filter: the "Smart" badge.</summary>
    public bool Smart { get; init; }
    public long Count { get; init; }
    public long LibraryId { get; init; }
    /// <summary>Null while the libraries aren't known.</summary>
    public string? LibraryName { get; init; }
    /// <summary>"movies" or "shows"; null while the libraries aren't known.</summary>
    public string? Kind { get; init; }
    /// <summary>What it holds and in which order, in words: its filter's ("Genre: Comedy", "Newest first"), else its order ("By release date").</summary>
    public IReadOnlyList<string> Notes { get; init; } = Array.Empty<string>();
    /// <summary>The backdrop's whole address (the collection's backdrop, else its poster); null without either.</summary>
    public string? Backdrop { get; init; }
    public string? Poster { get; init; }
    /// <summary>Its items as the library grid's posters, in its order.</summary>
    public IReadOnlyList<TileModel> Tiles { get; init; } = Array.Empty<TileModel>();
    public IReadOnlyList<ItemSummary> Items { get; init; } = Array.Empty<ItemSummary>();
    /// <summary>Its items are movies, played in its order; else shows, which play their next episode.</summary>
    public bool Movies { get; init; }
    /// <summary>
    /// A Home row of several libraries' collections of one title together (the Android app's
    /// MergedCollectionPage): its facts are how many movies and shows, and <see cref="Sources"/>
    /// name the libraries.
    /// </summary>
    public bool Merged { get; init; }
    public int MovieCount { get; init; }
    public int ShowCount { get; init; }
    /// <summary>A merged page's libraries, in the row's order; those not known yet left out.</summary>
    public IReadOnlyList<MergedSource> Sources { get; init; } = Array.Empty<MergedSource>();

    public bool CanPlay => Items.Count > 0;
    public bool CanShuffle => Items.Count >= 2;

    /// <summary>The grid's columns on TV: the library grid's.</summary>
    public int Columns => LibraryRules.GridColumns(TileKind.LibraryGrid);
}

/// <summary>What happened when Play or Shuffle was pressed, for a message beside the buttons.</summary>
public abstract record CollectionEvent
{
    /// <summary>The show picked has nothing to play yet.</summary>
    public sealed record NothingToPlay : CollectionEvent;

    public sealed record Failed(Exception Error) : CollectionEvent;
}

/// <summary>A collection's page, or the merged page: what its screen shows and does.</summary>
public interface ICollectionPageModel : IDisposable
{
    /// <summary>The page's key on the back stack.</summary>
    PageKey Key { get; }

    Signal<Loadable<CollectionPage>> State { get; }

    /// <summary>Nothing to play, or a failure: said beside the buttons for a few seconds (<see cref="CollectionText.MessageMs"/>).</summary>
    event Action<CollectionEvent>? Told;

    /// <summary>Something to play.</summary>
    event Action<PlayRequest>? PlayRequested;

    void Retry();

    /// <summary>Play (<paramref name="shuffle"/> false) or Shuffle.</summary>
    void Play(bool shuffle);

    /// <summary>Where opening the item at <paramref name="position"/> goes.</summary>
    PageKey? Opens(int position);
}

/// <summary>What the collections' pages read: the app's state, or a fake in tests.</summary>
public interface ICollectionSource
{
    /// <summary>The server's origin, which pictures are made whole against.</summary>
    string Origin { get; }

    /// <summary>The libraries this profile sees, as the app holds them; null until known.</summary>
    Signal<IReadOnlyList<Library>?> Libraries { get; }

    /// <summary>The last try at the libraries failed: with none saved, the page says why.</summary>
    Signal<Exception?> LibrariesError { get; }

    /// <summary>Asks the server for the libraries again (Retry); a failure is kept in <see cref="LibrariesError"/>.</summary>
    Task RefreshLibraries();

    Task<IReadOnlyList<Collection>> Collections(long libraryId);

    Task<CollectionDetail> Collection(long id);

    /// <summary>The collections <paramref name="ids"/> together (a Home row of several libraries').</summary>
    Task<MergedCollection> Merged(IReadOnlyList<long> ids);

    /// <summary>The episode a show's Play starts; null with none.</summary>
    Task<ItemSummary?> OnDeck(long id);

    /// <summary>Calls <paramref name="changed"/> when what pages show may have changed (the server's events).</summary>
    IDisposable OnChange(Action changed);
}

/// <summary>The collections' pages' source in the app: <see cref="AppState"/> and its API.</summary>
public sealed class AppCollectionSource : ICollectionSource
{
    readonly AppState app;

    public AppCollectionSource(AppState app) => this.app = app;

    Api.Api Api => app.Api.Value ?? throw new SignedOutException();

    public string Origin => app.Server.Value ?? "";

    public Signal<IReadOnlyList<Library>?> Libraries => app.Libraries;

    public Signal<Exception?> LibrariesError => app.AccountError;

    public async Task RefreshLibraries()
    {
        try
        {
            await app.LoadAccount();
        }
        catch (Exception)
        {
            // Kept in AccountError, which the page shows.
        }
    }

    public async Task<IReadOnlyList<Collection>> Collections(long libraryId) => (await Api.Collections(libraryId)).Collections;

    public Task<CollectionDetail> Collection(long id) => Api.Collection(id);

    public Task<MergedCollection> Merged(IReadOnlyList<long> ids) => Api.MergedCollection(ids);

    public async Task<ItemSummary?> OnDeck(long id) => (await Api.OnDeck(id)).Item;

    public IDisposable OnChange(Action changed) => AppChanges.Follow(app.Changes, changed);
}

/// <summary>
/// The Collections page (the Android app's CollectionsViewModel): the collections of one library
/// (<c>only</c>), or of every movie and show library, a row each in the server's order of
/// libraries, as the TV rail's Collections shows them. Music has none. It asks again when the
/// app hears that things changed.
/// </summary>
public sealed class CollectionsModel : IDisposable
{
    readonly ICollectionSource source;
    readonly IScheduler scheduler;
    readonly long? only;
    readonly List<IDisposable> watches = new List<IDisposable>();
    readonly Dictionary<long, Fetch<IReadOnlyList<Collection>>> shelves = new Dictionary<long, Fetch<IReadOnlyList<Collection>>>();
    bool gone;

    /// <param name="only">One library's collections; null for every movie and show library's (the TV's Collections page).</param>
    public CollectionsModel(ICollectionSource source, IScheduler scheduler, long? only = null)
    {
        this.source = source;
        this.scheduler = scheduler;
        this.only = only;
        watches.Add(source.Libraries.Watch(_ => Compute()));
        watches.Add(source.LibrariesError.Watch(_ => Compute()));
        watches.Add(source.OnChange(Reload));
    }

    /// <summary>The rows, once the libraries are known; failed when they couldn't be read and none were saved.</summary>
    public Signal<Loadable<IReadOnlyList<CollectionShelf>>> State { get; } =
        new Signal<Loadable<IReadOnlyList<CollectionShelf>>>(new Loadable<IReadOnlyList<CollectionShelf>>.Loading());

    /// <summary>Asks for the libraries and every library's collections again.</summary>
    public void Retry()
    {
        _ = source.RefreshLibraries();
        Reload();
    }

    public void Dispose()
    {
        gone = true;
        foreach (var w in watches) w.Dispose();
        watches.Clear();
        foreach (var f in shelves.Values) f.Drop();
    }

    void Reload()
    {
        if (gone) return;
        // An answer that comes at once works the rows out again, which may start another library's.
        foreach (var pair in shelves.ToList()) pair.Value.Ask(() => source.Collections(pair.Key));
        Compute();
    }

    void Compute()
    {
        if (gone) return;
        var libraries = source.Libraries.Value;
        if (libraries == null)
        {
            State.Value = source.LibrariesError.Value is { } e
                ? new Loadable<IReadOnlyList<CollectionShelf>>.Failed(e)
                : new Loadable<IReadOnlyList<CollectionShelf>>.Loading();
            return;
        }
        var origin = source.Origin;
        var shown = libraries.Where(l => l.Kind != "music" && (only == null || l.Id == only)).ToList();
        var rows = new List<CollectionShelf>();
        foreach (var library in shown)
        {
            if (!shelves.TryGetValue(library.Id, out var fetch))
            {
                fetch = new Fetch<IReadOnlyList<Collection>>(scheduler, Compute);
                shelves[library.Id] = fetch;
                var id = library.Id;
                fetch.Ask(() => source.Collections(id));
            }
            IReadOnlyList<CollectionTile>? Tiles(IReadOnlyList<Collection>? list) =>
                list?.Select(c => new CollectionTile(c.Id, c.ItemCount, LibraryText.CollectionTile(c, library.Kind, origin))).ToList();
            Loadable<IReadOnlyList<CollectionTile>> tiles = fetch.State switch
            {
                Loadable<IReadOnlyList<Collection>>.Ready r => new Loadable<IReadOnlyList<CollectionTile>>.Ready(Tiles(r.Data)!),
                Loadable<IReadOnlyList<Collection>>.Failed f => new Loadable<IReadOnlyList<CollectionTile>>.Failed(f.Error, Tiles(f.Stale)),
                _ => new Loadable<IReadOnlyList<CollectionTile>>.Loading(),
            };
            rows.Add(new CollectionShelf(library.Id, library.Name, library.Kind, tiles));
        }
        State.Value = new Loadable<IReadOnlyList<CollectionShelf>>.Ready(rows);
    }
}

/// <summary>
/// One collection's page (design spec §4.9; the Android app's CollectionViewModel): the
/// backdrop, the TMDB and Smart badges, what it holds in words, Play and Shuffle, then its items.
///
/// Play plays a collection of movies from its first, through the rest in order (a
/// <see cref="PlayQueue.Collection"/>); Shuffle from a random one. A collection of shows plays
/// the first show's next episode; Shuffle a random show's, as the web does. It asks again when
/// the app hears that things changed.
/// </summary>
public sealed class CollectionModel : ICollectionPageModel
{
    readonly ICollectionSource source;
    readonly IScheduler scheduler;
    readonly Random random;
    readonly List<IDisposable> watches = new List<IDisposable>();
    readonly Fetch<CollectionDetail> detail;
    bool gone;

    public CollectionModel(ICollectionSource source, long collectionId, IScheduler scheduler, Random? random = null)
    {
        this.source = source;
        this.scheduler = scheduler;
        this.random = random ?? new Random();
        CollectionId = collectionId;
        detail = new Fetch<CollectionDetail>(scheduler, Compute);
        watches.Add(source.Libraries.Watch(_ => Compute()));
        watches.Add(source.OnChange(Retry));
        detail.Ask(() => source.Collection(collectionId));
    }

    public long CollectionId { get; }

    public PageKey Key => new PageKey.Collection(CollectionId);

    public Signal<Loadable<CollectionPage>> State { get; } = new Signal<Loadable<CollectionPage>>(new Loadable<CollectionPage>.Loading());

    /// <summary>Nothing to play, or a failure: said beside the buttons for a few seconds (<see cref="CollectionText.MessageMs"/>).</summary>
    public event Action<CollectionEvent>? Told;

    /// <summary>Something to play.</summary>
    public event Action<PlayRequest>? PlayRequested;

    public void Retry()
    {
        if (!gone) detail.Ask(() => source.Collection(CollectionId));
    }

    /// <summary>Play (<paramref name="shuffle"/> false) or Shuffle.</summary>
    public void Play(bool shuffle)
    {
        if (State.Value.Value is not { } page || page.Items.Count == 0) return;
        if (page.Movies)
        {
            var videos = page.Items.Where(i => Format.IsPlayableVideo(i.Kind)).ToList();
            if (videos.Count == 0) return;
            var start = shuffle ? videos[random.Next(videos.Count)] : videos[0];
            PlayRequested?.Invoke(new PlayRequest(start.Id, start.UserState.Watched ? 0 : start.UserState.PositionMs, false, Queue: new PlayQueue.Collection(CollectionId)));
            return;
        }
        var pick = shuffle ? page.Items[random.Next(page.Items.Count)] : page.Items[0];
        Plays.When(scheduler, () => Resolve(pick), target =>
        {
            if (gone) return;
            if (target == null) Told?.Invoke(new CollectionEvent.NothingToPlay());
            else PlayRequested?.Invoke(target);
        }, e =>
        {
            if (!gone) Told?.Invoke(new CollectionEvent.Failed(e));
        });
    }

    /// <summary>Where opening the item at <paramref name="position"/> goes.</summary>
    public PageKey? Opens(int position) =>
        State.Value.Value is { } page && position >= 0 && position < page.Items.Count ? LibraryRules.Opens(page.Items[position]) : null;

    public void Dispose()
    {
        gone = true;
        foreach (var w in watches) w.Dispose();
        watches.Clear();
        detail.Drop();
    }

    Task<PlayRequest?> Resolve(ItemSummary item) => CollectionRules.Resolve(source, item);

    void Compute()
    {
        if (gone) return;
        CollectionPage? Page(CollectionDetail? d) => d == null ? null : CollectionRules.Page(d, source.Libraries.Value, source.Origin);
        State.Value = detail.State switch
        {
            Loadable<CollectionDetail>.Ready r => new Loadable<CollectionPage>.Ready(Page(r.Data)!),
            Loadable<CollectionDetail>.Failed f => new Loadable<CollectionPage>.Failed(f.Error, Page(f.Stale)),
            _ => new Loadable<CollectionPage>.Loading(),
        };
    }
}

/// <summary>One of a merged page's collections: its library, whose name opens that collection alone.</summary>
public sealed record MergedSource(long CollectionId, string LibraryName);

/// <summary>
/// The merged page of <see cref="CollectionIds"/> (the Android app's MergedCollectionViewModel).
/// Play plays its movies one after another in the row's order (a
/// <see cref="PlayQueue.MergedCollection"/>), Shuffle from a random one; without movies, the first
/// show's next episode, or a random show's. It asks again when the app hears that things changed,
/// and the page is made anew in another language, so its title is the server's in that language.
/// </summary>
public sealed class MergedCollectionModel : ICollectionPageModel
{
    readonly ICollectionSource source;
    readonly IScheduler scheduler;
    readonly Random random;
    readonly List<IDisposable> watches = new List<IDisposable>();
    readonly Fetch<MergedCollection> merged;
    bool gone;

    public MergedCollectionModel(ICollectionSource source, IReadOnlyList<long> collectionIds, IScheduler scheduler, Random? random = null)
    {
        this.source = source;
        this.scheduler = scheduler;
        this.random = random ?? new Random();
        CollectionIds = collectionIds;
        merged = new Fetch<MergedCollection>(scheduler, Compute);
        watches.Add(source.Libraries.Watch(_ => Compute()));
        watches.Add(source.OnChange(Retry));
        merged.Ask(() => source.Merged(collectionIds));
    }

    public IReadOnlyList<long> CollectionIds { get; }

    public PageKey Key => new PageKey.MergedCollection(CollectionIds);

    public Signal<Loadable<CollectionPage>> State { get; } = new Signal<Loadable<CollectionPage>>(new Loadable<CollectionPage>.Loading());

    /// <summary>Nothing to play, or a failure: said beside the buttons for a few seconds (<see cref="CollectionText.MessageMs"/>).</summary>
    public event Action<CollectionEvent>? Told;

    /// <summary>Something to play.</summary>
    public event Action<PlayRequest>? PlayRequested;

    public void Retry()
    {
        if (!gone) merged.Ask(() => source.Merged(CollectionIds));
    }

    /// <summary>Play (<paramref name="shuffle"/> false) or Shuffle.</summary>
    public void Play(bool shuffle)
    {
        if (State.Value.Value is not { } page || page.Items.Count == 0) return;
        var movies = page.Items.Where(i => i.Kind == "movie").ToList();
        if (movies.Count > 0)
        {
            var start = shuffle ? movies[random.Next(movies.Count)] : movies[0];
            PlayRequested?.Invoke(new PlayRequest(start.Id, start.UserState.Watched ? 0 : start.UserState.PositionMs, false, Queue: new PlayQueue.MergedCollection(CollectionIds)));
            return;
        }
        var pick = shuffle ? page.Items[random.Next(page.Items.Count)] : page.Items[0];
        Plays.When(scheduler, () => CollectionRules.Resolve(source, pick), target =>
        {
            if (gone) return;
            if (target == null) Told?.Invoke(new CollectionEvent.NothingToPlay());
            else PlayRequested?.Invoke(target);
        }, e =>
        {
            if (!gone) Told?.Invoke(new CollectionEvent.Failed(e));
        });
    }

    /// <summary>Where opening the item at <paramref name="position"/> goes.</summary>
    public PageKey? Opens(int position) =>
        State.Value.Value is { } page && position >= 0 && position < page.Items.Count ? LibraryRules.Opens(page.Items[position]) : null;

    public void Dispose()
    {
        gone = true;
        foreach (var w in watches) w.Dispose();
        watches.Clear();
        merged.Drop();
    }

    void Compute()
    {
        if (gone) return;
        CollectionPage? Page(MergedCollection? m) => m == null ? null : CollectionRules.Merged(m, source.Libraries.Value, source.Origin);
        State.Value = merged.State switch
        {
            Loadable<MergedCollection>.Ready r => new Loadable<CollectionPage>.Ready(Page(r.Data)!),
            Loadable<MergedCollection>.Failed f => new Loadable<CollectionPage>.Failed(f.Error, Page(f.Stale)),
            _ => new Loadable<CollectionPage>.Loading(),
        };
    }
}

/// <summary>A collection's page's rules (the Android app's CollectionViewModel.page).</summary>
public static class CollectionRules
{
    /// <summary>What playing <paramref name="item"/> plays, as the item pages' Play does.</summary>
    internal static Task<PlayRequest?> Resolve(ICollectionSource source, ItemSummary item)
    {
        if (Format.IsPlayableVideo(item.Kind)) return Task.FromResult<PlayRequest?>(new PlayRequest(item.Id, item.UserState.Watched ? 0 : item.UserState.PositionMs, false));
        if (item.Kind is not ("show" or "season")) return Task.FromResult<PlayRequest?>(null);
        return source.OnDeck(item.Id).ContinueWith(t =>
        {
            var next = t.GetAwaiter().GetResult();
            return next == null ? null : new PlayRequest(next.Id, next.UserState.PositionMs, false);
        }, System.Threading.CancellationToken.None, TaskContinuationOptions.ExecuteSynchronously, TaskScheduler.Default);
    }

    /// <summary>The merged page of <paramref name="merged"/>, its libraries found among <paramref name="libraries"/>.</summary>
    public static CollectionPage Merged(MergedCollection merged, IReadOnlyList<Library>? libraries, string origin)
    {
        var sources = new List<MergedSource>();
        foreach (var part in merged.Collections)
            if (libraries?.FirstOrDefault(l => l.Id == part.LibraryId) is { } library) sources.Add(new MergedSource(part.Id, library.Name));
        // No backdrop, as the Android app's: the titles are of several collections.
        var movies = merged.Items.Count(i => i.Kind == "movie");
        return new CollectionPage
        {
            Id = merged.Collections.FirstOrDefault()?.Id ?? 0,
            Title = merged.Title,
            Count = merged.Items.Count,
            Merged = true,
            MovieCount = movies,
            ShowCount = merged.Items.Count(i => i.Kind == "show"),
            Sources = sources,
            Tiles = merged.Items.Select(i => LibraryRules.Tile(i, TileLook.Poster, origin)).ToList(),
            Items = merged.Items,
            Movies = movies > 0,
        };
    }

    /// <summary>The page of <paramref name="detail"/>, its library found among <paramref name="libraries"/>.</summary>
    public static CollectionPage Page(CollectionDetail detail, IReadOnlyList<Library>? libraries, string origin)
    {
        var library = libraries?.FirstOrDefault(l => l.Id == detail.LibraryId);
        var kind = library?.Kind;
        var movies = detail.Items.Any(i => i.Kind == "movie") || (detail.Items.Count == 0 && kind != "shows");
        var notes = !string.IsNullOrEmpty(detail.SmartFilter)
            ? SmartFilter.Describe(detail.SmartFilter!)
            : SortNote(detail.Sort) is { } note ? new[] { note } : Array.Empty<string>();
        string? Whole(string url) => string.IsNullOrEmpty(url) ? null : Artwork.Absolute(url, origin);
        return new CollectionPage
        {
            Id = detail.Id,
            Title = detail.Title,
            Summary = string.IsNullOrWhiteSpace(detail.Summary) ? null : detail.Summary,
            Automatic = detail.Automatic,
            // Other only says it has no category, so the page doesn't name it.
            Category = detail.Category is { Key: not "other" } c ? c.Title : null,
            Smart = !string.IsNullOrEmpty(detail.SmartFilter),
            Count = detail.ItemCount,
            LibraryId = detail.LibraryId,
            LibraryName = library?.Name,
            Kind = kind,
            Notes = notes,
            Backdrop = Whole(detail.BackdropUrl) ?? Whole(detail.PosterUrl),
            Poster = Whole(detail.PosterUrl),
            Tiles = detail.Items.Select(i => LibraryRules.Tile(i, TileLook.Poster, origin)).ToList(),
            Items = detail.Items,
            Movies = movies,
        };
    }

    /// <summary>The web's words for a collection's order.</summary>
    public static string? SortNote(string? sort) => sort switch
    {
        "release" => Strings.T("item.collections.byRelease"),
        "title" => Strings.T("item.collections.byTitle"),
        "custom" => Strings.T("item.collections.customOrder"),
        _ => null,
    };
}

/// <summary>What the TV's collections' pages say (the Android app's tv_collection* strings in strings_tv_library.xml).</summary>
public static class CollectionText
{
    public static string Title => Strings.T("item.collections.title");
    public static string None => Strings.T("item.collections.none");
    public static string NoneBody => Strings.T("item.collections.noneBody");
    public static string Label => Strings.T("item.collections.label");
    /// <summary>The badge of a collection made from TMDB's: the service's name, in every language.</summary>
    public const string Tmdb = "TMDB";
    public static string Smart => Strings.T("item.collections.smart");
    public static string Play => ItemText.Play;
    public static string Shuffle => Strings.T("item.collections.shuffle");
    public static string Empty => Strings.T("item.collections.empty");
    public static string Failed => Strings.T("item.collections.failed");
    public static string NothingToPlay => Strings.T("item.collections.nothingToPlay");
    /// <summary>Before the merged page's libraries: "From".</summary>
    public static string From => Strings.T("item.collections.from");

    /// <summary>How long a message beside Play shows.</summary>
    public const long MessageMs = 4_000;

    /// <summary>The rows the page shows: the libraries with collections.</summary>
    public static IReadOnlyList<CollectionShelf> Shelves(Loadable<IReadOnlyList<CollectionShelf>> state) =>
        state.Value?.Where(s => s.Collections.Value is { Count: > 0 }).ToList() ?? (IReadOnlyList<CollectionShelf>)Array.Empty<CollectionShelf>();

    /// <summary>
    /// The Collections page's message in place of rows: the libraries failed with none saved
    /// (Retry), or every library is read and none has a collection; null otherwise.
    /// </summary>
    public static LibraryMessage? CollectionsMessage(Loadable<IReadOnlyList<CollectionShelf>> state)
    {
        if (state.Value == null && state is Loadable<IReadOnlyList<CollectionShelf>>.Failed f)
            return new LibraryMessage(LibraryText.FailedTitle(f.Error, LibraryText.CollectionsFailed), LibraryText.FailedBody(f.Error), MessageAction.Retry);
        if (state.Value is { } all && Shelves(state).Count == 0 && all.All(s => s.Collections is not Loadable<IReadOnlyList<CollectionTile>>.Loading))
            return new LibraryMessage(None, NoneBody);
        return null;
    }

    /// <summary>The collection page's message in place of it: it failed with nothing to show (Retry); null otherwise.</summary>
    public static LibraryMessage? CollectionMessage(Loadable<CollectionPage> state) =>
        state.Value == null && state is Loadable<CollectionPage>.Failed f
            ? new LibraryMessage(LibraryText.FailedTitle(f.Error, Failed), LibraryText.FailedBody(f.Error), MessageAction.Retry)
            : null;

    /// <summary>
    /// "12 movies  ·  Movies  ·  By release date": the count, the library, what it holds in words;
    /// the merged page's "3 movies  ·  1 show", how many of each it holds.
    /// </summary>
    public static string FactsLine(CollectionPage page)
    {
        if (!page.Merged)
            return string.Join("  ·  ", new[] { LibraryText.CollectionCount(page.Kind, page.Count), page.LibraryName }.Where(s => s != null).Concat(page.Notes));
        var parts = new List<string>();
        if (page.MovieCount > 0) parts.Add(LibraryText.Count("movies", page.MovieCount));
        if (page.ShowCount > 0) parts.Add(LibraryText.Count("shows", page.ShowCount));
        return string.Join("  ·  ", parts);
    }


    /// <summary>The words of what Play or Shuffle ran into.</summary>
    public static string Said(CollectionEvent e) => e switch
    {
        CollectionEvent.Failed f => Errors.Message(f.Error),
        _ => NothingToPlay,
    };
}
