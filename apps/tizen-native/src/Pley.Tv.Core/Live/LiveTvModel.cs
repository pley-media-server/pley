using System;
using System.Collections.Generic;
using System.Linq;
using Pley.Tv.Core.Api;
using Pley.Tv.Core.Ui;

namespace Pley.Tv.Core.Live;

/// <summary>How a Live TV page shows the channels: the guide (channels by time) or cards.</summary>
public enum LiveView
{
    Guide,
    Channels,
}

/// <summary>The Live TV page, as its screen draws it, the channels aside (<see cref="LiveTvModel.Channels"/>).</summary>
public sealed record LivePage
{
    /// <summary>Live TV is open to this profile (true until the server says).</summary>
    public bool Allowed { get; init; } = true;
    /// <summary>The groups and the counts; null while unknown.</summary>
    public LiveGroups? Groups { get; init; }
    public LiveCategory Category { get; init; } = new LiveCategory.All();
    public string Query { get; init; } = "";
    public LiveView View { get; init; } = LiveView.Guide;
    /// <summary>The server's origin: logos and programme pictures are on it.</summary>
    public string Origin { get; init; } = "";
    /// <summary>The groups a search names, as quick links to them.</summary>
    public IReadOnlyList<LiveGroup> Found { get; init; } = Array.Empty<LiveGroup>();

    public bool Searching => !string.IsNullOrWhiteSpace(Query);

    /// <summary>What the channels are: the category, or every channel while searching (search looks everywhere, as the web's).</summary>
    public LiveCategory Listed => Searching ? new LiveCategory.All() : Category;
}

/// <summary>Where a list opens: at <paramref name="Index"/> of <paramref name="Category"/> searched for <paramref name="Query"/> (the channel last watched).</summary>
public sealed record Landing(LiveCategory Category, string Query, int? Index);

/// <summary>The guide's programmes by channel, and whether those in view are in.</summary>
public sealed record GuidePrograms(IReadOnlyDictionary<long, IReadOnlyList<LiveProgram>> ByChannel, bool Loaded)
{
    public static readonly GuidePrograms None = new GuidePrograms(new Dictionary<long, IReadOnlyList<LiveProgram>>(), false);

    public IReadOnlyList<LiveProgram>? Of(long channelId) => ByChannel.TryGetValue(channelId, out var list) ? list : null;
}

/// <summary>Watching a channel: the live player opens on it, changing channel within <paramref name="CategoryKey"/>.</summary>
public sealed record LiveWatch(long ChannelId, string CategoryKey, LiveChannel? Seen = null);

/// <summary>
/// Live TV (the Android app's LiveTvViewModel): the categories first
/// (Favourites, Recently watched, All channels, then the provider's groups, those of a region
/// gathered under it), then the chosen one's channels, a page at a time as they scroll, as a guide
/// that scrolls through time (asking for the programmes of the rows in view only) or as cards;
/// searched by name, number or what's on. A channel opens the live player
/// (<see cref="WatchRequested"/>), which changes channel within the category. The list opens back
/// at the channel last watched (<see cref="Landing"/>).
///
/// The category and the view are kept for the profile on this TV: Live TV opens as it was left,
/// after a restart too.
/// </summary>
public sealed class LiveTvModel : IDisposable
{
    /// <summary>The progress bars and the now line move this often.</summary>
    public const long TickMs = 15_000;

    /// <summary>What's on is asked for again at most this often.</summary>
    public const long RefreshFloorMs = 60_000;

    /// <summary>A notice (a favourite refused) shows this long.</summary>
    public const long NoticeMs = 4_000;

    const long HalfChunk = LiveRules.GuideChunkS / 2;

    readonly ILiveSource source;
    readonly IScheduler scheduler;
    readonly ChannelPaging paging;
    readonly List<IDisposable> watches = new List<IDisposable>();
    /// <summary>The guide's chunks by (chunk start, the page's ids): null while asked for, an empty guide once failed.</summary>
    readonly Dictionary<string, LiveGuide?> guides = new Dictionary<string, LiveGuide?>();
    readonly HashSet<string> failedGuides = new HashSet<string>();
    (long From, long To)? window;
    IDisposable? ticker;
    long lastRefresh = long.MinValue;
    int landings;
    bool gone;

    public LiveTvModel(ILiveSource source, IScheduler scheduler)
    {
        this.source = source;
        this.scheduler = scheduler;
        paging = new ChannelPaging(source, scheduler);
        var kept = source.Kept();
        State = new Signal<LivePage>(new LivePage
        {
            Category = LiveCategory.Parse(kept.Category),
            View = ViewOf(kept.View) ?? LiveView.Guide,
            Origin = source.Origin,
        });
        Now = new Signal<long>(scheduler.Now / 1000);
        GuideStart = LiveRules.GuideStart(Now.Value);
        watches.Add(paging.Pages.Watch(pages =>
        {
            Channels.Value = pages;
            ComputeGuide();
        }));
        watches.Add(source.OnChange(Refresh));
        paging.List(State.Value.Listed, State.Value.Query);
        LoadSummary();
        LoadGroups();
        Land();
        Tick();
    }

    public Signal<LivePage> State { get; }

    /// <summary>The listed category's channels, as far as they are loaded.</summary>
    public Signal<ChannelPages> Channels { get; } = new Signal<ChannelPages>(ChannelPages.Empty);

    /// <summary>The guide's programmes, for the pages of channels in view and the time in view, either side too.</summary>
    public Signal<GuidePrograms> Guide { get; } = new Signal<GuidePrograms>(GuidePrograms.None);

    /// <summary>Unix seconds, every <see cref="TickMs"/>: progress bars, the guide's now line.</summary>
    public Signal<long> Now { get; }

    /// <summary>Where the guide starts: the half hour before the one it opened in, fixed while the page is open.</summary>
    public long GuideStart { get; }

    /// <summary>
    /// Where the list opens: at the channel last watched, when it is in the category (unsearched);
    /// at the top otherwise. Null while it is looked for; once per category, search and return
    /// from the player (<see cref="Returned"/>).
    /// </summary>
    public Signal<Landing?> Landing { get; } = new Signal<Landing?>(null);

    /// <summary>A favourite the server refused: the notice says <see cref="LiveText.FavoriteFailed"/>.</summary>
    public event Action<Exception>? Told;

    /// <summary>A channel to watch: the live player opens on it.</summary>
    public event Action<LiveWatch>? WatchRequested;

    public void SetView(LiveView view)
    {
        State.Value = State.Value with { View = view };
        source.Keep(view: view.ToString());
    }

    /// <summary>Shows <paramref name="category"/>'s channels; a search ends, as the web's does.</summary>
    public void SetCategory(LiveCategory category)
    {
        State.Value = State.Value with { Category = category, Query = "", Found = Array.Empty<LiveGroup>() };
        source.Keep(category: category.Key);
        Listed();
    }

    public void SetQuery(string query)
    {
        if (query == State.Value.Query) return;
        var trimmedBefore = State.Value.Query.Trim();
        State.Value = State.Value with { Query = query, Found = LiveRules.FoundGroups(State.Value.Groups?.Groups ?? Array.Empty<LiveGroup>(), query) };
        if (query.Trim() != trimmedBefore) Listed();
    }

    /// <summary>Asks again for what failed, and what's on.</summary>
    public void Retry() => Refresh();

    /// <summary>Back from the live player: the list opens at the channel watched.</summary>
    public void Returned()
    {
        Refresh();
        Land();
    }

    /// <summary>Rows <paramref name="first"/> to <paramref name="last"/> of the list show: their pages are loaded. Cheap: call it as it scrolls.</summary>
    public void ShowingRows(int first, int last) => paging.Showing(first, last);

    /// <summary>The guide shows <paramref name="from"/> to <paramref name="to"/> (unix seconds): their chunks are loaded. Cheap: call it as it scrolls.</summary>
    public void ShowingTimes(long from, long to)
    {
        var span = (LiveRules.FloorDiv(from, HalfChunk), LiveRules.FloorDiv(to, HalfChunk));
        var was = window is { } w ? (LiveRules.FloorDiv(w.From, HalfChunk), LiveRules.FloorDiv(w.To, HalfChunk)) : ((long, long)?)null;
        if (was == span) return;
        window = (from, to);
        ComputeGuide();
    }

    /// <summary>Opens the live player on <paramref name="channel"/>, which changes channel within the category it was chosen in (every channel, from a search).</summary>
    public void Watch(LiveChannel channel)
    {
        var page = State.Value;
        var from = page.Searching ? new LiveCategory.All() : page.Category;
        WatchRequested?.Invoke(new LiveWatch(channel.Id, from.Key, channel));
    }

    /// <summary>Marks <paramref name="channel"/> a favourite, or not: at once on screen, put back if the server refuses.</summary>
    public void ToggleFavorite(LiveChannel channel)
    {
        var favorite = !channel.Favorite;
        Mark(channel.Id, favorite);
        Plays.When(scheduler, () => source.SetFavorite(channel.Id, favorite), () =>
        {
            if (gone) return;
            // Favourites' own pages and counts come from the server.
            LoadGroups();
            if (State.Value.Listed is LiveCategory.Favorites) paging.Refresh();
        }, e =>
        {
            if (gone) return;
            Mark(channel.Id, !favorite);
            Told?.Invoke(e);
        });
    }

    public void Dispose()
    {
        gone = true;
        ticker?.Dispose();
        paging.Dispose();
        foreach (var w in watches) w.Dispose();
        watches.Clear();
    }

    void Mark(long id, bool favorite)
    {
        paging.Patch(c => c.Id == id && c.Favorite != favorite ? c with { Favorite = favorite } : c);
        if (State.Value.Groups is { } groups)
            State.Value = State.Value with { Groups = groups with { Favorites = Math.Max(0, groups.Favorites + (favorite ? 1 : -1)) } };
    }

    void Listed()
    {
        var page = State.Value;
        paging.List(page.Listed, page.Query);
        Land();
    }

    void Refresh()
    {
        if (gone) return;
        lastRefresh = scheduler.Elapsed;
        LoadSummary();
        LoadGroups();
        paging.Refresh();
        foreach (var key in failedGuides) guides.Remove(key);
        failedGuides.Clear();
        ComputeGuide();
    }

    void LoadSummary() => Plays.When(scheduler, source.Summary, summary =>
    {
        if (!gone) State.Value = State.Value with { Allowed = summary.Allowed };
    }, _ =>
    {
        // Allowed until the server says otherwise; the channels say why nothing shows.
    });

    void LoadGroups() => Plays.When(scheduler, source.Groups, groups =>
    {
        if (gone) return;
        var page = State.Value;
        State.Value = page with { Groups = groups, Found = LiveRules.FoundGroups(groups.Groups, page.Query) };
    }, _ =>
    {
        // The counts stay as they were; the categories show without them.
    });

    /// <summary>Where the list opens: at the channel last watched when it is in the category, unsearched.</summary>
    void Land()
    {
        var page = State.Value;
        var category = page.Category;
        var q = page.Query.Trim();
        var at = ++landings;
        var id = source.Kept().Channel;
        if (id == null || q.Length > 0)
        {
            Landing.Value = new Landing(category, q, null);
            return;
        }
        Landing.Value = null;
        Plays.When(scheduler, () => source.Around(category, id.Value), around =>
        {
            if (gone || at != landings) return;
            var i = around.Channels.Select((c, n) => (c, n)).FirstOrDefault(x => x.c.Id == id.Value);
            Landing.Value = new Landing(category, q, i.c != null ? around.Offset + i.n : null);
        }, _ =>
        {
            if (!gone && at == landings) Landing.Value = new Landing(category, q, null);
        });
    }

    void Tick()
    {
        ticker = scheduler.After(TickMs, () =>
        {
            if (gone) return;
            Now.Value = scheduler.Now / 1000;
            // What's on moves on: a programme listed as now that ended asks for the pages again.
            var t = Now.Value;
            var ended = Channels.Value.Pages.Values.Any(page => page.Any(c => c.Now != null && c.Now.Stop <= t));
            if (ended && (lastRefresh == long.MinValue || scheduler.Elapsed - lastRefresh > RefreshFloorMs)) Refresh();
            Tick();
        });
    }

    /// <summary>The guide for the chunks in view and the pages of channels in view: what is held shows at once, the rest is asked for.</summary>
    void ComputeGuide()
    {
        if (gone) return;
        var chunks = window is { } w ? LiveRules.Chunks(GuideStart, w.From, w.To) : Array.Empty<long>();
        var idPages = Channels.Value.Pages.Values.Select(p => p.Select(c => c.Id).ToList()).Where(ids => ids.Count > 0).ToList();
        if (chunks.Count == 0 || idPages.Count == 0)
        {
            Guide.Value = GuidePrograms.None;
            return;
        }
        var shown = new List<LiveGuide>();
        var missing = new List<(string Key, long Chunk, List<long> Ids)>();
        var current = new HashSet<string>();
        var loaded = true;
        foreach (var chunk in chunks)
        {
            foreach (var ids in idPages)
            {
                var key = chunk + ":" + string.Join(",", ids);
                current.Add(key);
                if (guides.TryGetValue(key, out var guide))
                {
                    if (guide == null) loaded = false;
                    else shown.Add(guide);
                    continue;
                }
                loaded = false;
                guides[key] = null;
                missing.Add((key, chunk, ids));
            }
        }
        // Chunks of pages long scrolled away go: asked again should they show.
        if (guides.Count > MaxGuides)
            foreach (var key in guides.Keys.Where(k => !current.Contains(k) && guides[k] != null).ToList()) guides.Remove(key);
        Guide.Value = new GuidePrograms(LiveRules.Merge(shown), loaded);
        foreach (var (key, chunk, ids) in missing)
        {
            Plays.When(scheduler, () => source.Guide(chunk, ids), answer =>
            {
                if (gone) return;
                guides[key] = answer;
                ComputeGuide();
            }, _ =>
            {
                if (gone) return;
                // Shown as without programmes; asked again with the next refresh.
                guides[key] = new LiveGuide { From = chunk, To = chunk + LiveRules.GuideChunkS };
                failedGuides.Add(key);
                ComputeGuide();
            });
        }
    }

    /// <summary>Chunks of programmes held, past which those out of view go.</summary>
    const int MaxGuides = 60;

    static LiveView? ViewOf(string? name) => Enum.TryParse<LiveView>(name, out var v) && Enum.IsDefined(typeof(LiveView), v) ? v : null;
}
