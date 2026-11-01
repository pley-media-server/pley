using System;
using System.Collections.Generic;
using System.Globalization;
using System.Linq;
using Pley.Tv.Core.Api;

namespace Pley.Tv.Core.Ui;

/// <summary>A chip over the Library tab's grid, in the order they stand.</summary>
public enum LibraryChipKind
{
    /// <summary>Opens the Sort panel; says the sort, its icon the order.</summary>
    Sort,
    /// <summary>Opens the Filter panel.</summary>
    Filter,
    /// <summary>Unwatched only, or both (<see cref="LibraryModel.ToggleUnwatched"/>).</summary>
    Unwatched,
    /// <summary>Opens the Genre panel; says the genre chosen.</summary>
    Genre,
    /// <summary>Opens the Year panel; says the year chosen.</summary>
    Year,
}

/// <summary>A chip (design spec §2.6): what it says, whether it is on (a check leads it then, in place of <see cref="Icon"/>), and its icon by Lucide's name.</summary>
public sealed record LibraryChip(LibraryChipKind Kind, string Label, bool Selected = false, string? Icon = null);

/// <summary>The side panels the chips open.</summary>
public enum LibraryPanel
{
    Sort,
    Filter,
    Genre,
    Year,
}

/// <summary>A row of a library's side panel, and what its <see cref="LibraryPanelRow.Value"/> is.</summary>
public enum LibraryRowKind
{
    Heading,
    /// <summary>A sort (<see cref="LibrarySort"/>).</summary>
    Sort,
    /// <summary>An order: "asc" or "desc".</summary>
    Order,
    /// <summary>Which titles show: "false" unwatched, "true" watched, null all.</summary>
    Show,
    /// <summary>Opens the Genre panel; <see cref="LibraryPanelRow.Detail"/> is the genre chosen.</summary>
    Genres,
    /// <summary>Opens the Year panel; <see cref="LibraryPanelRow.Detail"/> is the year chosen.</summary>
    Years,
    ClearFilters,
    /// <summary>"Save as…": the smart playlist page.</summary>
    SaveAs,
    /// <summary>A genre; null all of them.</summary>
    Genre,
    /// <summary>A year's digits; null all of them.</summary>
    Year,
}

/// <summary>A row of a library's side panel (design spec §2.7): its label, the check on the choice in force, the value beside it, the chevron of a row that opens another list.</summary>
public sealed record LibraryPanelRow(LibraryRowKind Kind, string Label, bool Checked = false, string? Value = null, string? Detail = null, bool Opens = false);

/// <summary>A library's side panel: its title, its rows, and the row focus starts on (the choice in force).</summary>
public sealed record LibraryPanelView(string Title, IReadOnlyList<LibraryPanelRow> Rows, int Focus);

/// <summary>What choosing a panel row does to the panel.</summary>
public abstract record PanelOutcome
{
    /// <summary>The choice applied; the panel stays (which titles show).</summary>
    public sealed record Stay : PanelOutcome;

    /// <summary>The choice applied; the panel closes, and focus goes back to the chip it was opened from.</summary>
    public sealed record Close : PanelOutcome;

    /// <summary>The panel shows another list (Genre, Year).</summary>
    public sealed record Show(LibraryPanel Panel) : PanelOutcome;

    /// <summary>The panel closes and the page opens <paramref name="Key"/> ("Save as…").</summary>
    public sealed record Open(PageKey Key) : PanelOutcome;
}

/// <summary>What a message's one button does.</summary>
public enum MessageAction
{
    None,
    Retry,
    ClearFilters,
    /// <summary>Back to the page before (a person or playlist not found, an empty history).</summary>
    Back,
}

/// <summary>
/// A page or tab with nothing to show (design spec §8): the title, the sentence under it, and
/// the one thing to do as a pill with focus. Without one, focus goes to the page's tab (on the
/// tab the page opened on), else to the rail.
/// </summary>
public sealed record LibraryMessage(string Title, string Body, MessageAction Action = MessageAction.None)
{
    public string? ActionLabel => Action switch
    {
        MessageAction.Retry => LibraryText.Retry,
        MessageAction.ClearFilters => LibraryText.ClearFilters,
        MessageAction.Back => ItemText.Back,
        _ => null,
    };
}

/// <summary>A button of the Libraries page's header, or a row of a card's actions.</summary>
public enum LibrariesAction
{
    /// <summary>Starts sorting (<see cref="LibrariesModel.StartSorting"/>).</summary>
    Sort,
    /// <summary>Ends sorting (<see cref="LibrariesModel.Done"/>).</summary>
    Done,
    /// <summary>The order everyone sees (<see cref="LibrariesModel.ServerOrder"/>).</summary>
    ServerOrder,
    /// <summary>Opens the library.</summary>
    Open,
    /// <summary>Lifts the card to move it (<see cref="LibrariesModel.MoveFromActions"/>).</summary>
    Move,
}

/// <summary>A Libraries button or action: what it says, its icon by Lucide's name, and whether it can be pressed.</summary>
public sealed record LibrariesButton(LibrariesAction Action, string Label, string Icon, bool Enabled = true);

/// <summary>
/// What the TV's library pages and the Libraries page say (the Android app's
/// strings_tv_library.xml and the Libraries strings of strings_tv_home.xml), and the rules their
/// composables hold that are words rather than drawing: the chips, the count, the panels, the
/// empty states, the jump control's description, the Libraries page's header and cards.
/// </summary>
public static class LibraryText
{
    public const string TabRecommended = "Recommended";
    public const string TabLibrary = "Library";
    public const string TabCollections = "Collections";

    public const string SortTitle = "Title";
    public const string SortYear = "Year";
    public const string SortAdded = "Date added";
    public const string SortRating = "Rating";
    public const string SortUserRating = "Your rating";
    public const string SortRelease = "Release date";
    public const string SortDuration = "Duration";
    public const string SortLastPlayed = "Last played";
    public const string SortBitrate = "Bitrate";
    public const string SortBy = "Sort by";
    public const string Order = "Order";
    public const string Ascending = "Ascending";
    public const string Descending = "Descending";

    public const string Filter = "Filter";
    public const string Show = "Show";
    public const string ShowAll = "All";
    public const string Unwatched = "Unwatched";
    public const string Watched = "Watched";
    public const string Genre = "Genre";
    public const string AllGenres = "All genres";
    public const string Year = "Year";
    public const string AllYears = "All years";
    public const string ClearFilters = "Clear filters";
    public const string SaveAs = "Save as…";

    public const string Failed = "Couldn't load this library";
    public const string MissingTitle = "This library isn't here";
    public const string MissingBody = "It may have been removed, or this profile may not see it.";
    public const string EmptyTitle = "This library is empty";
    public const string EmptyBody = "Nothing here yet. Titles appear as the server adds them.";
    public const string NoMatchesTitle = "No matches";
    public const string NoMatchesBody = "Nothing in this library matches the current filters.";
    public const string RecommendedFailed = "Couldn't load recommendations";
    public const string RecommendedEmptyTitle = "Nothing to recommend yet";
    public const string RecommendedEmptyBody = "Recommendations appear once the library has some items.";
    public const string CollectionsFailed = "Couldn't load collections";
    public const string CollectionsEmptyTitle = "No collections yet";
    public const string CollectionsEmptyMovies = "Movies of the same TMDB collection are grouped here once two of them are in this library.";
    public const string CollectionsEmptyShows = "There are no collections in this library yet.";
    public const string Retry = "Retry";
    public const string SeeAll = "See all";

    public const string LibrariesTitle = "Libraries";
    public const string LibrariesFailed = "Couldn't load your libraries";
    public const string LibrariesScanning = "Scanning…";
    public const string SortLibraries = "Sort libraries";
    public const string SortDone = "Done";
    public const string ServerOrder = "Use the server's order";
    public const string OpenLibrary = "Open";
    public const string MoveLibrary = "Move";
    public const string SortingHint = "Select a library to move it. Your order applies on all your devices.";
    public const string MovingHint = "Arrows to move it, OK to drop it here, Back to put it back";
    public const string SortNeedsNewerServer = "Sorting libraries needs a newer Pley on the server.";
    public const string NoLibraries = "No libraries yet";
    public const string NoLibrariesBody = "Ask the server owner to add a library.";

    /// <summary>The jump control's description: "Jump to a letter: M".</summary>
    public static string Jump(string letter) => $"Jump to a letter: {letter}";

    public static string TabLabel(LibraryTab tab) => tab switch
    {
        LibraryTab.Recommended => TabRecommended,
        LibraryTab.Collections => TabCollections,
        _ => TabLibrary,
    };

    public static string SortLabel(string sort) => sort switch
    {
        LibrarySort.Year => SortYear,
        LibrarySort.Added => SortAdded,
        LibrarySort.Rating => SortRating,
        LibrarySort.UserRating => SortUserRating,
        LibrarySort.Release => SortRelease,
        LibrarySort.Duration => SortDuration,
        LibrarySort.LastPlayed => SortLastPlayed,
        LibrarySort.Bitrate => SortBitrate,
        _ => SortTitle,
    };

    /// <summary>The noun a library of <paramref name="kind"/> counts: shows, artists, else movies.</summary>
    static (string One, string Many) Noun(string? kind) => kind switch
    {
        "shows" => ("show", "shows"),
        "music" => ("artist", "artists"),
        _ => ("movie", "movies"),
    };

    /// <summary>"1,500 movies", "38 shows", "120 artists": a listing's count by its library's kind.</summary>
    public static string Count(string? kind, long n)
    {
        var (one, many) = Noun(kind);
        return Format.Pluralize(n, one, many);
    }

    /// <summary>"23 of 1,500 movies": <paramref name="n"/> of <paramref name="of"/>, the noun by <paramref name="of"/>.</summary>
    public static string CountOf(string? kind, long n, long of)
    {
        var (one, many) = Noun(kind);
        return $"{n.ToString("N0", CultureInfo.InvariantCulture)} of {of.ToString("N0", CultureInfo.InvariantCulture)} {(of == 1 ? one : many)}";
    }

    /// <summary>"3 collections".</summary>
    public static string CollectionsCount(long n) => Format.Pluralize(n, "collection");

    /// <summary>A collection's count on its tile: "12 shows" in a library of shows, else "12 movies".</summary>
    public static string CollectionCount(string? kind, long n) => Format.Pluralize(n, kind == "shows" ? "show" : "movie");

    /// <summary>"412 movies", "38 shows", "120 artists": what a library holds, by its kind (the Android app's libraryCount).</summary>
    public static string LibraryCount(Library library)
    {
        var (one, many) = library.Kind switch
        {
            "movies" => ("movie", "movies"),
            "shows" => ("show", "shows"),
            "music" => ("artist", "artists"),
            _ => ("item", "items"),
        };
        return Format.Pluralize(library.ItemCount, one, many);
    }

    /// <summary>A library's icon by kind, by Lucide's name, as the rail draws it.</summary>
    public static string KindIcon(string? kind) => kind switch
    {
        "movies" => "clapperboard",
        "shows" => "tv",
        "music" => "music",
        _ => "film",
    };

    /// <summary>"42 Mbps · 23.4 GB", "Bitrate unknown" when neither is known, "" without a summary.</summary>
    public static string MediaLine(MediaSummary? media)
    {
        if (media == null) return "";
        var line = ItemFormat.Join(" · ", ItemFormat.Bitrate(media.Bitrate), ItemFormat.Bytes(media.Size));
        return line.Length > 0 ? line : "Bitrate unknown";
    }

    /// <summary>A listing's tile sorted by bitrate: resolution, dynamic range, codec ("4K", "DV", "HEVC").</summary>
    public static IReadOnlyList<string> MediaBadges(MediaSummary? media)
    {
        if (media == null) return Array.Empty<string>();
        var range = media.DynamicRange switch
        {
            "dolby_vision" => "DV",
            "hdr10plus" => "HDR10+",
            "hdr10" => "HDR10",
            "hlg" => "HLG",
            _ => null,
        };
        var codec = ItemFormat.CodecLabel(media.VideoCodec);
        return new[] { ItemFormat.ResolutionLabel(media.Width, media.Height), range, codec.Length > 0 ? codec : null }
            .Where(b => b != null).Select(b => b!).ToList();
    }

    /// <summary>A collection's tile (the Android app's TvTiles): its poster, and "12 movies" under its title.</summary>
    public static TileModel CollectionTile(Collection collection, string? kind, string origin)
    {
        var count = CollectionCount(kind, collection.ItemCount);
        return new TileModel(
            collection.Id,
            collection.Title,
            count,
            string.IsNullOrEmpty(collection.PosterUrl) ? Array.Empty<string>() : new[] { Artwork.Absolute(collection.PosterUrl, origin) },
            0f,
            false,
            0,
            Array.Empty<string>(),
            $"{collection.Title}, {count}");
    }

    // ---- the Library tab

    /// <summary>
    /// The chips over the grid: Sort (the sort, its icon the order); then, but for music, Filter,
    /// Unwatched, and Genre and Year when the library has some or one is chosen.
    /// </summary>
    public static IReadOnlyList<LibraryChip> Chips(LibraryState state)
    {
        var p = state.Params;
        var chips = new List<LibraryChip>
        {
            new LibraryChip(LibraryChipKind.Sort, SortLabel(p.Sort), Icon: p.EffectiveOrder == SortOrder.Ascending ? "arrow-up-narrow-wide" : "arrow-down-wide-narrow"),
        };
        if (state.Kind == "music") return chips;
        chips.Add(new LibraryChip(LibraryChipKind.Filter, Filter, Icon: "sliders-horizontal"));
        chips.Add(new LibraryChip(LibraryChipKind.Unwatched, Unwatched, Selected: p.Watched == false));
        if (state.Filters.Genres.Count > 0 || p.Genre != null) chips.Add(new LibraryChip(LibraryChipKind.Genre, p.Genre ?? Genre, Selected: p.Genre != null));
        if (state.Filters.Years.Count > 0 || p.Year != null)
            chips.Add(new LibraryChip(LibraryChipKind.Year, p.Year?.ToString(CultureInfo.InvariantCulture) ?? Year, Selected: p.Year != null));
        return chips;
    }

    /// <summary>The panel a chip opens; null for Unwatched, which applies at once.</summary>
    public static LibraryPanel? PanelOf(LibraryChipKind chip) => chip switch
    {
        LibraryChipKind.Sort => LibraryPanel.Sort,
        LibraryChipKind.Filter => LibraryPanel.Filter,
        LibraryChipKind.Genre => LibraryPanel.Genre,
        LibraryChipKind.Year => LibraryPanel.Year,
        _ => null,
    };

    /// <summary>
    /// The count after the chips: "1,500 movies", or filtered "23 of 1,500 movies" (out of the
    /// library's own count); null while the listing isn't known.
    /// </summary>
    public static string? CountLine(LibraryState state)
    {
        var cells = state.Grid.Cells.Value;
        if (cells == null) return null;
        return state.Filtered && state.Library?.ItemCount is { } all && all >= cells.Total ? CountOf(state.Kind, cells.Total, all) : Count(state.Kind, cells.Total);
    }

    /// <summary>The panel <paramref name="panel"/>, the choice in force checked and focused.</summary>
    public static LibraryPanelView Panel(LibraryPanel panel, LibraryState state)
    {
        var p = state.Params;
        var rows = new List<LibraryPanelRow>();
        var focus = -1;
        void Add(LibraryPanelRow row, bool takesFocus = false)
        {
            if (takesFocus && focus < 0) focus = rows.Count;
            rows.Add(row);
        }
        string title;
        switch (panel)
        {
            case LibraryPanel.Sort:
                title = SortBy;
                foreach (var sort in state.Sorts)
                    Add(new LibraryPanelRow(LibraryRowKind.Sort, SortLabel(sort), sort == p.Sort, sort), sort == p.Sort);
                Add(new LibraryPanelRow(LibraryRowKind.Heading, Order));
                foreach (var order in new[] { SortOrder.Ascending, SortOrder.Descending })
                    Add(new LibraryPanelRow(LibraryRowKind.Order, order == SortOrder.Ascending ? Ascending : Descending, order == p.EffectiveOrder, order));
                break;
            case LibraryPanel.Filter:
                title = Filter;
                Add(new LibraryPanelRow(LibraryRowKind.Heading, Show));
                foreach (var (watched, label) in new (bool?, string)[] { (null, ShowAll), (false, Unwatched), (true, Watched) })
                    Add(new LibraryPanelRow(LibraryRowKind.Show, label, p.Watched == watched, watched?.ToString().ToLowerInvariant()), p.Watched == watched);
                if (state.Kind != "music")
                {
                    Add(new LibraryPanelRow(LibraryRowKind.Genres, Genre, Detail: p.Genre ?? AllGenres, Opens: true));
                    Add(new LibraryPanelRow(LibraryRowKind.Years, Year, Detail: p.Year?.ToString(CultureInfo.InvariantCulture) ?? AllYears, Opens: true));
                }
                if (p.Filtered)
                {
                    Add(new LibraryPanelRow(LibraryRowKind.ClearFilters, ClearFilters));
                    Add(new LibraryPanelRow(LibraryRowKind.SaveAs, SaveAs));
                }
                break;
            case LibraryPanel.Genre:
                title = Genre;
                Add(new LibraryPanelRow(LibraryRowKind.Genre, AllGenres, p.Genre == null), p.Genre == null);
                foreach (var genre in state.Filters.Genres)
                    Add(new LibraryPanelRow(LibraryRowKind.Genre, genre, genre == p.Genre, genre), genre == p.Genre);
                break;
            default:
                title = Year;
                Add(new LibraryPanelRow(LibraryRowKind.Year, AllYears, p.Year == null), p.Year == null);
                foreach (var year in state.Filters.Years)
                {
                    var digits = year.ToString(CultureInfo.InvariantCulture);
                    Add(new LibraryPanelRow(LibraryRowKind.Year, digits, year == p.Year, digits), year == p.Year);
                }
                break;
        }
        if (focus < 0) focus = rows.FindIndex(r => r.Kind != LibraryRowKind.Heading);
        return new LibraryPanelView(title, rows, Math.Max(0, focus));
    }

    // ---- what shows in place of content

    /// <summary>A failed load's title: the server out of reach says so, else <paramref name="title"/>.</summary>
    public static string FailedTitle(Exception error, string title) => Errors.Unreachable(error) ? Errors.OfflineTitle : title;

    /// <summary>Why a load failed, and what to check when the server answered nothing.</summary>
    public static string FailedBody(Exception error) => Errors.Unreachable(error) ? Errors.OfflineBody(error) : Errors.Message(error);

    /// <summary>The whole page's message, in place of the tabs: no such library, or it couldn't be read; null otherwise.</summary>
    public static LibraryMessage? PageMessage(LibraryState state)
    {
        if (state.Missing) return new LibraryMessage(MissingTitle, MissingBody);
        if (state.LibraryError is { } e) return new LibraryMessage(FailedTitle(e, Failed), FailedBody(e), MessageAction.Retry);
        return null;
    }

    /// <summary>
    /// The Library tab's message in place of the grid: the listing failed with nothing to show,
    /// nothing matches the filters ("Clear filters"), or the library is empty; null while the
    /// grid shows (skeletons included).
    /// </summary>
    public static LibraryMessage? GridMessage(LibraryState state)
    {
        var loadable = state.Grid.Cells;
        var cells = loadable.Value;
        if (cells == null && loadable is Loadable<GridCells>.Failed f) return new LibraryMessage(FailedTitle(f.Error, Failed), FailedBody(f.Error), MessageAction.Retry);
        if (cells is { Total: 0 })
            return state.Filtered ? new LibraryMessage(NoMatchesTitle, NoMatchesBody, MessageAction.ClearFilters) : new LibraryMessage(EmptyTitle, EmptyBody);
        return null;
    }

    /// <summary>The Recommended tab's message: its rows failed with nothing to show, or there are none; null while they show or load.</summary>
    public static LibraryMessage? RecommendedMessage(LibraryState state)
    {
        var rows = state.Recommended.Value;
        if (rows == null && state.Recommended is Loadable<IReadOnlyList<LibraryRow>>.Failed f)
            return new LibraryMessage(FailedTitle(f.Error, RecommendedFailed), FailedBody(f.Error), MessageAction.Retry);
        if (rows is { Count: 0 }) return new LibraryMessage(RecommendedEmptyTitle, RecommendedEmptyBody);
        return null;
    }

    /// <summary>The Collections tab's message: they failed with nothing to show, or there are none; null while they show or load.</summary>
    public static LibraryMessage? CollectionsMessage(LibraryState state)
    {
        var list = state.Collections.Value;
        if (list == null && state.Collections is Loadable<IReadOnlyList<CollectionTile>>.Failed f)
            return new LibraryMessage(FailedTitle(f.Error, CollectionsFailed), FailedBody(f.Error), MessageAction.Retry);
        if (list is { Count: 0 }) return new LibraryMessage(CollectionsEmptyTitle, state.Kind == "shows" ? CollectionsEmptyShows : CollectionsEmptyMovies);
        return null;
    }

    /// <summary>The Collections tab's count: "3 collections"; null while they aren't known.</summary>
    public static string? CollectionsCountLine(LibraryState state) => state.Collections.Value is { } list ? CollectionsCount(list.Count) : null;

    // ---- the Libraries page

    /// <summary>The Libraries page's message in place of the cards: they failed with nothing saved, or there are none; null otherwise.</summary>
    public static LibraryMessage? LibrariesMessage(LibrariesState state) => state switch
    {
        LibrariesState.Failed f => new LibraryMessage(FailedTitle(f.Error, LibrariesFailed), FailedBody(f.Error), MessageAction.Retry),
        LibrariesState.Ready { Libraries.Count: 0 } => new LibraryMessage(NoLibraries, NoLibrariesBody),
        _ => null,
    };

    /// <summary>The page's title: "Sort libraries" while sorting.</summary>
    public static string LibrariesHeading(bool sorting) => sorting ? SortLibraries : LibrariesTitle;

    /// <summary>
    /// The header's buttons: while sorting, the server's order and Done (not while a card is
    /// lifted); else Sort when the libraries can be sorted.
    /// </summary>
    public static IReadOnlyList<LibrariesButton> LibrariesButtons(bool sorting, bool moving, bool sortable)
    {
        if (sorting && sortable)
            return new[]
            {
                new LibrariesButton(LibrariesAction.ServerOrder, ServerOrder, "rotate-ccw", !moving),
                new LibrariesButton(LibrariesAction.Done, SortDone, "check", !moving),
            };
        return sortable ? new[] { new LibrariesButton(LibrariesAction.Sort, SortLibraries, "arrow-up-down") } : Array.Empty<LibrariesButton>();
    }

    /// <summary>What the remote does while sorting, at the bottom of the screen; null when not sorting.</summary>
    public static string? SortHint(bool sorting, bool moving) => !sorting ? null : moving ? MovingHint : SortingHint;

    /// <summary>A card's second line: "412 movies", with "· Scanning…" while the server scans it.</summary>
    public static string CardLine(LibraryEntry entry) => entry.Scanning ? $"{entry.Count} · {LibrariesScanning}" : entry.Count;

    /// <summary>A card's actions: Open, and while they can be sorted, Move and the server's order.</summary>
    public static IReadOnlyList<LibrariesButton> CardActions(bool sortable)
    {
        var actions = new List<LibrariesButton> { new LibrariesButton(LibrariesAction.Open, OpenLibrary, "arrow-right") };
        if (sortable)
        {
            actions.Add(new LibrariesButton(LibrariesAction.Move, MoveLibrary, "arrow-up-down"));
            actions.Add(new LibrariesButton(LibrariesAction.ServerOrder, ServerOrder, "rotate-ccw"));
        }
        return actions;
    }
}
