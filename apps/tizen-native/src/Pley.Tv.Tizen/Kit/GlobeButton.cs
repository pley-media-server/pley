using System.Linq;
using Pley.Tv.Core.Ui;
using Tizen.NUI;

namespace Pley.Tv.Tizen.Kit;

/// <summary>
/// The globe of the screens before signing in (<see cref="LanguageGlobe"/>), where the web Samsung
/// app has it: at the bottom left, a small plain globe and the language shown by its two letters,
/// without a fill until it has focus; the remote's Down reaches it. OK opens the languages in a side panel,
/// as Settings shows its choices.
/// </summary>
static class GlobeButton
{
    /// <summary>From the screen's bottom edge to the button's.</summary>
    const float Bottom = 24;

    /// <summary>A small button's room either side of what it shows, which the globe hangs out by.</summary>
    const float Padding = 10;

    /// <summary>The globe, at the bottom left, the icon in line with <paramref name="left"/> (the screen's left edge, in dp).</summary>
    public static ButtonView Of(Shell shell, LanguageGlobe globe, float left)
    {
        var button = new ButtonView(globe.Label, ButtonKind.Small, () => Open(shell, globe), iconName: "globe");
        button.Position = new Position(Ui.Dp(left - Padding), Ui.Dp(PleyTv.CanvasHeight - Bottom - button.Height));
        return button;
    }

    /// <summary>Where a line beside the globe (a status, a hint) starts, in dp.</summary>
    public static float Beside(ButtonView button) => button.PositionX / Ui.Scale + button.WidthDp + 6;

    /// <summary>The top, in dp, of a line <paramref name="lineHeight"/> tall beside the globe, centred on it.</summary>
    public static float BesideTop(ButtonView button, float lineHeight) => PleyTv.CanvasHeight - Bottom - (button.Height + lineHeight) / 2;

    static void Open(Shell shell, LanguageGlobe globe)
    {
        var rows = globe.Choices.Select(c => new SidePanelSheet.Line(c.Label, Checked: c.Checked)).ToList();
        // The language changes as the panel closes: the screen is drawn again in it, the globe focused.
        new SidePanelSheet(shell, LanguageGlobe.Title, rows, globe.Focus, index =>
        {
            globe.Choose(index);
            return false;
        }).Show();
    }
}
