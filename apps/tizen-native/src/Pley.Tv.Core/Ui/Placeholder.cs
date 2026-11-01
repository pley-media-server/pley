using System;

namespace Pley.Tv.Core.Ui;

/// <summary>
/// The picture shown while artwork loads and when there is none (the Android app's Placeholder, the
/// web's <c>placeholderGradient</c> in apps/web/src/lib/images.ts): a 135° gradient from
/// hsl(h, 30 %, 22 %) to hsl(h + 40, 35 %, 12 %), where h comes from a hash of the title, so each
/// title has its own colour.
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
