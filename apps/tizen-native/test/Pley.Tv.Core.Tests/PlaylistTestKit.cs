using System;
using System.Collections.Generic;
using System.Linq;
using System.Threading.Tasks;
using Pley.Tv.Core.Api;
using Pley.Tv.Core.Ui;

namespace Pley.Tv.Core.Tests;

/// <summary>
/// The playlists' data (the Android app's FakePlaylists): what was sent, in order ("reorder 1
/// [3, 1, 2]", "remove 1 [7]", "rename 1 Road trip"…); changes answered at once unless a gate
/// holds them, and failing with <see cref="Fail"/>.
/// </summary>
sealed class FakePlaylists : IPlaylistSource
{
    readonly Listeners listeners = new Listeners();
    readonly List<Action> held = new List<Action>();
    long nextId = 100;

    public string Origin => PagesKit.Origin;
    public Dictionary<long, PlaylistDetail> Details { get; } = new Dictionary<long, PlaylistDetail>();
    public Dictionary<long, Exception> Missing { get; } = new Dictionary<long, Exception>();
    public IReadOnlyList<Playlist>? List { get; set; }
    public Dictionary<long, ItemSummary> Items { get; } = new Dictionary<long, ItemSummary>();
    public List<string> Sent { get; } = new List<string>();
    /// <summary>The next changes fail with this.</summary>
    public Exception? Fail { get; set; }
    /// <summary>Changes wait until <see cref="Open"/>.</summary>
    public bool Gate { get; set; }

    public void Set(PlaylistDetail detail) => Details[detail.Id] = detail;

    public void Changed() => listeners.Changed();

    /// <summary>The changes held by the gate are answered, in order.</summary>
    public void Open()
    {
        Gate = false;
        foreach (var answer in held.ToList())
        {
            held.Remove(answer);
            answer();
        }
    }

    public Task<IReadOnlyList<Playlist>> Playlists() => List != null ? Task.FromResult(List) : new TaskCompletionSource<IReadOnlyList<Playlist>>().Task;

    public Task<PlaylistDetail> Playlist(long id)
    {
        if (Missing.TryGetValue(id, out var e)) return Task.FromException<PlaylistDetail>(e);
        return Details.TryGetValue(id, out var d) ? Task.FromResult(d) : new TaskCompletionSource<PlaylistDetail>().Task;
    }

    public Task<long> Create(string title, string? kind, IReadOnlyList<long> itemIds)
    {
        Sent.Add($"create {title} {kind ?? "null"} [{string.Join(", ", itemIds)}]");
        return Server(() => nextId++);
    }

    public Task<long> CreateSmart(string title, long libraryId, string filter)
    {
        Sent.Add($"smart {title} {libraryId} {filter}");
        return Server(() => nextId++);
    }

    public Task<PlaylistDetail> Rename(long id, string title)
    {
        Sent.Add($"rename {id} {title}");
        return Server(() => Details[id] = Details[id] with { Title = title });
    }

    public Task Delete(long id)
    {
        Sent.Add($"delete {id}");
        return Server(() => true);
    }

    public Task Add(long id, IReadOnlyList<long> itemIds)
    {
        Sent.Add($"add {id} [{string.Join(", ", itemIds)}]");
        return Server(() => true);
    }

    public Task Remove(long id, long entryId)
    {
        Sent.Add($"remove {id} [{entryId}]");
        return Server(() =>
        {
            var d = Details[id];
            return Details[id] = d with { Entries = d.Entries.Where(e => e.EntryId != entryId).ToList(), ItemCount = d.ItemCount - 1 };
        });
    }

    public Task<PlaylistDetail> Reorder(long id, IReadOnlyList<long> entryIds)
    {
        Sent.Add($"reorder {id} [{string.Join(", ", entryIds)}]");
        return Server(() =>
        {
            var d = Details[id];
            var first = entryIds.Select(e => d.Entries.FirstOrDefault(x => x.EntryId == e)).Where(e => e != null).Select(e => e!).ToList();
            return Details[id] = d with { Entries = first.Concat(d.Entries.Where(e => !entryIds.Contains(e.EntryId))).ToList() };
        });
    }

    public Task<ItemSummary> Item(long id) => Items.TryGetValue(id, out var item) ? Task.FromResult(item) : new TaskCompletionSource<ItemSummary>().Task;

    public IDisposable OnChange(Action changed) => listeners.Add(changed);

    /// <summary>The server's answer: <paramref name="change"/> made, or the failure, at once or when the gate opens.</summary>
    Task<T> Server<T>(Func<T> change)
    {
        var answer = new TaskCompletionSource<T>();
        void Answer()
        {
            if (Fail != null) answer.SetException(Fail);
            else answer.SetResult(change());
        }
        if (Gate) held.Add(Answer);
        else Answer();
        return answer.Task;
    }
}

/// <summary>The items and playlists of the playlists' tests (the Android app's PlaylistFakes).</summary>
static class PlaylistKit
{
    public static ItemSummary Episode(long id, string title, string show, int season, int number, long durationMs = 3_000_000, long positionMs = 0) =>
        new ItemSummary
        {
            Id = id,
            LibraryId = 2,
            Kind = "episode",
            Title = title,
            Index = number,
            ParentIndex = season,
            ParentId = 900,
            GrandparentId = 901,
            GrandparentTitle = show,
            DurationMs = durationMs,
            UserState = new UserState { PositionMs = positionMs },
        };

    public static ItemSummary Track(long id, string title, long album = 50) =>
        new ItemSummary { Id = id, LibraryId = 3, Kind = "track", Title = title, ParentId = album, GrandparentTitle = "Harbour Lights", DurationMs = 200_000 };

    /// <summary>A playlist of <paramref name="items"/>, entries numbered from 1 in that order.</summary>
    public static PlaylistDetail Playlist(long id, string title, ItemSummary[] items, string kind = PlaylistKind.Video, string? smart = null) => new PlaylistDetail
    {
        Id = id,
        Title = title,
        Kind = kind,
        ItemCount = items.Length,
        DurationMs = items.Sum(i => i.DurationMs ?? 0),
        SmartFilter = smart,
        LibraryId = smart != null ? 1 : null,
        Entries = items.Select((item, i) => new PlaylistEntry { EntryId = i + 1, Item = item }).ToList(),
    };
}
