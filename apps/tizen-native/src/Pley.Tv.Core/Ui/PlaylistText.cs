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
    public const string PlaylistsTitle = "Playlists";
    public const string New = "New playlist";
    public const string PlaylistsFailed = "Couldn't load your playlists";
    public const string NoneTitle = "No playlists yet";
    public const string NoneBody = "Make one here, or use “Add to playlist” on a movie, show, album or episode (also when holding OK on a poster).";
    public const string Retry = "Retry";
    /// <summary>The Smart badge, and the word after a smart playlist's meta.</summary>
    public const string SmartBadge = "Smart";

    public const string VideoPlaylist = "Video playlist";
    public const string MusicPlaylist = "Music playlist";
    public const string SmartPlaylist = "Smart playlist";
    public const string Play = "Play";
    public const string Shuffle = "Shuffle";
    public const string Rename = "Rename";
    public const string Delete = "Delete";
    public const string Failed = "Couldn't load this playlist";
    public const string NotFound = "Playlist not found";
    public const string NotFoundBody = "It was deleted, or it belongs to another profile.";
    public const string Back = "Back";
    public const string EmptySmart = "Nothing matches this playlist's filter yet.";
    public const string EmptyVideo = "This playlist is empty. Use “Add to playlist” on a movie, show or episode, or hold OK on a poster.";
    public const string EmptyMusic = "This playlist is empty. Use “Add to playlist” on an artist or album, or hold OK on a cover.";
    public const string More = "More";
    public const string PlayFromHere = "Play from here";
    public const string Move = "Move";
    public const string Remove = "Remove from playlist";
    public const string Moving = "Up and down to move it, OK to drop it here, Back to put it back";
    public const string DeleteTitle = "Delete playlist?";
    public const string Cancel = "Cancel";
    public const string RenameTitle = "Rename playlist";
    public const string Save = "Save";

    public const string FormTitle = "Title";
    public const string FormFor = "For";
    public const string FormVideo = "Movies & TV";
    public const string FormMusic = "Music";
    public const string FormCreate = "Create";
    public const string FormHint = "Press OK on the title to type it.";
    public const string NewBody = "Name it, then add to it with “Add to playlist” on any title.";
    public const string SmartTitle = "Save these filters";
    public const string SmartBodyPlain = "It keeps up with the library by itself.";

    public const string AddToPlaylist = "Add to playlist";
    public const string AddNewVideo = "New video playlist";
    public const string AddNewMusic = "New music playlist";
    public const string AddCreate = "Create";
    public const string AddYours = "Your playlists";
    public const string AddNoneVideo = "No video playlists yet: name one above.";
    public const string AddNoneMusic = "No music playlists yet: name one above.";
    public const string AddNothing = "Nothing to add";

    /// <summary>"Go to Northern Lights".</summary>
    public static string GoTo(string title) => $"Go to {title}";

    /// <summary>Removed “The End of the World”.</summary>
    public static string Removed(string title) => $"Removed “{title}”";

    /// <summary>“Road trip” will be deleted. What's in it stays in your libraries.</summary>
    public static string DeleteBody(string title) => $"“{title}” will be deleted. What's in it stays in your libraries.";

    /// <summary>"Genre: Comedy · Unwatched. It keeps up with the library by itself."</summary>
    public static string SmartBody(IReadOnlyList<string> filters) =>
        filters.Count == 0 ? SmartBodyPlain : $"{string.Join(" · ", filters)}. It keeps up with the library by itself.";

    /// <summary>"Added to Road trip", "Added 19 items to Road trip", "Nothing to add", as the web says them.</summary>
    public static string Added(AddEvent.Added added) => added.Entries switch
    {
        0 => AddNothing,
        null or 1 => $"Added to {added.Playlist}",
        var n => $"Added {n} items to {added.Playlist}",
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
