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

    static readonly Dictionary<string, (string Ascending, string Descending)> SortNotes = new Dictionary<string, (string, string)>
    {
        ["titleSort"] = ("By title", "By title, Z to A"),
        ["year"] = ("Oldest first", "Newest first"),
        ["originallyAvailableAt"] = ("Earliest release first", "Latest release first"),
        ["addedAt"] = ("First added first", "Recently added first"),
        ["rating"] = ("Lowest rated first", "Best rated first"),
        ["audienceRating"] = ("Lowest rated first", "Best rated first"),
        ["userRating"] = ("Your lowest rated first", "Your best rated first"),
        ["lastViewedAt"] = ("Least recently played first", "Recently played first"),
        ["duration"] = ("Shortest first", "Longest first"),
        ["viewCount"] = ("Least played first", "Most played first"),
    };

    /// <summary>The library page's filters and order as a smart filter: what "Save as…" saves.</summary>
    public static string Of(LibraryParams p)
    {
        var parts = new List<(string Key, string Value)>();
        if (p.Genre != null) parts.Add(("genre", p.Genre));
        if (p.Year != null) parts.Add(("year", p.Year.Value.ToString(CultureInfo.InvariantCulture)));
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
                case "genre": words.Add($"Genre: {value}"); break;
                case "label": words.Add($"Label: {value}"); break;
                case "studio": words.Add($"Studio: {value}"); break;
                case "contentRating": words.Add($"Rated {value}"); break;
                case "unwatched": words.Add(value is "0" or "false" ? "Watched" : "Unwatched"); break;
                case "inProgress": words.Add(value is "0" or "false" ? "Not in progress" : "In progress"); break;
                case "year": words.Add($"From {value}"); break;
                case "year>>": words.Add($"After {value}"); break;
                case "year<<": words.Add($"Before {value}"); break;
                case "decade": words.Add($"The {value}s"); break;
                case "sort": words.Add(SortNote(value)); break;
                case "limit": words.Add($"At most {value}"); break;
                // Plumbing, not part of what is found.
                case "type":
                case "includeGuids":
                    break;
                default: words.Add($"{key}: {value}"); break;
            }
        }
        return words;
    }

    static string SortNote(string value)
    {
        var first = value.Split(',')[0];
        var colon = first.IndexOf(':');
        var field = colon < 0 ? first : first.Substring(0, colon);
        var direction = colon < 0 ? null : first.Substring(colon + 1);
        // python-plexapi may name the kind first: `movie.year`.
        var dot = field.IndexOf('.');
        var name = dot < 0 ? field : field.Substring(dot + 1);
        if (!SortNotes.TryGetValue(name, out var notes)) return $"Sorted by {name}";
        return direction == "desc" ? notes.Descending : notes.Ascending;
    }
}
