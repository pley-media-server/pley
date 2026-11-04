using System;
using System.Collections.Generic;
using System.Linq;
using System.Threading.Tasks;
using Pley.Tv.Core.Api;
using Pley.Tv.Core.Auth;
using Pley.Tv.Core.Ui;

namespace Pley.Tv.Core.Player;

/// <summary>Why music stopped playing.</summary>
public enum MusicError
{
    /// <summary>Nobody is signed in (anymore).</summary>
    SignedOut,
    /// <summary>The server couldn't be reached.</summary>
    Offline,
    /// <summary>The server couldn't play the track, nor convert it.</summary>
    Unplayable,
    /// <summary>The server stopped the playback (an administrator, or the profile elsewhere).</summary>
    Stopped,
}

/// <summary>
/// The track playing in the background, for Now Playing and the rail. It changes when the track,
/// the queue or playing changes, never with the position: that is read with
/// <see cref="MusicPlayer.Position"/> while drawing.
/// </summary>
public sealed record NowPlaying
{
    public long ItemId { get; init; }
    public string Title { get; init; } = "";
    /// <summary>The track's artist, else the album's.</summary>
    public string? Artist { get; init; }
    public bool Playing { get; init; }
    public string? Album { get; init; }
    public long? AlbumId { get; init; }
    public long? ArtistId { get; init; }
    /// <summary>The album art's full address on the server, without a width; null without one.</summary>
    public string? ArtUrl { get; init; }
    /// <summary>The track's length as the server knows it; 0 when it doesn't.</summary>
    public long DurationMs { get; init; }
    /// <summary>Where the track is in the queue, from 0, and how long the queue is.</summary>
    public int Index { get; init; }
    public int Count { get; init; } = 1;
    public bool Shuffle { get; init; }
    /// <summary>Waiting for the server or the network before it plays.</summary>
    public bool Loading { get; init; }
    /// <summary>The queue played to its end: Play starts the last track again.</summary>
    public bool Ended { get; init; }
    /// <summary>Why it doesn't play; null while it does.</summary>
    public MusicError? Error { get; init; }

    public bool HasNext => Index + 1 < Count;

    /// <summary>"Artist · Album".</summary>
    public string Byline => string.Join(" · ", new[] { Artist, Album }.Where(s => !string.IsNullOrWhiteSpace(s)));
}

/// <summary>Where the track is, read when drawn (the scrubber).</summary>
public sealed record MusicPosition(long PositionMs = 0, long DurationMs = 0, long BufferedMs = 0)
{
    /// <summary>How far along, 0 to 1.</summary>
    public float Fraction => DurationMs > 0 ? Math.Max(0f, Math.Min(1f, (float)PositionMs / DurationMs)) : 0f;
}

/// <summary>What playing music needs of the server: the app's API, or a fake in tests.</summary>
public interface IMusicServer
{
    /// <summary>An absolute address on the server for <paramref name="path"/>.</summary>
    string Url(string path);

    /// <summary>
    /// A session for track <paramref name="itemId"/> from <paramref name="startMs"/>, with keyed
    /// addresses, converted when <paramref name="transcode"/>. The reports of the session before
    /// go out first (for a few seconds at most): the server's limit on conversions.
    /// </summary>
    Task<IPlaybackSession> Start(long itemId, long startMs, bool transcode);

    /// <summary>Whether <paramref name="session"/> goes on: it throws when it is gone (404) or was stopped (<see cref="SessionStoppedException"/>).</summary>
    Task Status(string session);
}

/// <summary>What the music pages play through: the app's <see cref="MusicPlayer"/>, or a fake in tests.</summary>
public interface IMusicControl
{
    Signal<NowPlaying?> Now { get; }

    void Play(IReadOnlyList<ItemSummary> tracks, int startIndex = 0, bool shuffle = false);

    void Toggle();
}

/// <summary>
/// Music's queue and the server's side of it (the Android app's MusicPlayback, as packages/tv's
/// MusicPlayer does it on a Samsung TV):
///
/// - one server session per track (POST /api/playback, keyed addresses for the TV's player), the
///   one before ended first, so a conversion never waits on the last one;
/// - progress every 10 s while playing, on pause, and every 30 s paused (the server lets a session
///   it hasn't heard from go, its addresses with it); at the end of a track "stopped" at its
///   length and DELETE, then the next track by itself;
/// - a track the TV can't decode is asked for again as a conversion, and a session that went (a
///   long pause) is started again where it was, up to three times in a row;
/// - Previous restarts a track that has played more than 3 s;
/// - a sleep timer pauses after the minutes chosen, or at the end of the track (the next one
///   waits, paused);
/// - a video takes the TV's one media pipeline: <see cref="YieldToVideo"/> pauses the music and
///   ends its session; Play starts a new one where it was;
/// - the remote, on another of the profile's devices, plays and pauses, seeks, skips, and goes to
///   the next or previous track (<see cref="Obey"/>), behind every page too.
///
/// It lives as long as the app, so it plays on with no screen open. UI thread only.
/// </summary>
public sealed class MusicPlayer : IMusicControl
{
    /// <summary>Progress reports while playing, as on the web.</summary>
    public const long ProgressEveryMs = 10_000;

    /// <summary>While paused: the server forgets players it hasn't heard from, and their addresses with them.</summary>
    public const long PausedEveryMs = 30_000;

    /// <summary>New sessions asked in a row for one track before it counts as failed.</summary>
    public const int MaxRecreates = 3;

    readonly IMusicServer server;
    readonly Func<IMediaEngine> engines;
    readonly IScheduler scheduler;
    readonly MusicQueue queue;
    readonly SleepTimer timer;
    IMediaEngine? output;
    IPlaybackSession? session;
    IDisposable? ticker;
    /// <summary>Bumped as each track starts: an answer for one before is dropped.</summary>
    int loads;
    /// <summary>Ask for a conversion: direct play failed to decode the current track.</summary>
    bool fallback;
    int recreates;
    bool playing, loading, ended;
    /// <summary>A video took the TV's player: the session went, the place stays.</summary>
    bool yielded;
    /// <summary>What the person wants: playing, or paused (the TV's player has no such thing of its own).</summary>
    bool wantPlay;
    MusicError? error;

    public MusicPlayer(IMusicServer server, Func<IMediaEngine> engines, IScheduler scheduler, Random? random = null)
    {
        this.server = server;
        this.engines = engines;
        this.scheduler = scheduler;
        queue = new MusicQueue(random);
        timer = new SleepTimer(scheduler, TrackLeftMs, () => queue.Current?.Id, Pause);
    }

    /// <summary>What plays now; null when nothing does (Now Playing leaves the rail).</summary>
    public Signal<NowPlaying?> Now { get; } = new Signal<NowPlaying?>(null);

    /// <summary>The queue as it plays; empty when nothing does.</summary>
    public Signal<MusicQueueState> Queue { get; } = new Signal<MusicQueueState>(MusicQueueState.Empty);

    /// <summary>The sleep timer.</summary>
    public Signal<SleepState> Sleep => timer.State;

    /// <summary>Whether there is anything to play.</summary>
    public bool Active => !queue.IsEmpty;

    /// <summary>Where the track is now. Cheap; read it while drawing.</summary>
    public MusicPosition Position()
    {
        var o = output;
        if (o == null) return new MusicPosition();
        var duration = o.DurationMs > 0 ? o.DurationMs : queue.Current?.DurationMs ?? 0;
        var position = ended ? duration : o.PositionMs;
        return new MusicPosition(Math.Max(0, position), duration, Math.Max(0, o.BufferedMs));
    }

    /// <summary>Plays <paramref name="tracks"/> from <paramref name="startIndex"/>; shuffled, in a random order that starts with that track.</summary>
    public void Play(IReadOnlyList<ItemSummary> tracks, int startIndex = 0, bool shuffle = false)
    {
        if (tracks.Count == 0) return;
        queue.Start(tracks, startIndex, shuffle);
        StartTrack(0);
    }

    /// <summary>Plays; after the queue's end, a failure or a video, starts the current track again.</summary>
    public void Play()
    {
        if (queue.IsEmpty) return;
        if (ended) StartTrack(0);
        else if (error != null || yielded) StartTrack(LastPosition(), sameTrack: true, play: true);
        else if (session == null)
        {
            // Still asking for the track: it plays once it comes.
            wantPlay = true;
            Publish();
        }
        else
        {
            wantPlay = true;
            timer.Resumed();
            Output().Play();
            Publish();
        }
    }

    public void Pause()
    {
        if (output == null || !wantPlay) return;
        wantPlay = false;
        output.Pause();
        if (!ended) session?.Progress(LastPosition(), false);
        Publish();
    }

    /// <summary>Plays or pauses; after the queue's end, plays its last track again.</summary>
    public void Toggle()
    {
        if (output != null && wantPlay && !ended && error == null && !yielded) Pause();
        else Play();
    }

    /// <summary>The next track, if there is one.</summary>
    public void Next()
    {
        if (queue.Next()) StartTrack(0);
    }

    /// <summary>Back to the start of the track, or the track before when it has played under 3 seconds.</summary>
    public void Previous()
    {
        var o = output;
        switch (queue.Previous(o?.PositionMs ?? 0))
        {
            case PreviousMove.Moved:
                StartTrack(0);
                break;
            default:
                if (o == null || session == null || ended || error != null) StartTrack(0, sameTrack: !ended);
                else o.SeekTo(0);
                break;
        }
    }

    public void SeekTo(long positionMs)
    {
        if (ended || session == null) return;
        output?.SeekTo(Math.Max(0, positionMs));
    }

    /// <summary>Plays the queue's track at <paramref name="index"/>.</summary>
    public void JumpTo(int index)
    {
        if (queue.JumpTo(index)) StartTrack(0);
    }

    /// <summary>Shuffles what is left of the queue around the current track, or puts it back in order.</summary>
    public void SetShuffle(bool shuffle)
    {
        queue.SetShuffle(shuffle);
        Publish();
    }

    /// <summary>Sets the sleep timer (not remembered after the music stops).</summary>
    public void SetSleep(SleepChoice choice)
    {
        if (queue.IsEmpty && choice is not SleepChoice.Off) return;
        timer.Set(choice);
    }

    /// <summary>"Keep listening" during the sleep timer's notice: the same time again.</summary>
    public void KeepListening() => timer.Extend();

    /// <summary>The sleep button on Now Playing: during the notice "Keep listening", else the next choice (Off, 15 … 90 min, End of this track).</summary>
    public void StepSleep()
    {
        var s = Sleep.Value;
        if (s.SecondsLeft != null)
        {
            KeepListening();
            return;
        }
        var all = SleepChoice.All;
        var at = Enumerable.Range(0, all.Count).FirstOrDefault(i => all[i] == s.Set);
        SetSleep(all[(at + 1) % all.Count]);
    }

    /// <summary>
    /// A video takes the screen and the TV's one media pipeline: the music pauses and lets go of
    /// its session, which would otherwise be kept through a whole film; the queue and the place
    /// stay, and Play starts a new session there.
    /// </summary>
    public void YieldToVideo()
    {
        if (queue.IsEmpty) return;
        loads++;
        wantPlay = false;
        yielded = true;
        output?.Pause();
        StopTicker();
        EndSession();
        playing = false;
        loading = false;
        Publish();
    }

    /// <summary>
    /// A command of the profile's remote for the track playing, as Android's music obeys it
    /// (<see cref="Remote.Action"/>): a seek stays within the track, a skip goes from where it is
    /// now, next and previous as the buttons here. What changed is reported at once, so the
    /// remote sees it: a pause reports by itself, and another track with its new session; a play
    /// or pause that finds it so already says it again.
    /// </summary>
    public void Obey(ServerEvent command)
    {
        var s = session;
        var o = output;
        if (s == null || o == null || ended || error != null) return;
        var duration = o.DurationMs > 0 ? o.DurationMs : s.Info.DurationMs > 0 ? s.Info.DurationMs : queue.Current?.DurationMs;
        var player = new RemotePlayer(RemoteKind.Music, s.Id, wantPlay, LastPosition(), duration > 0 ? duration : null, queue.HasNext, queue.Index);
        var action = Remote.Action(player, command);
        switch (action.Act)
        {
            case RemoteAct.Play:
                Play();
                s.Progress(LastPosition(), true);
                break;
            case RemoteAct.Pause:
                Pause();
                break;
            case RemoteAct.Report:
                // Playing or paused already: said again, so a remote a little behind settles.
                s.Progress(LastPosition(), wantPlay);
                break;
            case RemoteAct.Seek:
                o.SeekTo(action.PositionMs);
                s.Progress(action.PositionMs, wantPlay);
                break;
            case RemoteAct.Next:
                Next();
                break;
            case RemoteAct.Previous:
                Previous();
                break;
        }
    }

    /// <summary>The server ended the track's session (an administrator, or the profile elsewhere): the music stops and says so. Play starts the track again.</summary>
    public void StoppedByServer(string sessionId)
    {
        var s = session;
        if (s == null || s.Id != sessionId) return;
        session = null;
        s.StoppedByServer(LastPosition());
        loads++;
        timer.Cancel();
        output?.Stop();
        Fail(MusicError.Stopped);
    }

    /// <summary>Ends the session, stops the player and forgets the queue (signing out, another profile, the app closing).</summary>
    public void Stop()
    {
        loads++;
        StopTicker();
        timer.Cancel();
        EndSession();
        if (output != null)
        {
            Unlisten(output);
            output.Release();
            output = null;
        }
        queue.Clear();
        fallback = false;
        recreates = 0;
        playing = loading = ended = wantPlay = false;
        error = null;
        Now.Value = null;
        Queue.Value = MusicQueueState.Empty;
    }

    IMediaEngine Output()
    {
        if (output != null) return output;
        output = engines();
        output.PlayingChanged += OnPlaying;
        output.StatusChanged += OnStatus;
        output.Failed += OnError;
        return output;
    }

    void Unlisten(IMediaEngine o)
    {
        o.PlayingChanged -= OnPlaying;
        o.StatusChanged -= OnStatus;
        o.Failed -= OnError;
    }

    /// <summary>
    /// Plays the current track from <paramref name="atMs"/> with a new session, ending the one
    /// before. <paramref name="sameTrack"/> keeps the conversion asked for and the count of
    /// sessions: it is the same track, asked again.
    /// </summary>
    void StartTrack(long atMs, bool sameTrack = false, bool? play = null)
    {
        var track = queue.Current;
        if (track == null) return;
        if (!sameTrack)
        {
            fallback = false;
            recreates = 0;
        }
        var play_ = play ?? (!sameTrack || error != null || wantPlay);
        var at = ++loads;
        yielded = false;
        StopTicker();
        EndSession();
        var o = Output();
        if (!sameTrack) o.Stop();
        ended = false;
        error = null;
        loading = true;
        playing = false;
        wantPlay = play_;
        Publish();
        Plays.When(scheduler, () => server.Start(track.Id, atMs, fallback), started =>
        {
            if (at != loads)
            {
                // Another track since: this one's session goes at once.
                started.End(atMs);
                return;
            }
            session = started;
            var info = started.Info;
            // Played or paused as the person wants it now: they may have pressed Play or Pause while it was asked for.
            o.Prepare(new EngineSource(server.Url(info.Url), info.Hls, StartMs: atMs, DurationHintMs: info.DurationMs > 0 ? info.DurationMs : null), wantPlay);
            StartTicker();
            Publish();
        }, e =>
        {
            if (at == loads) Fail(ErrorOf(e));
        });
    }

    long LastPosition() => Math.Max(0, output?.PositionMs ?? 0);

    /// <summary>How long the track has left, for the sleep timer; null while it isn't playing.</summary>
    long? TrackLeftMs()
    {
        if (session == null || ended || error != null) return null;
        var p = Position();
        return p.DurationMs > 0 ? p.DurationMs - p.PositionMs : null;
    }

    /// <summary>Ends the current track's session at the player's position.</summary>
    void EndSession()
    {
        var s = session;
        if (s == null) return;
        session = null;
        s.End(LastPosition());
    }

    void Fail(MusicError why)
    {
        loading = false;
        playing = false;
        error = why;
        StopTicker();
        Publish();
    }

    /// <summary>Progress every 10 s while it plays, every 30 s paused: the server keeps the session, and its addresses, as long as it hears from it.</summary>
    void StartTicker()
    {
        ticker?.Dispose();
        long quiet = 0;
        void Tick()
        {
            ticker = scheduler.After(ProgressEveryMs, () =>
            {
                var s = session;
                if (s == null) return;
                quiet += ProgressEveryMs;
                if (playing || quiet >= PausedEveryMs)
                {
                    quiet = 0;
                    s.Progress(LastPosition(), playing);
                }
                Tick();
            });
        }
        Tick();
    }

    void StopTicker()
    {
        ticker?.Dispose();
        ticker = null;
    }

    void OnPlaying(bool now)
    {
        playing = now;
        if (now)
        {
            loading = false;
            recreates = 0;
        }
        Publish();
    }

    void OnStatus(EngineStatus status)
    {
        switch (status)
        {
            case EngineStatus.Buffering when !loading:
                loading = true;
                Publish();
                break;
            case EngineStatus.Ready when loading:
                loading = false;
                Publish();
                break;
            case EngineStatus.Ended:
                OnEnded();
                break;
        }
    }

    void OnEnded()
    {
        var s = session;
        session = null;
        StopTicker();
        s?.End(s.Info.DurationMs > 0 ? s.Info.DurationMs : output?.DurationMs > 0 ? output!.DurationMs : LastPosition());
        // The sleep timer ends with this track: the next one waits, paused.
        var sleeping = timer.ItemEnded(queue.Current?.Id);
        if (queue.Next()) StartTrack(0, play: !sleeping);
        else
        {
            ended = true;
            playing = false;
            loading = false;
            Publish();
        }
    }

    /// <summary>
    /// The TV's player gave up. A 410 is the server stopping the session; a file it can't decode
    /// played as it is, once more as a conversion; a session gone (404) or the network, a new
    /// session where it was, three times in a row. Anything else, the server is asked whether the
    /// session goes on, which the TV's player can't say.
    /// </summary>
    void OnError(EngineError e)
    {
        var s = session;
        if (s == null) return;
        var direct = s.Info.Method == "direct";
        switch (e)
        {
            case EngineError.Stopped:
            case EngineError.Http { Status: 410 }:
                StoppedByServer(s.Id);
                return;
            case EngineError.Unsupported when direct && !fallback:
                Decode();
                return;
            case EngineError.Http { Status: 404 }:
            case EngineError.Network:
                Again(e is EngineError.Network ? MusicError.Offline : MusicError.Unplayable);
                return;
            case EngineError.Unsupported:
                EndSession();
                Fail(MusicError.Unplayable);
                return;
        }
        var at = loads;
        Plays.When(scheduler, () => server.Status(s.Id), () =>
        {
            if (at != loads) return;
            // The session goes on: the track is at fault.
            if (direct && !fallback) Decode();
            else Again(MusicError.Unplayable);
        }, err =>
        {
            if (at != loads) return;
            if (err is SessionStoppedException) StoppedByServer(s.Id);
            else if (err is ApiException { Status: 404 }) Again(MusicError.Unplayable);
            else if (Errors.Unreachable(err)) Again(MusicError.Offline);
            else Again(MusicError.Unplayable);
        });
    }

    /// <summary>Played as it is, the TV couldn't decode it: once more as a conversion.</summary>
    void Decode()
    {
        fallback = true;
        StartTrack(LastPosition(), sameTrack: true);
    }

    /// <summary>A new session where it was, up to three times in a row; then it fails as <paramref name="otherwise"/>.</summary>
    void Again(MusicError otherwise)
    {
        if (recreates < MaxRecreates)
        {
            recreates++;
            StartTrack(LastPosition(), sameTrack: true);
            return;
        }
        EndSession();
        Fail(otherwise);
    }

    void Publish()
    {
        var track = queue.Current;
        Queue.Value = queue.State;
        Now.Value = track == null ? null : new NowPlaying
        {
            ItemId = track.Id,
            Title = track.Title,
            Artist = track.GrandparentTitle,
            Playing = playing,
            Album = track.ParentTitle,
            AlbumId = track.ParentId,
            ArtistId = track.GrandparentId,
            ArtUrl = ArtPath(track) is { } path ? server.Url(path) : null,
            DurationMs = track.DurationMs ?? 0,
            Index = queue.Index,
            Count = queue.Count,
            Shuffle = queue.Shuffle,
            Loading = loading && error == null,
            Ended = ended,
            Error = error,
        };
    }

    /// <summary>The album art's path: the track's own picture, else its album's.</summary>
    public static string? ArtPath(ItemSummary track) =>
        !string.IsNullOrEmpty(track.PosterUrl) ? track.PosterUrl : track.ParentId is { } album ? $"/api/items/{album}/images/poster" : null;

    /// <summary>Why starting a track failed, as music shows it.</summary>
    public static MusicError ErrorOf(Exception e) => e switch
    {
        SessionStoppedException => MusicError.Stopped,
        SignedOutException or AuthException => MusicError.SignedOut,
        ApiException { Status: 401 } => MusicError.SignedOut,
        _ when Errors.Unreachable(e) => MusicError.Offline,
        _ => MusicError.Unplayable,
    };
}
