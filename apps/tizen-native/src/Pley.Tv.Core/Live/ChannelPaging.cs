using System;
using System.Collections.Generic;
using System.Linq;
using Pley.Tv.Core.Api;
using Pley.Tv.Core.Ui;

namespace Pley.Tv.Core.Live;

/// <summary>
/// A category's channels as far as they are loaded: <see cref="Total"/> in the list (null until
/// its first page is in), and the pages in view, by number.
/// </summary>
public sealed record ChannelPages
{
    public static readonly ChannelPages Empty = new ChannelPages();

    public int? Total { get; init; }

    public IReadOnlyDictionary<int, IReadOnlyList<LiveChannel>> Pages { get; init; } = new Dictionary<int, IReadOnlyList<LiveChannel>>();

    /// <summary>The first page failed, with nothing to show.</summary>
    public Exception? Error { get; init; }

    /// <summary>The channel at <paramref name="index"/>, once its page is in; null for a row still loading (a skeleton).</summary>
    public LiveChannel? this[int index]
    {
        get
        {
            if (index < 0 || !Pages.TryGetValue(index / LiveRules.Page, out var page)) return null;
            var at = index % LiveRules.Page;
            return at < page.Count ? page[at] : null;
        }
    }

    /// <summary>Every channel loaded, in order.</summary>
    public IReadOnlyList<LiveChannel> Loaded => Pages.OrderBy(p => p.Key).SelectMany(p => p.Value).ToList();

    /// <summary>A row's key on the page: the channel's id, or a negative number for a row still loading.</summary>
    public long Key(int index) => this[index]?.Id ?? -index - 1L;
}

/// <summary>
/// A category's channels a page at a time, as a list scrolls (<see cref="Showing"/>): its first
/// page, which says how many there are, and the pages covering the rows in view, so a lineup of
/// tens of thousands costs what's on screen (the Android app's ChannelPaging). Pages scrolled
/// away are kept a while, for scrolling back.
/// </summary>
public sealed class ChannelPaging : IDisposable
{
    /// <summary>Rows asked for either side of what shows.</summary>
    public const int Margin = 20;

    /// <summary>Pages kept besides those in view.</summary>
    const int Held = 8;

    /// <summary>A page that failed is asked for again after this long, as it shows.</summary>
    const long RetryMs = 10_000;

    readonly ILiveSource source;
    readonly IScheduler scheduler;
    readonly Dictionary<int, IReadOnlyList<LiveChannel>> held = new Dictionary<int, IReadOnlyList<LiveChannel>>();
    readonly HashSet<int> pending = new HashSet<int>();
    readonly HashSet<int> stale = new HashSet<int>();
    readonly Dictionary<int, long> failedAt = new Dictionary<int, long>();
    (int First, int Last) rows = (0, 0);
    int? total;
    Exception? error;
    int generation;
    bool listing, gone;

    public ChannelPaging(ILiveSource source, IScheduler scheduler)
    {
        this.source = source;
        this.scheduler = scheduler;
    }

    public Signal<ChannelPages> Pages { get; } = new Signal<ChannelPages>(ChannelPages.Empty);

    public LiveCategory Category { get; private set; } = new LiveCategory.All();

    /// <summary>What the channels are searched for, trimmed; empty unsearched.</summary>
    public string Query { get; private set; } = "";

    /// <summary>Lists <paramref name="category"/>'s channels matching <paramref name="query"/>, from the top; the same list again changes nothing.</summary>
    public void List(LiveCategory category, string query)
    {
        var q = query.Trim();
        if (listing && category == Category && q == Query) return;
        listing = true;
        Category = category;
        Query = q;
        generation++;
        held.Clear();
        pending.Clear();
        stale.Clear();
        failedAt.Clear();
        total = null;
        error = null;
        rows = (0, 0);
        Pages.Value = ChannelPages.Empty;
        Load();
    }

    /// <summary>Rows <paramref name="first"/> to <paramref name="last"/> are in view: their pages are loaded. Cheap: call it as the list scrolls.</summary>
    public void Showing(int first, int last)
    {
        var f = Math.Max(0, first);
        var next = (f, Math.Max(f, last));
        if (next == rows) return;
        rows = next;
        Load();
        Publish();
    }

    /// <summary>Asks again for the pages in view (what's on moved on, the lineup changed), showing them meanwhile; the rest when they show.</summary>
    public void Refresh()
    {
        if (!listing) return;
        foreach (var p in held.Keys) stale.Add(p);
        failedAt.Clear();
        if (total == null) error = null;
        Load();
    }

    /// <summary>A channel changed here (a favourite marked): every page shows it at once.</summary>
    public void Patch(Func<LiveChannel, LiveChannel> change)
    {
        foreach (var p in held.Keys.ToList()) held[p] = held[p].Select(change).ToList();
        Publish();
    }

    public void Dispose() => gone = true;

    IReadOnlyList<int> Wanted()
    {
        var from = Math.Max(0, rows.First - Margin) / LiveRules.Page;
        var to = (rows.Last + Margin) / LiveRules.Page;
        if (total is { } t) to = Math.Min(to, Math.Max(0, (t - 1) / LiveRules.Page));
        var wanted = new SortedSet<int> { 0 };
        for (var p = from; p <= to; p++) wanted.Add(p);
        return wanted.ToList();
    }

    void Load()
    {
        if (gone || !listing) return;
        var now = scheduler.Elapsed;
        foreach (var p in Wanted())
        {
            if (pending.Contains(p)) continue;
            if (held.ContainsKey(p) && !stale.Contains(p)) continue;
            if (failedAt.TryGetValue(p, out var at) && now - at < RetryMs) continue;
            Ask(p);
        }
    }

    void Ask(int page)
    {
        var at = generation;
        pending.Add(page);
        var category = Category;
        var query = Query;
        Ui.Plays.When(scheduler, () => source.Page(category, query, page), answer =>
        {
            if (gone || at != generation) return;
            pending.Remove(page);
            stale.Remove(page);
            failedAt.Remove(page);
            held[page] = answer.Channels;
            if (page == 0)
            {
                total = Math.Max(0, answer.Total);
                error = null;
            }
            Trim();
            Publish();
            // The total says how far the pages go: those in view past the first come now.
            if (page == 0) Load();
        }, e =>
        {
            if (gone || at != generation) return;
            pending.Remove(page);
            stale.Remove(page);
            failedAt[page] = scheduler.Elapsed;
            if (page == 0 && total == null) error = e;
            Publish();
        });
    }

    /// <summary>Pages far from the view go, past those kept for scrolling back.</summary>
    void Trim()
    {
        var wanted = new HashSet<int>(Wanted());
        var middle = (rows.First + rows.Last) / 2 / LiveRules.Page;
        foreach (var p in held.Keys.Where(p => !wanted.Contains(p)).OrderByDescending(p => Math.Abs(p - middle)).ToList())
        {
            if (held.Count <= wanted.Count + Held) break;
            held.Remove(p);
            stale.Remove(p);
        }
    }

    void Publish()
    {
        if (gone) return;
        var pages = new Dictionary<int, IReadOnlyList<LiveChannel>>();
        foreach (var p in Wanted())
            if (held.TryGetValue(p, out var channels)) pages[p] = channels;
        Pages.Value = new ChannelPages { Total = total, Pages = pages, Error = total == null ? error : null };
    }
}
