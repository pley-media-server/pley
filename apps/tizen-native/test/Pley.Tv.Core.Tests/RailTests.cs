using System.Collections.Generic;
using System.Linq;
using Pley.Tv.Core.Api;
using Pley.Tv.Core.Ui;
using Xunit;

namespace Pley.Tv.Core.Tests;

/// <summary>The rail's entries: the Android app's ShellRulesTest.</summary>
public class RailTests
{
    static List<Library> Libraries(int n) =>
        Enumerable.Range(1, n).Select(i => new Library { Id = i, Name = $"Library {i}", Kind = i % 2 == 0 ? "shows" : "movies" }).ToList();

    static IEnumerable<string> Ids(IEnumerable<RailItem> items) => items.Select(i => i.Id);

    [Fact]
    public void The_rail_lists_up_to_three_libraries_then_Collections()
    {
        var items = Rail.Items(Libraries(3));
        Assert.Equal(new[] { "search", "home", "library:1", "library:2", "library:3", "playlists", "collections" }, Ids(items));
        Assert.Equal(new PageKey.Collections(1), items.Last().Key);
    }

    [Fact]
    public void More_than_three_libraries_add_Libraries_in_Collections_place()
    {
        var items = Rail.Items(Libraries(5));
        Assert.Equal(new[] { "search", "home", "library:1", "library:2", "library:3", "libraries", "playlists" }, Ids(items));
        Assert.Equal(7, items.Count);
    }

    [Fact]
    public void While_music_plays_Now_Playing_comes_last_within_the_rails_seven_entries()
    {
        Assert.Equal(new[] { "search", "home", "library:1", "library:2", "library:3", "playlists", "now-playing" }, Ids(Rail.Items(Libraries(3), nowPlaying: true)));
        Assert.Equal(new[] { "search", "home", "library:1", "library:2", "libraries", "playlists", "now-playing" }, Ids(Rail.Items(Libraries(5), nowPlaying: true)));
        Assert.Equal(new[] { "search", "home", "library:1", "library:2", "playlists", "collections", "now-playing" }, Ids(Rail.Items(Libraries(2), nowPlaying: true)));
        Assert.Equal("now-playing", Rail.Current(new PageKey.NowPlaying(), Rail.Items(Libraries(3), nowPlaying: true)));
    }

    [Fact]
    public void Live_TV_comes_after_Home_and_makes_room_as_Now_Playing_does()
    {
        Assert.Equal(new[] { "search", "home", "live-tv", "library:1", "library:2", "library:3", "playlists" }, Ids(Rail.Items(Libraries(3), liveTv: true)));
        Assert.Equal(new[] { "search", "home", "live-tv", "library:1", "library:2", "libraries", "playlists" }, Ids(Rail.Items(Libraries(5), liveTv: true)));
        Assert.Equal(new[] { "search", "home", "live-tv", "library:1", "libraries", "playlists", "now-playing" }, Ids(Rail.Items(Libraries(3), nowPlaying: true, liveTv: true)));
        Assert.Equal(new[] { "search", "home", "live-tv", "playlists" }, Ids(Rail.Items(new List<Library>(), liveTv: true)));
        for (var n = 0; n <= 6; n++)
            foreach (var music in new[] { false, true })
                Assert.True(Rail.Items(Libraries(n), music, liveTv: true).Count <= Rail.MaxItems);
        Assert.Equal("live-tv", Rail.Current(new PageKey.LiveTv(), Rail.Items(Libraries(2), liveTv: true)));
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
    public void When_no_library_can_make_way_Playlists_leaves_for_the_Libraries_page()
    {
        // The rail has no Discover yet, so a full rail is made here as Android's is with it.
        // The libraries have all made way behind Libraries: Playlists makes way.
        var many = Rail.RoomIn(RailOf("search", "home", "discover", "live-tv", "libraries", "playlists", "now-playing"));
        Assert.Equal(new[] { "search", "home", "discover", "live-tv", "libraries", "now-playing" }, Ids(many));
        Assert.True(Rail.PlaylistsOnLibrariesPage(many));
        // The one library becomes Libraries, which lists it and Playlists.
        Assert.Equal(new[] { "search", "home", "discover", "live-tv", "libraries", "now-playing" },
            Ids(Rail.RoomIn(RailOf("search", "home", "discover", "live-tv", "library:1", "playlists", "now-playing"))));
        // No library at all: Playlists makes way.
        Assert.Equal(new[] { "search", "home", "discover", "live-tv", "now-playing", "storage" },
            Ids(Rail.RoomIn(RailOf("search", "home", "discover", "live-tv", "playlists", "now-playing", "storage"))));
        Assert.False(Rail.PlaylistsOnLibrariesPage(Rail.Items(Libraries(5), nowPlaying: true, liveTv: true, storage: true)));
    }

    [Fact]
    public void Playlists_are_in_the_rail_or_on_the_Libraries_page_with_every_extra()
    {
        for (var n = 0; n <= 8; n++)
            foreach (var music in new[] { false, true })
                foreach (var live in new[] { false, true })
                    foreach (var storage in new[] { false, true })
                    {
                        var at = $"{n} libraries, music {music}, live {live}, storage {storage}";
                        var items = Rail.Items(Libraries(n), music, live, storage);
                        Assert.True(items.Count <= Rail.MaxItems, at);
                        Assert.True(storage == items.Any(i => i.Slot == RailSlot.Storage), at);
                        // Playlists in the rail, else on the Libraries page, which the rail leads to.
                        var inRail = items.Any(i => i.Slot == RailSlot.Playlists);
                        Assert.True(inRail != Rail.PlaylistsOnLibrariesPage(items), at);
                        Assert.True(inRail || items.Any(i => i.Slot == RailSlot.Libraries), at);
                        // And every library, in the rail or on that page.
                        var listed = items.Any(i => i.Slot == RailSlot.Libraries);
                        Assert.True(listed || Libraries(n).All(l => items.Any(i => i.Key == new PageKey.Library(l.Id))), at);
                    }
    }

    [Fact]
    public void Playlists_not_in_the_rail_are_under_Libraries()
    {
        var items = Rail.RoomIn(RailOf("search", "home", "discover", "live-tv", "library:1", "playlists", "now-playing"));
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
        var items = Rail.Items(Libraries(5)).Concat(new[] { Rail.SettingsItem }).ToList();
        Assert.Equal("home", Rail.Current(new PageKey.Home(), items));
        Assert.Equal("search", Rail.Current(new PageKey.Search("north"), items));
        Assert.Equal("library:2", Rail.Current(new PageKey.Library(2), items));
        Assert.Equal("libraries", Rail.Current(new PageKey.Library(5), items));
        Assert.Equal("libraries", Rail.Current(new PageKey.Libraries(), items));
        Assert.Equal("playlists", Rail.Current(new PageKey.Playlists(), items));
        Assert.Equal("settings", Rail.Current(new PageKey.Settings(), items));
        Assert.Equal(Rail.ProfileItem, Rail.Current(new PageKey.Profiles(), items));
        Assert.Null(Rail.Current(new PageKey.Servers(), items));
    }
}
