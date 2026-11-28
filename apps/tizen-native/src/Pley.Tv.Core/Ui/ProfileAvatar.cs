using System;
using System.Collections.Generic;
using System.Globalization;

namespace Pley.Tv.Core.Ui;

/// <summary>
/// A profile's avatar without its picture, as every app draws it (docs/THEMES.md, "Profile
/// colors"; the web's Avatar.tsx; packages/tv/test/fixtures/avatar.json pins it): the circle's
/// color, the initial on it, and the initial's ink. These are the profile's colors, not the
/// theme's: they stay the same under every theme and accent.
/// </summary>
/// <param name="Color">The circle, 0xRRGGBB.</param>
/// <param name="Initial">The name's first letter in capitals; "?" for a blank name.</param>
/// <param name="Ink">The initial's color, 0xRRGGBB: <see cref="DarkInk"/> or <see cref="LightInk"/>.</param>
public sealed record ProfileAvatar(uint Color, string Initial, uint Ink)
{
    /// <summary>The colors the web offers for profiles; one is picked from the name of a profile without its own.</summary>
    public static readonly IReadOnlyList<uint> Colors = new uint[] { 0x38BDF8, 0xA78BFA, 0xF472B6, 0xFB923C, 0xFACC15, 0x4ADE80, 0x2DD4BF, 0xF87171 };

    /// <summary>The initial's two colors: Midnight's background on light circles, white on dark ones.</summary>
    public const uint DarkInk = 0x0B0D10, LightInk = 0xFFFFFF;

    /// <summary>The avatar of a profile named <paramref name="name"/> whose avatar_color is <paramref name="avatarColor"/>.</summary>
    public static ProfileAvatar Of(string name, string? avatarColor)
    {
        var color = ColorOf(name, avatarColor);
        return new ProfileAvatar(color, InitialOf(name), InkOn(color));
    }

    /// <summary>
    /// The profile's own color (#rgb or #rrggbb, any case), or, without a valid one, one of
    /// <see cref="Colors"/> picked from the name, so the profile has the same color on every
    /// screen and app: its code points folded as h = h * 31 + code point, in 32 bits, from 0.
    /// </summary>
    public static uint ColorOf(string name, string? avatarColor)
    {
        if (Hex(avatarColor) is { } own) return own;
        uint hash = 0;
        for (var i = 0; i < name.Length; i++)
        {
            // A step per code point, as the web's for…of: a surrogate pair is one, a lone surrogate its own.
            int point = name[i];
            if (char.IsHighSurrogate(name[i]) && i + 1 < name.Length && char.IsLowSurrogate(name[i + 1]))
                point = char.ConvertToUtf32(name[i], name[++i]);
            unchecked
            {
                hash = hash * 31 + (uint)point;
            }
        }
        return Colors[(int)(hash % (uint)Colors.Count)];
    }

    /// <summary>The initial's color on <paramref name="color"/>: the ink with the higher WCAG contrast with it, dark on a tie.</summary>
    public static uint InkOn(uint color)
    {
        var on = Luminance(color) + 0.05;
        var dark = on / (Luminance(DarkInk) + 0.05);
        var light = (Luminance(LightInk) + 0.05) / on;
        return dark >= light ? DarkInk : LightInk;
    }

    /// <summary>The trimmed name's first code point in capitals; "?" for a blank name.</summary>
    public static string InitialOf(string name)
    {
        var trimmed = name.Trim();
        if (trimmed.Length == 0) return "?";
        var length = char.IsSurrogatePair(trimmed, 0) ? 2 : 1;
        return trimmed.Substring(0, length).ToUpperInvariant();
    }

    /// <summary>A color as the server keeps it, #rgb or #rrggbb in any case, as 0xRRGGBB; null for anything else.</summary>
    static uint? Hex(string? color)
    {
        if (color == null || color.Length is not (4 or 7) || color[0] != '#') return null;
        var digits = color.Substring(1);
        foreach (var c in digits)
            if (!Uri.IsHexDigit(c)) return null;
        if (digits.Length == 3) digits = new string(new[] { digits[0], digits[0], digits[1], digits[1], digits[2], digits[2] });
        return uint.Parse(digits, NumberStyles.HexNumber, CultureInfo.InvariantCulture);
    }

    /// <summary>WCAG's relative luminance of 0xRRGGBB.</summary>
    static double Luminance(uint rgb)
    {
        static double Linear(uint channel)
        {
            var c = channel / 255.0;
            return c <= 0.04045 ? c / 12.92 : Math.Pow((c + 0.055) / 1.055, 2.4);
        }
        return 0.2126 * Linear(rgb >> 16 & 0xff) + 0.7152 * Linear(rgb >> 8 & 0xff) + 0.0722 * Linear(rgb & 0xff);
    }
}
