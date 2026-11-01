using System;
using System.Collections.Generic;
using System.Linq;
using System.Threading.Tasks;
using Pley.Tv.Core.Api;
using Pley.Tv.Core.Ui;

namespace Pley.Tv.Core.Tests;

/// <summary>
/// The data side of the library pages' tests (the Android app's LibraryFakes): listings the test
/// sets, answered page by page as the server would (until set, the page waits), and what was
/// asked and written.
/// </summary>
sealed class FakeLibraries : ILibrarySource, ILibrariesSource
{
    public const string Server = "http://192.168.1.20:7539";

    readonly Dictionary<LibraryQuery, (int Total, Func<int, ItemSummary> At)> listings = new Dictionary<LibraryQuery, (int, Func<int, ItemSummary>)>();
    readonly Dictionary<LibraryQuery, Exception> failures = new Dictionary<LibraryQuery, Exception>();
    readonly List<(LibraryQuery Query, TaskCompletionSource<ItemsPage> Answer)> waiting = new List<(LibraryQuery, TaskCompletionSource<ItemsPage>)>();
    readonly Dictionary<long, IReadOnlyList<Hub>> hubs = new Dictionary<long, IReadOnlyList<Hub>>();
    readonly Dictionary<long, IReadOnlyList<Collection>> collections = new Dictionary<long, IReadOnlyList<Collection>>();
    readonly List<Action> listeners = new List<Action>();

    public FakeLibraries(params Library[] libraries) => Libraries.Value = libraries;

    public string Origin => Server;

    public Signal<IReadOnlyList<Library>?> Libraries { get; } = new Signal<IReadOnlyList<Library>?>(null);
    public Signal<Exception?> LibrariesError { get; } = new Signal<Exception?>(null);
    public Signal<Settings> Settings { get; } = new Signal<Settings>(new Settings());
    public Signal<IReadOnlyList<ScanState>> Scans { get; } = new Signal<IReadOnlyList<ScanState>>(Array.Empty<ScanState>());
    public Signal<User?> Me { get; } = new Signal<User?>(null);

    /// <summary>Every page asked for, with its offset and limit.</summary>
    public List<LibraryQuery> Asked { get; } = new List<LibraryQuery>();
    public FiltersAnswer FiltersAnswer { get; set; } = new FiltersAnswer(new[] { "Comedy", "Drama" }, new[] { 2024, 1999 });
    public int HubAsks { get; private set; }
    public int CollectionAsks { get; private set; }
    public int Refreshes { get; private set; }
    /// <summary>Views kept by library, as a test sets and reads them.</summary>
    public Dictionary<long, string> Kept { get; } = new Dictionary<long, string>();
    /// <summary>Orders sent: the ids, or null for the server's order.</summary>
    public List<IReadOnlyList<long>?> Orders { get; } = new List<IReadOnlyList<long>?>();
    /// <summary>What sending an order throws, if anything.</summary>
    public Exception? OrderFailure { get; set; }

    static LibraryQuery Whole(LibraryQuery q) => q with { Offset = 0, Limit = null };

    /// <summary>The listing of <paramref name="query"/>: <paramref name="total"/> items, made by position.</summary>
    public void Set(LibraryQuery query, int total, Func<int, ItemSummary>? at = null)
    {
        var key = Whole(query);
        var make = at ?? (i => LibraryKit.Movie(i + 1));
        listings[key] = (total, make);
        failures.Remove(key);
        foreach (var w in waiting.Where(w => Whole(w.Query) == key).ToList())
        {
            waiting.Remove(w);
            w.Answer.SetResult(PageOf(w.Query, total, make));
        }
    }

    /// <summary>The listing of <paramref name="query"/> is <paramref name="items"/>, the same instances on every ask.</summary>
    public void Set(LibraryQuery query, IReadOnlyList<ItemSummary> items) => Set(query, items.Count, i => items[i]);

    public void Fail(LibraryQuery query, Exception error)
    {
        var key = Whole(query);
        failures[key] = error;
        listings.Remove(key);
        foreach (var w in waiting.Where(w => Whole(w.Query) == key).ToList())
        {
            waiting.Remove(w);
            w.Answer.SetException(error);
        }
    }

    public void SetHubs(long libraryId, params Hub[] list) => hubs[libraryId] = list;

    public void SetCollections(long libraryId, params Collection[] list) => collections[libraryId] = list;

    /// <summary>The app heard that things changed.</summary>
    public void Changed()
    {
        foreach (var l in listeners.ToList()) l();
    }

    public void UpdateSettings(Func<Settings, Settings> change) => Settings.Value = change(Settings.Value);

    public Task RefreshLibraries()
    {
        Refreshes++;
        return Task.CompletedTask;
    }

    /// <summary>Pages asked for wait, set or not, until <see cref="Release"/>.</summary>
    public bool Holding { get; set; }

    /// <summary>The pages held come, in the order they were asked for.</summary>
    public void Release()
    {
        Holding = false;
        foreach (var w in waiting.ToList())
        {
            if (!listings.TryGetValue(Whole(w.Query), out var listing)) continue;
            waiting.Remove(w);
            w.Answer.SetResult(PageOf(w.Query, listing.Total, listing.At));
        }
    }

    public Task<ItemsPage> Items(long libraryId, LibraryQuery query)
    {
        Asked.Add(query);
        var key = Whole(query);
        if (!Holding && listings.TryGetValue(key, out var listing)) return Task.FromResult(PageOf(query, listing.Total, listing.At));
        if (failures.TryGetValue(key, out var e)) return Task.FromException<ItemsPage>(e);
        var answer = new TaskCompletionSource<ItemsPage>();
        waiting.Add((query, answer));
        return answer.Task;
    }

    static ItemsPage PageOf(LibraryQuery query, int total, Func<int, ItemSummary> at)
    {
        var end = Math.Min(total, query.Offset + (query.Limit ?? total));
        return new ItemsPage(total, Enumerable.Range(query.Offset, Math.Max(0, end - query.Offset)).Select(at).ToList());
    }

    public Task<FiltersAnswer> Filters(long libraryId) => Task.FromResult(FiltersAnswer);

    public Task<IReadOnlyList<Hub>> Hubs(long libraryId)
    {
        HubAsks++;
        return hubs.TryGetValue(libraryId, out var list) ? Task.FromResult(list) : new TaskCompletionSource<IReadOnlyList<Hub>>().Task;
    }

    public Task<IReadOnlyList<Collection>> Collections(long libraryId)
    {
        CollectionAsks++;
        return collections.TryGetValue(libraryId, out var list) ? Task.FromResult(list) : new TaskCompletionSource<IReadOnlyList<Collection>>().Task;
    }

    public string? View(long libraryId) => Kept.TryGetValue(libraryId, out var v) ? v : null;

    public void KeepView(long libraryId, string view)
    {
        if (view.Length == 0) Kept.Remove(libraryId);
        else Kept[libraryId] = view;
    }

    public Task OrderLibraries(IReadOnlyList<long>? ids)
    {
        Orders.Add(ids);
        if (OrderFailure != null) return Task.FromException(OrderFailure);
        // The server's answer is the list in that order, which the app keeps.
        if (ids != null && Libraries.Value is { } list) Libraries.Value = ids.Select(id => list.First(l => l.Id == id)).ToList();
        return Task.CompletedTask;
    }

    public IDisposable OnChange(Action changed)
    {
        listeners.Add(changed);
        return new Unlisten(() => listeners.Remove(changed));
    }

    sealed class Unlisten : IDisposable
    {
        readonly Action undo;

        public Unlisten(Action undo) => this.undo = undo;

        public void Dispose() => undo();
    }
}

/// <summary>The libraries and items of the library pages' tests.</summary>
static class LibraryKit
{
    public static readonly Library Movies = new Library { Id = 1, Name = "Movies", Kind = "movies", ItemCount = 342 };
    public static readonly Library Music = new Library { Id = 2, Name = "Music", Kind = "music", ItemCount = 120 };
    public static readonly Library Shows = new Library { Id = 4, Name = "Shows", Kind = "shows", ItemCount = 38 };

    public static readonly LibraryQuery ByTitle = new LibraryQuery { Sort = LibrarySort.Title, Order = SortOrder.Ascending };

    public static ItemSummary Movie(long id, string? title = null, string kind = "movie") =>
        new ItemSummary { Id = id, LibraryId = 1, Kind = kind, Title = title ?? $"Movie {id}", SortTitle = title ?? $"Movie {id}" };

    public static Collection Collection(long id, string title, long count) =>
        new Collection { Id = id, LibraryId = 1, Title = title, ItemCount = count, PosterUrl = $"/api/collections/{id}/images/poster" };
}
