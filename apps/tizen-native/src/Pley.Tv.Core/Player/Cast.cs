using System;
using System.Collections.Generic;
using System.Linq;
using System.Threading.Tasks;
using Pley.Tv.Core.Api;
using Pley.Tv.Core.Auth;
using Pley.Tv.Core.Ui;

namespace Pley.Tv.Core.Player;

/// <summary>What a cast asks of this TV.</summary>
public enum CastAct
{
    /// <summary>Another TV's, or one that came while the app couldn't take it.</summary>
    None,
    /// <summary>The title <see cref="CastAction.Id"/>, from <see cref="CastAction.StartMs"/>.</summary>
    Item,
    /// <summary>The playlist <see cref="CastAction.Id"/>.</summary>
    Playlist,
    /// <summary>The Live TV channel <see cref="CastAction.Id"/>.</summary>
    Channel,
}

/// <summary>
/// The remote's profile, another of the household's, that this TV switches to before it plays a
/// cast, with the cast's <paramref name="Ticket"/>.
/// </summary>
public sealed record CastSwitch(long Profile, string Ticket);

/// <summary>
/// What a cast asks of this TV: <paramref name="Act"/> on <paramref name="Id"/>, a title from
/// <paramref name="StartMs"/> (null: where Play resumes) with the audio <paramref name="AudioIndex"/>
/// and the subtitles <paramref name="SubtitleIndex"/> (-1: none) picked on the remote's page (null:
/// the profile's choices), a playlist, album or artist <paramref name="Shuffle"/>d; after switching
/// to the remote's profile (<paramref name="SwitchTo"/>) when it is another one.
/// </summary>
public sealed record CastAction(CastAct Act, long Id = 0, long? StartMs = null, bool Shuffle = false, int? AudioIndex = null, int? SubtitleIndex = null,
    CastSwitch? SwitchTo = null)
{
    public static readonly CastAction None = new CastAction(CastAct.None);
}

/// <summary>What a title cast here plays, by its kind.</summary>
public enum ItemPlays
{
    /// <summary>A movie, an episode or an extra, in the player.</summary>
    Video,
    /// <summary>A show's or a season's next episode, as their Play has it.</summary>
    NextEpisode,
    /// <summary>An album's tracks, as the music's queue.</summary>
    Album,
    /// <summary>An artist's tracks, album by album.</summary>
    Artist,
    /// <summary>A track, in its album from it.</summary>
    Track,
    /// <summary>A kind a newer server knows.</summary>
    Nothing,
}

/// <summary>What a title cast here plays (<see cref="Casts.Plays"/>): from <paramref name="StartMs"/>, or shuffled.</summary>
public sealed record ItemPlay(ItemPlays Plays, long? StartMs = null, bool Shuffle = false);

/// <summary>
/// Casts from the profile's remote, on another of its devices (docs/API.md, "Casting"): this TV
/// takes them while it shows, and plays what it is sent as its own Play would, in place of what
/// plays, saying where it came from. A remote on another profile of the household switches the TV
/// to its profile first. The rules are packages/tv's, pinned for both Samsung apps by
/// packages/tv/test/fixtures/tracks.json.
/// </summary>
public static class Casts
{
    /// <summary>How long "Playing from …" shows over what starts.</summary>
    public const long BannerMs = 4_000;

    /// <summary>Said when a show, an album or a playlist has nothing to play.</summary>
    public const string NothingToPlay = "Nothing to play";

    /// <summary>Said when the switch to the remote's profile failed: the cast plays nothing.</summary>
    public const string SwitchFailed = "Couldn't switch to the remote's profile";

    /// <summary>
    /// The event stream's query that makes this TV a receiver: its install id, a TV, taking titles
    /// and playlists always and channels where Live TV shows. The server names the TV, as it does
    /// its players.
    /// </summary>
    // switch: it switches to the profile of a remote of another profile that casts to it.
    public static string ReceiverQuery(string id, bool live) => $"receiver={Uri.EscapeDataString(id)}&device_kind=tv&casts={(live ? "item,live,switch" : "item,switch")}";

    /// <summary>
    /// The casts' rule: one for another receiver is another device's, and one that comes while the
    /// app can't take it (hidden, signed out, asking who's watching: <paramref name="shown"/> false)
    /// does nothing; else it plays what it names. One from a remote on another profile than
    /// <paramref name="profile"/>, the one the app acts as (null while it isn't known), switches to it
    /// first with its ticket; without a ticket it does nothing, as the TV can't play it as the
    /// profile that sent it.
    /// </summary>
    public static CastAction Action(string receiver, long? profile, bool shown, ServerEvent e)
    {
        if (!shown || e.Type != "cast" || e.ReceiverId != receiver) return CastAction.None;
        CastSwitch? switchTo = null;
        if (e.ProfileId is long other && other != profile)
        {
            if (string.IsNullOrEmpty(e.SwitchTicket)) return CastAction.None;
            switchTo = new CastSwitch(other, e.SwitchTicket!);
        }
        if (e.ItemId is long item)
            return new CastAction(CastAct.Item, item, e.StartMs is long at ? Math.Max(0, at) : null, e.Shuffle == true, e.AudioIndex, e.SubtitleIndex, switchTo);
        if (e.PlaylistId is long playlist) return new CastAction(CastAct.Playlist, playlist, Shuffle: e.Shuffle == true, SwitchTo: switchTo);
        if (e.ChannelId is long channel) return new CastAction(CastAct.Channel, channel, SwitchTo: switchTo);
        return CastAction.None;
    }

    /// <summary>"Playing from {device}", as the cast starts; "another device" when the remote's has no name.</summary>
    public static string Banner(string? from)
    {
        var name = (from ?? "").Trim();
        return $"Playing from {(name.Length > 0 ? name : "another device")}";
    }

    /// <summary>
    /// What a title of <paramref name="kind"/> plays when cast here, as Play on its page does: a
    /// movie, an episode or an extra in the player (from <paramref name="startMs"/>, else where Play
    /// resumes); a show or a season its next episode; an album or an artist its tracks as the
    /// music's queue; a track its album from it.
    /// </summary>
    public static ItemPlay Plays(string kind, long? startMs, bool shuffle) => kind switch
    {
        "movie" or "episode" or "extra" => new ItemPlay(ItemPlays.Video, startMs),
        "show" or "season" => new ItemPlay(ItemPlays.NextEpisode),
        "album" => new ItemPlay(ItemPlays.Album, Shuffle: shuffle),
        "artist" => new ItemPlay(ItemPlays.Artist, Shuffle: shuffle),
        "track" => new ItemPlay(ItemPlays.Track, startMs),
        _ => new ItemPlay(ItemPlays.Nothing),
    };
}

/// <summary>What playing a cast reads from the server: the app's API, or a fake in tests.</summary>
public interface ICastServer
{
    Task<ItemDetail> Item(long id);

    /// <summary>The episode a show's or season's Play starts; null with none.</summary>
    Task<ItemSummary?> OnDeck(long id);

    /// <summary>An album's tracks, an artist's albums.</summary>
    Task<IReadOnlyList<ItemSummary>> Children(long id);

    Task<PlaylistDetail> Playlist(long id);
}

/// <summary>What a cast needs of the app around it.</summary>
public interface ICastTarget
{
    /// <summary>This TV's receiver id: its install id, which its playbacks are made with.</summary>
    string ReceiverId { get; }

    /// <summary>Whether it takes casts now: the app shows, signed in, past "Who's watching?".</summary>
    bool Shown { get; }

    /// <summary>The profile the app acts as, null while it isn't known.</summary>
    long? Profile { get; }

    /// <summary>
    /// Moves the app to <paramref name="profile"/> with a cast's <paramref name="ticket"/>, as a
    /// switch on "Who's watching?" would, what played as the profile before ending first; fails when
    /// it can't.
    /// </summary>
    Task SwitchTo(long profile, string ticket);

    /// <summary>Plays <paramref name="request"/> in the player, in place of what plays, <paramref name="banner"/> over it as it starts.</summary>
    void Video(PlayRequest request, string banner);

    /// <summary>
    /// Plays <paramref name="tracks"/> as the music's queue from <paramref name="at"/>
    /// (<paramref name="fromMs"/> into it), shuffled or not, in place of what plays: Now Playing
    /// shows, <paramref name="banner"/> over it.
    /// </summary>
    void Music(IReadOnlyList<ItemSummary> tracks, int at, bool shuffle, long fromMs, string banner);

    /// <summary>Tunes <paramref name="channelId"/> in the live player, in place of what plays, <paramref name="banner"/> over it.</summary>
    void Channel(long channelId, string banner);

    /// <summary>A notice that there is nothing to play.</summary>
    void Say(string text);

    /// <summary>Asking for what to play failed: the app says why, as its own Play does.</summary>
    void Failed(Exception error);
}

/// <summary>
/// This TV's side of casting: a cast for it plays (the latest one, when another comes while the
/// first is still asked for), and says where it came from as it starts. One from another profile's
/// remote plays once the TV switched to that profile, or says it couldn't. UI thread only.
/// </summary>
public sealed class CastReceiver
{
    readonly ICastServer server;
    readonly ICastTarget target;
    readonly IScheduler scheduler;
    readonly Random random;
    int casts;
    /// <summary>While a cast's switch is being made, the casts heard meanwhile, weighed once it is over; null otherwise.</summary>
    List<ServerEvent>? waiting;

    public CastReceiver(ICastServer server, ICastTarget target, IScheduler scheduler, Random? random = null)
    {
        this.server = server;
        this.target = target;
        this.scheduler = scheduler;
        this.random = random ?? new Random();
    }

    /// <summary>A cast heard on the event stream.</summary>
    public void Take(ServerEvent e)
    {
        if (waiting != null)
        {
            // Weighed once the switch is over, as the profile the TV is on then: one sent to the
            // profile it leaves (no profile id) is that profile's, which the TV no longer is.
            if (e.ProfileId != null) waiting.Add(e);
            return;
        }
        var action = Casts.Action(target.ReceiverId, target.Profile, target.Shown, e);
        if (action.Act == CastAct.None) return;
        var mine = ++casts;
        var banner = Casts.Banner(e.From);
        if (action.SwitchTo is not { } to)
        {
            Play(action, mine, banner);
            return;
        }
        waiting = new List<ServerEvent>();
        Plays.When(scheduler, () => target.SwitchTo(to.Profile, to.Ticket), () =>
        {
            // Those heard meanwhile are newer: the latest of them plays in its place.
            TakeWaiting();
            Play(action, mine, banner);
        }, _ =>
        {
            if (mine == casts && target.Shown) target.Say(Casts.SwitchFailed);
            TakeWaiting();
        });
    }

    /// <summary>The switch is over: the casts heard while it was made are taken now, in order.</summary>
    void TakeWaiting()
    {
        var heard = waiting ?? new List<ServerEvent>();
        waiting = null;
        foreach (var e in heard) Take(e);
    }

    /// <summary>Plays <paramref name="action"/>, cast number <paramref name="mine"/>, as the profile the app acts as now.</summary>
    void Play(CastAction action, int mine, string banner)
    {
        if (mine != casts || !target.Shown) return;
        Plays.When(scheduler, () => Resolve(action), play =>
        {
            // Another cast came since, or the app went: this one plays nothing.
            if (mine != casts || !target.Shown) return;
            if (play == null) target.Say(Casts.NothingToPlay);
            else play(banner);
        }, error =>
        {
            if (mine == casts && target.Shown) target.Failed(error);
        });
    }

    /// <summary>What playing <paramref name="action"/> does, from what the server says; null when it has nothing to play.</summary>
    async Task<Action<string>?> Resolve(CastAction action)
    {
        switch (action.Act)
        {
            case CastAct.Channel:
                return banner => target.Channel(action.Id, banner);
            case CastAct.Playlist:
            {
                var playlist = await server.Playlist(action.Id);
                var items = playlist.Entries.Select(e => e.Item).ToList();
                if (items.Count == 0) return null;
                var at = action.Shuffle ? random.Next(items.Count) : 0;
                // As the web's playlist page plays it: music as the queue, video in the player through the playlist.
                if (playlist.Kind == "audio") return banner => target.Music(items, at, action.Shuffle, 0, banner);
                var first = items[at];
                return banner => target.Video(new PlayRequest(first.Id, ResumeOf(first), false, Queue: new PlayQueue.Playlist(playlist.Id)), banner);
            }
        }
        var detail = await server.Item(action.Id);
        var plan = Casts.Plays(detail.Kind, action.StartMs, action.Shuffle);
        switch (plan.Plays)
        {
            case ItemPlays.Video:
                // The tracks picked on the remote's page play, in place of the profile's choices, for this play only.
                return banner => target.Video(new PlayRequest(detail.Id, plan.StartMs ?? ResumeOf(detail), false,
                    AudioIndex: action.AudioIndex, SubtitleIndex: action.SubtitleIndex), banner);
            case ItemPlays.NextEpisode:
            {
                // As a show's Play (Plays.Resolve): the episode from where it was left.
                var next = await server.OnDeck(detail.Id);
                if (next == null) return null;
                return banner => target.Video(new PlayRequest(next.Id, next.UserState.PositionMs, false), banner);
            }
            case ItemPlays.Album:
            case ItemPlays.Artist:
            {
                var tracks = plan.Plays == ItemPlays.Album ? await Tracks(detail.Id) : await ArtistTracks(detail.Id);
                if (tracks.Count == 0) return null;
                var at = plan.Shuffle ? random.Next(tracks.Count) : 0;
                return banner => target.Music(tracks, at, plan.Shuffle, 0, banner);
            }
            case ItemPlays.Track:
            {
                // Its album from it, as the album's page plays a track; alone when it has none.
                var album = detail.ParentId is long parent ? await Tracks(parent) : new List<ItemSummary>();
                var at = album.FindIndex(t => t.Id == detail.Id);
                IReadOnlyList<ItemSummary> queue = at >= 0 ? album : new List<ItemSummary> { detail };
                var from = plan.StartMs ?? 0;
                return banner => target.Music(queue, Math.Max(0, at), false, from, banner);
            }
            default:
                return null;
        }
    }

    /// <summary>Where Play starts a title: the start once watched, else where it was left.</summary>
    static long ResumeOf(ItemSummary item) => item.UserState.Watched ? 0 : Math.Max(0, item.UserState.PositionMs);

    /// <summary>An album's tracks, in order.</summary>
    async Task<List<ItemSummary>> Tracks(long albumId) => (await server.Children(albumId)).Where(i => i.Kind == "track").ToList();

    /// <summary>An artist's tracks, album by album as the server lists the albums: the artist page's Play.</summary>
    async Task<List<ItemSummary>> ArtistTracks(long artistId)
    {
        var albums = (await server.Children(artistId)).Where(i => i.Kind == "album").ToList();
        var each = await Task.WhenAll(albums.Select(a => Tracks(a.Id)));
        return each.SelectMany(t => t).ToList();
    }
}

/// <summary>What a cast reads in the app: <see cref="AppState"/>'s API, the details it keeps.</summary>
public sealed class AppCastServer : ICastServer
{
    readonly AppState app;

    public AppCastServer(AppState app) => this.app = app;

    Api.Api Api => app.Api.Value ?? throw new SignedOutException();

    public Task<ItemDetail> Item(long id) => app.Api.Value == null ? Task.FromException<ItemDetail>(new SignedOutException()) : app.Item(id);

    public async Task<ItemSummary?> OnDeck(long id) => (await Api.OnDeck(id).ConfigureAwait(false)).Item;

    public async Task<IReadOnlyList<ItemSummary>> Children(long id) => (await Api.Children(id).ConfigureAwait(false)).Items;

    public Task<PlaylistDetail> Playlist(long id) => Api.Playlist(id);
}
