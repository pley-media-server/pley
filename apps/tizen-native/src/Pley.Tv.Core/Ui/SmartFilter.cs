using System.Collections.Generic;
using System.Globalization;
using System.Linq;
using Pley.Tv.Core.Api;

namespace Pley.Tv.Core.Ui;

/// <summary>
/// Smart collection and playlist filters: a library listing in Plex's syntax
/// (<c>genre=Comedy&amp;unwatched=1&amp;sort=year:desc</c>, docs/COMPATIBILITY.md), as the Android
/// app's SmartFilter ports apps/web/src/lib/smartFilter.ts.
/// </summary>
public static class SmartFilter
{
    static readonly Dictionary<string, string> PlexSort = new Dictionary<string, string>
    {
        [LibrarySort.Title] = "titleSort",
        [LibrarySort.Year] = "year",
        [LibrarySort.Added] = "addedAt",
        [LibrarySort.Rating] = "rating",
        [LibrarySort.UserRating] = "userRating",
        [LibrarySort.Release] = "originallyAvailableAt",
        [LibrarySort.Duration] = "duration",
        [LibrarySort.LastPlayed] = "lastViewedAt",
        [LibrarySort.Bitrate] = "mediaBitrate",
    };

    /// <summary>The library page's filters and order as a smart filter: what "Save as…" saves.</summary>
    public static string Of(LibraryParams p)
    {
        var parts = new List<(string Key, string Value)>();
        if (p.Genre != null) parts.Add(("genre", p.Genre));
        if (p.Year != null) parts.Add(("year", p.Year.Value.ToString(CultureInfo.InvariantCulture)));
        // Plex's own field, and Pley's.
        if (p.Resolution != null) parts.Add(("resolution", p.Resolution));
        if (p.DynamicRange != null) parts.Add(("dynamicRange", p.DynamicRange));
        if (p.Watched != null) parts.Add(("unwatched", p.Watched.Value ? "0" : "1"));
        var field = PlexSort.TryGetValue(p.Sort, out var f) ? f : PlexSort[LibrarySort.Title];
        parts.Add(("sort", p.EffectiveOrder == SortOrder.Descending ? field + ":desc" : field));
        return string.Join("&", parts.Select(x => $"{LibraryParams.Encode(x.Key)}={LibraryParams.Encode(x.Value)}"));
    }

    /// <summary>What a smart filter finds, in words: "Genre: Comedy", "Unwatched", "Newest first"…</summary>
    public static IReadOnlyList<string> Describe(string filter)
    {
        var words = new List<string>();
        foreach (var part in filter.Split('&'))
        {
            if (part.Length == 0) continue;
            var at = part.IndexOf('=');
            var key = LibraryParams.Decode(at < 0 ? part : part.Substring(0, at));
            var value = at < 0 ? "" : LibraryParams.Decode(part.Substring(at + 1));
            switch (key)
            {
                case "genre": words.Add(Strings.T("library.smartFilter.genre", ("value", value))); break;
                case "label": words.Add(Strings.T("library.smartFilter.label", ("value", value))); break;
                case "studio": words.Add(Strings.T("library.smartFilter.studio", ("value", value))); break;
                case "contentRating": words.Add(Strings.T("library.smartFilter.rated", ("value", value))); break;
                case "unwatched": words.Add(value is "0" or "false" ? Strings.T("library.smartFilter.watched") : Strings.T("library.smartFilter.unwatched")); break;
                case "inProgress":
                    words.Add(value is "0" or "false" ? Strings.T("library.smartFilter.notInProgress") : Strings.T("library.smartFilter.inProgress"));
                    break;
                case "year": words.Add(Strings.T("library.smartFilter.from", ("value", value))); break;
                case "year>>": words.Add(Strings.T("library.smartFilter.after", ("value", value))); break;
                case "year<<": words.Add(Strings.T("library.smartFilter.before", ("value", value))); break;
                case "decade": words.Add(Strings.T("library.smartFilter.decade", ("value", value))); break;
                case "resolution": words.Add(LibraryText.ResolutionLabel(value) ?? Other(key, value)); break;
                case "dynamicRange": words.Add(LibraryText.DynamicRangeLabel(value) ?? Other(key, value)); break;
                case "sort": words.Add(SortNote(value)); break;
                case "limit": words.Add(Strings.T("library.smartFilter.limit", ("value", value))); break;
                // Plumbing, not part of what is found.
                case "type":
                case "includeGuids":
                    break;
                default: words.Add(Other(key, value)); break;
            }
        }
        return words;
    }

    /// <summary>A field without words of its own, as it is named: "mood: calm".</summary>
    static string Other(string key, string value) => Strings.T("library.smartFilter.other", ("name", key), ("value", value));

    static string SortNote(string value)
    {
        var first = value.Split(',')[0];
        var colon = first.IndexOf(':');
        var field = colon < 0 ? first : first.Substring(0, colon);
        var direction = colon < 0 ? null : first.Substring(colon + 1);
        // python-plexapi may name the kind first: `movie.year`.
        var dot = field.IndexOf('.');
        var name = dot < 0 ? field : field.Substring(dot + 1);
        return SortWords(name, direction == "desc") ?? Strings.T("library.smartFilter.sortedBy", ("name", name));
    }

    /// <summary>What sorting by a Plex field says, one way or the other: "Newest first"; null for a field without words of its own.</summary>
    static string? SortWords(string field, bool descending) => (field, descending) switch
    {
        ("titleSort", false) => Strings.T("library.smartFilter.sort.titleSort.asc"),
        ("titleSort", true) => Strings.T("library.smartFilter.sort.titleSort.desc"),
        ("year", false) => Strings.T("library.smartFilter.sort.year.asc"),
        ("year", true) => Strings.T("library.smartFilter.sort.year.desc"),
        ("originallyAvailableAt", false) => Strings.T("library.smartFilter.sort.originallyAvailableAt.asc"),
        ("originallyAvailableAt", true) => Strings.T("library.smartFilter.sort.originallyAvailableAt.desc"),
        ("addedAt", false) => Strings.T("library.smartFilter.sort.addedAt.asc"),
        ("addedAt", true) => Strings.T("library.smartFilter.sort.addedAt.desc"),
        ("rating" or "audienceRating", false) => Strings.T("library.smartFilter.sort.rating.asc"),
        ("rating" or "audienceRating", true) => Strings.T("library.smartFilter.sort.rating.desc"),
        ("userRating", false) => Strings.T("library.smartFilter.sort.userRating.asc"),
        ("userRating", true) => Strings.T("library.smartFilter.sort.userRating.desc"),
        ("lastViewedAt", false) => Strings.T("library.smartFilter.sort.lastViewedAt.asc"),
        ("lastViewedAt", true) => Strings.T("library.smartFilter.sort.lastViewedAt.desc"),
        ("duration", false) => Strings.T("library.smartFilter.sort.duration.asc"),
        ("duration", true) => Strings.T("library.smartFilter.sort.duration.desc"),
        ("viewCount", false) => Strings.T("library.smartFilter.sort.viewCount.asc"),
        ("viewCount", true) => Strings.T("library.smartFilter.sort.viewCount.desc"),
        _ => null,
    };
}
