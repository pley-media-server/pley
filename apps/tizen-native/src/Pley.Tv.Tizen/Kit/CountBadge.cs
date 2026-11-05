using System;
using System.Globalization;
using Tizen.NUI;
using Tizen.NUI.BaseComponents;

namespace Pley.Tv.Tizen.Kit;

/// <summary>A number on its own small pill, "99+" past 99: red while <paramref name="alarm"/>, else the accent (inverted while focused).</summary>
sealed class CountBadge : View
{
    /// <summary>Past it, a count says "99+".</summary>
    const int Max = 99;

    readonly TextLabel number;
    readonly bool alarm;

    public CountBadge(int count, bool alarm, float height = 20)
    {
        this.alarm = alarm;
        number = Text.Label(count > Max ? $"{Max}+" : count.ToString(CultureInfo.InvariantCulture), PleyType.Badge, PleyColors.OnAccent);
        var width = Math.Max(Ui.Dp(height), number.NaturalSize2D.Width + Ui.Dp(10));
        Size = new Size(width, Ui.Dp(height));
        CornerRadius = Ui.Dp(height / 2);
        number.Size = new Size(width, Ui.Dp(height));
        number.HorizontalAlignment = HorizontalAlignment.Center;
        number.VerticalAlignment = VerticalAlignment.Center;
        Add(number);
        Focused(false);
    }

    public void Focused(bool on)
    {
        if (Disposed) return;
        BackgroundColor = alarm ? PleyColors.Danger : on ? PleyColors.OnAccent : PleyColors.Accent;
        number.TextColor = alarm ? PleyColors.Bg : on ? PleyColors.AccentBright : PleyColors.OnAccent;
    }
}
