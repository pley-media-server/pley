using System.Collections.Generic;
using System.Linq;

namespace Pley.Tv.Core.Ui;

/// <summary>A language the globe lists: its words, and whether it is the choice in force.</summary>
public sealed record LanguageChoice(string Label, bool Checked);

/// <summary>
/// The globe of the screens before signing in (the web app's LanguagePicker, the web Samsung app's
/// LanguageGlobe): the language shown, named in itself, and its list: Automatic, with what that
/// is on this TV (its own language where Pley has it, else the server's, else English), then each
/// language named in itself, the choice in force checked. A choice shows at once and is kept on
/// the TV; Automatic forgets it (<see cref="AppState.ChooseDeviceLanguage"/>).
/// </summary>
public sealed class LanguageGlobe
{
    readonly AppState app;
    bool refocus;

    public LanguageGlobe(AppState app) => this.app = app;

    /// <summary>The globe's words: the language shown, in itself ("Français").</summary>
    public string Label => Languages.NameOf(app.Language.Value);

    /// <summary>The list's title: "Choose a language".</summary>
    public static string Title => Strings.T("signin.language.choose");

    /// <summary>Automatic, then English, Français and Español.</summary>
    public IReadOnlyList<LanguageChoice> Choices
    {
        get
        {
            var choice = app.DeviceLanguage;
            var automatic = SettingsText.Automatic(Languages.NameOf(Languages.OfSystem(app.ServerLanguage, app.TvLanguages)));
            return new[] { new LanguageChoice(automatic, choice == null) }
                .Concat(Languages.All.Select(l => new LanguageChoice(Languages.NameOf(l), l == choice)))
                .ToList();
        }
    }

    /// <summary>The row focus starts on: the choice in force.</summary>
    public int Focus => app.DeviceLanguage is { } choice ? Languages.All.ToList().IndexOf(choice) + 1 : 0;

    /// <summary>The row at <paramref name="index"/> chosen: Automatic (0), or a language.</summary>
    public void Choose(int index)
    {
        if (index < 0 || index > Languages.All.Count) return;
        var before = app.Language.Value;
        app.ChooseDeviceLanguage(index == 0 ? null : Languages.All[index - 1]);
        // The screen is drawn again in the new language: there, focus goes back to the globe.
        refocus = app.Language.Value != before;
    }

    /// <summary>
    /// Whether the screen being drawn again after a choice gives the globe focus, as it had it:
    /// true once after a choice changed the language.
    /// </summary>
    public bool TakeFocus()
    {
        var again = refocus;
        refocus = false;
        return again;
    }
}
