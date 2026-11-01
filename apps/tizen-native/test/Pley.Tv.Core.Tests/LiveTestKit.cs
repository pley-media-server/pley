using System;
using System.Collections.Generic;
using System.Linq;
using System.Threading;
using System.Threading.Tasks;
using Pley.Tv.Core.Api;
using Pley.Tv.Core.Live;
using Pley.Tv.Core.Player;

namespace Pley.Tv.Core.Tests;

/// <summary>
/// A Live TV server in a test (the Android app's FakeLiveTv): a lineup it browses as the server
/// does (a category, a search by name, number or what's on, pages of 100, the page around a
/// channel), a guide, favourites and watching, with what it was asked.
/// </summary>
sealed class FakeLive : ILiveSource, ILiveServer
{
    public const string Server = "http://192.168.1.20:7539";

    readonly List<Action> listeners = new List<Action>();

    public FakeLive()
    {
        Lineup = new List<LiveChannel>
        {
            new LiveChannel { Id = 1, Name = "Pley News HD", Number = "101", Group = "News", HasGuide = true, Now = Program(1, 0, 4_000_000_000, "The Briefing") },
            new LiveChannel { Id = 2, Name = "Pley Sport", Number = "102", Group = "Sport", HasGuide = true },
            new LiveChannel { Id = 5, Name = "Weather Now", Number = "105", Group = "News" },
        };
        Lineup.AddRange(Enumerable.Range(1, 200).Select(i => new LiveChannel { Id = 1000 + i, Name = $"Filler {i}", Number = $"{1000 + i}", Group = "Filler" }));
    }

    public List<LiveChannel> Lineup { get; }
    public List<long> Recent { get; set; } = new List<long> { 2, 1 };
    public LiveSummary SummaryAnswer { get; set; } = new LiveSummary { Available = true, Allowed = true, Channels = 203 };
    public Dictionary<long, LiveGuide> Guides { get; } = new Dictionary<long, LiveGuide>();
    /// <summary>Guide chunks held until <see cref="AnswerGuides"/>.</summary>
    public bool HoldGuides { get; set; }
    readonly List<(long From, TaskCompletionSource<LiveGuide> Answer)> heldGuides = new List<(long, TaskCompletionSource<LiveGuide>)>();
    public Exception? FavoriteFailure { get; set; }
    public Exception? WatchFailure { get; set; }
    /// <summary>What watching answers as the playlist's address; a keyed one by default.</summary>
    public Func<long, string> WatchUrl { get; set; } = id => $"/api/live/media/pley_mk_{id}/index.m3u8";
    public Exception? PrefetchFailure { get; set; }
    /// <summary>What <see cref="WhyEnded"/> answers: null while sessions go on.</summary>
    public string? Ended { get; set; }
    public LiveTvView View { get; set; } = new LiveTvView();
    public List<string> Sent { get; } = new List<string>();
    public List<string> Left { get; } = new List<string>();
    int sessions;

    public static LiveProgram Program(long channel, long start, long stop, string title) =>
        new LiveProgram { ChannelId = channel, Start = start, Stop = stop, Title = title };

    public string Origin => Server;

    public string ClientId => "tizen-1";

    public string Url(string path) => Server + path;

    public Task<LiveSummary> Summary() => Task.FromResult(SummaryAnswer);

    public Task<LiveGroups> Groups()
    {
        Sent.Add("groups");
        var groups = Lineup.GroupBy(c => c.Group).Select(g => new LiveGroup { Name = g.Key, Label = g.Key, Count = g.Count() }).ToList();
        groups.Add(new LiveGroup { Name = "UK| Sport", Region = "UK", Label = "Sport" });
        groups.Add(new LiveGroup { Name = "UK| News", Region = "UK", Label = "News" });
        return Task.FromResult(new LiveGroups { Total = Lineup.Count, Favorites = Lineup.Count(c => c.Favorite), Recent = Recent.Count, Groups = groups });
    }

    IReadOnlyList<LiveChannel> Of(LiveCategory category, string query)
    {
        IEnumerable<LiveChannel> list = category switch
        {
            LiveCategory.Favorites => Lineup.Where(c => c.Favorite),
            LiveCategory.Recent => Recent.Select(id => Lineup.First(c => c.Id == id)),
            LiveCategory.Group g => Lineup.Where(c => c.Group == g.Name),
            _ => Lineup,
        };
        var q = query.Trim();
        if (q.Length > 0)
            list = list.Where(c => c.Name.IndexOf(q, StringComparison.OrdinalIgnoreCase) >= 0 || (c.Number ?? "").StartsWith(q, StringComparison.Ordinal)
                || (c.Now?.Title ?? "").IndexOf(q, StringComparison.OrdinalIgnoreCase) >= 0);
        return list.ToList();
    }

    public Task<LiveChannelPage> Page(LiveCategory category, string query, int page)
    {
        Sent.Add($"page {category.Key} {query.Trim()} {page}");
        var all = Of(category, query);
        return Task.FromResult(new LiveChannelPage { Total = all.Count, Offset = page * LiveRules.Page, Channels = all.Skip(page * LiveRules.Page).Take(LiveRules.Page).ToList() });
    }

    public Task<LiveChannelPage> Around(LiveCategory category, long channelId)
    {
        var all = Of(category, "");
        var at = all.Select((c, i) => (c, i)).FirstOrDefault(x => x.c.Id == channelId);
        var page = at.c == null ? 0 : at.i / LiveRules.Page;
        return Task.FromResult(new LiveChannelPage { Total = all.Count, Offset = page * LiveRules.Page, Channels = all.Skip(page * LiveRules.Page).Take(LiveRules.Page).ToList() });
    }

    public Task<LiveChannel> Channel(long channelId) => Task.FromResult(Lineup.First(c => c.Id == channelId));

    public Task<LiveGuide> Guide(long from, IReadOnlyList<long> ids)
    {
        Sent.Add($"guide {from} {ids.Count}");
        if (HoldGuides)
        {
            var answer = new TaskCompletionSource<LiveGuide>();
            heldGuides.Add((from, answer));
            return answer.Task;
        }
        return Task.FromResult(Guides.TryGetValue(from, out var g) ? g : new LiveGuide { From = from, To = from + LiveRules.GuideChunkS });
    }

    public void AnswerGuides()
    {
        HoldGuides = false;
        foreach (var (from, answer) in heldGuides.ToList())
        {
            heldGuides.Remove((from, answer));
            answer.SetResult(Guides.TryGetValue(from, out var g) ? g : new LiveGuide { From = from, To = from + LiveRules.GuideChunkS });
        }
    }

    public Task<LiveChannel?> Numbered(string number) => Task.FromResult(Lineup.FirstOrDefault(c => c.Number == number));

    public Task SetFavorite(long channelId, bool favorite)
    {
        Sent.Add($"favorite {channelId} {favorite}");
        if (FavoriteFailure != null) return Task.FromException(FavoriteFailure);
        var i = Lineup.FindIndex(c => c.Id == channelId);
        Lineup[i] = Lineup[i] with { Favorite = favorite };
        return Task.CompletedTask;
    }

    public LiveTvView Kept() => View;

    public void Keep(string? category = null, string? view = null, long? channel = null) =>
        View = new LiveTvView(category ?? View.Category, view ?? View.View, channel ?? View.Channel);

    public IDisposable OnChange(Action changed)
    {
        listeners.Add(changed);
        return new Unlisten(() => listeners.Remove(changed));
    }

    public void Changed()
    {
        foreach (var l in listeners.ToList()) l();
    }

    public Task<LiveWatching> Watch(long channelId, long? takeOver, CancellationToken cancel)
    {
        Sent.Add(takeOver == null ? $"watch {channelId} {ClientId}" : $"watch {channelId} {ClientId} take {takeOver}");
        if (WatchFailure != null) return Task.FromException<LiveWatching>(WatchFailure);
        return Task.FromResult(new LiveWatching { SessionId = $"s{++sessions}", Url = WatchUrl(channelId) });
    }

    public Task Prefetch(string url, int timeoutMs, CancellationToken cancel)
    {
        Sent.Add($"prefetch {url}");
        return PrefetchFailure != null ? Task.FromException(PrefetchFailure) : Task.CompletedTask;
    }

    public Task<string?> WhyEnded(string session) => Task.FromResult(Ended);

    /// <summary>Where the channel's stream.ts is handed on; null (none, HLS only) by default.</summary>
    public Func<long, string?> StreamAt { get; set; } = _ => null;

    /// <summary>What <see cref="Plan"/> answers; held until <see cref="AnswerPlan"/> while <see cref="HoldPlan"/>.</summary>
    public LivePlan? PlanAnswer { get; set; }
    public bool HoldPlan { get; set; }
    TaskCompletionSource<LivePlan?>? heldPlan;

    public string? Stream(long channelId, string clientId)
    {
        var url = StreamAt(channelId);
        if (url != null) Sent.Add($"stream {channelId} {clientId}");
        return url;
    }

    public void EndStream() => Sent.Add("end stream");

    public Task<LivePlan?> Plan(string session)
    {
        Sent.Add($"plan {session}");
        if (!HoldPlan) return Task.FromResult(PlanAnswer);
        heldPlan = new TaskCompletionSource<LivePlan?>();
        return heldPlan.Task;
    }

    public void AnswerPlan(LivePlan? plan) => heldPlan!.SetResult(plan);

    public void Leave(string session) => Left.Add(session);

    public string LastWatch => Sent.Last(s => s.StartsWith("watch", StringComparison.Ordinal));

    sealed class Unlisten : IDisposable
    {
        readonly Action undo;

        public Unlisten(Action undo) => this.undo = undo;

        public void Dispose() => undo();
    }
}

/// <summary>The TV's player in a test: what it was asked, and the events a test makes it send.</summary>
sealed class FakeEngine : IMediaEngine
{
    public event Action<EngineStatus>? StatusChanged;
    public event Action? Started;
    public event Action<bool>? PlayingChanged;
    public event Action<EngineError>? Failed;

    public List<string> Calls { get; } = new List<string>();
    public EngineSource? Source { get; private set; }
    public bool Released { get; private set; }
    public long PositionMs { get; set; }
    public long DurationMs { get; set; }
    public long BufferedMs { get; set; }
    public bool Playing { get; private set; }
    public bool? HasVideo { get; set; } = true;

    public void Prepare(EngineSource source, bool play)
    {
        Source = source;
        PositionMs = source.StartMs;
        Calls.Add($"prepare {source.Url} {(play ? "play" : "paused")}");
    }

    public void Play()
    {
        Calls.Add("play");
        SetPlaying(true);
    }

    public void Pause()
    {
        Calls.Add("pause");
        SetPlaying(false);
    }

    public void SeekTo(long ms)
    {
        Calls.Add($"seek {ms}");
        PositionMs = ms;
    }

    public void Stop()
    {
        Calls.Add("stop");
        Playing = false;
    }

    public void Release()
    {
        Calls.Add("release");
        Released = true;
    }

    public void SetPlaying(bool playing)
    {
        if (Playing == playing) return;
        Playing = playing;
        PlayingChanged?.Invoke(playing);
    }

    public void Status(EngineStatus status) => StatusChanged?.Invoke(status);

    public void Start()
    {
        Started?.Invoke();
        SetPlaying(true);
    }

    public void Fail(EngineError error) => Failed?.Invoke(error);
}
