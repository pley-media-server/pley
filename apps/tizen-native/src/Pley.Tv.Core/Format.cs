using System;
using System.Collections.Generic;
using System.Globalization;
using System.Linq;
using Pley.Tv.Core.Api;

namespace Pley.Tv.Core;

/// <summary>
/// Times, titles, counts and badges as people read them: the Android app's model/format
/// (Durations.kt, Labels.kt, Media.kt), which says the same as the web, in the language shown
/// (Locales/*/format.json).
/// </summary>
public static class Format
{
    const long MinuteMs = 60_000;

    /// <summary>Whole minutes, rounding half a minute up as Kotlin's Math.round does.</summary>
    static long WholeMinutes(long ms) => (long)Math.Floor(ms / (double)MinuteMs + 0.5);

    static long RoundHalfUp(double v) => (long)Math.Floor(v + 0.5);

    /// <summary>6720000 → "1h 52m", 2700000 → "45m", 29000 → "29s": whole minutes from 30 s up, seconds below. Empty without a duration.</summary>
    public static string Duration(long? ms)
    {
        if (ms == null || ms <= 0) return "";
        var totalMin = WholeMinutes(ms.Value);
        if (totalMin < 1) return Strings.T("format.duration.seconds", ("n", Math.Max(1, RoundHalfUp(ms.Value / 1000.0))));
        var h = totalMin / 60;
        var m = totalMin % 60;
        if (h == 0) return Strings.T("format.duration.minutes", ("n", m));
        return m != 0 ? Strings.T("format.duration.hoursMinutes", ("hours", h), ("minutes", m)) : Strings.T("format.duration.hours", ("n", h));
    }

    /// <summary>A runtime as the TV screens show it: "58 min" under an hour, "1h 52m" from an hour, "29s" below half a minute.</summary>
    public static string Runtime(long? ms)
    {
        if (ms == null || ms <= 0) return "";
        var totalMin = WholeMinutes(ms.Value);
        return totalMin is >= 1 and <= 59 ? Strings.T("format.duration.runtimeMinutes", ("n", totalMin)) : Duration(ms);
    }

    /// <summary>"26 min left", "1h 12m left"; empty without a duration or once nothing is left.</summary>
    public static string TimeLeft(long positionMs, long? durationMs)
    {
        if (durationMs == null || durationMs <= 0) return "";
        var left = Runtime(Math.Max(0, durationMs.Value - positionMs));
        return left.Length > 0 ? Strings.T("format.duration.left", ("time", left)) : "";
    }

    /// <summary>A duration for screen readers, in words: "34 minutes", "1 hour 12 minutes", "45 seconds".</summary>
    public static string SpokenDuration(long? ms)
    {
        if (ms == null || ms <= 0) return "";
        var totalMin = WholeMinutes(ms.Value);
        if (totalMin < 1) return Strings.Count("format.spoken.seconds", Math.Max(1, RoundHalfUp(ms.Value / 1000.0)));
        var h = totalMin / 60;
        var m = totalMin % 60;
        if (h == 0) return Strings.Count("format.spoken.minutes", m);
        return m != 0
            ? Strings.T("format.spoken.hoursMinutes", ("hours", Strings.Count("format.spoken.hours", h)), ("minutes", Strings.Count("format.spoken.minutes", m)))
            : Strings.Count("format.spoken.hours", h);
    }

    /// <summary>"34 minutes left", for screen readers; empty once nothing is left.</summary>
    public static string SpokenTimeLeft(long positionMs, long? durationMs)
    {
        if (durationMs == null || durationMs <= 0) return "";
        var left = SpokenDuration(Math.Max(0, durationMs.Value - positionMs));
        return left.Length > 0 ? Strings.T("format.spoken.left", ("time", left)) : "";
    }

    /// <summary>"S1 · E3", "E3", or "" with neither number.</summary>
    public static string EpisodeLabel(int? season, int? episode) => (season, episode) switch
    {
        (int s, int e) => Strings.T("format.episode.short", ("season", s), ("episode", e)),
        (int s, null) => Strings.T("format.episode.seasonShort", ("season", s)),
        (null, int e) => Strings.T("format.episode.episodeShort", ("episode", e)),
        _ => "",
    };

    /// <summary><see cref="EpisodeLabel(int?, int?)"/> of an episode: its season is the parent index.</summary>
    public static string EpisodeLabel(ItemSummary item) => EpisodeLabel(item.ParentIndex, item.Index);

    /// <summary>"Season 2 · Episode 5", for the episode lines of item pages; the parts that are known.</summary>
    public static string EpisodeLongLabel(int? season, int? episode) =>
        string.Join(" · ", new[]
        {
            season != null ? (season == 0 ? Strings.T("format.episode.specials") : Strings.T("format.episode.season", ("number", season.Value))) : null,
            episode != null ? Strings.T("format.episode.episode", ("number", episode.Value)) : null,
        }.Where(s => s != null));

    /// <summary>The short line under a card, by kind: a year, "3 seasons", "S2 · E5", "Daft Punk · 2001"…</summary>
    public static string CardSubtitle(ItemSummary item)
    {
        string Year() => item.Year?.ToString(CultureInfo.InvariantCulture) ?? "";
        switch (item.Kind)
        {
            case "movie":
                return Year();
            case "show":
                if (item.ChildCount is > 0) return Strings.Count("format.count.seasons", item.ChildCount.Value);
                if (item.LeafCount != null) return Strings.Count("format.count.episodes", item.LeafCount.Value);
                return Year();
            case "season":
                return item.LeafCount != null ? Strings.Count("format.count.episodes", item.LeafCount.Value) : "";
            case "episode":
                return EpisodeLabel(item);
            case "artist":
                return item.ChildCount != null ? Strings.Count("format.count.albums", item.ChildCount.Value) : "";
            case "album":
                return string.Join(" · ", new[] { string.IsNullOrEmpty(item.ParentTitle) ? null : item.ParentTitle, item.Year?.ToString(CultureInfo.InvariantCulture) }.Where(s => s != null));
            case "track":
                return string.Join(" · ", new[] { item.GrandparentTitle, item.ParentTitle }.Where(s => !string.IsNullOrEmpty(s)));
            default:
                return "";
        }
    }

    /// <summary>Seconds per unit, and the age below which that unit is used, as the web's table.</summary>
    static readonly (long Below, string Unit, long Seconds)[] RelativeUnits =
    {
        (60, "second", 1),
        (3_600, "minute", 60),
        (86_400, "hour", 3_600),
        (86_400L * 7, "day", 86_400),
        (86_400L * 30, "week", 86_400L * 7),
        (86_400L * 365, "month", 86_400L * 30),
        (long.MaxValue, "year", 86_400L * 365),
    };

    /// <summary>
    /// Unix seconds → "3 minutes ago", "in 5 hours", "yesterday", "last week", "now": the largest
    /// unit that fits, worded as the web's Intl.RelativeTimeFormat words it with numeric "auto",
    /// in the language shown. "never" without a time.
    /// </summary>
    public static string Relative(long? ts, long nowSeconds)
    {
        if (ts == null || ts == 0) return Strings.T("format.relative.never");
        var diff = ts.Value - nowSeconds;
        var age = Math.Abs(diff);
        var (_, unit, seconds) = RelativeUnits.First(u => age < u.Below);
        // Math.round of Kotlin and JavaScript: halves go up.
        var value = (long)Math.Floor(diff / (double)seconds + 0.5);
        var named = (value, unit) switch
        {
            (0, "second") => Strings.T("format.relative.now"),
            (0, "minute") => Strings.T("format.relative.thisMinute"),
            (0, "hour") => Strings.T("format.relative.thisHour"),
            (0, "day") => Strings.T("format.relative.today"),
            (0, "week") => Strings.T("format.relative.thisWeek"),
            (0, "month") => Strings.T("format.relative.thisMonth"),
            (0, "year") => Strings.T("format.relative.thisYear"),
            (-1, "day") => Strings.T("format.relative.yesterday"),
            (-1, "week") => Strings.T("format.relative.lastWeek"),
            (-1, "month") => Strings.T("format.relative.lastMonth"),
            (-1, "year") => Strings.T("format.relative.lastYear"),
            (1, "day") => Strings.T("format.relative.tomorrow"),
            (1, "week") => Strings.T("format.relative.nextWeek"),
            (1, "month") => Strings.T("format.relative.nextMonth"),
            (1, "year") => Strings.T("format.relative.nextYear"),
            _ => null,
        };
        if (named != null) return named;
        var n = Math.Abs(value);
        if (value < 0)
            return unit switch
            {
                "second" => Strings.Count("format.relative.secondsAgo", n),
                "minute" => Strings.Count("format.relative.minutesAgo", n),
                "hour" => Strings.Count("format.relative.hoursAgo", n),
                "day" => Strings.Count("format.relative.daysAgo", n),
                "week" => Strings.Count("format.relative.weeksAgo", n),
                "month" => Strings.Count("format.relative.monthsAgo", n),
                _ => Strings.Count("format.relative.yearsAgo", n),
            };
        return unit switch
        {
            "second" => Strings.Count("format.relative.inSeconds", n),
            "minute" => Strings.Count("format.relative.inMinutes", n),
            "hour" => Strings.Count("format.relative.inHours", n),
            "day" => Strings.Count("format.relative.inDays", n),
            "week" => Strings.Count("format.relative.inWeeks", n),
            "month" => Strings.Count("format.relative.inMonths", n),
            _ => Strings.Count("format.relative.inYears", n),
        };
    }

    /// <summary>How much of an item has been played, 0 to 1.</summary>
    public static float ProgressFraction(long positionMs, long? durationMs)
    {
        if (positionMs <= 0 || durationMs == null || durationMs <= 0) return 0f;
        return (float)Math.Min(1.0, Math.Max(0.0, positionMs / (double)durationMs.Value));
    }

    public static float ProgressFraction(ItemSummary item) => ProgressFraction(item.UserState.PositionMs, item.DurationMs);

    /// <summary>Episodes of a show or season not watched yet; 0 for other kinds.</summary>
    public static int UnwatchedCount(ItemSummary item) => item.LeafCount == null ? 0 : Math.Max(0, item.LeafCount.Value - (item.ViewedLeafCount ?? 0));

    /// <summary>A video the player plays: a movie, an episode or an extra.</summary>
    public static bool IsPlayableVideo(string kind) => kind is "movie" or "episode" or "extra";

    /// <summary>An item's resume point is worth showing: started, not finished, not marked watched.</summary>
    public static bool IsInProgress(ItemSummary item) =>
        item.DurationMs != null && !item.UserState.Watched && item.UserState.PositionMs > 0 && item.UserState.PositionMs < item.DurationMs;

    /// <summary>What a screen reader says for a tile: "Glasshouse, Season 2 Episode 5, 26 minutes left".</summary>
    public static string TileDescription(ItemSummary item)
    {
        var context = item.Kind == "episode"
            ? string.Join(", ", new[]
            {
                !string.IsNullOrEmpty(item.GrandparentTitle) && item.GrandparentTitle != item.Title ? item.GrandparentTitle : null,
                EpisodeLongLabel(item.ParentIndex, item.Index).Replace(" · ", " ") is { Length: > 0 } l ? l : null,
            }.Where(s => s != null))
            : CardSubtitle(item);
        var unwatched = UnwatchedCount(item);
        var state = IsInProgress(item) ? SpokenTimeLeft(item.UserState.PositionMs, item.DurationMs)
            : item.UserState.Watched ? Strings.T("format.tile.watched")
            : item.Kind is "show" or "season" && unwatched > 0 ? Strings.Count("format.tile.unwatched", unwatched)
            : "";
        return string.Join(", ", new[] { item.Title, context, state }.Where(s => s.Length > 0));
    }

    /// <summary>"4K", "1080p", "720p", "SD", or null without a height.</summary>
    public static string? ResolutionBadge(int? width, int? height)
    {
        var w = width ?? 0;
        var h = height ?? 0;
        if (w >= 3800 || h >= 2100) return "4K";
        if (w >= 1800 || h >= 1000) return "1080p";
        if (w >= 1200 || h >= 700) return "720p";
        return h > 0 ? "SD" : null;
    }

    /// <summary>"Dolby Vision P8.1", "Dolby Vision P7 FEL", "HDR10+", "HDR10", "HLG", "HDR" (an older server's flag), or null for SDR.</summary>
    public static string? DynamicRangeLabel(string? range, int? dvProfile = null, int? dvBlCompat = null, bool? hdr = null, string? dvEl = null, bool? hdr10plus = null) =>
        range switch
        {
            "dolby_vision" => "Dolby Vision" + (DolbyVisionProfile(dvProfile, dvBlCompat, dvEl) is { } p ? " " + p : "") + (hdr10plus == true ? " + HDR10+" : ""),
            "hdr10plus" => "HDR10+",
            "hdr10" => "HDR10",
            "hlg" => "HLG",
            "sdr" => null,
            _ => hdr == true ? "HDR" : null,
        };

    /// <summary>"P8.1", "P8.4", "P5", "P7 FEL": profile 7's base layer is always Blu-ray HDR10, never "P7.6".</summary>
    static string? DolbyVisionProfile(int? profile, int? blCompat, string? el)
    {
        if (profile == null) return null;
        var compat = profile >= 8 && blCompat is 1 or 2 or 4 ? $".{blCompat}" : "";
        var layer = profile == 7 && !string.IsNullOrEmpty(el) ? " " + el!.ToUpperInvariant() : "";
        return $"P{profile}{compat}{layer}";
    }

    /// <summary>The short form for badges and tiles: "DV P8.1", "DV", "HDR10+", "HDR10", "HLG", "HDR", or null.</summary>
    public static string? DynamicRangeShort(string? range, int? dvProfile = null, int? dvBlCompat = null, bool? hdr = null, string? dvEl = null) =>
        DynamicRangeLabel(range, dvProfile, dvBlCompat, hdr, dvEl)?.Replace("Dolby Vision", "DV");

    /// <summary>Badges a listing can give (it has no streams): resolution and dynamic range, "DV" without its profile.</summary>
    public static IReadOnlyList<string> QualityBadges(MediaSummary? media)
    {
        if (media == null) return Array.Empty<string>();
        return new[] { ResolutionBadge(media.Width, media.Height), DynamicRangeShort(media.DynamicRange) }.Where(s => s != null).Select(s => s!).ToList();
    }
}
