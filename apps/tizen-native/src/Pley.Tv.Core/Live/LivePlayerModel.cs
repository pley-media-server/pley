using System;
using System.Collections.Generic;
using System.Linq;
using System.Threading.Tasks;
using Pley.Tv.Core.Api;
using Pley.Tv.Core.Ui;

namespace Pley.Tv.Core.Live;

/// <summary>A channel number being typed on the remote, or one that matched nothing.</summary>
public sealed record ChannelDigits(string Typed = "", string? NotFound = null)
{
    /// <summary>What the corner shows: the digits typed, "No channel 123", or nothing.</summary>
    public string? Text => Typed.Length > 0 ? Typed : NotFound != null ? LiveText.NoChannel(NotFound) : null;
}

/// <summary>
/// The live player (the Android app's LivePlayerViewModel): the channel playing, with what's on
/// now and next; channel up and down within the category it was opened from (the channels
/// either side of it in the server's pages, round the ends; in Recently watched, the order it had
/// as the player opened); a channel number typed, found among every channel; the category's
/// channels a page at a time for the channel list, which can show another category's (a channel
/// tuned from there makes channel up and down go through that one); watched as it airs, without
/// pausing or going back; a favourite. The channel is kept for the profile: the guide opens back
/// at it. The playback ends with the model.
///
/// The screen (the Android app's TvLivePlayerScreen): with nothing over the picture, Up and Down
/// and Channel up and down change channel (<see cref="Step"/>: Up the next), digits tune a
/// number, OK shows the controls (the channel list, the favourite) over what's on, Menu or Guide
/// the channel list, Info what's on for <see cref="InfoMs"/>; Back hides what shows, then leaves.
/// Play, pause, rewind, fast forward, Left and Right do nothing. The controls hide after
/// <see cref="ControlsMs"/> untouched while it plays.
/// </summary>
public sealed class LivePlayerModel : IDisposable
{
    public const long TickMs = 5_000;

    /// <summary>A typed number waits this long for another digit (the web's 1.4 s).</summary>
    public const long DigitsWaitMs = 1_400;

    public const long NotFoundMs = 2_000;

    public const int MaxDigits = 5;

    /// <summary>What's on shows this long when asked for (Info), once it plays.</summary>
    public const long InfoMs = 5_000;

    /// <summary>The controls hide after this long untouched while playing.</summary>
    public const long ControlsMs = 6_000;

    readonly ILiveSource source;
    readonly LivePlayback playback;
    readonly IScheduler scheduler;
    readonly ChannelPaging panel;
    readonly List<IDisposable> watches = new List<IDisposable>();
    /// <summary>What the lists knew of channels tuned from them: shown at once, before the server's own answer.</summary>
    readonly Dictionary<long, LiveChannel> known = new Dictionary<long, LiveChannel>();
    /// <summary>Recently watched's order as the player opened: watching changes it, zapping through it shouldn't.</summary>
    IReadOnlyList<LiveChannel>? recentOrder;
    IDisposable? ticker, digitsTimer;
    long current;
    int zaps, channelAsks, indexAsks;
    long? endedAsked;
    bool gone;

    /// <param name="seen">What the list knew of the channel, shown before the server answers.</param>
    public LivePlayerModel(ILiveSource source, LivePlayback playback, IScheduler scheduler, long channelId, string categoryKey, LiveChannel? seen = null)
    {
        this.source = source;
        this.playback = playback;
        this.scheduler = scheduler;
        if (seen != null) known[seen.Id] = seen;
        var category = LiveCategory.Parse(categoryKey);
        Category = new Signal<LiveCategory>(category);
        ListCategory = new Signal<LiveCategory>(category);
        Listed = new Signal<LiveCategory>(category);
        Now = new Signal<long>(scheduler.Now / 1000);
        panel = new ChannelPaging(source, scheduler);
        watches.Add(panel.Pages.Watch(p => List.Value = p));
        watches.Add(source.OnChange(() =>
        {
            panel.Refresh();
            LoadChannel(current);
        }));
        Plays.When(scheduler, source.Groups, g =>
        {
            if (!gone) Groups.Value = g;
        }, _ => { });
        current = channelId;
        TuneNow(channelId);
        Relist();
        Tick();
    }

    public Signal<LiveState> State => playback.State;

    /// <summary>The TV's player playing now, for the video plane; null while none is.</summary>
    public Player.IMediaEngine? Engine => playback.Engine;

    /// <summary>The channel playing, as the server has it now (a favourite set shows at once).</summary>
    public Signal<LiveChannel?> Channel { get; } = new Signal<LiveChannel?>(null);

    /// <summary>The server's origin, for logos.</summary>
    public string Origin => source.Origin;

    /// <summary>Unix seconds, every 5 s: what's on and how far it is.</summary>
    public Signal<long> Now { get; }

    /// <summary>A channel number being typed.</summary>
    public Signal<ChannelDigits> Digits { get; } = new Signal<ChannelDigits>(new ChannelDigits());

    /// <summary>The category channel up and down go through.</summary>
    public Signal<LiveCategory> Category { get; }

    /// <summary>The category the channel list shows: the player's, unless another was chosen in it.</summary>
    public Signal<LiveCategory> ListCategory { get; }

    /// <summary>What the channel list is searched for: among every channel, whatever category it shows.</summary>
    public Signal<string> ListQuery { get; } = new Signal<string>("");

    /// <summary>What the channel list's channels are: <see cref="ListCategory"/>, or every channel while searching.</summary>
    public Signal<LiveCategory> Listed { get; }

    /// <summary>The groups and the counts, for choosing another in the channel list; null while unknown.</summary>
    public Signal<LiveGroups?> Groups { get; } = new Signal<LiveGroups?>(null);

    /// <summary>The channel list's channels (those its search finds), as far as they are loaded.</summary>
    public Signal<ChannelPages> List { get; } = new Signal<ChannelPages>(ChannelPages.Empty);

    /// <summary>Where the channel playing is in the channel list; null while unknown, or when it isn't in it.</summary>
    public Signal<int?> ListIndex { get; } = new Signal<int?>(null);

    /// <summary>
    /// Watches <paramref name="channelId"/> in place of what plays; <paramref name="seen"/> is what a
    /// list knew of it. Chosen in another category's list (<paramref name="from"/>), channel up and
    /// down go through that one from then on.
    /// </summary>
    public void Tune(long channelId, LiveChannel? seen = null, LiveCategory? from = null)
    {
        if (seen != null) known[seen.Id] = seen;
        if (from != null && from != Category.Value)
        {
            Category.Value = from;
            recentOrder = null;
        }
        if (channelId == State.Value.ChannelId && State.Value.Is is not LivePhase.Failed) return;
        TuneNow(channelId);
    }

    /// <summary>Channel up (<paramref name="by"/> 1, the next in the category) or down (-1), round its ends.</summary>
    public async void Step(int by)
    {
        var at = ++zaps;
        LiveChannel? next;
        try
        {
            next = await Neighbour(current, by);
        }
        catch (Exception)
        {
            // Not answered: the channel stays.
            return;
        }
        if (gone || at != zaps || next == null) return;
        Tune(next.Id, next);
    }

    /// <summary>Rows <paramref name="first"/> to <paramref name="last"/> of the channel list show: their pages are loaded.</summary>
    public void ShowingListRows(int first, int last) => panel.Showing(first, last);

    /// <summary>The channel list opens: on the category channel up and down go through, unsearched.</summary>
    public void OpenList()
    {
        ListCategory.Value = Category.Value;
        ListQuery.Value = "";
        Relist();
    }

    /// <summary>The channel list shows <paramref name="category"/>'s channels; channel up and down keep theirs until one is tuned.</summary>
    public void ShowList(LiveCategory category)
    {
        ListCategory.Value = category;
        ListQuery.Value = "";
        Relist();
    }

    /// <summary>Searches the channel list (every channel); "" lists <see cref="ListCategory"/>'s again.</summary>
    public void SearchList(string query)
    {
        ListQuery.Value = query;
        Relist();
    }

    /// <summary>
    /// A digit of a channel number: the channel numbered so among every channel is tuned once no
    /// digit came for 1.4 s, or "No channel 123" shows for 2 s.
    /// </summary>
    public void Digit(char d)
    {
        if (d < '0' || d > '9') return;
        var typed = Digits.Value.Typed + d;
        if (typed.Length > MaxDigits) typed = typed.Substring(0, MaxDigits);
        Digits.Value = new ChannelDigits(typed);
        digitsTimer?.Dispose();
        digitsTimer = scheduler.After(DigitsWaitMs, () => Plays.When(scheduler, () => source.Numbered(typed), match => Matched(typed, match), _ => Matched(typed, null)));
    }

    void Matched(string typed, LiveChannel? match)
    {
        if (gone || Digits.Value.Typed != typed) return;
        if (match != null)
        {
            Digits.Value = new ChannelDigits();
            Tune(match.Id, match);
            return;
        }
        Digits.Value = new ChannelDigits(NotFound: typed);
        digitsTimer = scheduler.After(NotFoundMs, () =>
        {
            if (Digits.Value.NotFound == typed) Digits.Value = new ChannelDigits();
        });
    }

    public void Retry() => playback.Retry();

    /// <summary>Every connection being in use: watches the channel playing on <paramref name="channelId"/>'s connection instead (one the server said may be).</summary>
    public void TakeOver(long channelId) => playback.Retry(takeOver: channelId);

    /// <summary>The app went to the background: the connection and the player are let go.</summary>
    public void Stop() => playback.Stop();

    /// <summary>Back in front: the channel plays again.</summary>
    public void Resume() => playback.Resume();

    /// <summary>The favourite: at once on screen, put back if the server refuses (the star shows it).</summary>
    public void ToggleFavorite()
    {
        var c = Channel.Value;
        if (c == null) return;
        var favorite = !c.Favorite;
        Mark(c.Id, favorite);
        Plays.When(scheduler, () => source.SetFavorite(c.Id, favorite), () => { }, _ =>
        {
            if (!gone) Mark(c.Id, !favorite);
        });
    }

    public void Dispose()
    {
        gone = true;
        ticker?.Dispose();
        digitsTimer?.Dispose();
        panel.Dispose();
        foreach (var w in watches) w.Dispose();
        watches.Clear();
        playback.Close();
    }

    void Mark(long id, bool favorite)
    {
        if (Channel.Value is { } c && c.Id == id) Channel.Value = c with { Favorite = favorite };
        if (known.TryGetValue(id, out var k)) known[id] = k with { Favorite = favorite };
        panel.Patch(x => x.Id == id && x.Favorite != favorite ? x with { Favorite = favorite } : x);
    }

    void TuneNow(long channelId)
    {
        current = channelId;
        endedAsked = null;
        source.Keep(channel: channelId);
        Channel.Value = known.TryGetValue(channelId, out var seen) ? seen : Channel.Value?.Id == channelId ? Channel.Value : null;
        playback.Tune(channelId);
        LoadChannel(channelId);
        Locate();
    }

    void LoadChannel(long channelId)
    {
        var at = ++channelAsks;
        Plays.When(scheduler, () => source.Channel(channelId), channel =>
        {
            if (gone || at != channelAsks) return;
            known[channel.Id] = channel;
            Channel.Value = channel;
        }, _ => { });
    }

    void Relist()
    {
        var searching = !string.IsNullOrWhiteSpace(ListQuery.Value);
        Listed.Value = searching ? new LiveCategory.All() : ListCategory.Value;
        panel.List(Listed.Value, ListQuery.Value);
        Locate();
    }

    /// <summary>Where the channel playing is in the channel list, unsearched.</summary>
    void Locate()
    {
        var at = ++indexAsks;
        if (!string.IsNullOrWhiteSpace(ListQuery.Value))
        {
            ListIndex.Value = null;
            return;
        }
        var id = current;
        Plays.When(scheduler, () => source.Around(ListCategory.Value, id), page =>
        {
            if (gone || at != indexAsks) return;
            var i = page.Channels.Select((c, n) => (c, n)).FirstOrDefault(x => x.c.Id == id);
            ListIndex.Value = i.c != null ? page.Offset + i.n : null;
        }, _ =>
        {
            if (!gone && at == indexAsks) ListIndex.Value = null;
        });
    }

    void Tick()
    {
        ticker = scheduler.After(TickMs, () =>
        {
            if (gone) return;
            Now.Value = scheduler.Now / 1000;
            // What's on moves on while watching: asked again when the programme playing ends.
            if (Channel.Value?.Now is { } on && on.Stop <= Now.Value && endedAsked != on.Stop)
            {
                endedAsked = on.Stop;
                LoadChannel(current);
                panel.Refresh();
            }
            Tick();
        });
    }

    /// <summary>The channel <paramref name="by"/> places from <paramref name="id"/> in the category, round its ends; null when it has no other.</summary>
    async Task<LiveChannel?> Neighbour(long id, int by)
    {
        var category = Category.Value;
        if (category is LiveCategory.Recent)
        {
            var list = recentOrder ??= (await source.Page(category, "", 0)).Channels;
            if (list.Count < 2) return null;
            var at = Math.Max(0, IndexIn(list, id));
            return list[FloorMod(at + by, list.Count)];
        }
        var page = await source.Around(category, id);
        if (page.Channels.Count == 0) return null;
        var i = IndexIn(page.Channels, id);
        // Not in this category (tuned by number, say): its first channel.
        if (i < 0) return page.Channels[0];
        var j = i + by;
        if (j >= 0 && j < page.Channels.Count) return page.Channels[j];
        var pages = (page.Total + LiveRules.Page - 1) / LiveRules.Page;
        if (pages <= 1) return page.Channels[FloorMod(j, page.Channels.Count)];
        var other = FloorMod(page.Offset / LiveRules.Page + by, pages);
        var next = (await source.Page(category, "", other)).Channels;
        if (next.Count == 0) return null;
        return by < 0 ? next[next.Count - 1] : next[0];
    }

    static int IndexIn(IReadOnlyList<LiveChannel> list, long id)
    {
        for (var i = 0; i < list.Count; i++)
            if (list[i].Id == id) return i;
        return -1;
    }

    static int FloorMod(int a, int n) => ((a % n) + n) % n;
}

/// <summary>
/// Whether Live TV has a way in (the rail's Live TV, after Home): open to this profile and with
/// channels, as the web's sidebar shows it (the Android app's LiveTvEntryViewModel). Null while
/// unknown, so nothing pops in and out; a failure with nothing known is 0.
/// </summary>
public sealed class LiveTvEntry
{
    readonly ILiveSource source;
    readonly IScheduler scheduler;
    LiveSummary? last;

    public LiveTvEntry(ILiveSource source, IScheduler scheduler)
    {
        this.source = source;
        this.scheduler = scheduler;
        Refresh();
    }

    /// <summary>The channels there are when Live TV shows; 0 when it doesn't; null while unknown.</summary>
    public Signal<int?> Channels { get; } = new Signal<int?>(null);

    /// <summary>Asks the server again (a sign-in, a profile switch, the lineup changed).</summary>
    public void Refresh() => Plays.When(scheduler, source.Summary, summary =>
    {
        last = summary;
        Channels.Value = summary.Available ? summary.Channels : 0;
    }, _ => Channels.Value = last is { Available: true } s ? s.Channels : 0);
}
