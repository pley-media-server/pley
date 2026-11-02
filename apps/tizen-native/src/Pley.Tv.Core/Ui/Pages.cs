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
    /// <summary>A DVD's disc image on the drive: its titles.</summary>
    public sealed record Disc(string Path) : PageKey;
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

/// <summary>The rail's entries (the Android app's RailItems.kt, design spec §2.1).</summary>
public static class Rail
{
    /// <summary>At most this many libraries in the rail.</summary>
    public const int MaxLibraries = 3;

    /// <summary>Entries between the avatar and Settings, at most: their 56 dp pitch fills the rail.</summary>
    public const int MaxItems = 7;

    /// <summary>The avatar's id: it opens Who's watching?.</summary>
    public const string ProfileItem = "profile";

    public static readonly RailItem LiveTvItem = new RailItem("live-tv", RailSlot.LiveTv, new PageKey.LiveTv());
    public static readonly RailItem NowPlayingItem = new RailItem("now-playing", RailSlot.NowPlaying, new PageKey.NowPlaying());
    public static readonly RailItem StorageItem = new RailItem("storage", RailSlot.Storage, new PageKey.Storage());
    /// <summary>Settings, pinned at the rail's foot.</summary>
    public static readonly RailItem SettingsItem = new RailItem("settings", RailSlot.Settings, new PageKey.Settings());

    /// <summary>
    /// The entries above Settings: Search, Home, the first 3 libraries in the server's order,
    /// Playlists and Collections. With more libraries a "Libraries" entry, which lists them all,
    /// takes Collections' place. Live TV, where it has channels, comes after Home; while music has
    /// a queue, Now Playing comes last; while a drive is plugged in, the drive after it. Each takes
    /// room as it is added.
    /// </summary>
    public static IReadOnlyList<RailItem> Items(IReadOnlyList<Library> libraries, bool nowPlaying = false, bool liveTv = false, bool storage = false)
    {
        var items = MainItems(libraries);
        if (liveTv)
        {
            var room = RoomIn(items);
            items = room.Take(2).Concat(new[] { LiveTvItem }).Concat(room.Skip(2)).ToList();
        }
        if (nowPlaying) items = RoomIn(items).Concat(new[] { NowPlayingItem }).ToList();
        if (storage) items = RoomIn(items).Concat(new[] { StorageItem }).ToList();
        return items;
    }

    /// <summary>
    /// <paramref name="items"/> with room for one more: as they are when there is room, else
    /// without Collections, else without the last library ("Libraries" lists it); with no
    /// "Libraries" yet, the last two libraries make way for it.
    /// </summary>
    static List<RailItem> RoomIn(List<RailItem> items)
    {
        if (items.Count < MaxItems) return items;
        if (items.Any(i => i.Slot == RailSlot.Collections)) return items.Where(i => i.Slot != RailSlot.Collections).ToList();
        var libraries = items.Where(i => i.Slot == RailSlot.Library).ToList();
        if (items.Any(i => i.Slot == RailSlot.Libraries)) return items.Where(i => i != libraries.Last()).ToList();
        var leaving = libraries.Skip(libraries.Count - 2).ToList();
        var at = items.IndexOf(leaving[0]);
        var kept = items.Where(i => !leaving.Contains(i)).ToList();
        return kept.Take(at).Concat(new[] { new RailItem("libraries", RailSlot.Libraries, new PageKey.Libraries()) }).Concat(kept.Skip(at)).ToList();
    }

    static List<RailItem> MainItems(IReadOnlyList<Library> libraries)
    {
        var items = new List<RailItem>
        {
            new RailItem("search", RailSlot.Search, new PageKey.Search()),
            new RailItem("home", RailSlot.Home, new PageKey.Home()),
        };
        foreach (var library in libraries.Take(MaxLibraries))
            items.Add(new RailItem($"library:{library.Id}", RailSlot.Library, new PageKey.Library(library.Id), library.Name, library.Kind));
        if (libraries.Count > MaxLibraries) items.Add(new RailItem("libraries", RailSlot.Libraries, new PageKey.Libraries()));
        items.Add(new RailItem("playlists", RailSlot.Playlists, new PageKey.Playlists()));
        if (libraries.Count <= MaxLibraries)
        {
            // A library's collections; the first video library's, as the web's Collections link.
            var first = libraries.FirstOrDefault(l => l.Kind != "music") ?? libraries.FirstOrDefault();
            if (first != null) items.Add(new RailItem("collections", RailSlot.Collections, new PageKey.Collections(first.Id)));
        }
        return items;
    }

    /// <summary>
    /// Which entry is current for a back stack whose first page is <paramref name="root"/>: the
    /// entry that opened it, "libraries" for a library not in the rail, or null (Who's watching?, sign-in).
    /// </summary>
    public static string? Current(PageKey root, IReadOnlyList<RailItem> items) => root switch
    {
        PageKey.Library l => items.FirstOrDefault(i => i.Key == new PageKey.Library(l.LibraryId))?.Id
            ?? items.FirstOrDefault(i => i.Slot == RailSlot.Libraries)?.Id,
        PageKey.Collections c => items.FirstOrDefault(i => i.Slot == RailSlot.Collections)?.Id
            ?? Current(new PageKey.Library(c.LibraryId), items),
        PageKey.Search => "search",
        PageKey.Storage or PageKey.Disc => StorageItem.Id,
        PageKey.Settings => SettingsItem.Id,
        PageKey.Profiles => ProfileItem,
        _ => items.FirstOrDefault(i => i.Key == root)?.Id,
    };
}
