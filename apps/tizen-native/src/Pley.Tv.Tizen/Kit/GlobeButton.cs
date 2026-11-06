using System.Linq;
using Pley.Tv.Core.Ui;
using Tizen.NUI;

namespace Pley.Tv.Tizen.Kit;

/// <summary>
/// The globe of the screens before signing in (<see cref="LanguageGlobe"/>), where the web Samsung
/// app has it: a small plain globe and the language shown by its two letters, without a fill until
/// it has focus, "FR" at the screen's right edge. At the bottom right of Choose your server and the
/// address screen; on the sign-in code at the top right, centred on the logo's line. The remote's
/// Down or Up reaches it. OK opens the languages in a side panel, as Settings shows its choices.
/// </summary>
static class GlobeButton
{
    /// <summary>
    /// From the screen's right edge to what the globe shows, and from its bottom edge to the globe's:
    /// centred on the forget hint's line (Caption, 27 from the bottom).
    /// </summary>
    const float Right = 48, Bottom = 20;

    /// <summary>A small button's room either side of what it shows, which the globe hangs out by.</summary>
    const float Padding = 10;

    /// <summary>The globe at the bottom right, or at the top right centred on <paramref name="centreY"/> (in dp).</summary>
    public static ButtonView Of(Shell shell, LanguageGlobe globe, float? centreY = null)
    {
        var button = new ButtonView(globe.Label, ButtonKind.Small, () => Open(shell, globe), iconName: "globe");
        var top = centreY is { } y ? y - button.Height / 2 : PleyTv.CanvasHeight - Bottom - button.Height;
        button.Position = new Position(Ui.Dp(PleyTv.CanvasWidth - Right + Padding) - button.SizeWidth, Ui.Dp(top));
        return button;
    }

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
