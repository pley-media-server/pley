using System;
using System.Collections.Generic;
using System.Globalization;
using System.Linq;
using Pley.Tv.Core.Api;

namespace Pley.Tv.Core.Ui;

// Pages an item leads to that the back stack doesn't have yet: music, and adding to a playlist.
// They are PageKeys so the navigator handles them as it handles the others.

/// <summary>An artist's music page.</summary>
public sealed record ArtistPage(long ArtistId) : PageKey;

/// <summary>An album's music page (a track opens its album's, <paramref name="TrackId"/> picked out).</summary>
public sealed record AlbumPage(long AlbumId, long? TrackId = null) : PageKey;

/// <summary>Choosing a playlist to add <paramref name="ItemId"/> to.</summary>
public sealed record AddToPlaylistPage(long ItemId) : PageKey;

/// <summary>New playlist, an empty one: its title and what it plays.</summary>
public sealed record NewPlaylistPage : PageKey;

/// <summary>
/// What an item page shows: a movie, show, season or episode, as apps/web/src/pages/item shows
/// them (the Android app's ItemUiState). The screen only lays it out and puts its words on it.
/// </summary>
public abstract record ItemState
{
    /// <summary>Nothing known yet: a skeleton.</summary>
    public sealed record Loading : ItemState;

    /// <summary>No such item, or one this profile may not see (404): "Item not found".</summary>
    public sealed record NotFound : ItemState;

    /// <summary>It didn't load and nothing was saved: "Couldn't load this item", the error's sentence, and Retry.</summary>
    public sealed record Failed(Exception Error) : ItemState;

    /// <summary>
    /// The item has another page: artists, albums and tracks have the music pages, and an extra
    /// shows its movie's or show's. The page swaps itself for <paramref name="Key"/>.
    /// </summary>
    public sealed record Elsewhere(PageKey Key) : ItemState;

    /// <summary>The page; <paramref name="Error"/>: the server couldn't be asked again, and what shows is what was saved.</summary>
    public sealed record Ready(ItemPage Page, Exception? Error = null) : ItemState;
}

/// <summary>An item's page, every label made already.</summary>
public sealed record ItemPage
{
    public long Id { get; init; }
    /// <summary>"movie", "show", "season" or "episode".</summary>
    public string Kind { get; init; } = "";
    /// <summary>The server's origin: pictures are made absolute against it.</summary>
    public string Origin { get; init; } = "";
    /// <summary>The item as listings carry it: its pictures, and what its actions act on.</summary>
    public ItemSummary Item { get; init; } = new ItemSummary();
    /// <summary>The item's own title; a season's is "Season 2" or "Specials".</summary>
    public string Title { get; init; } = "";
    /// <summary>Episodes: their show and season. Seasons: their show.</summary>
    public ItemContext? Context { get; init; }
    public IReadOnlyList<MetaPart> Meta { get; init; } = Array.Empty<MetaPart>();
    public IReadOnlyList<string> Genres { get; init; } = Array.Empty<string>();
    /// <summary>The collections it is "Part of".</summary>
    public IReadOnlyList<CollectionRef> PartOf { get; init; } = Array.Empty<CollectionRef>();
    public PrimaryAction Primary { get; init; } = new PrimaryAction(PrimaryKind.Play);
    /// <summary>"From start" plays from the beginning what Play would resume.</summary>
    public bool FromStart { get; init; }
    /// <summary>Watched: movies and episodes, or every episode of a show or season.</summary>
    public bool Watched { get; init; }
    /// <summary>In My List; null where My List doesn't take the item (seasons, episodes).</summary>
    public bool? MyList { get; init; }
    /// <summary>The profile's own rating, 1 to 10 (a half star each); null: unrated.</summary>
    public int? UserRating { get; init; }
    /// <summary>Movies, shows and episodes take the profile's rating.</summary>
    public bool Rateable { get; init; }
    public string? Tagline { get; init; }
    public string? Summary { get; init; }
    /// <summary>The version, audio and subtitle choices; null when there is nothing to choose.</summary>
    public TrackChoices? Tracks { get; init; }
    /// <summary>The first trailer among the extras.</summary>
    public ItemSummary? Trailer { get; init; }
    public ItemDetails Details { get; init; } = ItemDetails.Empty;
    /// <summary>The files, as "Media info" lists them.</summary>
    public IReadOnlyList<MediaFileInfo> MediaInfo { get; init; } = Array.Empty<MediaFileInfo>();
    public IReadOnlyList<ItemSummary> Extras { get; init; } = Array.Empty<ItemSummary>();
    /// <summary>"Cast", or "Guest Stars" for an episode's.</summary>
    public CastTitle CastTitle { get; init; }
    /// <summary>Each person once, in billing order.</summary>
    public IReadOnlyList<Credit> Cast { get; init; } = Array.Empty<Credit>();
    /// <summary>More Like This: movies and shows only.</summary>
    public IReadOnlyList<ItemSummary> Related { get; init; } = Array.Empty<ItemSummary>();
    /// <summary>Shows, seasons and episodes: the seasons and the episodes of one.</summary>
    public SeasonsSection? Seasons { get; init; }
}

/// <summary>Where an episode or season sits: the TV's context line.</summary>
/// <param name="SeasonId">An episode's season.</param>
/// <param name="SeasonTitle">"Season 2", "Specials".</param>
/// <param name="EpisodeLabel">An episode's "S2 · E5".</param>
/// <param name="EpisodeLongLabel">An episode's "Season 2 · Episode 5".</param>
public sealed record ItemContext(long? ShowId, string? ShowTitle, long? SeasonId, string? SeasonTitle, string? EpisodeLabel, string? EpisodeLongLabel);

/// <summary>One piece of the metadata line, in order; the screen puts "·" between them.</summary>
public abstract record MetaPart
{
    /// <summary>A year, an air date, "3 seasons", "2021-2025", an edition, a runtime.</summary>
    public sealed record Text(string Value) : MetaPart;

    /// <summary>"PG-13", boxed.</summary>
    public sealed record ContentRating(string Value) : MetaPart;

    /// <summary>The critics' rating, "8.1", after a star.</summary>
    public sealed record Rating(string Value) : MetaPart;

    /// <summary>"4K", "DV P8.1", "Atmos": the short quality badges (design spec §2.6).</summary>
    public sealed record Badges(IReadOnlyList<string> Values) : MetaPart
    {
        public bool Equals(Badges? other) => other != null && Values.SequenceEqual(other.Values);

        public override int GetHashCode() => Values.Count;
    }
}

/// <summary>What the main button does.</summary>
public enum PrimaryKind
{
    Play,
    Resume,
    /// <summary>Nothing to play: no media files, or no episodes.</summary>
    Unavailable,
}

/// <summary>The main button.</summary>
/// <param name="Episode">Shows and seasons: the episode it plays, "S1 · E3".</param>
/// <param name="Progress">How far the item (or that episode) was played, 0 to 1.</param>
/// <param name="TimeLeft">"1h 33m left", "26 min left"; empty unless resuming.</param>
public sealed record PrimaryAction(PrimaryKind Kind, string? Episode = null, float Progress = 0f, string TimeLeft = "");

/// <summary>One choice in a select: a version (its file id) or a track (its stream index).</summary>
public sealed record TrackChoice(long Id, string Label, bool Selected);

/// <summary>
/// The web's version, audio and subtitle selects. A list with nothing to choose is empty: one
/// version, one audio track, no subtitles.
/// </summary>
/// <param name="Audio">Audio tracks, when there are two or more; none is selected while the audio is automatic.</param>
/// <param name="Subtitles">Subtitle tracks; "Off" comes before them on screen, chosen when none of them is.</param>
/// <param name="VersionId">The chosen version's id.</param>
/// <param name="AutomaticAudio">
/// No audio track is picked: each device plays its best track of <paramref name="AutomaticLanguage"/>
/// (the server picks it at play time), so "Automatic (English)" comes first among the audio
/// choices, ticked.
/// </param>
/// <param name="AutomaticLanguage">The language automatic audio plays, "English"; null when the tracks don't say.</param>
/// <param name="AudioIndex">The picked audio track's index; null while the audio is automatic.</param>
/// <param name="SubtitleIndex">The chosen subtitle track's index; null: off.</param>
public sealed record TrackChoices(
    IReadOnlyList<TrackChoice> Versions,
    IReadOnlyList<TrackChoice> Audio,
    IReadOnlyList<TrackChoice> Subtitles,
    long? VersionId,
    bool AutomaticAudio,
    string? AutomaticLanguage,
    int? AudioIndex,
    int? SubtitleIndex)
{
    public string? AudioLabel => Audio.FirstOrDefault(c => c.Selected)?.Label;
    public string? SubtitleLabel => Subtitles.FirstOrDefault(c => c.Selected)?.Label;
    public string? VersionLabel => Versions.FirstOrDefault(c => c.Selected)?.Label;
}

/// <summary>The Details list: who made it, and facts.</summary>
/// <param name="Crew">Created by, directed by, written by: up to four names each, "and N more".</param>
/// <param name="OriginalTitle">When it differs from the title.</param>
/// <param name="Released">"Apr 1, 2023"; not for episodes, whose date is in the metadata line.</param>
/// <param name="Network">Episodes and shows: <paramref name="Studio"/> is the network.</param>
/// <param name="Plays">Times played, when any.</param>
/// <param name="Versions">Each version in the web's long form, "4K · HEVC · Dolby Vision P8.1 · TrueHD Atmos 7.1 · 62.0 GB", the one that plays checked.</param>
public sealed record ItemDetails(
    IReadOnlyList<CrewLine> Crew,
    string? OriginalTitle,
    string? Released,
    string? Studio,
    bool Network,
    long? Plays,
    IReadOnlyList<TrackChoice> Versions)
{
    public static readonly ItemDetails Empty = new ItemDetails(Array.Empty<CrewLine>(), null, null, null, false, null, Array.Empty<TrackChoice>());

    public bool IsEmpty => Crew.Count == 0 && OriginalTitle == null && Released == null && Studio == null && Plays == null && Versions.Count == 0;
}

/// <summary>"Directors: A, B, C, D and 2 more": <paramref name="Role"/> ("creator", "director", "writer"), the people (four at most) and how many more.</summary>
public sealed record CrewLine(string Role, IReadOnlyList<Credit> People, int More)
{
    /// <summary>Two or more people: the plural label.</summary>
    public bool Plural => People.Count + More > 1;
}

/// <summary>The heading of the cast row.</summary>
public enum CastTitle
{
    Cast,
    GuestStars,
}

/// <summary>A file in Media info: its rows and its streams.</summary>
/// <param name="Title">Its edition, or "Version 2" when there are several; null for the only one.</param>
/// <param name="File">The file as the server describes it, for media info in full (<see cref="MediaFacts"/>).</param>
public sealed record MediaFileInfo(long Id, string? Title, IReadOnlyList<InfoRow> Rows, IReadOnlyList<StreamInfo> Streams, MediaFile? File = null);

/// <summary>A fact about a file.</summary>
public sealed record InfoRow(InfoLabel Label, string Value);

public enum InfoLabel
{
    File,
    Container,
    Size,
    Duration,
    Bitrate,
    Video,
    Audio,
}

/// <summary>A stream of a file, as the web's table has it.</summary>
/// <param name="Kind">"video", "audio" or "subtitle".</param>
/// <param name="Language">"English", or null.</param>
/// <param name="Details">"4K · 3840×2160 · 23.98 fps · 10-bit · Dolby Vision P8.1 · Main 10".</param>
public sealed record StreamInfo(string Kind, string Codec, string? Language, string? Title, string Details);

/// <summary>A show's seasons and one season's episodes: the season tabs and the episode row.</summary>
/// <param name="Seasons">The tabs: the show's seasons. Empty on an episode's page, which shows its own season only.</param>
/// <param name="Selected">The season shown.</param>
/// <param name="Episodes">Its episodes; null while they load.</param>
/// <param name="Focus">The episode to start from: the one Play plays, or the page's own episode.</param>
public sealed record SeasonsSection(IReadOnlyList<SeasonTab> Seasons, long? Selected, IReadOnlyList<EpisodeLine>? Episodes, long? Focus)
{
    /// <summary>The season shown, as a tab.</summary>
    public SeasonTab? SelectedTab => Seasons.FirstOrDefault(s => s.Id == Selected);
}

/// <summary>A season among a show's: "Season 2", "Specials", with "8 episodes".</summary>
public sealed record SeasonTab(long Id, string Title, string? Episodes);

/// <summary>An episode in a row, its words worked out.</summary>
/// <param name="Title">"5. Glasshouse".</param>
/// <param name="Length">"58 min" or "26 min left".</param>
/// <param name="Line">"58 min · Mar 5, 2024", "26 min left · Mar 7, 2024".</param>
/// <param name="Aired">"March 12, 2024", for the TV's "Aired …".</param>
/// <param name="LongLabel">"Season 2 · Episode 5".</param>
/// <param name="Runtime">Its runtime alone, "58 min".</param>
public sealed record EpisodeLine(
    ItemSummary Item,
    string Title,
    string Length,
    string Line,
    string? Aired,
    string LongLabel,
    string Runtime,
    string? Summary,
    float Progress,
    bool Watched);

/// <summary>
/// The version and tracks picked on the page (the web's VideoItemView state). A pick made for
/// another version doesn't apply: choosing a version goes back to the server's choice of tracks for it.
/// </summary>
/// <param name="FileId">The version picked; null: the first.</param>
/// <param name="AudioFile">The version <paramref name="Audio"/> was picked for; null: none was.</param>
/// <param name="Audio">The track picked; null with <paramref name="AudioFile"/>: Automatic was.</param>
/// <param name="SubtitleFile">The version <paramref name="Subtitle"/> was picked for; null: none was (<paramref name="Subtitle"/> null with a file: Off).</param>
public sealed record StreamPick(long? FileId = null, long? AudioFile = null, int? Audio = null, long? SubtitleFile = null, int? Subtitle = null);

/// <summary>What an item page is made from: the item and what was loaded beside it.</summary>
public sealed record PageInput
{
    public PageInput(ItemDetail detail, string origin)
    {
        Detail = detail;
        Origin = origin;
    }

    public ItemDetail Detail { get; init; }
    public string Origin { get; init; }
    public StreamPick Pick { get; init; } = new StreamPick();
    /// <summary>Shows and seasons: the episode Play plays; null while not known, or with none.</summary>
    public ItemSummary? OnDeck { get; init; }
    /// <summary>The show's seasons (a show's own, a season's siblings); null while they load.</summary>
    public IReadOnlyList<ItemSummary>? Seasons { get; init; }
    public long? SelectedSeason { get; init; }
    /// <summary>The selected season's episodes; null while they load.</summary>
    public IReadOnlyList<ItemSummary>? Episodes { get; init; }
    public IReadOnlyList<ItemSummary> Related { get; init; } = Array.Empty<ItemSummary>();
    public IReadOnlyList<CollectionRef> PartOf { get; init; } = Array.Empty<CollectionRef>();
    /// <summary>Dates' order and month names; the device's when null.</summary>
    public CultureInfo? Culture { get; init; }
}

/// <summary>
/// The file the page plays, and the tracks it starts with. <paramref name="PickedAudio"/> is the
/// audio picked on the page for that file, the only one the player and the saved choice are told
/// of: otherwise the server picks the track each device plays best, and <paramref name="AudioIndex"/>
/// is the details' choice, made for no device in particular. <paramref name="AutomaticAudio"/>: no
/// track is picked, on the page or before, so the page names only its language.
/// </summary>
internal sealed record Playing(MediaFile? File, int? AudioIndex, int? SubtitleIndex, MediaStream? DefaultAudio, int? PickedAudio, bool AutomaticAudio);

/// <summary>An item page's rules (the Android app's ItemPages.kt).</summary>
public static class ItemPages
{
    /// <summary>At most this many names per crew line, then "and N more" (Credits.tsx).</summary>
    public const int CrewNames = 4;

    static readonly string[] CrewRoles = { "creator", "director", "writer" };

    static readonly string[] StreamKinds = { "video", "audio", "subtitle" };

    /// <summary>Movies, shows, seasons and episodes, which have this page.</summary>
    public static bool HasPage(string kind) => kind is "movie" or "show" or "season" or "episode";

    /// <summary>Movies and shows, which My List takes.</summary>
    public static bool Listable(string kind) => kind is "movie" or "show";

    /// <summary>The version and tracks <paramref name="pick"/> comes to on <paramref name="detail"/>, as the web works them out.</summary>
    internal static Playing PlayingOf(ItemDetail detail, StreamPick pick)
    {
        var file = detail.Files.FirstOrDefault(f => f.Id == pick.FileId) ?? detail.Files.FirstOrDefault();
        var audio = file?.Streams.Where(s => s.Kind == "audio").ToList() ?? new List<MediaStream>();
        var defaultAudio = audio.FirstOrDefault(s => s.Default) ?? audio.FirstOrDefault();
        var pickedHere = file != null && pick.AudioFile == file.Id;
        var pickedAudio = pickedHere ? pick.Audio : null;
        var automatic = pickedHere ? pick.Audio == null : file?.AudioPicked != true;
        var audioIndex = pickedAudio ?? file?.SelectedAudio;
        var subtitleIndex = file != null && pick.SubtitleFile == file.Id ? pick.Subtitle : file?.SelectedSubtitle;
        return new Playing(file, audioIndex, subtitleIndex, defaultAudio, pickedAudio, automatic);
    }

    /// <summary>The page of <paramref name="input"/>'s item.</summary>
    public static ItemPage Page(PageInput input)
    {
        var d = input.Detail;
        var playing = PlayingOf(d, input.Pick);
        var onDeck = input.OnDeck;
        var kind = d.Kind;
        var video = Format.IsPlayableVideo(kind);
        var container = kind is "show" or "season";
        var resumable = video && !d.UserState.Watched && d.UserState.PositionMs > 0;
        PrimaryAction primary;
        if (video && d.Files.Count == 0) primary = new PrimaryAction(PrimaryKind.Unavailable);
        else if (video && resumable)
            primary = new PrimaryAction(PrimaryKind.Resume, Progress: Format.ProgressFraction(d.UserState.PositionMs, d.DurationMs),
                TimeLeft: Format.TimeLeft(d.UserState.PositionMs, d.DurationMs));
        else if (video) primary = new PrimaryAction(PrimaryKind.Play);
        else primary = ContainerPrimary(d, onDeck, kind == "season" && input.SelectedSeason == d.Id ? input.Episodes : null);
        bool watched;
        if (container)
        {
            var leaves = d.LeafCount ?? 0;
            watched = leaves > 0 && (d.ViewedLeafCount ?? 0) >= leaves;
        }
        else watched = d.UserState.Watched;
        var rating = d.UserState.Rating is { } r ? (int?)Math.Max(0, Math.Min(10, (int)Math.Floor(r + 0.5))) : null;
        return new ItemPage
        {
            Id = d.Id,
            Kind = kind,
            Origin = input.Origin,
            Item = d,
            Title = kind == "season" ? ItemFormat.SeasonTitle(d.Title, d.Index) : d.Title,
            Context = ContextOf(d),
            Meta = MetaOf(d, playing.File, input.Seasons, input.Culture),
            Genres = d.Genres,
            PartOf = input.PartOf,
            Primary = primary,
            FromStart = primary.Kind == PrimaryKind.Resume,
            Watched = watched,
            MyList = Listable(kind) ? d.InWatchlist ?? false : null,
            UserRating = rating > 0 ? rating : null,
            Rateable = kind is "movie" or "show" or "episode",
            Tagline = string.IsNullOrWhiteSpace(d.Tagline) ? null : d.Tagline,
            Summary = string.IsNullOrWhiteSpace(d.Summary) ? null : d.Summary,
            Tracks = TrackChoicesOf(d, playing),
            Trailer = d.Extras?.FirstOrDefault(e => e.ExtraType == "trailer"),
            Details = DetailsOf(d, playing.File, input.Culture),
            MediaInfo = MediaInfoOf(d.Files),
            Extras = Distinct(d.Extras ?? Array.Empty<ItemSummary>()),
            CastTitle = kind == "episode" ? CastTitle.GuestStars : CastTitle.Cast,
            Cast = (d.Cast ?? Array.Empty<Credit>()).GroupBy(c => c.Id).Select(g => g.First()).ToList(),
            Related = kind is "movie" or "show" ? Distinct(input.Related).Where(i => i.Id != d.Id).ToList() : Array.Empty<ItemSummary>(),
            Seasons = SeasonsOf(input, onDeck),
        };
    }

    static IReadOnlyList<ItemSummary> Distinct(IEnumerable<ItemSummary> items) => items.GroupBy(i => i.Id).Select(g => g.First()).ToList();

    /// <summary>A show's or season's button: its next episode, "Resume S2 · E5" or "Play S1 · E1".</summary>
    static PrimaryAction ContainerPrimary(ItemDetail d, ItemSummary? onDeck, IReadOnlyList<ItemSummary>? ownEpisodes)
    {
        if (onDeck == null)
        {
            var nothing = d.Kind == "season"
                ? ownEpisodes is { Count: 0 } || d.LeafCount == 0
                : d.LeafCount == null || d.LeafCount == 0;
            return new PrimaryAction(nothing ? PrimaryKind.Unavailable : PrimaryKind.Play);
        }
        var resuming = onDeck.UserState.PositionMs > 0 && !onDeck.UserState.Watched;
        var episode = Format.EpisodeLabel(onDeck);
        return new PrimaryAction(
            resuming ? PrimaryKind.Resume : PrimaryKind.Play,
            episode.Length > 0 ? episode : null,
            resuming ? Format.ProgressFraction(onDeck) : 0f,
            resuming ? Format.TimeLeft(onDeck.UserState.PositionMs, onDeck.DurationMs) : "");
    }

    static string? NonEmpty(string? s) => string.IsNullOrEmpty(s) ? null : s;

    static ItemContext? ContextOf(ItemDetail d) => d.Kind switch
    {
        "episode" => new ItemContext(
            d.GrandparentId,
            NonEmpty(d.GrandparentTitle),
            d.ParentId,
            d.ParentIndex != null ? ItemFormat.SeasonTitle(null, d.ParentIndex) : NonEmpty(d.ParentTitle),
            NonEmpty(Format.EpisodeLabel(d.ParentIndex, d.Index)),
            NonEmpty(Format.EpisodeLongLabel(d.ParentIndex, d.Index))),
        "season" => new ItemContext(d.ParentId, NonEmpty(d.ParentTitle), null, null, null, null),
        _ => null,
    };

    /// <summary>The metadata line (design spec §2.2, §4.3, §4.4).</summary>
    static IReadOnlyList<MetaPart> MetaOf(ItemDetail d, MediaFile? file, IReadOnlyList<ItemSummary>? seasons, CultureInfo? culture)
    {
        var parts = new List<MetaPart>();
        void Text(string? value)
        {
            if (!string.IsNullOrWhiteSpace(value)) parts.Add(new MetaPart.Text(value!));
        }
        switch (d.Kind)
        {
            case "show":
                var count = d.ChildCount ?? seasons?.Count(s => (s.Index ?? 1) > 0);
                if (count > 0) Text(Strings.Count("item.counts.seasons", count.Value));
                Text(YearRange(d.Year, seasons));
                break;
            case "season":
                Text(d.Year?.ToString(CultureInfo.InvariantCulture));
                if (d.LeafCount > 0) Text(Strings.Count("item.counts.episodes", d.LeafCount.Value));
                break;
            case "episode":
                Text(ItemFormat.Date(d.ReleaseDate, culture));
                Text(file?.Edition);
                Text(Format.Runtime(d.DurationMs));
                break;
            default:
                Text(d.Year?.ToString(CultureInfo.InvariantCulture));
                Text(file?.Edition);
                Text(Format.Runtime(d.DurationMs));
                break;
        }
        if (!string.IsNullOrWhiteSpace(d.ContentRating)) parts.Add(new MetaPart.ContentRating(d.ContentRating!));
        if (d.Rating > 0) parts.Add(new MetaPart.Rating(ItemFormat.Quantity(d.Rating.Value.ToString("0.0", CultureInfo.InvariantCulture))));
        var badges = ItemFormat.QualityBadges(file);
        if (badges.Count > 0) parts.Add(new MetaPart.Badges(badges));
        return parts;
    }

    /// <summary>"2021-2025" from a show's seasons, or its own year.</summary>
    static string? YearRange(int? year, IReadOnlyList<ItemSummary>? seasons)
    {
        var years = (seasons ?? Array.Empty<ItemSummary>()).Where(s => s.Year != null).Select(s => s.Year!.Value).ToList();
        if (year != null) years.Add(year.Value);
        if (years.Count == 0) return null;
        var first = years.Min();
        var last = years.Max();
        return last > first ? $"{first}-{last}" : first.ToString(CultureInfo.InvariantCulture);
    }

    /// <summary>The selects (VideoItemView.tsx): versions when there are several, audio when two or more, subtitles when any.</summary>
    static TrackChoices? TrackChoicesOf(ItemDetail d, Playing playing)
    {
        var file = playing.File;
        if (file == null) return null;
        var audio = file.Streams.Where(s => s.Kind == "audio").ToList();
        var subtitles = file.Streams.Where(s => s.Kind == "subtitle").ToList();
        if (d.Files.Count < 2 && audio.Count < 2 && subtitles.Count == 0) return null;
        var audioStart = playing.AudioIndex ?? playing.DefaultAudio?.Index;
        // Without a pick no track is named: each device plays its best of this one's language.
        var audioShown = playing.AutomaticAudio ? null : audioStart;
        return new TrackChoices(
            d.Files.Count > 1 ? d.Files.Select((f, i) => new TrackChoice(f.Id, ItemFormat.VersionLabel(f, i), f.Id == file.Id)).ToList() : Array.Empty<TrackChoice>(),
            audio.Count > 1 ? audio.Select(s => new TrackChoice(s.Index, ItemFormat.StreamLabel(s), s.Index == audioShown)).ToList() : Array.Empty<TrackChoice>(),
            subtitles.Select(s => new TrackChoice(s.Index, ItemFormat.StreamLabel(s), s.Index == playing.SubtitleIndex)).ToList(),
            file.Id,
            playing.AutomaticAudio,
            ItemFormat.LanguageName(audio.FirstOrDefault(s => s.Index == audioStart)?.Language),
            audioShown,
            subtitles.Any(s => s.Index == playing.SubtitleIndex) ? playing.SubtitleIndex : null);
    }

    static ItemDetails DetailsOf(ItemDetail d, MediaFile? file, CultureInfo? culture) => new ItemDetails(
        CrewRoles.Select(role =>
        {
            var people = (d.Crew ?? Array.Empty<Credit>()).Where(c => c.Role == role).GroupBy(c => c.Id).Select(g => g.First()).ToList();
            return people.Count == 0 ? null : new CrewLine(role, people.Take(CrewNames).ToList(), Math.Max(0, people.Count - CrewNames));
        }).Where(l => l != null).Select(l => l!).ToList(),
        !string.IsNullOrWhiteSpace(d.OriginalTitle) && d.OriginalTitle != d.Title ? d.OriginalTitle : null,
        d.Kind != "episode" && !string.IsNullOrWhiteSpace(d.ReleaseDate) ? ItemFormat.Date(d.ReleaseDate, culture) : null,
        string.IsNullOrWhiteSpace(d.Studio) ? null : d.Studio,
        d.Kind is "episode" or "show",
        d.UserState.PlayCount > 0 ? d.UserState.PlayCount : null,
        d.Files.Select(f => new TrackChoice(f.Id, ItemFormat.VersionDescription(f), f.Id == file?.Id)).ToList());

    /// <summary>Media info (MediaInfo.tsx). An app's token gets file names only; whatever comes, only the name shows.</summary>
    static IReadOnlyList<MediaFileInfo> MediaInfoOf(IReadOnlyList<MediaFile> files) => files.Select((f, i) =>
    {
        var video = f.Streams.FirstOrDefault(s => s.Kind == "video");
        var rows = new List<InfoRow>
        {
            new InfoRow(InfoLabel.File, FileName(f.Path)),
            new InfoRow(InfoLabel.Container, f.Container?.ToUpperInvariant() ?? "—"),
        };
        if (f.Size > 0) rows.Add(new InfoRow(InfoLabel.Size, ItemFormat.Bytes(f.Size)));
        if (f.DurationMs > 0) rows.Add(new InfoRow(InfoLabel.Duration, Format.Duration(f.DurationMs)));
        if (f.Bitrate > 0) rows.Add(new InfoRow(InfoLabel.Bitrate, ItemFormat.Bitrate(f.Bitrate)));
        if (!string.IsNullOrEmpty(f.VideoCodec))
            rows.Add(new InfoRow(InfoLabel.Video, ItemFormat.Join(" · ", ItemFormat.CodecLabel(f.VideoCodec), ItemFormat.ResolutionLabel(f.Width, f.Height), ItemFormat.DynamicRangeLabel(video))));
        if (!string.IsNullOrEmpty(f.AudioCodec)) rows.Add(new InfoRow(InfoLabel.Audio, ItemFormat.CodecLabel(f.AudioCodec)));
        var streams = f.Streams
            .Where(s => Array.IndexOf(StreamKinds, s.Kind) >= 0)
            .OrderBy(s => Array.IndexOf(StreamKinds, s.Kind)).ThenBy(s => s.Index)
            .Select(s => new StreamInfo(s.Kind, ItemFormat.CodecLabel(s.Codec), ItemFormat.LanguageName(s.Language), string.IsNullOrWhiteSpace(s.Title) ? null : s.Title, StreamDetails(s)))
            .ToList();
        var title = !string.IsNullOrWhiteSpace(f.Edition) ? f.Edition : files.Count > 1 ? ItemFormat.VersionNumber(i + 1) : null;
        return new MediaFileInfo(f.Id, title, rows, streams, f);
    }).ToList();

    /// <summary>The last part of a path, whichever separator it uses.</summary>
    public static string FileName(string path)
    {
        var name = path.Substring(path.LastIndexOf('/') + 1);
        return name.Substring(name.LastIndexOf('\\') + 1);
    }

    static string StreamDetails(MediaStream s)
    {
        var parts = new List<string>();
        switch (s.Kind)
        {
            case "video":
                if (ItemFormat.ResolutionLabel(s.Width, s.Height) is { } resolution) parts.Add(resolution);
                if (s.Width > 0 && s.Height > 0) parts.Add($"{s.Width}×{s.Height}");
                if (s.FrameRate > 0) parts.Add(ItemFormat.FramesPerSecond(ItemFormat.Quantity(Trimmed(Math.Floor(s.FrameRate.Value * 100 + 0.5) / 100.0))));
                if (s.BitDepth > 0) parts.Add(ItemFormat.BitDepth(s.BitDepth.Value));
                if (ItemFormat.DynamicRangeLabel(s) is { } range) parts.Add(range);
                if (!string.IsNullOrWhiteSpace(s.Profile)) parts.Add(s.Profile!);
                break;
            case "audio":
                var channels = ItemFormat.ChannelsLabel(s.Channels, s.ChannelLayout);
                if (channels.Length > 0) parts.Add(channels);
                if (s.SampleRate > 0) parts.Add(ItemFormat.Kilohertz(ItemFormat.Quantity(ItemFormat.Fixed(s.SampleRate.Value / 1000.0, 1))));
                break;
            default:
                parts.Add(s.TextBased == false ? Strings.T("item.mediaInfo.imageBased") : Strings.T("item.mediaInfo.textBased"));
                if (s.External == true) parts.Add(Strings.T("item.format.external"));
                break;
        }
        if (s.Bitrate > 0) parts.Add(ItemFormat.Bitrate(s.Bitrate));
        if (s.Forced) parts.Add(Strings.T("item.format.forced"));
        if (s.Default) parts.Add(Strings.T("item.format.default"));
        return string.Join(" · ", parts);
    }

    /// <summary>23.98 → "23.98", 24.0 → "24".</summary>
    static string Trimmed(double value) =>
        value == Math.Floor(value) ? ((long)value).ToString(CultureInfo.InvariantCulture) : value.ToString("R", CultureInfo.InvariantCulture);

    static SeasonsSection? SeasonsOf(PageInput input, ItemSummary? onDeck)
    {
        var d = input.Detail;
        IReadOnlyList<SeasonTab> tabs;
        switch (d.Kind)
        {
            case "show":
            case "season":
                tabs = (input.Seasons ?? Array.Empty<ItemSummary>())
                    .Select(s => new SeasonTab(s.Id, ItemFormat.SeasonTitle(s.Title, s.Index), s.LeafCount is { } n ? Strings.Count("item.counts.episodes", n) : null))
                    .ToList();
                break;
            case "episode":
                tabs = Array.Empty<SeasonTab>();
                break;
            default:
                return null;
        }
        var focus = d.Kind == "episode" ? d.Id : onDeck != null && onDeck.ParentId == input.SelectedSeason ? onDeck.Id : (long?)null;
        return new SeasonsSection(
            tabs,
            input.SelectedSeason,
            input.Episodes == null ? null : Distinct(input.Episodes).Select(e => EpisodeLine(e, input.Culture)).ToList(),
            focus);
    }

    /// <summary>An episode's words for rows.</summary>
    public static EpisodeLine EpisodeLine(ItemSummary e, CultureInfo? culture = null)
    {
        var inProgress = Format.IsInProgress(e);
        var runtime = Format.Runtime(e.DurationMs);
        var length = inProgress ? Format.TimeLeft(e.UserState.PositionMs, e.DurationMs) : runtime;
        var aired = ItemFormat.LongDate(e.ReleaseDate, culture);
        return new EpisodeLine(
            e,
            e.Index != null ? $"{e.Index}. {e.Title}" : e.Title,
            length,
            ItemFormat.Join(" · ", length, ItemFormat.Date(e.ReleaseDate, culture)),
            aired.Length > 0 ? aired : null,
            Format.EpisodeLongLabel(e.ParentIndex, e.Index),
            runtime,
            string.IsNullOrWhiteSpace(e.Summary) ? null : e.Summary,
            inProgress ? Format.ProgressFraction(e) : 0f,
            e.UserState.Watched);
    }

    /// <summary>The season a show page opens on: the next episode's, else the first that isn't Specials, else the first.</summary>
    public static long? DefaultSeason(IReadOnlyList<ItemSummary> seasons, ItemSummary? onDeck)
    {
        if (onDeck?.ParentId is { } parent && seasons.Any(s => s.Id == parent)) return parent;
        return seasons.FirstOrDefault(s => (s.Index ?? 1) > 0)?.Id ?? seasons.FirstOrDefault()?.Id;
    }

    /// <summary>What kind of extra it is: "Trailer", "Behind the Scenes"…, else "Extra".</summary>
    static string ExtraLabel(string? type) => type switch
    {
        "trailer" => Strings.T("item.extras.trailer"),
        "behind_the_scenes" => Strings.T("item.extras.behindTheScenes"),
        "deleted_scene" => Strings.T("item.extras.deletedScene"),
        "featurette" => Strings.T("item.extras.featurette"),
        "interview" => Strings.T("item.extras.interview"),
        "scene" => Strings.T("item.extras.scene"),
        "short" => Strings.T("item.extras.short"),
        _ => Strings.T("item.extras.other"),
    };

    /// <summary>An extra's caption line, as the web's Extras row: "Trailer · 2m".</summary>
    public static string ExtraSubtitle(ItemSummary item) => ItemFormat.Join(" · ", ExtraLabel(item.ExtraType), Format.Duration(item.DurationMs));
}
