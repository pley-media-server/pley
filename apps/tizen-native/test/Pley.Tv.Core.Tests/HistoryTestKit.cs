using System;
using System.Collections.Generic;
using System.Linq;
using System.Threading.Tasks;
using Pley.Tv.Core.Api;
using Pley.Tv.Core.Ui;

namespace Pley.Tv.Core.Tests;

/// <summary>The history's data (the Android app's AccountViewModelsTest History): its paging and its removals.</summary>
sealed class FakeHistory : IHistorySource
{
    readonly Listeners listeners = new Listeners();
    readonly List<HistoryEntry> all;

    public FakeHistory(int total) => all = Enumerable.Range(1, total).Select(i => HistoryKit.Play(i)).ToList();

    public string Origin => PagesKit.Origin;
    public Exception? RefuseRemoval { get; set; }
    public Exception? RefusePage { get; set; }
    public List<(int Offset, int Limit)> Asked { get; } = new List<(int, int)>();
    public List<long> Removed { get; } = new List<long>();

    public void Changed() => listeners.Changed();

    public Task<HistoryPage> History(int offset, int limit)
    {
        Asked.Add((offset, limit));
        if (RefusePage != null) return Task.FromException<HistoryPage>(RefusePage);
        return Task.FromResult(new HistoryPage(all.Count, all.Skip(offset).Take(limit).ToList()));
    }

    public Task Remove(long entryId)
    {
        Removed.Add(entryId);
        if (RefuseRemoval != null) return Task.FromException(RefuseRemoval);
        all.RemoveAll(e => e.Id == entryId);
        return Task.CompletedTask;
    }

    public IDisposable OnChange(Action changed) => listeners.Add(changed);
}

/// <summary>The plays of the history's tests.</summary>
static class HistoryKit
{
    /// <summary>Now, in unix seconds, for the history's times.</summary>
    public const long Now = 1_760_000_000;

    /// <summary>A play of "Film <paramref name="id"/>", <paramref name="id"/> minutes ago.</summary>
    public static HistoryEntry Play(long id) => new HistoryEntry
    {
        Id = id,
        ViewedAt = Now - id * 60,
        Device = "pleyAndroid",
        Item = new ItemSummary { Id = 100 + id, LibraryId = 1, Kind = "movie", Title = $"Film {id}", Year = 2020 },
    };
}
