using System;
using System.Collections.Generic;
using System.Globalization;
using System.Linq;
using System.Text;
using Pley.Tv.Core.Api;

namespace Pley.Tv.Core.Ui;

/// <summary>A library page's tabs (the Android app's LibraryTab).</summary>
public enum LibraryTab
{
    Recommended,
    Library,
    Collections,
}

/// <summary>A listing's order, as the server names it.</summary>
public static class SortOrder
{
    public const string Ascending = "asc";
    public const string Descending = "desc";
}

/// <summary>Saving a library's listing as a smart playlist: <paramref name="Filter"/> is in Plex's syntax (<see cref="SmartFilter.Of"/>).</summary>
public sealed record SaveSmartPlaylistPage(long LibraryId, string Filter) : PageKey;

/// <summary>
/// How a library page is shown (the Android app's LibraryParams): its tab, and the Library tab's
/// sort, order and filters, as the web keeps them in the page's address
/// (apps/web/src/pages/LibraryPage.tsx). <see cref="ToQueryString"/> and <see cref="Parse"/> turn
/// them into that address's query and back ("tab=collections", "sort=year&amp;watched=false&amp;genre=Comedy"),
/// which is also how a library is left, to open so again.
/// </summary>
/// <param name="Sort">One of <see cref="LibrarySort"/>.</param>
/// <param name="Order">Null is the sort's own order (<see cref="DefaultOrder"/>).</param>
/// <param name="Watched">The web's: false lists unwatched titles only, true watched ones, null both.</param>
public sealed record LibraryParams(
    LibraryTab Tab = LibraryTab.Library,
    string Sort = LibrarySort.Title,
    string? Order = null,
    bool? Watched = null,
    string? Genre = null,
    int? Year = null)
{
    /// <summary>Every sort, in the web's order.</summary>
    public static readonly IReadOnlyList<string> AllSorts = new[]
    {
        LibrarySort.Title, LibrarySort.Year, LibrarySort.Added, LibrarySort.Rating, LibrarySort.UserRating,
        LibrarySort.Release, LibrarySort.Duration, LibrarySort.LastPlayed, LibrarySort.Bitrate,
    };

    static readonly IReadOnlyList<string> MusicSorts = new[] { LibrarySort.Title, LibrarySort.Added, LibrarySort.UserRating, LibrarySort.LastPlayed };

    static readonly IReadOnlyList<string> ShowSorts = AllSorts.Where(s => s != LibrarySort.Duration).ToList();

    static readonly IReadOnlyList<LibraryTab> AllTabs = new[] { LibraryTab.Recommended, LibraryTab.Library, LibraryTab.Collections };

    static readonly IReadOnlyList<LibraryTab> MusicTabs = new[] { LibraryTab.Recommended, LibraryTab.Library };

    /// <summary>The order the listing is in: the one chosen, else the sort's own.</summary>
    public string EffectiveOrder => Order ?? DefaultOrder(Sort);

    /// <summary>A filter narrows the listing (the sort doesn't count).</summary>
    public bool Filtered => Watched != null || Genre != null || Year != null;

    /// <summary>The listing as the server is asked for it, from its start.</summary>
    public LibraryQuery Query() => new LibraryQuery { Sort = Sort, Order = EffectiveOrder, Watched = Watched, Genre = Genre, Year = Year };

    /// <summary>The tab, sort, order and filters: how a library is left, to open so again.</summary>
    public string View() => ToQueryString();

    /// <summary>Without any filter: "Clear filters".</summary>
    public LibraryParams Cleared() => this with { Watched = null, Genre = null, Year = null };

    /// <summary>Sorted by <paramref name="sort"/> in its own order, as choosing a sort does on the web.</summary>
    public LibraryParams SortedBy(string sort) => this with { Sort = sort, Order = null };

    /// <summary>The other order.</summary>
    public LibraryParams Flipped() => this with { Order = EffectiveOrder == SortOrder.Ascending ? SortOrder.Descending : SortOrder.Ascending };

    /// <summary>
    /// What a library of <paramref name="kind"/> can show: a sort it offers (else Title), and no
    /// filters for music, which has none on the web, nor a Collections tab.
    /// </summary>
    public LibraryParams ValidFor(string? kind)
    {
        var sort = SortsFor(kind).Contains(Sort) ? Sort : LibrarySort.Title;
        var valid = this with { Sort = sort, Order = sort != Sort ? null : Order };
        if (kind != "music") return valid;
        return valid with { Tab = Tab == LibraryTab.Collections ? LibraryTab.Library : Tab, Watched = null, Genre = null, Year = null };
    }

    /// <summary>The web's query: parameters at their default are left out, so a fresh page is "" and the Library tab is never named.</summary>
    public string ToQueryString()
    {
        var parts = new List<(string Key, string Value)>();
        if (Tab != LibraryTab.Library) parts.Add(("tab", Wire(Tab)));
        if (Sort != LibrarySort.Title) parts.Add(("sort", Sort));
        if (Order != null && Order != DefaultOrder(Sort)) parts.Add(("order", Order));
        if (Watched != null) parts.Add(("watched", Watched.Value ? "true" : "false"));
        if (Genre != null) parts.Add(("genre", Genre));
        if (Year != null) parts.Add(("year", Year.Value.ToString(CultureInfo.InvariantCulture)));
        return string.Join("&", parts.Select(p => $"{p.Key}={Encode(p.Value)}"));
    }

    /// <summary>Sorts the web offers each kind of library, in its order.</summary>
    public static IReadOnlyList<string> SortsFor(string? kind) => kind switch
    {
        "music" => MusicSorts,
        "shows" => ShowSorts,
        _ => AllSorts,
    };

    /// <summary>The tabs of a library of <paramref name="kind"/>: music has no collections.</summary>
    public static IReadOnlyList<LibraryTab> TabsFor(string? kind) => kind == "music" ? MusicTabs : AllTabs;

    /// <summary>Titles A to Z; everything else newest, biggest or best first, as on the web.</summary>
    public static string DefaultOrder(string sort) => sort == LibrarySort.Title ? SortOrder.Ascending : SortOrder.Descending;

    /// <summary>The tab's name in the web's address.</summary>
    public static string Wire(LibraryTab tab) => tab switch
    {
        LibraryTab.Recommended => "recommended",
        LibraryTab.Collections => "collections",
        _ => "library",
    };

    /// <summary>
    /// <paramref name="query"/> as the web's page reads it: unknown values fall back to the
    /// defaults, a page opened on <paramref name="tab"/> (a route's) unless the query names one.
    /// </summary>
    public static LibraryParams Parse(string query, LibraryTab tab = LibraryTab.Library)
    {
        var values = new Dictionary<string, string>();
        foreach (var part in query.Split('&'))
        {
            if (part.Length == 0) continue;
            var at = part.IndexOf('=');
            if (at < 0) values[Decode(part)] = "";
            else values[Decode(part.Substring(0, at))] = Decode(part.Substring(at + 1));
        }
        string? Value(string key) => values.TryGetValue(key, out var v) ? v : null;
        var sort = Value("sort") is { } s && AllSorts.Contains(s) ? s : LibrarySort.Title;
        var named = AllTabs.Where(t => Wire(t) == Value("tab")).Select(t => (LibraryTab?)t).FirstOrDefault();
        return new LibraryParams(
            named ?? tab,
            sort,
            Value("order") is SortOrder.Ascending or SortOrder.Descending ? Value("order") : null,
            Value("watched") switch
            {
                "true" => true,
                "false" => false,
                _ => null,
            },
            Value("genre") is { Length: > 0 } genre ? genre : null,
            int.TryParse(Value("year"), NumberStyles.AllowLeadingSign, CultureInfo.InvariantCulture, out var year) ? year : null);
    }

    /// <summary>
    /// <paramref name="value"/> as Java's URLEncoder writes it, as the Android app's addresses
    /// have it: letters, digits and <c>.-*_</c> as they are, a space as <c>+</c>, the rest in UTF-8 escapes.
    /// </summary>
    public static string Encode(string value)
    {
        var b = new StringBuilder(value.Length);
        foreach (var octet in Encoding.UTF8.GetBytes(value))
        {
            var c = (char)octet;
            if (c is >= 'a' and <= 'z' or >= 'A' and <= 'Z' or >= '0' and <= '9' or '.' or '-' or '*' or '_') b.Append(c);
            else if (c == ' ') b.Append('+');
            else b.Append('%').Append(octet.ToString("X2", CultureInfo.InvariantCulture));
        }
        return b.ToString();
    }

    /// <summary><paramref name="value"/> read as Java's URLDecoder reads it; one it can't read is taken as it is.</summary>
    public static string Decode(string value)
    {
        var bytes = new List<byte>(value.Length);
        for (var i = 0; i < value.Length; i++)
        {
            var c = value[i];
            if (c == '+')
            {
                bytes.Add((byte)' ');
            }
            else if (c == '%')
            {
                if (i + 2 >= value.Length) return value;
                if (!byte.TryParse(value.Substring(i + 1, 2), NumberStyles.AllowHexSpecifier, CultureInfo.InvariantCulture, out var octet)) return value;
                bytes.Add(octet);
                i += 2;
            }
            else
            {
                bytes.AddRange(Encoding.UTF8.GetBytes(c.ToString()));
            }
        }
        return Encoding.UTF8.GetString(bytes.ToArray());
    }
}

/// <summary>Something read from the server: not known yet, known, or failed (with what showed before, if anything).</summary>
public abstract record Loadable<T>
{
    public sealed record Loading : Loadable<T>;

    public sealed record Ready(T Data) : Loadable<T>;

    /// <param name="Stale">What showed before the failure, which stays on screen.</param>
    public sealed record Failed(Exception Error, T? Stale = default) : Loadable<T>;

    /// <summary>What to show: the value, or a failure's stale one; default when there is none.</summary>
    public T? Value => this switch
    {
        Ready r => r.Data,
        Failed f => f.Stale,
        _ => default,
    };
}

/// <summary>How an item is drawn in a library's grid or rows (the Android app's TileLook).</summary>
public enum TileLook
{
    /// <summary>Movies and shows, 2:3.</summary>
    Poster,
    /// <summary>Continue Watching, 16:9 stills.</summary>
    Landscape,
    /// <summary>Music, 1:1.</summary>
    Square,
    /// <summary>A poster that says what the files are like ("4K", "DV", "HEVC"; "42 Mbps · 23.4 GB"): sorted by bitrate.</summary>
    Bitrate,
}

/// <summary>
/// A library's items as one list, however many there are. A big library is held as a window
/// around what the grid shows: <see cref="Items"/> then starts at position <see cref="Offset"/>
/// of the listing, and the indexer is null outside it until the window moves there.
/// </summary>
public sealed record LibraryItems(int Total, IReadOnlyList<ItemSummary> Items, int Offset = 0, bool Windowed = false)
{
    /// <summary>The item at <paramref name="position"/> of the whole listing; null while it isn't loaded.</summary>
    public ItemSummary? this[int position] => position - Offset is var at && at >= 0 && at < Items.Count ? Items[at] : null;

    /// <summary>Every item is loaded (never true of a windowed listing).</summary>
    public bool Complete => !Windowed && Items.Count >= Total;
}

/// <summary>
/// A grid's cells, however many the listing has (<see cref="Total"/>): the items loaded so far
/// start at position <see cref="Offset"/>; the others are null until the listing's window gets
/// there, and the screen draws skeletons for them, focusable like tiles. A cell's tile is made
/// the first time it is asked for, and a tile made for the same item before is kept.
/// </summary>
public sealed class GridCells
{
    readonly IReadOnlyList<ItemSummary> items;
    readonly TileModel?[] tiles;
    readonly TileLook look;
    readonly string origin;

    GridCells(int total, int offset, IReadOnlyList<ItemSummary> items, TileModel?[] tiles, TileLook look, string origin)
    {
        Total = total;
        Offset = offset;
        this.items = items;
        this.tiles = tiles;
        this.look = look;
        this.origin = origin;
    }

    public int Total { get; }

    public int Offset { get; }

    /// <summary>How many cells are loaded, from <see cref="Offset"/>.</summary>
    public int Loaded => items.Count;

    /// <summary>Every cell is loaded.</summary>
    public bool Complete => Offset == 0 && items.Count >= Total;

    /// <summary>The tile at <paramref name="position"/>; null while it isn't loaded.</summary>
    public TileModel? this[int position]
    {
        get
        {
            var at = position - Offset;
            if (at < 0 || at >= items.Count) return null;
            return tiles[at] ??= LibraryRules.Tile(items[at], look, origin);
        }
    }

    /// <summary>The item at <paramref name="position"/>; null while it isn't loaded.</summary>
    public ItemSummary? ItemAt(int position) => position - Offset is var at && at >= 0 && at < items.Count ? items[at] : null;

    /// <summary>The item id at <paramref name="position"/>, or -1.</summary>
    public long IdAt(int position) => ItemAt(position)?.Id ?? -1;

    /// <summary>A stable key for the cell at <paramref name="position"/>: its item's id, or a negative one for a cell still loading (ids are positive).</summary>
    public long Key(int position) => IdAt(position) is var id && id >= 0 ? id : -(position + 1L);

    /// <summary>Where <paramref name="itemId"/> is in the listing, if loaded; else -1.</summary>
    public int PositionOf(long itemId)
    {
        for (var i = 0; i < items.Count; i++)
            if (items[i].Id == itemId) return i + Offset;
        return -1;
    }

    /// <summary>
    /// The item focus goes to if the one at <paramref name="position"/> leaves the grid (marked
    /// watched with Unwatched on): the next, else the one before; -1 with neither.
    /// </summary>
    public long Near(int position)
    {
        if (position < 0) return -1;
        var next = IdAt(position + 1);
        return next >= 0 ? next : IdAt(position - 1);
    }

    public static GridCells Empty { get; } = new GridCells(0, 0, Array.Empty<ItemSummary>(), Array.Empty<TileModel?>(), TileLook.Poster, "");

    /// <summary>
    /// The cells of <paramref name="listing"/>, keeping the tiles <paramref name="before"/> made
    /// for the same items (the listing keeps an item's instance until it changes): a listing that
    /// grows or moves its window makes only what is new.
    /// </summary>
    internal static GridCells Of(LibraryItems listing, TileLook look, string origin, GridCells? before)
    {
        var tiles = new TileModel?[listing.Items.Count];
        if (before != null && before.look == look && before.origin == origin)
        {
            var made = new Dictionary<ItemSummary, TileModel>(new Identity());
            for (var i = 0; i < before.items.Count; i++)
                if (before.tiles[i] is { } tile) made[before.items[i]] = tile;
            if (made.Count > 0)
                for (var i = 0; i < tiles.Length; i++)
                    if (made.TryGetValue(listing.Items[i], out var tile)) tiles[i] = tile;
        }
        return new GridCells(Math.Max(listing.Total, listing.Offset + listing.Items.Count), listing.Offset, listing.Items, tiles, look, origin);
    }

    /// <summary>Items compared as instances: an item that changed is a new one, and gets a new tile.</summary>
    sealed class Identity : IEqualityComparer<ItemSummary>
    {
        public bool Equals(ItemSummary? x, ItemSummary? y) => ReferenceEquals(x, y);

        public int GetHashCode(ItemSummary obj) => System.Runtime.CompilerServices.RuntimeHelpers.GetHashCode(obj);
    }
}

/// <summary>The Library tab's grid: its cells, the A to Z when it has one, how its tiles look.</summary>
/// <param name="Index">Sorted by title, every item loaded, more than 30 of them.</param>
public sealed record LibraryGrid(Loadable<GridCells> Cells, TileLook Look, AlphaIndex? Index)
{
    /// <summary>The tile the grid draws: squares for music, the library grid's posters otherwise.</summary>
    public TileKind Kind => Look == TileLook.Square ? TileKind.Square : TileKind.LibraryGrid;

    /// <summary>The grid's columns on TV (<see cref="LibraryRules.GridColumns"/>).</summary>
    public int Columns => LibraryRules.GridColumns(Kind);
}

/// <summary>
/// A row of the Recommended tab: one of the library's hubs, or its collections
/// (<see cref="CollectionsKey"/>). <see cref="Opens"/> says where each tile goes, by position.
/// </summary>
/// <param name="Title">The server's title; "Collections" for the collections.</param>
/// <param name="SeeAll">The Library tab's view its "See all" shows (<see cref="LibraryModel.SeeAll"/>); null without one. The collections' See all opens the Collections tab.</param>
public sealed record LibraryRow(string Key, string Title, TileLook Look, IReadOnlyList<TileModel> Tiles, IReadOnlyList<PageKey> Opens, string? SeeAll = null)
{
    /// <summary>The key of the library's collections row.</summary>
    public const string CollectionsKey = "collections";

    public bool IsCollections => Key == CollectionsKey;

    /// <summary>The library's Continue Watching: a tile's actions offer to take it out.</summary>
    public bool ContinueWatching => Key == "continue_watching" || Key.StartsWith("continue_watching:", StringComparison.Ordinal);

    /// <summary>The tile the row draws on TV.</summary>
    public TileKind Kind => Look switch
    {
        TileLook.Landscape => TileKind.Landscape,
        TileLook.Square => TileKind.Square,
        _ => TileKind.Poster,
    };
}

/// <summary>A collection's tile, with its count for the badge on its poster.</summary>
public sealed record CollectionTile(long Id, long Count, TileModel Tile);

/// <summary>
/// A library page (design spec §4.2; the Android app's LibraryUiState): its name, the tabs, and
/// each tab's content. <see cref="Library"/> is null until known; <see cref="Missing"/> when the
/// server has no such library for this profile.
/// </summary>
public sealed record LibraryState
{
    static readonly FiltersAnswer NoFilters = new FiltersAnswer(Array.Empty<string>(), Array.Empty<int>());

    public long LibraryId { get; init; }
    public Library? Library { get; init; }
    public bool Missing { get; init; }
    /// <summary>Why the library couldn't be read, while nothing saved shows.</summary>
    public Exception? LibraryError { get; init; }
    /// <summary>The tab, sort and filters shown: those chosen, as the library's kind allows them.</summary>
    public LibraryParams Params { get; init; } = new LibraryParams();
    /// <summary>The tab the page opened on: content takes focus by itself only there, before another tab is chosen.</summary>
    public LibraryTab OpenedOn { get; init; } = LibraryTab.Library;
    public IReadOnlyList<LibraryTab> Tabs { get; init; } = LibraryParams.TabsFor(null);
    /// <summary>The sorts the Sort panel offers, in order.</summary>
    public IReadOnlyList<string> Sorts { get; init; } = LibraryParams.SortsFor(null);
    /// <summary>Genres and years the listing can be filtered by (none for music).</summary>
    public FiltersAnswer Filters { get; init; } = NoFilters;
    public LibraryGrid Grid { get; init; } = new LibraryGrid(new Loadable<GridCells>.Loading(), TileLook.Poster, null);
    public GridDensity Density { get; init; } = GridDensity.Medium;
    /// <summary>The Recommended tab's rows: the library's hubs, then the collections.</summary>
    public Loadable<IReadOnlyList<LibraryRow>> Recommended { get; init; } = new Loadable<IReadOnlyList<LibraryRow>>.Loading();
    public Loadable<IReadOnlyList<CollectionTile>> Collections { get; init; } = new Loadable<IReadOnlyList<CollectionTile>>.Loading();

    /// <summary>"movies", "shows" or "music", once known.</summary>
    public string? Kind => Library?.Kind;

    /// <summary>The listing is narrowed by a filter.</summary>
    public bool Filtered => Params.Filtered;

    /// <summary>Where a tile of the grid goes: an artist's page in music, else the item's.</summary>
    public PageKey Opens(long itemId) => Kind == "music" ? new ArtistPage(itemId) : new PageKey.Item(itemId);
}

/// <summary>The library page's rules that aren't words (the Android app's LibraryViewModel, LibraryTiles and TvTiles).</summary>
public static class LibraryRules
{
    /// <summary>Listings this short have no A to Z (the web's 30).</summary>
    public const int AlphaMinItems = AlphaIndex.MinItems;

    // The TV canvas (design spec §4.2): the grid starts at the gutter and ends left of the jump
    // control, 14 dp clear of it, inside the safe area.
    const float CanvasWidth = 960;
    const float Gutter = 96;
    const float SafeHorizontal = 48;
    const float JumpSize = 40;
    const float TileGap = 14;

    /// <summary>
    /// The TV grid's columns: as many wells of <paramref name="kind"/> as fit between the gutter
    /// and the jump control, 14 dp apart (seven posters of 96).
    /// </summary>
    public static int GridColumns(TileKind kind)
    {
        var end = CanvasWidth - SafeHorizontal - JumpSize - TileGap;
        return Math.Max(1, (int)Math.Floor((end - Gutter + TileGap) / (kind.Width + TileGap)));
    }

    /// <summary>The first position of <paramref name="position"/>'s row: where a jump scrolls to, before focusing the position.</summary>
    public static int RowStart(int position, int columns) => columns <= 1 ? position : position - position % columns;

    /// <summary>How the grid draws a library of <paramref name="kind"/> listed by <paramref name="sort"/>.</summary>
    public static TileLook GridLook(string? kind, string sort) =>
        kind == "music" ? TileLook.Square : sort == LibrarySort.Bitrate ? TileLook.Bitrate : TileLook.Poster;

    /// <summary>How a Recommended row draws its tiles: Continue Watching as stills, music as squares.</summary>
    public static TileLook RowLook(string hubKey, string? kind) =>
        hubKey.StartsWith("continue_watching", StringComparison.Ordinal) ? TileLook.Landscape : kind == "music" ? TileLook.Square : TileLook.Poster;

    /// <summary>Where opening <paramref name="item"/> goes: artists and albums have pages of their own.</summary>
    public static PageKey Opens(ItemSummary item) => item.Kind switch
    {
        "artist" => new ArtistPage(item.Id),
        "album" => new AlbumPage(item.Id),
        _ => new PageKey.Item(item.Id),
    };

    /// <summary>
    /// <paramref name="item"/>'s tile drawn as <paramref name="look"/> (the Android app's TvTiles):
    /// sorted by bitrate, it says what the files are like.
    /// </summary>
    public static TileModel Tile(ItemSummary item, TileLook look, string origin)
    {
        switch (look)
        {
            case TileLook.Landscape:
                return TileModel.Of(item, TileKind.Landscape, origin);
            case TileLook.Square:
                return TileModel.Of(item, TileKind.Square, origin);
            case TileLook.Bitrate:
                var model = TileModel.Of(item, TileKind.LibraryGrid, origin);
                var line = LibraryText.MediaLine(item.Media);
                return model with
                {
                    Subtitle = line.Length > 0 ? line : model.Subtitle,
                    Badges = LibraryText.MediaBadges(item.Media),
                    Description = string.Join(", ", new[] { model.Description, line }.Where(s => s.Length > 0)),
                };
            default:
                return TileModel.Of(item, TileKind.LibraryGrid, origin) with { Badges = LibraryText.MediaBadges(item.Media) };
        }
    }

    /// <summary>The row of <paramref name="hub"/> in a library of <paramref name="kind"/>, with its See all.</summary>
    public static LibraryRow Row(Hub hub, string? kind, string origin)
    {
        var look = RowLook(hub.Key, kind);
        return new LibraryRow(
            hub.Key,
            hub.Title,
            look,
            hub.Items.Select(i => Tile(i, look, origin)).ToList(),
            hub.Items.Select(Opens).ToList(),
            SeeAllView(hub.Key));
    }

    /// <summary>The Library tab's view a row of <paramref name="hubKey"/> lists its items in, for its See all; null for rows without one.</summary>
    public static string? SeeAllView(string hubKey) =>
        hubKey.StartsWith("recently_added", StringComparison.Ordinal) ? "sort=added&order=desc"
        : hubKey.StartsWith("recently_released", StringComparison.Ordinal) ? "sort=release&order=desc"
        : hubKey.StartsWith("top_rated", StringComparison.Ordinal) ? "sort=rating&order=desc"
        : null;

    /// <summary>
    /// Where focus goes if tile <paramref name="tileId"/> leaves row <paramref name="rowKey"/>
    /// (taken out of Continue Watching): the next tile, else the one before, else the next row's
    /// first (or the row before's). Null with none.
    /// </summary>
    public static (string RowKey, long TileId)? StandIn(IReadOnlyList<LibraryRow> rows, string rowKey, long tileId)
    {
        var at = -1;
        for (var r = 0; r < rows.Count; r++)
            if (rows[r].Key == rowKey) at = r;
        if (at < 0) return null;
        var tiles = rows[at].Tiles;
        var i = -1;
        for (var t = 0; t < tiles.Count; t++)
            if (tiles[t].Id == tileId) i = t;
        if (i >= 0 && i + 1 < tiles.Count) return (rowKey, tiles[i + 1].Id);
        if (i > 0) return (rowKey, tiles[i - 1].Id);
        var other = at + 1 < rows.Count ? rows[at + 1] : at > 0 ? rows[at - 1] : null;
        return other is { Tiles.Count: > 0 } ? (other.Key, other.Tiles[0].Id) : null;
    }
}

/// <summary>A library in the list of libraries (the TV's Libraries page).</summary>
/// <param name="Count">"412 movies", "38 shows", "120 artists".</param>
/// <param name="Scanning">The server is scanning it now.</param>
public sealed record LibraryEntry(Library Library, string Count, bool Scanning);

/// <summary>The list of libraries (the TV's Libraries page; the Android app's LibrariesState).</summary>
public abstract record LibrariesState
{
    /// <summary>Nothing known yet.</summary>
    public sealed record Loading : LibrariesState;

    /// <summary>The list couldn't be loaded and none was saved: "Couldn't load your libraries".</summary>
    public sealed record Failed(Exception Error) : LibrariesState;

    /// <summary>The libraries in the server's order, with moves not saved yet; empty: "No libraries yet".</summary>
    /// <param name="Error">The server couldn't be asked again: the list is what was saved.</param>
    /// <param name="Sortable">They can be put in another order: there are two or more, and the sign-in may.</param>
    /// <param name="Playlists">The rail has no entry for Playlists (<see cref="Rail.PlaylistsOnLibrariesPage"/>): the page lists them after the libraries.</param>
    public sealed record Ready(IReadOnlyList<LibraryEntry> Libraries, Exception? Error = null, bool Refreshing = false, bool Sortable = false, bool Playlists = false) : LibrariesState;
}

/// <summary>Something the Libraries page tells the person once, as a toast.</summary>
public abstract record LibrariesEvent
{
    /// <summary>A new order wasn't saved: the list is back in the server's.</summary>
    public sealed record Failed(Exception Error) : LibrariesEvent;

    /// <summary>The server is older than libraries' own order: the list is back in its order.</summary>
    public sealed record NeedsNewerServer : LibrariesEvent;
}

/// <summary>The Libraries page's rules (the Android app's librariesState and TvLibraries.kt).</summary>
public static class LibrariesRules
{
    /// <summary>The page's cards a row.</summary>
    public const int Columns = 3;

    /// <summary>
    /// The list from the libraries the app holds and the scans running, in <paramref name="order"/>
    /// (the libraries' ids, those it doesn't name after) while moves are made or saved.
    /// <paramref name="mayChange"/>: the sign-in may change the profile's choices, its libraries' order among them.
    /// <paramref name="playlists"/>: the rail has no entry for Playlists.
    /// </summary>
    public static LibrariesState StateOf(IReadOnlyList<Library>? libraries, Exception? error, IReadOnlyList<ScanState> scans,
        bool refreshing = false, IReadOnlyList<long>? order = null, bool mayChange = false, bool playlists = false)
    {
        if (libraries == null) return error != null ? new LibrariesState.Failed(error) : new LibrariesState.Loading();
        var scanning = new HashSet<long>(scans.Select(s => s.LibraryId));
        IEnumerable<Library> shown = libraries;
        if (order != null)
        {
            var at = new Dictionary<long, int>();
            for (var i = 0; i < order.Count; i++) at[order[i]] = i;
            shown = libraries.OrderBy(l => at.TryGetValue(l.Id, out var i) ? i : int.MaxValue);
        }
        return new LibrariesState.Ready(
            shown.Select(l => new LibraryEntry(l, LibraryText.LibraryCount(l), l.Scanning || scanning.Contains(l.Id))).ToList(),
            error,
            refreshing,
            mayChange && libraries.Count > 1,
            playlists);
    }

    /// <summary>Whether the sign-in may change the profile's choices: a profile's own, or an app allowed to play.</summary>
    public static bool MayChange(User? me) =>
        me == null || string.IsNullOrWhiteSpace(me.Scope) || me.Scope!.Split(' ').Contains("play");

    /// <summary>
    /// Where a lifted card at <paramref name="index"/> goes moved <paramref name="by"/> places
    /// (±1, or ±<see cref="Columns"/> for a row) among <paramref name="count"/>: down onto a
    /// shorter last row it lands at its end; past either end it stays.
    /// </summary>
    public static int MoveTarget(int index, int by, int count)
    {
        var last = count - 1;
        var to = index + by;
        if (to > last && index / Columns < last / Columns) return last;
        return to < 0 || to > last ? index : to;
    }
}
