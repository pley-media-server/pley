using System;
using System.Collections.Generic;
using System.Linq;
using Pley.Tv.Core.Api;

namespace Pley.Tv.Core.Ui;

/// <summary>The Add to playlist panel: what is being added, and the playlists it can go in.</summary>
/// <param name="Title">"Doctor Who — S1 · E2 — The End of the World", "Her (2013)", "3 items"; null while it loads.</param>
/// <param name="Kind">What a playlist must play to take it (<see cref="PlaylistKind"/>); null while that isn't known.</param>
/// <param name="Playlists">The profile's playlists of that kind, smart ones left out (they fill themselves).</param>
/// <param name="Busy">A change is on its way: the panel's rows wait.</param>
/// <param name="Lead">The first item, whose backdrop the TV shows behind the panel; null while it loads.</param>
public sealed record AddToPlaylistState(string? Title, string? Kind, Loadable<IReadOnlyList<PlaylistCard>> Playlists, bool Busy, ItemSummary? Lead = null)
{
    /// <summary>The lead's backdrop addresses, best first.</summary>
    public IReadOnlyList<string> Backdrop { get; init; } = Array.Empty<string>();

    /// <summary>Adding to a playlist of music.</summary>
    public bool Music => Kind == PlaylistKind.Audio;

    /// <summary>A new playlist can be named and made: what it plays is known and nothing is on its way.</summary>
    public bool CanCreate => Kind != null && !Busy;
}

/// <summary>The outcome of adding, for the words the panel shows before it closes.</summary>
public abstract record AddEvent
{
    /// <summary>
    /// Added to <paramref name="Playlist"/>: <paramref name="Entries"/> entries, when known. 0 is
    /// "Nothing to add", 1 or unknown "Added to X", more "Added N items to X"
    /// (<see cref="PlaylistText.Added"/>).
    /// </summary>
    public sealed record Added(string Playlist, int? Entries) : AddEvent;

    /// <summary>A toast; the panel stays.</summary>
    public sealed record Failed(Exception Error) : AddEvent;
}

/// <summary>
/// Add to playlist (apps/web/src/components/lists/AddToPlaylistModal.tsx; the Android app's
/// AddToPlaylistViewModel), a side panel on TV: the full title of what is added, a new playlist
/// named on the spot (video or music, as the items are), and the profile's playlists that take
/// it. Shows, seasons, albums and artists add all their episodes or tracks. Once added, the panel
/// says so for <see cref="PlaylistRules.DoneMs"/> and closes.
/// </summary>
public sealed class AddToPlaylistModel : IDisposable
{
    readonly IPlaylistSource source;
    readonly IScheduler scheduler;
    readonly IReadOnlyList<long> itemIds;
    readonly Dictionary<long, ItemSummary?> known = new Dictionary<long, ItemSummary?>();
    readonly Fetch<IReadOnlyList<Playlist>> playlists;
    readonly IDisposable changes;
    bool busy, gone;

    public AddToPlaylistModel(IPlaylistSource source, IReadOnlyList<long> itemIds, IScheduler scheduler)
    {
        this.source = source;
        this.scheduler = scheduler;
        this.itemIds = itemIds;
        playlists = new Fetch<IReadOnlyList<Playlist>>(scheduler, Compute);
        changes = source.OnChange(() => playlists.Ask(source.Playlists));
        foreach (var id in itemIds)
        {
            var at = id;
            // An item that fails to load is left out of what is added in words; it is still sent.
            Plays.When(scheduler, () => source.Item(at), item =>
            {
                known[at] = item;
                Compute();
            }, _ =>
            {
                known[at] = null;
                Compute();
            });
        }
        playlists.Ask(source.Playlists);
        Compute();
    }

    /// <summary>As <see cref="AddToPlaylistPage"/> opens it.</summary>
    public AddToPlaylistModel(IPlaylistSource source, AddToPlaylistPage key, IScheduler scheduler) : this(source, new[] { key.ItemId }, scheduler)
    {
    }

    public Signal<AddToPlaylistState> State { get; } =
        new Signal<AddToPlaylistState>(new AddToPlaylistState(null, null, new Loadable<IReadOnlyList<PlaylistCard>>.Loading(), false));

    /// <summary>Added (the panel says so and closes), or a failure (a toast, and the panel stays).</summary>
    public event Action<AddEvent>? Told;

    /// <summary>The items, once each has loaded or failed; null until then.</summary>
    IReadOnlyList<ItemSummary>? Items =>
        itemIds.All(known.ContainsKey) ? itemIds.Select(id => known[id]).Where(i => i != null).Select(i => i!).ToList() : null;

    /// <summary>Adds the items to <paramref name="playlist"/>.</summary>
    public void Add(PlaylistCard playlist)
    {
        if (busy) return;
        var entries = PlaylistRules.EntriesOf(Items ?? Array.Empty<ItemSummary>());
        Change(() => source.Add(playlist.Id, itemIds), playlist.Title, entries);
    }

    /// <summary>Makes a playlist called <paramref name="title"/> holding the items; false (and nothing sent) when the title won't do or a change is on its way.</summary>
    public bool Create(string title)
    {
        if (PlaylistRules.Check(title) != null || busy) return false;
        var name = title.Trim();
        var kind = State.Value.Kind;
        var entries = PlaylistRules.EntriesOf(Items ?? Array.Empty<ItemSummary>());
        Change(() => source.Create(name, kind, itemIds), name, entries);
        return true;
    }

    public void Dispose()
    {
        gone = true;
        changes.Dispose();
        playlists.Drop();
    }

    void Change<T>(Func<System.Threading.Tasks.Task<T>> call, string playlist, int? entries)
    {
        busy = true;
        Compute();
        Plays.When(scheduler, call, _ =>
        {
            busy = false;
            Compute();
            if (!gone) Told?.Invoke(new AddEvent.Added(playlist, entries));
        }, e =>
        {
            busy = false;
            Compute();
            if (!gone) Told?.Invoke(new AddEvent.Failed(e));
        });
    }

    void Change(Func<System.Threading.Tasks.Task> call, string playlist, int? entries) =>
        Change(() => call().ContinueWith(t =>
        {
            t.GetAwaiter().GetResult();
            return true;
        }, System.Threading.CancellationToken.None, System.Threading.Tasks.TaskContinuationOptions.ExecuteSynchronously, System.Threading.Tasks.TaskScheduler.Default), playlist, entries);

    void Compute()
    {
        if (gone) return;
        var items = Items;
        var origin = source.Origin;
        var kind = items == null ? null : items.Count > 0 ? PlaylistRules.KindOf(items[0].Kind) : PlaylistKind.Video;
        IReadOnlyList<PlaylistCard>? Cards(IReadOnlyList<Playlist>? list) => list?
            .Where(p => string.IsNullOrEmpty(p.SmartFilter) && (kind == null || p.Kind == kind))
            .Select(p => PlaylistRules.Card(p, origin)).ToList();
        Loadable<IReadOnlyList<PlaylistCard>> cards = playlists.State switch
        {
            Loadable<IReadOnlyList<Playlist>>.Ready r => new Loadable<IReadOnlyList<PlaylistCard>>.Ready(Cards(r.Data)!),
            Loadable<IReadOnlyList<Playlist>>.Failed f => new Loadable<IReadOnlyList<PlaylistCard>>.Failed(f.Error, Cards(f.Stale)),
            _ => new Loadable<IReadOnlyList<PlaylistCard>>.Loading(),
        };
        var lead = items?.FirstOrDefault();
        State.Value = new AddToPlaylistState(items == null ? null : PlaylistRules.TitleOf(items), kind, cards, busy, lead)
        {
            Backdrop = lead == null ? Array.Empty<string>() : HomeRules.Backdrop(lead, origin),
        };
    }
}

/// <summary>What a new playlist starts from.</summary>
public abstract record NewPlaylistMode
{
    /// <summary>A playlist of <paramref name="ItemIds"/> (their kind decides what it plays), or an empty one of the kind chosen.</summary>
    public sealed record Items(IReadOnlyList<long> ItemIds) : NewPlaylistMode;

    /// <summary>A smart playlist of a library's filters (<paramref name="Filter"/>, in Plex's syntax).</summary>
    public sealed record Smart(long LibraryId, string Filter) : NewPlaylistMode;

    /// <summary>The mode a page opens: <see cref="NewPlaylistPage"/> an empty playlist, <see cref="SaveSmartPlaylistPage"/> a smart one.</summary>
    public static NewPlaylistMode? Of(PageKey key) => key switch
    {
        NewPlaylistPage => new Items(Array.Empty<long>()),
        SaveSmartPlaylistPage s => new Smart(s.LibraryId, s.Filter),
        _ => null,
    };
}

/// <summary>
/// New playlist (the web's PlaylistFormModal) and Save these filters (its SaveFiltersModal, as a
/// smart playlist: apps don't make collections), the Android app's NewPlaylistViewModel: a title
/// of at most 200 characters, and Movies &amp; TV or Music for an empty one.
/// <see cref="CreateEvent.Created"/> opens the playlist made, in this page's place.
/// </summary>
public sealed class NewPlaylistModel
{
    readonly IPlaylistSource source;
    readonly IScheduler scheduler;

    public NewPlaylistModel(IPlaylistSource source, NewPlaylistMode mode, IScheduler scheduler)
    {
        this.source = source;
        this.scheduler = scheduler;
        Mode = mode;
        AsksKind = mode is NewPlaylistMode.Items { ItemIds.Count: 0 };
        Filters = mode is NewPlaylistMode.Smart smart ? SmartFilter.Describe(smart.Filter) : Array.Empty<string>();
    }

    public NewPlaylistMode Mode { get; }

    /// <summary>Saving a library's filters, not a playlist of items.</summary>
    public bool IsSmart => Mode is NewPlaylistMode.Smart;

    /// <summary>The panel asks what the playlist plays: only an empty, ordinary one.</summary>
    public bool AsksKind { get; }

    /// <summary>A smart playlist's filters in words: "Genre: Comedy", "Unwatched".</summary>
    public IReadOnlyList<string> Filters { get; }

    /// <summary>It is on its way to the server.</summary>
    public Signal<bool> Busy { get; } = new Signal<bool>(false);

    /// <summary>Made (open it in this page's place), or a failure for a toast.</summary>
    public event Action<CreateEvent>? Told;

    /// <summary>Makes it; false (and nothing sent) when <paramref name="title"/> won't do or it is already being made.</summary>
    /// <param name="kind">What an empty playlist plays (<see cref="PlaylistKind"/>).</param>
    public bool Create(string title, string kind = PlaylistKind.Video)
    {
        if (PlaylistRules.Check(title) != null || Busy.Value) return false;
        Busy.Value = true;
        var name = title.Trim();
        Plays.When(scheduler, () => Mode switch
        {
            NewPlaylistMode.Smart s => source.CreateSmart(name, s.LibraryId, s.Filter),
            NewPlaylistMode.Items m => source.Create(name, m.ItemIds.Count == 0 ? kind : null, m.ItemIds),
            _ => throw new InvalidOperationException(),
        }, id =>
        {
            Busy.Value = false;
            Told?.Invoke(new CreateEvent.Created(id, name));
        }, e =>
        {
            Busy.Value = false;
            Told?.Invoke(new CreateEvent.Failed(e));
        });
        return true;
    }
}
