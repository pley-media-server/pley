using System;
using System.Collections.Generic;
using System.Linq;
using Pley.Tv.Core.Api;
using Pley.Tv.Core.Net;
using Pley.Tv.Core.Ui;
using Xunit;

namespace Pley.Tv.Core.Tests;

/// <summary>Home's rows, hero and info: the Android app's HomeStateTest.</summary>
public class HomeRulesTests
{
    const string Origin = "http://192.168.1.20:7539";

    static readonly Library Movies = new Library { Id = 1, Name = "Movies", Kind = "movies", ItemCount = 412 };
    static readonly Library Shows = new Library { Id = 2, Name = "Shows", Kind = "shows", ItemCount = 38 };
    static readonly Library Music = new Library { Id = 3, Name = "Music", Kind = "music", ItemCount = 120 };
    static readonly IReadOnlyList<Library> Libraries = new[] { Movies, Shows, Music };

    /// <summary>An episode 32 of 58 minutes in: "26 min left".</summary>
    static readonly ItemSummary Glasshouse = new ItemSummary
    {
        Id = 501, LibraryId = 2, Kind = "episode", Title = "Glasshouse", Year = 2024, Index = 3,
        ParentId = 50, ParentIndex = 1, GrandparentId = 5, GrandparentTitle = "The Lantern House",
        DurationMs = 58 * 60_000L, UserState = new UserState { PositionMs = 32 * 60_000L },
        Summary = "The lamp goes out for the first time in a hundred years.",
    };

    /// <summary>A movie with 1h 12m left.</summary>
    static readonly ItemSummary Ashfall = new ItemSummary
    {
        Id = 101, LibraryId = 1, Kind = "movie", Title = "Ashfall", Year = 2025, Rating = 7.4,
        DurationMs = 122 * 60_000L, UserState = new UserState { PositionMs = 50 * 60_000L }, AddedAt = 1_700_000_000,
    };

    static readonly ItemSummary Saltwind = new ItemSummary
    {
        Id = 102, LibraryId = 1, Kind = "movie", Title = "Saltwind", Year = 2024,
        DurationMs = 111 * 60_000L, UserState = new UserState { PositionMs = 73 * 60_000L }, AddedAt = 1_700_000_100,
    };

    static readonly ItemSummary LowOrbit = new ItemSummary { Id = 111, LibraryId = 1, Kind = "movie", Title = "Low Orbit", Year = 2024, AddedAt = 1_800_000_000 };
    static readonly ItemSummary Halcyon = new ItemSummary { Id = 112, LibraryId = 1, Kind = "movie", Title = "Halcyon", Year = 2025, AddedAt = 1_800_000_100 };
    static readonly ItemSummary Overgrowth = new ItemSummary
    {
        Id = 21, LibraryId = 2, Kind = "show", Title = "Overgrowth", Year = 2024, ChildCount = 2, LeafCount = 16, ViewedLeafCount = 10, AddedAt = 1_800_000_200,
    };
    static readonly ItemSummary Album = new ItemSummary { Id = 301, LibraryId = 3, Kind = "album", Title = "Tidal", ParentTitle = "Harbour Lights", Year = 2021, AddedAt = 1_900_000_000 };

    static Hub HubOf(string key, string title, long? library, params ItemSummary[] items) => new Hub { Key = key, Title = title, LibraryId = library, Items = items };

    static readonly Hub Continue = HubOf("continue_watching", "Continue Watching", null, Glasshouse, Ashfall, Saltwind);
    static readonly Hub AddedMovies = HubOf("recently_added:1", "Recently Added in Movies", 1, LowOrbit, Halcyon);
    static readonly Hub AddedShows = HubOf("recently_added:2", "Recently Added in Shows", 2, Overgrowth);
    static readonly Hub AddedMusic = HubOf("recently_added:3", "Recently Added in Music", 3, Album);
    static readonly IReadOnlyList<Hub> HomeHubs = new[] { Continue, AddedMovies, AddedShows, AddedMusic };

    [Fact]
    public void Rows_are_shaped_as_the_web_shapes_them()
    {
        var rows = HomeRules.Rows(HomeHubs, Libraries);
        Assert.Equal(new[] { RowShape.Landscape, RowShape.Poster, RowShape.Poster, RowShape.Square }, rows.Select(r => r.Shape));
        Assert.Equal(new[] { "Continue Watching", "Recently Added in Movies", "Recently Added in Shows", "Recently Added in Music" }, rows.Select(r => r.Title));
        Assert.True(rows[0].ContinueWatching);
        Assert.Equal(TileKind.Landscape, rows[0].Kind);
        // A row of episodes is stills, wherever it is; albums are squares even when the library isn't known.
        Assert.Equal(RowShape.Landscape, HomeRules.Shape(HubOf("recently_added:2", "Recently Added in Shows", 2, Glasshouse)));
        Assert.Equal(RowShape.Square, HomeRules.Shape(AddedMusic));
    }

    [Fact]
    public void Empty_rows_are_left_out() =>
        Assert.Equal(new[] { "recently_added:1" }, HomeRules.Rows(new[] { HubOf("continue_watching", "Continue Watching", null), AddedMovies }, null).Select(r => r.Key));

    [Fact]
    public void See_all_opens_the_library_for_its_listed_rows_only()
    {
        Assert.Null(HomeRules.SeeAllOf(Continue));
        Assert.Equal(new SeeAll(1, "sort=added&order=desc"), HomeRules.SeeAllOf(AddedMovies));
        Assert.Equal(new SeeAll(2, "sort=rating&order=desc"), HomeRules.SeeAllOf(HubOf("top_rated:2", "Top Rated", 2, Overgrowth)));
        Assert.Null(HomeRules.SeeAllOf(HubOf("continue_watching:2", "Continue Watching", 2, Glasshouse)));
    }

    [Fact]
    public void Nothing_saved_and_nothing_yet_is_loading()
    {
        Assert.IsType<HomeState.Loading>(HomeRules.StateOf(null, null, null, false, ""));
        Assert.IsType<HomeState.Loading>(HomeRules.StateOf(null, null, Libraries, false, ""));
    }

    [Fact]
    public void An_error_shows_only_without_what_was_saved()
    {
        var error = new NetworkException(NetworkFailure.NothingAnswers);
        Assert.Equal(new HomeState.Failed(error), HomeRules.StateOf(null, error, Libraries, false, ""));
        var stale = Assert.IsType<HomeState.Ready>(HomeRules.StateOf(HomeHubs, error, Libraries, false, Origin));
        Assert.Same(error, stale.Error);
        Assert.Equal(4, stale.Rows.Count);
        Assert.Equal(Origin, stale.Origin);
    }

    [Fact]
    public void No_libraries_and_empty_libraries_say_different_things()
    {
        var none = Array.Empty<Library>();
        Assert.IsType<HomeState.NoLibraries>(HomeRules.StateOf(null, null, none, false, ""));
        Assert.IsType<HomeState.NoLibraries>(HomeRules.StateOf(Array.Empty<Hub>(), null, none, false, ""));
        Assert.Equal(new HomeState.Empty(false), HomeRules.StateOf(Array.Empty<Hub>(), null, Libraries, false, ""));
        Assert.Equal(new HomeState.Empty(true), HomeRules.StateOf(Array.Empty<Hub>(), null, Libraries, true, ""));
        Assert.Equal(new HomeState.Empty(true), HomeRules.StateOf(Array.Empty<Hub>(), null, new[] { Movies with { Scanning = true } }, false, ""));
    }

    [Fact]
    public void The_hero_is_the_first_in_progress_else_the_newest_video()
    {
        Assert.Equal(Glasshouse, HomeRules.HeroOf(HomeRules.Rows(HomeHubs, null)));
        // The album is newer, but a video comes first.
        Assert.Equal(Overgrowth, HomeRules.HeroOf(HomeRules.Rows(new[] { AddedMovies, AddedShows, AddedMusic }, null)));
        Assert.Equal(Album, HomeRules.HeroOf(HomeRules.Rows(new[] { AddedMusic }, null)));
        Assert.Null(HomeRules.HeroOf(Array.Empty<HomeRow>()));
    }

    [Fact]
    public void An_episode_is_titled_by_its_show_with_its_number_and_time_left()
    {
        var info = HomeRules.InfoOf(Glasshouse, null);
        Assert.Equal("The Lantern House", info.Title);
        Assert.Equal("S1 · E3 · Glasshouse", info.Context);
        Assert.Equal("26 min left", info.TimeLeft);
        Assert.Equal("58 min", info.Meta.Runtime);
        Assert.Equal("2024", info.Meta.Year);
        Assert.True(info.InProgress);
        Assert.True(info.Video);
        Assert.False(info.Complete);
        Assert.Equal("The lamp goes out for the first time in a hundred years.", info.Summary);
        Assert.Equal(32f / 58f, info.Progress, 3);
    }

    [Fact]
    public void A_movie_gets_its_summary_rating_and_badges_with_its_details()
    {
        var before = HomeRules.InfoOf(Ashfall, null);
        Assert.Equal("Ashfall", before.Title);
        Assert.Equal("", before.Context);
        Assert.Equal("1h 12m left", before.TimeLeft);
        Assert.Equal("2h 2m", before.Meta.Runtime);
        Assert.Equal("7.4", before.Meta.Rating);
        Assert.Null(before.Meta.ContentRating);
        Assert.Equal("", before.Summary);

        var detail = new ItemDetail { Id = Ashfall.Id, LibraryId = 1, Kind = "movie", Title = "Ashfall", Summary = "A volcanologist and her estranged brother race the ash cloud.", ContentRating = "R" };
        var after = HomeRules.InfoOf(Ashfall, detail);
        Assert.Equal("R", after.Meta.ContentRating);
        Assert.Equal("A volcanologist and her estranged brother race the ash cloud.", after.Summary);
        Assert.True(after.Complete);
        // Another item's details are never mixed in.
        Assert.False(HomeRules.InfoOf(Saltwind, detail).Complete);
    }

    [Fact]
    public void A_show_says_its_seasons_and_a_watched_movie_no_progress()
    {
        var show = HomeRules.InfoOf(Overgrowth, null);
        Assert.Equal("2 seasons", show.Meta.Runtime);
        Assert.False(show.Video);
        var watched = HomeRules.InfoOf(LowOrbit with { DurationMs = 90 * 60_000L, UserState = new UserState { PositionMs = 0, Watched = true } }, null);
        Assert.False(watched.InProgress);
        Assert.Equal("", watched.TimeLeft);
        Assert.True(watched.Watched);
    }

    [Fact]
    public void The_backdrop_is_the_items_own_then_its_shows_then_its_parents()
    {
        Assert.Equal(new[] { Origin + "/api/items/5/images/backdrop", Origin + "/api/items/50/images/backdrop" }, HomeRules.Backdrop(Glasshouse, Origin));
        Assert.Equal(new[] { Origin + "/b.jpg" }, HomeRules.Backdrop(Ashfall with { BackdropUrl = "/b.jpg" }, Origin));
    }
}
