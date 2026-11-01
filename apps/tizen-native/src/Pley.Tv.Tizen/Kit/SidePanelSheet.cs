using System;
using System.Collections.Generic;
using System.Linq;
using Pley.Tv.Core.Ui;
using Tizen.NUI;
using Tizen.NUI.BaseComponents;

namespace Pley.Tv.Tizen.Kit;

/// <summary>
/// A side panel (design spec §2.7; the Android app's TvSidePanel and TvPanelRow): 360 dp from the
/// right edge, full height, Surface2 with 16 dp left corners, over the scrim. Its own sheet keeps
/// focus inside it, starting on the row it is given (the current choice); Back closes it, and the
/// page's focus is where it was. Rows are 44 tall, Body text with an optional icon, a value in
/// Caption and a check or a chevron; focused, an accent pill, never scaled. Rows past the screen
/// scroll into view. A row with a hint (what a choice does) has it under its label in Caption,
/// and grows to fit both.
/// </summary>
sealed class SidePanelSheet : Sheet
{
    const float Width = 360, Left = 24, RowHeight = 44, HeadingHeight = 36;

    /// <summary>A row as the panel draws it.</summary>
    /// <param name="Value">Muted beside the label (a library's genre chosen).</param>
    /// <param name="Opens">A chevron: the row opens another list.</param>
    /// <param name="Hint">Under the label, muted: what the choice does (Settings' choices).</param>
    public sealed record Line(string Label, bool Heading = false, bool Checked = false, string? Icon = null, string? Value = null, bool Opens = false, string? Hint = null);

    readonly Shell shell;
    readonly View list = new View();
    float scroll;

    /// <param name="choose">What a row does; true keeps the panel open (a track picked), false closes it.</param>
    public SidePanelSheet(Shell shell, SidePanel panel, Func<PanelRow, bool> choose)
        : this(shell, panel.Title, panel.Rows.Select(r => new Line(r.Label, r.Kind == PanelRowKind.Heading, r.Checked, r.Icon)).ToList(), panel.Focus, i => choose(panel.Rows[i]))
    {
    }

    /// <param name="choose">What a row does; true keeps the panel open, false closes it.</param>
    public SidePanelSheet(Shell shell, LibraryPanelView panel, Func<LibraryPanelRow, bool> choose)
        : this(shell, panel.Title, panel.Rows.Select(r => new Line(r.Label, r.Kind == LibraryRowKind.Heading, r.Checked, Value: r.Detail, Opens: r.Opens)).ToList(), panel.Focus, i => choose(panel.Rows[i]))
    {
    }

    /// <param name="focus">The row focus starts on; the first after it that can take focus otherwise.</param>
    /// <param name="choose">What the row at an index does; true keeps the panel open, false closes it.</param>
    public SidePanelSheet(Shell shell, string title, IReadOnlyList<Line> rows, int focus, Func<int, bool> choose) : base("panel")
    {
        this.shell = shell;
        View.BackgroundColor = PleyColors.Scrim;
        var card = new View
        {
            Size = new Size(Ui.Dp(Width), Ui.Dp(PleyTv.CanvasHeight)),
            Position = new Position(Ui.Dp(PleyTv.CanvasWidth - Width), 0),
            BackgroundColor = PleyColors.Surface2,
            CornerRadius = new Vector4(Ui.Dp(PleyShape.DialogRadius * 2), 0, 0, Ui.Dp(PleyShape.DialogRadius * 2)),
            ClippingMode = ClippingModeType.ClipChildren,
        };
        var inner = Width - Left - PleyTv.SafeHorizontal;
        var heading = Text.Label(title, PleyType.Title, PleyColors.Fg, width: inner);
        heading.Position = new Position(Ui.Dp(Left), Ui.Dp(PleyTv.SafeVertical));
        card.Add(heading);
        var listTop = PleyTv.SafeVertical + PleyType.Title.Line + 12;
        var viewport = new View
        {
            Position = new Position(Ui.Dp(Left), Ui.Dp(listTop)),
            Size = new Size(Ui.Dp(inner), Ui.Dp(PleyTv.CanvasHeight - listTop - PleyTv.SafeVertical)),
            ClippingMode = ClippingModeType.ClipChildren,
        };
        viewport.Add(list);
        card.Add(viewport);
        View.Add(card);

        var group = Root;
        FocusItem? first = null;
        float y = 0;
        for (var i = 0; i < rows.Count; i++)
        {
            var row = rows[i];
            if (row.Heading)
            {
                var label = Text.Label(row.Label, PleyType.Caption, PleyColors.Muted, width: inner);
                label.Position = new Position(Ui.Dp(16), Ui.Dp(y + HeadingHeight - PleyType.Caption.Line - 6));
                list.Add(label);
                y += HeadingHeight;
                continue;
            }
            var at = i;
            var view = new PanelRowView(row, inner, () =>
            {
                if (!choose(at)) Close();
            });
            view.PositionY = Ui.Dp(y);
            list.Add(view);
            group.Add(view.Item);
            var top = y;
            var height = view.HeightDp;
            view.Item.FocusChanged += on =>
            {
                if (on) Reveal(top, height, viewport.SizeHeight / Ui.Scale);
            };
            if (i == focus || first == null && i > focus) first ??= view.Item;
            y += height + 4;
        }
        first ??= group.Children.Count > 0 ? group.Children[0] as FocusItem : null;
        if (first != null) Tree.Focus(first);
    }

    public void Show() => shell.Push(this);

    public void Close() => shell.Pop(this);

    /// <summary>Keeps the focused row inside the panel's room.</summary>
    void Reveal(float top, float height, float room)
    {
        if (top < scroll) scroll = top;
        else if (top + height > scroll + room) scroll = top + height - room;
        list.PositionY = -Ui.Dp(scroll);
    }

    /// <summary>One row: its label in Body, an optional 20 dp icon, a muted value or hint, an accent check when chosen or a chevron.</summary>
    sealed class PanelRowView : View
    {
        readonly TextLabel label;
        readonly TextLabel? value;
        readonly TextLabel? hint;
        readonly ImageView? icon;
        readonly ImageView? check;
        readonly ImageView? chevron;

        public PanelRowView(Line row, float width, Action choose)
        {
            Size = new Size(Ui.Dp(width), Ui.Dp(RowHeight));
            CornerRadius = Ui.Dp(RowHeight / 2);
            HeightDp = RowHeight;
            Item = new FocusItem(row.Label) { Selected = choose };
            float x = 16;
            if (row.Icon != null)
            {
                icon = Icons.Icon(row.Icon, PleyTv.IconSize, PleyColors.Fg);
                icon.Position = new Position(Ui.Dp(x), Ui.Dp((RowHeight - PleyTv.IconSize) / 2));
                Add(icon);
                x += PleyTv.IconSize + 12;
            }
            var right = width - 16;
            if (row.Checked)
            {
                check = Icons.Icon("check", PleyTv.IconSize, PleyColors.Accent);
                check.Position = new Position(Ui.Dp(right - PleyTv.IconSize), Ui.Dp((RowHeight - PleyTv.IconSize) / 2));
                Add(check);
                right -= PleyTv.IconSize + 12;
            }
            else if (row.Opens)
            {
                chevron = Icons.Icon("chevron-right", PleyTv.IconSize, PleyColors.Fg);
                chevron.Position = new Position(Ui.Dp(right - PleyTv.IconSize), Ui.Dp((RowHeight - PleyTv.IconSize) / 2));
                Add(chevron);
                right -= PleyTv.IconSize + 12;
            }
            if (row.Value != null)
            {
                value = Text.Label(row.Value, PleyType.Caption, PleyColors.Muted, width: 120);
                value.HorizontalAlignment = HorizontalAlignment.End;
                value.Position = new Position(Ui.Dp(right - 120), Ui.Dp((RowHeight - PleyType.Caption.Line) / 2));
                Add(value);
                right -= 120 + 12;
            }
            var room = Math.Max(40, right - x);
            if (row.Hint != null)
            {
                // The label on up to two lines and the hint on up to three, 8 dp above and below.
                label = Text.Label(row.Label, PleyType.Body, PleyColors.Fg, maxLines: 2, width: room);
                hint = Text.Label(row.Hint, PleyType.Caption, PleyColors.Muted, maxLines: 3, width: room);
                var labelHeight = Fit(label, PleyType.Body, room, 2);
                var hintHeight = Fit(hint, PleyType.Caption, room, 3);
                var height = Math.Max(Ui.Dp(RowHeight), labelHeight + hintHeight + Ui.Dp(16));
                var textTop = (height - labelHeight - hintHeight) / 2;
                label.Position = new Position(Ui.Dp(x), textTop);
                hint.Position = new Position(Ui.Dp(x), textTop + labelHeight);
                Add(label);
                Add(hint);
                SizeHeight = height;
                HeightDp = height / Ui.Scale;
                foreach (var end in new View?[] { icon, check, chevron })
                    if (end != null) end.PositionY = (height - Ui.Dp(PleyTv.IconSize)) / 2;
            }
            else
            {
                label = Text.Label(row.Label, PleyType.Body, PleyColors.Fg, width: room);
                label.Position = new Position(Ui.Dp(x), Ui.Dp((RowHeight - PleyType.Body.Line) / 2));
                Add(label);
            }
            Item.FocusChanged += Look;
        }

        public FocusItem Item { get; }

        /// <summary>The row's height in dp: 44, or more for a hint.</summary>
        public float HeightDp { get; }

        /// <summary>Gives <paramref name="text"/> the height of its lines at <paramref name="widthDp"/>, <paramref name="max"/> at most; in pixels.</summary>
        static float Fit(TextLabel text, TextStyle style, float widthDp, int max)
        {
            var line = Text.LineHeight(style);
            var height = Math.Min(Math.Max(text.GetHeightForWidth(Ui.Dp(widthDp)), line), line * max);
            text.VerticalAlignment = VerticalAlignment.Top;
            text.SizeHeight = height;
            return height;
        }

        void Look(bool focused)
        {
            if (Disposed) return;
            BackgroundColor = focused ? PleyColors.Accent : Color.Transparent;
            var ink = focused ? PleyColors.OnAccent : PleyColors.Fg;
            label.TextColor = ink;
            if (icon != null) icon.Color = ink;
            if (chevron != null) chevron.Color = ink;
            if (value != null) value.TextColor = focused ? PleyColors.OnAccent : PleyColors.Muted;
            if (hint != null) hint.TextColor = focused ? PleyColors.OnAccent : PleyColors.Muted;
            if (check != null) check.Color = focused ? PleyColors.OnAccent : PleyColors.Accent;
        }
    }
}
