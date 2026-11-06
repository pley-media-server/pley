using System.Linq;
using Pley.Tv.Core.Ui;

namespace Pley.Tv.Tizen.Kit;

/// <summary>
/// The globe of the screens before signing in (<see cref="LanguageGlobe"/>), where the web Samsung
/// app has it: a text button at the top right with the globe and the language shown, the remote's Up
/// reaching it. OK opens the languages in a side panel, as Settings shows its choices.
/// </summary>
static class GlobeButton
{
    /// <summary>Its top: on the line of a screen's title, which is 38 dp tall from y = 27.</summary>
    public const float Top = 26;

    /// <summary>From the screen's right edge to the button's.</summary>
    public const float Right = 48;

    public static ButtonView Of(Shell shell, LanguageGlobe globe) =>
        new ButtonView(globe.Label, ButtonKind.Text, () => Open(shell, globe), iconName: "globe");

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
