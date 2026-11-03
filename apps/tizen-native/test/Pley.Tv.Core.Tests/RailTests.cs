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
                        Assert.False(Rail.PlaylistsOnLibrariesPage(items), at);
                        // Six libraries in the rail, and Libraries for more.
                        Assert.Equal(System.Math.Min(n, Rail.MaxLibraries), items.Count(i => i.Slot == RailSlot.Library));
                        Assert.True(n > Rail.MaxLibraries == items.Any(i => i.Slot == RailSlot.Libraries), at);
                        Assert.True((n > 0 && n <= Rail.MaxLibraries) == items.Any(i => i.Slot == RailSlot.Collections), at);
                    }
    }

    /// <summary>A rail of these ids: libraries, "Libraries" and Playlists as the rail makes them, anything else an entry of its own (Android's Discover).</summary>
    static List<RailItem> RailOf(params string[] ids) => ids.Select(id => id switch
    {
        "libraries" => new RailItem(id, RailSlot.Libraries, new PageKey.Libraries()),
        "playlists" => new RailItem(id, RailSlot.Playlists, new PageKey.Playlists()),
        _ when id.StartsWith("library:") => new RailItem(id, RailSlot.Library, new PageKey.Library(long.Parse(id.Substring("library:".Length)))),
        _ => new RailItem(id, RailSlot.Search, new PageKey.Search(id)),
    }).ToList();

    [Fact]
    public void Playlists_not_in_the_rail_are_under_Libraries()
    {
        // A rail made without Playlists, as the Libraries page still allows for.
        var items = RailOf("search", "home", "libraries", "now-playing");
        Assert.True(Rail.PlaylistsOnLibrariesPage(items));
        Assert.Equal("libraries", Rail.Current(new PageKey.Playlists(), items));
        Assert.Equal("libraries", Rail.Current(new PageKey.Library(1), items));
        // Opened from the Libraries page, the stack starts there.
        Assert.Equal("libraries", Rail.Current(new PageKey.Libraries(), items));
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
        var dir = input.GetProperty("dir");
        Direction? direction = dir.ValueKind == JsonValueKind.Null ? null : dir.GetString() == "down" ? Direction.Down : Direction.Up;
        var scroll = Rail.Scroll(input.GetProperty("scroll").GetSingle(), input.GetProperty("focused").GetInt32(), input.GetProperty("count").GetInt32(), direction);
        Assert.Equal(c.GetProperty("out").GetSingle(), scroll);
    }
}
