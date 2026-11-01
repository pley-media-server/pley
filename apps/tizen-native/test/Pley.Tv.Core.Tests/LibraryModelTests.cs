using System;
using System.Collections.Generic;
using System.Linq;
using Pley.Tv.Core.Api;
using Pley.Tv.Core.Ui;
using Xunit;
using static Pley.Tv.Core.Tests.LibraryKit;

namespace Pley.Tv.Core.Tests;

/// <summary>A library page and its listing: the Android app's LibraryViewModelTest and LibraryItemsTest.</summary>
public class LibraryModelTests
{
    readonly FakeLibraries data = new FakeLibraries(Movies, Music);
    readonly FakeScheduler scheduler = new FakeScheduler();

    LibraryModel Model(long libraryId = 1, LibraryTab? tab = null, string? saved = null, string? view = null, int windowAbove = LibraryModel.DefaultWindowAbove) =>
        new LibraryModel(data, libraryId, scheduler, tab, view, saved, windowAbove);

    static GridCells Cells(LibraryModel model) => Assert.IsType<Loadable<GridCells>.Ready>(model.State.Value.Grid.Cells).Data;

    static LibraryQuery Whole(LibraryQuery q) => q with { Offset = 0, Limit = null };

    LibraryQuery LastAsked => Whole(data.Asked.Last());

    [Fact]
    public void The_grid_is_sized_from_the_total_with_a_window_in_it()
    {
        data.Set(ByTitle, 9_000);
        var model = Model();
        var cells = Cells(model);
        Assert.Equal(9_000, cells.Total);
        Assert.Equal(1L, cells[0]!.Id);
        Assert.Null(cells[3_000]);
        Assert.Equal(-3_001L, cells.Key(3_000));
        // A windowed listing has no A to Z: not every title is known.
        Assert.Null(model.State.Value.Grid.Index);

        model.Showing(6_200);
        cells = Cells(model);
        Assert.Equal(9_000, cells.Total);
        Assert.Equal(6_201L, cells[6_200]!.Id);
        Assert.Equal(6_201L, cells.Key(6_200));
        Assert.Null(cells[0]);
        Assert.Equal(-1L, cells.Key(0));
    }

    [Fact]
    public void A_complete_listing_by_title_has_its_a_to_z()
    {
        var titles = Enumerable.Range(1, 40).Select(i => Movie(i, i <= 20 ? $"Alien {i}" : $"Zodiac {i}")).ToList();
        data.Set(ByTitle, titles);
        var model = Model();
        var index = model.State.Value.Grid.Index;
        Assert.NotNull(index);
        Assert.Equal(20, index!.PositionOf(AlphaIndex.Letters.ToList().IndexOf("Z")));
        // By year, none.
        var byYear = new LibraryQuery { Sort = LibrarySort.Year, Order = SortOrder.Descending };
        data.Set(byYear, titles);
        model.SortBy(LibrarySort.Year);
        Assert.Equal(byYear, LastAsked);
        Assert.Null(model.State.Value.Grid.Index);
    }

    [Fact]
    public void Short_listings_have_no_a_to_z()
    {
        data.Set(ByTitle, 30);
        var model = Model();
        Assert.Equal(30, Cells(model).Total);
        Assert.Null(model.State.Value.Grid.Index);
    }

    [Fact]
    public void Sorted_by_bitrate_tiles_say_what_the_files_are_like()
    {
        var byBitrate = new LibraryQuery { Sort = LibrarySort.Bitrate, Order = SortOrder.Descending };
        var media = new MediaSummary { Bitrate = 42_000_000, Size = 25_125_558_682, Width = 3840, Height = 2160, VideoCodec = "hevc", DynamicRange = "dolby_vision" };
        data.Set(byBitrate, new[] { Movie(7) with { Media = media } });
        var model = Model(saved: "sort=bitrate");
        Assert.Equal(TileLook.Bitrate, model.State.Value.Grid.Look);
        var tile = Cells(model)[0]!;
        Assert.Equal("42 Mbps · 23.4 GB", tile.Subtitle);
        Assert.Equal(new[] { "4K", "DV", "HEVC" }, tile.Badges);
        Assert.EndsWith(", 42 Mbps · 23.4 GB", tile.Description);
    }

    [Fact]
    public void Filters_change_the_listing_and_save_as_carries_them()
    {
        var model = Model();
        Assert.Equal(new[] { "Comedy", "Drama" }, model.State.Value.Filters.Genres);
        model.SetWatched(false);
        model.SetGenre("Comedy");
        model.SetYear(1999);
        Assert.Equal(ByTitle with { Watched = false, Genre = "Comedy", Year = 1999 }, LastAsked);
        Assert.True(model.State.Value.Filtered);
        Assert.Equal("watched=false&genre=Comedy&year=1999", model.State.Value.Params.ToQueryString());
        Assert.Equal(new SaveSmartPlaylistPage(1, "genre=Comedy&year=1999&unwatched=1&sort=titleSort"), model.SaveAs());
        model.ClearFilters();
        Assert.Equal(ByTitle, LastAsked);
    }

    [Fact]
    public void A_saved_page_comes_back_as_it_was()
    {
        var model = Model(tab: LibraryTab.Library, saved: "tab=collections&sort=year&order=asc");
        Assert.Equal(new LibraryParams(LibraryTab.Collections, LibrarySort.Year, SortOrder.Ascending), model.State.Value.Params);
    }

    [Fact]
    public void A_library_opens_as_it_was_left()
    {
        data.Kept[1] = "sort=added&watched=false";
        var model = Model(tab: LibraryTab.Library);
        Assert.Equal(new LibraryParams(Sort: LibrarySort.Added, Watched: false), model.State.Value.Params);
        // Read before the listing: it isn't asked for twice.
        Assert.Equal(new[] { new LibraryQuery { Sort = LibrarySort.Added, Order = SortOrder.Descending, Watched = false } }, data.Asked.Select(Whole).Distinct());
    }

    [Fact]
    public void What_is_chosen_is_how_the_library_opens_next_time()
    {
        var model = Model();
        Assert.Empty(data.Kept);
        model.SortBy(LibrarySort.Year);
        model.SetGenre("Comedy");
        model.Select(LibraryTab.Recommended);
        Assert.Equal("tab=recommended&sort=year&genre=Comedy", data.Kept[1]);
        model.ClearFilters();
        model.SortBy(LibrarySort.Title);
        model.Select(LibraryTab.Library);
        // The default view is forgotten.
        Assert.False(data.Kept.ContainsKey(1));
    }

    [Fact]
    public void A_library_opens_on_the_tab_it_was_left_on_unless_opened_on_one()
    {
        data.Kept[1] = "tab=recommended";
        data.Kept[3] = "tab=collections&sort=added";
        var left = Model();
        Assert.Equal(LibraryTab.Recommended, left.State.Value.Params.Tab);
        Assert.Equal(LibraryTab.Recommended, left.State.Value.OpenedOn);
        // Another library keeps its own.
        var other = Model(libraryId: 3);
        Assert.Equal(new LibraryParams(LibraryTab.Collections, LibrarySort.Added), other.State.Value.Params);
        // Opened on the Library tab, it keeps the sort it was left in.
        var opened = Model(libraryId: 3, tab: LibraryTab.Library);
        Assert.Equal(new LibraryParams(LibraryTab.Library, LibrarySort.Added), opened.State.Value.Params);
    }

    [Fact]
    public void See_all_opens_in_the_rows_order_not_the_one_the_library_was_left_in()
    {
        data.Kept[1] = "sort=year&watched=false&genre=Comedy";
        var model = new LibraryModel(data, new PageKey.Library(1, LibraryTab.Library, "sort=added&order=desc"), scheduler);
        Assert.Equal(LibraryTab.Library, model.State.Value.OpenedOn);
        Assert.Equal(new LibraryParams(LibraryTab.Library, LibrarySort.Added, SortOrder.Descending), model.State.Value.Params);
        // What the library was left in stays.
        Assert.Equal("sort=year&watched=false&genre=Comedy", data.Kept[1]);
    }

    [Fact]
    public void A_recommended_rows_see_all_shows_the_library_tab_in_its_order()
    {
        data.SetHubs(1, new Hub { Key = "recently_added:1", Title = "Recently Added", LibraryId = 1, Items = new[] { Movie(3) } },
            new Hub { Key = "continue_watching:1", Title = "Continue Watching", LibraryId = 1, Items = new[] { Movie(4) } });
        var model = Model(tab: LibraryTab.Recommended, saved: "tab=recommended&watched=false");
        var rows = Assert.IsType<Loadable<IReadOnlyList<LibraryRow>>.Ready>(model.State.Value.Recommended).Data;
        Assert.Equal(new[] { "sort=added&order=desc", null }, rows.Select(r => r.SeeAll));
        model.SeeAll(rows[0].SeeAll!);
        Assert.Equal(new LibraryParams(LibraryTab.Library, LibrarySort.Added, SortOrder.Descending), model.State.Value.Params);
    }

    [Fact]
    public void Nothing_kept_opens_on_library()
    {
        var model = Model();
        Assert.Equal(LibraryTab.Library, model.State.Value.Params.Tab);
        Assert.Equal(LibraryTab.Library, model.State.Value.OpenedOn);
    }

    [Fact]
    public void A_page_that_comes_back_keeps_its_own_view()
    {
        data.Kept[1] = "sort=added";
        var model = Model(saved: "sort=year");
        Assert.Equal(LibrarySort.Year, model.State.Value.Params.Sort);
        Assert.Equal("sort=added", data.Kept[1]);
    }

    [Fact]
    public void Tabs_read_only_once_shown()
    {
        data.SetHubs(1, new Hub { Key = "continue_watching:1", Title = "Continue Watching", LibraryId = 1, Items = new[] { Movie(3) } },
            new Hub { Key = "top_rated:1", Title = "Top Rated", LibraryId = 1, Items = new[] { Movie(4) } },
            new Hub { Key = "recently_released:1", Title = "Recently Released", LibraryId = 1 });
        data.SetCollections(1, Collection(9, "Alien Collection", 4));
        var model = Model();
        Assert.Equal(0, data.HubAsks);
        Assert.Equal(0, data.CollectionAsks);
        Assert.IsType<Loadable<IReadOnlyList<LibraryRow>>.Loading>(model.State.Value.Recommended);

        model.Select(LibraryTab.Recommended);
        Assert.Equal(1, data.HubAsks);
        var rows = Assert.IsType<Loadable<IReadOnlyList<LibraryRow>>.Ready>(model.State.Value.Recommended).Data;
        // An empty row isn't shown; the collections come last.
        Assert.Equal(new[] { "continue_watching:1", "top_rated:1", LibraryRow.CollectionsKey }, rows.Select(r => r.Key));
        Assert.Equal(TileLook.Landscape, rows[0].Look);
        Assert.True(rows[0].ContinueWatching);
        Assert.Equal(TileKind.Poster, rows[1].Kind);
        Assert.Equal(new PageKey[] { new PageKey.Collection(9) }, rows[2].Opens);
        Assert.Equal("Collections", rows[2].Title);
        var shelf = Assert.IsType<Loadable<IReadOnlyList<CollectionTile>>.Ready>(model.State.Value.Collections).Data;
        var only = Assert.Single(shelf);
        Assert.Equal((9L, 4L, "Alien Collection", "4 movies"), (only.Id, only.Count, only.Tile.Title, only.Tile.Subtitle));
        Assert.Equal(new[] { FakeLibraries.Server + "/api/collections/9/images/poster" }, only.Tile.Images);

        // Shown again, it doesn't read again.
        model.Select(LibraryTab.Library);
        model.Select(LibraryTab.Recommended);
        Assert.Equal(1, data.HubAsks);
        Assert.Equal(1, data.CollectionAsks);
    }

    [Fact]
    public void Music_has_no_collections_or_filters_and_opens_artists()
    {
        var model = Model(libraryId: 2, saved: "tab=collections&watched=false&sort=bitrate");
        var state = model.State.Value;
        Assert.Equal(new[] { LibraryTab.Recommended, LibraryTab.Library }, state.Tabs);
        Assert.Equal(new LibraryParams(), state.Params);
        Assert.Equal(ByTitle, LastAsked);
        Assert.Equal(new ArtistPage(5), state.Opens(5));
        Assert.Empty(state.Filters.Genres);
        Assert.Equal(TileKind.Square, state.Grid.Kind);
        Assert.Equal(0, data.CollectionAsks);
    }

    [Fact]
    public void An_unknown_library_is_missing()
    {
        var model = Model(libraryId: 42);
        Assert.True(model.State.Value.Missing);
        // Nothing is listed for it.
        Assert.Empty(data.Asked);
    }

    [Fact]
    public void Libraries_that_could_not_be_read_say_why_until_they_are()
    {
        data.Libraries.Value = null;
        var error = new ApiException(500, "internal", "the server failed");
        data.LibrariesError.Value = error;
        var model = Model();
        Assert.Same(error, model.State.Value.LibraryError);
        Assert.False(model.State.Value.Missing);
        Assert.Equal(MessageAction.Retry, LibraryText.PageMessage(model.State.Value)!.Action);
        model.Retry();
        Assert.Equal(1, data.Refreshes);
        data.Libraries.Value = new[] { Movies };
        data.LibrariesError.Value = null;
        Assert.Null(model.State.Value.LibraryError);
        Assert.Equal("Movies", model.State.Value.Library!.Name);
    }

    [Fact]
    public void Density_is_the_tvs_setting()
    {
        var model = Model();
        Assert.Equal(GridDensity.Medium, model.State.Value.Density);
        model.SetDensity(GridDensity.Large);
        Assert.Equal(GridDensity.Large, data.Settings.Value.GridDensity);
        Assert.Equal(GridDensity.Large, model.State.Value.Density);
    }

    [Fact]
    public void A_failed_listing_says_why_and_retry_reads_it_again()
    {
        var error = new ApiException(500, "internal", "the server failed");
        data.Fail(ByTitle, error);
        var model = Model();
        var failed = Assert.IsType<Loadable<GridCells>.Failed>(model.State.Value.Grid.Cells);
        Assert.Same(error, failed.Error);
        Assert.Equal(new LibraryMessage(LibraryText.Failed, "The server failed.", MessageAction.Retry), LibraryText.GridMessage(model.State.Value));
        data.Set(ByTitle, 10);
        model.Retry();
        Assert.Equal(10, Cells(model).Total);
        Assert.Null(LibraryText.GridMessage(model.State.Value));
    }

    [Fact]
    public void Changes_the_app_hears_read_the_page_again_while_it_shows_what_it_has()
    {
        data.Set(ByTitle, 10);
        var model = Model();
        var asked = data.Asked.Count;
        var seen = new List<LibraryState>();
        model.State.Changed += seen.Add;
        data.Changed();
        Assert.Equal(asked + 1, data.Asked.Count);
        Assert.All(seen, s => Assert.IsType<Loadable<GridCells>.Ready>(s.Grid.Cells));
    }

    [Fact]
    public void A_panels_choices_apply_and_say_what_the_panel_does_next()
    {
        var model = Model();
        var sort = LibraryText.Panel(LibraryPanel.Sort, model.State.Value);
        Assert.IsType<PanelOutcome.Close>(model.Choose(sort.Rows.First(r => r.Value == LibrarySort.Year)));
        Assert.Equal(LibrarySort.Year, model.State.Value.Params.Sort);
        Assert.IsType<PanelOutcome.Close>(model.Choose(sort.Rows.First(r => r.Kind == LibraryRowKind.Order && r.Value == SortOrder.Ascending)));
        Assert.Equal(SortOrder.Ascending, model.State.Value.Params.EffectiveOrder);

        var filter = LibraryText.Panel(LibraryPanel.Filter, model.State.Value);
        Assert.IsType<PanelOutcome.Stay>(model.Choose(filter.Rows.First(r => r.Label == LibraryText.Unwatched)));
        Assert.False(model.State.Value.Params.Watched);
        Assert.Equal(new PanelOutcome.Show(LibraryPanel.Genre), model.Choose(filter.Rows.First(r => r.Kind == LibraryRowKind.Genres)));
        Assert.IsType<PanelOutcome.Close>(model.Choose(LibraryText.Panel(LibraryPanel.Year, model.State.Value).Rows.First(r => r.Value == "1999")));
        Assert.Equal(1999, model.State.Value.Params.Year);
        var saveAs = LibraryText.Panel(LibraryPanel.Filter, model.State.Value).Rows.First(r => r.Kind == LibraryRowKind.SaveAs);
        Assert.Equal(new PanelOutcome.Open(new SaveSmartPlaylistPage(1, "year=1999&unwatched=1&sort=year")), model.Choose(saveAs));

        // The chip: unwatched only, then both again.
        model.ToggleUnwatched();
        Assert.Null(model.State.Value.Params.Watched);
        model.ToggleUnwatched();
        Assert.False(model.State.Value.Params.Watched);
    }

    // ---- the listing (the Android app's LibraryItemsTest)

    [Fact]
    public void The_first_200_come_first_then_pages_of_1000_into_one_list()
    {
        var model = Model();
        var sizes = new List<int>();
        model.State.Changed += s =>
        {
            if (s.Grid.Cells is Loadable<GridCells>.Ready r && (sizes.Count == 0 || sizes[sizes.Count - 1] != r.Data.Loaded)) sizes.Add(r.Data.Loaded);
        };
        data.Set(ByTitle, 3_500);
        Assert.Equal(new[] { 200, 2_200, 3_500 }, sizes);
        Assert.Equal(new[] { (0, 200), (200, 1_000), (1_200, 1_000), (2_200, 1_000), (3_200, 1_000) },
            data.Asked.Select(q => (q.Offset, q.Limit ?? -1)).OrderBy(p => p.Item1));
        var cells = Cells(model);
        Assert.True(cells.Complete);
        Assert.Equal(Enumerable.Range(1, 3_500).Select(i => (long)i), Enumerable.Range(0, 3_500).Select(cells.IdAt));
        Assert.NotNull(model.State.Value.Grid.Index);
    }

    [Fact]
    public void A_50000_item_library_stays_a_window_of_2000_around_what_the_grid_shows()
    {
        var model = Model();
        var largest = 0;
        model.State.Changed += s => largest = Math.Max(largest, s.Grid.Cells.Value?.Loaded ?? 0);
        data.Set(ByTitle, 50_000);
        var first = Cells(model);
        Assert.False(first.Complete);
        Assert.Equal(50_000, first.Total);
        Assert.Equal(0, first.Offset);
        Assert.Equal(2_000, first.Loaded);
        Assert.Equal(1L, first[0]!.Id);
        Assert.Null(first[30_000]);

        model.Showing(30_000);
        var moved = Cells(model);
        Assert.Equal(30_001L, moved[30_000]!.Id);
        Assert.InRange(moved.Offset, 28_001, 30_000);
        Assert.Equal(2_000, moved.Loaded);

        model.Showing(49_999);
        Assert.Equal(50_000L, Cells(model)[49_999]!.Id);

        // Never more than one window in a list, and only the pages asked for.
        Assert.True(largest <= 2_000);
        Assert.True(data.Asked.Count <= 1 + 6);
    }

    [Fact]
    public void Scrolling_back_reuses_the_pages_read_within_a_few()
    {
        data.Set(ByTitle, 10_000);
        var model = Model();
        model.Showing(5_200);
        var asked = data.Asked.Count;
        model.Showing(1_200);
        model.Showing(5_200);
        // The head, pages 0 and 1, then 4 and 5: nothing asked again on the way back and forth.
        Assert.Equal(5, asked);
        Assert.Equal(asked, data.Asked.Count);
        Assert.Equal(5_201L, Cells(model)[5_200]!.Id);
    }

    [Fact]
    public void A_newer_position_takes_over_from_one_still_being_read()
    {
        data.Set(ByTitle, 50_000);
        var model = Model();
        // The window at 30,000 is still being read when the grid comes back to the start.
        data.Holding = true;
        model.Showing(30_000);
        model.Showing(100);
        data.Release();
        Assert.Equal(0, Cells(model).Offset);
        Assert.Equal(1L, Cells(model)[0]!.Id);
        // What was read is kept for when the grid goes there.
        var asked = data.Asked.Count;
        model.Showing(30_000);
        Assert.Equal(asked, data.Asked.Count);
        Assert.Equal(30_001L, Cells(model)[30_000]!.Id);
    }

    [Fact]
    public void The_smallest_tvs_window_above_2000_items_others_above_5000()
    {
        data.Set(ByTitle, 3_000);
        Assert.False(Cells(Model(windowAbove: LibraryModel.SmallWindowAbove)).Complete);
        Assert.True(Cells(Model()).Complete);
    }

    [Fact]
    public void The_query_goes_to_the_server_as_the_web_sends_it()
    {
        Model(saved: "sort=added&order=desc&watched=false&genre=Drama&year=2016");
        Assert.Equal(new LibraryQuery { Sort = LibrarySort.Added, Order = SortOrder.Descending, Watched = false, Genre = "Drama", Year = 2016, Offset = 0, Limit = 200 },
            data.Asked.First());
    }

    [Fact]
    public void A_listing_that_grows_keeps_the_tiles_it_made()
    {
        var items = Enumerable.Range(1, 1_500).Select(i => Movie(i)).ToList();
        var model = Model();
        TileModel? early = null;
        model.State.Changed += s =>
        {
            if (early == null && s.Grid.Cells is Loadable<GridCells>.Ready r) early = r.Data[0];
        };
        data.Set(ByTitle, items);
        Assert.Equal(1_500, Cells(model).Loaded);
        Assert.Same(early, Cells(model)[0]);
    }

    [Fact]
    public void A_cell_its_actions_took_away_hands_focus_to_its_neighbour()
    {
        data.Set(ByTitle, 3);
        var cells = Cells(Model());
        Assert.Equal(2L, cells.Near(0));
        Assert.Equal(2L, cells.Near(2));
        Assert.Equal(1, cells.PositionOf(2));
        Assert.Equal(-1, cells.PositionOf(42));
    }
}
