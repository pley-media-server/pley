using System;
using System.Collections.Generic;
using System.Linq;
using Pley.Tv.Core;
using Pley.Tv.Core.Api;
using Pley.Tv.Core.Ui;
using Pley.Tv.Tizen.Kit;
using Tizen.NUI;
using Tizen.NUI.BaseComponents;

namespace Pley.Tv.Tizen.Screens;

/// <summary>
/// Watch history on TV (the Android app's TvHistoryScreen): the profile's plays, newest first, one
/// row each with its picture, title, what it is, when and on which device. OK opens it; the cross
/// at the row's end (Right) removes it at once, as does holding OK, and focus goes to the play
/// below it (or above, for the last). Show more loads the next 15. Focus starts on the first play.
/// </summary>
sealed class HistoryScreen : ShellPage
{
    const float ListTop = PleyTv.SafeVertical + 34 + 20;
    const float RowWidth = 700, RowHeight = 64, RowSpacing = 4, Pitch = RowHeight + RowSpacing;
    const float CrossSize = 40, CrossGap = 12, MoreGap = 12;
    /// <summary>Room under the list's end, so its last row needn't sit at the screen's edge.</summary>
    const float BottomPadding = 120;
    /// <summary>Where a focused row's top settles, as a share of the list's height (the TV's pivot for lists).</summary>
    const float Pivot = 0.3f;
    /// <summary>Rows made either side of those on screen, so a move never shows a gap.</summary>
    const int RowsNear = 3;
    const float Viewport = PleyTv.CanvasHeight - ListTop;

    readonly HistoryModel model;
    readonly Images images;
    readonly KeyRepeatGate gate;
    readonly Action<PageKey> go;
    readonly List<IDisposable> watches = new List<IDisposable>();
    readonly TextLabel count;
    readonly TextLabel problem;
    readonly View body = new View { WidthResizePolicy = ResizePolicyType.FillToParent, HeightResizePolicy = ResizePolicyType.FillToParent };
    readonly View list;
    readonly View content = new View();
    readonly FocusGroup lines = new FocusGroup(Axis.Grid, "plays") { Columns = 2 };
    readonly Dictionary<long, (FocusItem Line, FocusItem Cross)> items = new Dictionary<long, (FocusItem, FocusItem)>();
    readonly Dictionary<long, PlayView> views = new Dictionary<long, PlayView>();
    IReadOnlyList<HistoryRow> rows = Array.Empty<HistoryRow>();
    ButtonRow? moreRow;
    ButtonView? more;
    string shownKind = "";
    float scrollY;
    Animation? scroll;
    bool heldHere, gone;
    FocusItem? waitingOn;
    bool waiting;

    public HistoryScreen(TvShell shell, HistoryModel model, Images images, KeyRepeatGate gate, Action<PageKey> go)
        : base(shell, new PageKey.History(), "history")
    {
        this.model = model;
        this.images = images;
        this.gate = gate;
        this.go = go;
        var title = Text.Label(HistoryText.Title, PleyType.PageTitle, PleyColors.Fg);
        title.Size = new global::Tizen.NUI.Size(title.NaturalSize.Width + 2, Ui.Dp(PleyType.PageTitle.Line));
        title.Position = new Position(Ui.Dp(PleyTv.Gutter), Ui.Dp(PleyTv.SafeVertical));
        View.Add(title);
        // The count and the error sit on the title's baseline side: their bottoms 6 dp over its bottom, and on it.
        count = Text.Label("", PleyType.Caption, PleyColors.Muted, width: 200);
        count.Position = new Position(Ui.Dp(PleyTv.Gutter + 16) + title.SizeWidth, Ui.Dp(PleyTv.SafeVertical + PleyType.PageTitle.Line - 6 - PleyType.Caption.Line));
        View.Add(count);
        const float problemWidth = 400;
        problem = Text.Label("", PleyType.Label, PleyColors.DangerText, width: problemWidth);
        problem.HorizontalAlignment = HorizontalAlignment.End;
        problem.Position = new Position(Ui.Dp(PleyTv.CanvasWidth - PleyTv.SafeHorizontal - problemWidth), Ui.Dp(PleyTv.SafeVertical + PleyType.PageTitle.Line - PleyType.Label.Line));
        View.Add(problem);
        list = new View
        {
            Position = new Position(0, Ui.Dp(ListTop)),
            Size = new global::Tizen.NUI.Size(Ui.Dp(PleyTv.CanvasWidth), Ui.Dp(Viewport)),
            ClippingMode = ClippingModeType.ClipChildren,
        };
        list.Add(content);
        View.Add(list);
        View.Add(body);
        lines.Exit = d => d switch
        {
            Direction.Left => Scrolling.Left(gate, Shell.FocusRail),
            Direction.Right => true,
            _ => false,
        };
        Group.ChildFocused += _ => FollowFocus();
        gate.HoldEnded += Landed;
        model.Failed += OnFailed;
        watches.Add(model.State.Watch(Show));
    }

    public override void Enter()
    {
        if (!Tree.Enter(Group)) ToRail();
    }

    /// <summary>
    /// Nothing on the page can take focus yet: the rail has it (its entry, when the rail opened the
    /// page), and the page takes it back once it can, unless someone has moved on meanwhile.
    /// </summary>
    void ToRail()
    {
        if (Tree.Current == null || Tree.Current.Parent != Shell.Rail.Group) Shell.FocusRail();
        waiting = true;
        waitingOn = Tree.Current;
    }

    public override void Gone()
    {
        gone = true;
        foreach (var w in watches) w.Dispose();
        gate.HoldEnded -= Landed;
        model.Failed -= OnFailed;
        scroll?.Stop();
        model.Dispose();
    }

    void OnFailed(Exception e) => Text.Show(problem, Errors.Message(e));

    /// <summary>Whether the page may take focus on its own: it has none, or it is still where the page opened.</summary>
    bool TakesFocus()
    {
        var take = Tree.Current == null || (waiting && Tree.Current == waitingOn);
        waiting = false;
        return take;
    }

    void Show(HistoryState state)
    {
        Text.Show(count, state is HistoryState.Ready r ? HistoryText.Count(r.Total) : "");
        if (HistoryText.Message(state) is { } message)
        {
            var kind = "message:" + message.Title + message.Action;
            if (kind == shownKind) return;
            shownKind = kind;
            Clear();
            Message(message);
            return;
        }
        switch (state)
        {
            case HistoryState.Loading:
                if (shownKind == "loading") return;
                shownKind = "loading";
                Clear();
                Skeleton();
                return;
            case HistoryState.Ready ready:
                if (shownKind != "rows") Clear();
                shownKind = "rows";
                ShowRows(ready);
                return;
        }
    }

    /// <summary>Takes the page's content away: the loading bars, a message, the plays.</summary>
    void Clear()
    {
        var hadFocus = Tree.Current != null && Within(Group, Tree.Current);
        Group.Clear();
        lines.Clear();
        items.Clear();
        foreach (var view in views.Values) view.Dispose();
        views.Clear();
        rows = Array.Empty<HistoryRow>();
        foreach (var child in body.Children.ToList())
        {
            body.Remove(child);
            child.Dispose();
        }
        moreRow?.Dispose();
        moreRow = null;
        more = null;
        scroll?.Stop();
        scrollY = 0;
        content.PositionY = 0;
        if (hadFocus) ToRail();
    }

    /// <summary>Loading: five still bars where the plays will be, 12 apart. No spinner.</summary>
    void Skeleton()
    {
        for (var i = 0; i < 5; i++)
        {
            body.Add(new View
            {
                Position = new Position(Ui.Dp(PleyTv.Gutter), Ui.Dp(ListTop + i * (52 + 12))),
                Size = new global::Tizen.NUI.Size(Ui.Dp(560), Ui.Dp(52)),
                CornerRadius = Ui.Dp(PleyShape.FieldRadius),
                BackgroundColor = PleyColors.Surface2,
            });
        }
    }

    /// <summary>
    /// A page-sized message with one focused action (empty: Back; failed: Retry), 60 under the
    /// header: the title in Title, the sentence in Body Muted, 10 apart, then the pill.
    /// </summary>
    void Message(LibraryMessage m)
    {
        var width = PleyTv.CanvasWidth - PleyTv.Gutter - PleyTv.SafeHorizontal;
        var view = new View { Position = new Position(Ui.Dp(PleyTv.Gutter), Ui.Dp(ListTop + 60)) };
        view.Add(Text.Label(m.Title, PleyType.Title, PleyColors.Fg, width: width));
        var words = Text.Label(m.Body, PleyType.Body, PleyColors.Muted, maxLines: 3, width: width);
        words.VerticalAlignment = VerticalAlignment.Top;
        words.PositionY = Ui.Dp(PleyType.Title.Line + 10);
        view.Add(words);
        body.Add(view);
        var buttons = new ButtonRow();
        buttons.Add(new ButtonView(m.ActionLabel ?? LibraryText.Retry, ButtonKind.Pill, () =>
        {
            if (m.Action == MessageAction.Back) Shell.Back?.Invoke();
            else model.Retry();
        }));
        buttons.PositionY = Ui.Dp(PleyType.Title.Line + 10) + words.GetHeightForWidth(Ui.Dp(width)) + Ui.Dp(10 + 12 + 10);
        view.Add(buttons);
        Group.Add(buttons.Group);
        Tree.Enter(buttons.Group);
        waiting = false;
    }

    void ShowRows(HistoryState.Ready state)
    {
        var current = Tree.Current;
        var focusWasIn = current != null && Within(Group, current);
        var first = rows.Count == 0;
        rows = state.Rows;

        // The plays' focus items stay while their plays do, so focus stays on the one it was on.
        var shown = new HashSet<long>(rows.Select(r => r.EntryId));
        foreach (var id in items.Keys.Where(id => !shown.Contains(id)).ToList()) items.Remove(id);
        lines.Clear();
        var focusedAt = -1;
        foreach (var row in rows)
        {
            if (!items.TryGetValue(row.EntryId, out var pair))
            {
                var id = row.EntryId;
                var line = new FocusItem("play/" + id);
                var cross = new FocusItem("remove/" + id);
                line.Selected = () =>
                {
                    if (Find(id) is { } r) go(r.Opens);
                };
                line.LongSelected = () => Remove(id, fromCross: false);
                cross.Selected = () => Remove(id, fromCross: true);
                pair = (line, cross);
                items[id] = pair;
            }
            if (pair.Line == current) focusedAt = lines.Children.Count;
            if (pair.Cross == current) focusedAt = lines.Children.Count + 1;
            lines.Add(pair.Line);
            lines.Add(pair.Cross);
        }
        if (focusedAt >= 0) lines.RememberChild(focusedAt);

        ShowMore(state);
        Group.Clear();
        Group.Add(lines);
        if (moreRow != null) Group.Add(moreRow.Group);

        // A play that changed (its time, a picture) is drawn again; one that went goes.
        foreach (var id in views.Keys.ToList())
        {
            if (Find(id) is { } row && views[id].Key == PlayView.KeyOf(row)) continue;
            views[id].Dispose();
            views.Remove(id);
        }

        if (focusWasIn && !InTree(current!))
        {
            // What had focus went (a removal the stand-in couldn't take): the page's start has it.
            if (!Tree.Enter(Group)) ToRail();
        }
        else if (first && TakesFocus() && items.Count > 0)
        {
            Tree.Focus(items[rows[0].EntryId].Line);
        }
        FollowFocus();
    }

    /// <summary>Show more, 12 under the list while there are more plays; "Loading…" and dimmed while they come.</summary>
    void ShowMore(HistoryState.Ready state)
    {
        if (!state.CanLoadMore)
        {
            if (moreRow == null) return;
            var hadIt = more != null && more.Item.Focused;
            moreRow.Dispose();
            moreRow = null;
            more = null;
            if (hadIt && rows.Count > 0) Tree.Focus(items[rows[rows.Count - 1].EntryId].Line);
            return;
        }
        if (moreRow == null)
        {
            moreRow = new ButtonRow();
            more = moreRow.Add(new ButtonView(HistoryText.More, ButtonKind.Pill, model.LoadMore, iconName: "chevron-down"));
            moreRow.PositionX = Ui.Dp(PleyTv.Gutter);
            content.Add(moreRow);
        }
        more!.SetText(HistoryText.MoreLabel(state));
        moreRow.Arrange();
        moreRow.PositionY = Ui.Dp(rows.Count * Pitch - RowSpacing + MoreGap);
        var focused = more.Item.Focused;
        more.Enabled = !state.LoadingMore;
        // Disabled with focus, it hands focus to the play above it.
        if (focused && state.LoadingMore && rows.Count > 0) Tree.Focus(items[rows[rows.Count - 1].EntryId].Line);
    }

    /// <summary>
    /// Removes a play: focus moves on first, to the same place in the play below (the row, or its
    /// cross), else the one above; then the row goes.
    /// </summary>
    void Remove(long entryId, bool fromCross)
    {
        if (HistoryRules.StandIn(rows, entryId) is { } near && items.TryGetValue(near, out var pair))
            Tree.Focus(fromCross ? pair.Cross : pair.Line);
        model.Remove(entryId);
    }

    HistoryRow? Find(long entryId)
    {
        foreach (var row in rows)
            if (row.EntryId == entryId) return row;
        return null;
    }

    void FollowFocus()
    {
        if (Tree.Current is not { } item) return;
        var at = IndexIn(lines, item);
        if (at >= 0) ScrollTo(at / 2 * Pitch);
        else if (more != null && item == more.Item) ScrollTo(moreRow!.PositionY / Ui.Scale);
        else Draw();
    }

    /// <summary>The list moves so the focused row's top sits at the pivot, as far as its ends allow.</summary>
    void ScrollTo(float top)
    {
        if (gone) return;
        var length = rows.Count * Pitch - RowSpacing + (moreRow != null ? MoreGap + PleyTv.ButtonHeight : 0) + BottomPadding;
        var target = Math.Max(0, Math.Min(top - Viewport * Pivot, length - Viewport));
        if (gate.Holding) heldHere = true;
        if (Math.Abs(target - scrollY) > 0.5f)
        {
            scrollY = target;
            scroll?.Stop();
            scroll = Scrolling.Glide(gate);
            scroll.AnimateTo(content, "PositionY", -Ui.Dp(target));
            scroll.Play();
        }
        Draw();
        if (moreRow != null && more != null && more.Item.Focused) moreRow.RaiseToTop();
    }

    /// <summary>Plays on screen, and a few either side, have views; the rest go.</summary>
    void Draw()
    {
        if (gone) return;
        var firstRow = Math.Max(0, (int)(scrollY / Pitch) - RowsNear);
        var lastRow = Math.Min(rows.Count - 1, (int)((scrollY + Viewport) / Pitch) + RowsNear);
        var near = new HashSet<long>();
        for (var i = firstRow; i <= lastRow; i++) near.Add(rows[i].EntryId);
        foreach (var id in views.Keys.Where(id => !near.Contains(id)).ToList())
        {
            views[id].Dispose();
            views.Remove(id);
        }
        for (var i = firstRow; i <= lastRow; i++)
        {
            var row = rows[i];
            if (!views.TryGetValue(row.EntryId, out var view))
            {
                var pair = items[row.EntryId];
                view = new PlayView(row, pair.Line, pair.Cross, images);
                content.Add(view);
                views[row.EntryId] = view;
            }
            view.Position = new Position(Ui.Dp(PleyTv.Gutter), Ui.Dp(i * Pitch));
            if (!gate.Holding) view.LoadPicture();
        }
    }

    void Landed()
    {
        if (!heldHere || gone) return;
        heldHere = false;
        foreach (var view in views.Values) view.LoadPicture();
    }

    bool InTree(FocusItem item)
    {
        FocusNode node = item;
        while (node.Parent != null) node = node.Parent;
        return node == Tree.Root;
    }

    static int IndexIn(FocusGroup group, FocusNode node)
    {
        for (var i = 0; i < group.Children.Count; i++)
            if (group.Children[i] == node) return i;
        return -1;
    }

    static bool Within(FocusGroup group, FocusNode node)
    {
        for (var n = node.Parent; n != null; n = n.Parent)
            if (n == group) return true;
        return false;
    }

    /// <summary>
    /// A play's row: 700 wide, at least 64 tall, its picture in a 48 dp column (a 48 square, or a
    /// 34 × 51 poster), 16, then the title in Body over the line in Caption; the accent fill with
    /// OnAccent ink while it has focus. 12 to its right, the cross: a 40 dp circle on Surface3, the
    /// accent while it has focus. Neither scales: they are rows of a list.
    /// </summary>
    sealed class PlayView : View
    {
        readonly HistoryRow row;
        readonly FocusItem line;
        readonly FocusItem cross;
        readonly Images images;
        readonly View box;
        readonly View art;
        readonly ImageView picture;
        readonly TextLabel title;
        readonly TextLabel caption;
        readonly View circle;
        readonly ImageView x;
        readonly float artWidth;
        bool asked;

        public PlayView(HistoryRow row, FocusItem line, FocusItem cross, Images images)
        {
            this.row = row;
            this.line = line;
            this.cross = cross;
            this.images = images;
            Key = KeyOf(row);
            Size = new global::Tizen.NUI.Size(Ui.Dp(RowWidth + CrossGap + CrossSize), Ui.Dp(RowHeight));
            box = new View
            {
                Size = new global::Tizen.NUI.Size(Ui.Dp(RowWidth), Ui.Dp(RowHeight)),
                CornerRadius = Ui.Dp(PleyShape.FieldRadius),
            };
            Add(box);
            artWidth = row.Square ? 48 : 34;
            var artHeight = row.Square ? 48 : 51;
            art = new View
            {
                Position = new Position(Ui.Dp(12 + (48 - artWidth) / 2), Ui.Dp((RowHeight - artHeight) / 2)),
                Size = new global::Tizen.NUI.Size(Ui.Dp(artWidth), Ui.Dp(artHeight)),
                CornerRadius = Ui.Dp(PleyShape.BadgeRadius),
                BackgroundColor = PleyColors.Surface2,
                ClippingMode = ClippingModeType.ClipChildren,
            };
            picture = new ImageView { Size = art.Size, FittingMode = FittingModeType.ScaleToFill, Opacity = 0 };
            art.Add(picture);
            box.Add(art);
            const float textX = 12 + 48 + 16;
            var textWidth = RowWidth - textX - 12;
            var textTop = (RowHeight - PleyType.Body.Line - PleyType.Caption.Line) / 2;
            title = Text.Label(row.Title, PleyType.Body, PleyColors.Fg, width: textWidth);
            title.Position = new Position(Ui.Dp(textX), Ui.Dp(textTop));
            box.Add(title);
            caption = Text.Label(row.Line, PleyType.Caption, PleyColors.Muted, width: textWidth);
            caption.Position = new Position(Ui.Dp(textX), Ui.Dp(textTop + PleyType.Body.Line));
            box.Add(caption);
            circle = new View
            {
                Position = new Position(Ui.Dp(RowWidth + CrossGap), Ui.Dp((RowHeight - CrossSize) / 2)),
                Size = new global::Tizen.NUI.Size(Ui.Dp(CrossSize), Ui.Dp(CrossSize)),
                CornerRadius = Ui.Dp(CrossSize / 2),
            };
            x = Icons.Icon("x", 20, PleyColors.Muted);
            x.Position = new Position(Ui.Dp((CrossSize - 20) / 2), Ui.Dp((CrossSize - 20) / 2));
            circle.Add(x);
            Add(circle);
            line.FocusChanged += LookLine;
            cross.FocusChanged += LookCross;
            LookLine(line.Focused);
            LookCross(cross.Focused);
        }

        /// <summary>What it draws, to draw it again only when that changed.</summary>
        public string Key { get; }

        public static string KeyOf(HistoryRow row) => string.Join("\n", new[] { row.Title, row.Line, row.Square ? "1" : "0" }.Concat(row.Art));

        public void LoadPicture()
        {
            if (asked || row.Art.Count == 0) return;
            asked = true;
            images.Load(row.Art, artWidth, file =>
            {
                if (file == null || Disposed) return;
                picture.ResourceUrl = file;
                var fade = new Animation(PleyMotion.ImageFadeMs);
                fade.AnimateTo(picture, "Opacity", 1f);
                fade.PlayHeld();
            });
        }

        void LookLine(bool on)
        {
            if (Disposed) return;
            box.BackgroundColor = on ? PleyColors.Accent : Color.Transparent;
            title.TextColor = on ? PleyColors.OnAccent : PleyColors.Fg;
            caption.TextColor = on ? PleyColors.OnAccent : PleyColors.Muted;
        }

        void LookCross(bool on)
        {
            if (Disposed) return;
            circle.BackgroundColor = on ? PleyColors.Accent : PleyColors.Surface3;
            x.Color = on ? PleyColors.OnAccent : PleyColors.Muted;
        }

        protected override void Dispose(DisposeTypes type)
        {
            if (Disposed) return;
            line.FocusChanged -= LookLine;
            cross.FocusChanged -= LookCross;
            base.Dispose(type);
        }
    }
}
