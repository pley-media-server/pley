using System;
using System.Collections.Generic;
using System.Globalization;
using System.IO;
using System.Linq;
using System.Text;
using System.Text.Json;

namespace Pley.Tv.Core.Ui;

/// <summary>A color as themes.json writes it: <c>#rrggbb</c>, or <c>rgb(r g b / a)</c> for a shade.</summary>
public readonly struct Rgba : IEquatable<Rgba>
{
    public Rgba(byte r, byte g, byte b, float a = 1f)
    {
        R = r;
        G = g;
        B = b;
        A = a;
    }

    public byte R { get; }
    public byte G { get; }
    public byte B { get; }
    /// <summary>Its alpha, 0 to 1.</summary>
    public float A { get; }

    /// <summary>The same color at <paramref name="alpha"/> of its own alpha (a wash, the dim behind a dialog).</summary>
    public Rgba Times(float alpha) => new Rgba(R, G, B, A * alpha);

    /// <summary><c>#38bdf8</c>, or <c>rgb(16 26 44 / 0.42)</c>; null for anything else.</summary>
    public static Rgba? Parse(string? text)
    {
        if (text == null) return null;
        var s = text.Trim();
        if (s.Length == 7 && s[0] == '#' && uint.TryParse(s.Substring(1), NumberStyles.HexNumber, CultureInfo.InvariantCulture, out var rgb))
            return new Rgba((byte)(rgb >> 16), (byte)(rgb >> 8), (byte)rgb);
        if (!s.StartsWith("rgb(", StringComparison.Ordinal) || !s.EndsWith(")", StringComparison.Ordinal)) return null;
        var parts = s.Substring(4, s.Length - 5).Replace("/", " / ").Split(new[] { ' ' }, StringSplitOptions.RemoveEmptyEntries);
        if (parts.Length != 3 && !(parts.Length == 5 && parts[3] == "/")) return null;
        var c = new byte[3];
        for (var i = 0; i < 3; i++)
            if (!byte.TryParse(parts[i], NumberStyles.None, CultureInfo.InvariantCulture, out c[i])) return null;
        var a = 1f;
        if (parts.Length == 5 && !float.TryParse(parts[4], NumberStyles.Float, CultureInfo.InvariantCulture, out a)) return null;
        return a is >= 0 and <= 1 ? new Rgba(c[0], c[1], c[2], a) : null;
    }

    public bool Equals(Rgba other) => R == other.R && G == other.G && B == other.B && Math.Abs(A - other.A) < 0.0005f;

    public override bool Equals(object? obj) => obj is Rgba other && Equals(other);

    public override int GetHashCode() => (R << 16) | (G << 8) | B;

    public static bool operator ==(Rgba a, Rgba b) => a.Equals(b);

    public static bool operator !=(Rgba a, Rgba b) => !a.Equals(b);

    /// <summary><c>#38bdf8</c>, with its alpha as two more digits when it has one: for the logo's file and for tests' messages.</summary>
    public override string ToString() =>
        A >= 1 ? $"#{R:x2}{G:x2}{B:x2}" : $"#{R:x2}{G:x2}{B:x2}{(int)Math.Round(A * 255):x2}";

    /// <summary>As SVG writes it: <c>#38bdf8</c>, or <c>rgb(…)</c> with its alpha.</summary>
    public string Svg => A >= 1 ? ToString() : string.Format(CultureInfo.InvariantCulture, "rgba({0},{1},{2},{3:0.###})", R, G, B, A);
}

/// <summary>A theme of themes.json: its kind, its dark twin, its own colors and the tokens an accent replaces.</summary>
public sealed class ThemeSpec
{
    internal ThemeSpec(string id, bool dark, string? twin, IReadOnlyDictionary<string, Rgba> colors, IReadOnlyCollection<string> follows)
    {
        Id = id;
        Dark = dark;
        Twin = twin;
        Colors = colors;
        Follows = follows;
    }

    public string Id { get; }
    /// <summary>A dark theme, which takes an accent's dark set; else a light one, its light set.</summary>
    public bool Dark { get; }
    /// <summary>A light theme's dark twin, whose colors its players take; null for a dark theme.</summary>
    public string? Twin { get; }
    public IReadOnlyDictionary<string, Rgba> Colors { get; }
    public IReadOnlyCollection<string> Follows { get; }
}

/// <summary>An accent of themes.json: the colors it gives the tokens a dark theme follows, and a light one's.</summary>
public sealed class AccentSpec
{
    internal AccentSpec(string id, IReadOnlyDictionary<string, Rgba> dark, IReadOnlyDictionary<string, Rgba> light)
    {
        Id = id;
        DarkColors = dark;
        LightColors = light;
    }

    public string Id { get; }
    public IReadOnlyDictionary<string, Rgba> DarkColors { get; }
    public IReadOnlyDictionary<string, Rgba> LightColors { get; }
}

/// <summary>The theme and accent this TV shows; a null accent is the theme's own (Default).</summary>
public sealed record ThemePick(string Theme, string? Accent)
{
    /// <summary>Midnight in its own accent: a TV that picked none, as the app has always looked.</summary>
    public static ThemePick Default { get; } = new ThemePick(Themes.Default, null);

    /// <summary>The pick kept on the TV, ids it doesn't know as Midnight and the theme's own.</summary>
    public static ThemePick Of(string? theme, string? accent) => new ThemePick(Themes.Supported(theme), Themes.SupportedAccent(accent));
}

/// <summary>
/// Pley's themes and accents (docs/THEMES.md), read from packages/themes/themes.json, which is built
/// into Core as it is: the same file every app's tests hold their colors to. A token's color is the
/// accent's, from its set for the theme's kind, when the theme follows that token, and the theme's
/// own otherwise (the web's <c>colorOf</c>).
/// </summary>
public static class Themes
{
    /// <summary>Pley's own, for a TV that picked none or one this app doesn't know.</summary>
    public const string Default = "midnight";

    /// <summary>Every color a theme sets, as themes.json names them.</summary>
    public static readonly IReadOnlyList<string> Tokens = new[]
    {
        "bg", "surface", "surface-2", "surface-3", "border", "border-strong", "fg", "muted", "subtle",
        "accent", "accent-hover", "accent-fg", "success", "danger", "danger-fg", "warning", "info", "shade",
        "brand-light", "brand", "brand-deep", "brand-far", "brand-tile",
    };

    static readonly Lazy<(IReadOnlyList<ThemeSpec> Themes, IReadOnlyList<AccentSpec> Accents)> Spec = new Lazy<(IReadOnlyList<ThemeSpec>, IReadOnlyList<AccentSpec>)>(Read);

    /// <summary>The themes in the picker's order: themes.json's.</summary>
    public static IReadOnlyList<ThemeSpec> All => Spec.Value.Themes;

    /// <summary>The accents in the picker's order, after Default (the theme's own).</summary>
    public static IReadOnlyList<AccentSpec> Accents => Spec.Value.Accents;

    public static IReadOnlyList<string> Ids => All.Select(t => t.Id).ToList();

    public static IReadOnlyList<string> AccentIds => Accents.Select(a => a.Id).ToList();

    /// <summary><paramref name="id"/> when it is a theme's, else Midnight.</summary>
    public static string Supported(string? id) => All.Any(t => t.Id == id) ? id! : Default;

    /// <summary><paramref name="id"/> when it is an accent's, else null: the theme's own.</summary>
    public static string? SupportedAccent(string? id) => Accents.Any(a => a.Id == id) ? id : null;

    public static ThemeSpec Of(string id) => All.FirstOrDefault(t => t.Id == id) ?? All.First(t => t.Id == Default);

    /// <summary>A token's color in <paramref name="theme"/> under <paramref name="accent"/> (null: the theme's own).</summary>
    public static Rgba ColorOf(string theme, string? accent, string token)
    {
        var t = Of(theme);
        var a = accent == null ? null : Accents.FirstOrDefault(x => x.Id == accent);
        if (a != null && t.Follows.Contains(token) && (t.Dark ? a.DarkColors : a.LightColors).TryGetValue(token, out var color)) return color;
        return t.Colors[token];
    }

    /// <summary>Built into Core as Pley.Tv.Core.themes.json (the .csproj takes it from packages/themes).</summary>
    static (IReadOnlyList<ThemeSpec>, IReadOnlyList<AccentSpec>) Read()
    {
        using var stream = typeof(Themes).Assembly.GetManifestResourceStream("Pley.Tv.Core.themes.json")
            ?? throw new InvalidOperationException("themes.json isn't built in");
        using var reader = new StreamReader(stream, Encoding.UTF8);
        return Parse(reader.ReadToEnd());
    }

    /// <summary>themes.json's themes and accents, in its order.</summary>
    internal static (IReadOnlyList<ThemeSpec>, IReadOnlyList<AccentSpec>) Parse(string json)
    {
        using var doc = JsonDocument.Parse(json);
        static Dictionary<string, Rgba> Colors(JsonElement e)
        {
            var colors = new Dictionary<string, Rgba>(StringComparer.Ordinal);
            foreach (var p in e.EnumerateObject())
                colors[p.Name] = Rgba.Parse(p.Value.GetString()) ?? throw new FormatException($"themes.json: {p.Name} is {p.Value}");
            return colors;
        }
        var themes = doc.RootElement.GetProperty("themes").EnumerateArray().Select(t => new ThemeSpec(
            t.GetProperty("id").GetString()!,
            t.GetProperty("kind").GetString() != "light",
            t.GetProperty("twin").ValueKind == JsonValueKind.String ? t.GetProperty("twin").GetString() : null,
            Colors(t.GetProperty("colors")),
            new HashSet<string>(t.GetProperty("follows").EnumerateArray().Select(f => f.GetString()!), StringComparer.Ordinal))).ToList();
        var accents = doc.RootElement.GetProperty("accents").EnumerateArray().Select(a => new AccentSpec(
            a.GetProperty("id").GetString()!, Colors(a.GetProperty("dark")), Colors(a.GetProperty("light")))).ToList();
        return (themes, accents);
    }
}

/// <summary>
/// The app's colors by what they are for (the Android app's PleyColors, which the Tizen app's
/// PleyColors draws), in a theme under an accent: each one a token of themes.json, or a token at
/// a set strength where the app draws a wash of it.
/// </summary>
public sealed class PleyPalette
{
    PleyPalette(string theme, string? accent)
    {
        ThemeId = theme;
        AccentId = accent;
        Rgba C(string token) => Themes.ColorOf(theme, accent, token);
        Dark = Themes.Of(theme).Dark;
        Bg = C("bg");
        Surface = C("surface");
        Surface2 = C("surface-2");
        Surface3 = C("surface-3");
        Surface4 = C("border");
        Outline = C("border-strong");
        Fg = C("fg");
        Muted = C("muted");
        Subtle = C("subtle");
        Control = Subtle;
        Disabled = Subtle.Times(DisabledAlpha);
        Accent = C("accent");
        AccentBright = C("accent-hover");
        OnAccent = C("accent-fg");
        BrandLight = C("brand-light");
        Brand = C("brand");
        BrandDeep = C("brand-deep");
        BrandFar = C("brand-far");
        BrandTile = C("brand-tile");
        Success = C("success");
        Danger = C("danger");
        DangerFg = C("danger-fg");
        Warning = C("warning");
        Info = C("info");
        Shade = C("shade");
        OnArt = Bg.Times(0.8f);
        TileHairline = Fg.Times(0.06f);
        Scrim = Shade.Times(ScrimAlpha);
    }

    /// <summary>Disabled words and edges: Subtle at this strength.</summary>
    public const float DisabledAlpha = 0.75f;

    /// <summary>The dim behind dialogs, sheets and panels: the theme's shade at this strength (the web's bg-shade/60).</summary>
    public const float ScrimAlpha = 0.6f;

    public string ThemeId { get; }
    /// <summary>The accent picked; null for the theme's own.</summary>
    public string? AccentId { get; }
    public bool Dark { get; }

    public Rgba Bg { get; }
    /// <summary>The expanded rail, settings groups.</summary>
    public Rgba Surface { get; }
    /// <summary>Tile wells, inputs, dialogs, panels, skeletons.</summary>
    public Rgba Surface2 { get; }
    /// <summary>Unfocused buttons and chips, the current page in lists.</summary>
    public Rgba Surface3 { get; }
    /// <summary>Dividers, hairlines, the progress track: <c>border</c>.</summary>
    public Rgba Surface4 { get; }
    /// <summary>Decorative borders: <c>border-strong</c>.</summary>
    public Rgba Outline { get; }
    /// <summary>Edges of interactive things (input outlines, empty PIN dots, outlined pills): <c>subtle</c>.</summary>
    public Rgba Control { get; }
    public Rgba Fg { get; }
    public Rgba Muted { get; }
    public Rgba Subtle { get; }
    public Rgba Disabled { get; }
    /// <summary>Focus fills, progress, checks: <c>accent</c>.</summary>
    public Rgba Accent { get; }
    /// <summary>The focus ring: <c>accent-hover</c>.</summary>
    public Rgba AccentBright { get; }
    /// <summary>Words and icons on the accent and on the ring's color: <c>accent-fg</c>.</summary>
    public Rgba OnAccent { get; }
    public Rgba BrandLight { get; }
    public Rgba Brand { get; }
    public Rgba BrandDeep { get; }
    public Rgba BrandFar { get; }
    public Rgba BrandTile { get; }
    public Rgba Success { get; }
    /// <summary>Danger as words and as a fill, whose words are <see cref="DangerFg"/>.</summary>
    public Rgba Danger { get; }
    public Rgba DangerFg { get; }
    /// <summary>Requests waiting for approval; words on it are <see cref="Bg"/>.</summary>
    public Rgba Warning { get; }
    public Rgba Info { get; }
    /// <summary>Shadows.</summary>
    public Rgba Shade { get; }
    /// <summary>Badges, the watched tick and round buttons on art: Bg at 80 %.</summary>
    public Rgba OnArt { get; }
    /// <summary>The hairline inside a tile's well: Fg at 6 %.</summary>
    public Rgba TileHairline { get; }
    /// <summary>Behind dialogs, sheets and panels.</summary>
    public Rgba Scrim { get; }

    static readonly Dictionary<(string, string?), PleyPalette> Made = new Dictionary<(string, string?), PleyPalette>();

    /// <summary>The app's colors in <paramref name="theme"/> under <paramref name="accent"/> (null: its own); ids it doesn't know as Midnight and Default.</summary>
    public static PleyPalette Of(string? theme, string? accent)
    {
        var key = (Themes.Supported(theme), Themes.SupportedAccent(accent));
        lock (Made)
        {
            if (!Made.TryGetValue(key, out var palette)) Made[key] = palette = new PleyPalette(key.Item1, key.Item2);
            return palette;
        }
    }

    public static PleyPalette Of(ThemePick pick) => Of(pick.Theme, pick.Accent);

    /// <summary>
    /// The players' colors, and anything drawn over video: a light theme's dark twin's, with the same
    /// accent (its dark set), since a light frame would glare around the picture; a dark theme's own.
    /// </summary>
    public PleyPalette Player => Themes.Of(ThemeId).Twin is { } twin ? Of(twin, AccentId) : this;
}

/// <summary>The Pley mark as an SVG file, in a palette's colors (the web's Logo.tsx): what NUI draws it from.</summary>
public static class LogoMark
{
    /// <summary>The mark's shapes on its 64 × 64 tile: the back and middle posters, and the front one with play cut out.</summary>
    public const string Far = "M46 21.5h3a3 3 0 0 1 3 3v15a3 3 0 0 1-3 3h-3z";
    public const string Deep = "M36 18.5h4.5a3.5 3.5 0 0 1 3.5 3.5v20a3.5 3.5 0 0 1-3.5 3.5H36z";
    public const string Front = "M16 15.5h14a4 4 0 0 1 4 4v25a4 4 0 0 1-4 4H16a4 4 0 0 1-4-4v-25a4 4 0 0 1 4-4zM19 27v10a1 1 0 0 0 1.51.86l8.5-5a1 1 0 0 0 0-1.72l-8.5-5A1 1 0 0 0 19 27z";

    /// <summary>The front poster a gradient from brand-light to brand, the middle brand-deep, the back brand-far, on a brand-tile tile.</summary>
    public static string Svg(PleyPalette p) =>
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 64 64\">"
        + "<defs><linearGradient id=\"front\" x1=\"12\" y1=\"15\" x2=\"34\" y2=\"49\" gradientUnits=\"userSpaceOnUse\">"
        + $"<stop offset=\"0\" stop-color=\"{p.BrandLight.Svg}\"/><stop offset=\"1\" stop-color=\"{p.Brand.Svg}\"/>"
        + "</linearGradient></defs>"
        + $"<rect width=\"64\" height=\"64\" rx=\"15\" fill=\"{p.BrandTile.Svg}\"/>"
        + $"<path fill=\"{p.BrandFar.Svg}\" d=\"{Far}\"/>"
        + $"<path fill=\"{p.BrandDeep.Svg}\" d=\"{Deep}\"/>"
        + $"<path fill=\"url(#front)\" fill-rule=\"evenodd\" d=\"{Front}\"/>"
        + "</svg>\n";

    /// <summary>A file name for the mark in these colors, the same for the same colors: a new one for NUI, which keeps pictures by name.</summary>
    public static string FileName(PleyPalette p) => $"pley-mark-{p.BrandLight}{p.Brand}{p.BrandDeep}{p.BrandFar}{p.BrandTile}.svg".Replace("#", "");
}

/// <summary>The words of Settings' Appearance: the themes' and accents' names and the themes' captions, the web's.</summary>
public static class ThemeText
{
    public static string Title => Strings.T("settings.categories.appearance");
    public static string Theme => Strings.T("settings.appearance.theme");
    public static string ThemeHint => Strings.T("settings.appearance.themeHint");
    public static string Accent => Strings.T("settings.appearance.accent");
    public static string AccentHint => Strings.T("settings.appearance.accentHint");

    /// <summary>"Midnight".</summary>
    public static string Name(string theme) => Strings.T(Key("names", theme));

    /// <summary>"Lights off, popcorn on".</summary>
    public static string About(string theme) => Strings.T(Key("about", theme));

    /// <summary>"Violet"; Default for null, the theme's own.</summary>
    public static string AccentName(string? accent) => accent == null ? Strings.T("settings.appearance.own") : Strings.T(Key("accents", accent));

    /// <summary>A theme's or an accent's words by its id: ThemeTests holds every id to them in every language.</summary>
    static string Key(string group, string id) => "settings.appearance." + group + "." + id;
}
