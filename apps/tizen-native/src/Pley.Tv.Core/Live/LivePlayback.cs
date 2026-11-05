using System;
using System.Collections.Generic;
using System.Text.RegularExpressions;
using System.Threading;
using System.Threading.Tasks;
using Pley.Tv.Core.Api;
using Pley.Tv.Core.Net;
using Pley.Tv.Core.Player;
using Pley.Tv.Core.Ui;

namespace Pley.Tv.Core.Live;

/// <summary>Why a channel doesn't play, by what the server answered (docs/API.md, "Watching").</summary>
public enum LiveTroubleKind
{
    /// <summary>409: every connection of the channel's source is in use.</summary>
    Busy,
    /// <summary>410: an administrator stopped the channel, someone took its connection over, or the provider ended it.</summary>
    Stopped,
    /// <summary>404: the channel was hidden or its source removed.</summary>
    Missing,
    /// <summary>403: Live TV isn't open to this profile.</summary>
    NotAllowed,
    /// <summary>502, 503 or 504: the provider sends nothing, or not in time.</summary>
    Unavailable,
    /// <summary>The server is out of reach.</summary>
    Offline,
    /// <summary>The server answered an address the TV's player can't open without the token: an older Pley.</summary>
    Older,
    Other,
}

/// <summary>
/// Why a channel doesn't play, with its own sentence (<see cref="Message"/>, the server's when it
/// gave one): a 409 lists the connections holding every one of the source's
/// (<see cref="Connections"/>: who watches each, and which may be taken over). The title is
/// <see cref="LiveText.TroubleTitle"/>.
/// </summary>
public sealed record LiveTrouble(LiveTroubleKind Kind, string Message, IReadOnlyList<BusyConnection>? Connections = null)
{
    public IReadOnlyList<BusyConnection> Busy => Connections ?? Array.Empty<BusyConnection>();

    /// <summary>Whether trying again may help: not for a channel that is gone, nor a server that can't send it to this TV.</summary>
    public bool CanRetry => Kind != LiveTroubleKind.Missing && Kind != LiveTroubleKind.Older;

    /// <summary>
    /// Watching ended on the server (its connection taken over, stopped there, or let go after the
    /// TV asked for nothing for a while): the server's own words, as every app says them.
    /// </summary>
    public static LiveTrouble Stopped(string? why) =>
        new LiveTrouble(LiveTroubleKind.Stopped, string.IsNullOrWhiteSpace(why) ? LiveText.StoppedBody : Errors.AsSentence(why!.Trim()));

    /// <summary>The trouble <paramref name="error"/> is: a refusal of the server's, a network failure, or else.</summary>
    public static LiveTrouble Of(Exception error) => error switch
    {
        // Not a video's "stopped by the administrator": the server's words for this channel.
        SessionStoppedException e => Stopped(e.Note),
        ApiException e => new LiveTrouble(KindOf(e.Status), Errors.Message(e), BusyConnection.Of(e)),
        _ when Errors.Unreachable(error) => new LiveTrouble(LiveTroubleKind.Offline, Errors.Message(error)),
        _ => new LiveTrouble(LiveTroubleKind.Other, Errors.Message(error)),
    };

    static LiveTroubleKind KindOf(int status) => status switch
    {
        409 => LiveTroubleKind.Busy,
        410 => LiveTroubleKind.Stopped,
        404 => LiveTroubleKind.Missing,
        403 => LiveTroubleKind.NotAllowed,
        502 or 503 or 504 => LiveTroubleKind.Unavailable,
        _ => LiveTroubleKind.Other,
    };
}

/// <summary>Where watching a channel is.</summary>
public abstract record LivePhase
{
    /// <summary>Nothing is tuned: before the first channel, or let go in the background.</summary>
    public sealed record Idle : LivePhase;

    /// <summary>Asking the server for the channel, then for its first segments.</summary>
    public sealed record Tuning : LivePhase;

    /// <summary>The channel's picture (or sound) is out.</summary>
    public sealed record Playing : LivePhase;

    public sealed record Failed(LiveTrouble Trouble) : LivePhase;
}

/// <summary>What a live player shows. Live TV is watched as it airs: no pausing, no going back.</summary>
/// <param name="Buffering">Waiting for data while playing, for 0.7 s or more: the spinner shows.</param>
/// <param name="AudioOnly">The channel has sound only (radio): no picture comes.</param>
public sealed record LiveState(long? ChannelId = null, LivePhase? Phase = null, bool Buffering = false, bool AudioOnly = false)
{
    public LivePhase Is => Phase ?? new LivePhase.Idle();

    public bool Playing => Is is LivePhase.Playing;

    public LiveTrouble? Trouble => (Is as LivePhase.Failed)?.Trouble;
}

/// <summary>
/// Watching Live TV on this TV (the Android app's LivePlayback, as packages/tv's LiveController
/// does it on a Samsung TV):
///
/// - <see cref="Tune"/> asks the server for the channel with this TV's client id, which ends what
///   it watched before (zapping never holds two of the provider's connections), and a keyed
///   playlist address, since the TV's player can't send the token; the session before is let go.
/// - As Android's, the channel's <c>stream.ts</c> (MPEG-TS as the provider sends it) is read at
///   once, under an id of its own, while the server is asked what the channel is made into: sent
///   as it is, or with sound the TV decodes itself (<see cref="LiveRules.PlaysAsSent"/>), that plays
///   from its first keyframe rather than after the first HLS segments. Its address takes the
///   token, which the TV's player can't send, so this app reads it and hands it on
///   (<see cref="ILiveServer.Stream"/>). It is asked for with the channel, not after: the server
///   tunes for whichever comes first.
/// - Otherwise, or when it fails, the HLS: its playlist is asked for once first, as the server
///   waits for the channel's first segments before it answers and the TV's player gives up
///   sooner. Then the TV's player plays it from the live edge.
/// - It is watched as it airs: no pausing, no going back.
/// - The TV's player says nothing of a playlist that has gone: a stall of 5 s asks the server
///   whether watching ended (and why), one of 20 s is a failure. A failure asks the same, then
///   tunes again twice (after 1 s, then 3 s) before it says why it doesn't play. The stream.ts
///   failing is watched again as HLS, as Android's: before it played, at once; while it played,
///   once the server has said watching didn't end there.
/// - Someone taking over the channel's connection (the server's <c>live_taken_over</c>, sent
///   before the stream breaks off) stops it at once with the server's words; it isn't tuned again
///   by itself. The session stopped from elsewhere (<c>session_stopped</c>: an administrator, or
///   the profile on another device) stops it the same way, saying who and why.
/// - The profile's remote, on another device, changes channel: its next and previous are channel
///   up and down, in the order the player goes through (<see cref="Zap"/>), which a player
///   declares only when it has one.
/// - <see cref="Stop"/> lets the session and the player go (the app went to the background),
///   <see cref="Resume"/> tunes the channel again (not one that stopped on the server: its card
///   stays), <see cref="Close"/> ends it all.
///
/// Made and used on the UI thread.
/// </summary>
public sealed class LivePlayback
{
    /// <summary>The server waits for the channel's first segments before it answers the playlist.</summary>
    public const int PlaylistWaitMs = 45_000;

    /// <summary>No picture this long after the playlist came: the channel didn't start.</summary>
    public const long StartWaitMs = 30_000;

    /// <summary>Tuning again after the player fails, this many times: a second, then three later.</summary>
    public const int Retries = 2;

    /// <summary>The player is asked whether there is a picture this long after it starts.</summary>
    public const long PictureCheckMs = 1_500;

    /// <summary>A stall shows the spinner once it has lasted this long (the web's 0.7 s).</summary>
    public const long StallShownMs = 700;

    /// <summary>A stall this long and the server is asked whether watching ended there.</summary>
    public const long StallCheckMs = 5_000;

    /// <summary>A stall this long: tuned anew, as a failure.</summary>
    public const long StallGiveUpMs = 20_000;

    /// <summary><c>stream.ts</c> is read under this TV's id with this after it (Android's): the HLS session stays.</summary>
    public const string StreamClient = "-stream";

    /// <summary>
    /// The stream.ts not playing this long after it was given: watched as HLS. Android's player
    /// says when it can't; the TV's may wait on.
    /// </summary>
    public const long DirectWaitMs = 10_000;

    /// <summary>A keyed live address: anything else needs the token.</summary>
    static readonly Regex Keyed = new Regex(@"/api/live/media/", RegexOptions.CultureInvariant);

    readonly ILiveServer server;
    readonly Func<IMediaEngine> engines;
    readonly IScheduler scheduler;
    IMediaEngine? engine;
    string? session;
    CancellationTokenSource? cancel;
    int generation;
    int retries;
    bool shown, closed;
    /// <summary>What plays is the channel's stream.ts: failing, it is watched again as HLS.</summary>
    bool direct;
    IDisposable? startTimer, pictureTimer, retryTimer, stallShown, stallCheck;
    readonly IDisposable events;

    public LivePlayback(ILiveServer server, Func<IMediaEngine> engines, IScheduler scheduler)
    {
        this.server = server;
        this.engines = engines;
        this.scheduler = scheduler;
        events = server.OnEvent(OnEvent);
    }

    /// <summary>
    /// Channel up (1) or down (-1), in the order the player goes through them: the remote's next
    /// and previous do the same. Null, as long as the player has no order, declares neither.
    /// </summary>
    public Action<int>? Zap { get; set; }

    public Signal<LiveState> State { get; } = new Signal<LiveState>(new LiveState());

    /// <summary>The player playing now; null before the first tune and after <see cref="Stop"/>.</summary>
    public IMediaEngine? Engine => engine;

    /// <summary>Starts watching <paramref name="channelId"/> in place of what played.</summary>
    public void Tune(long channelId)
    {
        retries = 0;
        Start(channelId, null);
    }

    /// <summary>
    /// Tunes the channel again: after a failure, or back from the background.
    /// <paramref name="takeOver"/>: every connection being in use, the channel whose connection to
    /// take (one <see cref="LiveTrouble.Busy"/> said may be).
    /// </summary>
    public void Retry(long? takeOver = null)
    {
        retries = 0;
        if (State.Value.ChannelId is { } id) Start(id, takeOver);
    }

    /// <summary>
    /// Back in front: the channel left in <see cref="Stop"/> plays again, from the live edge,
    /// unless it stopped on the server: its card stays until Watch again.
    /// </summary>
    public void Resume()
    {
        if (State.Value.Is is LivePhase.Idle) Retry();
    }

    /// <summary>
    /// Lets the session and the player go, keeping which channel it was: the app is in the
    /// background. Nothing plays and no connection is held until <see cref="Resume"/>. A channel
    /// that stopped on the server (taken over, say) keeps its card, as Android's.
    /// </summary>
    public void Stop()
    {
        generation++;
        cancel?.Cancel();
        ClearTimers();
        direct = false;
        server.EndStream();
        Leave();
        if (engine != null)
        {
            Unlisten(engine);
            engine.Release();
            engine = null;
        }
        var stopped = State.Value.Trouble is { Kind: LiveTroubleKind.Stopped };
        State.Value = new LiveState(State.Value.ChannelId, stopped ? State.Value.Phase : null);
    }

    /// <summary>Ends watching for good: the server is told at once.</summary>
    public void Close()
    {
        Stop();
        closed = true;
        events.Dispose();
    }

    void OnEvent(ServerEvent e)
    {
        if (closed) return;
        switch (e.Type)
        {
            case "live_taken_over" when e.ChannelId is { } channel:
                TakenOver(channel, e.Message);
                break;
            case "session_stopped" when session != null && e.SessionId == session:
                StoppedElsewhere(e.Message, e.ByAdmin ?? true);
                break;
            case "player_command":
                Obey(e);
                break;
        }
    }

    /// <summary>
    /// The server stopped this TV's session (an administrator, or the profile on another device):
    /// it stops now and says so, as for a connection taken over. Watching ended at the server:
    /// nothing to leave.
    /// </summary>
    void StoppedElsewhere(string? message, bool byAdmin)
    {
        session = null;
        Fail(new LiveTrouble(LiveTroubleKind.Stopped, LiveText.StoppedBy(message, byAdmin)));
    }

    /// <summary>The remote's next and previous: channel up and down. The new channel's watch tells the server, and so the remote.</summary>
    void Obey(ServerEvent command)
    {
        var action = Remote.Action(new RemotePlayer(RemoteKind.Live, session, HasOrder: Zap != null), command);
        if (action.Act == RemoteAct.Channel) Zap?.Invoke(action.By);
    }

    /// <summary>
    /// Someone took over the connection of <paramref name="channelId"/>: the channel playing (or
    /// tuning) stops at once, with the server's words, and isn't tuned again by itself. The event
    /// names the channel, not the session (a stream's reader never learns its own), and the server
    /// ends every viewer of it, each with an event of its own: the first says so, the others change
    /// nothing, unless a 410 without words said so first. Let go in the background, nothing is held.
    /// </summary>
    void TakenOver(long channelId, string? message)
    {
        if (closed || State.Value.ChannelId != channelId) return;
        var trouble = LiveTrouble.Stopped(message);
        switch (State.Value.Is)
        {
            case LivePhase.Idle:
                return;
            case LivePhase.Failed failed:
                // Only a card without the server's words takes these: another stop's words stay.
                if (failed.Trouble.Kind == LiveTroubleKind.Stopped && failed.Trouble.Message == LiveText.StoppedBody && !string.IsNullOrWhiteSpace(message))
                    State.Value = State.Value with { Phase = new LivePhase.Failed(trouble) };
                return;
        }
        // Its session is gone on the server: nothing to leave.
        session = null;
        Fail(trouble);
    }

    void Start(long channelId, long? takeOver, bool allowDirect = true)
    {
        if (closed) return;
        var mine = ++generation;
        ClearTimers();
        cancel?.Cancel();
        var token = (cancel = new CancellationTokenSource()).Token;
        engine?.Stop();
        server.EndStream();
        Leave();
        shown = false;
        direct = false;
        State.Value = new LiveState(channelId, new LivePhase.Tuning());
        // As Android's: stream.ts is read under an id of its own (so it doesn't end the HLS session
        // the plan is asked of), and played once the plan says this TV plays the channel as it is.
        // Asked for with the channel, as the server tunes for whichever comes first; after it when a
        // connection is taken over, as only the watch takes it.
        var stream = allowDirect && takeOver == null ? server.Stream(channelId, server.ClientId + StreamClient) : null;
        var controls = Remote.LiveControls(Zap != null);
        Plays.When(scheduler, () => server.Watch(channelId, takeOver, controls, token), watching =>
        {
            if (mine != generation)
            {
                server.Leave(watching.SessionId);
                return;
            }
            session = watching.SessionId;
            if (Marks.On) Marks.Note("live: watching answered");
            if (!Keyed.IsMatch(watching.Url))
            {
                Fail(new LiveTrouble(LiveTroubleKind.Older, LiveText.OlderBody));
                return;
            }
            var playlist = server.Url(watching.Url);
            if (allowDirect && takeOver != null) stream = server.Stream(channelId, server.ClientId + StreamClient);
            if (stream == null)
            {
                Hls(playlist, mine, token);
                return;
            }
            direct = true;
            Open().Prepare(new EngineSource(stream, Hls: false, Live: true), play: false);
            startTimer = scheduler.After(DirectWaitMs, () =>
            {
                if (mine == generation && !shown) Failed(new EngineError.Other("its stream didn't start in time"));
            });
            void Planned(LivePlan? plan)
            {
                if (mine != generation) return;
                if (Marks.On) Marks.Note(plan == null ? "live: the plan isn't known" : $"live: the plan, video {plan.Video} ({plan.VideoCodec} {plan.Width}x{plan.Height} {plan.FrameRate}{(plan.Interlaced ? " interlaced" : "")}), audio {plan.Audio} ({plan.AudioCodec} {plan.AudioChannels} ch)");
                if (plan != null && LiveRules.PlaysAsSent(plan))
                {
                    engine?.Play();
                    return;
                }
                // Made into something else, or not known in time: its HLS.
                direct = false;
                startTimer?.Dispose();
                startTimer = null;
                server.EndStream();
                Hls(playlist, mine, token);
            }
            Plays.When(scheduler, () => server.Plan(watching.SessionId), Planned, _ => Planned(null));
        }, e =>
        {
            if (mine == generation && !Cancelled(e)) Fail(LiveTrouble.Of(e));
        });
    }

    /// <summary>The channel's HLS at <paramref name="playlist"/>, from the live edge, once its first segments are in.</summary>
    void Hls(string playlist, int mine, CancellationToken token)
    {
        // The channel's first segments, before the player asks: a TV's player gives up sooner than the server takes to tune.
        Plays.When(scheduler, () => server.Prefetch(playlist, PlaylistWaitMs, token), () =>
        {
            if (mine != generation) return;
            Open().Prepare(new EngineSource(playlist, Hls: true, Live: true), play: true);
            startTimer = scheduler.After(StartWaitMs, () =>
            {
                if (mine == generation && !shown) Failed(new EngineError.Other(LiveText.StartedLate));
            });
        }, e =>
        {
            if (mine != generation || Cancelled(e)) return;
            if (e is not ApiException { Status: 410 } || session is not { } asked)
            {
                Fail(LiveTrouble.Of(e));
                return;
            }
            // Ended before it played (taken over, say): a keyed address only says the session has
            // gone, its own address says why.
            var keyed = (e as SessionStoppedException)?.Note;
            Plays.When(scheduler, () => server.WhyEnded(asked), why => Ended(why), _ => Ended(null));

            void Ended(string? why)
            {
                if (mine != generation || closed) return;
                session = null;
                Fail(LiveTrouble.Stopped(string.IsNullOrWhiteSpace(why) ? keyed : why));
            }
        });
    }

    /// <summary>
    /// The stream.ts failed or ended (Android's onPlayerError and brokeOff). Before it played, the
    /// channel is watched again as HLS at once. While it played, the server is asked first whether
    /// watching ended there (its connection taken over, the channel stopped); if not, it broke off,
    /// and is watched again as HLS.
    /// </summary>
    void DirectFailed()
    {
        if (closed || State.Value.ChannelId is not { } channelId) return;
        var played = State.Value.Playing;
        direct = false;
        server.EndStream();
        if (!played)
        {
            Start(channelId, null, allowDirect: false);
            return;
        }
        var mine = ++generation;
        ClearTimers();
        engine?.Stop();
        State.Value = State.Value with { Phase = new LivePhase.Tuning(), Buffering = false };
        var asked = session;
        Plays.When(scheduler, () => asked != null ? server.WhyEnded(asked) : Task.FromResult<string?>(null), why => After(why), _ => After(null));

        void After(string? why)
        {
            if (mine != generation || closed) return;
            if (why != null)
            {
                // Already gone at the server: nothing to leave.
                session = null;
                Fail(LiveTrouble.Stopped(why));
                return;
            }
            Start(channelId, null, allowDirect: false);
        }
    }

    IMediaEngine Open()
    {
        if (engine != null) return engine;
        engine = engines();
        engine.StatusChanged += OnStatus;
        engine.Started += OnStarted;
        engine.Failed += Failed;
        return engine;
    }

    void Unlisten(IMediaEngine e)
    {
        e.StatusChanged -= OnStatus;
        e.Started -= OnStarted;
        e.Failed -= Failed;
    }

    void OnStatus(EngineStatus status)
    {
        // A stream.ts that ends before it plays: its HLS now, not after the wait.
        if (status == EngineStatus.Ended && direct && !State.Value.Playing)
        {
            Failed(new EngineError.Other("its stream ended"));
            return;
        }
        if (!State.Value.Playing) return;
        switch (status)
        {
            case EngineStatus.Buffering:
                WatchStall();
                break;
            case EngineStatus.Ready:
                ClearStall();
                if (State.Value.Buffering) State.Value = State.Value with { Buffering = false };
                break;
            case EngineStatus.Ended:
                Failed(new EngineError.Other(LiveText.StreamEnded));
                break;
        }
    }

    void OnStarted()
    {
        startTimer?.Dispose();
        startTimer = null;
        shown = true;
        if (State.Value.Is is LivePhase.Failed) return;
        State.Value = State.Value with { Phase = new LivePhase.Playing(), Buffering = false };
        var mine = generation;
        pictureTimer?.Dispose();
        pictureTimer = scheduler.After(PictureCheckMs, () =>
        {
            pictureTimer = null;
            if (mine == generation && engine != null) State.Value = State.Value with { AudioOnly = engine.HasVideo == false };
        });
    }

    /// <summary>A stall: the spinner once it lasts, the server asked at 5 s whether watching ended, a failure at 20 s.</summary>
    void WatchStall()
    {
        if (stallShown != null || stallCheck != null) return;
        var mine = generation;
        bool Stalled() => mine == generation && State.Value.Playing;
        stallShown = scheduler.After(StallShownMs, () =>
        {
            if (Stalled()) State.Value = State.Value with { Buffering = true };
        });
        stallCheck = scheduler.After(StallCheckMs, () =>
        {
            stallCheck = scheduler.After(StallGiveUpMs - StallCheckMs, () =>
            {
                stallCheck = null;
                if (Stalled()) Failed(new EngineError.Other(LiveText.Stalled));
            });
            var asked = session;
            // Not of a stream.ts, as Android asks nothing of a stall: its HLS session, unread, goes
            // after 45 s, and would say watching ended when it hadn't.
            if (!Stalled() || asked == null || direct) return;
            Plays.When(scheduler, () => server.WhyEnded(asked), why =>
            {
                if (why == null || !Stalled()) return;
                session = null;
                Fail(LiveTrouble.Stopped(why));
            }, _ => { });
        });
    }

    void ClearStall()
    {
        stallShown?.Dispose();
        stallCheck?.Dispose();
        stallShown = stallCheck = null;
    }

    /// <summary>
    /// The TV's player failed: the server is asked first whether watching ended there (someone
    /// took the connection over, or it was stopped), which the player can't say; then the channel
    /// is tuned anew, twice, the first time after a second: a take-over's event, on its way as the
    /// stream breaks off, stops it before it asks for a connection that is gone.
    /// </summary>
    void Failed(EngineError error)
    {
        // Stopped already (watching ended on the server, or it failed): the player's last words change nothing.
        if (closed || State.Value.ChannelId is not { } channelId || State.Value.Is is LivePhase.Failed) return;
        if (direct)
        {
            DirectFailed();
            return;
        }
        var mine = ++generation;
        ClearTimers();
        engine?.Stop();
        State.Value = State.Value with { Phase = new LivePhase.Tuning(), Buffering = false };
        var asked = session;
        Plays.When(scheduler, () => asked != null ? server.WhyEnded(asked) : Task.FromResult<string?>(null), why => After(why), _ => After(null));

        void After(string? why)
        {
            if (mine != generation || closed) return;
            var stopped = error is EngineError.Stopped || error is EngineError.Http { Status: 410 };
            if (why != null || stopped)
            {
                // Its session is gone on the server: nothing to leave.
                if (why != null) session = null;
                var said = !string.IsNullOrWhiteSpace(why) ? why : (error as EngineError.Stopped)?.Message;
                Fail(LiveTrouble.Stopped(said));
                return;
            }
            if (retries < Retries)
            {
                // Tuned anew: the server may have let the channel go, or the network blinked.
                retries++;
                var again = ++generation;
                retryTimer = scheduler.After(retries == 1 ? 1_000 : 3_000, () =>
                {
                    retryTimer = null;
                    if (again == generation) Start(channelId, null);
                });
                return;
            }
            Fail(new LiveTrouble(LiveTroubleKind.Other, error.Detail.Length > 0 ? LiveText.PlayerSaid(error.Detail) : ""));
        }
    }

    void Fail(LiveTrouble trouble)
    {
        generation++;
        ClearTimers();
        direct = false;
        engine?.Stop();
        server.EndStream();
        Leave();
        State.Value = State.Value with { Phase = new LivePhase.Failed(trouble), Buffering = false };
    }

    void Leave()
    {
        var s = session;
        session = null;
        if (s != null) server.Leave(s);
    }

    void ClearTimers()
    {
        startTimer?.Dispose();
        pictureTimer?.Dispose();
        retryTimer?.Dispose();
        startTimer = pictureTimer = retryTimer = null;
        ClearStall();
    }

    static bool Cancelled(Exception e) => e is NetworkException { Cancelled: true } or OperationCanceledException;
}
