using System;
using System.Collections.Generic;
using System.Globalization;
using System.Linq;
using System.Threading.Tasks;
using Pley.Tv.Core.Api;
using Pley.Tv.Core.Auth;

namespace Pley.Tv.Core.Ui;

/// <summary>A play in the history, as a row shows it (apps/web/src/components/HistoryList.tsx).</summary>
/// <param name="Title">The show for an episode, else the item's own title.</param>
/// <param name="Subtitle">"S2 · E5 · Glasshouse", "Artist · Album", or the year.</param>
/// <param name="Time">"3 hours ago".</param>
/// <param name="Device">"Chrome on Windows"; null when the server doesn't know.</param>
/// <param name="Art">The picture's whole addresses to try in order: its own, its parent's, its grandparent's.</param>
/// <param name="Square">Albums, tracks and artists have square art (48 × 48); the rest posters (34 × 51), in one 48 dp column.</param>
/// <param name="Opens">The page the row opens: the album for a track (the track picked out).</param>
public sealed record HistoryRow(long EntryId, long ItemId, string Title, string Subtitle, string Time, string? Device, IReadOnlyList<string> Art, bool Square, PageKey Opens)
{
    /// <summary>The row's second line: "S2 · E5 · Glasshouse · 3 hours ago · Chrome on Windows".</summary>
    public string Line => string.Join(" · ", new[] { Subtitle, Time, Device ?? "" }.Where(s => s.Length > 0));

    /// <summary>What the cross at the row's end says to a screen reader.</summary>
    public string RemoveLabel => HistoryText.Remove(Title);
}

/// <summary>The history's state (the Android app's HistoryUiState).</summary>
public abstract record HistoryState
{
    public sealed record Loading : HistoryState;

    /// <summary>It didn't load: "Couldn't load your watch history", and Retry.</summary>
    public sealed record Failed(Exception Error) : HistoryState;

    /// <param name="Total">Every play there is, however many are loaded.</param>
    /// <param name="Error">The server couldn't be asked again: what shows is what it said before.</param>
    public sealed record Ready(IReadOnlyList<HistoryRow> Rows, long Total, bool LoadingMore, Exception? Error = null) : HistoryState
    {
        /// <summary>Show more is offered.</summary>
        public bool CanLoadMore => Rows.Count < Total;
    }
}

/// <summary>What the history reads and changes: the app's state, or a fake in tests.</summary>
public interface IHistorySource
{
    /// <summary>The server's origin, which pictures are made whole against.</summary>
    string Origin { get; }

    /// <summary>A page of the profile's plays, newest first.</summary>
    Task<HistoryPage> History(int offset, int limit);

    Task Remove(long entryId);

    /// <summary>Calls <paramref name="changed"/> when what pages show may have changed (the server's events, a playback that ended).</summary>
    IDisposable OnChange(Action changed);
}

/// <summary>The history's source in the app: <see cref="AppState"/> and its API.</summary>
public sealed class AppHistorySource : IHistorySource
{
    readonly AppState app;

    public AppHistorySource(AppState app) => this.app = app;

    Api.Api Api => app.Api.Value ?? throw new SignedOutException();

    public string Origin => app.Server.Value ?? "";

    public Task<HistoryPage> History(int offset, int limit) => Api.History(offset, limit);

    public Task Remove(long entryId) => Api.RemoveHistory(entryId);

    public IDisposable OnChange(Action changed) => AppChanges.Follow(app.Changes, changed);
}

/// <summary>The history's rules (the Android app's historyRow and CachedHistory).</summary>
public static class HistoryRules
{
    /// <summary>Plays per page.</summary>
    public const int Page = 15;

    /// <summary>The most the server gives at once.</summary>
    public const int MaxLimit = 200;

    /// <summary><paramref name="entry"/> as its row shows it, its time worded from <paramref name="nowSeconds"/>, its pictures on <paramref name="origin"/>.</summary>
    public static HistoryRow Row(HistoryEntry entry, long nowSeconds, string origin)
    {
        var item = entry.Item;
        var (title, subtitle) = item.Kind switch
        {
            "episode" => (item.GrandparentTitle ?? item.Title, string.Join(" · ", new[] { Format.EpisodeLabel(item), item.Title }.Where(s => s.Length > 0))),
            "track" => (item.Title, string.Join(" · ", new[] { item.GrandparentTitle, item.ParentTitle }.Where(s => !string.IsNullOrEmpty(s)))),
            _ => (item.Title, item.Year?.ToString(CultureInfo.InvariantCulture) ?? ""),
        };
        PageKey opens = item.Kind switch
        {
            "track" => item.ParentId is { } album ? new AlbumPage(album, item.Id) : new PageKey.Item(item.Id),
            "album" => new AlbumPage(item.Id),
            "artist" => new ArtistPage(item.Id),
            _ => new PageKey.Item(item.Id),
        };
        var art = new List<string>();
        if (!string.IsNullOrEmpty(item.PosterUrl)) art.Add(item.PosterUrl!);
        if (item.ParentId != null) art.Add($"/api/items/{item.ParentId}/images/poster");
        if (item.GrandparentId != null) art.Add($"/api/items/{item.GrandparentId}/images/poster");
        return new HistoryRow(
            entry.Id,
            item.Id,
            title,
            subtitle,
            Format.Relative(entry.ViewedAt, nowSeconds),
            string.IsNullOrWhiteSpace(entry.Device) ? null : entry.Device,
            art.Select(a => Artwork.Absolute(a, origin)).ToList(),
            item.Kind is "track" or "album" or "artist",
            opens);
    }

    /// <summary>
    /// Where focus goes when the play <paramref name="entryId"/> is removed: the play below,
    /// else the one above (to the same place in it: the row, or its cross); null with neither.
    /// </summary>
    public static long? StandIn(IReadOnlyList<HistoryRow> rows, long entryId)
    {
        var at = -1;
        for (var i = 0; i < rows.Count; i++)
            if (rows[i].EntryId == entryId) at = i;
        if (at < 0) return null;
        if (at + 1 < rows.Count) return rows[at + 1].EntryId;
        return at > 0 ? rows[at - 1].EntryId : null;
    }
}

/// <summary>
/// Watch history (the web's HistoryList on the account page; the Android app's
/// HistoryViewModel): this profile's plays newest first, in pages of 15 with Show more, each
/// with when and where it played. Removing one takes it away at once (DELETE /api/history/{id})
/// and puts it back if the server refuses. Asked again (a change heard, Retry), it keeps as many
/// plays as were shown.
/// </summary>
public sealed class HistoryModel : IDisposable
{
    readonly IHistorySource source;
    readonly IScheduler scheduler;
    readonly IDisposable changes;
    HistoryPage? page;
    Exception? error;
    bool loadingMore, gone;
    int asks;

    public HistoryModel(IHistorySource source, IScheduler scheduler)
    {
        this.source = source;
        this.scheduler = scheduler;
        changes = source.OnChange(Retry);
        Retry();
    }

    public Signal<HistoryState> State { get; } = new Signal<HistoryState>(new HistoryState.Loading());

    /// <summary>Failures of Show more and of removals: said at the header's right, in the danger colour.</summary>
    public event Action<Exception>? Failed;

    /// <summary>Asks for the history again: the first page, or as many plays as are shown.</summary>
    public void Retry()
    {
        if (gone) return;
        var at = ++asks;
        var limit = Math.Max(HistoryRules.Page, Math.Min(HistoryRules.MaxLimit, page?.Entries.Count ?? 0));
        Plays.When(scheduler, () => source.History(0, limit), p =>
        {
            if (gone || at != asks) return;
            page = p;
            error = null;
            Compute();
        }, e =>
        {
            if (gone || at != asks) return;
            error = e;
            Compute();
        });
    }

    /// <summary>The next 15 plays.</summary>
    public void LoadMore()
    {
        if (loadingMore || page is not { } shown || shown.Entries.Count >= shown.Total) return;
        loadingMore = true;
        Compute();
        Plays.When(scheduler, () => source.History(shown.Entries.Count, HistoryRules.Page), more =>
        {
            loadingMore = false;
            if (gone) return;
            var now = page ?? shown;
            var known = new HashSet<long>(now.Entries.Select(e => e.Id));
            page = now with { Total = more.Total, Entries = now.Entries.Concat(more.Entries.Where(e => !known.Contains(e.Id))).ToList() };
            Compute();
        }, e =>
        {
            loadingMore = false;
            if (gone) return;
            Compute();
            Failed?.Invoke(e);
        });
    }

    /// <summary>Removes a play: the row goes at once, and comes back if the server refuses.</summary>
    public void Remove(long entryId)
    {
        if (page is not { } before || before.Entries.All(e => e.Id != entryId)) return;
        page = before with { Total = Math.Max(0, before.Total - 1), Entries = before.Entries.Where(e => e.Id != entryId).ToList() };
        var after = page;
        Compute();
        Plays.When(scheduler, () => source.Remove(entryId), () => { }, e =>
        {
            if (gone) return;
            if (ReferenceEquals(page, after)) page = before;
            Compute();
            Failed?.Invoke(e);
        });
    }

    public void Dispose()
    {
        gone = true;
        changes.Dispose();
    }

    void Compute()
    {
        if (gone) return;
        if (page == null)
        {
            State.Value = error != null ? new HistoryState.Failed(error) : new HistoryState.Loading();
            return;
        }
        var now = scheduler.Now / 1000;
        var origin = source.Origin;
        State.Value = new HistoryState.Ready(page.Entries.Select(e => HistoryRules.Row(e, now, origin)).ToList(), page.Total, loadingMore, error);
    }
}

/// <summary>What the TV's watch history says (the Android app's tv_account_history* strings).</summary>
public static class HistoryText
{
    public static string Title => Strings.T("history.title");
    public static string EmptyTitle => Strings.T("history.emptyTitle");
    public static string Empty => Strings.T("history.empty");
    public static string Failed => Strings.T("history.failed");
    public static string More => Strings.T("history.more");
    public static string Loading => Strings.T("history.loading");

    /// <summary>"1 play", "40 plays", after the title.</summary>
    public static string Count(long n) => Strings.Count("history.plays", n);

    /// <summary>"Remove Film 1 from history": the cross, to a screen reader.</summary>
    public static string Remove(string title) => Strings.T("history.remove", ("title", title));

    /// <summary>Show more's words: "Loading…" while the next plays come.</summary>
    public static string MoreLabel(HistoryState.Ready state) => state.LoadingMore ? Loading : More;

    /// <summary>The page's message in place of rows: failed (Retry), or no plays yet (Back); null otherwise.</summary>
    public static LibraryMessage? Message(HistoryState state) => state switch
    {
        HistoryState.Failed f => new LibraryMessage(LibraryText.FailedTitle(f.Error, Failed), LibraryText.FailedBody(f.Error), MessageAction.Retry),
        HistoryState.Ready { Rows.Count: 0 } => new LibraryMessage(EmptyTitle, Empty, MessageAction.Back),
        _ => null,
    };
}
