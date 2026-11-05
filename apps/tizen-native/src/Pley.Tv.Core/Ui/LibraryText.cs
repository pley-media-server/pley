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
    /// <summary>Opens the Resolution panel; says the resolution chosen, and shows only while one is.</summary>
    Resolution,
    /// <summary>Opens the Dynamic range panel; says the one chosen, and shows only while one is.</summary>
    DynamicRange,
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
    Resolution,
    DynamicRange,
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
    /// <summary>Opens the Resolution panel; <see cref="LibraryPanelRow.Detail"/> is the resolution chosen.</summary>
    Resolutions,
    /// <summary>Opens the Dynamic range panel; <see cref="LibraryPanelRow.Detail"/> is the dynamic range chosen.</summary>
    DynamicRanges,
    ClearFilters,
    /// <summary>"Save as…": the smart playlist page.</summary>
    SaveAs,
    /// <summary>A genre; null all of them.</summary>
    Genre,
    /// <summary>A year's digits; null all of them.</summary>
    Year,
    /// <summary>A resolution (<see cref="LibraryParams.Resolutions"/>); null all of them.</summary>
    Resolution,
    /// <summary>A dynamic range (<see cref="LibraryParams.DynamicRanges"/>); null all of them.</summary>
    DynamicRange,
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

    /// <summary>The panel shows another list (Genre, Year, Resolution, Dynamic range).</summary>
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
    public static string TabRecommended => Strings.T("library.tabs.recommended");
    public static string TabLibrary => Strings.T("library.tabs.library");
    public static string TabCollections => Strings.T("library.tabs.collections");

    public static string SortTitle => Strings.T("library.sorts.title");
    public static string SortYear => Strings.T("library.sorts.year");
    public static string SortAdded => Strings.T("library.sorts.added");
    public static string SortRating => Strings.T("library.sorts.rating");
    public static string SortUserRating => Strings.T("library.sorts.userRating");
    public static string SortRelease => Strings.T("library.sorts.release");
    public static string SortDuration => Strings.T("library.sorts.duration");
    public static string SortLastPlayed => Strings.T("library.sorts.lastPlayed");
    public static string SortBitrate => Strings.T("library.sorts.bitrate");
    public static string SortBy => Strings.T("library.panel.sortBy");
    public static string Order => Strings.T("library.panel.order");
    public static string Ascending => Strings.T("library.panel.ascending");
    public static string Descending => Strings.T("library.panel.descending");

    public static string Filter => Strings.T("library.panel.filter");
    public static string Show => Strings.T("library.panel.show");
    public static string ShowAll => Strings.T("library.panel.all");
    public static string Unwatched => Strings.T("library.panel.unwatched");
    public static string Watched => Strings.T("library.panel.watched");
    public static string Genre => Strings.T("library.panel.genre");
    public static string AllGenres => Strings.T("library.panel.allGenres");
    public static string Year => Strings.T("library.panel.year");
    public static string AllYears => Strings.T("library.panel.allYears");
    public static string Resolution => Strings.T("library.panel.resolution");
    public static string AllResolutions => Strings.T("library.panel.allResolutions");
    public static string DynamicRange => Strings.T("library.panel.dynamicRange");
    public static string AllDynamicRanges => Strings.T("library.panel.allDynamicRanges");
    public static string ClearFilters => Strings.T("library.panel.clearFilters");
    public static string SaveAs => Strings.T("library.panel.saveAs");

    // The value beside a Filter panel row while nothing is chosen. English repeats the row's "All
    // genres"; French and Spanish say "Tous" or "Todas" alone, to fit the value's 120 dp column.
    public static string NoGenreChosen => Strings.T("library.panel.unchosen.genre");
    public static string NoYearChosen => Strings.T("library.panel.unchosen.year");
    public static string NoResolutionChosen => Strings.T("library.panel.unchosen.resolution");
    public static string NoDynamicRangeChosen => Strings.T("library.panel.unchosen.dynamicRange");

    public static string Failed => Strings.T("library.messages.failed");
    public static string MissingTitle => Strings.T("library.messages.missingTitle");
    public static string MissingBody => Strings.T("library.messages.missingBody");
    public static string EmptyTitle => Strings.T("library.messages.emptyTitle");
    public static string EmptyBody => Strings.T("library.messages.emptyBody");
    public static string NoMatchesTitle => Strings.T("library.messages.noMatchesTitle");
    public static string NoMatchesBody => Strings.T("library.messages.noMatchesBody");
    public static string RecommendedFailed => Strings.T("library.messages.recommendedFailed");
    public static string RecommendedEmptyTitle => Strings.T("library.messages.recommendedEmptyTitle");
    public static string RecommendedEmptyBody => Strings.T("library.messages.recommendedEmptyBody");
    public static string CollectionsFailed => Strings.T("library.messages.collectionsFailed");
    public static string CollectionsEmptyTitle => Strings.T("library.messages.collectionsEmptyTitle");
    public static string CollectionsEmptyMovies => Strings.T("library.messages.collectionsEmptyMovies");
    public static string CollectionsEmptyShows => Strings.T("library.messages.collectionsEmptyShows");
    public static string Retry => Strings.T("library.retry");
    public static string SeeAll => Strings.T("library.seeAll");

    public static string LibrariesTitle => Strings.T("library.libraries.title");
    public static string LibrariesFailed => Strings.T("library.libraries.failed");
    public static string LibrariesScanning => Strings.T("library.libraries.scanning");
    public static string SortLibraries => Strings.T("library.libraries.sort");
    public static string SortDone => Strings.T("library.libraries.done");
    public static string ServerOrder => Strings.T("library.libraries.serverOrder");
    public static string OpenLibrary => Strings.T("library.libraries.open");
    public static string MoveLibrary => Strings.T("library.libraries.move");
    public static string SortingHint => Strings.T("library.libraries.sortingHint");
    public static string MovingHint => Strings.T("library.libraries.movingHint");
    public static string SortNeedsNewerServer => Strings.T("library.libraries.needsNewerServer");
    public static string NoLibraries => Strings.T("library.libraries.noneTitle");
    public static string NoLibrariesBody => Strings.T("library.libraries.noneBody");

    /// <summary>The jump control's description: "Jump to a letter: M".</summary>
    public static string Jump(string letter) => Strings.T("library.jump", ("letter", letter));

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

    /// <summary>A resolution's words ("4K", "1080p"); null for a value that isn't one of <see cref="LibraryParams.Resolutions"/>.</summary>
    public static string? ResolutionLabel(string? resolution) => resolution switch
    {
        "4k" => "4K",
        "1080" => "1080p",
        "720" => "720p",
        "sd" => "SD",
        _ => null,
    };

    /// <summary>
    /// A dynamic range's words ("Dolby Vision", "HDR+"); null for a value that isn't one of <see cref="LibraryParams.DynamicRanges"/>.
    /// HDR10 and HDR10+ read as plain "HDR" and "HDR+", as on the posters; a title's page names them exactly.
    /// </summary>
    public static string? DynamicRangeLabel(string? range) => range switch
    {
        "hdr" => Strings.T("library.anyHdr"),
        "dolby_vision" => "Dolby Vision",
        "hdr10plus" => "HDR+",
        "hdr10" => "HDR",
        "hlg" => "HLG",
        "sdr" => "SDR",
        _ => null,
    };

    /// <summary>"1,500 movies", "38 shows", "120 artists": a listing's count by its library's kind (shows, artists, else movies).</summary>
    public static string Count(string? kind, long n) => kind switch
    {
        "shows" => Strings.Count("library.count.shows", n),
        "music" => Strings.Count("library.count.artists", n),
        _ => Strings.Count("library.count.movies", n),
    };

    /// <summary>"23 of 1,500 movies": <paramref name="n"/> of <paramref name="of"/>, the noun by <paramref name="of"/>.</summary>
    public static string CountOf(string? kind, long n, long of) => kind switch
    {
        "shows" => Strings.Count("library.countOf.shows", of, ("shown", n)),
        "music" => Strings.Count("library.countOf.artists", of, ("shown", n)),
        _ => Strings.Count("library.countOf.movies", of, ("shown", n)),
    };

    /// <summary>"3 collections".</summary>
    public static string CollectionsCount(long n) => Strings.Count("library.count.collections", n);

    /// <summary>A collection's count on its tile: "12 shows" in a library of shows, else "12 movies".</summary>
    public static string CollectionCount(string? kind, long n) =>
        kind == "shows" ? Strings.Count("library.count.shows", n) : Strings.Count("library.count.movies", n);

    /// <summary>"412 movies", "38 shows", "120 artists": what a library holds, by its kind (the Android app's libraryCount).</summary>
    public static string LibraryCount(Library library) => library.Kind switch
    {
        "movies" => Strings.Count("library.count.movies", library.ItemCount),
        "shows" => Strings.Count("library.count.shows", library.ItemCount),
        "music" => Strings.Count("library.count.artists", library.ItemCount),
        _ => Strings.Count("library.count.items", library.ItemCount),
    };

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
        return line.Length > 0 ? line : Strings.T("library.bitrateUnknown");
    }

    /// <summary>A grid's tile sorted by bitrate or filtered by quality: 4K and the dynamic range ("4K", "DV"), no other resolution and no codec.</summary>
    public static IReadOnlyList<string> MediaBadges(MediaSummary? media)
    {
        if (media == null) return Array.Empty<string>();
        var range = media.DynamicRange switch
        {
            "dolby_vision" => "DV",
            "hdr10plus" => "HDR+",
            "hdr10" => "HDR",
            "hlg" => "HLG",
            _ => null,
        };
        var resolution = ItemFormat.ResolutionLabel(media.Width, media.Height) == "4K" ? "4K" : null;
        return new[] { resolution, range }
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
    /// Unwatched, and Genre and Year when the library has some or one is chosen. The row has no room
    /// for two more: a resolution and a dynamic range are chosen in the Filter panel, and have a chip
    /// only while chosen, saying it ("4K", "Dolby Vision").
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
        if (ResolutionLabel(p.Resolution) is { } resolution) chips.Add(new LibraryChip(LibraryChipKind.Resolution, resolution, Selected: true));
        if (DynamicRangeLabel(p.DynamicRange) is { } range) chips.Add(new LibraryChip(LibraryChipKind.DynamicRange, range, Selected: true));
        return chips;
    }

    /// <summary>The panel a chip opens; null for Unwatched, which applies at once.</summary>
    public static LibraryPanel? PanelOf(LibraryChipKind chip) => chip switch
    {
        LibraryChipKind.Sort => LibraryPanel.Sort,
        LibraryChipKind.Filter => LibraryPanel.Filter,
        LibraryChipKind.Genre => LibraryPanel.Genre,
        LibraryChipKind.Year => LibraryPanel.Year,
        LibraryChipKind.Resolution => LibraryPanel.Resolution,
        LibraryChipKind.DynamicRange => LibraryPanel.DynamicRange,
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
                    Add(new LibraryPanelRow(LibraryRowKind.Genres, Genre, Detail: p.Genre ?? NoGenreChosen, Opens: true));
                    Add(new LibraryPanelRow(LibraryRowKind.Years, Year, Detail: p.Year?.ToString(CultureInfo.InvariantCulture) ?? NoYearChosen, Opens: true));
                    // When the library has two or more to choose from, or one is chosen, to change or clear it.
                    if (state.Filters.Resolutions.Count > 0 || p.Resolution != null)
                        Add(new LibraryPanelRow(LibraryRowKind.Resolutions, Resolution, Detail: ResolutionLabel(p.Resolution) ?? NoResolutionChosen, Opens: true));
                    if (state.Filters.DynamicRanges.Count > 0 || p.DynamicRange != null)
                        Add(new LibraryPanelRow(LibraryRowKind.DynamicRanges, DynamicRange, Detail: DynamicRangeLabel(p.DynamicRange) ?? NoDynamicRangeChosen, Opens: true));
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
            case LibraryPanel.Resolution:
                title = Resolution;
                Add(new LibraryPanelRow(LibraryRowKind.Resolution, AllResolutions, p.Resolution == null), p.Resolution == null);
                foreach (var resolution in state.Filters.Resolutions)
                    Add(new LibraryPanelRow(LibraryRowKind.Resolution, ResolutionLabel(resolution) ?? resolution, resolution == p.Resolution, resolution), resolution == p.Resolution);
                break;
            case LibraryPanel.DynamicRange:
                title = DynamicRange;
                Add(new LibraryPanelRow(LibraryRowKind.DynamicRange, AllDynamicRanges, p.DynamicRange == null), p.DynamicRange == null);
                foreach (var range in state.Filters.DynamicRanges)
                    Add(new LibraryPanelRow(LibraryRowKind.DynamicRange, DynamicRangeLabel(range) ?? range, range == p.DynamicRange, range), range == p.DynamicRange);
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
    public static string CardLine(LibraryEntry entry) => entry.Scanning ? string.Join(" · ", entry.Count, LibrariesScanning) : entry.Count;

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
