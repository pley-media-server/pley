using System;
using System.Collections.Generic;
using System.Linq;
using System.Threading.Tasks;
using Pley.Tv.Core.Api;
using Pley.Tv.Core.Ui;

namespace Pley.Tv.Core.Tests;

/// <summary>
/// The data side of the item pages' tests (the Android app's ItemTestKit): items the test sets,
/// answered when set (until then, the page waits), and what was written.
/// </summary>
sealed class FakeItems : IItemSource
{
    readonly Dictionary<long, ItemDetail> details = new Dictionary<long, ItemDetail>();
    readonly Dictionary<long, Exception> failures = new Dictionary<long, Exception>();
    readonly Dictionary<long, TaskCompletionSource<ItemDetail>> waitingDetails = new Dictionary<long, TaskCompletionSource<ItemDetail>>();
    readonly Dictionary<long, IReadOnlyList<ItemSummary>> children = new Dictionary<long, IReadOnlyList<ItemSummary>>();
    readonly Dictionary<long, TaskCompletionSource<IReadOnlyList<ItemSummary>>> waitingChildren = new Dictionary<long, TaskCompletionSource<IReadOnlyList<ItemSummary>>>();
    readonly Dictionary<long, ItemSummary?> onDecks = new Dictionary<long, ItemSummary?>();
    readonly Dictionary<long, TaskCompletionSource<ItemSummary?>> waitingOnDecks = new Dictionary<long, TaskCompletionSource<ItemSummary?>>();
    readonly List<Action> listeners = new List<Action>();

    public sealed record StreamWrite(long ItemId, long? FileId, int? Audio, int? Subtitle, bool AutomaticAudio = false);

    public string Origin => ItemKit.Origin;

    /// <summary>Details the app kept from before, shown at once.</summary>
    public Dictionary<long, ItemDetail> SavedDetails { get; } = new Dictionary<long, ItemDetail>();

    public List<(long Id, bool Watched)> Watched { get; } = new List<(long, bool)>();
    public List<(long Id, double? Rating)> Ratings { get; } = new List<(long, double?)>();
    public List<StreamWrite> Streams { get; } = new List<StreamWrite>();
    public List<long> Hidden { get; } = new List<long>();
    /// <summary>My List's writes: the item, and whether it went in.</summary>
    public List<(long Id, bool InList)> Listed { get; } = new List<(long, bool)>();
    public List<ItemDetail?> TracksChangedFor { get; } = new List<ItemDetail?>();
    public int DetailAsks { get; private set; }

    /// <summary>What My List's writes throw, if anything.</summary>
    public Exception? ListFailure { get; set; }

    public void Set(ItemDetail detail)
    {
        details[detail.Id] = detail;
        failures.Remove(detail.Id);
        if (waitingDetails.TryGetValue(detail.Id, out var waiting))
        {
            waitingDetails.Remove(detail.Id);
            waiting.SetResult(detail);
        }
    }

    public void Fail(long id, Exception error)
    {
        failures[id] = error;
        if (waitingDetails.TryGetValue(id, out var waiting))
        {
            waitingDetails.Remove(id);
            waiting.SetException(error);
        }
    }

    public void SetChildren(long id, params ItemSummary[] items)
    {
        children[id] = items;
        if (waitingChildren.TryGetValue(id, out var waiting))
        {
            waitingChildren.Remove(id);
            waiting.SetResult(items);
        }
    }

    public void SetOnDeck(long id, ItemSummary? item)
    {
        onDecks[id] = item;
        if (waitingOnDecks.TryGetValue(id, out var waiting))
        {
            waitingOnDecks.Remove(id);
            waiting.SetResult(item);
        }
    }

    /// <summary>The app heard that things changed.</summary>
    public void Changed()
    {
        foreach (var l in listeners.ToList()) l();
    }

    public ItemDetail? Saved(long id) => SavedDetails.TryGetValue(id, out var d) ? d : null;

    public bool Stale(long id) => true;

    public Task<ItemDetail> Item(long id)
    {
        DetailAsks++;
        if (details.TryGetValue(id, out var d)) return Task.FromResult(d);
        if (failures.TryGetValue(id, out var e)) return Task.FromException<ItemDetail>(e);
        return Waiting(waitingDetails, id).Task;
    }

    public Task<IReadOnlyList<ItemSummary>> Children(long id) =>
        children.TryGetValue(id, out var list) ? Task.FromResult(list) : Waiting(waitingChildren, id).Task;

    public Task<ItemSummary?> OnDeck(long id) =>
        onDecks.TryGetValue(id, out var next) ? Task.FromResult(next) : Waiting(waitingOnDecks, id).Task;

    public Task<IReadOnlyList<ItemSummary>> Related(long id) => Task.FromResult<IReadOnlyList<ItemSummary>>(Array.Empty<ItemSummary>());

    public Task<IReadOnlyList<CollectionRef>> CollectionsOf(long id) => Task.FromResult<IReadOnlyList<CollectionRef>>(Array.Empty<CollectionRef>());

    public Task SetWatched(long id, bool watched)
    {
        Watched.Add((id, watched));
        return Task.CompletedTask;
    }

    public Task Rate(long id, double? rating)
    {
        Ratings.Add((id, rating));
        return Task.CompletedTask;
    }

    public Task SetMyList(long id, bool inList)
    {
        Listed.Add((id, inList));
        return ListFailure == null ? Task.CompletedTask : Task.FromException(ListFailure);
    }

    public Task HideFromContinueWatching(long id)
    {
        Hidden.Add(id);
        return Task.CompletedTask;
    }

    public Task ChooseStreams(long itemId, long? fileId, int? audio, int? subtitle, bool automaticAudio)
    {
        Streams.Add(new StreamWrite(itemId, fileId, audio, subtitle, automaticAudio));
        return Task.CompletedTask;
    }

    public void TracksChanged(ItemDetail? detail) => TracksChangedFor.Add(detail);

    public IDisposable OnChange(Action changed)
    {
        listeners.Add(changed);
        return new Unlisten(() => listeners.Remove(changed));
    }

    static TaskCompletionSource<T> Waiting<T>(Dictionary<long, TaskCompletionSource<T>> waiting, long id)
    {
        if (!waiting.TryGetValue(id, out var t)) waiting[id] = t = new TaskCompletionSource<T>();
        return t;
    }

    sealed class Unlisten : IDisposable
    {
        readonly Action undo;

        public Unlisten(Action undo) => this.undo = undo;

        public void Dispose() => undo();
    }
}

/// <summary>The items of the item pages' tests: Saltwind, a movie in 4K with two audio tracks and subtitles; shows of episodes.</summary>
static class ItemKit
{
    public const string Origin = "http://192.168.1.20:7539";

    public static MediaStream Stream(int index, string kind, string codec, string? language = null, bool isDefault = false, int? channels = null) =>
        new MediaStream { Index = index, Kind = kind, Codec = codec, Language = language, Default = isDefault, Channels = channels };

    public static MediaFile File(long id, MediaStream[] streams, int? selectedAudio = null, int? selectedSubtitle = null, bool audioPicked = false, int width = 3840, int height = 2160) =>
        new MediaFile
        {
            Id = id,
            Path = $"Movie ({id}).mkv",
            Size = 1L << 30,
            Container = "mkv",
            Width = width,
            Height = height,
            VideoCodec = "hevc",
            Streams = streams,
            SelectedAudio = selectedAudio,
            AudioPicked = audioPicked,
            SelectedSubtitle = selectedSubtitle,
        };

    /// <summary>A video, English TrueHD 7.1 (the default), French AC3 5.1, English subtitles.</summary>
    public static MediaStream[] SaltwindStreams() => new[]
    {
        Stream(0, "video", "hevc"),
        Stream(1, "audio", "truehd", "eng", isDefault: true, channels: 8),
        Stream(2, "audio", "ac3", "fra", channels: 6),
        Stream(3, "subtitle", "subrip", "eng"),
    };

    public static ItemDetail Movie(long id = 1, IReadOnlyList<MediaFile>? files = null, long positionMs = 0, bool watched = false, long durationMs = 9_060_000) =>
        new ItemDetail
        {
            Id = id,
            LibraryId = 1,
            Kind = "movie",
            Title = "Saltwind",
            Year = 2021,
            DurationMs = durationMs,
            Rating = 8.1,
            ContentRating = "PG-13",
            Files = files ?? new[] { File(11, SaltwindStreams()) },
            UserState = new UserState { PositionMs = positionMs, Watched = watched },
            InWatchlist = false,
        };

    public static ItemSummary Summary(long id, string kind, string? title = null, long? parentId = null, int? parentIndex = null, int? index = null,
        long positionMs = 0, long? durationMs = 3_480_000, long? grandparentId = null) =>
        new ItemSummary
        {
            Id = id,
            LibraryId = 2,
            Kind = kind,
            Title = title ?? $"Item {id}",
            ParentId = parentId,
            ParentIndex = parentIndex,
            Index = index,
            GrandparentId = grandparentId,
            DurationMs = durationMs,
            UserState = new UserState { PositionMs = positionMs },
        };
}
