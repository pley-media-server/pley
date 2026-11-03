using System;
using System.Collections.Generic;
using System.Linq;
using System.Threading.Tasks;
using Pley.Tv.Core.Api;
using Pley.Tv.Core.Auth;
using Pley.Tv.Core.Net;
using Pley.Tv.Core.Subtitles.Pictures;
using Pley.Tv.Core.Ui;

namespace Pley.Tv.Core.Player;

/// <summary>
/// The playback of one video, then of what plays after it, on one engine: the Android app's
/// SessionPlaybackController as the web TV app's PlaybackController ports it to a TV's own player.
/// <list type="bullet">
/// <item><b>Start.</b> The item's details and the TV's report come together, then POST
/// /api/playback (<see cref="PlaybackPlan"/>), the Dolby Vision guards, and the engine loads the
/// answer at the resume point.</item>
/// <item><b>Reports.</b> Progress every 10 s while playing and every 30 s paused, on pause, and
/// 1 s after the last seek; "stopped" then DELETE at the end, on closing and before any new session.</item>
/// <item><b>Recovery.</b> <see cref="Recovery"/>'s ladder for errors, 12 s stalls and dropped frames;
/// the server out of reach mid-playback is waited for ("Reconnecting…") rather than failed at once.</item>
/// <item><b>Tracks.</b> Audio switched by the engine in direct play, a new session for streams.
/// Text subtitles are always drawn by the app from the server's WebVTT (<see cref="Cues"/>);
/// pictures (PGS, VobSub, DVB) from the server's .mks of them (<see cref="Picture"/>), burned in
/// by the server only when it can't make that file. Choices are saved with PUT
/// /api/items/{id}/streams, the audio only when picked.</item>
/// <item><b>Markers, Up Next, the sleep timer, speed</b>, and a session the server stopped: it says
/// so and plays no more.</item>
/// </list>
/// Automatic plays the original: the steps down and up of the Android app's AutoQuality aren't
/// ported, and a slow server is offered a lower quality instead. The UI thread only; the engine's
/// events come there too.
/// </summary>
public sealed class PlayerModel : IDisposable
{
    public const long TickMs = 500;
    public const long ProgressMs = 10_000;
    /// <summary>While paused: the server forgets players it hasn't heard from for 90 s.</summary>
    public const long PausedHeartbeatMs = 30_000;
    public const long SeekReportMs = 1_000;
    /// <summary>How long playback may stand still, wanting to play, before it counts as stalled.</summary>
    public const long StallMs = 12_000;
    /// <summary>An end at least this far before the server's duration…</summary>
    public const long ShortEndMs = 60_000;
    /// <summary>…that comes less than this after where playback started is media that falls short, not the end.</summary>
    public const long ShortPlayMs = 5_000;
    /// <summary>Less than this buffered ahead while waiting: starving for data.</summary>
    public const long StarvingMs = 1_000;
    public const long StruggleSampleMs = 2_000;
    /// <summary>Frames dropped while the TV's player switches a file's audio track aren't the video struggling.</summary>
    public const long TrackSwitchQuietMs = 5_000;
    public const long DetailsMs = 2_000;
    /// <summary>The longest a new session waits for the one before to be ended.</summary>
    public const long EndWaitMs = 2_000;
    /// <summary>Seeks stop this short of the end, so the end isn't reached by a seek.</summary>
    public const long SeekEndMarginMs = 250;
    /// <summary>Buffering this soon after a seek is the seek's, not a stall.</summary>
    public const long SeekSettleMs = 2_000;
    /// <summary>The server out of reach mid-playback is asked again this long, as long as it waits for a player it doesn't hear from.</summary>
    public const long OutageMs = 90_000;
    /// <summary>How long each question to a server out of reach waits for an answer.</summary>
    public const int OutageAskMs = 5_000;
    /// <summary>The engine moving this far past where the server went means it came back by itself.</summary>
    public const long OutageMovedMs = 1_000;
    /// <summary>Between its questions: 0.5 s, 1 s, then every 2 s.</summary>
    static readonly long[] OutageStepsMs = { 500, 1_000, 2_000 };

    readonly IPlayerSource source;
    readonly Func<IVideoEngine> engineFactory;
    readonly IScheduler scheduler;
    readonly IReporter reporter;
    readonly SubtitleOffsets offsets;
    readonly SubtitlePresenter presenter;
    readonly SleepTimer sleep;
    readonly Recovery recovery = new Recovery();
    readonly StruggleMeter struggle = new StruggleMeter();
    readonly CreditsGate credits = new CreditsGate();
    /// <summary>Where the engine is, as its events said; Idle until it opens a source.</summary>
    EngineStatus engineStatus = EngineStatus.Idle;

    IVideoEngine? engine;
    PlayRequest current;
    ItemDetail? item;
    IReadOnlyDictionary<string, object?>? report;
    ClientCaps? caps;
    MediaFile? file;
    /// <summary>The audio playing; null while automatic, until the session says which track the server chose.</summary>
    MediaStream? audio;
    /// <summary>The person named the audio (the request, or the panel): saved choices carry it.</summary>
    bool audioPicked;
    MediaStream? subtitle;
    /// <summary>The quality chosen: the panel's or the remembered one, Automatic included.</summary>
    QualityPreset picked = Qualities.Automatic;
    /// <summary>What plays: the original or a transcode; the original while Automatic is chosen.</summary>
    QualityPreset quality = Qualities.Original;
    IReadOnlyList<Marker> markers = new Marker[0];
    /// <summary>The mode a Dolby Vision guard asked for, for this item's later sessions too.</summary>
    string? guardMode;
    /// <summary>The server has no .mks of picture subtitles (an older server or transcoder): they are burned in from then on.</summary>
    bool picturesUnavailable;

    PlaybackInfo? info;
    bool sessionEnded = true;
    /// <summary>Bumped by every new start: what an older one finds when it resumes is no longer its own.</summary>
    int generation;
    long lastPositionMs;
    long startedAtMs;
    bool mediaShown;
    long seekedAt = -1_000_000_000;
    bool wantPlay = true;
    bool ended;
    bool closed;
    /// <summary>The server stopped the playback: nothing plays again.</summary>
    bool stopped;
    long offset;
    /// <summary>Something has played: a lost server is waited for from now on, rather than failing at once.</summary>
    bool shownOnce;
    Outage? outage;
    TranscodeInfo? live;
    long lastAdvanceAt;
    long lastAdvancePos = -1;
    long lastStruggleSample;
    /// <summary>Dropped frames aren't counted before this (an audio track switched in place).</summary>
    long framesQuietUntil;
    /// <summary>The session's player has been ready once: its audio track is switched to then.</summary>
    bool engineReady;
    long sessionStartedAt;
    /// <summary>When the screen asked to play: the first frame is timed from it.</summary>
    long? pressedAt;

    IDisposable? tickTimer;
    IDisposable? progressTimer;
    IDisposable? seekReportTimer;
    IDisposable? countdownTimer;
    IDisposable? detailsTimer;
    IDisposable? trickplayTimer;
    IDisposable? unsubscribe;
    bool detailsOpen;

    ItemSummary? next;
    ItemDetail? nextDetail;
    /// <summary>Track choices saved so far: a next item's details asked for before them are asked for again.</summary>
    int choicesSaved;
    int nextLookup;

    /// <param name="engine">Makes the engine, once the item's details are in.</param>
    /// <param name="reporter">The app's reports queue, so a player that closes still gets its end out; one of its own without.</param>
    public PlayerModel(PlayRequest request, IPlayerSource source, Func<IVideoEngine> engine, IScheduler scheduler, IReporter? reporter = null)
    {
        current = request;
        this.source = source;
        engineFactory = engine;
        this.scheduler = scheduler;
        this.reporter = reporter ?? new SessionReporter();
        pressedAt = scheduler.Elapsed;
        offsets = new SubtitleOffsets(source.Storage, () => source.ProfileId);
        presenter = new SubtitlePresenter(scheduler, source.Subtitles, LivePositionMs, () => offset, () => State.Value.Speed, () => Notice(SessionText.SubtitlesFailed),
            source.PictureSubtitles, PicturesFailed, source.Local);
        sleep = new SleepTimer(scheduler, ItemLeftMs, () => item?.Id, OnSleep);
        sleep.State.Changed += s => Update(st => st with { Sleep = s });
        lastPositionMs = request.StartMs;
        Clock.Value = new PlayerClock(Math.Max(0, request.StartMs), 0, 0);
    }

    public Signal<PlayerState> State { get; } = new Signal<PlayerState>(new PlayerState());

    public Signal<PlayerClock> Clock { get; } = new Signal<PlayerClock>(new PlayerClock(0, 0, 0));

    /// <summary>The text subtitles showing now, at the playhead moved by the offset; empty when none. The screen draws them.</summary>
    public Signal<IReadOnlyList<Cue>> Cues => presenter.Shown;

    /// <summary>The picture subtitles showing now (PGS, VobSub, DVB), at the playhead moved by the offset; null when none. The screen draws them.</summary>
    public Signal<PictureFrame?> Picture => presenter.Picture;

    /// <summary>A message for the top right: the playback's notices, a transcode it chose, a slow server, the CAPTIONS key.</summary>
    public event Action<PlayerToast>? Toast;

    /// <summary>The engine, once the item's details came.</summary>
    public IVideoEngine? Engine => engine;

    /// <summary>Play or pause, from what the player means to do now.</summary>
    public bool WantsToPlay => wantPlay;

    /// <summary>The subtitles' look (Settings → Subtitles), which the Subtitles panel changes.</summary>
    public Settings Settings => source.Settings;

    /// <summary>
    /// Where the video is now, asked of the engine between the clock's ticks; the clock's position
    /// while the engine has nothing to say.
    /// </summary>
    public long LivePositionMs()
    {
        if (engine == null || !mediaShown || ended || closed || info == null || sessionEnded) return lastPositionMs;
        var pos = engine.PositionMs;
        return pos >= 0 ? pos : lastPositionMs;
    }

    // ---------------------------------------------------------------- start

    /// <summary>Starts preparing: called once, as the player shows.</summary>
    public void Start()
    {
        ArmTick();
        unsubscribe = source.OnEvent(e =>
        {
            if (e.Type == "session_stopped" && info != null && e.SessionId == info.SessionId) StopByServer(e.Message, e.ByAdmin ?? true);
        });
        _ = PrepareFirst();
    }

    async Task PrepareFirst()
    {
        var at = NextGeneration();
        try
        {
            var saved = source.Saved(current.ItemId);
            if (Marks.On) Marks.Note(saved != null ? "player: the item's details kept" : "player: asking for the item's details");
            var detail = saved ?? await Ui(() => source.Item(current.ItemId));
            if (at != generation || closed) return;
            if (engine == null)
            {
                engine = engineFactory();
                Attach(engine);
            }
            Update(s => s with { Speeds = engine.Speeds });
            report = source.Caps();
            caps = report != null ? ReadCaps(report) : null;
            Load(detail, current);
            await Begin(PlaybackPlan.StartMs(detail, current.StartMs, current.FromStart), at);
        }
        catch (Exception e)
        {
            if (at == generation) FailWith(e);
        }
    }

    static ClientCaps? ReadCaps(IReadOnlyDictionary<string, object?> report)
    {
        try
        {
            return ClientCaps.Of(report);
        }
        catch (Exception)
        {
            // A report the plan can't read: the server decides alone, as without one.
            return null;
        }
    }

    /// <summary>Takes <paramref name="detail"/> as the item playing, with <paramref name="request"/>'s choices.</summary>
    void Load(ItemDetail detail, PlayRequest request)
    {
        item = detail;
        current = request;
        var chosen = PlaybackPlan.File(detail, request.FileId, caps);
        file = chosen;
        audio = PlaybackPlan.InitialAudio(chosen, request.AudioIndex);
        audioPicked = audio != null;
        subtitle = PlaybackPlan.InitialSubtitle(chosen, request.SubtitleIndex);
        picked = Qualities.Choose(chosen, source.Settings.Quality);
        quality = picked.Automatic ? Qualities.Original : picked;
        offset = chosen != null ? offsets.Get(source.Server, chosen.Id) : 0;
        markers = Markers.ForFile(detail, chosen?.Id);
        Update(s => s with
        {
            ItemId = detail.Id,
            Kind = detail.Kind,
            Title = PlayerText.Title(detail),
            Subtitle = PlayerText.Line(detail),
            Markers = markers,
            Chapters = (chosen?.Chapters ?? new Chapter[0]).Select(c => new ChapterMark(c.Title, c.StartMs, c.EndMs, c.ThumbUrl != null ? source.Url(c.ThumbUrl) : null)).ToList(),
            Trickplay = null,
            SubtitleOffsetMs = offset,
        });
        SetClock(durationMs: chosen?.DurationMs ?? detail.DurationMs ?? 0);
        PublishChoices();
        LoadTrickplay(chosen);
    }

    // ---------------------------------------------------------------- sessions

    int NextGeneration() => ++generation;

    /// <summary>Starts a server session at <paramref name="atMs"/>, ending the one before; an earlier start still running loses.</summary>
    void Restart(long atMs)
    {
        if (stopped || closed) return;
        var at = NextGeneration();
        _ = RestartAt(atMs, at);
    }

    async Task RestartAt(long atMs, int at)
    {
        try
        {
            await Begin(atMs, at);
        }
        catch (Exception e)
        {
            if (at != generation) return;
            // Mid-playback, a server that doesn't answer is waited for.
            if (shownOnce && Errors.Unreachable(e))
            {
                lastPositionMs = atMs;
                ReachLost(engineFailed: true);
            }
            else FailWith(e);
        }
    }

    /// <summary>Starts a server session at <paramref name="atMs"/> and has the engine play it.</summary>
    async Task Begin(long atMs, int at)
    {
        var detail = item;
        if (detail == null) return;
        var chosen = file;
        if (chosen == null)
        {
            Fail(SessionText.NoFile, canRetry: false);
            return;
        }
        // The session before ends first ("stopped", then DELETE), unless the server is slow to say so.
        if (EndSession(atMs)) await Ui(() => Task.WhenAny(reporter.Flush(), Task.Delay(TimeSpan.FromMilliseconds(EndWaitMs))));
        if (at != generation || closed) return;
        Reset(atMs);

        // A lower quality is always a transcode; the original keeps a fallback's mode, or a guard's.
        var mode = !quality.Original ? "transcode" : recovery.FallbackMode ?? guardMode ?? quality.Mode;
        CapsDecision? decision = null;
        if (caps != null)
        {
            try
            {
                decision = CapsPolicy.Decide(chosen, caps);
            }
            catch (Exception)
            {
                // Not known: the server decides.
            }
        }
        // Automatic audio names no track: the server picks it again for each session.
        var wanted = audioPicked ? audio : null;
        // Picture subtitles this player can't draw (the server makes no .mks of them) are asked to be
        // burned in, as for a stream, wherever the file would play as it is.
        var streaming = PlaybackPlan.Streams(mode, decision, chosen, wanted, caps) || (PlaybackPlan.IsImage(subtitle) && !Sideload);
        var choices = new PlaybackPlan.Choices(chosen, atMs, wanted, subtitle, quality, mode, streaming, Sideload);
        var sent = Body(detail.Id, choices);
        if (Marks.On) Marks.Note("player: asking the server to play");
        var started = await Ui(() => source.StartPlayback(sent));
        if (Marks.On) Marks.Note("player: the server answered");
        if (at != generation || closed)
        {
            // Closed or replaced while the server made it: it never played.
            Discard(started);
            return;
        }
        // A file on a drive has no server to convert it: played as it is, the TV shows Dolby Vision
        // profile 7's HDR10 base layer, where asking again only refused it.
        if (!source.Local && DvGuards.RetryMode(started, chosen, caps) is string guarded)
        {
            Discard(started);
            guardMode = guarded;
            choices = choices with { Mode = guarded, Streaming = true };
            sent = Body(detail.Id, choices);
            started = await Ui(() => source.StartPlayback(sent));
        }
        if (at == generation && !closed && PlaybackPlan.NeedsIndexes(started, sent, choices))
        {
            Discard(started);
            choices = choices with { Streaming = true };
            sent = Body(detail.Id, choices);
            started = await Ui(() => source.StartPlayback(sent));
        }
        if (at != generation || closed)
        {
            Discard(started);
            return;
        }
        // A file on a drive plays at its own path.
        if (!source.Local && !IsKeyed(started.Url))
        {
            // The TV's player can't send the token: an address without its own key won't play.
            Discard(started);
            Fail("This server is too old to play on this TV: update it.", canRetry: false);
            return;
        }
        info = started;
        sessionEnded = false;
        live = null;
        // Automatic: the track the server chose plays, switched to by the engine in direct play.
        audio ??= PlaybackPlan.PlayedAudio(chosen, started.AudioIndex);
        Update(s => s with { Method = SessionText.Method(started), Note = NoteNow(), CanPlayOriginal = CanPlayOriginal() });
        if (started.DurationMs > 0) SetClock(durationMs: started.DurationMs);
        PublishChoices();
        ArmProgress();
        RefreshDetails(force: false);
        if (engine == null) return;
        var video = PlaybackPlan.VideoStream(chosen);
        var converted = started.Method == "transcode";
        var source_ = new EngineSource(
            source.Url(started.Url),
            started.Hls,
            StartMs: atMs,
            DurationHintMs: started.DurationMs > 0 ? started.DurationMs : chosen.DurationMs ?? detail.DurationMs,
            Mime: started.Hls ? null : PlaybackPlan.MimeTypeOf(chosen.Container),
            Video: video == null ? null : new EngineVideo(
                // A conversion keeps the frame rate; its codec and dynamic range are the server's to choose.
                converted ? "unknown" : video.Codec, video.Width, video.Height, video.FrameRate,
                converted ? "sdr" : CapsPolicy.RangeOf(video), converted ? null : video.DvProfile, video.BitDepth ?? 8),
            AudioCodec: started.AudioConverted ? null : audio?.Codec);
        // The engine says where it is by its events, which come after this returns.
        engineStatus = EngineStatus.Buffering;
        engineReady = false;
        engine.Prepare(source_, wantPlay);
        if (State.Value.Speed != 1) engine.SetSpeed(State.Value.Speed);
        ApplyTracks();
    }

    Dictionary<string, object?> Body(long itemId, PlaybackPlan.Choices choices) =>
        PlayRequests.Body(itemId, choices, source.ClientId, report ?? new Dictionary<string, object?>(), caps);

    /// <summary>Keyed playback addresses live under /api/media/ (docs/API.md, "Keyed URLs").</summary>
    internal static bool IsKeyed(string url)
    {
        var path = url;
        var scheme = url.IndexOf("://", StringComparison.Ordinal);
        if (scheme >= 0)
        {
            var slash = url.IndexOf('/', scheme + 3);
            path = slash >= 0 ? url.Substring(slash) : "/";
        }
        if (!path.StartsWith("/api/media/", StringComparison.Ordinal)) return false;
        var key = path.Substring("/api/media/".Length);
        var end = key.IndexOf('/');
        return end > 0;
    }

    /// <summary>Reports "stopped" at <paramref name="positionMs"/> and deletes the session playing; false when none plays.</summary>
    bool EndSession(long positionMs)
    {
        var s = info;
        if (s == null || sessionEnded) return false;
        sessionEnded = true;
        ClearProgress();
        var itemId = item?.Id ?? current.ItemId;
        var duration = s.DurationMs > 0 ? s.DurationMs : (long?)null;
        var position = Math.Max(0, positionMs);
        reporter.Send(() => source.Progress(itemId, position, duration, "stopped", s.SessionId));
        reporter.Send(() => source.StopPlayback(s.SessionId));
        return true;
    }

    /// <summary>Deletes a session that never played: no "stopped", which would move the resume point.</summary>
    void Discard(PlaybackInfo started) => reporter.Send(() => source.StopPlayback(started.SessionId));

    /// <summary>What is watched of a session starts over, for one at <paramref name="atMs"/>.</summary>
    void Reset(long atMs)
    {
        lastPositionMs = atMs;
        startedAtMs = atMs;
        mediaShown = false;
        ended = false;
        struggle.Reset();
        recovery.OnJump();
        lastAdvanceAt = scheduler.Elapsed;
        lastAdvancePos = -1;
        sessionStartedAt = scheduler.Elapsed;
        // Automatic audio is chosen again for each session: a stream has the track it copies.
        if (!audioPicked) audio = null;
        Update(s => s with { Phase = PlayerPhase.Preparing, SkipLabel = null, FirstFrameMs = null });
        SetClock(positionMs: atMs);
    }

    /// <summary>Picture subtitles are drawn here, as the report says, unless the server can't make them.</summary>
    bool Sideload => PlaybackPlan.SideloadsPictures(caps, !picturesUnavailable);

    /// <summary>
    /// The tracks chosen, where they play: the engine switches a file's audio in direct play; text
    /// subtitles come from the server's WebVTT and pictures from its .mks, drawn by the app; nothing
    /// is drawn for pictures the server burns in.
    /// </summary>
    void ApplyTracks()
    {
        var s = info;
        if (s == null || engine == null) return;
        ApplyAudio();
        var sub = subtitle;
        if (sub == null)
        {
            presenter.Hide();
            return;
        }
        if (!PlaybackPlan.IsImage(sub))
        {
            var path = s.Subtitles.FirstOrDefault(t => t.Index == sub.Index)?.Url ?? PlaybackPlan.TextSubtitlesPath(s.FileId, sub.Index);
            presenter.Show(source.Url(path));
        }
        else if (!s.BurnSubtitles && Sideload) presenter.ShowPictures(source.Url(PlaybackPlan.PictureSubtitlesPath(s.FileId, sub.Index)));
        else presenter.Hide();
    }

    /// <summary>
    /// The audio chosen, switched to on the TV's player once it is ready: asked before, the player
    /// said no, and every file played its first track whatever the profile's language.
    /// </summary>
    void ApplyAudio()
    {
        var s = info;
        if (s == null || engine == null || s.Hls || audio == null || !engineReady) return;
        var streams = PlaybackPlan.AudioStreams(file);
        // A track the TV's player leaves out (TrueHD) can't be switched to: it plays its own.
        if (PlaybackPlan.EngineAudioOrdinal(streams, audio, engine.AudioTracks().Count) is not int ordinal) return;
        // Another track than the one the file starts with: the switch may drop frames.
        if (engine.SelectAudio(ordinal, CheckedLanguage(audio)) && !audio.Default) QuietFrames();
    }

    /// <summary>The audio stream the TV's player plays, as it says; the one chosen when it doesn't, or the server's in a stream.</summary>
    MediaStream? PlayingAudio()
    {
        if (info is not { Hls: false } || engine == null || !engineReady) return audio;
        return PlaybackPlan.EngineAudioStream(PlaybackPlan.AudioStreams(file), engine.PlayingAudio(), engine.AudioTracks().Count) ?? audio;
    }

    /// <summary>
    /// The language the TV's track must have for <paramref name="track"/>: a server's file may list
    /// its tracks in another order than the TV's player reads them. A file on a drive is read in the
    /// player's own order, and its codes differ in form ("jpn", "ja"): none, or every switch failed
    /// and started the player again.
    /// </summary>
    string? CheckedLanguage(MediaStream track) => source.Local ? null : track.Language;

    /// <summary>The server can't make the .mks (an older server or transcoder): picture subtitles are burned in from now on.</summary>
    void PicturesFailed()
    {
        if (closed || stopped) return;
        picturesUnavailable = true;
        PublishChoices();
        if (PlaybackPlan.IsImage(subtitle) && info is { BurnSubtitles: false }) Restart(LivePositionMs());
    }

    /// <summary>An audio track switched in place: the frames it drops aren't counted, and counting starts afresh after.</summary>
    void QuietFrames()
    {
        framesQuietUntil = scheduler.Elapsed + TrackSwitchQuietMs;
        struggle.Reset();
    }

    // ---------------------------------------------------------------- engine events

    void Attach(IVideoEngine e)
    {
        e.StatusChanged += OnStatus;
        e.Started += OnFirstFrame;
        e.PlayingChanged += OnPlaying;
        e.Failed += OnError;
    }

    void Detach(IVideoEngine e)
    {
        e.StatusChanged -= OnStatus;
        e.Started -= OnFirstFrame;
        e.PlayingChanged -= OnPlaying;
        e.Failed -= OnError;
    }

    /// <summary>It plays again by itself (after waiting for data): the screen says so.</summary>
    void OnPlaying(bool playing)
    {
        if (playing && info != null && !closed && !ended && outage == null && engineStatus == EngineStatus.Ready) Phase(PlayerPhase.Playing);
    }

    void OnStatus(EngineStatus status)
    {
        engineStatus = status;
        if (info == null || closed) return;
        switch (status)
        {
            case EngineStatus.Buffering:
                if (ended) break;
                // A stall: it played, and no seek of its own asked it to wait.
                if (State.Value.Phase.Kind == PhaseKind.Playing && scheduler.Elapsed - seekedAt > SeekSettleMs) Update(s => s with { Stalls = s.Stalls + 1 });
                Phase(PlayerPhase.Buffering);
                break;
            case EngineStatus.Ready:
                mediaShown = true;
                shownOnce = true;
                if (!engineReady)
                {
                    engineReady = true;
                    ApplyAudio();
                }
                if (outage == null) Phase(engine?.Playing == true ? PlayerPhase.Playing : PlayerPhase.Paused);
                break;
            case EngineStatus.Ended:
                if (EndedShort()) OnEndedShort();
                else OnEnded();
                break;
        }
    }

    void OnFirstFrame()
    {
        if (closed || info == null) return;
        mediaShown = true;
        shownOnce = true;
        if (State.Value.FirstFrameMs == null)
        {
            // From the press for the first session; from the session's start after that.
            var from = pressedAt ?? sessionStartedAt;
            pressedAt = null;
            Update(s => s with { FirstFrameMs = scheduler.Elapsed - from });
        }
        presenter.Refresh();
    }

    void OnError(EngineError error)
    {
        var s = info;
        if (s == null || closed || stopped) return;
        if (error is EngineError.Stopped stop)
        {
            // A stream answers 410 once the server stopped it, also to players that didn't hear the event.
            StopByServer(stop.Message, byAdmin: true);
            return;
        }
        // A TV's player can't say why a request failed: the session's own answer tells a stop from the rest.
        if (error is EngineError.Http or EngineError.Network or EngineError.Playlist)
        {
            if (outage != null)
            {
                // Reconnecting already: the engine gave up too, so a new session starts once the server answers.
                outage.EngineFailed = true;
                return;
            }
            var session = s.SessionId;
            Plays.When(scheduler, () => source.PlaybackStatus(session, OutageAskMs), _ => Recover(error, s), e =>
            {
                if (info?.SessionId != session) return;
                if (e is SessionStoppedException stopped_) StopByServer(stopped_.Note, byAdmin: true);
                else if (Errors.Unreachable(e)) ReachLost(engineFailed: true);
                else if (e is ApiException { Status: 404 } gone) Recover(new EngineError.Http(404, gone.Message), s);
                else Recover(error, s);
            });
            return;
        }
        Recover(error, s);
    }

    void Recover(EngineError error, PlaybackInfo s)
    {
        if (info != s || closed || stopped) return;
        Act(recovery.Decide(error, s.Method, PlaybackPlan.VideoStream(file)?.Codec), starving: false);
    }

    /// <summary>Does what the recovery ladder says.</summary>
    void Act(RecoveryAction action, bool starving)
    {
        switch (action)
        {
            case RecoveryAction.NewSession n:
                if (n.Reason == FallbackReason.NotSmooth && recovery.StruggleSwitched && !picked.Automatic)
                {
                    Update(s => s with { CanPlayOriginal = true });
                    // As the phone's player does: a switch the player made by itself says why, and where the way back is.
                    Say(new PlayerToast(PlayerText.TranscodeToast(SessionText.Fallback(FallbackReason.NotSmooth)), PlayerText.ErrorToastMs));
                }
                Restart(LivePositionMs());
                break;
            case RecoveryAction.CheckServer:
                var s = info;
                if (s == null) return;
                Plays.When(scheduler, () => source.PlaybackStatus(s.SessionId, OutageAskMs), status =>
                {
                    if (info == s) Act(recovery.AfterServerCheck(status.Transcode?.Speed, starving), starving);
                }, e =>
                {
                    if (info != s) return;
                    // A stall with the server gone isn't the video's fault: wait for the server rather than convert.
                    if (Errors.Unreachable(e)) ReachLost(engineFailed: false);
                    else if (e is SessionStoppedException stop) StopByServer(stop.Note, byAdmin: true);
                    else Act(recovery.AfterServerCheck(null, starving), starving);
                });
                break;
            case RecoveryAction.SuggestLowerQuality:
                Update(st => st with { SuggestLowerQuality = true, Note = SessionText.SlowServer });
                Say(new PlayerToast(PlayerText.SlowServerToast(SessionText.SlowServer), PlayerText.ErrorToastMs, Error: true));
                break;
            case RecoveryAction.Fail f:
                Fail(SessionText.Failure(f.Failure, f.Detail), canRetry: f.Failure != Failure.Unplayable, title: f.Failure == Failure.Network ? PlayerText.OfflineTitle : PlayerText.ErrorTitle);
                break;
        }
    }

    /// <summary>
    /// The media ended moments after where it started, a minute or more before the end the server
    /// gives: the stream or file doesn't hold that part. That isn't the end of the video.
    /// </summary>
    bool EndedShort()
    {
        if (ended || engine == null) return false;
        var expected = info?.DurationMs > 0 ? info.DurationMs : file?.DurationMs ?? 0;
        if (expected <= 0) return false;
        var reached = engine.PositionMs;
        return reached + ShortEndMs < expected && reached < startedAtMs + ShortPlayMs;
    }

    /// <summary>Where it was to play from stays the playhead: nothing is reported at the media's end, nothing is marked watched.</summary>
    void OnEndedShort()
    {
        var s = info;
        if (s == null) return;
        mediaShown = false;
        lastPositionMs = startedAtMs;
        SetClock(positionMs: startedAtMs);
        Update(st => st with { SkipLabel = null });
        Act(recovery.OnEndedShort(s.Method), starving: false);
    }

    void OnEnded()
    {
        if (ended) return;
        ended = true;
        // The sleep timer ends with this item: what comes next waits for the person.
        var sleeping = sleep.ItemEnded(item?.Id);
        var engineDuration = engine?.DurationMs ?? 0;
        var duration = info?.DurationMs > 0 ? info.DurationMs : engineDuration > 0 ? engineDuration : lastPositionMs;
        lastPositionMs = duration;
        EndSession(duration);
        SetClock(positionMs: duration);
        Update(s => s with { SkipLabel = null });
        var detail = item;
        if (detail == null || !NextUp.HasNext(detail.Kind, current.Queue))
        {
            Phase(PlayerPhase.Ended);
            return;
        }
        Phase(PlayerPhase.Paused);
        if (next != null)
        {
            ShowUpNext(next, restartCountdown: !sleeping && countdownTimer == null);
            return;
        }
        LookUpNext(found =>
        {
            if (found == null) Phase(PlayerPhase.Ended);
            else ShowUpNext(found, restartCountdown: !sleeping);
        });
    }

    // ---------------------------------------------------------------- the clock

    void ArmTick()
    {
        tickTimer = scheduler.After(TickMs, () =>
        {
            if (closed) return;
            Tick();
            ArmTick();
        });
    }

    void Tick()
    {
        var e = engine;
        var s = info;
        if (e == null || s == null || closed || sessionEnded) return;
        var now = scheduler.Elapsed;
        var status = engineStatus;
        var moving = mediaShown && status == EngineStatus.Ready && e.Playing && !ended;
        if (mediaShown && !ended)
        {
            var pos = e.PositionMs;
            lastPositionMs = pos;
            if (moving) recovery.OnPosition(pos);
            var marker = State.Value.Phase.Kind == PhaseKind.Error ? null : Markers.Active(markers, pos);
            var engineDuration = e.DurationMs;
            SetClock(positionMs: pos, bufferedMs: e.BufferedMs, durationMs: s.DurationMs > 0 ? s.DurationMs : engineDuration > 0 ? engineDuration : Clock.Value.DurationMs);
            var label = marker != null ? Markers.SkipLabel(marker) : null;
            if (label != State.Value.SkipLabel) Update(st => st with { SkipLabel = label });
            if (credits.Entered(marker) != null) OnCredits();
        }
        // Reconnecting: the stall ladder and the frame counts wait; the engine moving on by itself ends it.
        if (outage != null)
        {
            if (moving && e.PositionMs > outage.AtMs + OutageMovedMs)
            {
                EndOutage();
                Phase(PlayerPhase.Playing);
            }
            return;
        }
        // A stall: meant to play, not moving.
        var stalling = wantPlay && !ended && State.Value.Phase.Kind != PhaseKind.Error && status != EngineStatus.Idle;
        var position = mediaShown ? e.PositionMs : -1;
        if (!stalling || position != lastAdvancePos)
        {
            lastAdvanceAt = now;
            lastAdvancePos = position;
        }
        else if (now - lastAdvanceAt >= StallMs)
        {
            lastAdvanceAt = now;
            var starving = status == EngineStatus.Buffering && e.BufferedMs - e.PositionMs < StarvingMs;
            OnStalled(s, starving);
            return;
        }
        // Dropped frames, sampled every 2 s of playback.
        if (moving && now >= framesQuietUntil && now - lastStruggleSample >= StruggleSampleMs)
        {
            lastStruggleSample = now;
            if (e.FrameCounts() is { } counts && struggle.Sample(now, counts))
            {
                struggle.Reset();
                // The original falls back to a conversion once.
                if (recovery.OnStruggling(s.Method) is { } action) Act(action, starving: false);
            }
        }
    }

    /// <summary>
    /// A 12 s stall. The TV's own player says nothing when the server goes, so the server is asked
    /// first: one that doesn't answer is waited for; else the recovery ladder decides.
    /// </summary>
    void OnStalled(PlaybackInfo s, bool starving)
    {
        // A file on a drive waits for no server, and the same file again would stand still again.
        if (source.Local)
        {
            Fail(SessionText.LocalStuck, canRetry: true);
            return;
        }
        Plays.When(scheduler, () => source.PlaybackStatus(s.SessionId, OutageAskMs), _ =>
        {
            if (info == s && outage == null && !closed) Act(recovery.OnStall(s.Method, starving), starving);
        }, e =>
        {
            if (info != s || outage != null || closed) return;
            if (Errors.Unreachable(e)) ReachLost(engineFailed: false);
            else if (e is SessionStoppedException stop) StopByServer(stop.Note, byAdmin: true);
            else Act(recovery.OnStall(s.Method, starving), starving);
        });
    }

    /// <summary>Progress every 10 s while playing and every 30 s paused, so the server keeps the session and its transcode.</summary>
    void ArmProgress()
    {
        ClearProgress();
        if (info == null || sessionEnded) return;
        progressTimer = scheduler.After(wantPlay ? ProgressMs : PausedHeartbeatMs, () =>
        {
            progressTimer = null;
            // Not while reconnecting: unanswered reports would queue up behind each other.
            if (!ended && outage == null && State.Value.Phase.Kind != PhaseKind.Error) Report(wantPlay ? "playing" : "paused");
            ArmProgress();
        });
    }

    void ClearProgress()
    {
        progressTimer?.Dispose();
        progressTimer = null;
    }

    void Report(string state)
    {
        var s = info;
        if (s == null || sessionEnded) return;
        var itemId = item?.Id ?? current.ItemId;
        var position = Math.Max(0, LivePositionMs());
        var duration = s.DurationMs > 0 ? s.DurationMs : (long?)null;
        reporter.Send(() => source.Progress(itemId, position, duration, state, s.SessionId));
    }

    /// <summary>How long the item playing has left, for the sleep timer; null while unknown.</summary>
    long? ItemLeftMs()
    {
        if (info == null || !mediaShown || ended) return null;
        var duration = Clock.Value.DurationMs;
        return duration > 0 ? duration - LivePositionMs() : null;
    }

    /// <summary>The sleep timer ended: it pauses, and what plays next doesn't start by itself.</summary>
    void OnSleep()
    {
        StopCountdown();
        if (State.Value.UpNext is { } card) Update(s => s with { UpNext = card with { SecondsLeft = null } });
        engine?.Pause();
        SetWantPlay(false);
    }

    /// <summary>
    /// The server ended the session: the engine stops, nothing more is asked of the server (only
    /// "stopped", for the resume point), and the screen says why, without Try again, then leaves.
    /// </summary>
    void StopByServer(string? message, bool byAdmin)
    {
        if (closed || stopped) return;
        stopped = true;
        NextGeneration();
        var position = LivePositionMs();
        lastPositionMs = position;
        var s = info;
        if (s != null && !sessionEnded)
        {
            sessionEnded = true;
            ClearProgress();
            var itemId = item?.Id ?? current.ItemId;
            var duration = s.DurationMs > 0 ? s.DurationMs : (long?)null;
            reporter.Send(() => source.Progress(itemId, Math.Max(0, position), duration, "stopped", s.SessionId));
        }
        EndOutage();
        seekReportTimer?.Dispose();
        nextLookup++;
        HideUpNext();
        presenter.Hide();
        sleep.Cancel();
        engine?.Stop();
        Update(st => st with { Phase = PlayerPhase.Error(SessionText.Stopped(message, byAdmin), canRetry: false, PlayerText.StoppedTitle), SkipLabel = null, StoppedByServer = true });
    }

    // ---------------------------------------------------------------- commands

    public void Play()
    {
        if (closed || stopped) return;
        if (ended)
        {
            Restart(0);
            return;
        }
        SetWantPlay(true);
        engine?.Play();
    }

    public void Pause()
    {
        if (closed) return;
        engine?.Pause();
        SetWantPlay(false);
    }

    void SetWantPlay(bool play)
    {
        var was = wantPlay;
        wantPlay = play;
        if (play) sleep.Resumed();
        if (info == null || was == play) return;
        if (!play)
        {
            struggle.Reset();
            if (!ended) Report("paused");
            if (engineStatus == EngineStatus.Ready) Phase(PlayerPhase.Paused);
        }
        else if (engineStatus == EngineStatus.Ready) Phase(PlayerPhase.Playing);
        ArmProgress();
    }

    /// <summary>Goes to <paramref name="positionMs"/>, stopping short of the end; after the end, a new session there.</summary>
    public void SeekTo(long positionMs)
    {
        var e = engine;
        if (e == null || closed || stopped) return;
        var duration = Clock.Value.DurationMs;
        var target = duration > 0 ? Math.Max(0, Math.Min(positionMs, Math.Max(0, duration - SeekEndMarginMs))) : Math.Max(0, positionMs);
        HideUpNext();
        if (ended || info == null || sessionEnded)
        {
            lastPositionMs = target;
            SetClock(positionMs: target);
            Restart(target);
            return;
        }
        e.SeekTo(target);
        seekedAt = scheduler.Elapsed;
        lastPositionMs = target;
        startedAtMs = target;
        recovery.OnJump();
        struggle.Reset();
        lastAdvanceAt = scheduler.Elapsed;
        SetClock(positionMs: target);
        presenter.Sought(target);
        seekReportTimer?.Dispose();
        seekReportTimer = scheduler.After(SeekReportMs, () =>
        {
            seekReportTimer = null;
            Report(wantPlay ? "playing" : "paused");
        });
    }

    /// <summary>The audio track <paramref name="index"/>: switched by the engine in direct play when it plays here, else a new session with it.</summary>
    public void SelectAudio(int index)
    {
        var chosen = PlaybackPlan.AudioStreams(file).FirstOrDefault(s => s.Index == index);
        if (chosen == null || chosen.Index == audio?.Index) return;
        if (Marks.On)
            Marks.Note($"audio: stream {chosen.Index} ({chosen.Codec}, {chosen.Channels} ch) chosen, {(PlaybackPlan.AudioPlaysHere(chosen, caps) ? "plays here" : "not this TV's")}; the TV passes through {string.Join(" ", caps?.AudioPassthrough ?? new string[0])}");
        audio = chosen;
        audioPicked = true;
        SaveStreams();
        PublishChoices();
        var s = info;
        if (s == null) return;
        if (!s.Hls && engine != null && engineReady && PlaybackPlan.AudioPlaysHere(chosen, caps)
            && PlaybackPlan.EngineAudioOrdinal(PlaybackPlan.AudioStreams(file), chosen, engine.AudioTracks().Count) is int ordinal
            && engine.SelectAudio(ordinal, CheckedLanguage(chosen)))
        {
            QuietFrames();
            return;
        }
        Restart(LivePositionMs());
    }

    /// <summary>The subtitle track <paramref name="index"/>, or none (null). Pictures the server burns in take a new session.</summary>
    public void SelectSubtitle(int? index)
    {
        MediaStream? chosen = null;
        if (index is int i)
        {
            chosen = PlaybackPlan.SubtitleStreams(file).FirstOrDefault(s => s.Index == i);
            if (chosen == null) return;
        }
        if (chosen?.Index == subtitle?.Index) return;
        subtitle = chosen;
        SaveStreams();
        PublishChoices();
        var s = info;
        if (s == null) return;
        var burn = PlaybackPlan.IsImage(chosen) && !Sideload;
        if (burn != s.BurnSubtitles || (burn && s.SubtitleIndex != chosen?.Index)) Restart(LivePositionMs());
        else ApplyTracks();
    }

    /// <summary>
    /// The CAPTIONS key: turns the subtitles off, remembering them, or back on (the ones turned off,
    /// else the first not burned in, else the first), saying so. Returns the subtitles to remember;
    /// <paramref name="remembered"/> is what it returned last.
    /// </summary>
    public int? ToggleCaptions(int? remembered)
    {
        var subs = State.Value.Subtitles;
        if (subs.Count == 0) return remembered;
        if (subs.FirstOrDefault(t => t.Selected && t.Index != null) is { } on)
        {
            SelectSubtitle(null);
            Say(new PlayerToast(PlayerText.SubtitlesOffToast, PlayerText.CaptionsToastMs));
            return on.Index;
        }
        var pick = subs.FirstOrDefault(t => t.Index != null && t.Index == remembered)
            ?? subs.FirstOrDefault(t => t.Index != null && !t.BurnIn)
            ?? subs.FirstOrDefault(t => t.Index != null);
        if (pick == null) return remembered;
        SelectSubtitle(pick.Index);
        Say(new PlayerToast(PlayerText.SubtitlesToast(pick.Label), PlayerText.CaptionsToastMs));
        return remembered;
    }

    /// <summary>Moves the subtitles: rounded to 0.1 s within ±10 s, kept for the file and the profile on this TV.</summary>
    public void SetSubtitleOffset(long ms)
    {
        if (closed) return;
        var moved = SubtitleSync.Clamp(ms);
        if (moved == offset) return;
        offset = moved;
        Update(s => s with { SubtitleOffsetMs = moved });
        if (file != null) offsets.Set(source.Server, file.Id, moved);
        presenter.Refresh();
    }

    /// <summary>Plays at <paramref name="rate"/> times normal speed, until the next title; only at the speeds the TV's player plays with sound.</summary>
    public void SetSpeed(double rate)
    {
        var e = engine;
        if (closed || stopped || e == null || !State.Value.Speeds.Contains(rate) || rate == State.Value.Speed) return;
        if (e.SetSpeed(rate)) Update(s => s with { Speed = rate });
    }

    public void SetSleep(SleepChoice choice)
    {
        if (closed || stopped) return;
        sleep.Set(choice);
    }

    /// <summary>The sleep timer's notice: Keep watching adds the same time again.</summary>
    public void KeepWatching()
    {
        if (closed || stopped) return;
        sleep.Extend();
        // What plays next was held for the timer: its countdown goes on.
        if (next != null && State.Value.UpNext != null && countdownTimer == null && !ended) ShowUpNext(next, restartCountdown: true);
    }

    /// <summary>
    /// A quality picked in the panel ("original", "auto", "8m"…), kept for the next titles. The
    /// session starts again only when it plays something else: Original and Automatic play the same.
    /// </summary>
    public void SelectQuality(string id)
    {
        var wanted = Qualities.ByIdOrLabel(id);
        var choice = Qualities.ForFile(file).FirstOrDefault(q => q.Id == wanted.Id);
        if (choice == null || choice.Id == picked.Id) return;
        var before = quality;
        picked = choice;
        quality = choice.Automatic ? Qualities.Original : choice;
        source.UpdateSettings(s => s with { Quality = Qualities.IdOf(choice) });
        var same = quality.Id == before.Id;
        if (!same) recovery.QualityChanged();
        Update(s => s with { SuggestLowerQuality = false, Note = same ? NoteNow() : null, CanPlayOriginal = same && CanPlayOriginal() });
        PublishChoices();
        RefreshDetails(force: false);
        if (item != null && !same) Restart(LivePositionMs());
    }

    /// <summary>The panel <paramref name="kind"/> as it stands now (<see cref="PlayerPanels.Build"/>); Info with the readout.</summary>
    public PlayerPanel Panel(PlayerPanelKind kind, IReadOnlyList<SubtitleFont>? fonts = null) =>
        PlayerPanels.Build(kind, State.Value, LivePositionMs(), source.Settings, kind == PlayerPanelKind.Info ? Readout() : null, fonts);

    /// <summary>
    /// OK on a panel's row: does what it says. Returns true when the panel stays open (the
    /// subtitles' look changes behind it; OK on Sync puts the subtitles back), false when it closes.
    /// </summary>
    public bool Choose(PlayerRow row)
    {
        switch (row.Action)
        {
            case PlayerRowAction.Subtitle s:
                SelectSubtitle(s.Index);
                return false;
            case PlayerRowAction.Audio a:
                SelectAudio(a.Index);
                return false;
            case PlayerRowAction.Quality q:
                SelectQuality(q.Id);
                return false;
            case PlayerRowAction.PlayOriginal:
                PlayOriginal();
                return false;
            case PlayerRowAction.Chapter c:
                SeekTo(c.StartMs);
                return false;
            case PlayerRowAction.Sleep s:
                SetSleep(s.Choice);
                return false;
            case PlayerRowAction.Speed s:
                SetSpeed(s.Rate);
                return false;
            case PlayerRowAction.SubtitleSync:
                SetSubtitleOffset(0);
                return true;
            case PlayerRowAction.SubtitleSize s:
                source.UpdateSettings(x => x with { SubtitleSize = s.Size });
                return true;
            case PlayerRowAction.SubtitleBackground b:
                source.UpdateSettings(x => x with { SubtitleBackground = b.Background });
                return true;
            case PlayerRowAction.SubtitleFont f:
                source.UpdateSettings(x => x with { SubtitleFont = f.Font });
                return true;
            case PlayerRowAction.SubtitleRaise r:
                source.UpdateSettings(x => x with { SubtitleRaise = r.Raise });
                return true;
            default:
                return false;
        }
    }

    /// <summary>Left (−1) or Right (+1) on a step row: Sync moves the subtitles 0.1 s earlier or later. Held, the key repeats it.</summary>
    public void Step(PlayerRow row, int direction)
    {
        if (row.Action is PlayerRowAction.SubtitleSync) SetSubtitleOffset(offset + Math.Sign(direction) * SubtitleSync.StepMs);
    }

    /// <summary>
    /// The Next button and key (the Android app's TvPlayerScreen): an episode plays what comes next;
    /// else the next chapter more than a second past <paramref name="positionMs"/> (the seek's target
    /// while one is coming).
    /// </summary>
    public void Next(long positionMs)
    {
        var s = State.Value;
        if (s.Subtitle != null) PlayNext();
        else if (PlayerText.NextChapterStart(s.Chapters, positionMs) is long start) SeekTo(start);
    }

    /// <summary>The Previous button and key: the chapter playing from its start when 3 s in, else the one before; outside chapters, the start.</summary>
    public void Previous(long positionMs) => SeekTo(PlayerText.PreviousTarget(State.Value.Chapters, positionMs));

    /// <summary>Skip Intro or Skip Credits, while one shows.</summary>
    public void Skip()
    {
        var marker = Markers.Active(markers, LivePositionMs());
        if (marker == null) return;
        var duration = Clock.Value.DurationMs;
        SeekTo(duration > 0 ? Math.Min(Markers.SkipTarget(marker), duration) : Markers.SkipTarget(marker));
    }

    /// <summary>What comes next, at once (Up Next's Play now, the Next button of an episode); says so when nothing does.</summary>
    public void PlayNext()
    {
        if (closed || stopped) return;
        if (next != null)
        {
            SwitchTo(next);
            return;
        }
        LookUpNext(found =>
        {
            if (found == null) Notice(current.Queue != null ? SessionText.LastInQueue : SessionText.LastEpisode);
            else SwitchTo(found);
        });
    }

    /// <summary>Up Next's Cancel: the card goes; at the end, playback ends.</summary>
    public void CancelUpNext()
    {
        HideUpNext();
        if (ended) Phase(PlayerPhase.Ended);
    }

    /// <summary>"Play original" after the switch for dropped frames: the quality's own mode again.</summary>
    public void PlayOriginal()
    {
        if (recovery.FallbackMode == null) return;
        recovery.PlayOriginal();
        Update(s => s with { CanPlayOriginal = false, Note = null });
        Restart(LivePositionMs());
    }

    /// <summary>
    /// The details are asked for while the Info panel is open, or Playback details show over the
    /// video: the session's live numbers every 2 s for a stream (<see cref="PlayerState.Details"/>).
    /// </summary>
    public void SetDetailsOpen(bool open)
    {
        detailsTimer?.Dispose();
        detailsTimer = null;
        detailsOpen = open;
        if (!open)
        {
            Update(s => s with { Details = null });
            return;
        }
        LoadDetails();
    }

    void LoadDetails()
    {
        if (!detailsOpen || closed) return;
        void Done()
        {
            if (!detailsOpen || closed) return;
            RefreshDetails(force: true);
            detailsTimer = scheduler.After(DetailsMs, LoadDetails);
        }
        var s = info;
        if (s == null || !s.Hls || sessionEnded)
        {
            Done();
            return;
        }
        Plays.When(scheduler, () => source.PlaybackStatus(s.SessionId, Transports.ReadTimeoutMs), status =>
        {
            if (info == s) live = status.Transcode;
            Done();
        }, _ => Done());
    }

    void RefreshDetails(bool force)
    {
        if (!force && State.Value.Details == null) return;
        var s = info;
        if (s == null) return;
        // What the TV plays, which isn't the track chosen when its player leaves that one out.
        var playing = PlayingAudio();
        var note = NoteNow() ?? (playing != null && audio != null && playing.Index != audio.Index ? SessionText.AudioNotOffered(Details.AudioLabel(audio), Details.AudioLabel(playing)) : null);
        var details = Details.Build(s, file, playing?.Index, note, LivePositionMs(), live, subtitle, Sideload);
        Update(st => st with { Details = details });
    }

    /// <summary>What the Info panel reads, once a second while it shows.</summary>
    public ReadoutInput Readout()
    {
        var clock = Clock.Value;
        var ahead = engine?.KnowsBuffer == true ? Math.Max(0, clock.BufferedMs - clock.PositionMs) : 0;
        EngineStats? stats = null;
        try
        {
            stats = engine?.Stats();
        }
        catch (Exception)
        {
            // Between states, the TV's player may refuse: no stats this time.
        }
        var s = State.Value;
        return new ReadoutInput(s.Details, stats, ahead, s.Quality, s.Stalls, s.FirstFrameMs);
    }

    /// <summary>The seek preview at <paramref name="positionMs"/>, once the file has them; null without.</summary>
    public TrickplayFrame? Preview(long positionMs) =>
        State.Value.Trickplay is { } set ? Player.Trickplay.At(set.Info, set.FileId, positionMs, source.TrickplaySheetUrl) : null;

    /// <summary>Try again, after an error: every recovery budget again, from where it was.</summary>
    public void Retry()
    {
        if (closed || stopped) return;
        EndOutage();
        recovery.Retry();
        Update(s => s with { Phase = PlayerPhase.Preparing });
        if (item == null) _ = PrepareFirst();
        else Restart(lastPositionMs);
    }

    /// <summary>The player closes: the session says where it stopped and ends, and the engine is let go.</summary>
    public void Close()
    {
        if (closed) return;
        var position = LivePositionMs();
        closed = true;
        NextGeneration();
        EndOutage();
        EndSession(position);
        sleep.Cancel();
        presenter.Hide();
        StopCountdown();
        foreach (var t in new[] { tickTimer, seekReportTimer, detailsTimer, trickplayTimer, progressTimer }) t?.Dispose();
        unsubscribe?.Dispose();
        if (engine != null) Detach(engine);
        engine?.Release();
        engine = null;
    }

    public void Dispose() => Close();

    // ---------------------------------------------------------------- Up Next

    void OnCredits()
    {
        var detail = item;
        if (detail == null || !NextUp.HasNext(detail.Kind, current.Queue) || State.Value.UpNext != null) return;
        LookUpNext(found =>
        {
            if (found != null && !closed) ShowUpNext(found, restartCountdown: true);
        });
    }

    /// <summary>Finds what plays next (one lookup at a time), then calls <paramref name="then"/> with it.</summary>
    void LookUpNext(Action<ItemSummary?> then)
    {
        var detail = item;
        if (detail == null)
        {
            then(null);
            return;
        }
        var lookup = ++nextLookup;
        var queue = current.Queue;
        Plays.When(scheduler, async () =>
        {
            if (queue != null) return NextUp.NextIn(await source.Queue(queue).ConfigureAwait(false), detail.Id);
            return detail.Kind == "episode" ? await source.Next(detail.Id).ConfigureAwait(false) : null;
        }, found =>
        {
            if (lookup != nextLookup || closed) return;
            if (found != null) next = found;
            then(found);
        }, _ =>
        {
            if (lookup == nextLookup && !closed) then(null);
        });
    }

    void ShowUpNext(ItemSummary found, bool restartCountdown)
    {
        next = found;
        // The next item's details ahead of time.
        if (nextDetail?.Id != found.Id)
        {
            var choices = choicesSaved;
            Plays.When(scheduler, () => source.Item(found.Id), d =>
            {
                // Tracks picked meanwhile change what it starts with: asked again then.
                if (choices == choicesSaved) nextDetail = d;
            }, _ => { });
        }
        var image = found.ThumbUrl ?? found.BackdropUrl;
        var card = new UpNextCard(found.Id, found.Title, SessionText.UpNext(NextUp.LabelOf(found)),
            image != null ? source.Url(Artwork.Sized(image, NextUp.ImageWidth)) : null, null);
        Update(s => s with { UpNext = card });
        if (!restartCountdown) return;
        StopCountdown();
        if (!source.Settings.AutoplayNext || sleep.EndsWith(item?.Id)) return;
        Countdown(found, NextUp.CountdownSeconds);
    }

    void Countdown(ItemSummary found, int left)
    {
        var card = State.Value.UpNext;
        if (card == null || card.ItemId != found.Id) return;
        if (left <= 0)
        {
            countdownTimer = null;
            SwitchTo(found);
            return;
        }
        Update(s => s with { UpNext = card with { SecondsLeft = left } });
        countdownTimer = scheduler.After(1_000, () => Countdown(found, left - 1));
    }

    void StopCountdown()
    {
        countdownTimer?.Dispose();
        countdownTimer = null;
    }

    void HideUpNext()
    {
        StopCountdown();
        if (State.Value.UpNext != null) Update(s => s with { UpNext = null });
    }

    /// <summary>Plays <paramref name="target"/> on the same engine: the session before ends, the next one starts where <paramref name="target"/> resumes.</summary>
    void SwitchTo(ItemSummary target)
    {
        HideUpNext();
        nextLookup++;
        var position = LivePositionMs();
        var endedBefore = EndSession(position);
        next = null;
        recovery.NewItem();
        credits.Reset();
        guardMode = null;
        presenter.Hide();
        ended = false;
        mediaShown = false;
        var request = current with
        {
            ItemId = target.Id,
            FileId = null,
            StartMs = NextUp.StartOf(target),
            FromStart = false,
            AudioIndex = null,
            SubtitleIndex = null,
        };
        current = request;
        // The speed is this title's: the next one plays at normal speed.
        if (State.Value.Speed != 1) engine?.SetSpeed(1);
        var was = State.Value;
        State.Value = new PlayerState { ItemId = target.Id, Title = was.Title, Sleep = was.Sleep, Speeds = was.Speeds };
        SetClock(positionMs: request.StartMs, bufferedMs: 0);
        var at = NextGeneration();
        var prefetched = nextDetail?.Id == target.Id ? nextDetail : null;
        _ = SwitchAt(request, prefetched, endedBefore, at);
    }

    async Task SwitchAt(PlayRequest request, ItemDetail? prefetched, bool endedBefore, int at)
    {
        try
        {
            if (endedBefore) await Ui(() => Task.WhenAny(reporter.Flush(), Task.Delay(TimeSpan.FromMilliseconds(EndWaitMs))));
            var detail = prefetched ?? await Ui(() => source.Item(request.ItemId));
            if (at != generation || closed) return;
            Load(detail, request);
            await Begin(request.StartMs, at);
        }
        catch (Exception e)
        {
            if (at == generation) FailWith(e);
        }
    }

    // ---------------------------------------------------------------- seek previews

    /// <summary>The file's seek previews: asked again every 20 s, for 10 minutes at most, while the server makes them.</summary>
    void LoadTrickplay(MediaFile? chosen)
    {
        trickplayTimer?.Dispose();
        trickplayTimer = null;
        // Only a server makes them.
        if (chosen == null || source.Local) return;
        var started = scheduler.Elapsed;
        void Ask()
        {
            Plays.When(scheduler, () => source.Trickplay(chosen.Id), found =>
            {
                if (file?.Id != chosen.Id || closed) return;
                if (found != null && found.Count > 0) Update(s => s with { Trickplay = new TrickplaySet(chosen.Id, found) });
                else Again();
            }, _ =>
            {
                // Failures count as "not yet" too.
                if (file?.Id == chosen.Id && !closed) Again();
            });
        }
        void Again()
        {
            if (scheduler.Elapsed - started < Player.Trickplay.GiveUpMs) trickplayTimer = scheduler.After(Player.Trickplay.AgainMs, Ask);
        }
        Ask();
    }

    // ---------------------------------------------------------------- state

    /// <summary>Why it plays this way when the player chose it: a fallback's reason.</summary>
    string? NoteNow() => recovery.FallbackReason is FallbackReason reason ? SessionText.Fallback(reason) : null;

    /// <summary>"Play original" after the switch for dropped frames; Automatic has Original in its panel instead.</summary>
    bool CanPlayOriginal() => recovery.StruggleSwitched && recovery.FallbackMode != null && !picked.Automatic;

    /// <summary>The quality the server converts the original at; null when it doesn't, or plays a quality asked for.</summary>
    QualityPreset? ServerQuality() =>
        quality.Original && info?.AutoQuality is string id && Qualities.ById(id) is { Original: false, Automatic: false } preset && preset.Id == id ? preset : null;

    /// <summary><paramref name="preset"/>'s name for the file playing: one taller than the file goes by the file's lines ("8 Mbps 720p").</summary>
    string Label(QualityPreset preset) => SessionText.Quality(preset, Qualities.LinesClassOf(file));

    /// <summary>The panels' tracks and qualities, as chosen now.</summary>
    void PublishChoices()
    {
        var chosenFile = file;
        var audioChoices = PlaybackPlan.AudioStreams(chosenFile).Select(s => new TrackChoice(s.Index, ItemFormat.StreamLabel(s), s.Index == audio?.Index)).ToList();
        var subs = PlaybackPlan.SubtitleStreams(chosenFile);
        var subtitleChoices = subs.Count == 0 ? new List<TrackChoice>()
            : new[] { new TrackChoice(null, SessionText.SubtitlesOff, subtitle == null) }
                .Concat(subs.Select(s => new TrackChoice(s.Index, ItemFormat.StreamLabel(s), s.Index == subtitle?.Index, BurnIn: PlaybackPlan.IsImage(s) && !Sideload)))
                .ToList();
        var sub = subtitle;
        // Text moves always; pictures only when drawn here.
        var syncable = sub != null && (!PlaybackPlan.IsImage(sub) || (Sideload && info?.BurnSubtitles != true));
        var auto = ServerQuality();
        var playing = Label(auto ?? quality);
        var automatic = picked.Automatic;
        var fileHint = Qualities.FileHint(chosenFile);
        // Nothing converts a file on a drive: it plays as it is.
        var qualities = (source.Local ? new List<QualityPreset>() : Qualities.ForFile(chosenFile)).Select(p => new QualityChoice(
            p.Id,
            Label(p),
            p.Automatic ? (automatic && (auto != null || !quality.Original) ? SessionText.AutomaticNow(playing) : SessionText.AutomaticHint)
                : p.Original ? fileHint
                : null,
            p.Id == picked.Id)).ToList();
        Update(s => s with
        {
            Audio = audioChoices,
            Subtitles = subtitleChoices,
            Qualities = qualities,
            Quality = automatic ? SessionText.Automatic(playing) : Label(quality),
            SubtitleSync = syncable,
            SubtitleOffsetMs = offset,
        });
    }

    /// <summary>
    /// PUT /api/items/{id}/streams: the next episode, or the next viewing, starts with the same
    /// tracks. Audio the server chose for this TV isn't saved: another device may play another
    /// track of that language better.
    /// </summary>
    void SaveStreams()
    {
        var detail = item;
        if (detail == null) return;
        var fileId = file?.Id;
        var audioIndex = audioPicked ? audio?.Index : null;
        var subtitleIndex = subtitle?.Index;
        reporter.Send(() => source.SetStreams(detail.Id, fileId, audioIndex, subtitleIndex));
        source.TracksChosen(detail);
        // The next episode's details, asked for before, name the tracks it started with then.
        choicesSaved++;
        nextDetail = null;
    }

    void Update(Func<PlayerState, PlayerState> change) => State.Value = change(State.Value);

    void SetClock(long? positionMs = null, long? durationMs = null, long? bufferedMs = null)
    {
        var c = Clock.Value;
        Clock.Value = new PlayerClock(positionMs ?? c.PositionMs, durationMs ?? c.DurationMs, bufferedMs ?? c.BufferedMs);
    }

    void Phase(PlayerPhase phase)
    {
        var was = State.Value.Phase;
        if (was.Kind == PhaseKind.Error || was.Kind == phase.Kind) return;
        Update(s => s with { Phase = phase });
        if (phase.Kind == PhaseKind.Playing) presenter.Resume();
    }

    void Notice(string text) => Say(new PlayerToast(text, PlayerText.ToastMs));

    void Say(PlayerToast toast) => Toast?.Invoke(toast);

    void Fail(string message, bool canRetry, string title = PlayerText.ErrorTitle) =>
        Update(s => s with { Phase = PlayerPhase.Error(message, canRetry, title), SkipLabel = null });

    void FailWith(Exception e)
    {
        if (e is SessionStoppedException stop)
        {
            StopByServer(stop.Note, byAdmin: true);
            return;
        }
        if (Errors.Unreachable(e))
        {
            Fail(SessionText.Unreachable(Errors.Describe(e)), canRetry: true, PlayerText.OfflineTitle);
            return;
        }
        var canRetry = e is not SignedOutException && !(e is ApiException { Status: 401 or 403 or 404 });
        Fail(Errors.Message(e), canRetry);
    }

    /// <summary>Waits for <paramref name="start"/>'s task, carrying on on the UI thread.</summary>
    Task<T> Ui<T>(Func<Task<T>> start)
    {
        var done = new TaskCompletionSource<T>();
        Plays.When(scheduler, start, v => done.TrySetResult(v), e => done.TrySetException(e));
        return done.Task;
    }

    // ---------------------------------------------------------------- the server out of reach

    /// <summary>The server stopped answering: since when, how often it was asked, and whether the engine gave up too.</summary>
    sealed class Outage
    {
        public long Since;
        public int Attempts;
        public IDisposable? Timer;
        public bool EngineFailed;
        public long AtMs;
    }

    /// <summary>
    /// The server stopped answering mid-playback: the player keeps its place, says "Reconnecting…"
    /// and asks again after 0.5 s, 1 s, then every 2 s, for 90 s, as long as the server waits for a
    /// player it doesn't hear from. No recovery is spent on it, and no stall makes it convert.
    /// </summary>
    void ReachLost(bool engineFailed)
    {
        if (closed || stopped) return;
        if (outage != null)
        {
            if (engineFailed) outage.EngineFailed = true;
            return;
        }
        outage = new Outage { Since = scheduler.Elapsed, EngineFailed = engineFailed, AtMs = lastPositionMs };
        Update(s => s with { Reconnecting = true, SkipLabel = null });
        if (State.Value.Phase.Kind != PhaseKind.Error) Phase(PlayerPhase.Buffering);
        AskAgainLater(outage);
    }

    void AskAgainLater(Outage o)
    {
        var wait = OutageStepsMs[Math.Min(o.Attempts, OutageStepsMs.Length - 1)];
        o.Attempts++;
        o.Timer = scheduler.After(wait, () =>
        {
            o.Timer = null;
            if (outage == o) AskAgain(o);
        });
    }

    void AskAgain(Outage o)
    {
        var s = info;
        Func<Task> ask = s != null && !sessionEnded ? () => source.PlaybackStatus(s.SessionId, OutageAskMs) : () => source.Reachable(OutageAskMs);
        Plays.When(scheduler, ask, () => ReachBack(o, sessionGone: false), e =>
        {
            if (outage != o || closed) return;
            if (e is SessionStoppedException stop)
            {
                EndOutage();
                StopByServer(stop.Note, byAdmin: true);
            }
            // It answers again, but the session went (404) or failed: a new one.
            else if (!Errors.Unreachable(e)) ReachBack(o, sessionGone: true);
            else if (scheduler.Elapsed - o.Since >= OutageMs)
            {
                EndOutage();
                Fail(SessionText.Unreachable(Errors.Describe(e)), canRetry: true, PlayerText.OfflineTitle);
            }
            else AskAgainLater(o);
        });
    }

    /// <summary>The server answers again: the engine carries on if it came back by itself, else a new session starts at the place.</summary>
    void ReachBack(Outage o, bool sessionGone)
    {
        if (outage != o || closed || stopped) return;
        var at = lastPositionMs;
        EndOutage();
        if (!sessionGone && !o.EngineFailed && engine != null && engine.Playing && engineStatus == EngineStatus.Ready)
        {
            Phase(PlayerPhase.Playing);
            return;
        }
        Restart(at);
    }

    void EndOutage()
    {
        var o = outage;
        if (o == null) return;
        o.Timer?.Dispose();
        outage = null;
        // The stall clock starts over.
        lastAdvanceAt = scheduler.Elapsed;
        Update(s => s with { Reconnecting = false });
    }
}
