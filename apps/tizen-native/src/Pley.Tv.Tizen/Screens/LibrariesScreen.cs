using System;
using System.Collections.Generic;
using System.Linq;
using Pley.Tv.Core.Api;
using Pley.Tv.Core.Ui;
using Pley.Tv.Tizen.Kit;
using Tizen.NUI;
using Tizen.NUI.BaseComponents;

namespace Pley.Tv.Tizen.Screens;

/// <summary>
/// Every library (design spec §2.1; the Android app's TvLibrariesScreen): the rail shows the first
/// three, and "Libraries" opens this page. The title, then a card per library in three columns:
/// its kind's icon, its name, how much it holds, "Scanning…" while the server scans it. After
/// them, while the rail has no room for Playlists, a card like theirs opens Playlists. The
/// profile may put them in its own order: Sort, or Move in a card's actions (holding OK). While
/// sorting, OK lifts a card, the arrows move it, OK drops it there (and saves), Back puts it back.
/// </summary>
sealed class LibrariesScreen : ShellPage
{
    const int Columns = LibrariesRules.Columns;
    const float CardHeight = 80, Gap = 16, Top = 20;

    readonly Shell layers;
    readonly LibrariesModel model;
    readonly Action<PageKey> go;
    readonly List<IDisposable> watches = new List<IDisposable>();
    readonly TextLabel title;
    readonly ButtonRow buttons = new ButtonRow(12);
    readonly View cards = new View();
    readonly FocusGroup grid = new FocusGroup(Axis.Grid, "libraries") { Columns = Columns };
    readonly View hint = new View();
    readonly TextLabel hintText;
    View? message;
    float scroll;

    public LibrariesScreen(TvShell shell, Shell layers, LibrariesModel model, KeyRepeatGate gate, Action<PageKey> go) : base(shell, new PageKey.Libraries(), "libraries")
    {
        this.layers = layers;
        this.model = model;
        this.go = go;
        title = Text.Label(LibraryText.LibrariesTitle, PleyType.PageTitle, PleyColors.Fg, width: 480);
        title.Position = new Position(Ui.Dp(PleyTv.Gutter), Ui.Dp(PleyTv.SafeVertical + (PleyTv.ButtonHeight - PleyType.PageTitle.Line) / 2));
        View.Add(title);
        buttons.PositionY = Ui.Dp(PleyTv.SafeVertical);
        View.Add(buttons);
        var top = PleyTv.SafeVertical + PleyTv.ButtonHeight + Top + 8;
        var viewport = new View
        {
            Position = new Position(0, Ui.Dp(top - 8)),
            Size = new Size(Ui.Dp(PleyTv.CanvasWidth), Ui.Dp(PleyTv.CanvasHeight - top + 8)),
            ClippingMode = ClippingModeType.ClipChildren,
        };
        cards.PositionY = Ui.Dp(8);
        viewport.Add(cards);
        View.Add(viewport);
        hintText = Text.Label("", PleyType.Label, PleyColors.Fg);
        hint.Add(hintText);
        hint.BackgroundColor = PleyColors.Surface3;
        hint.Hide();
        View.Add(hint);
        Group.Add(buttons.Group);
        Group.Add(grid);
        buttons.Group.Exit = d => d == Direction.Left && Shell.FocusRail();
        grid.Exit = d => d == Direction.Left && Scrolling.Left(gate, Shell.FocusRail);
        watches.Add(model.State.Watch(_ => Draw()));
        watches.Add(model.Sorting.Watch(_ => Draw()));
        watches.Add(model.Moving.Watch(_ => Draw()));
        model.Told += Told;
    }

    public override void Enter()
    {
        if (!Tree.Enter(grid) && !Tree.Enter(Group)) Shell.FocusRail();
    }

    public override bool Back() => model.Back();

    public override void Gone()
    {
        foreach (var w in watches) w.Dispose();
        model.Told -= Told;
        model.Dispose();
    }

    void Told(LibrariesEvent e) => Shell.Toast(e switch
    {
        LibrariesEvent.Failed f => Errors.Message(f.Error),
        _ => LibraryText.SortNeedsNewerServer,
    });

    void Draw()
    {
        var state = model.State.Value;
        var sorting = model.Sorting.Value;
        var moving = model.Moving.Value;
        var ready = state as LibrariesState.Ready;
        var sortable = ready?.Sortable == true;
        var focusedName = Tree.Current?.Name;
        var inPage = Tree.Current != null && InPage(Tree.Current);

        title.Text = LibraryText.LibrariesHeading(sorting && sortable);
        // The header's buttons at the right safe edge.
        foreach (var child in buttons.Children.ToList())
        {
            buttons.Remove(child);
            child.Dispose();
        }
        buttons.Group.Clear();
        buttons.Arrange();
        foreach (var b in LibraryText.LibrariesButtons(sorting, moving != null, sortable))
        {
            var action = b.Action;
            var view = buttons.Add(new ButtonView(b.Label, ButtonKind.Pill, () => Act(action, null), iconName: b.Icon));
            view.Enabled = b.Enabled;
        }
        buttons.PositionX = Ui.Dp(PleyTv.CanvasWidth - PleyTv.SafeHorizontal - buttons.WidthDp);

        message?.Dispose();
        message = null;
        foreach (var child in cards.Children.ToList())
        {
            cards.Remove(child);
            child.Dispose();
        }
        grid.Clear();
        var notice = LibraryText.LibrariesMessage(state);
        if (notice != null)
        {
            message = Notice(notice);
            View.Add(message);
        }
        else if (ready != null)
        {
            var width = (PleyTv.CanvasWidth - PleyTv.Gutter - PleyTv.SafeHorizontal - Gap * (Columns - 1)) / Columns;
            for (var i = 0; i < ready.Libraries.Count; i++)
            {
                var entry = ready.Libraries[i];
                var card = new Card("library/" + entry.Library.Id, LibraryText.KindIcon(entry.Library.Kind), entry.Library.Name, LibraryText.CardLine(entry),
                    width, sorting && sortable, moving == entry.Library.Id);
                card.Position = new Position(Ui.Dp(PleyTv.Gutter + i % Columns * (width + Gap)), Ui.Dp(i / Columns * (CardHeight + Gap)));
                var id = entry.Library.Id;
                var top = i / Columns * (CardHeight + Gap);
                card.Item.Selected = () =>
                {
                    if (model.Select(id) is { } key) go(key);
                };
                card.Item.LongSelected = () =>
                {
                    if (model.Moving.Value == null) Actions(entry, sortable);
                };
                card.Item.Menu = card.Item.LongSelected;
                card.Item.OwnKey = (key, down) =>
                {
                    if (model.Moving.Value != id) return false;
                    var by = key switch
                    {
                        "Left" => -1,
                        "Right" => 1,
                        "Up" => -Columns,
                        "Down" => Columns,
                        _ => 0,
                    };
                    if (by == 0) return false;
                    if (down) model.MoveLifted(by);
                    return true;
                };
                card.Item.FocusChanged += on =>
                {
                    if (on) Reveal(top);
                };
                cards.Add(card);
                grid.Add(card.Item);
            }
            if (model.ListsPlaylists)
            {
                var at = ready.Libraries.Count;
                var top = at / Columns * (CardHeight + Gap);
                var card = new Card("playlists", "list-music", PlaylistText.PlaylistsTitle, null, width, sorting: false, moving: false);
                card.Position = new Position(Ui.Dp(PleyTv.Gutter + at % Columns * (width + Gap)), Ui.Dp(top));
                card.Item.Selected = () =>
                {
                    if (model.SelectPlaylists() is { } key) go(key);
                };
                card.Item.FocusChanged += on =>
                {
                    if (on) Reveal(top);
                };
                cards.Add(card);
                grid.Add(card.Item);
            }
        }

        var hintLine = LibraryText.SortHint(sorting && sortable, moving != null);
        if (hintLine != null)
        {
            hintText.Text = hintLine;
            var w = hintText.NaturalSize.Width + Ui.Dp(40);
            hint.Size = new Size(w, Ui.Dp(PleyType.Label.Line + 24));
            hint.CornerRadius = hint.SizeHeight / 2;
            hintText.Position = new Position(Ui.Dp(20), Ui.Dp(12));
            hint.Position = new Position((Ui.Dp(PleyTv.CanvasWidth) - w) / 2, Ui.Dp(PleyTv.CanvasHeight - PleyTv.SafeVertical) - hint.SizeHeight);
            hint.Show();
        }
        else hint.Hide();

        // Focus where it was: the lifted card follows its library; the first card otherwise.
        if (!inPage) return;
        var named = moving != null ? "library/" + moving : focusedName;
        var again = Find(named);
        if (again == null || !Tree.Focus(again)) Enter();
    }

    FocusItem? Find(string? name)
    {
        if (name == null) return null;
        foreach (var g in new[] { grid, buttons.Group })
            foreach (var c in g.Children)
                if (c is FocusItem item && item.Name == name) return item;
        return null;
    }

    bool InPage(FocusNode node)
    {
        for (var n = node.Parent; n != null; n = n.Parent)
            if (n == Group) return true;
        return false;
    }

    /// <summary>Keeps the focused card's row in view.</summary>
    void Reveal(float top)
    {
        var room = PleyTv.CanvasHeight - (PleyTv.SafeVertical + PleyTv.ButtonHeight + Top) - PleyTv.SafeVertical;
        if (top < scroll) scroll = top;
        else if (top + CardHeight > scroll + room) scroll = top + CardHeight - room;
        cards.PositionY = Ui.Dp(8 - scroll);
    }

    void Act(LibrariesAction action, Library? library)
    {
        switch (action)
        {
            case LibrariesAction.Sort:
                model.StartSorting();
                // The button goes: the first library takes focus, ready to lift.
                if (grid.Children.Count > 0 && grid.Children[0] is FocusItem first) Tree.Focus(first);
                break;
            case LibrariesAction.Done:
                model.Done();
                break;
            case LibrariesAction.ServerOrder:
                model.ServerOrder();
                break;
            case LibrariesAction.Open:
                if (library != null) go(new PageKey.Library(library.Id));
                break;
            case LibrariesAction.Move:
                if (library != null) model.MoveFromActions(library.Id);
                break;
        }
    }

    /// <summary>A card's actions in the side panel: Open, and Move and the server's order while they can be sorted.</summary>
    void Actions(LibraryEntry entry, bool sortable)
    {
        var actions = LibraryText.CardActions(sortable);
        var lines = actions.Select(a => new SidePanelSheet.Line(a.Label, Icon: a.Icon)).ToList();
        new SidePanelSheet(layers, entry.Library.Name, lines, 0, i =>
        {
            Act(actions[i].Action, entry.Library);
            return false;
        }).Show();
    }

    View Notice(LibraryMessage m)
    {
        var view = new View { Position = new Position(Ui.Dp(PleyTv.Gutter), Ui.Dp(140)) };
        view.Add(Text.Label(m.Title, PleyType.Section, PleyColors.Fg, width: PleyTv.CanvasWidth - 2 * PleyTv.Gutter));
        var words = Text.Label(m.Body, PleyType.Body, PleyColors.Muted, maxLines: 3, width: 520);
        words.VerticalAlignment = VerticalAlignment.Top;
        words.PositionY = Ui.Dp(PleyType.Section.Line + 8);
        view.Add(words);
        if (m.ActionLabel is { } label)
        {
            var row = new ButtonRow();
            row.Add(new ButtonView(label, ButtonKind.Pill, model.Refresh, iconName: "refresh-cw"));
            row.PositionY = Ui.Dp(PleyType.Section.Line + 8 + PleyType.Body.Line * 2 + 24);
            view.Add(row);
            grid.Add(row.Group);
        }
        return view;
    }

    /// <summary>
    /// One library, or Playlists with no line under its name: a Surface2 card; focused, the accent
    /// fill with OnAccent content at 1.05. While sorting, a handle at its end; lifted, Surface3
    /// with a ring around it, at its size.
    /// </summary>
    sealed class Card : View
    {
        readonly bool moving;
        readonly View iconBox;
        readonly ImageView icon;
        readonly TextLabel name;
        readonly TextLabel? line;
        readonly ImageView? grip;

        public Card(string focusName, string iconName, string title, string? caption, float width, bool sorting, bool moving)
        {
            this.moving = moving;
            Size = new Size(Ui.Dp(width), Ui.Dp(CardHeight));
            CornerRadius = Ui.Dp(PleyShape.CardRadius);
            PivotPoint = global::Tizen.NUI.PivotPoint.Center;
            if (moving)
            {
                BorderlineWidth = Ui.Dp(PleyShape.RingStroke);
                BorderlineColor = PleyColors.AccentBright;
                BorderlineOffset = -1;
            }
            Item = new FocusItem(focusName);
            iconBox = new View
            {
                Size = new Size(Ui.Dp(PleyTv.ButtonHeight), Ui.Dp(PleyTv.ButtonHeight)),
                Position = new Position(Ui.Dp(18), Ui.Dp((CardHeight - PleyTv.ButtonHeight) / 2)),
                CornerRadius = Ui.Dp(PleyShape.ControlRadius),
            };
            icon = Icons.Icon(iconName, PleyTv.IconSize, PleyColors.Fg);
            icon.Position = new Position(Ui.Dp((PleyTv.ButtonHeight - PleyTv.IconSize) / 2), Ui.Dp((PleyTv.ButtonHeight - PleyTv.IconSize) / 2));
            iconBox.Add(icon);
            Add(iconBox);
            var x = 18 + PleyTv.ButtonHeight + 14;
            var room = width - x - 18 - (sorting ? PleyTv.IconSize + 12 : 0);
            var textTop = (CardHeight - PleyType.Title.Line - (caption != null ? 2 + PleyType.Caption.Line : 0)) / 2;
            name = Text.Label(title, PleyType.Title, PleyColors.Fg, width: room);
            name.Position = new Position(Ui.Dp(x), Ui.Dp(textTop));
            Add(name);
            if (caption != null)
            {
                line = Text.Label(caption, PleyType.Caption, PleyColors.Fg, width: room);
                line.Position = new Position(Ui.Dp(x), Ui.Dp(textTop + PleyType.Title.Line + 2));
                Add(line);
            }
            if (sorting)
            {
                grip = Icons.Icon("grip-vertical", PleyTv.IconSize, PleyColors.Fg);
                grip.Position = new Position(Ui.Dp(width - 18 - PleyTv.IconSize), Ui.Dp((CardHeight - PleyTv.IconSize) / 2));
                Add(grip);
            }
            Item.FocusChanged += Look;
            Look(false);
        }

        public FocusItem Item { get; }

        void Look(bool focused)
        {
            if (Disposed) return;
            var accent = focused && !moving;
            BackgroundColor = moving ? PleyColors.Surface3 : accent ? PleyColors.Accent : PleyColors.Surface2;
            var ink = accent ? PleyColors.OnAccent : PleyColors.Fg;
            name.TextColor = ink;
            if (line != null) line.TextColor = new Color(ink.R, ink.G, ink.B, 0.75f);
            icon.Color = ink;
            iconBox.BackgroundColor = new Color(ink.R, ink.G, ink.B, 0.12f);
            if (grip != null) grip.Color = moving ? PleyColors.AccentBright : new Color(ink.R, ink.G, ink.B, 0.6f);
            var scale = accent ? 1.05f : 1f;
            Scale = new Vector3(scale, scale, 1);
        }
    }
}
