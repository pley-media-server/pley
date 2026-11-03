using System.Collections.Generic;
using System.Linq;
using System.Text.Json;
using Pley.Tv.Core.Api;
using Pley.Tv.Core.Ui;
using Xunit;

namespace Pley.Tv.Core.Tests;

/// <summary>The rail's entries and how they scroll: the Android app's ShellRulesTest, and the shared fixture.</summary>
public class RailTests
{
    static List<Library> Libraries(int n) =>
        Enumerable.Range(1, n).Select(i => new Library { Id = i, Name = $"Library {i}", Kind = i % 2 == 0 ? "shows" : "movies" }).ToList();

    static IEnumerable<string> Ids(IEnumerable<RailItem> items) => items.Select(i => i.Id);

    [Fact]
    public void The_rail_lists_up_to_six_libraries_then_Collections()
    {
        var items = Rail.Items(Libraries(3));
        Assert.Equal(new[] { "search", "home", "library:1", "library:2", "library:3", "playlists", "collections" }, Ids(items));
        Assert.Equal(new PageKey.Collections(1), items.Last().Key);
        Assert.Equal(new[] { "search", "home", "library:1", "library:2", "library:3", "library:4", "library:5", "library:6", "playlists", "collections" },
            Ids(Rail.Items(Libraries(6))));
    }

    [Fact]
    public void More_than_six_libraries_add_Libraries_in_Collections_place()
    {
        var items = Rail.Items(Libraries(7));
        Assert.Equal(new[] { "search", "home", "library:1", "library:2", "library:3", "library:4", "library:5", "library:6", "libraries", "playlists" }, Ids(items));
    }

    [Fact]
    public void While_music_plays_Now_Playing_comes_last_leaving_nothing_out()
    {
        Assert.Equal(new[] { "search", "home", "library:1", "library:2", "library:3", "playlists", "collections", "now-playing" },
            Ids(Rail.Items(Libraries(3), nowPlaying: true)));
        Assert.Equal(new[] { "search", "home", "library:1", "library:2", "library:3", "library:4", "library:5", "library:6", "libraries", "playlists", "now-playing" },
            Ids(Rail.Items(Libraries(8), nowPlaying: true)));
        Assert.Equal("now-playing", Rail.Current(new PageKey.NowPlaying(), Rail.Items(Libraries(3), nowPlaying: true)));
    }

    [Fact]
    public void Live_TV_comes_after_Home_leaving_nothing_out()
    {
        Assert.Equal(new[] { "search", "home", "live-tv", "library:1", "library:2", "library:3", "playlists", "collections" }, Ids(Rail.Items(Libraries(3), liveTv: true)));
        Assert.Equal(new[] { "search", "home", "live-tv", "playlists" }, Ids(Rail.Items(new List<Library>(), liveTv: true)));
        Assert.Equal("live-tv", Rail.Current(new PageKey.LiveTv(), Rail.Items(Libraries(2), liveTv: true)));
    }

    [Fact]
    public void A_USB_drive_comes_last_after_Now_Playing()
    {
        Assert.Equal(new[] { "search", "home", "live-tv", "library:1", "library:2", "library:3", "library:4", "library:5", "library:6", "libraries", "playlists", "now-playing", "storage" },
            Ids(Rail.Items(Libraries(9), nowPlaying: true, liveTv: true, storage: true)));
        Assert.Equal("storage", Rail.Current(new PageKey.Storage(), Rail.Items(Libraries(1), storage: true)));
    }

    [Fact]
    public void Every_entry_is_in_the_rail_with_every_extra()
    {
        for (var n = 0; n <= 9; n++)
            foreach (var music in new[] { false, true })
                foreach (var live in new[] { false, true })
                    foreach (var storage in new[] { false, true })
                    {
                        var at = $"{n} libraries, music {music}, live {live}, storage {storage}";
                        var items = Rail.Items(Libraries(n), music, live, storage);
                        Assert.True(music == items.Any(i => i.Slot == RailSlot.NowPlaying), at);
                        Assert.True(live == items.Any(i => i.Slot == RailSlot.LiveTv), at);
                        Assert.True(storage == items.Any(i => i.Slot == RailSlot.Storage), at);
                        Assert.Contains(items, i => i.Slot == RailSlot.Playlists);
                        // Six libraries in the rail, and Libraries for more.
                        Assert.Equal(System.Math.Min(n, Rail.MaxLibraries), items.Count(i => i.Slot == RailSlot.Library));
                        Assert.True(n > Rail.MaxLibraries == items.Any(i => i.Slot == RailSlot.Libraries), at);
                        Assert.True((n > 0 && n <= Rail.MaxLibraries) == items.Any(i => i.Slot == RailSlot.Collections), at);
                    }
    }

    [Fact]
    public void A_rail_with_every_optional_entry_still_has_Playlists()
    {
        // Live TV, Now Playing and a USB drive, with six libraries: nothing makes way for them.
        var full = Rail.Items(Libraries(6), nowPlaying: true, liveTv: true, storage: true);
        Assert.Equal(new[] { "search", "home", "live-tv", "library:1", "library:2", "library:3", "library:4", "library:5", "library:6", "playlists", "collections", "now-playing", "storage" },
            Ids(full));
        Assert.Equal("playlists", Rail.Current(new PageKey.Playlists(), full));
        // With one library or none, Playlists keeps its entry too: the rail is the way to it.
        Assert.Equal(new[] { "search", "home", "live-tv", "library:1", "playlists", "collections", "now-playing", "storage" },
            Ids(Rail.Items(Libraries(1), nowPlaying: true, liveTv: true, storage: true)));
        Assert.Equal(new[] { "search", "home", "live-tv", "playlists", "now-playing", "storage" },
            Ids(Rail.Items(new List<Library>(), nowPlaying: true, liveTv: true, storage: true)));
    }

    [Fact]
    public void A_rail_without_libraries_has_no_Collections() =>
        Assert.Equal(new[] { "search", "home", "playlists" }, Ids(Rail.Items(new List<Library>())));

    [Fact]
    public void The_current_entry_follows_the_back_stacks_first_page()
    {
        var items = Rail.Items(Libraries(8)).Concat(new[] { Rail.SettingsItem }).ToList();
        Assert.Equal("home", Rail.Current(new PageKey.Home(), items));
        Assert.Equal("search", Rail.Current(new PageKey.Search("north"), items));
        Assert.Equal("library:6", Rail.Current(new PageKey.Library(6), items));
        Assert.Equal("libraries", Rail.Current(new PageKey.Library(7), items));
        Assert.Equal("libraries", Rail.Current(new PageKey.Libraries(), items));
        Assert.Equal("playlists", Rail.Current(new PageKey.Playlists(), items));
        Assert.Equal("settings", Rail.Current(new PageKey.Settings(), items));
        Assert.Equal(Rail.ProfileItem, Rail.Current(new PageKey.Profiles(), items));
        Assert.Null(Rail.Current(new PageKey.Servers(), items));
    }

    [Fact]
    public void Entries_are_40_tall_and_4_apart_nine_fitting_above_Settings()
    {
        Assert.Equal((40f, 4f, 44f, 67f, 473f), (Rail.EntryHeight, Rail.EntryGap, Rail.Pitch, Rail.FirstEntry, Rail.SettingsY));
        // Settings 27 from the foot of the 540 canvas.
        Assert.Equal(27f, 540 - Rail.SettingsY - Rail.EntryHeight);
        Assert.True(Rail.FirstEntry + 9 * Rail.Pitch <= Rail.SettingsY);
        Assert.False(Rail.FirstEntry + 10 * Rail.Pitch <= Rail.SettingsY);
        Assert.Equal(9, Rail.Fits);
    }

    [Fact]
    public void The_focused_entry_is_fourth_from_the_top_while_the_entries_scroll_the_same_both_ways()
    {
        // Fourteen entries: focus goes down to the fourth place, the entries move under it an entry a press until
        // the last nine show, and focus goes on to the last. Each entry's place is its own, whichever way focus came.
        var places = Enumerable.Range(0, 14).Select(i => i * Rail.Pitch - Rail.Scroll(0, i, 14)).ToList();
        Assert.Equal(new[] { 0f, 44, 88, 132, 132, 132, 132, 132, 132, 176, 220, 264, 308, 352 }, places);
        // Whole entries at either end: the last ends 392 down the 402 of the view.
        Assert.All(Enumerable.Range(0, 14), i => Assert.Equal(0f, Rail.Scroll(0, i, 14) % Rail.Pitch));
        Assert.Equal(392f, places[13] + Rail.EntryHeight);
    }

    [Fact]
    public void Entries_made_again_keep_focus_on_the_same_entry()
    {
        // Search, Home, Live TV, six libraries, Libraries, Playlists, Now Playing, the drive: focus on Now Playing.
        Assert.Equal("now-playing", Rail.Items(Libraries(8), nowPlaying: true, liveTv: true, storage: true)[11].Id);
        // A scan reports the same libraries: the same entry, where it was.
        Assert.Equal(11, Rail.Refocus("now-playing", 11, Rail.Items(Libraries(8), nowPlaying: true, liveTv: true, storage: true)));
        // Live TV's channels go: the entry moves up one, and focus with it.
        Assert.Equal(10, Rail.Refocus("now-playing", 11, Rail.Items(Libraries(8), nowPlaying: true, storage: true)));
        // One of six libraries goes, from above it.
        Assert.Equal("now-playing", Rail.Items(Libraries(6), nowPlaying: true, liveTv: true, storage: true)[11].Id);
        Assert.Equal(10, Rail.Refocus("now-playing", 11, Rail.Items(Libraries(5), nowPlaying: true, liveTv: true, storage: true)));
    }

    [Fact]
    public void Focus_on_an_entry_that_went_goes_to_the_one_in_its_place()
    {
        // The music stops while focus is on Now Playing: the drive comes up into its place.
        var after = Rail.Items(Libraries(8), liveTv: true, storage: true);
        Assert.Equal("storage", after[Rail.Refocus("now-playing", 11, after)].Id);
        // The drive goes while focus is on it, the last: the last there is now.
        after = Rail.Items(Libraries(8), nowPlaying: true, liveTv: true);
        Assert.Equal("now-playing", after[Rail.Refocus("storage", 12, after)].Id);
        // No entries left: Settings, after them.
        Assert.Equal(0, Rail.Refocus("home", 1, new List<RailItem>()));
    }

    // ---- the fixture packages/tv's rail.test.ts reads too

    public static IEnumerable<object[]> EntriesCases => Enumerable.Range(0, Fixtures.Load("rail").GetProperty("entries").GetArrayLength()).Select(i => new object[] { i });

    public static IEnumerable<object[]> ScrollCases => Enumerable.Range(0, Fixtures.Load("rail").GetProperty("scroll").GetArrayLength()).Select(i => new object[] { i });

    static JsonElement Case(string rule, int i) => Fixtures.Load("rail").GetProperty(rule)[i];

    [Theory]
    [MemberData(nameof(EntriesCases))]
    public void Entries_as_the_fixture_has_them(int i)
    {
        var c = Case("entries", i);
        var input = c.GetProperty("in");
        var libraries = input.GetProperty("libraries").EnumerateArray()
            .Select((kind, at) => new Library { Id = at + 1, Name = $"Library {at + 1}", Kind = kind.GetString()! }).ToList();
        var items = Rail.Items(libraries, input.GetProperty("nowPlaying").GetBoolean(), input.GetProperty("live").GetBoolean());
        // The web TV app calls Live TV's entry "live"; this app, as Android's, "live-tv".
        var ids = items.Select(item => item.Slot == RailSlot.LiveTv ? "live" : item.Id);
        Assert.Equal(c.GetProperty("out").EnumerateArray().Select(e => e.GetString()), ids);
    }

    [Theory]
    [MemberData(nameof(ScrollCases))]
    public void Scroll_as_the_fixture_has_it(int i)
    {
        var c = Case("scroll", i);
        var input = c.GetProperty("in");
        // With an entry focused, where it was scrolled before makes no difference.
        var from = input.TryGetProperty("scroll", out var s) ? new[] { s.GetSingle() } : new[] { 0f, 37f, 1000f };
        foreach (var scroll in from)
            Assert.Equal(c.GetProperty("out").GetSingle(), Rail.Scroll(scroll, input.GetProperty("focused").GetInt32(), input.GetProperty("count").GetInt32()));
    }
}
