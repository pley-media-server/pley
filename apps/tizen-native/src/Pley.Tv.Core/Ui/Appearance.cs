using System.Collections.Generic;
using System.Linq;

namespace Pley.Tv.Core.Ui;

/// <summary>Settings' Appearance page, opened at its themes, or at its accents (<paramref name="Accents"/>).</summary>
public sealed record AppearancePage(bool Accents = false) : PageKey;

/// <summary>A theme to pick: its name and caption, its colors under the accent picked (for its preview), and whether it is the one shown.</summary>
public sealed record ThemeChoice(string Id, string Name, string About, PleyPalette Palette, bool Checked);

/// <summary>An accent to pick (a null id: Default, the theme's own): its name, its color in the theme shown, and whether it is the one shown.</summary>
public sealed record AccentChoice(string? Id, string Name, PleyPalette Palette, bool Checked);

/// <summary>
/// What Appearance offers (the web's ThemeCard, docs/THEMES.md "In every app"): the nine themes in
/// themes.json's order, each drawn under the accent picked, as the web's previews take the page's;
/// then Default and the accents, each in the theme shown.
/// </summary>
public static class Appearance
{
    /// <summary>The themes picker shows at a time in a row.</summary>
    public const int Columns = 5;

    public static IReadOnlyList<ThemeChoice> Themes(ThemePick pick) =>
        Ui.Themes.Ids.Select(id => new ThemeChoice(id, ThemeText.Name(id), ThemeText.About(id), PleyPalette.Of(id, pick.Accent), id == pick.Theme)).ToList();

    public static IReadOnlyList<AccentChoice> Accents(ThemePick pick) =>
        new string?[] { null }.Concat(Ui.Themes.AccentIds).Select(id => new AccentChoice(id, ThemeText.AccentName(id), PleyPalette.Of(pick.Theme, id), id == pick.Accent)).ToList();
}
