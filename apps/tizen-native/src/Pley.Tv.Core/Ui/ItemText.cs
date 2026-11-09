using System;
using System.Collections.Generic;
using System.Linq;
using Pley.Tv.Core.Api;

namespace Pley.Tv.Core.Ui;

/// <summary>A button of the item page's hero, in the order they stand.</summary>
public enum HeroButtonKind
{
    /// <summary>Resume, Play, "Play S1 · E3", or Unavailable (not pressable).</summary>
    Primary,
    FromStart,
    Watched,
    /// <summary>Audio and subtitles: the tracks panel.</summary>
    Tracks,
    MyList,
    /// <summary>Plays the first trailer, from its start.</summary>
    Trailer,
    AddToPlaylist,
    /// <summary>The More panel.</summary>
    More,
}

/// <summary>A hero button: what it says (round buttons show it on focus) and its icon, by Lucide's name.</summary>
public sealed record HeroButton(HeroButtonKind Kind, string Label, string? Icon, bool Enabled = true);

/// <summary>A row of a side panel: a heading, or a choice that does something.</summary>
public enum PanelRowKind
{
    Heading,
    /// <summary><see cref="PanelRow.Value"/>: the version's file id (<see cref="ItemModel.ChooseVersion"/>).</summary>
    Version,
    /// <summary><see cref="PanelRow.Value"/>: the track's index; null: Automatic (<see cref="ItemModel.ChooseAudio"/>).</summary>
    Audio,
    /// <summary><see cref="PanelRow.Value"/>: the track's index; null: Off (<see cref="ItemModel.ChooseSubtitle"/>).</summary>
    Subtitle,
    /// <summary>The tagline and summary in a dialog (<see cref="ItemText.FullSummaryText"/>).</summary>
    FullSummary,
    /// <summary><see cref="PanelRow.Value"/>: 2 to 10 (one to five stars); null: no rating (<see cref="ItemModel.Rate"/>).</summary>
    Rating,
    /// <summary>Goes to <see cref="PanelRow.Key"/>: the show, the season, a collection, adding to a playlist.</summary>
    Open,
    /// <summary>A tile's action (<see cref="PanelRow.Action"/>, <see cref="ItemModel.Perform"/>).</summary>
    Action,
}

/// <summary>A row of a side panel (design spec §2.7): its label, whether it is ticked, and what it does.</summary>
public sealed record PanelRow(PanelRowKind Kind, string Label, bool Checked = false, long? Value = null, string? Icon = null, PageKey? Key = null, ItemAction? Action = null);

/// <summary>A side panel: its title, its rows, and the row focus starts on (the current choice).</summary>
public sealed record SidePanel(string Title, IReadOnlyList<PanelRow> Rows, int Focus);

/// <summary>"Directed by Lena Okafor · Starring Idris Vale, Maren Holt": the screen draws the labels muted.</summary>
/// <param name="Made">"Directed by" or "Created by"; null without either.</param>
/// <param name="Makers">Up to two names.</param>
/// <param name="Starring">Up to three names; empty without a cast.</param>
public sealed record CreditsLine(string? Made, string Makers, string Starring)
{
    public static string StarringLabel => ItemText.Starring;
}

/// <summary>A person in the Cast &amp; Crew row: their part or their job, and their picture's address.</summary>
public sealed record PersonTile(long Id, string Name, string Subtitle, string? Photo);

/// <summary>
/// What the TV's item page and its panels say (the Android app's strings_tv_item.xml), and the
/// rules its composables hold that are words rather than drawing: the hero's buttons, the credits
/// line, the panels' rows, the details' facts.
/// </summary>
public static class ItemText
{
    public static string Play => Strings.T("item.hero.play");
    public static string Resume => Strings.T("item.hero.resume");
    public static string Unavailable => Strings.T("item.hero.unavailable");
    public static string FromStart => Strings.T("item.hero.fromStart");
    public static string MarkWatched => Strings.T("item.hero.markWatched");
    public static string MarkUnwatched => Strings.T("item.hero.markUnwatched");
    public static string Tracks => Strings.T("item.hero.tracks");
    public static string AddToPlaylist => Strings.T("item.hero.addToPlaylist");
    public static string AddToMyList => Strings.T("item.hero.addToMyList");
    public static string InMyList => Strings.T("item.hero.inMyList");
    public static string Trailer => Strings.T("item.hero.trailer");
    public static string More => Strings.T("item.hero.more");
    public static string FullSummary => Strings.T("item.panels.fullSummary");
    /// <summary>The credits line's "Directed by" (with a colon where the language puts one).</summary>
    public static string DirectedBy => Strings.T("item.credits.directedBy");
    public static string CreatedBy => Strings.T("item.credits.createdBy");
    public static string Starring => Strings.T("item.credits.starring");
    public static string Extras => Strings.T("item.rows.extras");
    public static string MoreLikeThis => Strings.T("item.rows.moreLikeThis");
    public static string Episodes => Strings.T("item.rows.episodes");
    public static string NoEpisodes => Strings.T("item.rows.noEpisodes");
    public static string Details => Strings.T("item.details.title");
    public static string Versions => Strings.T("item.details.versions");
    public static string MediaInfo => Strings.T("item.details.mediaInfo");
    public static string MediaInfoHint => Strings.T("item.details.mediaInfoHint");
    /// <summary>Media info's state, for screen readers.</summary>
    public static string MediaInfoOpen => Strings.T("item.details.mediaInfoOpen");
    public static string MediaInfoClosed => Strings.T("item.details.mediaInfoClosed");
    public static string Close => Strings.T("item.states.close");
    public static string Back => Strings.T("item.states.back");
    public static string Retry => Strings.T("item.states.retry");
    public static string Version => Strings.T("item.panels.version");
    public static string Audio => Strings.T("item.panels.audio");
    public static string Subtitles => Strings.T("item.panels.subtitles");
    public static string SubtitlesOff => Strings.T("item.panels.subtitlesOff");
    public static string YourRating => Strings.T("item.panels.yourRating");
    public static string NoRating => Strings.T("item.panels.noRating");
    public static string GoToShow => Strings.T("item.panels.goToShow");
    public static string GoToSeason => Strings.T("item.panels.goToSeason");
    public static string FailedTitle => Strings.T("item.states.failed");
    public static string NotFound => Strings.T("item.states.notFound");
    public static string NotFoundBody => Strings.T("item.states.notFoundBody");
    public static string NothingToPlay => Strings.T("item.states.nothingToPlay");

    /// <summary>Stars the rating offers; each is two points of the server's 0 to 10.</summary>
    public const int Stars = 5;

    /// <summary>Names after "Starring".</summary>
    const int StarringNames = 3;

    // ---- the page's states

    /// <summary>A failed page's title: the server answered nothing, or this item didn't load.</summary>
    public static string FailedTitleOf(Exception e) => Errors.Unreachable(e) ? Errors.OfflineTitle : FailedTitle;

    /// <summary>A failed page's text: why, and what to check when the server answered nothing.</summary>
    public static string FailedBody(Exception e) => Errors.Unreachable(e) ? Errors.OfflineBody(e) : Errors.Message(e);

    /// <summary>A toast's text, and whether it is an error's.</summary>
    public static (string Text, bool Error) Toast(ItemEvent e) => e switch
    {
        ItemEvent.Failed f => (Errors.Message(f.Error), true),
        _ => (NothingToPlay, false),
    };

    // ---- the hero

    /// <summary>The hero's title: an episode's or season's show, else the item's own.</summary>
    public static string HeroTitle(ItemPage page) => page.Kind is "episode" or "season" ? page.Context?.ShowTitle ?? page.Title : page.Title;

    /// <summary>An episode's "S2 · E5 · Glasshouse"; a season's own title under its show's; null for the others.</summary>
    public static string? ContextLine(ItemPage page) => page.Kind switch
    {
        "episode" => ItemFormat.Join(" · ", page.Context?.EpisodeLabel, page.Title),
        "season" => page.Context?.ShowTitle != null ? page.Title : null,
        _ => null,
    };

    /// <summary>How far a resumed title was played, "1h 33m left", as on Home; null unless resuming.</summary>
    public static string? TimeLeft(ItemPage page) =>
        page.Primary.Kind == PrimaryKind.Resume && page.Primary.TimeLeft.Length > 0 ? page.Primary.TimeLeft : null;

    /// <summary>"Drama · Thriller · Part of The Saltwind Collection"; empty without either.</summary>
    public static string GenresLine(ItemPage page) => string.Join(" · ", page.Genres.Concat(page.PartOf.Select(c => PartOf(c.Title))));

    /// <summary>"Part of The Saltwind Collection".</summary>
    public static string PartOf(string collection) => Strings.T("item.credits.partOf", ("collection", collection));

    /// <summary>"Directed by Lena Okafor · Starring Idris Vale, Maren Holt"; null with neither.</summary>
    public static CreditsLine? Credits(ItemPage page)
    {
        var makers = page.Details.Crew.FirstOrDefault(l => l.Role == "director") ?? page.Details.Crew.FirstOrDefault(l => l.Role == "creator");
        var stars = page.Cast.Take(StarringNames).ToList();
        if (makers == null && stars.Count == 0) return null;
        return new CreditsLine(
            makers == null ? null : makers.Role == "director" ? DirectedBy : CreatedBy,
            makers == null ? "" : string.Join(", ", makers.People.Take(2).Select(p => p.Name)),
            string.Join(", ", stars.Select(p => p.Name)));
    }

    /// <summary>"Resume", "Resume S2 · E5", "Play S1 · E3", "Play", or "Unavailable".</summary>
    public static string PrimaryLabel(PrimaryAction primary) => primary switch
    {
        { Kind: PrimaryKind.Unavailable } => Unavailable,
        { Kind: PrimaryKind.Resume, Episode: { } episode } => Strings.T("item.hero.resumeEpisode", ("episode", episode)),
        { Kind: PrimaryKind.Resume } => Resume,
        { Episode: { } episode } => Strings.T("item.hero.playEpisode", ("episode", episode)),
        _ => Play,
    };

    /// <summary>
    /// The hero's buttons, in order: Resume or Play, From start, Watched, Audio and subtitles, My
    /// List, Trailer, Add to playlist, More; each only where it applies.
    /// </summary>
    public static IReadOnlyList<HeroButton> Buttons(ItemPage page)
    {
        var unavailable = page.Primary.Kind == PrimaryKind.Unavailable;
        var buttons = new List<HeroButton> { new HeroButton(HeroButtonKind.Primary, PrimaryLabel(page.Primary), unavailable ? null : "play", !unavailable) };
        if (page.FromStart) buttons.Add(new HeroButton(HeroButtonKind.FromStart, FromStart, "rotate-ccw"));
        buttons.Add(page.Watched
            ? new HeroButton(HeroButtonKind.Watched, MarkUnwatched, "eye-off")
            : new HeroButton(HeroButtonKind.Watched, MarkWatched, "circle-check"));
        if (page.Tracks != null) buttons.Add(new HeroButton(HeroButtonKind.Tracks, Tracks, "captions"));
        if (page.MyList is { } inList)
            buttons.Add(inList ? new HeroButton(HeroButtonKind.MyList, InMyList, "bookmark-check") : new HeroButton(HeroButtonKind.MyList, AddToMyList, "bookmark-plus"));
        if (page.Trailer != null) buttons.Add(new HeroButton(HeroButtonKind.Trailer, Trailer, "film"));
        buttons.Add(new HeroButton(HeroButtonKind.AddToPlaylist, AddToPlaylist, "list-plus"));
        buttons.Add(new HeroButton(HeroButtonKind.More, More, "ellipsis"));
        return buttons;
    }

    /// <summary>The button focus starts on: Resume or Play, or Watched when there is nothing to play.</summary>
    public static HeroButtonKind FirstFocus(ItemPage page) => page.Primary.Kind == PrimaryKind.Unavailable ? HeroButtonKind.Watched : HeroButtonKind.Primary;

    /// <summary>The More panel's summary dialog: the tagline and the summary.</summary>
    public static string FullSummaryText(ItemPage page) => string.Join("\n\n", new[] { page.Tagline, page.Summary }.Where(s => s != null));

    // ---- the episodes and the rows

    /// <summary>Season tabs show when the show has more than one season and no season posters; else the season's name heads the row.</summary>
    public static bool ShowsTabs(SeasonsSection seasons) => seasons.Seasons.Count > 1 && !seasons.Posters;

    /// <summary>The episode row's heading without tabs: the season's, else the page's own season, else "Episodes".</summary>
    public static string EpisodesHeading(ItemPage page, SeasonsSection seasons) => seasons.SelectedTab?.Title ?? page.Context?.SeasonTitle ?? Episodes;

    /// <summary>"Aired March 12, 2024".</summary>
    public static string Aired(string date) => Strings.T("item.rows.aired", ("date", date));

    /// <summary>The info block's line for a focused episode: "Season 2 · Episode 5 · 58 min · Aired March 12, 2024".</summary>
    public static string EpisodeInfo(EpisodeLine episode) =>
        ItemFormat.Join(" · ", episode.LongLabel, episode.Runtime, episode.Aired == null ? null : Aired(episode.Aired));

    /// <summary>The cast row's heading: "Cast", "Guest Stars", "Crew", "Cast &amp; Crew" or "Guest Stars &amp; Crew".</summary>
    public static string CastRowTitle(ItemPage page)
    {
        var episode = page.Kind == "episode";
        if (page.Details.Crew.Count == 0) return episode ? Strings.T("item.rows.guestStars") : Strings.T("item.rows.cast");
        if (page.Cast.Count == 0) return Strings.T("item.rows.crew");
        return episode ? Strings.T("item.rows.guestStarsAndCrew") : Strings.T("item.rows.castAndCrew");
    }

    /// <summary>Whether the page has a Cast &amp; Crew row.</summary>
    public static bool HasCastRow(ItemPage page) => page.Cast.Count > 0 || page.Details.Crew.Count > 0;

    /// <summary>
    /// The tiles of Cast &amp; Crew: the cast with their parts, then who made it with their role
    /// ("Director"), each person once, so that their names lead to their pages as on the web.
    /// </summary>
    public static IReadOnlyList<PersonTile> CastAndCrew(ItemPage page)
    {
        var seen = new HashSet<long>();
        var tiles = new List<PersonTile>();
        string? Photo(Credit c) => string.IsNullOrEmpty(c.PhotoUrl) ? null : Artwork.Absolute(c.PhotoUrl!, page.Origin);
        foreach (var c in page.Cast)
            if (seen.Add(c.Id)) tiles.Add(new PersonTile(c.Id, c.Name, c.Character ?? "", Photo(c)));
        foreach (var line in page.Details.Crew)
            foreach (var c in line.People)
                if (seen.Add(c.Id)) tiles.Add(new PersonTile(c.Id, c.Name, CrewRole(line.Role), Photo(c)));
        return tiles;
    }

    /// <summary>A maker's job under their picture: "Director", "Writer", "Creator".</summary>
    static string CrewRole(string role) => role switch
    {
        "director" => Strings.T("item.roles.director"),
        "writer" => Strings.T("item.roles.writer"),
        "creator" => Strings.T("item.roles.creator"),
        _ => "",
    };

    /// <summary>The extras' tiles, captioned "Trailer · 2m".</summary>
    public static IReadOnlyList<TileModel> ExtraTiles(ItemPage page) =>
        page.Extras.Select(e => TileModel.Of(e, TileKind.Landscape, page.Origin) with { Subtitle = ItemPages.ExtraSubtitle(e) }).ToList();

    // ---- details

    /// <summary>Whether the page has a Details section: facts, versions or media info.</summary>
    public static bool HasDetails(ItemPage page) => !page.Details.IsEmpty || page.MediaInfo.Count > 0;

    /// <summary>The details' facts, label and value: who made it ("Directors: A, B and 2 more"), original title, release, studio or network, plays.</summary>
    public static IReadOnlyList<(string Label, string Value)> Facts(ItemDetails details)
    {
        var facts = new List<(string, string)>();
        foreach (var line in details.Crew)
        {
            // "Directors" for two or more, as CrewLine.Plural says.
            var people = line.People.Count + line.More;
            var label = line.Role switch
            {
                "creator" => Strings.T("item.details.createdBy"),
                "director" => Strings.Count("item.details.directors", people),
                _ => Strings.Count("item.details.writers", people),
            };
            var more = line.More > 0 ? " " + AndMore(line.More) : "";
            facts.Add((label, string.Join(", ", line.People.Select(p => p.Name)) + more));
        }
        if (details.OriginalTitle != null) facts.Add((Strings.T("item.details.originalTitle"), details.OriginalTitle));
        if (details.Released != null) facts.Add((Strings.T("item.details.released"), details.Released));
        if (details.Studio != null) facts.Add((details.Network ? Strings.T("item.details.network") : Strings.T("item.details.studio"), details.Studio));
        if (details.Plays != null) facts.Add((Strings.T("item.details.plays"), details.Plays.Value.ToString(System.Globalization.CultureInfo.InvariantCulture)));
        return facts;
    }

    /// <summary>"and 2 more".</summary>
    public static string AndMore(int n) => Strings.Count("item.details.andMore", n);

    // ---- the panels

    /// <summary>"Automatic (English)", or "Automatic" when the tracks don't say their language.</summary>
    public static string AutomaticAudio(TrackChoices tracks) =>
        tracks.AutomaticLanguage != null ? Strings.T("item.panels.automaticLanguage", ("language", tracks.AutomaticLanguage)) : Strings.T("item.panels.automatic");

    /// <summary>"1 star", "3 stars".</summary>
    public static string StarsLabel(int stars) => Strings.Count("item.panels.stars", stars);

    /// <summary>
    /// Audio and subtitles: Version, Audio (Automatic first) and Subtitles (Off first), each choice
    /// ticked; focus starts on the first ticked. Choosing keeps the panel open. Null when there is
    /// nothing to choose.
    /// </summary>
    public static SidePanel? TracksPanel(ItemPage page)
    {
        var tracks = page.Tracks;
        if (tracks == null) return null;
        var rows = new List<PanelRow>();
        if (tracks.Versions.Count > 0)
        {
            rows.Add(new PanelRow(PanelRowKind.Heading, Version));
            rows.AddRange(tracks.Versions.Select(v => new PanelRow(PanelRowKind.Version, v.Label, v.Selected, v.Id)));
        }
        if (tracks.Audio.Count > 0)
        {
            rows.Add(new PanelRow(PanelRowKind.Heading, Audio));
            // Automatic: no pick, each device plays its best track of the language.
            rows.Add(new PanelRow(PanelRowKind.Audio, AutomaticAudio(tracks), tracks.AutomaticAudio));
            rows.AddRange(tracks.Audio.Select(a => new PanelRow(PanelRowKind.Audio, a.Label, a.Selected, a.Id)));
        }
        if (tracks.Subtitles.Count > 0)
        {
            rows.Add(new PanelRow(PanelRowKind.Heading, Subtitles));
            rows.Add(new PanelRow(PanelRowKind.Subtitle, SubtitlesOff, tracks.SubtitleIndex == null));
            rows.AddRange(tracks.Subtitles.Select(s => new PanelRow(PanelRowKind.Subtitle, s.Label, s.Selected, s.Id)));
        }
        return new SidePanel(Tracks, rows, FirstFocusable(rows, r => r.Checked));
    }

    /// <summary>
    /// More: the whole summary when the hero cut it (<paramref name="summaryCut"/>), the profile's
    /// rating, then going to the show or season, and the collections it is part of. Focus starts
    /// on the rating chosen; without a rating to choose, on the first row after it; Add to
    /// playlist is offered here only when nothing else is.
    /// </summary>
    public static SidePanel MorePanel(ItemPage page, bool summaryCut)
    {
        var rows = new List<PanelRow>();
        int? focus = null;
        void Add(PanelRow row, bool takesFocus)
        {
            if (takesFocus && focus == null) focus = rows.Count;
            rows.Add(row);
        }
        if (summaryCut) rows.Add(new PanelRow(PanelRowKind.FullSummary, FullSummary, Icon: "info"));
        if (page.Rateable)
        {
            rows.Add(new PanelRow(PanelRowKind.Heading, YourRating));
            var current = page.UserRating;
            Add(new PanelRow(PanelRowKind.Rating, NoRating, current == null), current == null);
            for (var stars = 1; stars <= Stars; stars++)
            {
                var value = stars * 2;
                // A half star rounds up to the star it is on.
                var ticked = current == value || current == value - 1;
                Add(new PanelRow(PanelRowKind.Rating, StarsLabel(stars), ticked, value, "star"), ticked);
            }
        }
        if (page.Context?.ShowId is { } show)
            Add(new PanelRow(PanelRowKind.Open, GoToShow, Icon: "tv", Key: new PageKey.Item(show)), true);
        if (page.Kind == "episode" && page.Context?.SeasonId is { } season)
            Add(new PanelRow(PanelRowKind.Open, GoToSeason, Icon: "layers", Key: new PageKey.Item(season)), true);
        foreach (var collection in page.PartOf)
            Add(new PanelRow(PanelRowKind.Open, PartOf(collection.Title), Icon: "layers", Key: new PageKey.Collection(collection.Id)), true);
        if (focus == null) Add(new PanelRow(PanelRowKind.Open, AddToPlaylist, Icon: "list-plus", Key: new AddToPlaylistPage(page.Id)), true);
        return new SidePanel(More, rows, focus ?? 0);
    }

    /// <summary>A tile's actions (a long press of Select): what its actions screen offers, focus on the first.</summary>
    public static SidePanel ActionsPanel(ItemSummary item) =>
        new SidePanel(item.Title, ItemActions.Of(item).Select(a => new PanelRow(PanelRowKind.Action, ItemActions.Label(a), Icon: ItemActions.Icon(a), Action: a)).ToList(), 0);

    static int FirstFocusable(IReadOnlyList<PanelRow> rows, Func<PanelRow, bool> wanted)
    {
        for (var i = 0; i < rows.Count; i++)
            if (rows[i].Kind != PanelRowKind.Heading && wanted(rows[i])) return i;
        for (var i = 0; i < rows.Count; i++)
            if (rows[i].Kind != PanelRowKind.Heading) return i;
        return 0;
    }
}
