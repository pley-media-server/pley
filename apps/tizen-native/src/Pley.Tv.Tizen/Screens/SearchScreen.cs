using System;
using System.Collections.Generic;
using System.Linq;
using Pley.Tv.Core.Ui;
using Pley.Tv.Tizen.Kit;
using Tizen.NUI;
using Tizen.NUI.BaseComponents;

namespace Pley.Tv.Tizen.Screens;

/// <summary>
/// Search on TV (design spec §4.5; the Android app's TvSearchScreen): the field at the top, 520
/// wide, with initial focus; OK opens the TV's keyboard, and Back or its Search key closes it.
/// Before typing, the recent searches as chips. While typing (250 ms after the last key), the
/// results as rows, grouped (People, Movies, Shows, Seasons, Episodes, Artists, Albums, Tracks),
/// the first header at y = 96. Down from the field goes into the rows, the chips or Retry, Up from
/// the first row back to it; Left goes to the rail.
/// </summary>
sealed class SearchScreen : ShellPage
{
    const float FieldWidth = 520, ResultsTop = 96, NoticeTop = 120, NoticeWidth = 560, ChipsGap = 12, ChipMaxWidth = 320;
    /// <summary>The chips' row sits 16 dp under its header, plus the room Android's row keeps above them for their growth.</summary>
    const float ChipsTop = ResultsTop + PleyTv.RowHeaderHeight + 16 + 6;
    const int SkeletonRows = 2, SkeletonTiles = 7;

    readonly SearchModel model;
    readonly Images images;
    readonly KeyRepeatGate gate;
    readonly Action<PageKey> go;
    readonly List<IDisposable> watches = new List<IDisposable>();
    readonly View body = new View { WidthResizePolicy = ResizePolicyType.FillToParent, HeightResizePolicy = ResizePolicyType.FillToParent };
    readonly TextInput field;
    readonly TextLabel searching;
    /// <summary>What is under the field: the rows, the chips or Retry.</summary>
    readonly FocusGroup below = new FocusGroup(Axis.Column, "below");

    /// <summary>What the body shows now: the groups, the recent searches, a message, or "skeleton".</summary>
    object? shown;
    View? chipStrip;
    float chipScroll;
    Animation? chipAnimation;

    /// <param name="layers">The app's sheets: the field's keyboard takes the remote's keys through their input.</param>
    /// <param name="go">Opens another page: an item, a person, an album.</param>
    public SearchScreen(TvShell shell, Shell layers, SearchModel model, Images images, KeyRepeatGate gate, Action<PageKey> go)
        : base(shell, new PageKey.Search(model.State.Value.Query), "search")
    {
        this.model = model;
        this.images = images;
        this.gate = gate;
        this.go = go;
        View.BackgroundColor = PleyColors.Bg;
        // The rows scroll up under the field, which is drawn over them.
        View.Add(body);
        field = new TextInput(layers.Input, FieldWidth, SearchText.Field, iconName: "search")
        {
            Position = new Position(Ui.Dp(PleyTv.Gutter), Ui.Dp(PleyTv.SafeVertical)),
        };
        field.Value = model.State.Value.Query;
        field.Changed += model.Type;
        field.Submitted = model.Submit;
        View.Add(field);
        searching = Text.Label(SearchText.Searching, PleyType.Caption, PleyColors.Muted, width: 200);
        searching.Position = new Position(Ui.Dp(PleyTv.Gutter + FieldWidth + 20), Ui.Dp(PleyTv.SafeVertical + (TextInput.Height - PleyType.Caption.Line) / 2));
        searching.Hide();
        View.Add(searching);

        Group.Add(field.Item);
        Group.Add(below);
        // Left from the field, a chip's or Retry's start goes to the rail; Right stops at their end.
        Group.Exit = d => d switch
        {
            Direction.Left => Scrolling.Left(gate, Shell.FocusRail),
            Direction.Right => true,
            _ => false,
        };
        watches.Add(model.State.Watch(Show));
    }

    public override void Enter()
    {
        if (!Tree.Enter(Group)) Shell.FocusRail();
    }

    public override void Gone()
    {
        foreach (var w in watches) w.Dispose();
        field.Changed -= model.Type;
        chipAnimation?.Stop();
        model.Dispose();
    }

    void Show(SearchState s)
    {
        if (s.Searching) searching.Show();
        else searching.Hide();

        if (SearchText.Message(s) is { } message)
        {
            if (!Equals(message, shown)) Notice(message);
            return;
        }
        switch (s.Results)
        {
            case SearchResultsState.Idle:
                if (!ReferenceEquals(s.Recent, shown)) Recent(s.Recent);
                break;
            case SearchResultsState.Loading:
                if (!Equals(shown, "skeleton")) Skeletons();
                break;
            case SearchResultsState.Ready ready:
                if (!ReferenceEquals(ready.Groups, shown)) Rows(ready.Groups);
                break;
        }
    }

    /// <summary>What was under the field goes; focus that was in it goes back to the field.</summary>
    void ClearBody(object? next)
    {
        var hadFocus = Tree.Current != null && Below(Tree.Current);
        chipAnimation?.Stop();
        chipAnimation = null;
        chipStrip = null;
        chipScroll = 0;
        below.Clear();
        foreach (var child in body.Children.ToList())
        {
            body.Remove(child);
            child.Dispose();
        }
        shown = next;
        if (hadFocus) Tree.Focus(field.Item);
    }

    bool Below(FocusNode node)
    {
        for (var n = node.Parent; n != null; n = n.Parent)
            if (n == below) return true;
        return false;
    }

    /// <summary>The results' rows: people as circles, episodes as stills, music as squares, the rest posters.</summary>
    void Rows(IReadOnlyList<SearchGroup> groups)
    {
        ClearBody(groups);
        var column = new RowColumnView(gate, top: ResultsTop, headerY: ResultsTop);
        foreach (var group in groups)
        {
            var g = group;
            column.Add(new RowView(g.Title, g.Tiles, g.TileKind, gate, images,
                tile =>
                {
                    if (model.Open(g.Kind, tile.Id) is { } key) go(key);
                },
                g.Kind == SearchGroupKind.People ? null : tile => Shell.Actions?.Invoke(tile.Id, false),
                Shell.FocusRail, focusItem: Tree.Focus));
        }
        body.Add(column);
        below.Add(column.Group);
    }

    /// <summary>
    /// Before typing: "Recent searches" at y = 96 and a chip for each (a history icon, at most 320
    /// wide), 12 apart, then Clear. Choosing one searches it again; focus goes back to the field,
    /// since the chips go once the results come.
    /// </summary>
    void Recent(IReadOnlyList<string> recent)
    {
        ClearBody(recent);
        var header = Text.Label(SearchText.Recent, PleyType.Section, PleyColors.Fg, width: PleyTv.CanvasWidth - PleyTv.Gutter - PleyTv.SafeHorizontal);
        header.Position = new Position(Ui.Dp(PleyTv.Gutter), Ui.Dp(ResultsTop));
        body.Add(header);
        var strip = new View { Position = new Position(Ui.Dp(PleyTv.Gutter), Ui.Dp(ChipsTop)) };
        var chips = new FocusGroup(Axis.Row, "recent");
        var lefts = new List<float>();
        var widths = new List<float>();
        float x = 0;
        void Place(ButtonView button)
        {
            button.PositionX = Ui.Dp(x);
            strip.Add(button);
            chips.Add(button.Item);
            lefts.Add(x);
            widths.Add(button.WidthDp);
            x += button.WidthDp + ChipsGap;
        }
        foreach (var query in recent)
        {
            var q = query;
            Place(new ButtonView(q, ButtonKind.Text, () => SearchAgain(q), iconName: "history", maxWidth: ChipMaxWidth) { Rest = PleyColors.Surface3 });
        }
        Place(new ButtonView(SearchText.RecentClear, ButtonKind.Text, () =>
        {
            Tree.Focus(field.Item);
            model.ClearRecent();
        }));
        chips.ChildFocused += i => ScrollChips(lefts[i], widths[i]);
        body.Add(strip);
        chipStrip = strip;
        below.Add(chips);
    }

    void SearchAgain(string query)
    {
        Tree.Focus(field.Item);
        field.Value = query;
        model.SearchAgain(query);
    }

    /// <summary>Keeps the focused chip between the gutter and the right safe edge.</summary>
    void ScrollChips(float left, float width)
    {
        if (chipStrip == null) return;
        var room = PleyTv.CanvasWidth - PleyTv.Gutter - PleyTv.SafeHorizontal;
        if (left < chipScroll) chipScroll = left;
        else if (left + width > chipScroll + room) chipScroll = left + width - room;
        chipAnimation?.Stop();
        chipAnimation = Scrolling.Glide(gate);
        chipAnimation.AnimateTo(chipStrip, "PositionX", Ui.Dp(PleyTv.Gutter - chipScroll));
        chipAnimation.Play();
    }

    /// <summary>
    /// "No results for “north”", "Search failed", or what search finds before anything is typed:
    /// at the gutter, y = 120, at most 560 wide, the title in Title over the sentence in Body
    /// Muted. Retry sits 12 dp under them; focus stays on the field until Down.
    /// </summary>
    void Notice(LibraryMessage message)
    {
        ClearBody(message);
        var view = new View { Position = new Position(Ui.Dp(PleyTv.Gutter), Ui.Dp(NoticeTop)) };
        var title = Text.Label(message.Title, PleyType.Title, PleyColors.Fg, width: NoticeWidth);
        view.Add(title);
        var words = Text.Label(message.Body, PleyType.Body, PleyColors.Muted, maxLines: 4, width: NoticeWidth);
        words.VerticalAlignment = VerticalAlignment.Top;
        words.SizeHeight = Math.Min(words.GetHeightForWidth(Ui.Dp(NoticeWidth)), Text.LineHeight(PleyType.Body) * 4);
        words.PositionY = Ui.Dp(PleyType.Title.Line + 8);
        view.Add(words);
        if (message.ActionLabel is { } action)
        {
            var buttons = new ButtonRow();
            buttons.Add(new ButtonView(action, ButtonKind.Pill, model.Retry, iconName: "rotate-ccw"));
            buttons.PositionY = words.PositionY + words.SizeHeight + Ui.Dp(8 + 12 + 8);
            view.Add(buttons);
            below.Add(buttons.Group);
        }
        body.Add(view);
    }

    /// <summary>Two rows of still poster skeletons while the first answer comes (no shimmer on TV).</summary>
    void Skeletons()
    {
        ClearBody("skeleton");
        var tile = TileKind.Poster;
        var y = ResultsTop;
        for (var r = 0; r < SkeletonRows; r++)
        {
            Block(PleyTv.Gutter, y, 140, 22, PleyShape.BadgeRadius);
            y += 22 + PleyTv.HeaderToTiles;
            for (var i = 0; i < SkeletonTiles; i++) Block(PleyTv.Gutter + i * (tile.Width + PleyTv.TileGap), y, tile.Width, tile.Height, PleyShape.TileRadius);
            y += tile.Height + PleyTv.RowGap;
        }
    }

    void Block(float x, float y, float w, float h, float radius) =>
        body.Add(new View { Position = new Position(Ui.Dp(x), Ui.Dp(y)), Size = new Size(Ui.Dp(w), Ui.Dp(h)), CornerRadius = Ui.Dp(radius), BackgroundColor = PleyColors.Surface2 });
}
