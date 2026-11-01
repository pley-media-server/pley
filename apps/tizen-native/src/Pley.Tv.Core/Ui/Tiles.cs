using System;
using System.Collections.Generic;
using System.Linq;
using Pley.Tv.Core.Api;

namespace Pley.Tv.Core.Ui;

/// <summary>The tiles Pley draws, with their TV sizes on the 960 × 540 canvas (the Android app's TileKind, design spec §2.3).</summary>
public sealed class TileKind
{
    /// <summary>Movies, shows, seasons: 7 and a peek in a TV row.</summary>
    public static readonly TileKind Poster = new TileKind("poster", 108, 162);
    /// <summary>Continue Watching and extras, 16:9: 3 and most of a 4th.</summary>
    public static readonly TileKind Landscape = new TileKind("landscape", 204, 115, wide: true);
    /// <summary>Episode rows, 16:9: 4 and a peek.</summary>
    public static readonly TileKind Episode = new TileKind("episode", 184, 104, wide: true);
    /// <summary>The library grid's posters: 7 columns.</summary>
    public static readonly TileKind LibraryGrid = new TileKind("libraryGrid", 96, 144);
    /// <summary>Albums.</summary>
    public static readonly TileKind Square = new TileKind("square", 132, 132);
    /// <summary>Cast, a circle.</summary>
    public static readonly TileKind Person = new TileKind("person", 96, 96, round: true);
    /// <summary>Profiles on "Who's watching?", a circle.</summary>
    public static readonly TileKind Avatar = new TileKind("avatar", 100, 100, round: true);

    TileKind(string name, float width, float height, bool round = false, bool wide = false)
    {
        Name = name;
        Width = width;
        Height = height;
        Round = round;
        // Wide tiles grow more when focused.
        Gap = wide ? 18 : 14;
    }

    public string Name { get; }
    /// <summary>The well's size, in dp.</summary>
    public float Width { get; }
    public float Height { get; }
    public bool Round { get; }
    /// <summary>The gap between tiles of this kind in a row.</summary>
    public float Gap { get; }
    public bool Wide => this == Landscape || this == Episode;

    public override string ToString() => Name;
}

/// <summary>Picture addresses as the server makes them (the Android app's ImageUrls and ArtworkCandidates).</summary>
public static class Artwork
{
    /// <summary>The widths the server makes.</summary>
    public static readonly int[] Buckets = { 160, 240, 320, 480, 640, 960, 1280, 1920 };

    /// <summary>The server's width for a picture drawn <paramref name="px"/> wide: the smallest bucket at least that wide, never above <paramref name="max"/>.</summary>
    public static int Bucket(int px, int max = 1920)
    {
        var cap = Buckets.LastOrDefault(b => b <= max);
        if (cap == 0) cap = Buckets[0];
        var fit = Buckets.FirstOrDefault(b => b >= px);
        if (fit == 0) fit = Buckets[Buckets.Length - 1];
        return Math.Min(fit, cap);
    }

    /// <summary><paramref name="url"/> (as the API gave it) asking for the width of <see cref="Bucket"/>.</summary>
    public static string Sized(string url, int px, int max = 1920)
    {
        var hash = url.IndexOf('#');
        var baseUrl = hash >= 0 ? url.Substring(0, hash) : url;
        var tail = hash >= 0 ? url.Substring(hash) : "";
        var q = baseUrl.IndexOf('?');
        var path = q >= 0 ? baseUrl.Substring(0, q) : baseUrl;
        var query = q >= 0 ? baseUrl.Substring(q + 1) : "";
        var kept = query.Split('&').Where(p => p.Length > 0 && !p.StartsWith("w=", StringComparison.Ordinal)).ToList();
        kept.Add($"w={Bucket(px, max)}");
        return path + "?" + string.Join("&", kept) + tail;
    }

    /// <summary><paramref name="url"/> as the API gave it ("/api/items/12/images/poster?v=1") on the server at <paramref name="origin"/>.</summary>
    public static string Absolute(string url, string origin)
    {
        if (url.StartsWith("http://", StringComparison.Ordinal) || url.StartsWith("https://", StringComparison.Ordinal) || origin.Length == 0) return url;
        return url.StartsWith("/", StringComparison.Ordinal) ? origin.TrimEnd('/') + url : origin.TrimEnd('/') + "/" + url;
    }

    static string ItemImage(long id, string kind, string origin) => Absolute($"/api/items/{id}/images/{kind}", origin);

    /// <summary>Own poster → parent's (an episode's season) → grandparent's (its show).</summary>
    public static IReadOnlyList<string> Poster(ItemSummary item, string origin)
    {
        var out_ = new List<string>();
        if (!string.IsNullOrEmpty(item.PosterUrl)) out_.Add(Absolute(item.PosterUrl!, origin));
        if (item.ParentId != null) out_.Add(ItemImage(item.ParentId.Value, "poster", origin));
        if (item.GrandparentId != null) out_.Add(ItemImage(item.GrandparentId.Value, "poster", origin));
        return out_;
    }

    /// <summary>16:9 tiles: a movie's backdrop, the item's thumbnail, an episode's show backdrop, then anything's backdrop.</summary>
    public static IReadOnlyList<string> Thumb(ItemSummary item, string origin)
    {
        var backdrop = string.IsNullOrEmpty(item.BackdropUrl) ? null : item.BackdropUrl;
        var out_ = new List<string>();
        if (item.Kind == "movie" && backdrop != null) out_.Add(backdrop);
        if (!string.IsNullOrEmpty(item.ThumbUrl)) out_.Add(item.ThumbUrl!);
        if (item.Kind == "episode" && item.GrandparentId != null) out_.Add($"/api/items/{item.GrandparentId}/images/backdrop");
        if (item.Kind != "episode" && backdrop != null && !out_.Contains(backdrop)) out_.Add(backdrop);
        return out_.Select(u => Absolute(u, origin)).ToList();
    }
}

/// <summary>
/// Everything a tile shows, worked out once when a screen builds its state (the Android app's
/// ArtworkTileModel): the picture's addresses, the caption, the overlays and what a screen reader says.
/// </summary>
public sealed record TileModel(
    long Id,
    string Title,
    string Subtitle,
    IReadOnlyList<string> Images,
    float Progress,
    bool Watched,
    int Unwatched,
    IReadOnlyList<string> Badges,
    string Description)
{
    /// <summary>The model of <paramref name="item"/> drawn as <paramref name="kind"/>, its pictures on the server at <paramref name="origin"/>.</summary>
    public static TileModel Of(ItemSummary item, TileKind kind, string origin, bool badges = false)
    {
        var inProgress = Format.IsInProgress(item);
        var timeLeft = inProgress ? Format.TimeLeft(item.UserState.PositionMs, item.DurationMs) : "";
        string title, subtitle;
        if (kind == TileKind.Episode)
        {
            title = item.Index != null ? $"{item.Index}. {item.Title}" : item.Title;
            subtitle = timeLeft.Length > 0 ? timeLeft : Format.Runtime(item.DurationMs);
        }
        else if (kind == TileKind.Landscape && item.Kind == "episode")
        {
            var show = string.IsNullOrEmpty(item.GrandparentTitle) ? item.Title : item.GrandparentTitle!;
            title = show;
            subtitle = string.Join(" · ", new[] { Format.EpisodeLabel(item), timeLeft.Length > 0 ? timeLeft : show != item.Title ? item.Title : "" }.Where(s => s.Length > 0));
        }
        else
        {
            title = item.Title;
            subtitle = timeLeft.Length > 0 ? timeLeft : Format.CardSubtitle(item);
        }
        var unwatched = item.Kind is "show" or "season" ? Format.UnwatchedCount(item) : 0;
        return new TileModel(
            item.Id,
            title,
            subtitle,
            kind.Wide ? Artwork.Thumb(item, origin) : Artwork.Poster(item, origin),
            inProgress ? Format.ProgressFraction(item) : 0f,
            item.UserState.Watched && unwatched == 0,
            unwatched,
            badges ? Format.QualityBadges(item.Media) : Array.Empty<string>(),
            Format.TileDescription(item));
    }
}
