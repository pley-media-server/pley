using System;
using System.Collections.Generic;
using System.Linq;
using System.Threading.Tasks;
using Pley.Tv.Core.Api;
using Pley.Tv.Core.Auth;

namespace Pley.Tv.Core.Ui;

/// <summary>What a playlist plays, as the server names it.</summary>
public static class PlaylistKind
{
    /// <summary>Movies and episodes.</summary>
    public const string Video = "video";

    /// <summary>Tracks.</summary>
    public const string Audio = "audio";
}

/// <summary>Why a title won't do.</summary>
public enum TitleProblem
{
    Empty,
    TooLong,
}

/// <summary>
/// A playlist's card: its title, "12 items · 3h 12m", whether it plays video or music, the Smart
/// badge, and the whole addresses of its first entries' pictures for the mosaic (four make a 2 by
/// 2 mosaic, one or more the first alone, none its kind's glyph: list-video or list-music).
/// </summary>
public sealed record PlaylistCard(long Id, string Title, string Meta, bool Video, bool Smart, IReadOnlyList<string> Artwork)
{
    /// <summary>The caption's second line on TV: "12 items · 3h 12m · Smart".</summary>
    public string Caption => string.Join(" · ", new[] { Meta.Length > 0 ? Meta : null, Smart ? PlaylistText.SmartBadge : null }.Where(s => s != null));
}

/// <summary>
/// An entry of a playlist's page: its position from 1, its two lines (the web's), its length, and
/// its thumbnail (<paramref name="Tile"/>: a 16:9 still for video, the square cover for music,
/// with how much was played).
/// </summary>
public sealed record PlaylistRow(long EntryId, ItemSummary Item, int Number, string Title, string Subtitle, string Duration, TileModel Tile)
{
    /// <summary>What a screen reader says for the row: "1. Northern Lights, 2024, 1h 40m".</summary>
    public string Description => $"{Number}. {Title}, {Subtitle}, {Duration}";
}

/// <summary>
/// A playlist as its page shows it (apps/web/src/pages/playlists/PlaylistPage.tsx; design spec
/// §4.9): what it plays, its Smart badge, its title, its count and length with its filter in
/// words, then its entries.
/// </summary>
public sealed record PlaylistPage
{
    public long Id { get; init; }
    public string Title { get; init; } = "";
    /// <summary>Plays movies and episodes; else music.</summary>
    public bool Video { get; init; }
    /// <summary>Filled by its filter: read-only, its entries never moved or removed.</summary>
    public bool Smart { get; init; }
    /// <summary>"12 items · 3h 12m".</summary>
    public string Meta { get; init; } = "";
    /// <summary>A smart playlist's filter in words ("Genre: Comedy", "Newest first").</summary>
    public IReadOnlyList<string> Notes { get; init; } = Array.Empty<string>();
    /// <summary>The first entry, whose backdrop the TV page shows (<see cref="HomeRules.Backdrop"/>).</summary>
    public ItemSummary? Lead { get; init; }
    /// <summary>The lead's backdrop addresses, best first.</summary>
    public IReadOnlyList<string> Backdrop { get; init; } = Array.Empty<string>();
    public IReadOnlyList<PlaylistRow> Rows { get; init; } = Array.Empty<PlaylistRow>();

    public bool Editable => !Smart;
    public bool CanPlay => Rows.Count > 0;
    public bool CanShuffle => Rows.Count >= 2;
}

/// <summary>A playlist was made: open it. Or making it failed.</summary>
public abstract record CreateEvent
{
    public sealed record Created(long PlaylistId, string Title) : CreateEvent;

    public sealed record Failed(Exception Error) : CreateEvent;
}

/// <summary>What happened on a playlist's page, for a toast or a move.</summary>
public abstract record PlaylistEvent
{
    /// <summary><paramref name="Title"/>'s entry left the playlist: "Removed “Half Loop”". With an undo, <see cref="PlaylistModel.Undo"/> puts it back for a few seconds.</summary>
    public sealed record Removed(long EntryId, string Title) : PlaylistEvent;

    /// <summary>The playlist is gone: back to the playlists.</summary>
    public sealed record Deleted : PlaylistEvent;

    /// <summary>A music entry opens its album, which plays it.</summary>
    public sealed record Open(PageKey Key) : PlaylistEvent;

    public sealed record Failed(Exception Error) : PlaylistEvent;
}

/// <summary>What the playlists' pages and panels read and change: the app's state, or a fake in tests.</summary>
public interface IPlaylistSource
{
    /// <summary>The server's origin, which pictures are made whole against.</summary>
    string Origin { get; }

    /// <summary>The profile's playlists, by title.</summary>
    Task<IReadOnlyList<Playlist>> Playlists();

    Task<PlaylistDetail> Playlist(long id);

    /// <summary>A playlist called <paramref name="title"/> of <paramref name="itemIds"/>; <paramref name="kind"/> null lets the items decide. Its id.</summary>
    Task<long> Create(string title, string? kind, IReadOnlyList<long> itemIds);

    /// <summary>A smart playlist of <paramref name="libraryId"/>'s listing by <paramref name="filter"/>; its id.</summary>
    Task<long> CreateSmart(string title, long libraryId, string filter);

    /// <summary>The playlist renamed, as the server has it now.</summary>
    Task<PlaylistDetail> Rename(long id, string title);

    Task Delete(long id);

    /// <summary>Adds the items: shows, seasons, albums and artists add their episodes or tracks.</summary>
    Task Add(long id, IReadOnlyList<long> itemIds);

    Task Remove(long id, long entryId);

    /// <summary>These entries first, in this order; the playlist as the server has it now.</summary>
    Task<PlaylistDetail> Reorder(long id, IReadOnlyList<long> entryIds);

    /// <summary>An item being added to a playlist, from what the app kept or the server.</summary>
    Task<ItemSummary> Item(long id);

    /// <summary>Calls <paramref name="changed"/> when playlists may have changed: a change made here, or the server's events.</summary>
    IDisposable OnChange(Action changed);
}

/// <summary>The playlists' source in the app: <see cref="AppState"/> and its API. Each change made tells the playlists' pages.</summary>
public sealed class AppPlaylistSource : IPlaylistSource
{
    readonly AppState app;

    public AppPlaylistSource(AppState app) => this.app = app;

    Api.Api Api => app.Api.Value ?? throw new SignedOutException();

    public string Origin => app.Server.Value ?? "";

    public async Task<IReadOnlyList<Playlist>> Playlists() => (await Api.Playlists()).Playlists;

    public Task<PlaylistDetail> Playlist(long id) => Api.Playlist(id);

    public async Task<long> Create(string title, string? kind, IReadOnlyList<long> itemIds) => Changed(await Api.CreatePlaylist(title, kind, itemIds)).Id;

    public async Task<long> CreateSmart(string title, long libraryId, string filter) => Changed(await Api.CreateSmartPlaylist(title, libraryId, filter)).Id;

    public async Task<PlaylistDetail> Rename(long id, string title) => Changed(await Api.RenamePlaylist(id, title));

    public async Task Delete(long id)
    {
        await Api.DeletePlaylist(id);
        Changed(id);
    }

    public async Task Add(long id, IReadOnlyList<long> itemIds) => Changed(await Api.AddToPlaylist(id, itemIds));

    public async Task Remove(long id, long entryId)
    {
        await Api.RemoveFromPlaylist(id, entryId);
        Changed(id);
    }

    public async Task<PlaylistDetail> Reorder(long id, IReadOnlyList<long> entryIds) => Changed(await Api.ReorderPlaylist(id, entryIds));

    public async Task<ItemSummary> Item(long id) => app.CachedItem(id) ?? (app.Api.Value == null ? throw new SignedOutException() : await app.Item(id));

    public IDisposable OnChange(Action changed) => AppChanges.Follow(app.Changes, changed, app.PlaylistChanges);

    /// <summary>
    /// The playlists' pages fetch again. The pages call from the UI thread and the awaits come
    /// back to it, so the signal is set there.
    /// </summary>
    T Changed<T>(T answer)
    {
        app.PlaylistChanges.Value++;
        return answer;
    }
}

/// <summary>
/// The rules and words playlists share across their pages and panels (the Android app's
/// Playlists object; apps/web/src/pages/playlists, components/lists).
/// </summary>
public static class PlaylistRules
{
    /// <summary>A title is at most this long, as on the web.</summary>
    public const int TitleMax = 200;

    /// <summary>How long a removed entry can be put back: the toast's 3.5 s and a little.</summary>
    public const long UndoMs = 4_000;

    /// <summary>How long "Added to …" shows before the Add to playlist panel closes.</summary>
    public const long DoneMs = 1_400;

    /// <summary>The playlists page's covers a row.</summary>
    public const int Columns = 5;

    /// <summary>"12 items · 3h 12m", "1 item".</summary>
    public static string Meta(long itemCount, long durationMs) =>
        string.Join(" · ", new[] { PlaylistText.Items(itemCount), Format.Duration(durationMs) }.Where(s => s.Length > 0));

    /// <summary>What a playlist of <paramref name="kind"/>'s items plays: music for artists, albums and tracks, else video.</summary>
    public static string KindOf(string kind) => kind is "artist" or "album" or "track" ? PlaylistKind.Audio : PlaylistKind.Video;

    /// <summary><paramref name="ids"/> with the one at <paramref name="from"/> moved to <paramref name="to"/> (unchanged when <paramref name="from"/> is out of range), as the web's <c>moved</c>.</summary>
    public static IReadOnlyList<T> Moved<T>(IReadOnlyList<T> ids, int from, int to)
    {
        if (from < 0 || from >= ids.Count) return ids;
        var next = ids.ToList();
        var id = next[from];
        next.RemoveAt(from);
        next.Insert(Math.Max(0, Math.Min(to, next.Count)), id);
        return next;
    }

    /// <summary>What is wrong with <paramref name="title"/> as a playlist's title, or null when it will do.</summary>
    public static TitleProblem? Check(string title)
    {
        var t = title.Trim();
        if (t.Length == 0) return TitleProblem.Empty;
        if (t.Length > TitleMax) return TitleProblem.TooLong;
        return null;
    }

    /// <summary>
    /// How many entries adding <paramref name="items"/> makes: one for a movie, episode, track or
    /// extra, a show's, season's, album's or artist's episodes or tracks for the others; null when
    /// that isn't known (a container without its count).
    /// </summary>
    public static int? EntriesOf(IReadOnlyList<ItemSummary> items)
    {
        var total = 0;
        foreach (var item in items)
        {
            if (item.Kind is "movie" or "episode" or "track" or "extra") total++;
            else if (item.LeafCount is { } leaves) total += leaves;
            else return null;
        }
        return total;
    }

    /// <summary>
    /// A row's two lines, as the web's playlist page writes them: an episode's title over its show
    /// and number, a season's name over its show, else the title over the year (or what a card says).
    /// </summary>
    public static (string Title, string Subtitle) RowTitles(ItemSummary item) => item.Kind switch
    {
        "episode" => (item.Title, string.Join(" · ", new[] { item.GrandparentTitle ?? "", Format.EpisodeLabel(item) }.Where(s => s.Length > 0))),
        "season" => (ItemFormat.SeasonTitle(item.Title, item.Index), Format.CardSubtitle(item)),
        _ => (item.Title, Format.CardSubtitle(item)),
    };

    /// <summary>
    /// What is added, in words: "Severance — S1 · E2 — Half Loop", "Heat (1995)", a season's
    /// "Severance — Season 1", a track's "Swell — Harbour Lights".
    /// </summary>
    public static string FullTitle(ItemSummary item) => item.Kind switch
    {
        "episode" => string.Join(" — ", new[] { item.GrandparentTitle ?? "", Format.EpisodeLabel(item), item.Title }.Where(s => s.Length > 0)),
        "season" => string.Join(" — ", new[] { item.ParentTitle ?? "", ItemFormat.SeasonTitle(item.Title, item.Index) }.Where(s => s.Length > 0)),
        "track" => string.Join(" — ", new[] { item.Title, item.GrandparentTitle ?? "" }.Where(s => s.Length > 0)),
        _ => item.Year != null ? $"{item.Title} ({item.Year})" : item.Title,
    };

    /// <summary>What is added, in words: one item's full title, else how many.</summary>
    public static string TitleOf(IReadOnlyList<ItemSummary> items) => items.Count switch
    {
        0 => "",
        1 => FullTitle(items[0]),
        _ => PlaylistText.Items(items.Count),
    };

    /// <summary>A playlist as its card and the panels list it.</summary>
    public static PlaylistCard Card(Playlist playlist, string origin) => new PlaylistCard(
        playlist.Id,
        playlist.Title,
        Meta(playlist.ItemCount, playlist.DurationMs),
        playlist.Kind != PlaylistKind.Audio,
        !string.IsNullOrEmpty(playlist.SmartFilter),
        playlist.Artwork.Where(a => !string.IsNullOrEmpty(a)).Select(a => Artwork.Absolute(a, origin)).ToList());

    /// <summary>The page of <paramref name="detail"/>, its entries in <paramref name="order"/> (null: the server's), those <paramref name="hidden"/> left out.</summary>
    public static PlaylistPage Page(PlaylistDetail detail, string origin, IReadOnlyList<long>? order = null, ISet<long>? hidden = null)
    {
        var video = detail.Kind != PlaylistKind.Audio;
        var kind = video ? TileKind.Landscape : TileKind.Square;
        var gone = hidden == null ? new List<PlaylistEntry>() : detail.Entries.Where(e => hidden.Contains(e.EntryId)).ToList();
        IEnumerable<PlaylistEntry> entries = hidden == null ? detail.Entries : detail.Entries.Where(e => !hidden.Contains(e.EntryId));
        if (order != null)
        {
            var at = new Dictionary<long, int>();
            for (var i = 0; i < order.Count; i++) at[order[i]] = i;
            entries = entries.OrderBy(e => at.TryGetValue(e.EntryId, out var i) ? i : int.MaxValue);
        }
        var shown = entries.ToList();
        var rows = shown.Select((e, i) =>
        {
            var (title, subtitle) = RowTitles(e.Item);
            return new PlaylistRow(e.EntryId, e.Item, i + 1, title, subtitle, Format.Duration(e.Item.DurationMs), TileModel.Of(e.Item, kind, origin));
        }).ToList();
        var smart = !string.IsNullOrEmpty(detail.SmartFilter);
        var count = Math.Max(0, detail.ItemCount - gone.Count);
        var duration = Math.Max(0, detail.DurationMs - gone.Sum(e => e.Item.DurationMs ?? 0));
        var lead = shown.FirstOrDefault()?.Item ?? detail.Entries.FirstOrDefault()?.Item;
        return new PlaylistPage
        {
            Id = detail.Id,
            Title = detail.Title,
            Video = video,
            Smart = smart,
            Meta = Meta(count, duration),
            Notes = smart ? SmartFilter.Describe(detail.SmartFilter!) : Array.Empty<string>(),
            Lead = lead,
            Backdrop = lead == null ? Array.Empty<string>() : HomeRules.Backdrop(lead, origin),
            Rows = rows,
        };
    }

    /// <summary>
    /// Where focus goes when the entry <paramref name="entryId"/> is removed: the entry below,
    /// else the one above; null when it was the only one (Delete takes focus then).
    /// </summary>
    public static long? StandIn(IReadOnlyList<PlaylistRow> rows, long entryId)
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
/// The profile's playlists (apps/web/src/pages/playlists/PlaylistsPage.tsx; the Android app's
/// PlaylistsViewModel): a card each, by title. New playlist opens <see cref="NewPlaylistPage"/>
/// on TV; <see cref="Create"/> makes one here too. It asks again when a playlist changes.
/// </summary>
public sealed class PlaylistsModel : IDisposable
{
    readonly IPlaylistSource source;
    readonly IDisposable changes;
    readonly Fetch<IReadOnlyList<Playlist>> list;
    bool gone;

    public PlaylistsModel(IPlaylistSource source, IScheduler scheduler)
    {
        this.source = source;
        Scheduler = scheduler;
        list = new Fetch<IReadOnlyList<Playlist>>(scheduler, Compute);
        changes = source.OnChange(Retry);
        Retry();
    }

    IScheduler Scheduler { get; }

    public Signal<Loadable<IReadOnlyList<PlaylistCard>>> State { get; } =
        new Signal<Loadable<IReadOnlyList<PlaylistCard>>>(new Loadable<IReadOnlyList<PlaylistCard>>.Loading());

    /// <summary>A New playlist is on its way to the server.</summary>
    public Signal<bool> Busy { get; } = new Signal<bool>(false);

    /// <summary>A playlist made (open it), or a failure to tell.</summary>
    public event Action<CreateEvent>? Told;

    public void Retry()
    {
        if (!gone) list.Ask(source.Playlists);
    }

    /// <summary>Makes a playlist called <paramref name="title"/> for <paramref name="kind"/>; false (and nothing sent) when the title won't do or one is already being made.</summary>
    public bool Create(string title, string kind)
    {
        if (PlaylistRules.Check(title) != null || Busy.Value) return false;
        Busy.Value = true;
        var name = title.Trim();
        Plays.When(Scheduler, () => source.Create(name, kind, Array.Empty<long>()), id =>
        {
            Busy.Value = false;
            if (!gone) Told?.Invoke(new CreateEvent.Created(id, name));
        }, e =>
        {
            Busy.Value = false;
            if (!gone) Told?.Invoke(new CreateEvent.Failed(e));
        });
        return true;
    }

    public void Dispose()
    {
        gone = true;
        changes.Dispose();
        list.Drop();
    }

    void Compute()
    {
        if (gone) return;
        var origin = source.Origin;
        IReadOnlyList<PlaylistCard>? Cards(IReadOnlyList<Playlist>? l) => l?.Select(p => PlaylistRules.Card(p, origin)).ToList();
        State.Value = list.State switch
        {
            Loadable<IReadOnlyList<Playlist>>.Ready r => new Loadable<IReadOnlyList<PlaylistCard>>.Ready(Cards(r.Data)!),
            Loadable<IReadOnlyList<Playlist>>.Failed f => new Loadable<IReadOnlyList<PlaylistCard>>.Failed(f.Error, Cards(f.Stale)),
            _ => new Loadable<IReadOnlyList<PlaylistCard>>.Loading(),
        };
    }
}

/// <summary>
/// One playlist's page (the Android app's PlaylistViewModel, as the TV uses it).
///
/// - **Play and Shuffle.** Video plays as a queue of the playlist (<see cref="PlayQueue.Playlist"/>)
///   from its first entry, or from a random one. Music opens the entry's album, which plays it.
/// - **Rename and Delete** (Delete asked first by the page).
/// - **Moving entries**: on TV a move mode, <see cref="MoveBy"/> with <c>send: false</c> as Up and
///   Down move the lifted entry, <see cref="SendOrder"/> on OK, <see cref="CancelMoves"/> on Back.
///   The new order shows at once and goes to the server; when the server refuses, the order it
///   had comes back and <see cref="PlaylistEvent.Failed"/> says why. Moves made while one is on
///   its way are sent after it, the latest order only.
/// - **Removing entries:** the entry goes at once; with an undo it is sent once the undo has had
///   its time (<see cref="PlaylistRules.UndoMs"/>), else straight away. A refusal brings it back.
///
/// Smart playlists are read-only: their entries come from their filter. Changes still go to the
/// server after the page closes (removals waiting for their undo go when it is disposed); only
/// their events stop.
/// </summary>
public sealed class PlaylistModel : IDisposable
{
    readonly IPlaylistSource source;
    readonly IScheduler scheduler;
    readonly Random random;
    readonly long undoMs;
    readonly IDisposable changes;
    readonly HashSet<long> hidden = new HashSet<long>();
    readonly Dictionary<long, IDisposable> pending = new Dictionary<long, IDisposable>();
    PlaylistDetail? detail;
    Exception? error;
    /// <summary>Entries' order as moved here, until the server has it; null for the server's.</summary>
    IReadOnlyList<long>? order;
    /// <summary>An order is on its way; the latest order made meanwhile waits here.</summary>
    bool sending;
    IReadOnlyList<long>? next;
    int asks;
    bool gone;

    public PlaylistModel(IPlaylistSource source, long playlistId, IScheduler scheduler, Random? random = null, long undoMs = PlaylistRules.UndoMs)
    {
        this.source = source;
        this.scheduler = scheduler;
        this.random = random ?? new Random();
        this.undoMs = undoMs;
        PlaylistId = playlistId;
        changes = source.OnChange(Retry);
        Retry();
    }

    public long PlaylistId { get; }

    public Signal<Loadable<PlaylistPage>> State { get; } = new Signal<Loadable<PlaylistPage>>(new Loadable<PlaylistPage>.Loading());

    /// <summary>Toasts and moves: removed, deleted (back to the playlists), a failure, an album to open.</summary>
    public event Action<PlaylistEvent>? Told;

    /// <summary>Video to play, as a queue of the playlist.</summary>
    public event Action<PlayRequest>? PlayRequested;

    PlaylistPage? Page => State.Value.Value;

    public void Retry()
    {
        if (gone) return;
        var at = ++asks;
        Plays.When(scheduler, () => source.Playlist(PlaylistId), d =>
        {
            if (gone || at != asks) return;
            detail = d;
            error = null;
            Compute();
        }, e =>
        {
            if (gone || at != asks) return;
            error = e;
            Compute();
        });
    }

    /// <summary>Plays from the entry at <paramref name="from"/>, or from a random one when <paramref name="shuffle"/>.</summary>
    public void Play(int from = 0, bool shuffle = false)
    {
        if (Page is not { } page || page.Rows.Count == 0) return;
        if (!shuffle && (from < 0 || from >= page.Rows.Count)) return;
        var start = shuffle ? page.Rows[random.Next(page.Rows.Count)] : page.Rows[from];
        if (page.Video)
        {
            var item = start.Item;
            PlayRequested?.Invoke(new PlayRequest(item.Id, item.UserState.Watched ? 0 : item.UserState.PositionMs, false, Queue: new PlayQueue.Playlist(PlaylistId)));
        }
        else Told?.Invoke(new PlaylistEvent.Open(SearchRules.Opens(start.Item)));
    }

    /// <summary>Where the entry's "Go to" goes: its item's page (a track's album).</summary>
    public PageKey Opens(PlaylistRow row) => SearchRules.Opens(row.Item);

    /// <summary>Moves the entry at <paramref name="from"/> to <paramref name="to"/>: shown at once, and sent unless <paramref name="send"/> is false.</summary>
    public void Move(int from, int to, bool send = true)
    {
        if (ShownOrder() is not { } ids || from < 0 || from >= ids.Count || to < 0 || to >= ids.Count || from == to) return;
        var moved = PlaylistRules.Moved(ids, from, to);
        order = moved;
        Compute();
        if (send) Send(moved);
    }

    /// <summary>Moves <paramref name="entryId"/> <paramref name="by"/> places (-1 up, 1 down) from where it is shown.</summary>
    public void MoveBy(long entryId, int by, bool send = true)
    {
        var from = ShownOrder()?.ToList().IndexOf(entryId) ?? -1;
        if (from >= 0) Move(from, from + by, send);
    }

    /// <summary>Sends the order shown now (after moves made with <c>send: false</c>): OK drops a lifted entry.</summary>
    public void SendOrder()
    {
        if (order != null) Send(order);
    }

    /// <summary>Forgets moves not sent, back to the order the server has: Back puts a lifted entry back.</summary>
    public void CancelMoves()
    {
        order = null;
        Compute();
    }

    /// <summary>
    /// Removes <paramref name="entryId"/>. With <paramref name="undoable"/> it is sent after the
    /// undo's time, unless <see cref="Undo"/> comes first, and <see cref="PlaylistEvent.Removed"/>
    /// offers that; without (the TV's), it is sent at once and the page shows "Removed “…”" itself.
    /// </summary>
    public void Remove(long entryId, bool undoable)
    {
        if (Page is not { Editable: true } page || pending.ContainsKey(entryId) || hidden.Contains(entryId)) return;
        var row = page.Rows.FirstOrDefault(r => r.EntryId == entryId);
        if (row == null) return;
        hidden.Add(entryId);
        Compute();
        if (undoable)
        {
            pending[entryId] = scheduler.After(undoMs, () => SendRemoval(entryId));
            Told?.Invoke(new PlaylistEvent.Removed(entryId, row.Title));
        }
        else SendRemoval(entryId);
    }

    /// <summary>Puts back an entry removed with an undo, while it hasn't been sent.</summary>
    public void Undo(long entryId)
    {
        if (!pending.TryGetValue(entryId, out var timer)) return;
        timer.Dispose();
        pending.Remove(entryId);
        hidden.Remove(entryId);
        Compute();
    }

    /// <summary>Renames the playlist; false (and nothing sent) when <paramref name="title"/> won't do.</summary>
    public bool Rename(string title)
    {
        if (PlaylistRules.Check(title) != null) return false;
        var name = title.Trim();
        if (name == Page?.Title) return true;
        Plays.When(scheduler, () => source.Rename(PlaylistId, name), answer =>
        {
            if (gone) return;
            detail = answer;
            Compute();
        }, Fail);
        return true;
    }

    /// <summary>Deletes the playlist (the page asked first); <see cref="PlaylistEvent.Deleted"/> once it is gone.</summary>
    public void Delete() => Plays.When(scheduler, () => source.Delete(PlaylistId), () =>
    {
        if (!gone) Told?.Invoke(new PlaylistEvent.Deleted());
    }, Fail);

    public void Dispose()
    {
        if (gone) return;
        gone = true;
        changes.Dispose();
        // Removals still waiting for their undo go now.
        foreach (var pair in pending.ToList())
        {
            pair.Value.Dispose();
            SendRemoval(pair.Key);
        }
        pending.Clear();
    }

    /// <summary>The entries' order shown; null when they can't move.</summary>
    IReadOnlyList<long>? ShownOrder()
    {
        if (Page is not { Editable: true } page) return null;
        return page.Rows.Select(r => r.EntryId).ToList();
    }

    void Send(IReadOnlyList<long> ids)
    {
        if (sending)
        {
            next = ids;
            return;
        }
        sending = true;
        Plays.When(scheduler, () => source.Reorder(PlaylistId, ids), answer =>
        {
            detail = answer;
            // The server's answer holds this order; a newer move keeps its own.
            if (order != null && order.SequenceEqual(ids)) order = null;
            Sent();
        }, e =>
        {
            order = null;
            Fail(e);
            Sent();
        });
    }

    void Sent()
    {
        sending = false;
        Compute();
        if (next is { } waiting)
        {
            next = null;
            Send(waiting);
        }
    }

    void SendRemoval(long entryId)
    {
        pending.Remove(entryId);
        Plays.When(scheduler, () => source.Remove(PlaylistId, entryId), () =>
        {
            hidden.Remove(entryId);
            if (detail != null && detail.Entries.FirstOrDefault(e => e.EntryId == entryId) is { } entry)
                detail = detail with
                {
                    Entries = detail.Entries.Where(e => e.EntryId != entryId).ToList(),
                    ItemCount = Math.Max(0, detail.ItemCount - 1),
                    DurationMs = Math.Max(0, detail.DurationMs - (entry.Item.DurationMs ?? 0)),
                };
            Compute();
        }, e =>
        {
            hidden.Remove(entryId);
            Compute();
            Fail(e);
        });
    }

    void Fail(Exception e)
    {
        if (!gone) Told?.Invoke(new PlaylistEvent.Failed(e));
    }

    void Compute()
    {
        if (gone) return;
        var origin = source.Origin;
        PlaylistPage? PageOf(PlaylistDetail? d) => d == null ? null : PlaylistRules.Page(d, origin, order, hidden);
        State.Value = detail != null
            ? error != null ? new Loadable<PlaylistPage>.Failed(error, PageOf(detail)) : new Loadable<PlaylistPage>.Ready(PageOf(detail)!)
            : error != null ? new Loadable<PlaylistPage>.Failed(error) : new Loadable<PlaylistPage>.Loading();
    }
}
