using System;
using System.Linq;
using Pley.Tv.Core.Ui;
using Tizen.NUI;
using Tizen.NUI.BaseComponents;

namespace Pley.Tv.Tizen.Kit;

/// <summary>
/// A side panel's card with any content (the Android app's TvSidePanel with TvPanelTitle,
/// TvPanelHeading and TvPanelRow): 360 dp from the right edge, full height, Surface2 with rounded
/// left corners, its content from x = 24 to the safe edge. Parts are added top to bottom; those
/// that take focus join <see cref="Group"/>, and a part focused past the card's room scrolls into
/// view. <see cref="SidePanelSheet"/> draws plain lists; this one has fields, art and buttons too.
/// </summary>
sealed class PanelColumn : View
{
    public const float Width = 360, Inset = 24;

    /// <summary>The content's width: 24 from the card's left edge to the safe edge.</summary>
    public const float Inner = Width - Inset - PleyTv.SafeHorizontal;

    readonly View viewport;
    readonly View list = new View();
    float bottom;
    float scroll;

    public PanelColumn()
    {
        Size = new Size(Ui.Dp(Width), Ui.Dp(PleyTv.CanvasHeight));
        Position = new Position(Ui.Dp(PleyTv.CanvasWidth - Width), 0);
        BackgroundColor = PleyColors.Surface2;
        CornerRadius = new Vector4(Ui.Dp(PleyShape.DialogRadius), 0, 0, Ui.Dp(PleyShape.DialogRadius));
        ClippingMode = ClippingModeType.ClipChildren;
        // Level 2's hairline along the top.
        Add(new View { Size = new Size(Ui.Dp(Width), Ui.Dp(1)), BackgroundColor = PleyColors.Surface4 });
        viewport = new View
        {
            Position = new Position(Ui.Dp(Inset), Ui.Dp(PleyTv.SafeVertical)),
            Size = new Size(Ui.Dp(Inner), Ui.Dp(Room)),
            ClippingMode = ClippingModeType.ClipChildren,
        };
        viewport.Add(list);
        Add(viewport);
    }

    /// <summary>The parts that take focus, top to bottom.</summary>
    public FocusGroup Group { get; } = new FocusGroup(Axis.Column, "panel");

    /// <summary>The content's height in dp, between the safe area's top and bottom.</summary>
    public static float Room => PleyTv.CanvasHeight - 2 * PleyTv.SafeVertical;

    /// <summary>Where the parts end so far, to take away what comes after it later (<see cref="ClearFrom"/>).</summary>
    public readonly struct Mark
    {
        internal Mark(int views, int items, float bottom)
        {
            Views = views;
            Items = items;
            Bottom = bottom;
        }

        internal int Views { get; }
        internal int Items { get; }
        internal float Bottom { get; }
    }

    /// <summary>Where the parts end now.</summary>
    public Mark Here => new Mark(list.Children.Count, Group.Children.Count, bottom);

    /// <summary>Takes every part away (the panel shows something else now).</summary>
    public void Clear()
    {
        ClearFrom(new Mark(0, 0, 0));
        scroll = 0;
        list.PositionY = 0;
    }

    /// <summary>Takes away the parts placed after <paramref name="mark"/>: the rows under a field that stays as it is, typing and all.</summary>
    public void ClearFrom(Mark mark)
    {
        var views = list.Children.ToArray();
        for (var i = views.Length - 1; i >= mark.Views; i--)
        {
            list.Remove(views[i]);
            views[i].Dispose();
        }
        var items = Group.Children.ToArray();
        for (var i = items.Length - 1; i >= mark.Items; i--)
        {
            if (items[i] is FocusItem item && reveals.TryGetValue(item, out var reveal))
            {
                item.FocusChanged -= reveal;
                reveals.Remove(item);
            }
            Group.Remove(items[i]);
        }
        bottom = mark.Bottom;
    }

    readonly System.Collections.Generic.Dictionary<FocusItem, Action<bool>> reveals = new System.Collections.Generic.Dictionary<FocusItem, Action<bool>>();

    /// <summary>
    /// Adds <paramref name="view"/> under the parts so far, <paramref name="x"/> in and
    /// <paramref name="gap"/> below them, taking <paramref name="height"/> dp; with
    /// <paramref name="item"/>, focus can land on it, and doing so brings it into view.
    /// </summary>
    public T Place<T>(T view, float height, float x = 0, float gap = 0, FocusItem? item = null) where T : View
    {
        var top = bottom + gap;
        view.Position = new Position(Ui.Dp(x), Ui.Dp(top));
        list.Add(view);
        bottom = top + height;
        if (item != null)
        {
            Group.Add(item);
            if (reveals.TryGetValue(item, out var before)) item.FocusChanged -= before;
            Action<bool> reveal = on =>
            {
                if (on && !Disposed) Reveal(top, height);
            };
            reveals[item] = reveal;
            item.FocusChanged += reveal;
        }
        return view;
    }

    /// <summary>Empty room under the parts so far.</summary>
    public void Space(float height) => bottom += height;

    /// <summary>The panel's title in Title (two lines at most), with what it is for under it in Caption, muted.</summary>
    public void Title(string title, string? subtitle = null)
    {
        var width = Inner - 16;
        var heading = Lines(title, PleyType.Title, PleyColors.Fg, width, 2);
        var block = new View();
        block.Add(heading);
        var height = heading.SizeHeight / Ui.Scale;
        if (!string.IsNullOrEmpty(subtitle))
        {
            var under = Lines(subtitle!, PleyType.Caption, PleyColors.Muted, width, 2);
            under.PositionY = heading.SizeHeight;
            block.Add(under);
            height += under.SizeHeight / Ui.Scale;
        }
        Place(block, height, 16, 8);
        Space(12);
    }

    /// <summary>A heading inside the panel ("Your playlists", "For"), in Label, muted.</summary>
    public void Heading(string text)
    {
        var label = Text.Label(text, PleyType.Label, PleyColors.Muted, width: Inner - 16);
        Place(label, PleyType.Label.Line, 16, 16);
        Space(6);
    }

    /// <summary>A sentence in the panel, <paramref name="lines"/> at most: a hint, a list's failure.</summary>
    public TextLabel Sentence(string text, TextStyle style, Color color, float x = 16, float gap = 8, int lines = 2)
    {
        var label = Lines(text, style, color, Inner - x - 16, lines);
        return Place(label, label.SizeHeight / Ui.Scale, x, gap);
    }

    /// <summary>A row (see <see cref="PanelChoice"/>), focus landing on it when it is enabled.</summary>
    public PanelChoice Choice(PanelChoice choice, float gap = 0) => Place(choice, choice.SizeHeight / Ui.Scale, 0, gap, choice.Item);

    /// <summary>Text of <paramref name="style"/> wrapped to <paramref name="width"/>, as tall as its lines (at most <paramref name="max"/>).</summary>
    public static TextLabel Lines(string text, TextStyle style, Color color, float width, int max)
    {
        var label = Text.Label(text, style, color, maxLines: max, width: width);
        label.VerticalAlignment = VerticalAlignment.Top;
        if (max > 1) label.SizeHeight = Math.Min(Math.Max(label.GetHeightForWidth(Ui.Dp(width)), Text.LineHeight(style)), Text.LineHeight(style) * max);
        return label;
    }

    /// <summary>Keeps the part from <paramref name="top"/>, <paramref name="height"/> tall, inside the panel's room.</summary>
    void Reveal(float top, float height)
    {
        if (top < scroll) scroll = top;
        else if (top + height > scroll + Room) scroll = top + height - Room;
        // The first focusable parts sit under the title: back at the top, the title shows again.
        if (top <= Room - height && scroll <= top) scroll = 0;
        list.PositionY = -Ui.Dp(scroll);
    }
}

/// <summary>
/// A side panel's row (design spec §2.7; the Android app's TvPanelRow): 44 tall at least, its
/// label in Body (two lines at most) with a 20 dp icon or art of its own before it and a line in
/// Caption under it; a chosen row has an accent check. Focused, an accent pill with OnAccent
/// content, never scaled; a danger row reads in DangerText; disabled, it is at 40 % and focus
/// passes it by.
/// </summary>
sealed class PanelChoice : View
{
    const float MinHeight = 44, Pad = 16, DisabledAlpha = 0.4f;

    readonly bool danger;
    readonly TextLabel label;
    readonly TextLabel? supporting;
    readonly ImageView? icon;
    readonly ImageView? check;

    /// <param name="leading">Art in place of the icon (a playlist's cover), drawn as it is.</param>
    /// <param name="leadingSize">The art's size in dp.</param>
    public PanelChoice(string text, Action onClick, string? iconName = null, string? line = null, bool chosen = false, bool danger = false,
        View? leading = null, float leadingSize = 0, float width = PanelColumn.Inner)
    {
        this.danger = danger;
        Item = new FocusItem(text) { Selected = onClick };
        float x = Pad;
        var lead = 0f;
        if (leading != null)
        {
            Add(leading);
            lead = leadingSize;
            x += leadingSize + 12;
        }
        else if (iconName != null)
        {
            icon = Icons.Icon(iconName, PleyTv.IconSize, PleyColors.Fg);
            Add(icon);
            lead = PleyTv.IconSize;
            x += PleyTv.IconSize + 12;
        }
        var right = width - Pad;
        if (chosen)
        {
            check = Icons.Icon("check", PleyTv.IconSize, PleyColors.Accent);
            Add(check);
            right -= PleyTv.IconSize + 12;
        }
        var room = Math.Max(40, right - x);
        label = PanelColumn.Lines(text, PleyType.Body, Rest, room, 2);
        Add(label);
        var textHeight = label.SizeHeight / Ui.Scale;
        if (!string.IsNullOrEmpty(line))
        {
            // Two lines where Android has one: a quality's hint ("4K · HDR10 · 62 Mbps") is longer
            // than the panel in the TV's font, and "…" would hide the part that tells them apart.
            supporting = PanelColumn.Lines(line!, PleyType.Caption, PleyColors.Muted, room, 2);
            Add(supporting);
            textHeight += supporting.SizeHeight / Ui.Scale;
        }
        var height = Math.Max(MinHeight, Math.Max(textHeight, lead) + 16);
        Size = new Size(Ui.Dp(width), Ui.Dp(height));
        CornerRadius = Ui.Dp(Math.Min(height, MinHeight) / 2);
        if (leading != null) leading.Position = new Position(Ui.Dp(Pad), Ui.Dp((height - leadingSize) / 2));
        if (icon != null) icon.Position = new Position(Ui.Dp(Pad), Ui.Dp((height - PleyTv.IconSize) / 2));
        if (check != null) check.Position = new Position(Ui.Dp(right + 12), Ui.Dp((height - PleyTv.IconSize) / 2));
        var textTop = (height - textHeight) / 2;
        label.Position = new Position(Ui.Dp(x), Ui.Dp(textTop));
        if (supporting != null) supporting.Position = new Position(Ui.Dp(x), Ui.Dp(textTop) + label.SizeHeight);
        Item.FocusChanged += Look;
        Look(false);
    }

    public FocusItem Item { get; }

    public bool Enabled
    {
        get => Item.Enabled;
        set
        {
            Item.Enabled = value;
            Opacity = value ? 1f : DisabledAlpha;
        }
    }

    Color Rest => danger ? PleyColors.DangerText : PleyColors.Fg;

    void Look(bool focused)
    {
        // Let go already: a focus change can still reach it on its way to the view in its place.
        if (Disposed) return;
        BackgroundColor = focused ? PleyColors.Accent : Color.Transparent;
        var ink = focused ? PleyColors.OnAccent : Rest;
        label.TextColor = ink;
        if (icon != null) icon.Color = ink;
        if (supporting != null) supporting.TextColor = focused ? PleyColors.OnAccent : PleyColors.Muted;
        if (check != null) check.Color = focused ? PleyColors.OnAccent : PleyColors.Accent;
    }
}

/// <summary>
/// A <see cref="PanelColumn"/> over the page, on the scrim, as its own sheet: focus stays in it,
/// Back closes it, and the page's focus is where it was once it closes.
/// </summary>
sealed class PanelSheet : Sheet
{
    readonly Shell shell;

    public PanelSheet(Shell shell) : base("panel")
    {
        this.shell = shell;
        View.BackgroundColor = PleyColors.Scrim;
        View.Add(Column);
        Root.Add(Column.Group);
    }

    public PanelColumn Column { get; } = new PanelColumn();

    /// <summary>Shows the panel, focus on <paramref name="first"/> (else its first part that takes focus).</summary>
    public void Show(FocusItem? first = null)
    {
        shell.Push(this);
        if (first == null || !Tree.Focus(first)) Tree.Enter(Root);
    }

    public void Close() => shell.Pop(this);
}
