using System;
using Pley.Tv.Core.Ui;
using Tizen.NUI;
using Tizen.NUI.BaseComponents;

namespace Pley.Tv.Tizen.Kit;

/// <summary>The TV's two kinds of text button (the Android app's TvButtons.kt, design spec §2.5).</summary>
enum ButtonKind
{
    /// <summary>Change server, Cancel: a faint fill (Fg at 8 %), no edge, 16 either side of the label.</summary>
    Text,
    /// <summary>Play, Connect: at least 128 wide, Fg at 12 % with a 1 dp Control edge, 20 either side.</summary>
    Pill,
}

/// <summary>
/// A TV button: 40 tall, its label in Label. Focus is always the accent fill with OnAccent ink and a
/// small scale (1.05); disabled, it is at 40 % and can't take focus, so focus skips it.
/// </summary>
sealed class ButtonView : View
{
    const float PillMinWidth = 128, DisabledAlpha = 0.4f;

    readonly ButtonKind kind;
    readonly TextLabel label;
    readonly ImageView? icon;
    readonly float maxWidth;
    Color ink;
    Color? rest;
    Animation? focusAnimation;

    /// <param name="maxWidth">The widest it grows, in dp (a recent search's chip); a longer label ends in "…". 0: as wide as its label.</param>
    public ButtonView(string text, ButtonKind kind, Action onClick, Color? color = null, string? iconName = null, float maxWidth = 0)
    {
        this.kind = kind;
        this.maxWidth = maxWidth;
        ink = color ?? PleyColors.Fg;
        Item = new FocusItem(text) { Selected = onClick };
        label = Text.Label(text, PleyType.Label, ink);
        label.HorizontalAlignment = HorizontalAlignment.Center;
        CornerRadius = Ui.Dp(PleyShape.ControlRadius);
        PivotPoint = global::Tizen.NUI.PivotPoint.Center;
        if (kind == ButtonKind.Pill)
        {
            BorderlineWidth = Ui.Dp(1);
            BorderlineOffset = -1;
        }
        if (iconName != null)
        {
            icon = Icons.Icon(iconName, PleyTv.IconSize, ink);
            Add(icon);
        }
        Add(label);
        SetText(text);
        Item.FocusChanged += Look;
        Look(false);
    }

    public FocusItem Item { get; }

    public void SetText(string text)
    {
        label.Text = text;
        var padding = kind == ButtonKind.Pill ? 20 : 16;
        var textWidth = Text.Width(label);
        var iconRoom = icon != null ? PleyTv.IconSize + 8 : 0;
        if (maxWidth > 0 && textWidth > maxWidth - padding * 2 - iconRoom)
        {
            // Wider than the button may grow: cut to fit with "…" (Text.Fit), as NUI's own ellipsis isn't to be trusted.
            textWidth = maxWidth - padding * 2 - iconRoom;
            label.Text = Text.Fit(text, PleyType.Label, textWidth);
        }
        var content = textWidth + iconRoom;
        var width = Math.Max(content + padding * 2, kind == ButtonKind.Pill ? PillMinWidth : 0);
        Size = new Size(Ui.Dp(width), Ui.Dp(PleyTv.ButtonHeight));
        var x = (width - content) / 2;
        if (icon != null)
        {
            icon.Position = new Position(Ui.Dp(x), Ui.Dp((PleyTv.ButtonHeight - PleyTv.IconSize) / 2));
            x += PleyTv.IconSize + 8;
        }
        label.Size = new Size(Ui.Dp(textWidth) + 2, Ui.Dp(PleyTv.ButtonHeight));
        label.Position = new Position(Ui.Dp(x), 0);
    }

    public bool Enabled
    {
        get => Item.Enabled;
        set
        {
            Item.Enabled = value;
            Opacity = value ? 1f : DisabledAlpha;
        }
    }

    /// <summary>The fill at rest in place of the kind's (a season tab: Surface3 for the season shown).</summary>
    public Color? Rest
    {
        set
        {
            rest = value;
            Look(Item.Focused);
        }
    }

    /// <summary>The label's colour at rest (a season tab: Muted, Fg for the season shown).</summary>
    public Color Ink
    {
        set
        {
            ink = value;
            Look(Item.Focused);
        }
    }

    /// <summary>The icon's colour at rest, when not the label's (a chip's accent check).</summary>
    public Color? IconInk
    {
        set
        {
            iconInk = value;
            Look(Item.Focused);
        }
    }

    Color? iconInk;

    /// <summary>Width in dp, for laying buttons out in a row.</summary>
    public float WidthDp => SizeWidth / Ui.Scale;

    void Look(bool focused)
    {
        // Let go already: a focus change can still reach it on its way to the view in its place.
        if (Disposed) return;
        var fill = kind == ButtonKind.Pill ? 0.12f : 0.08f;
        BackgroundColor = focused ? PleyColors.Accent : rest ?? new Color(PleyColors.Fg.R, PleyColors.Fg.G, PleyColors.Fg.B, fill);
        if (kind == ButtonKind.Pill) BorderlineColor = focused ? Color.Transparent : PleyColors.Control;
        var content = focused ? PleyColors.OnAccent : ink;
        label.TextColor = content;
        if (icon != null) icon.Color = focused ? content : iconInk ?? content;
        focusAnimation?.Stop();
        var scale = focused ? PleyShape.PillScale : 1f;
        focusAnimation = new Animation(focused ? PleyMotion.FocusMs : PleyMotion.UnfocusMs)
        {
            DefaultAlphaFunction = new AlphaFunction(focused ? AlphaFunction.BuiltinFunctions.EaseOut : AlphaFunction.BuiltinFunctions.EaseIn),
        };
        focusAnimation.AnimateTo(this, "Scale", new Vector3(scale, scale, 1));
        focusAnimation.Play();
    }
}

/// <summary>Buttons side by side, <see cref="PleyTv.ButtonGap"/> or <c>gap</c> apart, and their focus row.</summary>
sealed class ButtonRow : View
{
    readonly float gap;
    float width;

    public ButtonRow(float gap = PleyTv.ButtonGap)
    {
        this.gap = gap;
        Group = new FocusGroup(Axis.Row, "buttons");
        HeightResizePolicy = ResizePolicyType.FitToChildren;
    }

    public FocusGroup Group { get; }

    /// <summary>Width in dp of the buttons laid out so far.</summary>
    public float WidthDp => width;

    public ButtonView Add(ButtonView button)
    {
        button.Position = new Position(Ui.Dp(width > 0 ? width + gap : 0), 0);
        width = (width > 0 ? width + gap : 0) + button.WidthDp;
        Size = new Size(Ui.Dp(width), Ui.Dp(PleyTv.ButtonHeight));
        base.Add(button);
        Group.Add(button.Item);
        return button;
    }

    /// <summary>Lays the buttons out again, after one's label changed or one was hidden.</summary>
    public void Arrange()
    {
        width = 0;
        foreach (var child in Children)
        {
            if (child is not ButtonView b || !b.Visibility) continue;
            b.Position = new Position(Ui.Dp(width > 0 ? width + gap : 0), 0);
            width = (width > 0 ? width + gap : 0) + b.WidthDp;
        }
        Size = new Size(Ui.Dp(width), Ui.Dp(PleyTv.ButtonHeight));
    }
}

/// <summary>
/// An icon button (the Android app's TvRoundButton): From start, Info, Watched. A 40 dp square with a
/// control's corners, a 20 dp icon and the pill's Control edge, on Bg at 80 % over a backdrop (on
/// Surface3 off one). Its label fades in 6 dp below while it has focus; the room for it is kept, so
/// a row never jumps.
/// </summary>
sealed class RoundButton : View
{
    readonly View box;
    readonly ImageView icon;
    readonly TextLabel label;
    readonly Color rest;
    Animation? focusAnimation;

    /// <param name="onArt">Over a backdrop or art (Bg at 80 %); false on the page's own ground (Surface3).</param>
    public RoundButton(string text, string iconName, Action onClick, bool onArt = true)
    {
        rest = onArt ? PleyColors.OnArt : PleyColors.Surface3;
        Size = new Size(Ui.Dp(PleyTv.ButtonHeight), Ui.Dp(PleyTv.ButtonHeight));
        Item = new FocusItem(text) { Selected = onClick };
        box = new View
        {
            Size = new Size(Ui.Dp(PleyTv.ButtonHeight), Ui.Dp(PleyTv.ButtonHeight)),
            CornerRadius = Ui.Dp(PleyShape.ControlRadius),
            BorderlineWidth = Ui.Dp(1),
            BorderlineOffset = -1,
            PivotPoint = global::Tizen.NUI.PivotPoint.Center,
        };
        icon = Icons.Icon(iconName, PleyTv.IconSize, PleyColors.Fg);
        icon.Position = new Position(Ui.Dp((PleyTv.ButtonHeight - PleyTv.IconSize) / 2), Ui.Dp((PleyTv.ButtonHeight - PleyTv.IconSize) / 2));
        box.Add(icon);
        Add(box);
        label = Text.Label(text, PleyType.Caption, PleyColors.Fg, width: 160);
        label.HorizontalAlignment = HorizontalAlignment.Center;
        label.Position = new Position(Ui.Dp(PleyTv.ButtonHeight / 2 - 80), Ui.Dp(PleyTv.ButtonHeight + 6));
        label.Opacity = 0;
        Add(label);
        Item.FocusChanged += Look;
        Look(false);
    }

    public FocusItem Item { get; }

    public void SetText(string text) => label.Text = text;

    /// <summary>Disabled, it is at 40 % and can't take focus, as a text button (a collection of one has no Shuffle).</summary>
    public bool Enabled
    {
        get => Item.Enabled;
        set
        {
            Item.Enabled = value;
            Opacity = value ? 1f : 0.4f;
        }
    }

    void Look(bool focused)
    {
        // Let go already: a focus change can still reach it on its way to the view in its place.
        if (Disposed) return;
        box.BackgroundColor = focused ? PleyColors.Accent : rest;
        box.BorderlineColor = focused ? Color.Transparent : PleyColors.Control;
        icon.Color = focused ? PleyColors.OnAccent : PleyColors.Fg;
        focusAnimation?.Stop();
        var scale = focused ? PleyShape.RoundScale : 1f;
        focusAnimation = new Animation(focused ? PleyMotion.FocusMs : PleyMotion.UnfocusMs)
        {
            DefaultAlphaFunction = new AlphaFunction(focused ? AlphaFunction.BuiltinFunctions.EaseOut : AlphaFunction.BuiltinFunctions.EaseIn),
        };
        focusAnimation.AnimateTo(box, "Scale", new Vector3(scale, scale, 1));
        focusAnimation.AnimateTo(label, "Opacity", focused ? 1f : 0f);
        focusAnimation.Play();
    }
}
