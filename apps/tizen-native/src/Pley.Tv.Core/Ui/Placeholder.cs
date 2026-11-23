using System;
using System.Collections.Generic;

namespace Pley.Tv.Core.Ui;

/// <summary>
/// The picture shown while artwork loads and when there is none (the Android app's Placeholder, the
/// web's <c>placeholderGradient</c> in apps/web/src/lib/images.ts): a 135° gradient from
/// hsl(h, 30 %, 22 %) to hsl(h + 40, 35 %, 12 %), where h comes from a hash of the title, so each
/// title has its own color. Once no picture will come, Pley's mark is on it.
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
    /// Pley's mark on a placeholder whose picture won't come (the Android app's PleyMark.Layers):
    /// the logo's three posters (shared/res/pley-mark.svg) on its 64 by 64 grid, back to front, each
    /// filled white at its alpha, the front one even-odd. The app draws them from
    /// shared/res/pley-mark-placeholder.svg, whose paths and alphas the tests hold to these.
    /// </summary>
    public static readonly IReadOnlyList<MarkLayer> MarkLayers = new[]
    {
        new MarkLayer("M46 21.5h3a3 3 0 0 1 3 3v15a3 3 0 0 1-3 3h-3z", 0.16f, false),
        new MarkLayer("M36 18.5h4.5a3.5 3.5 0 0 1 3.5 3.5v20a3.5 3.5 0 0 1-3.5 3.5H36z", 0.24f, false),
        new MarkLayer("M16 15.5h14a4 4 0 0 1 4 4v25a4 4 0 0 1-4 4H16a4 4 0 0 1-4-4v-25a4 4 0 0 1 4-4zM19 27v10a1 1 0 0 0 1.51.86l8.5-5a1 1 0 0 0 0-1.72l-8.5-5A1 1 0 0 0 19 27z", 0.36f, true),
    };

    /// <summary>How much of a placeholder's shorter side the mark takes, alone and over a title.</summary>
    public const float MarkShare = 0.4f, MarkShareOverTitle = 0.26f;

    /// <summary>The mark's largest side (a backdrop's), and its gap to the title under it, in dp.</summary>
    public const float MarkMax = 120, MarkGap = 10;

    /// <summary>
    /// What a placeholder <paramref name="width"/> by <paramref name="height"/> dp draws on its
    /// gradient. Pley's mark only when <paramref name="missing"/>, no picture to come (none given, or
    /// none of its addresses loaded): not while one loads, or a page of tiles scrolling in would
    /// flash marks before their pictures. <paramref name="titleHeight"/> is the title's height as
    /// laid out, null for a placeholder without one; the mark goes above it, smaller, and is left
    /// out where the title leaves it no room (a long one on a small cover).
    /// </summary>
    public static PlaceholderLayout Layout(float width, float height, bool missing, float? titleHeight = null)
    {
        var side = missing ? MarkSide(width, height, titleHeight) : 0f;
        var gap = side > 0 && titleHeight != null ? MarkGap : 0f;
        var top = (height - side - gap - (titleHeight ?? 0f)) / 2f;
        return new PlaceholderLayout(side, top, titleHeight != null, top + side + gap);
    }

    /// <summary>
    /// A tile's placeholder: no title, as the caption under the tile names it, and Pley's mark when
    /// <paramref name="model"/> has no picture or its picture <paramref name="failed"/> to load.
    /// </summary>
    public static PlaceholderLayout Tile(TileModel model, TileKind kind, bool failed = false) =>
        Layout(kind.Width, kind.Height, missing: model.Images.Count == 0 || failed);

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

/// <summary>One poster of Pley's mark: its path on the 64 by 64 grid, the white's alpha, and whether it fills even-odd.</summary>
public readonly record struct MarkLayer(string Path, float Alpha, bool EvenOdd);

/// <summary>
/// What a placeholder draws on its gradient, in dp from its top: Pley's mark <see cref="MarkSide"/>
/// square, centered across (none when 0), and its title, when it has one, from <see cref="TitleTop"/>.
/// </summary>
public readonly record struct PlaceholderLayout(float MarkSide, float MarkTop, bool Title, float TitleTop)
{
    public bool Mark => MarkSide > 0;
}
