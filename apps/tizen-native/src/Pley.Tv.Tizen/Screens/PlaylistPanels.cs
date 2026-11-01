using System;
using System.Collections.Generic;
using System.Linq;
using Pley.Tv.Core.Api;
using Pley.Tv.Core.Ui;
using Pley.Tv.Tizen.Kit;
using Tizen.NUI;
using Tizen.NUI.BaseComponents;
using Timer = Tizen.NUI.Timer;

namespace Pley.Tv.Tizen.Screens;

/// <summary>
/// Add to playlist on TV (the Android app's TvAddToPlaylistScreen; design spec: a side panel), a
/// page of its own: over the item's backdrop and name, a panel with a new playlist to name (OK
/// types; the keyboard's Done makes it), then the profile's playlists that take it. Focus starts
/// on the first of those, else on the name. Once added, the panel says so ("Added to Road trip")
/// for a moment and the page closes; Back closes it.
/// </summary>
sealed class AddToPlaylistScreen : ShellPage
{
    const float TextColumn = 464, Art = 40;

    readonly Shell layers;
    readonly AddToPlaylistModel model;
    readonly Images images;
    readonly IDisposable watch;
    readonly BackdropView backdrop;
    readonly TextLabel name;
    readonly PanelColumn column = new PanelColumn();
    readonly Timer done = new Timer((uint)PlaylistRules.DoneMs);
    readonly Dictionary<FocusItem, long> cards = new Dictionary<FocusItem, long>();
    TextInput? field;
    PanelChoice? create;
    PanelColumn.Mark listMark;
    string? shownTitle;
    string? shownBackdrop;
    bool built;
    bool placed;
    bool finished;
    bool gone;

    public AddToPlaylistScreen(TvShell shell, Shell layers, AddToPlaylistModel model, PageKey key, Images images) : base(shell, key, "add-to-playlist")
    {
        this.layers = layers;
        this.model = model;
        this.images = images;
        View.BackgroundColor = PleyColors.Bg;
        backdrop = new BackdropView(images);
        View.Add(backdrop);
        View.Add(new ScrimView(ScrimView.Item));
        var behind = new View { Position = new Position(Ui.Dp(PleyTv.Gutter), Ui.Dp(58)) };
        behind.Add(Icons.Icon("list-plus", 32, PleyColors.Muted));
        var label = Text.Label(PlaylistText.AddToPlaylist, PleyType.Label, PleyColors.Muted, width: TextColumn);
        label.PositionY = Ui.Dp(32 + 16);
        behind.Add(label);
        name = Text.Label("", PleyType.PageTitle, PleyColors.Fg, maxLines: 3, width: TextColumn);
        name.VerticalAlignment = VerticalAlignment.Top;
        name.PositionY = label.PositionY + label.SizeHeight + Ui.Dp(6);
        behind.Add(name);
        View.Add(behind);
        View.Add(new View { Size = new Size(Ui.Dp(PleyTv.CanvasWidth), Ui.Dp(PleyTv.CanvasHeight)), BackgroundColor = PleyColors.Scrim });
        View.Add(column);
        Group.Add(column.Group);
        // Focus stays in the panel.
        Group.Exit = _ => true;
        done.Tick += (s, e) =>
        {
            if (!gone && Shell.Top == this) Shell.Back?.Invoke();
            return false;
        };
        model.Told += OnTold;
        watch = model.State.Watch(Show);
    }

    public override void Enter()
    {
        // Nothing to focus while what is added loads: no key reaches the page under it meanwhile.
        if (!Tree.Enter(Group)) Tree.Blur();
    }

    public override void Gone()
    {
        gone = true;
        watch.Dispose();
        model.Told -= OnTold;
        done.Stop();
        model.Dispose();
    }

    void OnTold(AddEvent e)
    {
        switch (e)
        {
            case AddEvent.Added added:
                finished = true;
                Done(PlaylistText.Added(added));
                done.Start();
                break;
            case AddEvent.Failed failed:
                Shell.Toast(Errors.Message(failed.Error));
                break;
        }
    }

    void Show(AddToPlaylistState state)
    {
        if (finished) return;
        if (state.Backdrop.FirstOrDefault() != shownBackdrop)
        {
            shownBackdrop = state.Backdrop.FirstOrDefault();
            backdrop.Show(state.Backdrop);
        }
        name.Text = state.Title ?? "";

        // The top (the title, the field, Create) is made again only when what is added is known;
        // the field's words and kind come with it, so it is never made again while typing.
        if (!built || state.Title != shownTitle)
        {
            built = true;
            shownTitle = state.Title;
            column.Clear();
            cards.Clear();
            column.Title(PlaylistText.AddToPlaylist, state.Title);
            var input = new TextInput(layers.Input, PanelColumn.Inner - 8, PlaylistText.AddNewPlaceholder(state), action: InputMethod.ActionButtonTitleType.Done);
            input.Changed += _ => Enable(model.State.Value);
            input.Submitted = Create;
            field = column.Place(input, TextInput.Height, 4, 6, input.Item);
            column.Space(6);
            create = column.Choice(new PanelChoice(PlaylistText.AddCreate, Create, "plus"));
            listMark = column.Here;
        }
        Enable(state);
        ShowPlaylists(state);
    }

    /// <summary>The field and Create take focus once what is added is known and nothing is on its way; Create also needs a title that will do.</summary>
    void Enable(AddToPlaylistState state)
    {
        if (field == null || create == null) return;
        field.Item.Enabled = state.CanCreate;
        create.Enabled = state.CanCreate && PlaylistRules.Check(field.Value) == null;
    }

    void Create()
    {
        if (field != null) model.Create(field.Value);
    }

    /// <summary>"Your playlists", then a row each (art, title, meta), or their failure, or none yet, or still bars while they load.</summary>
    void ShowPlaylists(AddToPlaylistState state)
    {
        long? focusedCard = Tree.Current != null && cards.TryGetValue(Tree.Current, out var id) ? id : null;
        column.ClearFrom(listMark);
        cards.Clear();
        column.Heading(PlaylistText.AddYours);
        switch (state.Playlists)
        {
            case Loadable<IReadOnlyList<PlaylistCard>>.Loading:
                for (var i = 0; i < 2; i++)
                {
                    var bar = new View
                    {
                        Size = new Size(Ui.Dp((PanelColumn.Inner - 32) * 0.8f), Ui.Dp(40)),
                        CornerRadius = Ui.Dp(20),
                        BackgroundColor = PleyColors.Surface3,
                    };
                    column.Place(bar, 40, 16, i == 0 ? 8 : 16);
                }
                break;
            default:
                if (PlaylistText.AddFailed(state) is { } failed) column.Sentence(failed, PleyType.Body, PleyColors.DangerText, 16, 8, 3);
                else if (PlaylistText.AddNone(state) is { } none) column.Sentence(none, PleyType.Body, PleyColors.Muted, 16, 8, 3);
                break;
        }
        PanelChoice? first = null;
        PanelChoice? again = null;
        foreach (var card in state.Playlists.Value ?? Array.Empty<PlaylistCard>())
        {
            var art = new PlaylistArtView(card, images, Art, glyph: 18);
            var row = column.Choice(new PanelChoice(card.Title, () => model.Add(card), line: card.Meta, leading: art, leadingSize: Art));
            // Waiting while a change is on its way: drawn as disabled, OK does nothing.
            if (state.Busy) row.Opacity = 0.4f;
            cards[row.Item] = card.Id;
            first ??= row;
            if (card.Id == focusedCard) again = row;
        }
        if (again != null) Tree.Focus(again.Item);
        // The playlist with focus went: the first, else the field.
        else if (focusedCard != null && !(first != null && Tree.Focus(first.Item)) && field != null) Tree.Focus(field.Item);

        // Focus starts once the playlists have come: on the first of them, else on the name.
        if (placed || state.Kind == null || state.Playlists is Loadable<IReadOnlyList<PlaylistCard>>.Loading) return;
        placed = true;
        if (Shell.Top != this) return;
        if (!(first != null && Tree.Focus(first.Item)) && field != null) Tree.Focus(field.Item);
    }

    /// <summary>"Added to Road trip": the accent check and the words, centred in the panel, until the page closes.</summary>
    void Done(string message)
    {
        column.Clear();
        cards.Clear();
        field = null;
        create = null;
        var words = PanelColumn.Lines(message, PleyType.Title, PleyColors.Fg, PanelColumn.Inner, 3);
        words.HorizontalAlignment = HorizontalAlignment.Center;
        var height = 48 + 16 + words.SizeHeight / Ui.Scale;
        column.Place(Icons.Icon("circle-check", 48, PleyColors.Accent), 48, (PanelColumn.Inner - 48) / 2, (PanelColumn.Room - height) / 2);
        column.Place(words, words.SizeHeight / Ui.Scale, 0, 16);
        // Nothing in the panel takes focus now; Back still closes the page.
        Tree.Blur();
    }
}

/// <summary>
/// New playlist and Save these filters on TV (the Android app's TvNewPlaylistScreen), a page of
/// its own: what it makes beside a side panel with the title (OK types) and, for an empty
/// playlist, what it plays; Create (or Save, for filters) opens the playlist made in this page's
/// place. Focus starts on the title.
/// </summary>
sealed class NewPlaylistScreen : ShellPage
{
    const float TextColumn = 464;

    readonly NewPlaylistModel model;
    readonly IDisposable watch;
    readonly PanelColumn column = new PanelColumn();
    readonly TextInput field;
    readonly PanelColumn.Mark choicesMark;
    readonly Dictionary<FocusItem, string> kindRows = new Dictionary<FocusItem, string>();
    PanelChoice? action;
    string kind = PlaylistKind.Video;
    bool started;
    bool gone;

    public NewPlaylistScreen(TvShell shell, Shell layers, NewPlaylistModel model, PageKey key) : base(shell, key, "new-playlist")
    {
        this.model = model;
        View.BackgroundColor = PleyColors.Bg;
        var title = PlaylistText.NewTitle(model);
        var behind = new View { Position = new Position(Ui.Dp(PleyTv.Gutter), Ui.Dp(58)) };
        behind.Add(Icons.Icon("list-plus", 32, PleyColors.Muted));
        var heading = PanelColumn.Lines(title, PleyType.PageTitle, PleyColors.Fg, TextColumn, 2);
        heading.PositionY = Ui.Dp(32 + 16);
        behind.Add(heading);
        var body = PanelColumn.Lines(PlaylistText.NewBodyOf(model), PleyType.BodyLarge, PleyColors.Muted, TextColumn, 8);
        body.PositionY = heading.PositionY + heading.SizeHeight + Ui.Dp(12);
        behind.Add(body);
        View.Add(behind);
        View.Add(new View { Size = new Size(Ui.Dp(PleyTv.CanvasWidth), Ui.Dp(PleyTv.CanvasHeight)), BackgroundColor = PleyColors.Scrim });
        View.Add(column);

        column.Title(title);
        field = new TextInput(layers.Input, PanelColumn.Inner - 8, PlaylistText.FormTitle, action: InputMethod.ActionButtonTitleType.Done);
        field.Changed += _ => Enable();
        field.Submitted = () =>
        {
            if (Ok) Create();
        };
        column.Place(field, TextInput.Height, 4, 6, field.Item);
        column.Space(6);
        column.Sentence(PlaylistText.FormHint, PleyType.Caption, PleyColors.Muted, 8, 4);
        choicesMark = column.Here;
        Choices();
        Group.Add(column.Group);
        // Focus stays in the panel.
        Group.Exit = _ => true;
        model.Told += OnTold;
        watch = model.Busy.Watch(_ => Enable());
    }

    bool Ok => PlaylistRules.Check(field.Value) == null && !model.Busy.Value;

    public override void Enter()
    {
        if (!started)
        {
            started = true;
            if (Tree.Focus(field.Item)) return;
        }
        Tree.Enter(Group);
    }

    public override void Gone()
    {
        gone = true;
        watch.Dispose();
        model.Told -= OnTold;
    }

    void OnTold(CreateEvent e)
    {
        if (gone) return;
        switch (e)
        {
            case CreateEvent.Created created:
                // The playlist made opens in this page's place.
                if (Shell.Top == this) Shell.ReplaceTop(new PageKey.Playlist(created.PlaylistId));
                break;
            case CreateEvent.Failed failed:
                Shell.Toast(Errors.Message(failed.Error));
                break;
        }
    }

    void Create() => model.Create(field.Value, kind);

    void Enable()
    {
        if (action != null) action.Enabled = Ok;
    }

    /// <summary>
    /// Under the title: for an empty playlist "For" with Movies &amp; TV and Music (the one chosen
    /// checked), then Create, or Save for filters. Made again when the choice changes, focus
    /// staying on the row it was on.
    /// </summary>
    void Choices()
    {
        var focusedKind = Tree.Current != null && kindRows.TryGetValue(Tree.Current, out var k) ? k : null;
        var onAction = action != null && Tree.Current == action.Item;
        column.ClearFrom(choicesMark);
        kindRows.Clear();
        FocusItem? again = null;
        if (model.AsksKind)
        {
            column.Heading(PlaylistText.FormFor);
            foreach (var (label, icon, value) in new[]
            {
                (PlaylistText.FormVideo, "list-video", PlaylistKind.Video),
                (PlaylistText.FormMusic, "list-music", PlaylistKind.Audio),
            })
            {
                var row = column.Choice(new PanelChoice(label, () =>
                {
                    kind = value;
                    Choices();
                }, icon, chosen: kind == value));
                kindRows[row.Item] = value;
                if (value == focusedKind) again = row.Item;
            }
        }
        column.Space(16);
        var (text, actionIcon) = PlaylistText.NewAction(model);
        action = column.Choice(new PanelChoice(text, Create, actionIcon));
        Enable();
        if (onAction) again = action.Item;
        if (again != null) Tree.Focus(again);
    }
}
