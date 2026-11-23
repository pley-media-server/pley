using System;
using System.Collections.Generic;

namespace Pley.Tv.Core.Ui;

/// <summary>
/// The picture shown while artwork loads and when there is none (the Android app's Placeholder, the
/// web's <c>placeholderGradient</c> in apps/web/src/lib/images.ts): a 135° gradient from
/// hsl(h, 30 %, 22 %) to hsl(h + 40, 35 %, 12 %), where h comes from a hash of the title, so each
/// title has its own color. Once no picture will come, Pley's mark is on it, with a glyph for the
/// title's kind of library cut out of its front poster.
/// </summary>
public static class Placeholder
{
    /// <summary>The web's hue for <paramref name="seed"/>: <c>h = h * 31 + code</c> over UTF-16 units, unsigned 32-bit, mod 360.</summary>
    public static int Hue(string seed)
    {
        ulong h = 0;
        foreach (var ch in seed) h = (h * 31 + ch) & 0xFFFFFFFFUL;
        return (int)(h % 360);
    }

    /// <summary>The gradient's two ends for <paramref name="seed"/>, as 0xRRGGBB.</summary>
    public static (uint From, uint To) Colors(string seed)
    {
        var hue = Hue(seed);
        return (Hsl(hue, 0.30f, 0.22f), Hsl((hue + 40) % 360, 0.35f, 0.12f));
    }

    /// <summary>
    /// The glyph cut out of the mark's front poster for each kind of item (the API's kind): its
    /// library's. packages/placeholder-mark/marks.json defines the mark for every app; the app draws
    /// it from shared/res/placeholder-mark-&lt;glyph&gt;.svg, which
    /// scripts/render-placeholder-marks.mjs makes from it, and the tests hold this map and those
    /// files to it.
    /// </summary>
    public static readonly IReadOnlyDictionary<string, string> Kinds = new Dictionary<string, string>(StringComparer.Ordinal)
    {
        ["movie"] = "movies",
        ["show"] = "shows",
        ["season"] = "shows",
        ["episode"] = "shows",
        ["artist"] = "music",
        ["album"] = "music",
        ["track"] = "music",
        ["audiobook"] = "audiobooks",
        ["part"] = "audiobooks",
    };

    /// <summary>The glyph of any other kind (extras, collections): the logo's play.</summary>
    public const string DefaultGlyph = "play";

    /// <summary>The glyph cut out of the mark on the placeholder of an item of <paramref name="kind"/>.</summary>
    public static string Glyph(string kind) => Kinds.TryGetValue(kind, out var glyph) ? glyph : DefaultGlyph;

    /// <summary>How much of a placeholder's shorter side the mark takes, alone and over a title.</summary>
    public const float MarkShare = 0.4f, MarkShareOverTitle = 0.26f;

    /// <summary>The mark's largest side (a backdrop's), and its gap to the title under it, in dp.</summary>
    public const float MarkMax = 120, MarkGap = 10;

    /// <summary>
    /// What a placeholder <paramref name="width"/> by <paramref name="height"/> dp draws on its
    /// gradient. Pley's mark with <paramref name="glyph"/> cut out of it only when given a glyph
    /// (callers give one where the item's name shows by the picture: a tile's caption, a row's
    /// title, a cover's own title) and <paramref name="missing"/>, no picture to come (none given,
    /// or none of its addresses loaded): not while one loads, or a page of tiles scrolling in would
    /// flash marks before their pictures.
    /// <paramref name="titleHeight"/> is the title's height as laid out, null for a placeholder
    /// without one; the mark goes above it, smaller, and is left out where the title leaves it no
    /// room (a long one on a small cover).
    /// </summary>
    public static PlaceholderLayout Layout(float width, float height, bool missing, string? glyph, float? titleHeight = null)
    {
        var side = missing && glyph != null ? MarkSide(width, height, titleHeight) : 0f;
        var gap = side > 0 && titleHeight != null ? MarkGap : 0f;
        var top = (height - side - gap - (titleHeight ?? 0f)) / 2f;
        return new PlaceholderLayout(side > 0 ? glyph : null, side, top, titleHeight != null, top + side + gap);
    }

    /// <summary>
    /// A tile's placeholder: no title, as the caption under the tile names it, and Pley's mark with
    /// its kind's glyph when <paramref name="model"/> has a kind (people have none) and no picture,
    /// or its picture <paramref name="failed"/> to load.
    /// </summary>
    public static PlaceholderLayout Tile(TileModel model, TileKind kind, bool failed = false) =>
        Layout(kind.Width, kind.Height, missing: model.Images.Count == 0 || failed, model.MarkGlyph);

    static float MarkSide(float width, float height, float? titleHeight)
    {
        var side = Math.Min(width, height) * (titleHeight == null ? MarkShare : MarkShareOverTitle);
        var room = height - (titleHeight ?? 0f) - MarkGap;
        return titleHeight != null && side > room * 0.9f ? 0f : Math.Min(side, MarkMax);
    }

    /// <summary>CSS's hsl(), hue in degrees, saturation and lightness 0 to 1, as 0xRRGGBB.</summary>
    public static uint Hsl(float hue, float saturation, float lightness)
    {
        var c = (1f - Math.Abs(2f * lightness - 1f)) * saturation;
        var hp = hue % 360f / 60f;
        var x = c * (1f - Math.Abs(hp % 2f - 1f));
        var (r, g, b) = hp switch
        {
            < 1f => (c, x, 0f),
            < 2f => (x, c, 0f),
            < 3f => (0f, c, x),
            < 4f => (0f, x, c),
            < 5f => (x, 0f, c),
            _ => (c, 0f, x),
        };
        var m = lightness - c / 2f;
        uint Channel(float v) => (uint)Math.Round(Math.Max(0f, Math.Min(1f, v + m)) * 255f);
        return Channel(r) << 16 | Channel(g) << 8 | Channel(b);
    }
}

/// <summary>
/// What a placeholder draws on its gradient, in dp from its top: Pley's mark with
/// <see cref="Glyph"/> cut out of it (none when null), <see cref="MarkSide"/> square and centered
/// across, and its title, when it has one, from <see cref="TitleTop"/>.
/// </summary>
public readonly record struct PlaceholderLayout(string? Glyph, float MarkSide, float MarkTop, bool Title, float TitleTop)
{
    public bool Mark => Glyph != null;
}
