using System.Collections.Generic;
using System.Linq;
using Pley.Tv.Core.Api;
using Pley.Tv.Core.Requests;

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
    /// <summary>Discover: TMDB's lists, to ask for what the libraries lack.</summary>
    public sealed record Discover : PageKey;
    /// <summary>One of Discover's lists whole (a row's See all).</summary>
    public sealed record DiscoverList(Pley.Tv.Core.Requests.DiscoverList List) : PageKey;
    /// <summary>Every title TMDB finds for <paramref name="Query"/> (Search's Discover row's See all).</summary>
    public sealed record DiscoverResults(string Query) : PageKey;
    /// <summary>A title found on TMDB; <paramref name="Pick"/>: Which seasons? opens with the page.</summary>
    public sealed record DiscoverTitle(string Kind, long TmdbId, bool Pick = false) : PageKey;
    /// <summary>Your requests, or for managers everyone's.</summary>
    public sealed record Requests : PageKey;
}

/// <summary>The kinds of place the rail goes to.</summary>
public enum RailSlot
{
    Search,
    Home,
    Discover,
    Requests,
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
/// <param name="Requests">The way into requests: whose requests it leads to (the managers' Requests), and the number that waits there.</param>
public sealed record RailItem(string Id, RailSlot Slot, PageKey Key, string? Name = null, string? LibraryKind = null, RequestsEntry? Requests = null)
{
    /// <summary>The entry's icon, by Lucide's name.</summary>
    public string Icon => Slot switch
    {
        RailSlot.Search => "search",
        RailSlot.Home => "house",
        RailSlot.Discover => "compass",
        RailSlot.Requests => "list-video",
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

    /// <summary>What the entry says (the Android app's tv_shell_rail_* strings), in the language shown; a library's name is the server's.</summary>
    public string Label => Slot switch
    {
        RailSlot.Search => Strings.T("shell.rail.search"),
        RailSlot.Home => Strings.T("shell.rail.home"),
        RailSlot.Discover => RequestText.Discover,
        RailSlot.Requests => Requests?.Manage == true ? RequestText.Manage : RequestText.Yours,
        RailSlot.LiveTv => Strings.T("shell.rail.liveTv"),
        RailSlot.Library => Name ?? "",
        RailSlot.Libraries => Strings.T("shell.rail.libraries"),
        RailSlot.Playlists => Strings.T("shell.rail.playlists"),
        RailSlot.Collections => Strings.T("shell.rail.collections"),
        RailSlot.NowPlaying => Strings.T("shell.rail.nowPlaying"),
        RailSlot.Storage => Strings.T("shell.rail.storage"),
        _ => Strings.T("shell.rail.settings"),
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

    /// <summary>Whole entries in the view, the last one's gap aside: nine.</summary>
    public const int Fits = (int)((View + EntryGap) / Pitch);

    /// <summary>The entries shown above the focused one while they scroll under it (<see cref="Scroll"/>).</summary>
    public const int Pivot = 3;

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
    /// a queue, Now Playing comes last; while a drive is plugged in, the drive after it. The way into requests (<paramref name="requests"/>,
    /// <see cref="RequestsItem"/>) comes after Home, before Live TV. Every entry is listed: those that
    /// don't fit scroll.
    /// </summary>
    public static IReadOnlyList<RailItem> Items(IReadOnlyList<Library> libraries, bool nowPlaying = false, bool liveTv = false, bool storage = false, RailItem? requests = null)
    {
        var items = new List<RailItem>
        {
            new RailItem("search", RailSlot.Search, new PageKey.Search()),
            new RailItem("home", RailSlot.Home, new PageKey.Home()),
        };
        if (requests != null) items.Add(requests);
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
    /// How far the entries are scrolled, in dp, while entry <paramref name="focused"/> of
    /// <paramref name="count"/> has focus: by whole entries, so none is ever cut in half at either end.
    /// Going down, focus moves until it is the fourth entry shown, then stays there while the entries
    /// move under it an entry a press, until the last ones are all in view and focus goes on to the
    /// last; going up is the same backwards. Where an entry shows depends on it alone, never on the way
    /// focus came or on <paramref name="scroll"/>, as the app's lists keep their focused row in one
    /// place. With no entry focused (the avatar, Settings: -1 or <paramref name="count"/>),
    /// <paramref name="scroll"/> is kept, within the ends. packages/tv's railScroll, which the shared
    /// fixture pins.
    /// </summary>
    public static float Scroll(float scroll, int focused, int count)
    {
        var max = System.Math.Max(0, count - Fits) * Pitch;
        var s = focused >= 0 && focused < count ? (focused - Pivot) * Pitch : scroll;
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
    /// The rail's way into requests, as Android TV has it: Discover where the profile may ask, else
    /// its requests (the managers' Requests) where it has any to see, else none. It carries their number.
    /// </summary>
    public static RailItem? RequestsItem(RequestsEntry entry) =>
        entry.Discover ? new RailItem("discover", RailSlot.Discover, new PageKey.Discover(), Requests: entry)
        : entry.Requests ? new RailItem("requests", RailSlot.Requests, new PageKey.Requests(), Requests: entry)
        : null;

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
        // Discover's pages, and the requests reached from it, under whichever of the two the rail has.
        PageKey.Discover or PageKey.Requests or PageKey.DiscoverList or PageKey.DiscoverResults or PageKey.DiscoverTitle =>
            (items.FirstOrDefault(i => i.Key == root) ?? items.FirstOrDefault(i => i.Slot is RailSlot.Discover or RailSlot.Requests))?.Id,
        PageKey.Storage => StorageItem.Id,
        PageKey.Settings => SettingsItem.Id,
        PageKey.Profiles => ProfileItem,
        _ => items.FirstOrDefault(i => i.Key == root)?.Id,
    };
}
