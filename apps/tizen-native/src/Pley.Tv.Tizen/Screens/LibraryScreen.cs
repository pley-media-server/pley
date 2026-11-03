using System;
using System.Collections.Generic;
using System.Linq;
using Pley.Tv.Core.Ui;
using Pley.Tv.Tizen.Kit;
using Tizen.NUI;
using Tizen.NUI.BaseComponents;

namespace Pley.Tv.Tizen.Screens;

/// <summary>
/// A library's page on TV (design spec §4.2; the Android app's TvLibraryScreen): the name at the
/// top left and the tabs on the same line at the right, then the tab. The Library tab: the chips
/// (Sort, Filter, Unwatched, Genre, Year, the resolution and dynamic range chosen, and the count) at
/// y = 80 over a grid of posters from y = 136, with the A to Z jump beside it; Recommended: the
/// library's rows; Collections: their posters with their counts. Sort and filters open in a side
/// panel. Content takes focus on its own only on the tab the page opened on.
/// </summary>
sealed class LibraryScreen : ShellPage
{
    const float HeaderHeight = 44, ChipsTop = 80, GridTop = 136, ChipHeight = 40, MessageWidth = 520;
    /// <summary>Skeleton tiles while nothing is known: three rows.</summary>
    const int Skeletons = 18;

    readonly Shell layers;
    readonly LibraryModel model;
    readonly Images images;
    readonly KeyRepeatGate gate;
    readonly Action<PageKey> go;
    readonly List<IDisposable> watches = new List<IDisposable>();

    readonly TextLabel name;
    readonly View tabsView = new View();
    readonly FocusGroup tabs = new FocusGroup(Axis.Row, "tabs");
    readonly View body = new View();
    readonly FocusGroup content = new FocusGroup(Axis.Column, "content");

    LibraryTab? shownTab;
    bool switched;
    IReadOnlyList<LibraryTab> shownTabs = Array.Empty<LibraryTab>();

    // The Library tab's parts.
    readonly View chipsView = new View();
    readonly FocusGroup chips = new FocusGroup(Axis.Row, "chips");
    IReadOnlyList<LibraryChip> shownChips = Array.Empty<LibraryChip>();
    TextLabel? count;
    /// <summary>The count's text as shown, and the room after the chips for it, in dp.</summary>
    string? shownCount;
    float countRoom;
    /// <summary>Each chip's left and width in the row, in dp, and how far the row slides left to keep the focused one in view.</summary>
    readonly List<(float Left, float Width)> chipPlaces = new List<(float, float)>();
    float chipScroll;
    Animation? chipAnimation;
    GridView? grid;
    JumpView? jump;
    string? shownQuery;
    GridCells? cells;
    View? message;
    FocusGroup? messageGroup;
    LibraryMessage? shownMessage;

    // The Recommended and Collections tabs' parts.
    object? shownRows;

    public LibraryScreen(TvShell shell, Shell layers, LibraryModel model, Images images, KeyRepeatGate gate, Action<PageKey> go)
        : base(shell, new PageKey.Library(model.LibraryId), "library")
    {
        this.layers = layers;
        this.model = model;
        this.images = images;
        this.gate = gate;
        this.go = go;
        View.Add(body);
        name = Text.Label("", PleyType.PageTitle, PleyColors.Fg, width: 480);
        name.Position = new Position(Ui.Dp(PleyTv.Gutter), Ui.Dp(PleyTv.SafeVertical + (HeaderHeight - PleyType.PageTitle.Line) / 2));
        View.Add(name);
        View.Add(tabsView);
        Group.Add(tabs);
        Group.Add(content);
        chips.ChildFocused += i =>
        {
            if (i >= 0 && i < chipPlaces.Count) ScrollChips(chipPlaces[i].Left, chipPlaces[i].Width);
        };
        tabs.Exit = d => d == Direction.Left && Scrolling.Left(gate, Shell.FocusRail);
        watches.Add(model.State.Watch(Show));
    }

    public override void Enter()
    {
        if (!Tree.Enter(content) && !Tree.Enter(tabs)) Shell.FocusRail();
    }

    public override void Gone()
    {
        foreach (var w in watches) w.Dispose();
        model.Dispose();
    }

    void Show(LibraryState s)
    {
        Text.Show(name, s.Library?.Name ?? "");
        var page = LibraryText.PageMessage(s);
        ShowTabs(page != null ? Array.Empty<LibraryTab>() : s.Tabs, s.Params.Tab);
        if (page != null)
        {
            shownTab = null;
            ClearBody();
            Message(page, 140, takeFocus: true);
            return;
        }
        if (shownTab != s.Params.Tab)
        {
            if (shownTab != null) switched = true;
            shownTab = s.Params.Tab;
            ClearBody();
        }
        switch (s.Params.Tab)
        {
            case LibraryTab.Library:
                ShowLibrary(s);
                break;
            case LibraryTab.Recommended:
                ShowRecommended(s);
                break;
            case LibraryTab.Collections:
                ShowCollections(s);
                break;
        }
    }

    /// <summary>Whether content may take focus on its own: on the tab the page opened on, before another was chosen.</summary>
    bool Initial(LibraryState s) => !switched && s.Params.Tab == s.OpenedOn;

    /// <summary>The tabs at the right safe edge, 8 dp apart: the current one on Surface3 in Fg, the others Muted; OK shows one.</summary>
    void ShowTabs(IReadOnlyList<LibraryTab> list, LibraryTab selected)
    {
        var focusedTab = Tree.Current?.Parent == tabs ? Tree.Current.Name : null;
        if (!list.SequenceEqual(shownTabs) || tabs.Children.Count == 0 && list.Count > 0)
        {
            shownTabs = list;
            foreach (var child in tabsView.Children.ToList())
            {
                tabsView.Remove(child);
                child.Dispose();
            }
            tabs.Clear();
            float x = 0;
            foreach (var tab in list)
            {
                var t = tab;
                var button = new ButtonView(LibraryText.TabLabel(tab), ButtonKind.Text, () => model.Select(t));
                button.PositionX = Ui.Dp(x);
                tabsView.Add(button);
                tabs.Add(button.Item);
                x += button.WidthDp + 8;
            }
            var width = Math.Max(0, x - 8);
            tabsView.Position = new Position(Ui.Dp(PleyTv.CanvasWidth - PleyTv.SafeHorizontal - width), Ui.Dp(PleyTv.SafeVertical + (HeaderHeight - ChipHeight) / 2));
            if (focusedTab != null && tabs.Children.OfType<FocusItem>().FirstOrDefault(i => i.Name == focusedTab) is { } again) Tree.Focus(again);
        }
        for (var i = 0; i < list.Count; i++)
        {
            if (tabsView.Children[i] is not ButtonView b) continue;
            var on = list[i] == selected;
            b.Rest = on ? PleyColors.Surface3 : Color.Transparent;
            b.Ink = on ? PleyColors.Fg : PleyColors.Muted;
            if (on) tabs.RememberChild(i);
        }
    }

    void ClearBody()
    {
        var hadFocus = Tree.Current != null && InContent(Tree.Current);
        // The chips' view stays for the next time the Library tab shows; what is in it goes.
        if (chipsView.GetParent() != null) body.Remove(chipsView);
        foreach (var child in chipsView.Children.ToList())
        {
            chipsView.Remove(child);
            child.Dispose();
        }
        foreach (var child in body.Children.ToList())
        {
            body.Remove(child);
            child.Dispose();
        }
        content.Clear();
        chips.Clear();
        messageGroup = null;
        shownChips = Array.Empty<LibraryChip>();
        count = null;
        grid = null;
        jump = null;
        shownQuery = null;
        cells = null;
        message = null;
        shownMessage = null;
        shownRows = null;
        // Focus was in what went: the selected tab has it now.
        if (hadFocus && !Tree.Enter(tabs)) Shell.FocusRail();
    }

    bool InContent(FocusNode node)
    {
        for (var n = node.Parent; n != null; n = n.Parent)
            if (n == content) return true;
        return false;
    }

    // ---- the Library tab

    void ShowLibrary(LibraryState s)
    {
        if (chipsView.GetParent() == null)
        {
            chipScroll = 0;
            chipAnimation?.Stop();
            chipAnimation = null;
            chipsView.Position = new Position(Ui.Dp(PleyTv.Gutter), Ui.Dp(ChipsTop));
            body.Add(chipsView);
            content.Add(chips);
        }
        ShowChips(s);

        var gridMessage = LibraryText.GridMessage(s);
        if (gridMessage != null)
        {
            RemoveGrid();
            if (gridMessage != shownMessage)
            {
                RemoveMessage();
                Message(gridMessage, GridTop - GridView.TileRoom + 24, takeFocus: Initial(s));
            }
            return;
        }
        RemoveMessage();

        var grid0 = s.Grid;
        cells = grid0.Cells.Value;
        if (grid == null || grid.Columns != grid0.Columns)
        {
            RemoveGrid();
            var made = new GridView(grid0.Kind, grid0.Columns, PleyTv.CanvasHeight - (GridTop - GridView.TileRoom), gate, images, p => cells?[p]);
            made.PositionY = Ui.Dp(GridTop - GridView.TileRoom);
            made.Open = (p, _) =>
            {
                var id = cells?.IdAt(p) ?? -1;
                if (id >= 0) go(model.State.Value.Opens(id));
            };
            made.Actions = (p, _) =>
            {
                var id = cells?.IdAt(p) ?? -1;
                if (id >= 0) Shell.Actions?.Invoke(id, false);
            };
            made.Showing += model.Showing;
            made.Group.Exit = d => d switch
            {
                Direction.Left => Scrolling.Left(gate, Shell.FocusRail),
                // A held key stays in the grid: the A to Z jump (Right) and the chips (Up) take a press of their own.
                Direction.Right or Direction.Up => gate.Holding,
                // The grid's last row: Down stays.
                Direction.Down => true,
                _ => false,
            };
            body.Add(made);
            grid = made;
            var row = new FocusGroup(Axis.Row, "grid+jump");
            row.Add(made.Group);
            content.Add(row);
            content.RememberChild(content.Children.Count - 1);
            shownQuery = s.Params.Query().ToString();
        }
        var hadFocus = Tree.Current != null && Tree.Current.Parent == grid.Group;
        var before = grid.Current;
        grid.Update(cells?.Total ?? Skeletons);
        // A new sort or filter starts at the top.
        var query = s.Params.Query().ToString();
        if (query != shownQuery)
        {
            shownQuery = query;
            grid.ScrollToTop(Tree);
        }
        else if (hadFocus && Tree.Current?.Parent != grid.Group)
        {
            // Its positions were made again (the total changed): focus goes back where it was.
            var at = Math.Max(0, Math.Min(before, grid.Total - 1));
            if (grid.ItemAt(at) is { } item)
            {
                grid.Jump(at);
                Tree.Focus(item);
            }
        }
        ShowJump(grid0.Index);
        // Opened on this tab: the first tile has focus, once there is one.
        if (Initial(s) && Tree.Current == null && grid.ItemAt(0) is { } first) Tree.Focus(first);
    }

    /// <summary>The grid and its jump go (a message in their place, or a grid of another shape).</summary>
    void RemoveGrid()
    {
        if (grid == null) return;
        if (grid.Group.Parent is FocusGroup row && row != content) content.Remove(row);
        else content.Remove(grid.Group);
        grid.Unparent();
        grid.Dispose();
        grid = null;
        if (jump != null)
        {
            jump.Unparent();
            jump.Dispose();
            jump = null;
        }
    }

    void RemoveMessage()
    {
        if (message == null) return;
        message.Unparent();
        message.Dispose();
        message = null;
        shownMessage = null;
        if (messageGroup != null) content.Remove(messageGroup);
        messageGroup = null;
    }

    /// <summary>The chips at y = 80, 10 dp apart: 40 dp pills on Surface3, a selected one with an accent check; then the count.</summary>
    void ShowChips(LibraryState s)
    {
        var list = LibraryText.Chips(s);
        var line = LibraryText.CountLine(s);
        if (list.SequenceEqual(shownChips))
        {
            ShowCount(line);
            return;
        }
        // Focus stays on the chip of the same kind, by kind and not by its label: a sort chosen
        // changes the Sort chip's label, and focus was left on the chip that went, with nowhere to go.
        var focusedAt = Tree.Current?.Parent == chips ? chips.Children.ToList().IndexOf(Tree.Current) : -1;
        LibraryChipKind? focusedKind = focusedAt >= 0 && focusedAt < shownChips.Count ? shownChips[focusedAt].Kind : null;
        shownChips = list;
        foreach (var child in chipsView.Children.ToList())
        {
            chipsView.Remove(child);
            child.Dispose();
        }
        chips.Clear();
        chipPlaces.Clear();
        float x = 0;
        FocusItem? again = null;
        foreach (var chip in list)
        {
            var c = chip;
            var button = new ButtonView(chip.Label, ButtonKind.Text, () => Chip(c), iconName: chip.Selected ? "check" : chip.Icon)
            {
                Rest = PleyColors.Surface3,
            };
            if (chip.Selected) button.IconInk = PleyColors.Accent;
            button.PositionX = Ui.Dp(x);
            chipsView.Add(button);
            chips.Add(button.Item);
            chipPlaces.Add((x, button.WidthDp));
            if (again == null && chip.Kind == focusedKind) again = button.Item;
            x += button.WidthDp + 10;
        }
        // Its kind gone: the chip in its place.
        if (again == null && focusedAt >= 0 && chips.Children.Count > 0) again = chips.Children[Math.Min(focusedAt, chips.Children.Count - 1)] as FocusItem;
        // The count gives way to the chips rather than run past the row's end (ShowCount).
        var countX = x - 10 + 8;
        countRoom = Math.Min(300, ChipsRoom - countX);
        count = Text.Label("", PleyType.Caption, PleyColors.Muted, width: Math.Max(countRoom, 1));
        count.Position = new Position(Ui.Dp(countX), Ui.Dp((ChipHeight - PleyType.Caption.Line) / 2));
        shownCount = null;
        ShowCount(line);
        chipsView.Add(count);
        // Fewer chips (a filter cleared): the row slides no further than they need.
        var most = Math.Max(0, x - 10 - ChipsRoom);
        if (chipScroll > most) SlideChips(most);
        if (again != null) Tree.Focus(again);
    }

    /// <summary>The chips' room: from the gutter to the right safe edge.</summary>
    static float ChipsRoom => PleyTv.CanvasWidth - PleyTv.Gutter - PleyTv.SafeHorizontal;

    /// <summary>
    /// Keeps the focused chip between the gutter and the right safe edge, as Search's recent searches
    /// do: the row is wider than that only with many filters chosen.
    /// </summary>
    void ScrollChips(float left, float width)
    {
        var target = chipScroll;
        if (left < target) target = left;
        else if (left + width > target + ChipsRoom) target = left + width - ChipsRoom;
        if (target != chipScroll) SlideChips(target);
    }

    void SlideChips(float target)
    {
        chipScroll = target;
        chipAnimation?.Stop();
        chipAnimation = Scrolling.Glide(gate);
        chipAnimation.AnimateTo(chipsView, "PositionX", Ui.Dp(PleyTv.Gutter - chipScroll));
        chipAnimation.Play();
    }

    /// <summary>The count after the chips, whole or not at all: with many filters chosen, cut short ("23 of 3…") it says nothing.</summary>
    void ShowCount(string? line)
    {
        // While the listing isn't known, the count shown stays.
        if (count == null || line == null || line == shownCount) return;
        shownCount = line;
        count.Text = line;
        if (Text.Width(count) <= countRoom) count.Show();
        else count.Hide();
    }

    void Chip(LibraryChip chip)
    {
        var panel = LibraryText.PanelOf(chip.Kind);
        if (panel != null) OpenPanel(panel.Value, 0);
        else if (chip.Kind == LibraryChipKind.Unwatched) model.ToggleUnwatched();
    }

    /// <summary>
    /// The chip's side panel: a choice applies at once; a sort, order, genre or year closes it, a
    /// filter row stays (shown again with its check moved), a row with a chevron opens its list.
    /// </summary>
    void OpenPanel(LibraryPanel panel, int? focus)
    {
        var view = LibraryText.Panel(panel, model.State.Value);
        if (focus != null && focus.Value > 0) view = view with { Focus = focus.Value };
        SidePanelSheet? sheet = null;
        sheet = new SidePanelSheet(layers, view, row =>
        {
            var at = view.Rows.ToList().IndexOf(row);
            switch (model.Choose(row))
            {
                case PanelOutcome.Stay:
                    // Shown again over itself, so its check follows the choice.
                    sheet!.Close();
                    OpenPanel(panel, at);
                    return true;
                case PanelOutcome.Show show:
                    sheet!.Close();
                    OpenPanel(show.Panel, null);
                    return true;
                case PanelOutcome.Open open:
                    go(open.Key);
                    return false;
                default:
                    return false;
            }
        });
        sheet.Show();
    }

    void ShowJump(AlphaIndex? index)
    {
        if (grid == null) return;
        if (index == null)
        {
            if (jump != null)
            {
                jump.Item.Parent?.Remove(jump.Item);
                jump.Dispose();
                jump = null;
            }
            return;
        }
        if (jump == null)
        {
            var g = grid;
            jump = new JumpView(index, () => g.Current, target =>
            {
                g.Jump(target);
                if (g.ItemAt(target) is { } item) Tree.Focus(item);
            });
            jump.Position = new Position(Ui.Dp(PleyTv.CanvasWidth - PleyTv.SafeHorizontal - JumpView.Diameter), Ui.Dp(GridTop - JumpView.Room));
            body.Add(jump);
            (g.Group.Parent as FocusGroup)?.Add(jump.Item);
            g.Moved += _ => jump?.Draw();
        }
        else jump.Index = index;
    }

    // ---- the Recommended tab

    void ShowRecommended(LibraryState s)
    {
        var msg = LibraryText.RecommendedMessage(s);
        if (msg != null)
        {
            if (msg != shownMessage)
            {
                ClearRows();
                Message(msg, 80, takeFocus: Initial(s));
            }
            return;
        }
        var rows = s.Recommended.Value;
        if (rows == null || ReferenceEquals(rows, shownRows)) return;
        ClearRows();
        shownRows = rows;
        var column = new RowColumnView(gate, top: 80, headerY: 80);
        foreach (var row in rows)
        {
            var r = row;
            column.Add(new RowView(row.Title, row.Tiles, row.Kind, gate, images,
                tile =>
                {
                    var at = IndexOf(r.Tiles, tile);
                    if (at >= 0 && at < r.Opens.Count) go(r.Opens[at]);
                },
                r.IsCollections ? null : tile => Shell.Actions?.Invoke(tile.Id, r.ContinueWatching),
                Shell.FocusRail, focusItem: Tree.Focus,
                seeAll: r.IsCollections ? (LibraryText.SeeAll, () => model.Select(LibraryTab.Collections))
                    : r.SeeAll is { } view ? (LibraryText.SeeAll, () => model.SeeAll(view)) : null));
        }
        body.Add(column);
        content.Add(column.Group);
        if (Initial(s) && Tree.Current == null) Tree.Enter(column.Group);
    }

    static int IndexOf(IReadOnlyList<TileModel> tiles, TileModel tile)
    {
        for (var i = 0; i < tiles.Count; i++)
            if (ReferenceEquals(tiles[i], tile)) return i;
        return -1;
    }

    void ClearRows()
    {
        var hadFocus = Tree.Current != null && InContent(Tree.Current);
        foreach (var child in body.Children.ToList())
        {
            body.Remove(child);
            child.Dispose();
        }
        content.Clear();
        message = null;
        messageGroup = null;
        shownMessage = null;
        shownRows = null;
        grid = null;
        if (hadFocus && !Tree.Enter(tabs)) Shell.FocusRail();
    }

    // ---- the Collections tab

    void ShowCollections(LibraryState s)
    {
        var msg = LibraryText.CollectionsMessage(s);
        if (msg != null)
        {
            if (msg != shownMessage)
            {
                ClearRows();
                Message(msg, GridTop - GridView.TileRoom + 24, takeFocus: Initial(s));
            }
            return;
        }
        var list = s.Collections.Value;
        if (ReferenceEquals(list, shownRows) && list != null) return;
        ClearRows();
        shownRows = list;
        var line = LibraryText.CollectionsCountLine(s);
        if (line != null)
        {
            var label = Text.Label(line, PleyType.Caption, PleyColors.Muted, width: 300);
            label.Position = new Position(Ui.Dp(PleyTv.Gutter), Ui.Dp(ChipsTop + (ChipHeight - PleyType.Caption.Line) / 2));
            body.Add(label);
        }
        var made = new GridView(TileKind.LibraryGrid, LibraryRules.GridColumns(TileKind.LibraryGrid), PleyTv.CanvasHeight - (GridTop - GridView.TileRoom), gate, images,
            p => list != null && p < list.Count ? list[p].Tile : null,
            p => list != null && p < list.Count ? list[p].Count.ToString(System.Globalization.CultureInfo.InvariantCulture) : null);
        made.PositionY = Ui.Dp(GridTop - GridView.TileRoom);
        made.Open = (p, _) =>
        {
            if (list != null && p < list.Count) go(new PageKey.Collection(list[p].Id));
        };
        made.Group.Exit = d => d switch
        {
            Direction.Left => Scrolling.Left(gate, Shell.FocusRail),
            Direction.Down or Direction.Right => true,
            // A held Up stays in the grid: the tabs take a press of their own.
            Direction.Up => gate.Holding,
            _ => false,
        };
        body.Add(made);
        content.Add(made.Group);
        made.Update(list?.Count ?? Skeletons);
        grid = made;
        if (Initial(s) && Tree.Current == null && made.ItemAt(0) is { } first) Tree.Focus(first);
    }

    // ---- messages

    /// <summary>
    /// Nothing to show (design spec §8): the title in Section, the sentence in Body Muted, and the
    /// one thing to do (Retry, Clear filters) as a pill with focus. Without one, focus goes to the
    /// selected tab, else to the rail, so the remote is never dead.
    /// </summary>
    void Message(LibraryMessage m, float top, bool takeFocus)
    {
        shownMessage = m;
        var view = new View { Position = new Position(Ui.Dp(PleyTv.Gutter), Ui.Dp(top)) };
        var title = Text.Label(m.Title, PleyType.Section, PleyColors.Fg, width: PleyTv.CanvasWidth - 2 * PleyTv.Gutter);
        view.Add(title);
        var words = Text.Label(m.Body, PleyType.Body, PleyColors.Muted, maxLines: 3, width: MessageWidth);
        words.VerticalAlignment = VerticalAlignment.Top;
        words.PositionY = Ui.Dp(PleyType.Section.Line + 8);
        view.Add(words);
        body.Add(view);
        message = view;
        if (m.ActionLabel is { } action)
        {
            var buttons = new ButtonRow();
            buttons.Add(new ButtonView(action, ButtonKind.Pill, () =>
            {
                if (m.Action == MessageAction.ClearFilters) model.ClearFilters();
                else model.Retry();
            }, iconName: m.Action == MessageAction.Retry ? "refresh-cw" : null));
            buttons.PositionY = Ui.Dp(PleyType.Section.Line + 8 + PleyType.Body.Line * 2 + 8 + 16);
            view.Add(buttons);
            content.Add(buttons.Group);
            messageGroup = buttons.Group;
            if (Tree.Current == null || InContent(Tree.Current) || takeFocus) Tree.Enter(buttons.Group);
        }
        else if (takeFocus && (Tree.Current == null || InContent(Tree.Current)))
        {
            if (!Tree.Enter(tabs)) Shell.FocusRail();
        }
    }

    /// <summary>
    /// The A to Z jump (design spec §4.2): a 40 dp circle beside the grid showing the letter of the
    /// focused title's section, in Title on Surface3. Focused (the accent fill, 1.08), Up and Down
    /// step through #, A to Z, the letters before and after showing above and below it, dimmed where
    /// no title starts with them; OK moves focus to the first title of the letter. Up from "#"
    /// leaves upwards.
    /// </summary>
    sealed class JumpView : View
    {
        public const float Diameter = 40, Room = 20;

        readonly AlphaJump jump;
        readonly Func<int> focused;
        readonly View circle;
        readonly TextLabel letter;
        readonly TextLabel above;
        readonly TextLabel below;

        public JumpView(AlphaIndex index, Func<int> focused, Action<int> onJump)
        {
            jump = new AlphaJump(index);
            this.focused = focused;
            Size = new global::Tizen.NUI.Size(Ui.Dp(Diameter), Ui.Dp(Diameter + 2 * Room));
            above = Neighbour(0);
            circle = new View
            {
                Size = new global::Tizen.NUI.Size(Ui.Dp(Diameter), Ui.Dp(Diameter)),
                Position = new Position(0, Ui.Dp(Room)),
                CornerRadius = Ui.Dp(PleyShape.ControlRadius),
                PivotPoint = global::Tizen.NUI.PivotPoint.Center,
                PositionUsesPivotPoint = false,
            };
            letter = Text.Label("", PleyType.Title, PleyColors.Fg, width: Diameter);
            letter.HorizontalAlignment = HorizontalAlignment.Center;
            letter.PositionY = Ui.Dp((Diameter - PleyType.Title.Line) / 2);
            circle.Add(letter);
            Add(circle);
            below = Neighbour(Room + Diameter);
            Item = new FocusItem("jump")
            {
                Selected = () => onJump(jump.Target),
                OwnKey = (key, down) =>
                {
                    if (key == "Up")
                    {
                        if (!down) return true;
                        if (!jump.Up()) return false;
                        Draw();
                        return true;
                    }
                    if (key == "Down")
                    {
                        if (down)
                        {
                            jump.Down();
                            Draw();
                        }
                        return true;
                    }
                    return false;
                },
            };
            Item.FocusChanged += on =>
            {
                if (Disposed) return;
                if (on) jump.Focus(focused());
                else jump.Blur();
                Draw();
            };
            Draw();
        }

        public FocusItem Item { get; }

        public AlphaIndex Index
        {
            set
            {
                jump.Index = value;
                Draw();
            }
        }

        TextLabel Neighbour(float top)
        {
            var label = Text.Label("", PleyType.Caption, PleyColors.Muted, width: Diameter);
            label.HorizontalAlignment = HorizontalAlignment.Center;
            label.PositionY = Ui.Dp(top + (Room - PleyType.Caption.Line) / 2);
            Add(label);
            return label;
        }

        public void Draw()
        {
            if (Disposed) return;
            var on = jump.Focused;
            letter.Text = AlphaIndex.Letters[jump.Shown(focused())];
            letter.TextColor = on ? PleyColors.OnAccent : PleyColors.Fg;
            circle.BackgroundColor = on ? PleyColors.Accent : PleyColors.Surface3;
            var scale = on ? PleyShape.RoundScale : 1f;
            circle.Scale = new Vector3(scale, scale, 1);
            Show(above, jump.Above);
            Show(below, jump.Below);
        }

        void Show(TextLabel label, int? at)
        {
            if (at is not int l)
            {
                label.Text = "";
                return;
            }
            label.Text = AlphaIndex.Letters[l];
            label.TextColor = jump.Index.Has(l) ? PleyColors.Muted : new Color(PleyColors.Disabled.R, PleyColors.Disabled.G, PleyColors.Disabled.B, 0.6f);
        }
    }
}
