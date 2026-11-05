using System;
using System.Collections.Generic;
using System.Linq;
using Pley.Tv.Core.Api;

namespace Pley.Tv.Core.Ui;

/// <summary>What a playlist entry's actions do (the side panel a held OK or More opens).</summary>
public enum EntryAction
{
    /// <summary>Plays from the entry (<see cref="PlaylistModel.Play"/> from its position).</summary>
    PlayFromHere,
    /// <summary>Opens the entry's page (<see cref="PlaylistModel.Opens"/>).</summary>
    GoTo,
    /// <summary>Lifts the entry: Up and Down move it, OK drops it, Back puts it back.</summary>
    Move,
    /// <summary>Takes it out at once (<see cref="PlaylistModel.Remove"/> without an undo); focus goes to <see cref="PlaylistRules.StandIn"/>.</summary>
    Remove,
}

/// <summary>An entry's action: what it says, its icon by Lucide's name, whether it reads as a danger, and whether it can be pressed.</summary>
public sealed record EntryActionRow(EntryAction Action, string Label, string Icon, bool Danger = false, bool Enabled = true);

/// <summary>The playlist page's buttons, in order.</summary>
public enum PlaylistButton
{
    Play,
    Shuffle,
    Rename,
    Delete,
}

/// <summary>A button of the playlist page: what it says, its icon by Lucide's name, whether it can be pressed. Play is a pill; the others round.</summary>
public sealed record PlaylistButtonView(PlaylistButton Button, string Label, string Icon, bool Enabled = true);

/// <summary>
/// What the TV's playlists' pages and panels say (the Android app's strings_tv_playlists.xml), and
/// the rules their composables hold that are words rather than drawing.
/// </summary>
public static class PlaylistText
{
    public static string PlaylistsTitle => Strings.T("playlists.list.title");
    public static string New => Strings.T("playlists.list.new");
    public static string PlaylistsFailed => Strings.T("playlists.list.failed");
    public static string NoneTitle => Strings.T("playlists.list.noneTitle");
    public static string NoneBody => Strings.T("playlists.list.noneBody");
    public static string Retry => Strings.T("playlists.list.retry");
    /// <summary>The Smart badge, and the word after a smart playlist's meta.</summary>
    public static string SmartBadge => Strings.T("playlists.list.smart");

    public static string VideoPlaylist => Strings.T("playlists.page.video");
    public static string MusicPlaylist => Strings.T("playlists.page.music");
    public static string SmartPlaylist => Strings.T("playlists.page.smart");
    public static string Play => Strings.T("playlists.page.play");
    public static string Shuffle => Strings.T("playlists.page.shuffle");
    public static string Rename => Strings.T("playlists.page.rename");
    public static string Delete => Strings.T("playlists.page.delete");
    public static string Failed => Strings.T("playlists.page.failed");
    public static string NotFound => Strings.T("playlists.page.notFound");
    public static string NotFoundBody => Strings.T("playlists.page.notFoundBody");
    public static string Back => Strings.T("playlists.page.back");
    public static string EmptySmart => Strings.T("playlists.page.emptySmart");
    public static string EmptyVideo => Strings.T("playlists.page.emptyVideo");
    public static string EmptyMusic => Strings.T("playlists.page.emptyMusic");
    public static string More => Strings.T("playlists.page.more");
    public static string PlayFromHere => Strings.T("playlists.page.playFromHere");
    public static string Move => Strings.T("playlists.page.move");
    public static string Remove => Strings.T("playlists.page.remove");
    public static string Moving => Strings.T("playlists.page.moving");
    public static string DeleteTitle => Strings.T("playlists.page.deleteTitle");
    public static string Cancel => Strings.T("playlists.page.cancel");
    public static string RenameTitle => Strings.T("playlists.page.renameTitle");
    public static string Save => Strings.T("playlists.page.save");

    public static string FormTitle => Strings.T("playlists.form.title");
    public static string FormFor => Strings.T("playlists.form.for");
    public static string FormVideo => Strings.T("playlists.form.video");
    public static string FormMusic => Strings.T("playlists.form.music");
    public static string FormCreate => Strings.T("playlists.form.create");
    public static string FormHint => Strings.T("playlists.form.hint");
    public static string NewBody => Strings.T("playlists.form.newBody");
    public static string SmartTitle => Strings.T("playlists.form.smartTitle");
    public static string SmartBodyPlain => Strings.T("playlists.form.smartBody");

    public static string AddToPlaylist => Strings.T("playlists.add.title");
    public static string AddNewVideo => Strings.T("playlists.add.newVideo");
    public static string AddNewMusic => Strings.T("playlists.add.newMusic");
    public static string AddCreate => Strings.T("playlists.add.create");
    public static string AddYours => Strings.T("playlists.add.yours");
    public static string AddNoneVideo => Strings.T("playlists.add.noneVideo");
    public static string AddNoneMusic => Strings.T("playlists.add.noneMusic");
    public static string AddNothing => Strings.T("playlists.add.nothing");

    /// <summary>"12 items", "1 item": a playlist's entries, or what is added.</summary>
    public static string Items(long n) => Strings.Count("playlists.count.items", n);

    /// <summary>"Go to Northern Lights".</summary>
    public static string GoTo(string title) => Strings.T("playlists.page.goTo", ("title", title));

    /// <summary>Removed “Half Loop”.</summary>
    public static string Removed(string title) => Strings.T("playlists.page.removed", ("title", title));

    /// <summary>“Road trip” will be deleted. What's in it stays in your libraries.</summary>
    public static string DeleteBody(string title) => Strings.T("playlists.page.deleteBody", ("title", title));

    /// <summary>"Genre: Comedy · Unwatched. It keeps up with the library by itself."</summary>
    public static string SmartBody(IReadOnlyList<string> filters) =>
        filters.Count == 0 ? SmartBodyPlain : Strings.T("playlists.form.smartBodyFilters", ("filters", string.Join(" · ", filters)));

    /// <summary>"Added to Road trip", "Added 19 items to Road trip", "Nothing to add", as the web says them.</summary>
    public static string Added(AddEvent.Added added) => added.Entries switch
    {
        0 => AddNothing,
        null or 1 => Strings.T("playlists.add.addedTo", ("playlist", added.Playlist)),
        int n => Strings.Count("playlists.add.addedItems", n, ("playlist", added.Playlist)),
    };

    // ---- the playlists page

    /// <summary>The playlists page's message in place of covers: they failed with nothing to show (Retry), or there are none; null otherwise.</summary>
    public static LibraryMessage? PlaylistsMessage(Loadable<IReadOnlyList<PlaylistCard>> state)
    {
        if (state.Value == null && state is Loadable<IReadOnlyList<PlaylistCard>>.Failed f)
            return new LibraryMessage(LibraryText.FailedTitle(f.Error, PlaylistsFailed), LibraryText.FailedBody(f.Error), MessageAction.Retry);
        if (state.Value is { Count: 0 }) return new LibraryMessage(NoneTitle, NoneBody);
        return null;
    }

    /// <summary>New playlist shows unless the playlists failed with nothing to show.</summary>
    public static bool ShowsNew(Loadable<IReadOnlyList<PlaylistCard>> state) => state.Value != null || state is not Loadable<IReadOnlyList<PlaylistCard>>.Failed;

    // ---- a playlist's page

    /// <summary>The page's message in place of it: not found (404, Back), or failed with nothing to show (Retry); null otherwise.</summary>
    public static LibraryMessage? PlaylistMessage(Loadable<PlaylistPage> state)
    {
        if (state.Value != null || state is not Loadable<PlaylistPage>.Failed f) return null;
        if (f.Error is ApiException { Status: 404 }) return new LibraryMessage(NotFound, NotFoundBody, MessageAction.Back);
        return new LibraryMessage(LibraryText.FailedTitle(f.Error, Failed), LibraryText.FailedBody(f.Error), MessageAction.Retry);
    }

    /// <summary>What the playlist plays, over its title: "Smart playlist", "Video playlist" or "Music playlist"; its icon list-video or list-music.</summary>
    public static string KindLine(PlaylistPage page) => page.Smart ? SmartPlaylist : page.Video ? VideoPlaylist : MusicPlaylist;

    /// <summary>"12 items · 3h 12m  ·  Genre: Comedy": the meta and a smart playlist's filter, after its Smart badge.</summary>
    public static string FactsLine(PlaylistPage page) => string.Join("  ·  ", new[] { page.Meta }.Concat(page.Notes).Where(s => s.Length > 0));

    /// <summary>What shows under the buttons of a playlist without entries; null with some.</summary>
    public static string? EmptyLine(PlaylistPage page) => page.Rows.Count > 0 ? null : page.Smart ? EmptySmart : page.Video ? EmptyVideo : EmptyMusic;

    /// <summary>
    /// The buttons: Play (with focus when there is something to play), Shuffle, Rename for a
    /// playlist that isn't smart, Delete.
    /// </summary>
    public static IReadOnlyList<PlaylistButtonView> Buttons(PlaylistPage page)
    {
        var buttons = new List<PlaylistButtonView>
        {
            new PlaylistButtonView(PlaylistButton.Play, Play, "play", page.CanPlay),
            new PlaylistButtonView(PlaylistButton.Shuffle, Shuffle, "shuffle", page.CanShuffle),
        };
        if (page.Editable) buttons.Add(new PlaylistButtonView(PlaylistButton.Rename, Rename, "pencil"));
        buttons.Add(new PlaylistButtonView(PlaylistButton.Delete, Delete, "trash-2"));
        return buttons;
    }

    /// <summary>The button focus starts on: Play, else Rename, else Delete (a smart playlist with nothing in it).</summary>
    public static PlaylistButton FirstFocus(PlaylistPage page) => page.CanPlay ? PlaylistButton.Play : page.Editable ? PlaylistButton.Rename : PlaylistButton.Delete;

    /// <summary>An entry's actions: Play from here, Go to it, and for playlists that aren't smart Move (with two or more entries) and Remove.</summary>
    public static IReadOnlyList<EntryActionRow> EntryActions(PlaylistRow row, PlaylistPage page)
    {
        var actions = new List<EntryActionRow>
        {
            new EntryActionRow(EntryAction.PlayFromHere, PlayFromHere, "play"),
            new EntryActionRow(EntryAction.GoTo, GoTo(row.Title), "arrow-right"),
        };
        if (page.Editable)
        {
            actions.Add(new EntryActionRow(EntryAction.Move, Move, "arrow-up-down", Enabled: page.Rows.Count > 1));
            actions.Add(new EntryActionRow(EntryAction.Remove, Remove, "x", Danger: true));
        }
        return actions;
    }

    // ---- the panels

    /// <summary>The Add to playlist panel's line under "Your playlists" when there are none of its kind; null otherwise.</summary>
    public static string? AddNone(AddToPlaylistState state) =>
        state.Kind != null && state.Playlists is Loadable<IReadOnlyList<PlaylistCard>>.Ready { Data.Count: 0 } ? state.Music ? AddNoneMusic : AddNoneVideo : null;

    /// <summary>The playlists' failure under "Your playlists", with nothing to show; null otherwise.</summary>
    public static string? AddFailed(AddToPlaylistState state) =>
        state.Playlists is Loadable<IReadOnlyList<PlaylistCard>>.Failed { Stale: null } f ? Errors.Message(f.Error) : null;

    /// <summary>The new playlist field's placeholder: "New video playlist" or "New music playlist".</summary>
    public static string AddNewPlaceholder(AddToPlaylistState state) => state.Music ? AddNewMusic : AddNewVideo;

    /// <summary>New playlist's or Save these filters' title, beside the panel and at its top.</summary>
    public static string NewTitle(NewPlaylistModel model) => model.IsSmart ? SmartTitle : New;

    /// <summary>The sentence under the title beside the panel.</summary>
    public static string NewBodyOf(NewPlaylistModel model) => model.IsSmart ? SmartBody(model.Filters) : NewBody;

    /// <summary>The panel's last row: Save (icon save) for filters, Create (icon plus) for a playlist.</summary>
    public static (string Label, string Icon) NewAction(NewPlaylistModel model) => model.IsSmart ? (Save, "save") : (FormCreate, "plus");
}
