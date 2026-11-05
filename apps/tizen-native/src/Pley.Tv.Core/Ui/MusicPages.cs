using System;
using System.Collections.Generic;
using System.Globalization;
using System.Linq;
using System.Threading.Tasks;
using Pley.Tv.Core.Api;
using Pley.Tv.Core.Player;

namespace Pley.Tv.Core.Ui;

/// <summary>
/// An artist's page (the web's MusicViews.tsx ArtistView; the Android app's ArtistPage): the round
/// picture, "4 albums · 40 tracks", Play and Shuffle, and the albums, newest first.
/// </summary>
/// <param name="Origin">The server's address, which the pictures' paths are on.</param>
/// <param name="Counts">"4 albums · 40 tracks"; empty when the server doesn't say.</param>
/// <param name="Albums">Newest first, then by title; empty while they load (<paramref name="AlbumsLoading"/>).</param>
/// <param name="Item">The artist as a listing has it.</param>
public sealed record ArtistView(long Id, string Title, string Origin, string? PosterUrl, string? BackdropUrl, string? Summary, IReadOnlyList<string> Genres,
    string Counts, IReadOnlyList<ItemSummary> Albums, bool AlbumsLoading, ItemSummary Item);

/// <summary>
/// An album's page (MusicViews.tsx AlbumView; the Android app's AlbumPage): the art, the artist
/// (a link to their page when <paramref name="ArtistId"/> is known), the year, "10 tracks · 42m",
/// the profile's rating, Play and Shuffle, and the tracks with disc headers when there are several discs.
/// </summary>
/// <param name="UserRating">The profile's rating, 1 to 10 (a half star each); null: unrated.</param>
/// <param name="Tracks">The tracks in order, which Play plays.</param>
/// <param name="Rows">What the list shows: disc headers, then their tracks.</param>
public sealed record AlbumView(long Id, string Title, string Origin, string? PosterUrl, string? Artist, long? ArtistId, int? Year, string Meta, int? UserRating,
    string? Summary, IReadOnlyList<string> Genres, IReadOnlyList<ItemSummary> Tracks, IReadOnlyList<AlbumRow> Rows, bool TracksLoading)
{
    /// <summary>The line under the artist: the year, then <see cref="Meta"/>, " · " apart.</summary>
    public string Line => string.Join(" · ", new[] { Year?.ToString(CultureInfo.InvariantCulture), Meta }.Where(s => !string.IsNullOrEmpty(s)));
}

/// <summary>One line of an album's track list.</summary>
public abstract record AlbumRow
{
    public abstract string Key { get; }

    /// <summary>"Disc 2", above its tracks, on albums of several discs.</summary>
    public sealed record Disc(int Number) : AlbumRow
    {
        public override string Key => "disc-" + Number.ToString(CultureInfo.InvariantCulture);

        public string Label => MusicText.Disc(Number);
    }

    /// <summary>
    /// A track: its number on the disc, its title, the track's own artist when it isn't the
    /// album's, its length ("3:12"), and its position in the album's tracks (what plays from it).
    /// </summary>
    public sealed record Track(long Id, int Position, string Number, string Title, string? Artist, string Duration) : AlbumRow
    {
        public override string Key => "track-" + Id.ToString(CultureInfo.InvariantCulture);
    }
}

/// <summary>The state of an artist's or album's page.</summary>
public abstract record MusicPageState<TPage>
{
    public sealed record Loading : MusicPageState<TPage>;

    /// <summary>No such item, or one this profile may not see (404): "Artist not found", with Back.</summary>
    public sealed record NotFound : MusicPageState<TPage>;

    /// <summary>It didn't load and nothing was saved: "Couldn't load this album", the error's sentence, and Retry.</summary>
    public sealed record Failed(Exception Error) : MusicPageState<TPage>;

    /// <summary>
    /// The page; <paramref name="Error"/>: the server couldn't be asked again, and what shows is what
    /// was saved. <paramref name="Current"/>: the track playing in the music (one of the page's),
    /// and whether it <paramref name="Playing"/>; <paramref name="Busy"/> while Play gathers the tracks.
    /// </summary>
    public sealed record Ready(TPage Page, Exception? Error = null, long? Current = null, bool Playing = false, bool Busy = false) : MusicPageState<TPage>;
}

/// <summary>What a music page tells its screen once.</summary>
public abstract record MusicEvent
{
    /// <summary>Music started: the TV opens Now Playing.</summary>
    public sealed record Played : MusicEvent;

    /// <summary>An album or artist without tracks: "Nothing to play: it has no tracks."</summary>
    public sealed record NothingToPlay : MusicEvent;

    /// <summary>Something the person did failed, with the error's sentence.</summary>
    public sealed record Failed(Exception Error) : MusicEvent;
}

/// <summary>The pages' rules (the Android app's MusicPages.kt).</summary>
public static class MusicPages
{
    /// <summary>The artist page of <paramref name="detail"/> with <paramref name="albums"/> (null while they load).</summary>
    public static ArtistView Artist(ItemDetail detail, IReadOnlyList<ItemSummary>? albums, string origin)
    {
        var sorted = (albums ?? Array.Empty<ItemSummary>())
            .OrderByDescending(a => a.Year ?? 0)
            .ThenBy(a => (a.SortTitle.Length > 0 ? a.SortTitle : a.Title).ToLowerInvariant(), StringComparer.Ordinal)
            .ToList();
        var albumCount = detail.ChildCount ?? albums?.Count;
        var counts = string.Join(" · ", new[]
        {
            albumCount is { } a ? MusicText.AlbumCount(a) : null,
            detail.LeafCount is { } t ? MusicText.TrackCount(t) : null,
        }.Where(s => s != null));
        return new ArtistView(detail.Id, detail.Title, origin, NonEmpty(detail.PosterUrl), NonEmpty(detail.BackdropUrl), NonBlank(detail.Summary), detail.Genres,
            counts, sorted, albums == null, detail);
    }

    /// <summary>The album page of <paramref name="detail"/> with <paramref name="tracks"/> (null while they load).</summary>
    public static AlbumView Album(ItemDetail detail, IReadOnlyList<ItemSummary>? tracks, string origin)
    {
        var list = tracks ?? Array.Empty<ItemSummary>();
        var total = list.Sum(t => t.DurationMs ?? 0);
        var meta = string.Join(" · ", new[] { list.Count > 0 ? MusicText.TrackCount(list.Count) : null, Format.Duration(total) }.Where(s => !string.IsNullOrEmpty(s)));
        var rating = detail.UserState.Rating is { } r ? (int)Math.Floor(r + 0.5) : 0;
        return new AlbumView(detail.Id, detail.Title, origin, NonEmpty(detail.PosterUrl), detail.ParentTitle, detail.ParentId, detail.Year, meta,
            rating > 0 ? rating : null, NonBlank(detail.Summary), detail.Genres, list, Rows(list, detail.ParentTitle), tracks == null);
    }

    /// <summary>Disc headers (when there are several discs) and the tracks, as the web lists them.</summary>
    public static IReadOnlyList<AlbumRow> Rows(IReadOnlyList<ItemSummary> tracks, string? albumArtist)
    {
        var several = tracks.Select(t => t.ParentIndex ?? 1).Distinct().Count() > 1;
        var rows = new List<AlbumRow>();
        int? disc = null;
        for (var i = 0; i < tracks.Count; i++)
        {
            var t = tracks[i];
            var d = t.ParentIndex ?? 1;
            if (several && d != disc) rows.Add(new AlbumRow.Disc(d));
            disc = d;
            rows.Add(new AlbumRow.Track(
                t.Id,
                i,
                t.Index?.ToString(CultureInfo.InvariantCulture) ?? "",
                t.Title,
                !string.IsNullOrWhiteSpace(t.GrandparentTitle) && t.GrandparentTitle != albumArtist ? t.GrandparentTitle : null,
                t.DurationMs is > 0 ? PlayerText.Time(t.DurationMs.Value) : ""));
        }
        return rows;
    }

    /// <summary>The profile's rating as five stars: filled for each two points, and for a half (the Android app's Stars).</summary>
    public static IReadOnlyList<bool> Stars(int rating) => Enumerable.Range(1, 5).Select(star => rating >= star * 2 || rating == star * 2 - 1).ToList();

    static string? NonEmpty(string? s) => string.IsNullOrEmpty(s) ? null : s;

    static string? NonBlank(string? s) => string.IsNullOrWhiteSpace(s) ? null : s;
}

/// <summary>
/// What an artist's or album's page has in common (the Android app's pageState): its item from
/// what was saved first, the server's answer after, its children beside it, and the music playing.
/// </summary>
public abstract class MusicPageModel<TPage> : IDisposable
{
    readonly IItemSource source;
    readonly IScheduler scheduler;
    readonly List<IDisposable> watches = new List<IDisposable>();
    ItemDetail? detail;
    IReadOnlyList<ItemSummary>? children;
    Exception? error;
    int generation;
    bool reloadPosted;

    protected MusicPageModel(IItemSource source, IMusicControl music, IScheduler scheduler, long id)
    {
        this.source = source;
        this.scheduler = scheduler;
        Music = music;
        Id = id;
    }

    protected IMusicControl Music { get; }

    protected IItemSource Source => source;

    protected IScheduler Scheduler => scheduler;

    protected bool Gone { get; private set; }

    /// <summary>Play gathers the tracks.</summary>
    protected bool Busy { get; set; }

    public long Id { get; }

    public Signal<MusicPageState<TPage>> State { get; } = new Signal<MusicPageState<TPage>>(new MusicPageState<TPage>.Loading());

    /// <summary>What the page says once: Now Playing opens on <see cref="MusicEvent.Played"/>; the others show in danger text under the buttons.</summary>
    public event Action<MusicEvent>? Told;

    protected void Tell(MusicEvent e) => Told?.Invoke(e);

    /// <summary>Starts: the subclass's constructor calls it once it is ready.</summary>
    protected void Begin()
    {
        watches.Add(Music.Now.Watch(_ => Compute()));
        watches.Add(source.OnChange(() =>
        {
            // Changes come in bursts: the page asks again once for them.
            if (reloadPosted) return;
            reloadPosted = true;
            scheduler.Post(() =>
            {
                reloadPosted = false;
                Load();
            });
        }));
        Load();
    }

    /// <summary>Loads the page again (Retry).</summary>
    public void Retry()
    {
        error = null;
        Compute();
        Load();
    }

    void Load()
    {
        if (Gone) return;
        var at = ++generation;
        var saved = source.Saved(Id);
        if (saved != null && detail == null) detail = saved;
        Compute();
        if (saved == null || source.Stale(Id))
            Plays.When(scheduler, () => source.Item(Id), d =>
            {
                if (Gone || at != generation) return;
                detail = d;
                error = null;
                Compute();
            }, e =>
            {
                if (Gone || at != generation) return;
                error = e;
                Compute();
            });
        Plays.When(scheduler, () => source.Children(Id), list =>
        {
            if (Gone || at != generation) return;
            children = list;
            Compute();
        }, _ =>
        {
            // The children show as loading until the page is asked again (Retry, or the server's next change).
        });
    }

    protected abstract TPage Make(ItemDetail detail, IReadOnlyList<ItemSummary>? children, string origin);

    /// <summary>The track playing that the page marks: any (an artist's), or one of its own (an album's).</summary>
    protected virtual long? Mine(long current, TPage page) => current;

    protected void Compute()
    {
        if (Gone) return;
        if (detail != null)
        {
            var page = Make(detail, children, source.Origin);
            var now = Music.Now.Value;
            var current = now != null ? Mine(now.ItemId, page) : null;
            var playing = current != null && (now!.Playing || now.Loading);
            State.Value = new MusicPageState<TPage>.Ready(page, error, current, playing, Busy);
        }
        else if (error is ApiException { Status: 404 }) State.Value = new MusicPageState<TPage>.NotFound();
        else if (error != null) State.Value = new MusicPageState<TPage>.Failed(error);
        else State.Value = new MusicPageState<TPage>.Loading();
    }

    protected TPage? Page => State.Value is MusicPageState<TPage>.Ready r ? r.Page : default;

    public void Dispose()
    {
        Gone = true;
        foreach (var w in watches) w.Dispose();
        watches.Clear();
    }
}

/// <summary>
/// An artist's page (the Android app's ArtistViewModel; TvArtistScreen): their backdrop, the round
/// picture (120 dp), the name, "4 albums · 40 tracks" and their genres; Play and Shuffle (every
/// track of theirs, album by album, which opens Now Playing); then their albums, newest first, as a
/// row of square tiles. Focus starts on Play; Down goes to the albums.
/// </summary>
public sealed class ArtistModel : MusicPageModel<ArtistView>
{
    readonly Random random;

    public ArtistModel(IItemSource source, IMusicControl music, IScheduler scheduler, long artistId, Random? random = null)
        : base(source, music, scheduler, artistId)
    {
        this.random = random ?? new Random();
        Begin();
    }

    protected override ArtistView Make(ItemDetail detail, IReadOnlyList<ItemSummary>? children, string origin) => MusicPages.Artist(detail, children, origin);

    /// <summary>Plays every track of the artist's, in album order, or shuffled.</summary>
    public async void Play(bool shuffle = false)
    {
        if (Page == null || Busy) return;
        Busy = true;
        Compute();
        try
        {
            var tracks = await Tracks();
            if (Gone) return;
            if (tracks.Count == 0) Tell(new MusicEvent.NothingToPlay());
            else
            {
                Music.Play(tracks, shuffle ? random.Next(tracks.Count) : 0, shuffle);
                Tell(new MusicEvent.Played());
            }
        }
        catch (Exception e)
        {
            if (!Gone) Tell(new MusicEvent.Failed(e));
        }
        finally
        {
            Busy = false;
            Compute();
        }
    }

    /// <summary>The artist's tracks, album by album as the server lists the albums.</summary>
    async Task<IReadOnlyList<ItemSummary>> Tracks()
    {
        var albums = await Source.Children(Id);
        var each = await Task.WhenAll(albums.Select(a => Source.Children(a.Id)));
        return each.SelectMany(t => t).ToList();
    }
}

/// <summary>
/// An album's page (the Android app's AlbumViewModel; TvAlbumScreen): the art (220 dp) and what it
/// is in a 280 dp column on the left (the artist, a link to their page; the year, the tracks and
/// their length; the profile's rating as stars), Play, Shuffle and Now playing under it; the tracks
/// on the right with disc headers, the one playing marked. Select on a track plays the album from
/// it, or pauses the track playing (<see cref="PlayOrToggle"/>). Playing opens Now Playing. Focus
/// starts on Play, or on the track the album was opened from (<see cref="HighlightId"/>).
/// </summary>
public sealed class AlbumModel : MusicPageModel<AlbumView>
{
    readonly Random random;

    public AlbumModel(IItemSource source, IMusicControl music, IScheduler scheduler, long albumId, long? highlightId = null, Random? random = null)
        : base(source, music, scheduler, albumId)
    {
        this.random = random ?? new Random();
        HighlightId = highlightId;
        Begin();
    }

    /// <summary>The track the album was opened from, which takes focus first.</summary>
    public long? HighlightId { get; }

    protected override AlbumView Make(ItemDetail detail, IReadOnlyList<ItemSummary>? children, string origin) => MusicPages.Album(detail, children, origin);

    protected override long? Mine(long current, AlbumView page) => page.Tracks.Any(t => t.Id == current) ? current : null;

    IReadOnlyList<ItemSummary> TrackList => Page?.Tracks ?? Array.Empty<ItemSummary>();

    /// <summary>Plays the album from its track at <paramref name="position"/> (the first by default).</summary>
    public void Play(int position = 0)
    {
        var list = TrackList;
        if (list.Count == 0)
        {
            Tell(new MusicEvent.NothingToPlay());
            return;
        }
        Music.Play(list, Math.Max(0, Math.Min(position, list.Count - 1)));
        Tell(new MusicEvent.Played());
    }

    public void Shuffle()
    {
        var list = TrackList;
        if (list.Count == 0)
        {
            Tell(new MusicEvent.NothingToPlay());
            return;
        }
        Music.Play(list, random.Next(list.Count), shuffle: true);
        Tell(new MusicEvent.Played());
    }

    /// <summary>A track's row: the track playing pauses or plays on; another one plays the album from it.</summary>
    public void PlayOrToggle(int position)
    {
        var list = TrackList;
        if (position < 0 || position >= list.Count) return;
        if (State.Value is MusicPageState<AlbumView>.Ready { Current: { } current } && current == list[position].Id) Music.Toggle();
        else Play(position);
    }

    /// <summary>The profile's rating, 0 to 10 in half stars; null clears it.</summary>
    public void Rate(int? value) =>
        Plays.When(Scheduler, () => Source.Rate(Id, value), () => { }, e =>
        {
            if (!Gone) Tell(new MusicEvent.Failed(e));
        });
}
