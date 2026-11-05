using System;
using System.Collections.Generic;
using System.Globalization;
using System.Linq;
using System.Threading.Tasks;
using Pley.Tv.Core.Api;
using Pley.Tv.Core.Auth;

namespace Pley.Tv.Core.Ui;

/// <summary>Something an item page or sheet tells the person once, as a toast.</summary>
public abstract record ItemEvent
{
    /// <summary>"Nothing to play: this item has no media files."</summary>
    public sealed record NothingToPlay : ItemEvent;

    /// <summary>An action that failed; what it changed on screen is undone by then.</summary>
    public sealed record Failed(Exception Error) : ItemEvent;
}

/// <summary>
/// What the player is asked to play: <paramref name="ItemId"/> from <paramref name="StartMs"/>
/// (0: the start). The version and tracks are the page's own choices, for the page's own item:
/// the audio only when picked there, else the server picks the track the device plays best. A
/// cast's tracks are those picked on the remote's page (<paramref name="SubtitleIndex"/> -1: none),
/// for that play only.
/// </summary>
/// <param name="FileId">The version, when the item has several.</param>
/// <param name="Queue">A playlist or collection to keep playing from, in its order, when this item ends.</param>
public sealed record PlayRequest(long ItemId, long StartMs, bool FromStart, long? FileId = null, int? AudioIndex = null, int? SubtitleIndex = null,
    PlayQueue? Queue = null);

/// <summary>Where the next item comes from when one ends (the Android app's PlayQueue).</summary>
public abstract record PlayQueue
{
    public sealed record Playlist(long PlaylistId) : PlayQueue;

    public sealed record Collection(long CollectionId) : PlayQueue;
}

/// <summary>What item pages read and change: the app's state, or a fake in tests.</summary>
public interface IItemSource
{
    /// <summary>The server's origin, which pictures are made whole against.</summary>
    string Origin { get; }

    /// <summary>The detail of <paramref name="id"/> the app saw lately, to show at once; null when none is kept.</summary>
    ItemDetail? Saved(long id);

    /// <summary>Whether the saved detail should be asked for again.</summary>
    bool Stale(long id);

    /// <summary>The item from the server, kept for the next page.</summary>
    Task<ItemDetail> Item(long id);

    /// <summary>A show's seasons, a season's episodes.</summary>
    Task<IReadOnlyList<ItemSummary>> Children(long id);

    /// <summary>The episode a show's or season's Play starts; null with none.</summary>
    Task<ItemSummary?> OnDeck(long id);

    /// <summary>More Like This.</summary>
    Task<IReadOnlyList<ItemSummary>> Related(long id);

    /// <summary>The collections a movie or show is part of.</summary>
    Task<IReadOnlyList<CollectionRef>> CollectionsOf(long id);

    Task SetWatched(long id, bool watched);

    /// <summary>The profile's rating, 0 to 10 in half stars; null clears it.</summary>
    Task Rate(long id, double? rating);

    Task SetMyList(long id, bool inList);

    Task HideFromContinueWatching(long id);

    /// <summary>
    /// Keeps the tracks picked for <paramref name="itemId"/> (the whole show for an episode).
    /// <paramref name="audio"/> null keeps the audio chosen before, unless <paramref name="automaticAudio"/>,
    /// which forgets it.
    /// </summary>
    Task ChooseStreams(long itemId, long? fileId, int? audio, int? subtitle, bool automaticAudio);

    /// <summary>Tracks were chosen for <paramref name="detail"/>'s title: other saved details name the tracks of before.</summary>
    void TracksChanged(ItemDetail? detail);

    /// <summary>Calls <paramref name="changed"/> when what pages show may have changed (the server's events, a playback that ended).</summary>
    IDisposable OnChange(Action changed);
}

/// <summary>Item pages' source in the app: <see cref="AppState"/> and its API.</summary>
public sealed class AppItemSource : IItemSource
{
    readonly AppState app;

    public AppItemSource(AppState app) => this.app = app;

    Api.Api Api => app.Api.Value ?? throw new SignedOutException();

    public string Origin => app.Server.Value ?? "";

    public ItemDetail? Saved(long id) => app.CachedItem(id);

    public bool Stale(long id) => app.CachedItemStale(id);

    public Task<ItemDetail> Item(long id) => app.Api.Value == null ? Task.FromException<ItemDetail>(new SignedOutException()) : app.Item(id);

    public async Task<IReadOnlyList<ItemSummary>> Children(long id) => (await Api.Children(id)).Items;

    public async Task<ItemSummary?> OnDeck(long id) => (await Api.OnDeck(id)).Item;

    public async Task<IReadOnlyList<ItemSummary>> Related(long id) => (await Api.Related(id)).Items;

    public async Task<IReadOnlyList<CollectionRef>> CollectionsOf(long id) => (await Api.ItemCollections(id)).Collections;

    public Task SetWatched(long id, bool watched) => watched ? Api.MarkWatched(id) : Api.MarkUnwatched(id);

    public Task Rate(long id, double? rating) => Api.Rate(id, rating);

    public Task SetMyList(long id, bool inList) => inList ? Api.AddToMyList(id) : Api.RemoveFromMyList(id);

    public Task HideFromContinueWatching(long id) => Api.HideFromContinueWatching(id);

    public Task ChooseStreams(long itemId, long? fileId, int? audio, int? subtitle, bool automaticAudio) =>
        SendStreams(Api, itemId, fileId, audio, subtitle, automaticAudio);

    /// <summary>The choice as the server takes it: the audio left out keeps the one chosen before; its null (Automatic) forgets it.</summary>
    internal static Task SendStreams(Api.Api api, long itemId, long? fileId, int? audio, int? subtitle, bool automaticAudio) =>
        api.SetStreams(itemId, fileId, automaticAudio ? null : audio, subtitle, keepAudio: audio == null && !automaticAudio);

    public void TracksChanged(ItemDetail? detail) => app.TracksChanged(detail);

    public IDisposable OnChange(Action changed)
    {
        // Watch calls at once with the count as it is: only the changes after count.
        var first = true;
        return app.Changes.Watch(_ =>
        {
            if (first) first = false;
            else changed();
        });
    }
}

/// <summary>Pressing play, as the web does it (apps/web/src/lib/usePlayItem.ts; the Android app's PlayItem).</summary>
public static class Plays
{
    /// <summary>
    /// What playing <paramref name="item"/> plays: a movie, episode or extra from where it was
    /// left unless watched or <paramref name="fromStart"/>; a show or season its next episode.
    /// Null: nothing to play.
    /// </summary>
    public static Task<PlayRequest?> Resolve(IItemSource source, ItemSummary item, bool fromStart)
    {
        switch (item.Kind)
        {
            case "movie":
            case "episode":
            case "extra":
                return Task.FromResult<PlayRequest?>(new PlayRequest(item.Id, fromStart || item.UserState.Watched ? 0 : item.UserState.PositionMs, fromStart));
            case "show":
            case "season":
                return Then(source.OnDeck(item.Id), next => next == null ? null : new PlayRequest(next.Id, fromStart ? 0 : next.UserState.PositionMs, fromStart));
            default:
                return Task.FromResult<PlayRequest?>(null);
        }
    }

    /// <summary>
    /// <paramref name="task"/>'s value through <paramref name="map"/>, worked out where the task
    /// ends: an await would leave a UI thread's context for the thread pool first.
    /// </summary>
    static Task<TOut> Then<TIn, TOut>(Task<TIn> task, Func<TIn, TOut> map) =>
        task.ContinueWith(t => map(t.GetAwaiter().GetResult()), System.Threading.CancellationToken.None, TaskContinuationOptions.ExecuteSynchronously, TaskScheduler.Default);

    /// <summary>
    /// Runs <paramref name="task"/> and hands its outcome to <paramref name="done"/> or
    /// <paramref name="failed"/> on the UI thread, through <paramref name="scheduler"/>.
    /// </summary>
    internal static void When<T>(IScheduler scheduler, Func<Task<T>> task, Action<T> done, Action<Exception> failed)
    {
        Task<T> running;
        try
        {
            running = task();
        }
        catch (Exception e)
        {
            running = Task.FromException<T>(e);
        }
        running.ContinueWith(t => scheduler.Post(() =>
        {
            if (t.IsFaulted) failed(t.Exception!.InnerException ?? t.Exception);
            else if (t.IsCanceled) failed(new TaskCanceledException(t));
            else done(t.Result);
        }), System.Threading.CancellationToken.None, TaskContinuationOptions.ExecuteSynchronously, TaskScheduler.Default);
    }

    internal static void When(IScheduler scheduler, Func<Task> task, Action done, Action<Exception> failed) =>
        When(scheduler, () => task().ContinueWith(t =>
        {
            t.GetAwaiter().GetResult();
            return true;
        }, System.Threading.CancellationToken.None, TaskContinuationOptions.ExecuteSynchronously, TaskScheduler.Default), _ => done(), failed);
}

/// <summary>
/// An item page (movie, show, season, episode), as the Android app's ItemViewModel: the item from
/// what was saved first, the server's answer after; its seasons and episodes, the next episode,
/// More Like This and its collections beside it; and what can be done there.
///
/// Play asks the screen to play (<see cref="PlayRequested"/>) with the version and tracks chosen
/// here. Choosing an audio or subtitle track is kept for the item (PUT /api/items/{id}/streams),
/// as on the web. Watched marks, ratings and My List show at once and are undone if the server
/// refuses. The page asks again when the app hears that things changed.
/// </summary>
public sealed class ItemModel : IDisposable
{
    readonly IItemSource source;
    readonly IScheduler scheduler;
    readonly CultureInfo? culture;
    readonly IDisposable changes;
    /// <summary>The seasons whose episodes were asked for since the page last asked again.</summary>
    readonly HashSet<long> asked = new HashSet<long>();
    Held held = new Held();
    StreamPick pick = new StreamPick();
    long? chosenSeason;
    bool besideAsked;
    /// <summary>Bumped by Retry and Dispose: answers to what was asked before are dropped.</summary>
    int generation;
    bool computing, again, reloadPosted, gone;

    public ItemModel(IItemSource source, long itemId, IScheduler scheduler, CultureInfo? culture = null)
    {
        this.source = source;
        this.scheduler = scheduler;
        this.culture = culture;
        ItemId = itemId;
        changes = source.OnChange(() =>
        {
            // Changes come in bursts: the page asks again once for them.
            if (reloadPosted) return;
            reloadPosted = true;
            scheduler.Post(() =>
            {
                reloadPosted = false;
                Reload();
            });
        });
        Load();
    }

    public long ItemId { get; }

    public Signal<ItemState> State { get; } = new Signal<ItemState>(new ItemState.Loading());

    /// <summary>What the page says once: shown as a toast.</summary>
    public event Action<ItemEvent>? Told;

    /// <summary>Something to play: the player takes it from here.</summary>
    public event Action<PlayRequest>? PlayRequested;

    /// <summary>A tile's action that goes to another page (its show, its season, a playlist, its music page).</summary>
    public event Action<PageKey>? OpenRequested;

    /// <summary>The page as it shows now; null unless ready.</summary>
    public ItemPage? Page => (State.Value as ItemState.Ready)?.Page;

    /// <summary>Loads the page again after it failed.</summary>
    public void Retry()
    {
        generation++;
        held = new Held();
        besideAsked = false;
        asked.Clear();
        State.Value = new ItemState.Loading();
        Load();
    }

    // ---- playing

    /// <summary>
    /// Plays the item: a movie or episode from where it was left (or from the start), with the
    /// version and tracks chosen here; a show or season its next episode.
    /// </summary>
    public void Play(bool fromStart = false)
    {
        var d = held.Detail;
        if (d == null) return;
        Run(() => Plays.Resolve(source, d, fromStart), target =>
        {
            if (target == null) Tell(new ItemEvent.NothingToPlay());
            else PlayRequested?.Invoke(RequestFor(d, target));
        }, stillShown: false);
    }

    /// <summary>Plays <paramref name="item"/> (an episode of the row, an extra, a tile's), from where it was left unless <paramref name="fromStart"/>.</summary>
    public void Play(ItemSummary item, bool fromStart = false)
    {
        if (item.Id == held.Detail?.Id)
        {
            Play(fromStart);
            return;
        }
        Run(() => Plays.Resolve(source, item, fromStart), target =>
        {
            if (target == null) Tell(new ItemEvent.NothingToPlay());
            else PlayRequested?.Invoke(target);
        }, stillShown: false);
    }

    /// <summary>
    /// The request for <paramref name="target"/>: this item's own choices when it is the item
    /// played. The audio only when picked here: otherwise the server picks the track this device
    /// plays best.
    /// </summary>
    PlayRequest RequestFor(ItemDetail d, PlayRequest target)
    {
        if (target.ItemId != d.Id) return target;
        var playing = ItemPages.PlayingOf(d, pick);
        return target with
        {
            FileId = d.Files.Count > 1 ? playing.File?.Id : null,
            AudioIndex = playing.PickedAudio,
            SubtitleIndex = playing.SubtitleIndex,
        };
    }

    // ---- what can be done on the page

    /// <summary>Marks the item watched, or unwatched when it is.</summary>
    public void ToggleWatched()
    {
        var page = Page;
        if (page != null) SetWatched(page.Id, !page.Watched);
    }

    /// <summary>Marks <paramref name="id"/> (this item, or one of its episodes) watched or not.</summary>
    public void SetWatched(long id, bool watched) =>
        Change(ItemPatch.Watched(id, watched, Find(id)), () => source.SetWatched(id, watched));

    /// <summary>Adds the item to My List, or takes it off when it is in it.</summary>
    public void ToggleMyList()
    {
        var page = Page;
        if (page?.MyList is not { } inList) return;
        Change(ItemPatch.InWatchlist(page.Id, !inList), () => source.SetMyList(page.Id, !inList));
    }

    /// <summary>The profile's rating, 1 to 10 (a half star each), or none.</summary>
    public void Rate(int? rating)
    {
        double? kept = rating > 0 ? Math.Min(10, rating.Value) : null;
        Change(ItemPatch.Rating(ItemId, kept), () => source.Rate(ItemId, kept));
    }

    /// <summary>Plays another version: its tracks start as the server would pick them. Not kept, as on the web.</summary>
    public void ChooseVersion(long fileId)
    {
        pick = pick with { FileId = fileId };
        Compute();
    }

    /// <summary>
    /// Plays audio track <paramref name="index"/>, and keeps it for the item (and the rest of a
    /// show). Null is Automatic: the pick is forgotten, and each device plays its best track of
    /// the language.
    /// </summary>
    public void ChooseAudio(int? index)
    {
        var d = held.Detail;
        var file = d == null ? null : ItemPages.PlayingOf(d, pick).File;
        if (d == null || file == null) return;
        pick = pick with { AudioFile = file.Id, Audio = index };
        Remember(d, file.Id, index, ItemPages.PlayingOf(d, pick).SubtitleIndex, automaticAudio: index == null);
    }

    /// <summary>
    /// Shows subtitle track <paramref name="index"/> (null: off), and keeps it for the item, with
    /// the audio picked here, if any: else the audio chosen before stays.
    /// </summary>
    public void ChooseSubtitle(int? index)
    {
        var d = held.Detail;
        if (d == null) return;
        var playing = ItemPages.PlayingOf(d, pick);
        var file = playing.File;
        if (file == null) return;
        pick = pick with { SubtitleFile = file.Id, Subtitle = index };
        Remember(d, file.Id, playing.PickedAudio, index, automaticAudio: false);
    }

    void Remember(ItemDetail d, long fileId, int? audio, int? subtitle, bool automaticAudio) =>
        Change(ItemPatch.Streams(d.Id, fileId, audio, subtitle, automaticAudio), () => source.ChooseStreams(d.Id, fileId, audio, subtitle, automaticAudio), () =>
        {
            source.TracksChanged(automaticAudio ? null : held.Detail);
            // Without a pick, the details' track is the server's to work out: the page asks again.
            if (automaticAudio) FetchDetail();
        });

    /// <summary>Shows <paramref name="seasonId"/>'s episodes.</summary>
    public void SelectSeason(long seasonId)
    {
        chosenSeason = seasonId;
        Compute();
    }

    /// <summary>
    /// <paramref name="action"/> on a tile's <paramref name="item"/> (its actions, from a long
    /// press): plays and marks here, the rest open their page (<see cref="OpenRequested"/>).
    /// </summary>
    public void Perform(ItemAction action, ItemSummary item)
    {
        switch (action)
        {
            case ItemAction.Play:
            case ItemAction.Resume:
                Play(item);
                break;
            case ItemAction.FromStart:
                Play(item, fromStart: true);
                break;
            case ItemAction.MarkWatched:
            case ItemAction.MarkUnwatched:
                SetWatched(item.Id, action == ItemAction.MarkWatched);
                break;
            default:
                if (ItemActions.Destination(action, item) is { } key) OpenRequested?.Invoke(key);
                break;
        }
    }

    public void Dispose()
    {
        gone = true;
        generation++;
        changes.Dispose();
    }

    // ---- loading

    void Load()
    {
        var saved = source.Saved(ItemId);
        if (saved != null) Update(h => h with { Detail = saved });
        if (saved == null || source.Stale(ItemId)) FetchDetail();
        else Compute();
    }

    /// <summary>The app heard of changes: everything is asked for again, while the page shows what it has.</summary>
    void Reload()
    {
        if (gone) return;
        besideAsked = false;
        asked.Clear();
        FetchDetail();
        if (held.Detail != null) Compute();
    }

    void FetchDetail() => Run(() => source.Item(ItemId), d => Update(h => h with { Detail = d, Error = null }), e => Update(h => h with { Error = e }));

    /// <summary>What is loaded beside the item, once its kind is known.</summary>
    void AskBeside(ItemDetail d)
    {
        besideAsked = true;
        var id = d.Id;
        if (d.Kind is "movie" or "show")
        {
            Run(() => source.Related(id), list => Update(h => h with { Related = list }), _ => { });
            Run(() => source.CollectionsOf(id), list => Update(h => h with { PartOf = list }), _ => { });
        }
        if (d.Kind is "show" or "season")
            // Failed, the page goes on without it: Play asks again when pressed.
            Run(() => source.OnDeck(id), next => Update(h => h with { OnDeck = next, OnDeckKnown = true }), _ => Update(h => h with { OnDeckKnown = true }));
        // The tabs: a show's seasons, or a season's and its siblings.
        var show = d.Kind == "show" ? id : d.Kind == "season" ? d.ParentId : null;
        if (show != null) Run(() => source.Children(show.Value), list => Update(h => h with { Seasons = list }), _ => { });
    }

    /// <summary>The season shown: the one chosen, else a show's next episode's (once known, so the page doesn't jump), a season's own, an episode's.</summary>
    long? Selected(ItemDetail d) => d.Kind switch
    {
        "show" => chosenSeason ?? (held.Seasons == null || !held.OnDeckKnown ? null : ItemPages.DefaultSeason(held.Seasons, held.OnDeck)),
        "season" => chosenSeason ?? d.Id,
        "episode" => d.ParentId,
        _ => null,
    };

    void AskEpisodes(long season)
    {
        if (!asked.Add(season)) return;
        Run(() => source.Children(season), list => Update(h =>
        {
            var episodes = new Dictionary<long, IReadOnlyList<ItemSummary>>(h.Episodes.Count + 1);
            foreach (var kv in h.Episodes) episodes[kv.Key] = kv.Value;
            episodes[season] = list;
            return h with { Episodes = episodes };
        }), _ => { });
    }

    void Update(Func<Held, Held> change)
    {
        held = change(held);
        Compute();
    }

    /// <summary>Works the state out again; asked while it already is (an answer that came at once), it goes round once more.</summary>
    void Compute()
    {
        if (gone) return;
        if (computing)
        {
            again = true;
            return;
        }
        computing = true;
        try
        {
            do
            {
                again = false;
                State.Value = StateNow();
            }
            while (again);
        }
        finally
        {
            computing = false;
        }
    }

    ItemState StateNow()
    {
        var d = held.Detail;
        if (d == null)
            return held.Error switch
            {
                null => new ItemState.Loading(),
                ApiException { Status: 404 } => new ItemState.NotFound(),
                var e => new ItemState.Failed(e),
            };
        if (ElsewhereOf(d) is { } elsewhere) return elsewhere;
        if (!besideAsked) AskBeside(d);
        var selected = Selected(d);
        if (selected != null) AskEpisodes(selected.Value);
        var page = ItemPages.Page(new PageInput(d, source.Origin)
        {
            Pick = pick,
            OnDeck = held.OnDeck,
            Seasons = held.Seasons,
            SelectedSeason = selected,
            Episodes = selected != null && held.Episodes.TryGetValue(selected.Value, out var episodes) ? episodes : null,
            Related = held.Related,
            PartOf = held.PartOf,
            Culture = culture,
        });
        return new ItemState.Ready(page, held.Error);
    }

    /// <summary>Music has its own pages, an extra its movie's or show's; kinds this app doesn't know aren't found.</summary>
    static ItemState? ElsewhereOf(ItemDetail d) => d.Kind switch
    {
        "artist" => new ItemState.Elsewhere(new ArtistPage(d.Id)),
        "album" => new ItemState.Elsewhere(new AlbumPage(d.Id)),
        "track" => d.ParentId is { } album ? new ItemState.Elsewhere(new AlbumPage(album)) : new ItemState.NotFound(),
        "extra" => d.ParentId is { } parent ? new ItemState.Elsewhere(new PageKey.Item(parent)) : new ItemState.NotFound(),
        _ => ItemPages.HasPage(d.Kind) ? null : new ItemState.NotFound(),
    };

    // ---- changes

    /// <summary>
    /// Shows <paramref name="patch"/> at once and sends <paramref name="call"/>; refused, the page
    /// shows what it was and says why.
    /// </summary>
    void Change(ItemPatch patch, Func<Task> call, Action? done = null)
    {
        var before = held;
        var after = patch.Apply(held);
        held = after;
        Compute();
        var at = generation;
        Plays.When(scheduler, call, () =>
        {
            if (at == generation) done?.Invoke();
        }, e =>
        {
            if (at != generation) return;
            // Something else came in meanwhile: the server says how things are.
            if (ReferenceEquals(held, after)) Update(_ => before);
            else Reload();
            Tell(new ItemEvent.Failed(e));
        });
    }

    /// <summary>The item <paramref name="id"/> as the page holds it, for what marking it changes around it.</summary>
    ItemSummary? Find(long id)
    {
        if (held.Detail?.Id == id) return held.Detail;
        if (held.OnDeck?.Id == id) return held.OnDeck;
        return held.Episodes.Values.SelectMany(l => l).Concat(held.Seasons ?? Array.Empty<ItemSummary>()).Concat(held.Related)
            .FirstOrDefault(i => i.Id == id);
    }

    /// <summary>
    /// Runs <paramref name="task"/>; its answer is used while the page is shown and asks for the
    /// same thing. With <paramref name="stillShown"/> false (a play), it is used even after.
    /// </summary>
    void Run<T>(Func<Task<T>> task, Action<T> done, Action<Exception>? failed = null, bool stillShown = true)
    {
        var at = generation;
        Plays.When(scheduler, task, value =>
        {
            if (!stillShown || at == generation) done(value);
        }, e =>
        {
            if (stillShown && at != generation) return;
            if (failed != null) failed(e);
            else Tell(new ItemEvent.Failed(e));
        });
    }

    void Tell(ItemEvent e) => Told?.Invoke(e);

    /// <summary>What the page holds, as one value: a change is shown, and undone, as a whole.</summary>
    internal sealed record Held
    {
        public ItemDetail? Detail { get; init; }
        /// <summary>The detail's last fetch failed: a page shows what was saved.</summary>
        public Exception? Error { get; init; }
        public ItemSummary? OnDeck { get; init; }
        /// <summary>The next episode was answered (or couldn't be): a show picks its season then.</summary>
        public bool OnDeckKnown { get; init; }
        public IReadOnlyList<ItemSummary>? Seasons { get; init; }
        public IReadOnlyDictionary<long, IReadOnlyList<ItemSummary>> Episodes { get; init; } = new Dictionary<long, IReadOnlyList<ItemSummary>>();
        public IReadOnlyList<ItemSummary> Related { get; init; } = Array.Empty<ItemSummary>();
        public IReadOnlyList<CollectionRef> PartOf { get; init; } = Array.Empty<CollectionRef>();
    }
}

/// <summary>
/// A change shown before the server has it (the Android app's ItemPatch): every item the page
/// holds gets it at once, and an item it doesn't touch stays the same instance.
/// </summary>
internal sealed class ItemPatch
{
    readonly Func<ItemSummary, ItemSummary> summary;

    ItemPatch(Func<ItemSummary, ItemSummary> summary) => this.summary = summary;

    public ItemSummary Apply(ItemSummary item) => summary(item);

    public ItemDetail Apply(ItemDetail detail) => (ItemDetail)summary(detail);

    internal ItemModel.Held Apply(ItemModel.Held held)
    {
        IReadOnlyList<ItemSummary> All(IReadOnlyList<ItemSummary> items)
        {
            var changed = false;
            var out_ = items.Select(i =>
            {
                var next = summary(i);
                if (!ReferenceEquals(next, i)) changed = true;
                return next;
            }).ToList();
            return changed ? out_ : items;
        }
        return held with
        {
            // A record's copy keeps its type: a detail stays one.
            Detail = held.Detail == null ? null : Apply(held.Detail),
            OnDeck = held.OnDeck == null ? null : summary(held.OnDeck),
            Seasons = held.Seasons == null ? null : All(held.Seasons),
            Episodes = held.Episodes.ToDictionary(kv => kv.Key, kv => All(kv.Value)),
            Related = All(held.Related),
        };
    }

    /// <summary>
    /// <paramref name="itemId"/> marked watched by hand: it, and everything under it, watched or
    /// not, with no point to resume; its season and show count one more or one fewer watched
    /// episode (<paramref name="target"/> is the item as it was, when known).
    /// </summary>
    public static ItemPatch Watched(long itemId, bool watched, ItemSummary? target)
    {
        var delta = target == null || target.Kind != "episode" || target.UserState.Watched == watched ? 0 : watched ? 1 : -1;
        var containers = delta == 0 ? new HashSet<long>() : new HashSet<long>(new[] { target?.ParentId, target?.GrandparentId }.Where(i => i != null).Select(i => i!.Value));
        return new ItemPatch(s =>
        {
            if (s.Id == itemId || s.ParentId == itemId || s.GrandparentId == itemId)
                return s with
                {
                    UserState = s.UserState with { Watched = watched, PositionMs = 0 },
                    ViewedLeafCount = s.LeafCount is { } leaves ? watched ? leaves : 0 : s.ViewedLeafCount,
                };
            if (containers.Contains(s.Id) && s.LeafCount is { } all)
            {
                var viewed = Math.Max(0, Math.Min(all, (s.ViewedLeafCount ?? 0) + delta));
                return s with { ViewedLeafCount = viewed, UserState = s.UserState with { Watched = viewed == all } };
            }
            return s;
        });
    }

    /// <summary><paramref name="itemId"/>'s page says whether it is in My List.</summary>
    public static ItemPatch InWatchlist(long itemId, bool inList) =>
        new ItemPatch(s => s is ItemDetail d && d.Id == itemId && d.InWatchlist != inList ? d with { InWatchlist = inList } : s);

    /// <summary>The profile's own <paramref name="rating"/> of <paramref name="itemId"/> (null: none).</summary>
    public static ItemPatch Rating(long itemId, double? rating) =>
        new ItemPatch(s => s.Id == itemId ? s with { UserState = s.UserState with { Rating = rating } } : s);

    /// <summary>
    /// The version and tracks <paramref name="itemId"/> starts with next time: on
    /// <paramref name="fileId"/>, or its first file. <paramref name="audio"/> null keeps the audio
    /// it had, and <paramref name="automaticAudio"/> says no track is picked any more.
    /// </summary>
    public static ItemPatch Streams(long itemId, long? fileId, int? audio, int? subtitle, bool automaticAudio) =>
        new ItemPatch(s =>
        {
            if (s is not ItemDetail d || d.Id != itemId || d.Files.Count == 0) return s;
            var file = fileId ?? d.Files[0].Id;
            return d with
            {
                Files = d.Files.Select(f => f.Id != file ? f : f with
                {
                    SelectedAudio = !automaticAudio && audio != null ? audio : f.SelectedAudio,
                    AudioPicked = automaticAudio ? false : audio != null ? true : f.AudioPicked,
                    SelectedSubtitle = subtitle,
                }).ToList(),
            };
        });
}
