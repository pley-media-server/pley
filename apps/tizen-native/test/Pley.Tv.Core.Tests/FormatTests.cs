using Pley.Tv.Core.Api;
using Pley.Tv.Core.Ui;
using Xunit;

namespace Pley.Tv.Core.Tests;

/// <summary>The Android app's FormatTest, for what tiles show, and its tile model.</summary>
public class FormatTests
{
    static ItemSummary Item(string kind = "movie", string title = "Her", int? year = null, int? childCount = null, int? leafCount = null,
        int? viewedLeafCount = null, int? parentIndex = null, int? index = null, string? parentTitle = null, string? grandparentTitle = null,
        UserState? userState = null, long? durationMs = null) =>
        new ItemSummary
        {
            Id = 1, Kind = kind, Title = title, Year = year, ChildCount = childCount, LeafCount = leafCount, ViewedLeafCount = viewedLeafCount,
            ParentIndex = parentIndex, Index = index, ParentTitle = parentTitle, GrandparentTitle = grandparentTitle,
            UserState = userState ?? new UserState(), DurationMs = durationMs,
        };

    [Fact]
    public void Durations_runtimes_and_time_left()
    {
        Assert.Equal("1h 52m", Format.Duration(6_720_000));
        Assert.Equal("45m", Format.Duration(2_700_000));
        Assert.Equal("1h", Format.Duration(3_600_000));
        Assert.Equal("2h", Format.Duration(7_170_000));
        Assert.Equal("1m", Format.Duration(59_000));
        Assert.Equal("29s", Format.Duration(29_000));
        Assert.Equal("1s", Format.Duration(400));
        Assert.Equal("", Format.Duration(0));
        Assert.Equal("", Format.Duration(null));
        Assert.Equal("", Format.Duration(-5_000));
        Assert.Equal("58 min", Format.Runtime(3_480_000));
        Assert.Equal("1h 52m", Format.Runtime(6_720_000));
        Assert.Equal("2h", Format.Runtime(7_200_000));
        Assert.Equal("29s", Format.Runtime(29_000));
        Assert.Equal("", Format.Runtime(null));
        Assert.Equal("26 min left", Format.TimeLeft(1_920_000, 3_480_000));
        Assert.Equal("1h 12m left", Format.TimeLeft(3_000_000, 7_320_000));
        Assert.Equal("", Format.TimeLeft(7_320_000, 7_320_000));
        Assert.Equal("34 minutes left", Format.SpokenTimeLeft(600_000, 2_640_000));
    }

    [Fact]
    public void Counts_and_episode_labels()
    {
        Assert.Equal("1 movie", Format.Pluralize(1, "movie"));
        Assert.Equal("0 movies", Format.Pluralize(0, "movie"));
        Assert.Equal("2 movies", Format.Pluralize(2, "movie"));
        Assert.Equal("3 children", Format.Pluralize(3, "child", "children"));
        Assert.Equal("12,345 movies", Format.Pluralize(12_345, "movie"));
        Assert.Equal("S1 · E3", Format.EpisodeLabel(1, 3));
        Assert.Equal("E3", Format.EpisodeLabel(null, 3));
        Assert.Equal("S0 · E0", Format.EpisodeLabel(0, 0));
        Assert.Equal("", Format.EpisodeLabel(null, null));
    }

    [Fact]
    public void Dynamic_range_and_badges()
    {
        Assert.Equal("DV P8.1", Format.DynamicRangeShort("dolby_vision", 8, 1));
        Assert.Equal("DV P7", Format.DynamicRangeShort("dolby_vision", 7, 6));
        Assert.Equal("DV P7 FEL", Format.DynamicRangeShort("dolby_vision", 7, 6, dvEl: "fel"));
        Assert.Equal("DV", Format.DynamicRangeShort("dolby_vision"));
        Assert.Equal("HDR", Format.DynamicRangeShort(null, hdr: true));
        Assert.Null(Format.DynamicRangeShort("sdr"));
        Assert.Equal(new[] { "4K", "DV" }, Format.QualityBadges(new MediaSummary { Width = 3840, Height = 2160, DynamicRange = "dolby_vision" }));
    }

    [Fact]
    public void Card_subtitles_by_kind()
    {
        Assert.Equal("1999", Format.CardSubtitle(Item(year: 1999)));
        Assert.Equal("", Format.CardSubtitle(Item()));
        Assert.Equal("3 seasons", Format.CardSubtitle(Item("show", childCount: 3, leafCount: 30)));
        Assert.Equal("1 episode", Format.CardSubtitle(Item("show", childCount: 0, leafCount: 1)));
        Assert.Equal("2005", Format.CardSubtitle(Item("show", year: 2005)));
        Assert.Equal("10 episodes", Format.CardSubtitle(Item("season", leafCount: 10)));
        Assert.Equal("S2 · E5", Format.CardSubtitle(Item("episode", parentIndex: 2, index: 5)));
        Assert.Equal("1 album", Format.CardSubtitle(Item("artist", childCount: 1)));
        Assert.Equal("Daft Punk · 2001", Format.CardSubtitle(Item("album", parentTitle: "Daft Punk", year: 2001)));
        Assert.Equal("Daft Punk · Discovery", Format.CardSubtitle(Item("track", grandparentTitle: "Daft Punk", parentTitle: "Discovery")));
        Assert.Equal("", Format.CardSubtitle(Item("extra")));
    }

    [Fact]
    public void Progress_unwatched_and_descriptions()
    {
        Assert.Equal(0.25f, Format.ProgressFraction(30_000, 120_000));
        Assert.Equal(1f, Format.ProgressFraction(200_000, 120_000));
        Assert.Equal(0f, Format.ProgressFraction(30_000, null));
        Assert.Equal(0f, Format.ProgressFraction(0, 120_000));
        Assert.Equal(6, Format.UnwatchedCount(Item("show", leafCount: 10, viewedLeafCount: 4)));
        Assert.Equal(10, Format.UnwatchedCount(Item("show", leafCount: 10)));
        Assert.Equal(0, Format.UnwatchedCount(Item("show", leafCount: 3, viewedLeafCount: 5)));
        Assert.Equal(0, Format.UnwatchedCount(Item()));
        var episode = Item("episode", "Glasshouse", parentIndex: 2, index: 5, grandparentTitle: "Driftline",
            userState: new UserState { PositionMs = 1_920_000 }, durationMs: 3_480_000);
        Assert.Equal("Glasshouse, Driftline, Season 2 Episode 5, 26 minutes left", Format.TileDescription(episode));
        Assert.Equal("Northern Lights, 2024, watched", Format.TileDescription(Item(title: "Northern Lights", year: 2024, userState: new UserState { Watched = true })));
    }

    [Fact]
    public void Tiles_caption_episodes_by_their_show_in_Continue_Watching()
    {
        var episode = Item("episode", "Glasshouse", parentIndex: 2, index: 5, grandparentTitle: "Driftline",
            userState: new UserState { PositionMs = 1_920_000 }, durationMs: 3_480_000) with { GrandparentId = 40, ParentId = 41 };
        var wide = TileModel.Of(episode, TileKind.Landscape, "https://pley.example.com");
        Assert.Equal(("Driftline", "S2 · E5 · 26 min left"), (wide.Title, wide.Subtitle));
        Assert.Equal(new[] { "https://pley.example.com/api/items/40/images/backdrop" }, wide.Images);
        var row = TileModel.Of(episode, TileKind.Episode, "https://pley.example.com");
        Assert.Equal(("5. Glasshouse", "26 min left"), (row.Title, row.Subtitle));
        var poster = TileModel.Of(episode, TileKind.Poster, "https://pley.example.com");
        Assert.Equal(new[] { "https://pley.example.com/api/items/41/images/poster", "https://pley.example.com/api/items/40/images/poster" }, poster.Images);
    }

    [Fact]
    public void Asks_the_server_for_the_bucket_a_picture_fills()
    {
        Assert.Equal(240, Artwork.Bucket(216));
        Assert.Equal(1920, Artwork.Bucket(5000));
        Assert.Equal(1280, Artwork.Bucket(1500, 1280));
        Assert.Equal("/api/items/1/images/poster?v=9&w=240", Artwork.Sized("/api/items/1/images/poster?v=9&w=1920", 216));
    }
}
