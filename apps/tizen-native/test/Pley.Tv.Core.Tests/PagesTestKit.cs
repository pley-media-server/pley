using System;
using System.Collections.Generic;
using System.Linq;
using System.Threading;
using System.Threading.Tasks;
using Pley.Tv.Core.Api;
using Pley.Tv.Core.Ui;

namespace Pley.Tv.Core.Tests;

/// <summary>Listeners a fake calls when the test says things changed.</summary>
sealed class Listeners
{
    readonly List<Action> listeners = new List<Action>();

    public IDisposable Add(Action changed)
    {
        listeners.Add(changed);
        return new Unlisten(() => listeners.Remove(changed));
    }

    public void Changed()
    {
        foreach (var l in listeners.ToList()) l();
    }

    sealed class Unlisten : IDisposable
    {
        readonly Action undo;

        public Unlisten(Action undo) => this.undo = undo;

        public void Dispose() => undo();
    }
}

/// <summary>Search's data (the Android app's SearchViewModelTest's FakeSearch): answers the test gives per query; a query without one waits.</summary>
sealed class FakeSearch : ISearchSource
{
    readonly Dictionary<string, TaskCompletionSource<SearchAnswer>> answers = new Dictionary<string, TaskCompletionSource<SearchAnswer>>();

    public string Origin => PagesKit.Origin;
    public List<string> Asked { get; } = new List<string>();
    public List<string> Cancelled { get; } = new List<string>();
    public List<string> Kept { get; set; } = new List<string> { "halcyon" };

    public TaskCompletionSource<SearchAnswer> Answer(string q)
    {
        if (!answers.TryGetValue(q, out var answer)) answers[q] = answer = new TaskCompletionSource<SearchAnswer>();
        return answer;
    }

    /// <summary>The next ask of <paramref name="q"/> waits for a new answer.</summary>
    public void Forget(string q) => answers.Remove(q);

    public Task<SearchAnswer> Search(string query, CancellationToken cancel)
    {
        Asked.Add(query);
        var answer = Answer(query);
        cancel.Register(() =>
        {
            if (!answer.Task.IsCompleted) Cancelled.Add(query);
        });
        return answer.Task;
    }

    public IReadOnlyList<string> Recent() => Kept;

    public void KeepRecent(IReadOnlyList<string> recent) => Kept = recent.ToList();
}

/// <summary>A person's page's data: the person once set, or a failure.</summary>
sealed class FakePeople : IPersonSource
{
    readonly Listeners listeners = new Listeners();

    public string Origin => PagesKit.Origin;
    public PersonDetail? Person { get; set; }
    public Exception? Failure { get; set; }
    public int Asks { get; private set; }

    public void Changed() => listeners.Changed();

    Task<PersonDetail> IPersonSource.Person(long id)
    {
        Asks++;
        if (Failure != null) return Task.FromException<PersonDetail>(Failure);
        return Person != null ? Task.FromResult(Person) : new TaskCompletionSource<PersonDetail>().Task;
    }

    public IDisposable OnChange(Action changed) => listeners.Add(changed);
}

/// <summary>The collections' pages' data (the Android app's FakeCollections and FakeLibraries).</summary>
sealed class FakeCollections : ICollectionSource
{
    readonly Listeners listeners = new Listeners();

    public FakeCollections(params Library[] libraries) => Libraries.Value = libraries;

    public string Origin => PagesKit.Origin;
    public Signal<IReadOnlyList<Library>?> Libraries { get; } = new Signal<IReadOnlyList<Library>?>(null);
    public Signal<Exception?> LibrariesError { get; } = new Signal<Exception?>(null);
    public Dictionary<long, IReadOnlyList<Collection>> Lists { get; } = new Dictionary<long, IReadOnlyList<Collection>>();
    public Dictionary<long, CollectionDetail> Details { get; } = new Dictionary<long, CollectionDetail>();
    public Dictionary<long, Exception> Failures { get; } = new Dictionary<long, Exception>();
    /// <summary>Each show's next episode; a show without one has nothing to play.</summary>
    public Dictionary<long, ItemSummary> Next { get; } = new Dictionary<long, ItemSummary>();
    public List<long> Resolved { get; } = new List<long>();
    public int Refreshes { get; private set; }

    public void Changed() => listeners.Changed();

    public Task RefreshLibraries()
    {
        Refreshes++;
        return Task.CompletedTask;
    }

    public Task<IReadOnlyList<Collection>> Collections(long libraryId) =>
        Lists.TryGetValue(libraryId, out var list) ? Task.FromResult(list) : Task.FromResult<IReadOnlyList<Collection>>(Array.Empty<Collection>());

    public Task<CollectionDetail> Collection(long id)
    {
        if (Failures.TryGetValue(id, out var e)) return Task.FromException<CollectionDetail>(e);
        return Details.TryGetValue(id, out var d) ? Task.FromResult(d) : new TaskCompletionSource<CollectionDetail>().Task;
    }

    public Task<ItemSummary?> OnDeck(long id)
    {
        Resolved.Add(id);
        return Task.FromResult(Next.TryGetValue(id, out var next) ? next : null);
    }

    public IDisposable OnChange(Action changed) => listeners.Add(changed);
}

/// <summary>The items of these pages' tests.</summary>
static class PagesKit
{
    public const string Origin = "http://pley";

    public static ItemSummary Item(long id, string kind, string? title = null, long? parentId = null) =>
        new ItemSummary { Id = id, LibraryId = 1, Kind = kind, Title = title ?? $"Item {id}", ParentId = parentId };

    public static ItemSummary Film(long id, string title, int year = 2024, long durationMs = 6_000_000) =>
        new ItemSummary { Id = id, LibraryId = 1, Kind = "movie", Title = title, Year = year, DurationMs = durationMs };
}
