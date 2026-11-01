using System;
using Pley.Tv.Core.Ui;
using Tizen.NUI;
using Tizen.NUI.BaseComponents;

namespace Pley.Tv.Tizen.Kit;

/// <summary>
/// A dialog (design spec §2.10; the Android app's TvDialog): a Surface2 card, radius 16, 560 wide at
/// most, centred over the scrim; its title in Title, its body in Body muted, and its buttons
/// right-aligned. Back closes it. Its own sheet keeps focus on its buttons while it is open, and
/// the page's focus is as it was once it closes.
/// </summary>
sealed class Dialog : Sheet
{
    /// <summary>A dialog's width, unless it asks for its own (a short question needs less).</summary>
    public const float DefaultWidth = 560;

    const float Padding = 24, Radius = 16;

    readonly float width;

    readonly Shell shell;
    readonly ButtonRow buttons = new ButtonRow(12);
    readonly View card;
    ButtonView? first;

    public Dialog(Shell shell, string title, string body, float width = DefaultWidth) : base("dialog", Axis.Row)
    {
        this.shell = shell;
        this.width = width;
        View.BackgroundColor = PleyColors.Scrim;
        var inner = width - Padding * 2;
        var titleLabel = Text.Label(title, PleyType.Title, PleyColors.Fg, maxLines: 2, width: inner);
        titleLabel.VerticalAlignment = VerticalAlignment.Top;
        var bodyLabel = Text.Label(body, PleyType.Body, PleyColors.Muted, maxLines: 4, width: inner);
        bodyLabel.VerticalAlignment = VerticalAlignment.Top;
        // As tall as the text in each: the card fits its words.
        var titleHeight = Math.Min(titleLabel.GetHeightForWidth(Ui.Dp(inner)), Ui.Dp(PleyType.Title.Line * 2));
        var bodyHeight = Math.Min(bodyLabel.GetHeightForWidth(Ui.Dp(inner)), Ui.Dp(PleyType.Body.Line * 4));
        titleLabel.SizeHeight = titleHeight;
        bodyLabel.SizeHeight = bodyHeight;
        titleLabel.Position = new Position(Ui.Dp(Padding), Ui.Dp(Padding));
        bodyLabel.Position = new Position(Ui.Dp(Padding), Ui.Dp(Padding + 8) + titleHeight);
        var buttonsY = Ui.Dp(Padding + 8 + 16) + titleHeight + bodyHeight;
        buttons.PositionY = buttonsY;
        var height = buttonsY + Ui.Dp(PleyTv.ButtonHeight + Padding);
        card = new View
        {
            Size = new Size(Ui.Dp(width), height),
            Position = new Position(Ui.Dp((PleyTv.CanvasWidth - width) / 2), (Ui.Dp(PleyTv.CanvasHeight) - height) / 2),
            CornerRadius = Ui.Dp(Radius),
            BackgroundColor = PleyColors.Surface2,
        };
        card.Add(titleLabel);
        card.Add(bodyLabel);
        card.Add(buttons);
        View.Add(card);
        Root.Add(buttons.Group);
    }

    /// <summary>Adds a button; <paramref name="initial"/> is where focus starts (the safe choice).</summary>
    public ButtonView Button(string text, Action onClick, Color? color = null, bool initial = false)
    {
        var b = buttons.Add(new ButtonView(text, ButtonKind.Text, onClick, color));
        buttons.PositionX = Ui.Dp(width - Padding - buttons.WidthDp);
        if (initial || first == null) first = b;
        return b;
    }

    public void Show()
    {
        shell.Push(this);
        if (first != null) Tree.Focus(first.Item);
    }

    public void Close() => shell.Pop(this);
}
