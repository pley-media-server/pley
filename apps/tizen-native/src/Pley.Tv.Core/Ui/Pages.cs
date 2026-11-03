using System.Collections.Generic;
using System.Linq;
using Pley.Tv.Core.Api;

namespace Pley.Tv.Core.Ui;

/// <summary>The app's pages, as the back stack holds them (the Android app's PleyKey).</summary>
public abstract record PageKey
{
    public sealed record Servers : PageKey;
    public sealed record Link(string Server) : PageKey;
    /// <summary>"Who's watching?"; <paramref name="Pin"/>: a start as the profile the TV was left on, its PIN pad first.</summary>
    public sealed record Profiles(bool Pin = false) : PageKey;
    public sealed record Home : PageKey;
    public sealed record Libraries : PageKey;
    /// <summary>A library's page: on <paramref name="Tab"/> (null: the one it was left on), in <paramref name="View"/> (a row's See all, "sort=added&amp;order=desc").</summary>
    public sealed record Library(long LibraryId, LibraryTab? Tab = null, string? View = null) : PageKey;
    public sealed record Collections(long LibraryId) : PageKey;
    public sealed record Collection(long CollectionId) : PageKey;
    public sealed record Item(long ItemId) : PageKey;
    public sealed record Person(long PersonId) : PageKey;
    public sealed record Search(string Query = "") : PageKey;
    public sealed record Playlists : PageKey;
    public sealed record Playlist(long PlaylistId) : PageKey;
    public sealed record LiveTv : PageKey;
    public sealed record History : PageKey;
    public sealed record Settings : PageKey;
    public sealed record NowPlaying : PageKey;
    /// <summary>A folder of the drive plugged into the TV; null: the drive's top.</summary>
    public sealed record Storage(string? Path = null) : PageKey;
}

/// <summary>The kinds of place the rail goes to.</summary>
public enum RailSlot
{
    Search,
    Home,
    LiveTv,
    Library,
    Libraries,
    Playlists,
    Collections,
    NowPlaying,
    Storage,
    Settings,
}

/// <summary>One rail entry: where it goes, and for a library its name and kind.</summary>
/// <param name="Id">Stable across library changes: "home", "library:3", "settings".</param>
/// <param name="Name">A library's name, as the server has it; the other slots have their own words.</param>
public sealed record RailItem(string Id, RailSlot Slot, PageKey Key, string? Name = null, string? LibraryKind = null)
{
    /// <summary>The entry's icon, by Lucide's name.</summary>
    public string Icon => Slot switch
    {
        RailSlot.Search => "search",
        RailSlot.Home => "house",
        RailSlot.LiveTv => "tv-minimal",
        RailSlot.Library => LibraryKind switch
        {
            "movies" => "clapperboard",
            "shows" => "tv",
            "music" => "music",
            _ => "film",
        },
        RailSlot.Libraries => "layers",
        RailSlot.Playlists => "list-music",
        RailSlot.Collections => "layers",
        RailSlot.NowPlaying => "audio-lines",
        RailSlot.Storage => "usb",
        _ => "settings",
    };

    /// <summary>What the entry says (the Android app's tv_shell_rail_* strings).</summary>
    public string Label => Slot switch
    {
        RailSlot.Search => "Search",
        RailSlot.Home => "Home",
        RailSlot.LiveTv => "Live TV",
        RailSlot.Library => Name ?? "",
        RailSlot.Libraries => "Libraries",
        RailSlot.Playlists => "Playlists",
        RailSlot.Collections => "Collections",
        RailSlot.NowPlaying => "Now Playing",
        RailSlot.Storage => "USB drive",
        _ => "Settings",
    };
}

/// <summary>The rail's entries (the Android app's RailItems.kt, design spec §2.1), and how they scroll.</summary>
public static class Rail
{
    /// <summary>At most this many libraries in the rail; with more, "Libraries" lists them all.</summary>
    public const int MaxLibraries = 6;

    /// <summary>
    /// The rail's geometry on the 960 × 540 canvas: entries 40 dp tall and 4 apart, from y = 67 under
    /// the avatar to Settings at y = 473, so nine show at once; the rest scroll (<see cref="Scroll"/>).
    /// </summary>
    public const float EntryHeight = 40, EntryGap = 4, Pitch = EntryHeight + EntryGap, FirstEntry = 67, SettingsY = 473;

    /// <summary>The entries' view: from the first entry's place to the gap above Settings.</summary>
    public const float View = SettingsY - EntryGap - FirstEntry;

    /// <summary>The avatar's id: it opens Who's watching?.</summary>
    public const string ProfileItem = "profile";

    /// <summary>Every library: the Libraries page.</summary>
    static readonly RailItem LibrariesItem = new RailItem("libraries", RailSlot.Libraries, new PageKey.Libraries());
    public static readonly RailItem LiveTvItem = new RailItem("live-tv", RailSlot.LiveTv, new PageKey.LiveTv());
    public static readonly RailItem NowPlayingItem = new RailItem("now-playing", RailSlot.NowPlaying, new PageKey.NowPlaying());
    public static readonly RailItem StorageItem = new RailItem("storage", RailSlot.Storage, new PageKey.Storage());
    /// <summary>Settings, pinned at the rail's foot.</summary>
    public static readonly RailItem SettingsItem = new RailItem("settings", RailSlot.Settings, new PageKey.Settings());

    /// <summary>
    /// The entries above Settings: Search, Home, the first 6 libraries in the server's order,
    /// Playlists and Collections. With more libraries a "Libraries" entry, which lists them all,
    /// takes Collections' place. Live TV, where it has channels, comes after Home; while music has
    /// a queue, Now Playing comes last; while a drive is plugged in, the drive after it. Every entry
    /// is listed: those that don't fit scroll.
    /// </summary>
    public static IReadOnlyList<RailItem> Items(IReadOnlyList<Library> libraries, bool nowPlaying = false, bool liveTv = false, bool storage = false)
    {
        var items = new List<RailItem>
        {
            new RailItem("search", RailSlot.Search, new PageKey.Search()),
            new RailItem("home", RailSlot.Home, new PageKey.Home()),
        };
        if (liveTv) items.Add(LiveTvItem);
        foreach (var library in libraries.Take(MaxLibraries))
            items.Add(new RailItem($"library:{library.Id}", RailSlot.Library, new PageKey.Library(library.Id), library.Name, library.Kind));
        if (libraries.Count > MaxLibraries) items.Add(LibrariesItem);
        items.Add(new RailItem("playlists", RailSlot.Playlists, new PageKey.Playlists()));
        if (libraries.Count <= MaxLibraries)
        {
            // A library's collections; the first video library's, as the web's Collections link.
            var first = libraries.FirstOrDefault(l => l.Kind != "music") ?? libraries.FirstOrDefault();
            if (first != null) items.Add(new RailItem("collections", RailSlot.Collections, new PageKey.Collections(first.Id)));
        }
        if (nowPlaying) items.Add(NowPlayingItem);
        if (storage) items.Add(StorageItem);
        return items;
    }

    /// <summary>
    /// How far the entries are scrolled, in dp, once entry <paramref name="focused"/> of
    /// <paramref name="count"/> has focus, from <paramref name="scroll"/>: the entry wholly in view and,
    /// moving <paramref name="direction"/> (Down from above it or the avatar, Up from below or
    /// Settings), the next one that way too, so what is beyond the edge shows coming; within the first
    /// entry at its place and the last ending above Settings. With no direction (the rail opening on its
    /// current entry, the entries made again) only the entry is brought into view. With no entry
    /// focused (the avatar, Settings), the scroll is kept within its ends. packages/tv's railScroll,
    /// which the shared fixture pins.
    /// </summary>
    public static float Scroll(float scroll, int focused, int count, Direction? direction)
    {
        var max = System.Math.Max(0, count * Pitch - EntryGap - View);
        var s = scroll;
        if (focused >= 0 && focused < count)
        {
            var top = focused * Pitch - (direction == Direction.Up && focused > 0 ? Pitch : 0);
            var bottom = focused * Pitch + EntryHeight + (direction == Direction.Down && focused + 1 < count ? Pitch : 0);
            if (bottom - s > View) s = bottom - View;
            if (top < s) s = top;
        }
        return System.Math.Max(0, System.Math.Min(max, s));
    }

    /// <summary>
    /// Where focus goes in the rail when its entries are made again (a library scan, Live TV's
    /// channels, Now Playing, a USB drive) while it was on entry <paramref name="index"/>,
    /// <paramref name="id"/>: on that entry where it still is, as packages/tv's rail, whose entries
    /// are keyed, keeps it; else on the one now in its place, the last when there are fewer; on
    /// Settings, <paramref name="items"/>' count, when there are none.
    /// </summary>
    public static int Refocus(string id, int index, IReadOnlyList<RailItem> items)
    {
        for (var i = 0; i < items.Count; i++)
            if (items[i].Id == id) return i;
        return items.Count == 0 ? 0 : System.Math.Max(0, System.Math.Min(index, items.Count - 1));
    }

    /// <summary>
    /// Which entry is current for a back stack whose first page is <paramref name="root"/>: the
    /// entry that opened it, "libraries" for a library not in the rail, or null (Who's watching?,
    /// sign-in).
    /// </summary>
    public static string? Current(PageKey root, IReadOnlyList<RailItem> items) => root switch
    {
        PageKey.Library l => items.FirstOrDefault(i => i.Key == new PageKey.Library(l.LibraryId))?.Id
            ?? items.FirstOrDefault(i => i.Slot == RailSlot.Libraries)?.Id,
        PageKey.Collections c => items.FirstOrDefault(i => i.Slot == RailSlot.Collections)?.Id
            ?? Current(new PageKey.Library(c.LibraryId), items),
        PageKey.Search => "search",
        PageKey.Storage => StorageItem.Id,
        PageKey.Settings => SettingsItem.Id,
        PageKey.Profiles => ProfileItem,
        _ => items.FirstOrDefault(i => i.Key == root)?.Id,
    };
}
