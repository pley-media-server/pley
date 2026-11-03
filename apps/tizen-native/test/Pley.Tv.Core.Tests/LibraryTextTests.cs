using System;
using System.Linq;
using Pley.Tv.Core.Api;
using Pley.Tv.Core.Ui;
using Xunit;
using static Pley.Tv.Core.Tests.LibraryKit;

namespace Pley.Tv.Core.Tests;

/// <summary>What the TV's library pages say, and which controls they show: the Android app's TvLibraryScreen, TvLibraryParts and TvTiles.</summary>
public class LibraryTextTests
{
    readonly FakeLibraries data = new FakeLibraries(Movies, Music, Shows);
    readonly FakeScheduler scheduler = new FakeScheduler();

    LibraryState StateOf(long libraryId = 1, string? saved = null) => new LibraryModel(data, libraryId, scheduler, saved: saved).State.Value;

    [Fact]
    public void Counts_say_what_a_library_holds_by_its_kind()
    {
        Assert.Equal("1,500 movies", LibraryText.Count("movies", 1_500));
        Assert.Equal("1 show", LibraryText.Count("shows", 1));
        Assert.Equal("0 artists", LibraryText.Count("music", 0));
        Assert.Equal("23 of 1,500 movies", LibraryText.CountOf("movies", 23, 1_500));
        Assert.Equal("1 of 1 artist", LibraryText.CountOf("music", 1, 1));
        Assert.Equal("3 collections", LibraryText.CollectionsCount(3));
        Assert.Equal("12 shows", LibraryText.CollectionCount("shows", 12));
        Assert.Equal("1 movie", LibraryText.CollectionCount("movies", 1));
        Assert.Equal("412 movies", LibraryText.LibraryCount(Movies with { ItemCount = 412 }));
        Assert.Equal("2 items", LibraryText.LibraryCount(new Library { Kind = "other", ItemCount = 2 }));
    }

    [Fact]
    public void The_chips_and_the_count_follow_the_listing()
    {
        data.Set(ByTitle, 342);
        var state = StateOf();
        Assert.Equal(new[] { LibraryChipKind.Sort, LibraryChipKind.Filter, LibraryChipKind.Unwatched, LibraryChipKind.Genre, LibraryChipKind.Year },
            LibraryText.Chips(state).Select(c => c.Kind));
        var sort = LibraryText.Chips(state)[0];
        Assert.Equal(("Title", "arrow-up-narrow-wide"), (sort.Label, sort.Icon));
        Assert.Equal("342 movies", LibraryText.CountLine(state));

        var filtered = ByTitle with { Watched = false, Genre = "Comedy" };
        data.Set(filtered, 23);
        state = StateOf(saved: "watched=false&genre=Comedy");
        var chips = LibraryText.Chips(state);
        Assert.True(chips.Single(c => c.Kind == LibraryChipKind.Unwatched).Selected);
        Assert.Equal(("Comedy", true), (chips.Single(c => c.Kind == LibraryChipKind.Genre).Label, chips.Single(c => c.Kind == LibraryChipKind.Genre).Selected));
        Assert.Equal(("Year", false), (chips.Single(c => c.Kind == LibraryChipKind.Year).Label, chips.Single(c => c.Kind == LibraryChipKind.Year).Selected));
        Assert.Equal("23 of 342 movies", LibraryText.CountLine(state));
        Assert.Equal(LibraryPanel.Genre, LibraryText.PanelOf(LibraryChipKind.Genre));
        Assert.Null(LibraryText.PanelOf(LibraryChipKind.Unwatched));
    }

    [Fact]
    public void A_library_without_genres_or_years_has_no_chips_for_them_and_music_has_only_sort()
    {
        data.FiltersAnswer = new FiltersAnswer(Array.Empty<string>(), Array.Empty<int>());
        Assert.Equal(new[] { LibraryChipKind.Sort, LibraryChipKind.Filter, LibraryChipKind.Unwatched }, LibraryText.Chips(StateOf()).Select(c => c.Kind));
        var music = LibraryText.Chips(StateOf(Music.Id, "sort=added"));
        Assert.Equal(("Date added", "arrow-down-wide-narrow"), (Assert.Single(music).Label, music[0].Icon));
    }

    [Fact]
    public void A_resolution_or_dynamic_range_has_a_chip_only_while_chosen_saying_it()
    {
        data.FiltersAnswer = new FiltersAnswer(new[] { "Drama" }, new[] { 1999 }) { Resolutions = new[] { "4k", "1080" }, DynamicRanges = new[] { "hdr", "sdr" } };
        Assert.DoesNotContain(LibraryText.Chips(StateOf()), c => c.Kind is LibraryChipKind.Resolution or LibraryChipKind.DynamicRange);
        var chips = LibraryText.Chips(StateOf(saved: "resolution=4k&dynamic_range=dolby_vision"));
        Assert.Equal(new[] { LibraryChipKind.Sort, LibraryChipKind.Filter, LibraryChipKind.Unwatched, LibraryChipKind.Genre, LibraryChipKind.Year, LibraryChipKind.Resolution, LibraryChipKind.DynamicRange },
            chips.Select(c => c.Kind));
        Assert.Equal(new[] { ("4K", true), ("Dolby Vision", true) }, chips.Skip(5).Select(c => (c.Label, c.Selected)));
        Assert.Equal(LibraryPanel.Resolution, LibraryText.PanelOf(LibraryChipKind.Resolution));
        Assert.Equal(LibraryPanel.DynamicRange, LibraryText.PanelOf(LibraryChipKind.DynamicRange));
        // Music has none.
        Assert.Single(LibraryText.Chips(StateOf(Music.Id, "resolution=4k")));
    }

    [Fact]
    public void The_filter_panel_offers_resolution_and_dynamic_range_after_year_when_there_is_a_choice()
    {
        data.FiltersAnswer = new FiltersAnswer(new[] { "Drama" }, new[] { 1999 }) { Resolutions = new[] { "4k", "1080", "sd" }, DynamicRanges = new[] { "hdr", "dolby_vision", "hdr10", "sdr" } };
        var plain = LibraryText.Panel(LibraryPanel.Filter, StateOf());
        Assert.Equal(new[] { "Show", "All", "Unwatched", "Watched", "Genre", "Year", "Resolution", "Dynamic range" }, plain.Rows.Select(r => r.Label));
        Assert.Equal(new[] { "All genres", "All years", "All resolutions", "All dynamic ranges" }, plain.Rows.Where(r => r.Opens).Select(r => r.Detail));

        var chosen = LibraryText.Panel(LibraryPanel.Filter, StateOf(saved: "resolution=1080&dynamic_range=hdr10"));
        Assert.Equal(new[] { "Genre", "Year", "Resolution", "Dynamic range", "Clear filters", "Save as…" }, chosen.Rows.Skip(4).Select(r => r.Label));
        Assert.Equal(new[] { "1080p", "HDR10" }, chosen.Rows.Where(r => r.Kind is LibraryRowKind.Resolutions or LibraryRowKind.DynamicRanges).Select(r => r.Detail));

        var resolutions = LibraryText.Panel(LibraryPanel.Resolution, StateOf(saved: "resolution=1080"));
        Assert.Equal("Resolution", resolutions.Title);
        Assert.Equal(new[] { "All resolutions", "4K", "1080p", "SD" }, resolutions.Rows.Select(r => r.Label));
        Assert.Equal(new[] { null, "4k", "1080", "sd" }, resolutions.Rows.Select(r => r.Value));
        Assert.Equal(2, resolutions.Focus);
        var ranges = LibraryText.Panel(LibraryPanel.DynamicRange, StateOf());
        Assert.Equal("Dynamic range", ranges.Title);
        Assert.Equal(new[] { "All dynamic ranges", "HDR", "Dolby Vision", "HDR10", "SDR" }, ranges.Rows.Select(r => r.Label));
        Assert.Equal(0, ranges.Focus);

        // A library with one of each has nothing to choose: the rows show only while one is chosen, to change or clear it.
        data.FiltersAnswer = new FiltersAnswer(new[] { "Drama" }, new[] { 1999 });
        Assert.DoesNotContain(LibraryText.Panel(LibraryPanel.Filter, StateOf()).Rows, r => r.Kind is LibraryRowKind.Resolutions or LibraryRowKind.DynamicRanges);
        Assert.Equal(new[] { "Resolution" }, LibraryText.Panel(LibraryPanel.Filter, StateOf(saved: "resolution=sd")).Rows
            .Where(r => r.Kind is LibraryRowKind.Resolutions or LibraryRowKind.DynamicRanges).Select(r => r.Label));
    }

    [Fact]
    public void Resolutions_and_dynamic_ranges_have_their_words()
    {
        Assert.Equal(new[] { "4K", "1080p", "720p", "SD" }, LibraryParams.Resolutions.Select(LibraryText.ResolutionLabel));
        Assert.Equal(new[] { "HDR", "Dolby Vision", "HDR10+", "HDR10", "HLG", "SDR" }, LibraryParams.DynamicRanges.Select(LibraryText.DynamicRangeLabel));
        Assert.Null(LibraryText.ResolutionLabel("8k"));
        Assert.Null(LibraryText.DynamicRangeLabel(null));
    }

    [Fact]
    public void The_sort_panel_lists_the_kinds_sorts_then_the_order_focused_on_the_one_in_force()
    {
        var panel = LibraryText.Panel(LibraryPanel.Sort, StateOf(Shows.Id, "sort=added"));
        Assert.Equal("Sort by", panel.Title);
        Assert.Equal(new[] { "Title", "Year", "Date added", "Rating", "Your rating", "Release date", "Last played", "Bitrate", "Order", "Ascending", "Descending" },
            panel.Rows.Select(r => r.Label));
        Assert.Equal(2, panel.Focus);
        Assert.Equal(new[] { "Date added", "Descending" }, panel.Rows.Where(r => r.Checked).Select(r => r.Label));
        Assert.Equal(LibraryRowKind.Heading, panel.Rows[8].Kind);
    }

    [Fact]
    public void The_filter_panel_offers_clear_and_save_as_once_something_is_filtered()
    {
        var plain = LibraryText.Panel(LibraryPanel.Filter, StateOf());
        Assert.Equal(new[] { "Show", "All", "Unwatched", "Watched", "Genre", "Year" }, plain.Rows.Select(r => r.Label));
        Assert.Equal(1, plain.Focus);
        Assert.Equal(new[] { "All genres", "All years" }, plain.Rows.Where(r => r.Opens).Select(r => r.Detail));

        var filtered = LibraryText.Panel(LibraryPanel.Filter, StateOf(saved: "watched=true&year=1999"));
        Assert.Equal(new[] { "Show", "All", "Unwatched", "Watched", "Genre", "Year", "Clear filters", "Save as…" }, filtered.Rows.Select(r => r.Label));
        Assert.Equal(3, filtered.Focus);
        Assert.Equal("1999", filtered.Rows.Single(r => r.Kind == LibraryRowKind.Years).Detail);
    }

    [Fact]
    public void The_genre_and_year_panels_start_with_all_of_them()
    {
        var genres = LibraryText.Panel(LibraryPanel.Genre, StateOf(saved: "genre=Drama"));
        Assert.Equal(new[] { "All genres", "Comedy", "Drama" }, genres.Rows.Select(r => r.Label));
        Assert.Equal(2, genres.Focus);
        var years = LibraryText.Panel(LibraryPanel.Year, StateOf());
        Assert.Equal(new[] { "All years", "2024", "1999" }, years.Rows.Select(r => r.Label));
        Assert.Equal(0, years.Focus);
        Assert.Null(years.Rows[0].Value);
    }

    [Fact]
    public void Empty_and_failed_tabs_say_so()
    {
        data.Set(ByTitle, 0);
        Assert.Equal(new LibraryMessage(LibraryText.EmptyTitle, LibraryText.EmptyBody), LibraryText.GridMessage(StateOf()));
        data.Set(ByTitle with { Genre = "Comedy" }, 0);
        var none = LibraryText.GridMessage(StateOf(saved: "genre=Comedy"))!;
        Assert.Equal(("No matches", MessageAction.ClearFilters, "Clear filters"), (none.Title, none.Action, none.ActionLabel));

        data.SetCollections(Shows.Id);
        Assert.Equal(new LibraryMessage(LibraryText.CollectionsEmptyTitle, LibraryText.CollectionsEmptyShows),
            LibraryText.CollectionsMessage(StateOf(Shows.Id, "tab=collections")));
        Assert.Equal("0 collections", LibraryText.CollectionsCountLine(StateOf(Shows.Id, "tab=collections")));
        data.SetHubs(1);
        Assert.Equal(new LibraryMessage(LibraryText.RecommendedEmptyTitle, LibraryText.RecommendedEmptyBody), LibraryText.RecommendedMessage(StateOf(saved: "tab=recommended")));

        // While they load, nothing is said: skeletons show.
        Assert.Null(LibraryText.CollectionsMessage(StateOf(saved: "tab=collections")));
        Assert.Null(LibraryText.GridMessage(StateOf(Movies.Id, "sort=year")));

        var missing = LibraryText.PageMessage(StateOf(42))!;
        Assert.Equal(("This library isn't here", MessageAction.None), (missing.Title, missing.Action));
    }

    [Fact]
    public void The_server_out_of_reach_says_so()
    {
        var away = new Pley.Tv.Core.Net.NetworkException(Pley.Tv.Core.Net.NetworkFailure.NothingAnswers);
        data.Fail(ByTitle, away);
        var message = LibraryText.GridMessage(StateOf())!;
        Assert.Equal(Errors.OfflineTitle, message.Title);
        Assert.Equal(Errors.OfflineBody(away), message.Body);
        Assert.Equal("Retry", message.ActionLabel);
    }

    [Fact]
    public void Tabs_and_sorts_have_the_tvs_words()
    {
        Assert.Equal(new[] { "Recommended", "Library", "Collections" }, StateOf().Tabs.Select(LibraryText.TabLabel));
        Assert.Equal(new[] { "Title", "Year", "Date added", "Rating", "Your rating", "Release date", "Duration", "Last played", "Bitrate" },
            LibraryParams.AllSorts.Select(LibraryText.SortLabel));
        Assert.Equal("film", LibraryText.KindIcon("other"));
    }

    [Fact]
    public void A_files_line_and_badges_say_what_is_known()
    {
        Assert.Equal("", LibraryText.MediaLine(null));
        Assert.Equal("Bitrate unknown", LibraryText.MediaLine(new MediaSummary()));
        Assert.Equal("8.2 Mbps", LibraryText.MediaLine(new MediaSummary { Bitrate = 8_200_000 }));
        Assert.Equal(new[] { "HDR10" }, LibraryText.MediaBadges(new MediaSummary { Width = 1920, Height = 1080, DynamicRange = "hdr10" }));
        Assert.Empty(LibraryText.MediaBadges(new MediaSummary { Width = 1920, Height = 1080, VideoCodec = "h264", DynamicRange = "sdr" }));
        Assert.Equal(new[] { "4K" }, LibraryText.MediaBadges(new MediaSummary { Width = 3840, Height = 2160, VideoCodec = "h264", DynamicRange = "sdr" }));
        Assert.Empty(LibraryText.MediaBadges(null));
    }

    [Fact]
    public void The_libraries_page_header_and_hint_follow_sorting()
    {
        Assert.Equal("Libraries", LibraryText.LibrariesHeading(false));
        Assert.Equal(new[] { LibrariesAction.Sort }, LibraryText.LibrariesButtons(false, false, true).Select(b => b.Action));
        Assert.Equal(new[] { LibrariesAction.ServerOrder, LibrariesAction.Done }, LibraryText.LibrariesButtons(true, false, true).Select(b => b.Action));
        Assert.Null(LibraryText.SortHint(false, false));
        Assert.Equal(LibraryText.SortingHint, LibraryText.SortHint(true, false));
    }
}
