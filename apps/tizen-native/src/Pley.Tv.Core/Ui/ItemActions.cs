using System;
using System.Collections.Generic;
using System.Linq;
using Pley.Tv.Core.Api;

namespace Pley.Tv.Core.Ui;

/// <summary>
/// What can be done with an item from its actions (ItemActions.tsx without the administrator's
/// part): playing, watched marks, leaving Continue Watching, My List, playlists, and going to its
/// show or season. Editing, refreshing and analysing are the web's, for administrators: never
/// offered here, whatever the server says of the profile.
/// </summary>
public enum ItemAction
{
    Play,
    Resume,
    FromStart,
    MarkWatched,
    MarkUnwatched,
    /// <summary>Opened from a Continue Watching tile: it (an episode: its show) leaves the row.</summary>
    RemoveFromContinueWatching,
    AddToMyList,
    RemoveFromMyList,
    AddToPlaylist,
    GoToShow,
    GoToSeason,
    /// <summary>Artists, albums and tracks: their music page, which plays them.</summary>
    Open,
}

/// <summary>An item's actions (the Android app's ItemActions.kt), with the TV's words and icons.</summary>
public static class ItemActions
{
    /// <summary>
    /// The actions for <paramref name="item"/>, in the order they are listed. My List is offered
    /// for movies and shows when it is known whether the item is in it (<paramref name="inWatchlist"/>);
    /// leaving Continue Watching when the actions were opened from that row (<paramref name="continueWatching"/>).
    /// </summary>
    public static IReadOnlyList<ItemAction> Of(ItemSummary item, bool? inWatchlist = null, bool continueWatching = false)
    {
        var kind = item.Kind;
        var known = ItemPages.HasPage(kind) || kind is "artist" or "album" or "track" or "extra";
        var actions = new List<ItemAction>();
        if (kind is "artist" or "album" or "track") actions.Add(ItemAction.Open);
        else if (known)
        {
            if (Format.IsInProgress(item))
            {
                actions.Add(ItemAction.Resume);
                actions.Add(ItemAction.FromStart);
            }
            else actions.Add(ItemAction.Play);
        }
        if (kind is "movie" or "episode" or "show" or "season") actions.Add(Watched(item) ? ItemAction.MarkUnwatched : ItemAction.MarkWatched);
        if (continueWatching) actions.Add(ItemAction.RemoveFromContinueWatching);
        if (inWatchlist != null && ItemPages.Listable(kind)) actions.Add(inWatchlist.Value ? ItemAction.RemoveFromMyList : ItemAction.AddToMyList);
        if (known && kind != "extra") actions.Add(ItemAction.AddToPlaylist);
        if (kind == "episode" && item.GrandparentId != null) actions.Add(ItemAction.GoToShow);
        if (kind == "episode" && item.ParentId != null) actions.Add(ItemAction.GoToSeason);
        if (kind == "season" && item.ParentId != null) actions.Add(ItemAction.GoToShow);
        return actions;
    }

    /// <summary>Watched: a video marked so, or every episode of a show or season.</summary>
    public static bool Watched(ItemSummary item)
    {
        if (item.Kind is "show" or "season")
        {
            var leaves = item.LeafCount ?? 0;
            return leaves > 0 && (item.ViewedLeafCount ?? 0) >= leaves;
        }
        return item.UserState.Watched;
    }

    /// <summary>Where <paramref name="action"/> on <paramref name="item"/> goes, for the actions that open a page; null for the others.</summary>
    public static PageKey? Destination(ItemAction action, ItemSummary item) => action switch
    {
        ItemAction.AddToPlaylist => new AddToPlaylistPage(item.Id),
        ItemAction.GoToShow => (item.Kind == "season" ? item.ParentId : item.GrandparentId) is { } show ? new PageKey.Item(show) : null,
        ItemAction.GoToSeason => item.ParentId is { } season ? new PageKey.Item(season) : null,
        ItemAction.Open => item.Kind switch
        {
            "artist" => new ArtistPage(item.Id),
            "album" => new AlbumPage(item.Id),
            _ => item.ParentId is { } album ? new AlbumPage(album) : null,
        },
        _ => null,
    };

    /// <summary>What the action says (the Android app's tv_item_* strings).</summary>
    public static string Label(ItemAction action) => action switch
    {
        ItemAction.Play => ItemText.Play,
        ItemAction.Resume => ItemText.Resume,
        ItemAction.FromStart => Strings.T("item.actions.playFromStart"),
        ItemAction.MarkWatched => ItemText.MarkWatched,
        ItemAction.MarkUnwatched => ItemText.MarkUnwatched,
        ItemAction.RemoveFromContinueWatching => Strings.T("item.actions.removeFromContinueWatching"),
        ItemAction.AddToMyList => ItemText.AddToMyList,
        ItemAction.RemoveFromMyList => Strings.T("item.actions.removeFromMyList"),
        ItemAction.AddToPlaylist => ItemText.AddToPlaylist,
        ItemAction.GoToShow => ItemText.GoToShow,
        ItemAction.GoToSeason => ItemText.GoToSeason,
        _ => Strings.T("item.actions.open"),
    };

    /// <summary>The action's icon in a side panel, by Lucide's name.</summary>
    public static string Icon(ItemAction action) => action switch
    {
        ItemAction.Play or ItemAction.Resume => "play",
        ItemAction.FromStart => "rotate-ccw",
        ItemAction.MarkWatched => "circle-check",
        ItemAction.MarkUnwatched => "eye-off",
        ItemAction.RemoveFromContinueWatching => "x",
        ItemAction.AddToMyList => "bookmark-plus",
        ItemAction.RemoveFromMyList => "bookmark-minus",
        ItemAction.AddToPlaylist => "list-plus",
        ItemAction.GoToShow => "tv",
        ItemAction.GoToSeason => "layers",
        _ => "arrow-right",
    };
}

/// <summary>An item's actions as a screen of their own: its name and its actions.</summary>
public abstract record ItemActionsState
{
    public sealed record Loading : ItemActionsState;

    /// <summary>It couldn't load and nothing was saved: "Couldn't load this item", and Back.</summary>
    public sealed record Failed(Exception Error) : ItemActionsState;

    /// <param name="Origin">The server's origin, for the item's pictures.</param>
    /// <param name="Title">"The Office", or an episode's show.</param>
    /// <param name="Subtitle">"S2 · E5 · Office Olympics", "2019", "3 seasons".</param>
    public sealed record Ready(ItemSummary Item, string Origin, string Title, string Subtitle, IReadOnlyList<ItemAction> Actions) : ItemActionsState;
}

/// <summary>
/// The actions of an item as a screen (the Android app's ItemActionsViewModel), opened by a long
/// press on a tile elsewhere. Play, the watched marks, Continue Watching and My List are done
/// here; the rest open a page (<see cref="ItemActions.Destination"/>). Opened from a Continue
/// Watching tile, it offers to take the item out of that row.
///
/// The screen closes as soon as an action is chosen, while the action goes on: a play asked for
/// after it closed still comes (<see cref="PlayRequested"/>), so whatever plays should hear it
/// beyond the screen.
/// </summary>
public sealed class ItemActionsModel : IDisposable
{
    readonly IItemSource source;
    readonly IScheduler scheduler;
    readonly bool continueWatching;
    readonly IDisposable changes;
    ItemDetail? detail;
    Exception? error;
    bool gone;

    public ItemActionsModel(IItemSource source, long itemId, IScheduler scheduler, bool continueWatching = false)
    {
        this.source = source;
        this.scheduler = scheduler;
        this.continueWatching = continueWatching;
        ItemId = itemId;
        changes = source.OnChange(Fetch);
        detail = source.Saved(itemId);
        if (detail == null || source.Stale(itemId)) Fetch();
        else Compute();
    }

    public long ItemId { get; }

    public Signal<ItemActionsState> State { get; } = new Signal<ItemActionsState>(new ItemActionsState.Loading());

    /// <summary>Failures and "nothing to play", told once.</summary>
    public event Action<ItemEvent>? Told;

    /// <summary>Something to play, even after the screen closed.</summary>
    public event Action<PlayRequest>? PlayRequested;

    /// <summary>
    /// Does <paramref name="action"/> if it is done here (play, from start, the watched marks,
    /// Continue Watching, My List): true once it is under way, so the screen can close; false for
    /// the actions that open a page.
    /// </summary>
    public bool Perform(ItemAction action)
    {
        if (State.Value is not ItemActionsState.Ready ready) return false;
        var item = ready.Item;
        switch (action)
        {
            case ItemAction.Play:
            case ItemAction.Resume:
            case ItemAction.FromStart:
                Plays.When(scheduler, () => Plays.Resolve(source, item, action == ItemAction.FromStart), target =>
                {
                    if (target == null) Tell(new ItemEvent.NothingToPlay());
                    else PlayRequested?.Invoke(target);
                }, e => Tell(new ItemEvent.Failed(e)));
                break;
            case ItemAction.MarkWatched:
            case ItemAction.MarkUnwatched:
                var watched = action == ItemAction.MarkWatched;
                Change(ItemPatch.Watched(item.Id, watched, item), () => source.SetWatched(item.Id, watched));
                break;
            case ItemAction.RemoveFromContinueWatching:
                Change(null, () => source.HideFromContinueWatching(item.Id));
                break;
            case ItemAction.AddToMyList:
            case ItemAction.RemoveFromMyList:
                var inList = action == ItemAction.AddToMyList;
                Change(ItemPatch.InWatchlist(item.Id, inList), () => source.SetMyList(item.Id, inList));
                break;
            default:
                return false;
        }
        return true;
    }

    public void Dispose()
    {
        gone = true;
        changes.Dispose();
    }

    void Fetch() => Plays.When(scheduler, () => source.Item(ItemId), d =>
    {
        detail = d;
        error = null;
        Compute();
    }, e =>
    {
        error = e;
        Compute();
    });

    /// <summary>Shows <paramref name="patch"/> at once and sends <paramref name="call"/>; refused, the screen, if still open, shows what it was and says why.</summary>
    void Change(ItemPatch? patch, Func<System.Threading.Tasks.Task> call)
    {
        var before = detail;
        if (patch != null && detail != null)
        {
            detail = patch.Apply(detail);
            Compute();
        }
        var after = detail;
        Plays.When(scheduler, call, () => { }, e =>
        {
            if (ReferenceEquals(detail, after)) detail = before;
            Compute();
            Tell(new ItemEvent.Failed(e));
        });
    }

    void Compute()
    {
        if (gone) return;
        if (detail == null)
        {
            State.Value = error == null ? new ItemActionsState.Loading() : new ItemActionsState.Failed(error);
            return;
        }
        var item = detail;
        var episode = item.Kind == "episode";
        var show = string.IsNullOrEmpty(item.GrandparentTitle) ? null : item.GrandparentTitle;
        State.Value = new ItemActionsState.Ready(
            item,
            source.Origin,
            episode ? show ?? item.Title : item.Title,
            episode ? ItemFormat.Join(" · ", Format.CardSubtitle(item), show != null ? item.Title : null) : Format.CardSubtitle(item),
            ItemActions.Of(item, detail.InWatchlist ?? false, continueWatching));
    }

    void Tell(ItemEvent e) => Told?.Invoke(e);
}
