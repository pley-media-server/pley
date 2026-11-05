using System;
using System.Collections.Generic;
using System.Globalization;
using System.Linq;

namespace Pley.Tv.Core;

/// <summary>
/// The languages Pley is in, and which one the app shows (docs/API.md, "Language"): the account's
/// own, else the server's, else the first of the TV's languages that Pley has, else English. The
/// web Samsung app picks the same (packages/tv/test/fixtures/language.json).
/// </summary>
public static class Languages
{
    public const string English = "en";

    /// <summary>ISO 639-1 codes, English first: the fallback.</summary>
    public static readonly IReadOnlyList<string> All = new[] { "en", "fr", "es" };

    /// <summary>The language Pley has for a tag ("fr-CA", "fr_FR" → "fr"); null for one it hasn't, or none.</summary>
    public static string? Supported(string? tag)
    {
        if (string.IsNullOrWhiteSpace(tag)) return null;
        var code = tag!.Trim().Split('-', '_')[0].ToLowerInvariant();
        return All.FirstOrDefault(l => l == code);
    }

    /// <summary>The language to show: <paramref name="account"/>'s, else <paramref name="server"/>'s, else the first of <paramref name="device"/>'s Pley has, else English.</summary>
    public static string Pick(string? account, string? server, IEnumerable<string> device) =>
        Supported(account) ?? Supported(server) ?? device.Select(Supported).FirstOrDefault(l => l != null) ?? English;

    /// <summary>A language's name in itself, as the Language row lists it: "Français".</summary>
    public static string NameOf(string language) => language switch
    {
        "fr" => "Français",
        "es" => "Español",
        _ => "English",
    };

    /// <summary>
    /// What dates and numbers are written in for <paramref name="language"/>: the first of the TV's
    /// tags in that language (its region's, so "en-GB" keeps its 24-hour clock, "fr-CA" its own
    /// ways), else the language's own.
    /// </summary>
    public static CultureInfo CultureOf(string language, IEnumerable<string> device)
    {
        foreach (var tag in device)
        {
            if (Supported(tag) != language) continue;
            if (Culture(tag) is { } culture) return culture;
        }
        return Culture(language) ?? CultureInfo.InvariantCulture;
    }

    /// <summary>The culture of a tag as Tizen or .NET writes it ("en_GB.UTF-8", "en-GB"); null when the TV has none for it.</summary>
    static CultureInfo? Culture(string tag)
    {
        var name = tag.Trim();
        var end = name.IndexOfAny(new[] { '.', '@' });
        if (end >= 0) name = name.Substring(0, end);
        try
        {
            return CultureInfo.GetCultureInfo(name.Replace('_', '-'));
        }
        catch (CultureNotFoundException)
        {
            return null;
        }
        catch (ArgumentException)
        {
            return null;
        }
    }
}
