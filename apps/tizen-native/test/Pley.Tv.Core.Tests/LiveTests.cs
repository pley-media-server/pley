using System;
using System.Collections.Generic;
using System.Globalization;
using System.Linq;
using System.Text.Json;
using Pley.Tv.Core.Api;
using Pley.Tv.Core.Live;
using Pley.Tv.Core.Net;
using Pley.Tv.Core.Player;
using Xunit;

namespace Pley.Tv.Core.Tests;

/// <summary>Live TV's rules and words: the Android app's LiveTvTest, and its TV strings.</summary>
public class LiveRulesTests
{
    static LiveGroup Group(string name, string? region = null, string? label = null, int count = 0) =>
        new LiveGroup { Name = name, Region = region, Label = label ?? name, Count = count };

    [Fact]
    public void Categories_keep_a_key_the_player_is_given()
    {
        foreach (var c in new LiveCategory[] { new LiveCategory.All(), new LiveCategory.Favorites(), new LiveCategory.Recent(), new LiveCategory.Group("UK| Sport") })
            Assert.Equal(c, LiveCategory.Parse(c.Key));
        Assert.Equal("group", new LiveCategory.Group("News").Wire);
        Assert.Equal(new LiveCategory.All(), LiveCategory.Parse(null));
    }

    [Fact]
    public void Regions_come_first_alphabetically_then_the_other_groups_in_the_providers_order()
    {
        var groups = new[]
        {
            Group("UK| Sport", "UK", "Sport", 3), Group("Movies", count: 4), Group("FR| News", "FR", "News", 1), Group("UK| News", "UK", "News", 2),
            Group("FR| Kids", "FR", "Kids", 5), Group("DE| Only", "DE", "Only", 1),
        };
        var s = LiveRules.Sections(groups);
        Assert.Equal(new[] { "FR", "UK" }, s.Regions.Select(r => r.Region));
        Assert.Equal(6, s.Regions[0].Count);
        Assert.Equal(new[] { "Movies", "DE| Only" }, s.Others.Select(g => g.Name));
        // With fewer than two regions, one list, as the provider has it.
        var one = LiveRules.Sections(new[] { Group("UK| Sport", "UK"), Group("UK| News", "UK"), Group("Movies") });
        Assert.Empty(one.Regions);
        Assert.Equal(3, one.Others.Count);
    }

    [Fact]
    public void Names_leave_out_the_region_their_group_already_says()
    {
        Assert.Equal(("UK", "Sport"), LiveRules.SplitRegion("UK: Sport"));
        Assert.Equal(("BEE", "SOLO SPORT F1 HEVC"), LiveRules.SplitRegion("|BEE| SOLO SPORT F1 HEVC"));
        Assert.Equal(("UK", "Sport"), LiveRules.SplitRegion("[UK] Sport"));
        Assert.Null(LiveRules.SplitRegion("Sport 1080i"));
        Assert.Null(LiveRules.SplitRegion("Sky: News at Ten"));
        var sky = new LiveChannel { Id = 1, Name = "UK| Sky Sports 1", Group = "UK| Sport" };
        Assert.Equal("Sky Sports 1", LiveRules.ChannelName(sky, new LiveCategory.Group("UK| Sport")));
        Assert.Equal("UK| Sky Sports 1", LiveRules.ChannelName(sky, new LiveCategory.All()));
        Assert.Equal("FR| TF1", LiveRules.ChannelName(new LiveChannel { Name = "FR| TF1", Group = "UK| Sport" }, new LiveCategory.Group("UK| Sport")));
        Assert.Equal("UK · Sport", LiveRules.GroupName(Group("UK| Sport", "UK", "Sport"), "x"));
        Assert.Equal("UK · Sport", LiveText.CategoryLabel(new LiveCategory.Group("UK| Sport")));
    }

    [Fact]
    public void Pages_find_a_channel_by_its_place_in_the_whole_list()
    {
        var pages = new ChannelPages
        {
            Total = LiveRules.Page + 2,
            Pages = new Dictionary<int, IReadOnlyList<LiveChannel>>
            {
                [1] = new[] { new LiveChannel { Id = 7 }, new LiveChannel { Id = 8 } },
                [0] = new[] { new LiveChannel { Id = 1 } },
            },
        };
        Assert.Equal(1L, pages[0]?.Id);
        Assert.Null(pages[1]);
        Assert.Equal(-2L, pages.Key(1));
        Assert.Equal(8L, pages[LiveRules.Page + 1]?.Id);
        Assert.Equal(new[] { 1L, 7L, 8L }, pages.Loaded.Select(c => c.Id));
    }

    [Fact]
    public void The_guide_starts_half_an_hour_before_the_half_hour_now_is_in_and_asks_for_the_chunks_around_what_shows()
    {
        const long now = 1_761_752_400;
        var start = LiveRules.GuideStart(now);
        Assert.Equal(1_761_750_000L, start);
        const long chunk = LiveRules.GuideChunkS;
        var first = start / chunk * chunk;
        Assert.Equal(new[] { first, first + chunk }, LiveRules.Chunks(start, start, start + 3600));
        Assert.Empty(LiveRules.Chunks(start, start + LiveRules.GuideSpanS + chunk * 2, start + LiveRules.GuideSpanS + chunk * 3));
    }

    [Fact]
    public void Chunks_merge_into_one_list_a_channel_each_programme_once()
    {
        var a = FakeLive.Program(1, 100, 200, "A");
        var b = FakeLive.Program(1, 200, 300, "B");
        var merged = LiveRules.Merge(new[]
        {
            new LiveGuide { From = 0, To = 250, Programs = new Dictionary<long, IReadOnlyList<LiveProgram>> { [1] = new[] { b, a } } },
            new LiveGuide { From = 250, To = 500, Programs = new Dictionary<long, IReadOnlyList<LiveProgram>> { [1] = new[] { b, FakeLive.Program(1, 300, 400, "C") } } },
        });
        Assert.Equal(new[] { "A", "B", "C" }, merged[1].Select(p => p.Title));
        Assert.Equal(new[] { "B" }, LiveRules.Between(merged[1], 210, 290).Select(p => p.Title));
        Assert.Equal((1, 5), LiveRules.Left(FakeLive.Program(1, 0, 3900, "long"), 0));
    }

    [Fact]
    public void A_search_names_the_groups_it_matches_a_dozen_at_most()
    {
        var groups = Enumerable.Range(1, 20).Select(i => Group($"UK| Sport {i}", "UK", $"Sport {i}", i)).Append(Group("News")).ToList();
        Assert.Equal(LiveRules.FoundGroupsMax, LiveRules.FoundGroups(groups, " sport ").Count);
        Assert.Empty(LiveRules.FoundGroups(groups, "  "));
    }

    [Fact]
    public void What_is_on_and_next_follow_the_clock()
    {
        var now = FakeLive.Program(1, 100, 200, "Now");
        var next = FakeLive.Program(1, 200, 300, "Next");
        var c = new LiveChannel { Now = now, Next = next };
        Assert.Equal("Now", c.OnAt(150)?.Title);
        Assert.Equal("Next", c.NextAt(150)?.Title);
        // The list's now ended before it was asked again: its next is on.
        Assert.Equal("Next", c.OnAt(250)?.Title);
        Assert.Null(c.NextAt(250));
        Assert.Equal("Now", c.NextAt(50)?.Title);
        Assert.Equal(0.5f, now.ProgressAt(150));
        Assert.Equal("S2 E14", (now with { Season = 2, Episode = 14 }).EpisodeLabel);
        Assert.Equal("E3", (now with { Episode = 3 }).EpisodeLabel);
    }

    [Fact]
    public void Times_left_and_days_read_as_the_tv_app_says_them()
    {
        Assert.Equal("12 min left", LiveText.TimeLeft(FakeLive.Program(1, 0, 720, ""), 0));
        Assert.Equal("1 h left", LiveText.TimeLeft(FakeLive.Program(1, 0, 3600, ""), 0));
        Assert.Equal("1 h 5 min left", LiveText.TimeLeft(FakeLive.Program(1, 0, 3900, ""), 0));
        Assert.Equal("1 min left", LiveText.TimeLeft(FakeLive.Program(1, 0, 10, ""), 0));
        var clock = new LiveClock(CultureInfo.GetCultureInfo("en-GB"), TimeZoneInfo.Utc);
        const long noon = 1_761_825_600; // 30 Oct 2025, 12:00 UTC
        Assert.Equal("12:00", clock.Time(noon));
        Assert.Equal("Today", clock.Day(noon + 3600, noon));
        Assert.Equal("Tomorrow", clock.Day(noon + 86_400, noon));
        Assert.Equal("Yesterday", clock.Day(noon - 86_400, noon));
        // Android writes other days in the TV's medium date; the guide reaches two days, so the TV app's short "Fri 31 Oct" serves.
        Assert.Equal("Sat 1 Nov", clock.Day(noon + 2 * 86_400, noon));
        Assert.Equal("Next 12:00 · The News", clock.Next(FakeLive.Program(1, noon, noon + 1, "The News")));
        Assert.True(clock.Midnight(noon + 12 * 3600));
    }

    [Fact]
    public void Who_holds_a_connection_is_said_as_the_player_says_it()
    {
        Assert.Equal("You, on another device", LiveText.Holders(new BusyConnection { Yours = true, People = 1 }));
        Assert.Equal("Ann, Bo", LiveText.Holders(new BusyConnection { Watchers = new[] { "Ann", "Bo" }, People = 2 }));
        Assert.Equal("Someone else", LiveText.Holders(new BusyConnection { People = 0 }));
        Assert.Equal("3 people", LiveText.Holders(new BusyConnection { People = 3 }));
        Assert.Equal("Its one connection is watching:", LiveText.ConnectionsWatching(1));
        Assert.Equal("Its 2 connections are watching:", LiveText.ConnectionsWatching(2));
    }

    [Fact]
    public void A_refusal_is_the_trouble_its_status_says_with_the_connections_a_409_lists()
    {
        using var doc = JsonDocument.Parse("""{"code":"conflict","message":"every connection is in use","connections":[{"channel_id":9,"channel_name":"Sky","watchers":[],"people":1,"yours":true,"can_take_over":true}]}""");
        var busy = LiveTrouble.Of(new ApiException(409, "conflict", "every connection is in use", doc.RootElement.Clone()));
        Assert.Equal(LiveTroubleKind.Busy, busy.Kind);
        Assert.Equal("Every connection is in use.", busy.Message);
        Assert.Equal(9L, Assert.Single(busy.Busy).ChannelId);
        Assert.True(busy.Busy[0].CanTakeOver);
        Assert.Equal(LiveTroubleKind.Missing, LiveTrouble.Of(new ApiException(404, "not_found", "gone")).Kind);
        Assert.False(LiveTrouble.Of(new ApiException(404, "not_found", "gone")).CanRetry);
        Assert.Equal(LiveTroubleKind.Unavailable, LiveTrouble.Of(new ApiException(503, "unavailable", "x")).Kind);
        Assert.Equal(LiveTroubleKind.Offline, LiveTrouble.Of(new NetworkException(NetworkFailure.NoAnswerInTime)).Kind);
        Assert.Equal("BBC One isn't playing", LiveText.TroubleTitle(new LiveTrouble(LiveTroubleKind.Unavailable, ""), "BBC One"));
        Assert.Equal("BBC One didn't start", LiveText.TroubleTitle(new LiveTrouble(LiveTroubleKind.Other, ""), "BBC One"));
    }

    [Fact]
    public void Live_answers_read_as_the_server_writes_them()
    {
        var guide = JsonSerializer.Deserialize<LiveGuide>("""{"from":0,"to":10,"programs":{"7":[{"channel_id":7,"start":0,"stop":10,"title":"A","is_new":true,"is_live":false,"categories":[]}]}}""", ApiJson.Options)!;
        Assert.True(guide.Programs[7][0].IsNew);
        var group = JsonSerializer.Deserialize<LiveGroup>("""{"name":"UK| Sport","region":"UK","label":"Sport","count":3}""", ApiJson.Options)!;
        Assert.Equal("Sport", group.Label);
        Assert.Equal("Movies", JsonSerializer.Deserialize<LiveGroup>("""{"name":"Movies","region":null}""", ApiJson.Options)!.Label);
        var channel = JsonSerializer.Deserialize<LiveChannel>("""{"id":3,"name":"X","number":null,"has_guide":true,"now":null}""", ApiJson.Options)!;
        Assert.True(channel.HasGuide);
    }
}

/// <summary>The guide's focus, moved by the guide itself (the Android app's TvGuide).</summary>
public class GuideCursorTests
{
    const long Start = 1_761_750_000; // a half hour
    const long Now = Start + 2400; // 40 minutes in

    static IReadOnlyList<LiveProgram> Row() => new[]
    {
        FakeLive.Program(1, Start, Start + 1800, "A"),
        FakeLive.Program(1, Start + 1800, Start + 5400, "B"),
        FakeLive.Program(1, Start + 5400, Start + 12_600, "C"),
    };

    [Fact]
    public void The_guide_opens_at_now_the_grid_from_the_half_hour_before()
    {
        var c = new GuideCursor(Start, Now);
        Assert.Equal(Start, c.Left);
        Assert.Equal("B", c.Current(Row())?.Title);
        Assert.Equal(GuideOk.Watch, c.Ok(new LiveChannel { Id = 1 }, Row(), Now));
        Assert.False(c.AwayFromNow(Row(), Now));
    }

    [Fact]
    public void Left_and_right_go_through_programmes_and_the_grid_follows_by_half_hours()
    {
        var c = new GuideCursor(Start, Now);
        c.MoveRight(Row());
        Assert.Equal("C", c.Current(Row())?.Title);
        Assert.Equal(GuideOk.Details, c.Ok(new LiveChannel { Id = 1 }, Row(), Now));
        Assert.True(c.AwayFromNow(Row(), Now));
        // C starts 90 minutes in: the grid keeps a half hour after it in view.
        Assert.Equal(Start + 3600, c.Left);
        Assert.True(c.MoveLeft(Row()));
        Assert.Equal("B", c.Current(Row())?.Title);
        Assert.True(c.MoveLeft(Row()));
        Assert.Equal("A", c.Current(Row())?.Title);
        Assert.Equal(Start, c.Left);
        // From the guide's first half hour, Left goes to the categories.
        Assert.False(c.MoveLeft(Row()));
        c.BackToNow(Now);
        Assert.Equal("B", c.Current(Row())?.Title);
    }

    [Fact]
    public void A_channel_without_programmes_moves_through_time_by_half_hours_both_ways()
    {
        var c = new GuideCursor(Start, Now);
        c.MoveRight(null);
        Assert.Equal(GuideCursor.Slot(Now) + LiveRules.SlotS, c.Cursor);
        // Left comes back the way Right went, rather than leave the guide from there, and goes on
        // to the guide's first half hour; only from there does it leave for the categories.
        Assert.True(c.MoveLeft(null));
        Assert.Equal(GuideCursor.Slot(Now), c.Cursor);
        while (c.MoveLeft(null)) Assert.True(c.Cursor >= Start);
        Assert.Equal(GuideCursor.Slot(Start), GuideCursor.Slot(c.Cursor));
        c.BackToNow(Now);
        c.MoveRight(null);
        Assert.Equal(GuideOk.Watch, c.Ok(new LiveChannel { Id = 1 }, null, Now));
        Assert.Equal(GuideOk.None, c.Ok(null, null, Now));
    }

    [Fact]
    public void Left_goes_back_through_a_long_programme_before_it_leaves_the_guide()
    {
        // A five-hour programme from the guide's start, then another.
        var row = new[]
        {
            FakeLive.Program(1, Start, Start + 18_000, "Long"),
            FakeLive.Program(1, Start + 18_000, Start + 21_600, "Next"),
        };
        var c = new GuideCursor(Start, Now);
        c.MoveRight(row);
        Assert.Equal("Next", c.Current(row)?.Title);
        Assert.True(c.Left > Start);
        Assert.True(c.MoveLeft(row));
        Assert.Equal("Long", c.Current(row)?.Title);
        // Back through the long programme a screen at a time, the view with it, to the guide's start.
        var lefts = new List<long>();
        while (c.Left > Start)
        {
            Assert.True(c.MoveLeft(row));
            Assert.Equal("Long", c.Current(row)?.Title);
            lefts.Add(c.Left);
        }
        Assert.True(lefts.Count >= 2);
        // Only from there does Left leave for the categories.
        Assert.False(c.MoveLeft(row));
    }

    [Fact]
    public void On_a_channel_without_programmes_each_press_moves_the_view_a_half_hour()
    {
        var c = new GuideCursor(Start, Now);
        var left = c.Left;
        c.MoveRight(null);
        Assert.Equal(left + LiveRules.SlotS, c.Left);
        c.MoveRight(null);
        Assert.Equal(left + 2 * LiveRules.SlotS, c.Left);
        Assert.True(c.Cursor >= c.Left && c.Cursor < c.Left + GuideLayout.ViewS);
        Assert.True(c.MoveLeft(null));
        Assert.Equal(left + LiveRules.SlotS, c.Left);
    }

    [Fact]
    public void On_a_channel_without_programmes_back_comes_to_now_once_then_leaves()
    {
        var c = new GuideCursor(Start, Now);
        // At now on a channel without a guide: not away, so Back leaves the guide.
        Assert.False(c.AwayFromNow(null, Now));
        c.MoveRight(null);
        c.MoveRight(null);
        Assert.True(c.AwayFromNow(null, Now));
        c.BackToNow(Now);
        Assert.False(c.AwayFromNow(null, Now));
    }

    [Fact]
    public void Up_and_down_keep_the_time_and_channel_up_and_down_move_five_rows()
    {
        var c = new GuideCursor(Start, Now);
        Assert.False(c.MoveUp());
        c.MoveDown(3);
        c.MoveDown(3);
        c.MoveDown(3);
        Assert.Equal(2, c.Row);
        Assert.True(c.MoveUp());
        Assert.Equal(1, c.Row);
        c.PageDown(30);
        Assert.Equal(6, c.Row);
        c.PageUp();
        c.PageUp();
        Assert.Equal(0, c.Row);
        c.Place(50, 10);
        Assert.Equal(9, c.Row);
        Assert.Equal(Now, c.Cursor);
    }

    [Fact]
    public void The_grid_is_laid_out_at_three_point_six_dp_a_minute()
    {
        Assert.Equal(432f, GuideLayout.ViewS / 60 * GuideLayout.DpPerMinute, 3);
        var (x, width) = GuideLayout.Cell(FakeLive.Program(1, Start - 600, Start + 1800, "begun before"), Start);
        Assert.Equal(0f, x);
        Assert.Equal(108f, width, 3);
        var slots = GuideLayout.Slots(Start, Start);
        Assert.Equal(Start, slots[0].Time);
        Assert.Equal(0f, slots[0].X);
        // None before the guide starts; two screens after the left edge.
        Assert.Equal(8, slots.Count);
    }
}

/// <summary>The categories column, as Live TV and the player's channel list show it.</summary>
public class LiveCategoriesTests
{
    [Fact]
    public void The_column_has_the_marked_categories_then_the_regions_then_the_other_groups()
    {
        var groups = new LiveGroups
        {
            Total = 300, Favorites = 2, Recent = 4,
            Groups = new[]
            {
                new LiveGroup { Name = "UK| Sport", Region = "UK", Label = "Sport", Count = 3 }, new LiveGroup { Name = "UK| News", Region = "UK", Label = "News", Count = 2 },
                new LiveGroup { Name = "FR| A", Region = "FR", Label = "A", Count = 1 }, new LiveGroup { Name = "FR| B", Region = "FR", Label = "B", Count = 1 },
                new LiveGroup { Name = "Movies", Count = 9 }, new LiveGroup { Name = "", Count = 1 },
            },
        };
        var selected = new LiveCategory.Group("UK| News");
        var sections = LiveRules.Sections(groups.Groups);
        var open = LiveCategories.RegionOf(selected, sections);
        Assert.Equal("UK", open);
        var rows = LiveCategories.Rows(groups, selected, open, search: true);
        Assert.Equal(
            new[] { "search", "favorites", "recent", "all", "heading:REGIONS", "region:FR", "region:UK", "group:UK| Sport", "group:UK| News", "heading:MORE GROUPS", "group:Movies", "group:" },
            rows.Select(r => r.Key));
        Assert.Equal(8, LiveCategories.ShownIndex(rows, selected, sections));
        Assert.Equal("No group", ((CategoryRow.Choice)rows.Last()).Name);
        Assert.Equal(300, ((CategoryRow.Choice)rows[3]).Count);
        // The region closed, focus comes back to it.
        var closed = LiveCategories.Rows(groups, selected, null, search: true);
        Assert.Equal(6, LiveCategories.ShownIndex(closed, selected, sections));
    }
}

/// <summary>The Live TV page (the Android app's LiveTvViewModelTest, its page's half).</summary>
public class LiveTvModelTests
{
    readonly FakeLive live = new FakeLive();
    readonly FakeScheduler scheduler = new FakeScheduler();

    LiveTvModel Page() => new LiveTvModel(live, scheduler);

    [Fact]
    public void The_page_lists_a_categorys_channels_a_page_at_a_time()
    {
        var vm = Page();
        Assert.Equal(5, vm.State.Value.Groups?.Groups.Count);
        Assert.Equal(203, vm.Channels.Value.Total);
        Assert.Equal(100, vm.Channels.Value.Loaded.Count);
        Assert.Null(vm.Channels.Value[150]);
        vm.ShowingRows(150, 160);
        Assert.Equal(1148L, vm.Channels.Value[150]?.Id);

        vm.SetCategory(new LiveCategory.Group("News"));
        Assert.Equal(new[] { 1L, 5L }, vm.Channels.Value.Loaded.Select(c => c.Id));
        vm.SetQuery("weather");
        Assert.Equal(new[] { 5L }, vm.Channels.Value.Loaded.Select(c => c.Id));
        Assert.Equal(1, vm.Channels.Value.Total);
    }

    [Fact]
    public void A_search_looks_among_every_channel_names_the_groups_it_matches_and_ends_with_a_category_chosen()
    {
        var vm = Page();
        vm.SetCategory(new LiveCategory.Group("News"));
        vm.SetQuery("sport");
        Assert.True(vm.State.Value.Searching);
        Assert.Equal(new LiveCategory.All(), vm.State.Value.Listed);
        Assert.Equal(new[] { 2L }, vm.Channels.Value.Loaded.Select(c => c.Id));
        Assert.Equal(new[] { "Sport", "UK| Sport" }, vm.State.Value.Found.Select(g => g.Name));
        vm.SetCategory(new LiveCategory.Favorites());
        Assert.Equal("", vm.State.Value.Query);
        Assert.Empty(vm.State.Value.Found);
    }

    [Fact]
    public void The_category_and_the_view_are_kept_for_the_next_launch()
    {
        var vm = Page();
        vm.SetCategory(new LiveCategory.Group("News"));
        vm.SetView(LiveView.Channels);
        Assert.Equal(new LiveTvView("group:News", "Channels"), live.View);
        var again = Page();
        Assert.Equal(new LiveCategory.Group("News"), again.State.Value.Category);
        Assert.Equal(LiveView.Channels, again.State.Value.View);
    }

    [Fact]
    public void The_list_opens_at_the_channel_last_watched_when_it_is_in_the_category()
    {
        live.View = new LiveTvView(Channel: 1150);
        var vm = Page();
        Assert.Equal(new Landing(new LiveCategory.All(), "", 152), vm.Landing.Value);
        vm.SetCategory(new LiveCategory.Group("News"));
        Assert.Equal(new Landing(new LiveCategory.Group("News"), "", null), vm.Landing.Value);
        vm.SetQuery("filler");
        Assert.Null(vm.Landing.Value!.Index);
    }

    [Fact]
    public void A_channel_opens_the_live_player_in_its_category_and_a_refused_favourite_is_told()
    {
        var vm = Page();
        var watched = new List<LiveWatch>();
        var told = new List<Exception>();
        vm.WatchRequested += watched.Add;
        vm.Told += told.Add;
        vm.SetCategory(new LiveCategory.Group("News"));
        var news = vm.Channels.Value[0]!;
        vm.Watch(news);
        Assert.Equal("group:News", watched.Single().CategoryKey);
        vm.SetQuery("sport");
        vm.Watch(vm.Channels.Value[0]!);
        Assert.Equal(new LiveWatch(2, "all", vm.Channels.Value[0]), watched.Last());

        vm.SetCategory(new LiveCategory.All());
        vm.ToggleFavorite(vm.Channels.Value[0]!);
        Assert.True(vm.Channels.Value[0]!.Favorite);
        Assert.Equal(1, vm.State.Value.Groups!.Favorites);
        live.FavoriteFailure = new ApiException(500, "internal", "no");
        vm.ToggleFavorite(vm.Channels.Value[1]!);
        // Put back, and told.
        Assert.False(vm.Channels.Value[1]!.Favorite);
        Assert.Equal("The favourite wasn't saved: No.", LiveText.FavoriteFailed(Assert.Single(told)));
    }

    [Fact]
    public void The_guide_asks_for_the_programmes_of_the_pages_in_view_chunk_by_chunk()
    {
        live.HoldGuides = true;
        var vm = Page();
        vm.ShowingTimes(vm.GuideStart, vm.GuideStart + 3600);
        var asked = live.Sent.Where(s => s.StartsWith("guide", StringComparison.Ordinal)).ToList();
        // The first page's hundred channels, for each chunk asked.
        Assert.NotEmpty(asked);
        Assert.All(asked, s => Assert.EndsWith(" 100", s));
        Assert.False(vm.Guide.Value.Loaded);
        var chunks = LiveRules.Chunks(vm.GuideStart, vm.GuideStart, vm.GuideStart + 3600);
        foreach (var c in chunks)
            live.Guides[c] = new LiveGuide { From = c, To = c + 1, Programs = new Dictionary<long, IReadOnlyList<LiveProgram>> { [1] = new[] { FakeLive.Program(1, c, c + 1800, $"At {c}") } } };
        live.AnswerGuides();
        Assert.True(vm.Guide.Value.Loaded);
        Assert.Equal(chunks.Select(c => $"At {c}"), vm.Guide.Value.Of(1)!.Select(p => p.Title));
        // Moving within the same half chunk asks for nothing more.
        var count = live.Sent.Count;
        vm.ShowingTimes(vm.GuideStart + 60, vm.GuideStart + 3660);
        Assert.Equal(count, live.Sent.Count);
    }

    [Fact]
    public void A_programme_that_ended_asks_for_the_pages_again_at_most_once_a_minute()
    {
        live.Lineup[0] = live.Lineup[0] with { Now = FakeLive.Program(1, 0, scheduler.Now / 1000 + 10, "Ending") };
        var vm = Page();
        var asks = live.Sent.Count(s => s == "page all  0");
        scheduler.Advance(LiveTvModel.TickMs);
        Assert.Equal(asks + 1, live.Sent.Count(s => s == "page all  0"));
        scheduler.Advance(LiveTvModel.TickMs);
        Assert.Equal(asks + 1, live.Sent.Count(s => s == "page all  0"));
        vm.Dispose();
    }

    [Fact]
    public void Live_tv_shows_in_the_rail_while_it_has_channels_for_this_profile()
    {
        Assert.Equal(203, new LiveTvEntry(live, scheduler).Channels.Value);
        live.SummaryAnswer = new LiveSummary { Allowed = true, Available = false };
        Assert.Equal(0, new LiveTvEntry(live, scheduler).Channels.Value);
        live.SummaryAnswer = new LiveSummary { Allowed = false };
        var vm = Page();
        Assert.False(vm.State.Value.Allowed);
    }
}

/// <summary>Watching a channel on the TV's own player, and the live player (the Android app's LivePlaybackTest and LiveTvViewModelTest's player half).</summary>
public class LivePlayerTests
{
    readonly FakeLive live = new FakeLive();
    readonly FakeScheduler scheduler = new FakeScheduler();
    readonly List<FakeEngine> engines = new List<FakeEngine>();

    LivePlayback Playback() => new LivePlayback(live, () =>
    {
        var e = new FakeEngine();
        engines.Add(e);
        return e;
    }, scheduler);

    LivePlayerModel Player(long channelId, LiveCategory? category = null) =>
        new LivePlayerModel(live, Playback(), scheduler, channelId, (category ?? new LiveCategory.All()).Key);

    [Fact]
    public void A_channel_is_watched_with_this_tvs_id_and_a_keyed_address_from_the_live_edge()
    {
        var p = Playback();
        p.Tune(1);
        Assert.Equal("watch 1 tizen-1", live.LastWatch);
        // Its playlist first, as the player would ask: the server answers it once the first segments are in.
        Assert.Contains($"prefetch {FakeLive.Server}/api/live/media/pley_mk_1/index.m3u8", live.Sent);
        var engine = Assert.Single(engines);
        Assert.Equal(new EngineSource($"{FakeLive.Server}/api/live/media/pley_mk_1/index.m3u8", Hls: true, Live: true), engine.Source);
        Assert.IsType<LivePhase.Tuning>(p.State.Value.Is);
        engine.Start();
        Assert.True(p.State.Value.Playing);
        engine.HasVideo = false;
        scheduler.Advance(LivePlayback.PictureCheckMs);
        Assert.True(p.State.Value.AudioOnly);
    }

    const string Relayed = "http://127.0.0.1:5000/r/stream.ts";
    static string Hls(long id) => $"{FakeLive.Server}/api/live/media/pley_mk_{id}/index.m3u8";
    static readonly LivePlan AsItIs = new LivePlan { Video = "copy", Audio = "copy" };

    [Fact]
    public void Sent_as_it_is_a_channel_plays_from_its_stream_ts_read_under_an_id_of_its_own()
    {
        live.StreamAt = _ => Relayed;
        live.HoldPlan = true;
        var p = Playback();
        p.Tune(1);
        Assert.Contains("stream 1 tizen-1-stream", live.Sent);
        var engine = Assert.Single(engines);
        // Ready while the plan is asked for, played once it says the channel is sent as it is.
        Assert.Equal(new EngineSource(Relayed, Hls: false, Live: true), engine.Source);
        Assert.Equal(new[] { $"prepare {Relayed} paused" }, engine.Calls);
        Assert.Contains("plan s1", live.Sent);
        live.AnswerPlan(AsItIs);
        Assert.Equal("play", engine.Calls.Last());
        engine.Start();
        Assert.True(p.State.Value.Playing);
        Assert.DoesNotContain(live.Sent, s => s.StartsWith("prefetch", StringComparison.Ordinal));
    }

    [Fact]
    public void Made_into_something_else_or_not_known_in_time_a_channel_plays_its_hls()
    {
        live.StreamAt = _ => Relayed;
        live.PlanAnswer = new LivePlan { Video = "h264_vaapi", Audio = "aac" };
        var p = Playback();
        p.Tune(1);
        Assert.Equal(new EngineSource(Hls(1), Hls: true, Live: true), engines[0].Source);
        Assert.Equal("end stream", live.Sent[live.Sent.IndexOf("plan s1") + 1]);
        live.PlanAnswer = null;
        p.Tune(2);
        Assert.Equal(new EngineSource(Hls(2), Hls: true, Live: true), engines[0].Source);
    }

    [Fact]
    public void Sound_the_tv_decodes_itself_plays_from_stream_ts_though_the_plan_converts_it()
    {
        // Every Samsung TV decodes AC3 (Android asks its device's probe).
        live.StreamAt = _ => Relayed;
        live.PlanAnswer = new LivePlan { Video = "copy", Audio = "aac", AudioCodec = "ac3", AudioChannels = 6 };
        var p = Playback();
        p.Tune(1);
        Assert.Equal("play", engines[0].Calls.Last());
        Assert.Equal(Relayed, engines[0].Source?.Url);
        Assert.False(LiveRules.PlaysAsSent(new LivePlan { Video = "copy", Audio = "aac", AudioCodec = "dts" }));
        Assert.False(LiveRules.PlaysAsSent(new LivePlan { Video = "h264_vaapi", Audio = "copy", AudioCodec = "aac" }));
        Assert.True(LiveRules.PlaysAsSent(new LivePlan { Video = "copy", Audio = "none" }));
    }

    [Fact]
    public void The_stream_ts_is_asked_for_with_the_channel_or_after_it_when_taking_a_connection_over()
    {
        live.StreamAt = _ => Relayed;
        live.PlanAnswer = AsItIs;
        var p = Playback();
        p.Tune(1);
        Assert.True(live.Sent.IndexOf("stream 1 tizen-1-stream") < live.Sent.IndexOf("watch 1 tizen-1"));
        live.Sent.Clear();
        p.Retry(takeOver: 3);
        Assert.True(live.Sent.IndexOf("watch 1 tizen-1 take 3") < live.Sent.IndexOf("stream 1 tizen-1-stream"));
    }

    [Fact]
    public void A_stream_ts_that_fails_before_it_plays_is_watched_again_as_hls()
    {
        live.StreamAt = _ => Relayed;
        live.PlanAnswer = AsItIs;
        var p = Playback();
        p.Tune(1);
        engines[0].Fail(new EngineError.Unsupported("the TV's player can't read it"));
        Assert.Equal(2, live.Sent.Count(s => s.StartsWith("watch 1", StringComparison.Ordinal)));
        Assert.Single(live.Sent, s => s.StartsWith("stream 1", StringComparison.Ordinal));
        Assert.Equal(new EngineSource(Hls(1), Hls: true, Live: true), engines[0].Source);
        Assert.IsType<LivePhase.Tuning>(p.State.Value.Is);
    }

    [Fact]
    public void A_stream_ts_that_never_starts_is_watched_as_hls_after_its_wait()
    {
        // Deviation from Android, whose player says when it can't: the TV's may wait on.
        live.StreamAt = _ => Relayed;
        live.PlanAnswer = AsItIs;
        var p = Playback();
        p.Tune(1);
        scheduler.Advance(LivePlayback.DirectWaitMs - 1);
        Assert.Equal(Relayed, engines[0].Source?.Url);
        scheduler.Advance(1);
        Assert.Equal(new EngineSource(Hls(1), Hls: true, Live: true), engines[0].Source);
    }

    [Fact]
    public void A_stream_ts_that_breaks_off_says_why_watching_ended_or_is_watched_again_as_hls()
    {
        live.StreamAt = _ => Relayed;
        live.PlanAnswer = AsItIs;
        var p = Playback();
        p.Tune(1);
        engines[0].Start();
        live.Ended = "Ann took this connection over to watch Sky.";
        engines[0].Status(EngineStatus.Ended);
        Assert.Equal(new LiveTrouble(LiveTroubleKind.Stopped, "Ann took this connection over to watch Sky."), p.State.Value.Trouble);
        Assert.Contains("end stream", live.Sent);

        live.Ended = null;
        p.Retry();
        engines[0].Start();
        engines[0].Fail(new EngineError.Network("the network went"));
        Assert.Equal(new EngineSource(Hls(1), Hls: true, Live: true), engines[0].Source);
        Assert.IsType<LivePhase.Tuning>(p.State.Value.Is);
    }

    [Fact]
    public void A_stall_of_a_stream_ts_asks_nothing_of_its_unread_hls_session()
    {
        live.StreamAt = _ => Relayed;
        live.PlanAnswer = AsItIs;
        var p = Playback();
        p.Tune(1);
        engines[0].Start();
        engines[0].Status(EngineStatus.Buffering);
        live.Ended = "this channel isn't being watched any more";
        scheduler.Advance(LivePlayback.StallCheckMs);
        Assert.True(p.State.Value.Playing);
        Assert.True(p.State.Value.Buffering);
    }

    [Fact]
    public void Leaving_or_zapping_ends_the_stream_handed_on()
    {
        live.StreamAt = _ => Relayed;
        live.PlanAnswer = AsItIs;
        var p = Playback();
        p.Tune(1);
        live.Sent.Clear();
        p.Tune(2);
        Assert.Equal("end stream", live.Sent.First());
        live.Sent.Clear();
        p.Stop();
        Assert.Contains("end stream", live.Sent);
    }

    [Fact]
    public void Zapping_lets_the_last_channel_go_and_keeps_the_one_player()
    {
        var p = Playback();
        p.Tune(1);
        engines[0].Start();
        p.Tune(2);
        Assert.Equal(new[] { "s1" }, live.Left);
        Assert.Single(engines);
        Assert.Contains("stop", engines[0].Calls);
    }

    [Fact]
    public void An_address_that_needs_the_token_says_the_server_needs_a_newer_pley()
    {
        // Deviation from Android, whose player sends the token: the TV's can't.
        live.WatchUrl = id => $"/api/live/sessions/s/index.m3u8";
        var p = Playback();
        p.Tune(1);
        Assert.Equal(LiveTroubleKind.Older, p.State.Value.Trouble?.Kind);
        Assert.False(p.State.Value.Trouble!.CanRetry);
        Assert.Equal(new[] { "s1" }, live.Left);
        Assert.Empty(engines);
    }

    [Fact]
    public void Every_connection_in_use_says_which_channels_hold_them_and_one_may_be_taken_over()
    {
        using var doc = JsonDocument.Parse("""{"connections":[{"channel_id":9,"channel_name":"Sky","people":2,"can_take_over":false},{"channel_id":3,"channel_name":"BBC","yours":true,"people":1,"can_take_over":true}]}""");
        live.WatchFailure = new ApiException(409, "conflict", "every connection is in use", doc.RootElement.Clone());
        var p = Playback();
        p.Tune(1);
        var trouble = p.State.Value.Trouble!;
        Assert.Equal(LiveTroubleKind.Busy, trouble.Kind);
        Assert.Equal(new[] { 9L, 3L }, trouble.Busy.Select(c => c.ChannelId));
        live.WatchFailure = null;
        p.Retry(takeOver: 3);
        Assert.Equal("watch 1 tizen-1 take 3", live.LastWatch);
    }

    [Fact]
    public void A_stall_shows_once_it_lasts_and_the_server_says_whether_watching_ended()
    {
        var p = Playback();
        p.Tune(1);
        var engine = engines[0];
        engine.Start();
        engine.Status(EngineStatus.Buffering);
        scheduler.Advance(LivePlayback.StallShownMs - 1);
        Assert.False(p.State.Value.Buffering);
        scheduler.Advance(1);
        Assert.True(p.State.Value.Buffering);
        engine.Status(EngineStatus.Ready);
        Assert.False(p.State.Value.Buffering);

        engine.Status(EngineStatus.Buffering);
        live.Ended = "Ann took this connection over to watch Sky.";
        scheduler.Advance(LivePlayback.StallCheckMs);
        Assert.Equal(new LiveTrouble(LiveTroubleKind.Stopped, "Ann took this connection over to watch Sky."), p.State.Value.Trouble);
    }

    [Fact]
    public void A_player_failure_tunes_again_twice_then_says_why()
    {
        var p = Playback();
        p.Tune(1);
        engines[0].Start();
        engines[0].Fail(new EngineError.Network("the network went"));
        Assert.IsType<LivePhase.Tuning>(p.State.Value.Is);
        scheduler.Advance(1_000);
        Assert.Equal(2, live.Sent.Count(s => s.StartsWith("watch 1", StringComparison.Ordinal)));
        engines[0].Fail(new EngineError.Network("the network went"));
        scheduler.Advance(3_000);
        Assert.Equal(3, live.Sent.Count(s => s.StartsWith("watch 1", StringComparison.Ordinal)));
        engines[0].Fail(new EngineError.Network("the network went"));
        Assert.Equal(new LiveTrouble(LiveTroubleKind.Other, "The TV's player said: the network went"), p.State.Value.Trouble);
        // A 410 from the player is the server stopping it: no tuning again.
        p.Retry();
        engines[0].Fail(new EngineError.Stopped("Stopped by the administrator."));
        Assert.Equal(LiveTroubleKind.Stopped, p.State.Value.Trouble?.Kind);
    }

    [Fact]
    public void A_channel_that_never_shows_fails_after_its_wait()
    {
        var p = Playback();
        p.Tune(1);
        scheduler.Advance(LivePlayback.StartWaitMs);
        scheduler.Advance(1_000);
        Assert.Equal(2, live.Sent.Count(s => s.StartsWith("watch 1", StringComparison.Ordinal)));
    }

    [Fact]
    public void The_background_lets_the_connection_and_the_player_go_and_coming_back_tunes_again()
    {
        var p = Playback();
        p.Tune(1);
        engines[0].Start();
        p.Stop();
        Assert.True(engines[0].Released);
        Assert.Equal(new[] { "s1" }, live.Left);
        Assert.Equal(new LiveState(1), p.State.Value);
        p.Resume();
        Assert.Equal(2, engines.Count);
        p.Close();
        p.Tune(2);
        Assert.DoesNotContain("watch 2 tizen-1", live.Sent);
    }

    [Fact]
    public void Channel_up_and_down_go_through_the_categorys_pages_round_its_ends()
    {
        var vm = Player(1);
        Assert.Equal("watch 1 tizen-1", live.LastWatch);
        Assert.Equal("Pley News HD", vm.Channel.Value?.Name);
        Assert.Equal(1L, live.View.Channel);
        vm.Step(1);
        Assert.Equal("watch 2 tizen-1", live.LastWatch);
        vm.Step(-1);
        vm.Step(-1);
        Assert.Equal("watch 1200 tizen-1", live.LastWatch);
        vm.Step(1);
        Assert.Equal("watch 1 tizen-1", live.LastWatch);
        vm.Tune(1097);
        vm.Step(1);
        Assert.Equal("watch 1098 tizen-1", live.LastWatch);
    }

    [Fact]
    public void In_a_group_channel_up_stays_in_it()
    {
        Player(1, new LiveCategory.Group("News")).Step(1);
        Assert.Equal("watch 5 tizen-1", live.LastWatch);
    }

    [Fact]
    public void Recently_watched_keeps_the_order_it_had_as_the_player_opened()
    {
        var vm = Player(2, new LiveCategory.Recent());
        vm.Step(1);
        Assert.Equal("watch 1 tizen-1", live.LastWatch);
        live.Recent = new List<long> { 1, 2 };
        vm.Step(1);
        Assert.Equal("watch 2 tizen-1", live.LastWatch);
    }

    [Fact]
    public void A_typed_number_tunes_once_the_digits_stop_or_says_there_is_no_such_channel()
    {
        var vm = Player(1, new LiveCategory.Group("Sport"));
        vm.Digit('1');
        vm.Digit('0');
        vm.Digit('5');
        Assert.Equal("105", vm.Digits.Value.Text);
        scheduler.Advance(LivePlayerModel.DigitsWaitMs);
        Assert.Equal("watch 5 tizen-1", live.LastWatch);
        vm.Digit('9');
        scheduler.Advance(LivePlayerModel.DigitsWaitMs);
        Assert.Equal("No channel 9", vm.Digits.Value.Text);
        scheduler.Advance(LivePlayerModel.NotFoundMs);
        Assert.Equal(new ChannelDigits(), vm.Digits.Value);
    }

    [Fact]
    public void The_channel_list_shows_another_group_and_a_channel_tuned_from_it_changes_what_channel_up_goes_through()
    {
        var vm = Player(1);
        Assert.Equal(0, vm.ListIndex.Value);
        vm.ShowList(new LiveCategory.Group("News"));
        Assert.Equal(new[] { 1L, 5L }, vm.List.Value.Loaded.Select(c => c.Id));
        // Channel up keeps its category until one is tuned from the list.
        Assert.Equal(new LiveCategory.All(), vm.Category.Value);
        vm.Tune(5, vm.List.Value[1], new LiveCategory.Group("News"));
        vm.Step(1);
        Assert.Equal("watch 1 tizen-1", live.LastWatch);
        vm.SearchList("sport");
        Assert.Equal(new LiveCategory.All(), vm.Listed.Value);
        Assert.Equal(new[] { 2L }, vm.List.Value.Loaded.Select(c => c.Id));
        Assert.Null(vm.ListIndex.Value);
        vm.OpenList();
        Assert.Equal(new LiveCategory.Group("News"), vm.ListCategory.Value);
        Assert.Equal(0, vm.ListIndex.Value);
    }

    [Fact]
    public void A_favourite_shows_at_once_and_is_put_back_when_refused()
    {
        var vm = Player(1);
        vm.ToggleFavorite();
        Assert.True(vm.Channel.Value!.Favorite);
        live.FavoriteFailure = new ApiException(500, "internal", "no");
        vm.ToggleFavorite();
        Assert.True(vm.Channel.Value!.Favorite);
    }

    [Fact]
    public void Leaving_the_player_ends_watching_at_once()
    {
        var vm = Player(1);
        engines[0].Start();
        vm.Dispose();
        Assert.Equal(new[] { "s1" }, live.Left);
        Assert.True(engines[0].Released);
    }
}
