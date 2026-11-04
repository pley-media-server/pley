using System;
using System.Collections.Generic;
using System.Linq;
using System.Threading;
using System.Threading.Tasks;
using Pley.Tv.Core;
using Pley.Tv.Core.Player;
using Tizen.NUI;
using Media = Tizen.Multimedia;
using Timer = Tizen.NUI.Timer;

namespace Pley.Tv.Tizen.Kit;

/// <summary>
/// The TV's own player (Tizen.Multimedia.Player) as Core drives it: Live TV, music and the video
/// player. Each source gets a player of its own, made in <see cref="Prepare"/> and let go in
/// <see cref="Stop"/>, so nothing of one source's state reaches the next. With a picture, it
/// draws on the video plane under the window, letterboxed; the window must be clear there (a
/// covering sheet). Without (music), it has no display at all.
///
/// Its events come on the UI thread, posted, so always after the call that caused them returns;
/// those of a source stopped or replaced are dropped. <see cref="Stop"/> raises nothing, and
/// <see cref="Pause"/> before <see cref="Prepare"/> does nothing. Made and used on the UI thread.
/// </summary>
sealed class TizenMediaEngine : IVideoEngine
{
    readonly bool video;
    readonly SynchronizationContext? ui = SynchronizationContext.Current;
    Media.Player? player;

    /// <summary>
    /// Whether a streamed film resumed part-way is given playlists that begin where it resumes
    /// (<see cref="StartWindow"/>), so the TV's player prepares there rather than at the film's start
    /// and then seeks. A profiling build's listener turns it off and on ("trim off"), for the next film.
    /// </summary>
    public static bool Trim = true;

    /// <summary>From how far in a film's playlists are trimmed: before it, the seek costs little.</summary>
    const long TrimFromMs = 10_000;

    /// <summary>The trimmed playlists of the film playing.</summary>
    StartWindow? window;

    /// <summary>
    /// What the source does, from its position read again and again (<see cref="PlaybackWatch"/>):
    /// its status, its first picture, where it is on the film's timeline. One a source.
    /// </summary>
    PlaybackWatch? state;
    EngineSource? source;
    /// <summary>The source playing: events of an earlier one are dropped.</summary>
    int generation;
    /// <summary>The source whose failure was said (<see cref="generation"/>'s value then).</summary>
    int failedAt = -1;
    /// <summary>The status and whether it plays, as last said.</summary>
    EngineStatus status = EngineStatus.Idle;
    bool playingNow;
    bool prepared, wanted, started, released;
    bool? hasVideo;
    long durationMs;

    /// <param name="video">Whether it shows a picture on the video plane; music plays without.</param>
    public TizenMediaEngine(bool video = true) => this.video = video;

    public event Action<EngineStatus>? StatusChanged;
    public event Action? Started;
    public event Action<bool>? PlayingChanged;
    public event Action<EngineError>? Failed;

    /// <summary>The picture's size in pixels, once known and when it changes: a screen draws black where the letterbox leaves the plane empty.</summary>
    public event Action<int, int>? VideoSize;

    /// <summary>
    /// Where it plays: the last position the TV's player said, read again off the UI thread as it's
    /// asked (asked on it, each read held the UI 30 to 200 ms, several times a second), and moved on
    /// by the time since while it plays, so the clock runs smooth between reads.
    /// </summary>
    public long PositionMs
    {
        get
        {
            var p = player;
            var w = state;
            if (w == null) return 0;
            if (p != null && prepared && w.Pending == null) AskPosition(p, w);
            return w.PositionMs;
        }
    }

    bool positionAsked;

    void AskPosition(Media.Player p, PlaybackWatch w)
    {
        if (positionAsked) return;
        positionAsked = true;
        _ = ReadPosition(p, w, generation, w.Epoch);
    }

    async Task ReadPosition(Media.Player p, PlaybackWatch w, int mine, long epoch)
    {
        try
        {
            var (raw, playing) = await OffUi("its position", () => p.State switch
            {
                Media.PlayerState.Playing => ((long)p.GetPlayPosition(), true),
                Media.PlayerState.Paused => (p.GetPlayPosition(), false),
                _ => (-1L, false),
            });
            // Asked before a seek, read after it: where it was, not where it is (the watch drops it).
            if (mine == generation && player == p && prepared) w.Read(epoch, raw, playing, weigh: false);
        }
        catch (Exception)
        {
            // Between states: the last one known.
        }
        finally
        {
            positionAsked = false;
        }
    }

    static readonly object offUiGate = new object();
    static Task offUi = Task.CompletedTask;

    /// <summary>
    /// A call into the TV's player that holds its caller (its position, its streams' details, a
    /// prepare, a seek: 30 to 500 ms each), made off the UI thread so the screen keeps drawing; one
    /// after another, for every engine, so two never meet in the player. Not those bound to the window
    /// (the picture's region, letting a player go): made from another thread, letting go held the UI
    /// 8 s and the app died.
    /// </summary>
    static Task<T> OffUi<T>(string what, Func<T> call)
    {
        lock (offUiGate)
        {
            var next = offUi.ContinueWith(_ =>
            {
                var asked = Profile.Start();
                try
                {
                    return call();
                }
                finally
                {
                    Slow(what + " (off the UI)", asked);
                }
            }, CancellationToken.None, TaskContinuationOptions.DenyChildAttach, TaskScheduler.Default);
            offUi = next;
            return next;
        }
    }

    /// <summary>
    /// Makes and lets go of a player once, off the UI thread, before any film: the first made in the
    /// app held the UI 435 ms as the first film opened; the next are quick.
    /// </summary>
    public static void Warm()
    {
        if (warmed) return;
        warmed = true;
        _ = OffUi("made to warm up", () =>
        {
            new Media.Player().Dispose();
            return true;
        });
    }

    static bool warmed;

    /// <summary>A call into the TV's player that held the UI thread: marked, in a profiling build.</summary>
    [System.Diagnostics.Conditional("PROFILE")]
    static void Slow(string what, long asked)
    {
        var ms = (System.Diagnostics.Stopwatch.GetTimestamp() - asked) * 1000.0 / System.Diagnostics.Stopwatch.Frequency;
        if (ms >= 30) Profile.Mark($"engine: the player took {ms:0} ms for {what}");
    }

    public long DurationMs => durationMs > 0 ? durationMs : source?.DurationHintMs ?? 0;

    /// <summary>Tizen's player says how far it has data as a percentage of a stream at most: the position.</summary>
    public long BufferedMs => PositionMs;

    public bool KnowsBuffer => false;

    public bool Playing => playingNow;

    public bool? HasVideo => hasVideo;

    /// <summary>Tizen's rate changes the pitch or drops the sound on many TVs: normal speed only.</summary>
    public IReadOnlyList<double> Speeds => Array.Empty<double>();

    public bool SetSpeed(double rate) => Math.Abs(rate - 1) < 0.001;

    public FrameCounts? FrameCounts() => null;

    public long? BandwidthKbps() => Network() is long bps ? bps / 1000 : null;

    readonly NetworkMeter network = new NetworkMeter();

    /// <summary>
    /// The TV's network speed (<see cref="NetworkMeter"/>), read as it's asked for: the Info panel
    /// asks once a second while it shows, and nothing reads it otherwise. Tizen's player doesn't
    /// say how fast its downloads go.
    /// </summary>
    long? Network()
    {
        try
        {
            if (NetworkMeter.Received(System.IO.File.ReadAllText("/proc/net/dev")) is long bytes) network.Add(Environment.TickCount64, bytes);
        }
        catch (Exception)
        {
            // Not readable on this TV: not known.
        }
        return network.Bps;
    }

    public void Prepare(EngineSource source, bool play)
    {
        if (released) return;
        Close();
        var mine = ++generation;
        this.source = source;
        wanted = play;
        prepared = started = false;
        hasVideo = null;
        durationMs = 0;
        var w = new PlaybackWatch(source.Live ? 0 : source.StartMs, play, () => Environment.TickCount64);
        w.StatusChanged += next => Say(mine, next);
        w.PlayingChanged += now => SayPlaying(mine, now);
        w.FirstMove += () =>
        {
            Profile.Mark("engine: picture moving (black lifts)");
            Post(mine, () => Started?.Invoke());
        };
#if PROFILE
        w.Settled += landed =>
        {
            if (seekClock > 0) Profile.Mark($"engine: seek: moving again {Environment.TickCount64 - seekClock} ms after it was asked, from {landed / 1000.0:0.0} s for {seekGoal / 1000.0:0.0} s");
            seekClock = 0;
        };
#endif
        state = w;
        Profile.Mark($"engine: {(source.Hls ? "HLS" : source.Live ? "live stream" : "file")} given, preparing");
        if (source.Url.StartsWith("http", StringComparison.OrdinalIgnoreCase)) NetTrace.Start();
        NetTrace.Playing(source.Url, () => w.PositionMs);
        Profile.Enter("engine: told buffering (the screen's update)");
        Say(mine, w.Status);
        SayPlaying(mine, false);
        Profile.Leave();
        _ = Open(source, mine);
    }

    /// <summary>
    /// The TV's player for <paramref name="s"/>, made and given its source off the UI thread, with
    /// the trimmed playlists of a film resumed part-way (<see cref="StartWindow"/>): made on it, the
    /// first player of the app held it 470 ms, and the window's server and the source 100 to 300 ms
    /// more, as the screen's spinner came.
    /// </summary>
    static async Task<(Media.Player Player, StartWindow? Window)> Made(EngineSource s)
    {
        var url = s.Url;
        StartWindow? window = null;
        if (Trim && s.Hls && !s.Live && s.StartMs >= TrimFromMs && url.StartsWith("http", StringComparison.OrdinalIgnoreCase))
        {
            window = await Task.Run(() => new StartWindow(new Uri(url), s.StartMs, mark: m => Profile.Mark("engine: " + m)));
            url = window.Url.ToString();
        }
        try
        {
            var p = await OffUi("made, its source given (the TV's object)", () =>
            {
                var made = new Media.Player();
                try
                {
                    made.SetSource(new Media.MediaUriSource(url));
                    Buffer(made, s, trimmed: window != null);
                    return made;
                }
                catch (Exception)
                {
                    made.Dispose();
                    throw;
                }
            });
            return (p, window);
        }
        catch (Exception)
        {
            window?.Dispose();
            throw;
        }
    }

    /// <summary>A player made for a source let go of before it opened: it never had the display, so it goes off the UI thread.</summary>
    static void Unused(Task<(Media.Player Player, StartWindow? Window)> made) => _ = made.ContinueWith(t =>
    {
        if (t.Status != TaskStatus.RanToCompletion) return;
        t.Result.Window?.Dispose();
        _ = OffUi("let go, never opened", () =>
        {
            t.Result.Player.Dispose();
            return true;
        });
    }, TaskScheduler.Default);

    async Task Open(EngineSource s, int mine)
    {
        // The player before (this engine's or another's) has let go of the video plane first.
        opening++;
        try
        {
            await Releasing;
        }
        finally
        {
            opening--;
        }
        if (mine != generation) return;
        // Made meanwhile: the window's alpha takes a few frames.
        var made = Made(s);
        if (video)
        {
            // The window without its alpha for a few frames, then with it again, before each player
            // takes the display: played one after another under the window's alpha (the next
            // episode, a channel tuned again), every second player showed the TV's menu where its
            // picture was, with its sound. Off and on at once did nothing: the window has to be
            // drawn without it. The screen is black here, as it starts. A player let go just before
            // went opaque already (Close), for longer than this.
            if (!opaque)
            {
                Profile.Enter("window: its alpha off (open)");
                Window.Instance.SetTransparency(false);
                Profile.Leave();
                opaque = true;
                await Task.Delay(OpaqueMs);
                if (mine != generation)
                {
                    Unused(made);
                    return;
                }
            }
            Profile.Enter("window: its alpha on");
            Window.Instance.SetTransparency(true);
            Profile.Leave();
            opaque = false;
        }
        Media.Player p;
        StartWindow? trimmed;
        try
        {
            (p, trimmed) = await made;
        }
        catch (Exception e)
        {
            if (mine == generation) Post(mine, () => Fail(ErrorOf(e)));
            return;
        }
        if (mine != generation)
        {
            Unused(made);
            return;
        }
        player = p;
        window = trimmed;
        try
        {
            Profile.Enter("player: set up (display, its events)");
            if (video) p.Display = new Media.Display(Window.Instance);
            p.ErrorOccurred += (_, e) => Post(mine, () =>
            {
                Profile.Mark($"engine: the TV's player failed: {e.Error}");
                Fail(ErrorOf(e.Error));
            });
            p.PlaybackCompleted += (_, _) => Post(mine, () =>
            {
                Profile.Mark("engine: the TV's player says the source ended");
                Completed();
            });
            p.PlaybackInterrupted += (_, e) => Post(mine, () =>
            {
                Profile.Mark($"engine: the TV took its player: {e.Reason}");
                Fail(new EngineError.Other("the TV took its player for something else"));
            });
            p.VideoStreamChanged += (_, e) => Post(mine, () =>
            {
                stats = null;
                Picture(e.Size.Width, e.Size.Height);
            });
#if PROFILE
            // Off the player's thread, its first and last only: a callback of the TV's player returns at once.
            p.BufferingProgressChanged += (_, e) =>
            {
                var percent = e.Percent;
                NetTrace.Buffered(percent);
                if (percent is 0 or 100) Post(mine, () => Profile.Mark($"engine: buffered {percent} %"));
            };
#endif
            Profile.Leave();
        }
        catch (Exception e)
        {
            Post(mine, () => Fail(ErrorOf(e)));
            return;
        }
        var w = state!;
        try
        {
            preparing = OffUi("prepare (the call)", () => p.PrepareAsync()).Unwrap();
            await preparing;
            if (mine != generation) return;
            prepared = true;
            Profile.Mark("engine: prepared");
            if (video)
            {
                // Bound to the window: on the UI thread.
                Profile.Enter("player: after prepare (display mode)");
                p.DisplaySettings.Mode = Media.PlayerDisplayMode.LetterBox;
                Profile.Leave();
            }
            // What the streams are, off the UI thread: read on it, they held it up to 240 ms between the prepare and the seek.
            var facts = await OffUi("its streams (after prepare)", () => StreamFacts(p));
            if (mine != generation) return;
            if (video && facts.Width > 0 && facts.Height > 0) Picture(facts.Width, facts.Height);
            hasVideo ??= facts.HasVideo;
            durationMs = facts.DurationMs;
            // A file's timeline is the file's: a conversion that starts there still seeks there. Live starts at its edge.
            // A seek asked before it was prepared: from there.
            var start = w.Pending ?? (s.Live ? 0 : s.StartMs);
            if (w.Pending is long asked) w.Sought(asked);
            if (window?.OffsetMs is >= 0 and var begins)
            {
                // Trimmed: the player stands at the start of the segment the film resumes in, a second or
                // two before it, with no seek. Taken to count from its playlist until it says otherwise.
                w.Trimmed(begins);
                // The film's duration, as the item says it; else the player's, which counts from the playlist too.
                durationMs = s.DurationHintMs is > 0 ? 0 : facts.DurationMs + begins;
                Profile.Mark($"engine: prepared where it resumes, {begins / 1000.0:0.0} s, for {start / 1000.0:0.0} s");
                if (start < begins) Reopen(start);
                else if (start - begins >= 3000) await SeekPlayer(p, start, mine);
            }
            else if (start >= 1000) await SeekPlayer(p, start, mine);
            if (mine != generation) return;
            // Prepared: its tracks can be switched now, before it plays. To play, it waits for its
            // first picture; said ready here, the screen's spinner went and came back.
            w.Opened();
            Post(mine, () => Opened?.Invoke());
            if (wanted) StartPlayer(mine);
        }
        catch (Exception e)
        {
            if (mine == generation) Post(mine, () => Fail(ErrorOf(e)));
        }
    }

    /// <summary>What the TV's player says of a prepared source's streams: the picture's size, whether it has one, its duration.</summary>
    readonly record struct Facts(int Width, int Height, bool? HasVideo, long DurationMs);

    /// <summary>Read off the UI thread (<see cref="OffUi"/>): none of these is bound to the window.</summary>
    static Facts StreamFacts(Media.Player p)
    {
        int width = 0, height = 0;
        try
        {
            var size = p.StreamInfo.GetVideoProperties().Size;
            width = size.Width;
            height = size.Height;
        }
        catch (Exception)
        {
            // No picture, or not known yet: the stream says when it is.
        }
        var hasVideo = HasPicture(p);
#if PROFILE
        try
        {
            var v = p.StreamInfo.GetVideoProperties();
            var a = p.StreamInfo.GetAudioProperties();
            Profile.Mark($"engine: the player reads video {p.StreamInfo.GetVideoCodec()} {v.Size.Width}x{v.Size.Height} {v.Fps} fps, audio {p.StreamInfo.GetAudioCodec()} {a.Channels} ch, picture {hasVideo}");
            var tracks = p.AudioTrackInfo;
            var languages = string.Join(" ", Enumerable.Range(0, tracks.GetCount()).Select(i =>
            {
                try
                {
                    return tracks.GetLanguageCode(i) is { Length: > 0 } code ? code : "?";
                }
                catch (Exception)
                {
                    return "?";
                }
            }));
            Profile.Mark($"engine: the player lists {tracks.GetCount()} audio tracks ({languages}), plays {tracks.Selected}");
            try
            {
                Profile.Mark($"engine: the sound is decoded by {p.AudioCodecType}; offload takes {string.Join(" ", p.AudioOffload.SupportedFormats)}");
            }
            catch (Exception e)
            {
                Profile.Mark($"engine: the sound is decoded by {p.AudioCodecType}; offload: {e.GetType().Name} {e.Message}");
            }
        }
        catch (Exception e)
        {
            Profile.Mark($"engine: the player says nothing of its streams ({e.GetType().Name})");
        }
#endif
        long duration = 0;
        try
        {
            duration = Math.Max(0, p.StreamInfo.GetDuration());
        }
        catch (Exception)
        {
            // A live stream has none.
        }
        return new Facts(width, height, hasVideo, duration);
    }

    /// <summary>
    /// What is buffered before playing, as the Android app's BufferPlan has it: 0.5 s from a server
    /// at home, 1 s from elsewhere; a live channel 0.5 s, wherever it comes from, as it airs.
    /// </summary>
    const int StartHomeMs = 500, StartRemoteMs = 1_000;

    /// <summary>
    /// A film started from a trimmed playlist (<see cref="StartWindow"/>) waits longer before it
    /// plays: on 0.5 to 1 s, a 4K remux's buffer ran dry within a second of its picture and its
    /// sound broke off, at 1.5 s it played on.
    /// </summary>
    const int TrimmedStartMs = 1_500;

    /// <summary>
    /// What is buffered before playing again: Tizen's player waits for it after a stall and after
    /// every seek too (Android waits 1.5 s and 2 s after a stall only). At 2 s a seek took 2 s, at
    /// 0.5 s under one. A live channel waits 1.5 s after a stall: on less, a weak one stalled again
    /// and again.
    /// </summary>
    const int RestartMs = 500, RestartLiveMs = 1_500;

#if PROFILE
    /// <summary>A profiling build's trimmed start ("buffer 1000") and wait after a stall or a seek ("rebuffer 2000") instead; null as above.</summary>
    public static int? TrimmedStartTry { get; set; }
    public static int? RestartTry { get; set; }
#endif

    /// <summary>
    /// The source's buffering. Left to the TV, a channel waited seconds after each stall and stalled
    /// again soon after, and a film took 0.5 s more to show its first picture.
    /// </summary>
    static void Buffer(Media.Player p, EngineSource s, bool trimmed)
    {
        // A file on a drive isn't streamed, and has no buffering to set: once its buffering was
        // only read, the TV's player said Playing and stood at 0 for good.
        if (!s.Url.StartsWith("http", StringComparison.OrdinalIgnoreCase)) return;
        try
        {
            var start = trimmed ? TrimmedStartMs : s.Live || Addresses.AtHome(s.Url) ? StartHomeMs : StartRemoteMs;
            var restart = s.Live ? RestartLiveMs : RestartMs;
#if PROFILE
            if (trimmed) start = TrimmedStartTry ?? start;
            restart = RestartTry ?? restart;
#endif
            p.BufferingTime = new Media.PlayerBufferingTime(start, restart);
            Profile.Mark($"engine: buffering {start} ms before it plays, {restart} ms after a stall or a seek");
        }
        catch (Exception)
        {
            // Not settable on this TV: its own.
        }
    }

    public void Play()
    {
        wanted = true;
        state?.Play();
        if (prepared && status != EngineStatus.Ended) StartPlayer(generation);
    }

    public void Pause()
    {
        Profile.Mark("engine: pause asked");
        wanted = false;
        state?.Pause();
        var p = player;
        if (p == null || !prepared) return;
        try
        {
            Profile.Enter("player: pause");
            if (p.State == Media.PlayerState.Playing) p.Pause();
            Profile.Leave();
        }
        catch (Exception)
        {
            // Not playing after all.
        }
    }

    public void SeekTo(long ms)
    {
        var to = Math.Max(0, ms);
        var p = player;
        var w = state;
        if (w == null) return;
        Profile.Mark($"engine: seek to {to} ms asked");
#if PROFILE
        seekClock = Environment.TickCount64;
        seekGoal = to;
#endif
        // Before the trimmed playlists begin: they don't have it.
        if (w.Before(to))
        {
            Reopen(to);
            return;
        }
        w.Seeking(to);
        if (p == null || !prepared) return;
        _ = Seek(p, w, to, generation);
    }

    public event Action<long>? RestartAt;

    public event Action? Opened;

    /// <summary>
    /// The stream can't go on from <paramref name="ms"/> (a seek before the trimmed playlists begin,
    /// an end said too soon): the player starts a session there, with playlists that begin where it
    /// now goes, its cover and spinner up meanwhile. Opened again here, without them, the old player
    /// went and the window showed the TV's menu until the new one's first picture.
    /// </summary>
    void Reopen(long ms)
    {
        var s = source;
        if (s == null) return;
        Profile.Mark($"engine: can't go on from {ms / 1000} s in this stream: a new session there");
        if (RestartAt != null) RestartAt(ms);
        else Prepare(s with { StartMs = ms }, wanted);
    }

    /// <summary>
    /// The TV's player sent to <paramref name="ms"/> on the film's timeline, off the UI thread (it holds
    /// its caller 70 to 480 ms), after any read asked before it.
    /// </summary>
    Task SeekCall(Media.Player p, PlaybackWatch w, long ms) => OffUi("seek (the call)", () => p.SetPlayPositionAsync((int)w.ToPlayer(ms), false)).Unwrap();

    /// <summary>The player sent to <paramref name="ms"/> on the film's timeline before it plays, then taken to be there.</summary>
    async Task SeekPlayer(Media.Player p, long ms, int mine)
    {
        var w = state!;
        Profile.Mark($"engine: going to {ms / 1000} s");
        await SeekCall(p, w, ms);
        Profile.Mark("engine: there");
        if (mine == generation) w.Jumped(ms);
    }

    /// <summary>A seek while it plays or is paused: it waits until it plays on from where it lands (<see cref="PlaybackWatch"/>), or plays again from the end.</summary>
    async Task Seek(Media.Player p, PlaybackWatch w, long ms, int mine)
    {
        try
        {
            await SeekCall(p, w, ms);
        }
        catch (Exception)
        {
            // A seek the player refuses leaves it where it was.
        }
        if (mine != generation) return;
        var fromEnd = w.Status == EngineStatus.Ended;
        if (!w.Sought(ms)) return;
        ReadEvery(w);
        if (fromEnd && wanted) StartPlayer(mine);
    }

    public void Stop()
    {
        generation++;
        Close();
        status = EngineStatus.Idle;
        playingNow = false;
    }

    public void Release()
    {
        Stop();
        released = true;
    }

    public IReadOnlyList<EngineTrack> AudioTracks()
    {
        var p = player;
        var tracks = new List<EngineTrack>();
        if (p == null || !prepared) return tracks;
        try
        {
            var info = p.AudioTrackInfo;
            var count = info.GetCount();
            for (var i = 0; i < count; i++)
            {
                string? language = null;
                try
                {
                    language = info.GetLanguageCode(i);
                }
                catch (Exception)
                {
                    // Not said for this one.
                }
                tracks.Add(new EngineTrack(string.IsNullOrEmpty(language) ? null : language, null, null));
            }
        }
        catch (Exception)
        {
            // Not known in this state.
        }
        return tracks;
    }

    public int? PlayingAudio()
    {
        var p = player;
        if (p == null || !prepared) return null;
        try
        {
            return p.AudioTrackInfo.Selected;
        }
        catch (Exception)
        {
            return null;
        }
    }

    public bool SelectAudio(int ordinal, string? language)
    {
        var p = player;
        if (p == null || !prepared)
        {
            Profile.Mark($"engine: audio track {ordinal} not chosen: the player isn't ready");
            return false;
        }
        try
        {
            Profile.Enter("player: audio track");
            var info = p.AudioTrackInfo;
            if (ordinal < 0 || ordinal >= info.GetCount())
            {
                Profile.Mark($"engine: audio track {ordinal} not chosen: the player lists {info.GetCount()}");
                return false;
            }
            if (language != null)
            {
                var code = info.GetLanguageCode(ordinal);
                if (!string.IsNullOrEmpty(code) && !Same(code, language))
                {
                    Profile.Mark($"engine: audio track {ordinal} not chosen: the player says {code}, not {language}");
                    return false;
                }
            }
            Profile.Mark($"engine: audio track {ordinal} of {info.GetCount()} chosen (was {info.Selected})");
            if (info.Selected != ordinal) info.Selected = ordinal;
            return true;
        }
        catch (Exception e)
        {
            Profile.Mark($"engine: audio track {ordinal} not chosen: {e.GetType().Name} {e.Message}");
            return false;
        }
        finally
        {
            Profile.Leave();
        }
    }

    /// <summary>
    /// Languages as two or three letters ("en", "eng"; "ja", "jpn"; "fr", "fre", "fra"): the same
    /// when one begins the other, or when both name one language. Japanese's "ja" doesn't begin
    /// "jpn", and every switch to a Japanese track failed and started the player again.
    /// </summary>
    static bool Same(string a, string b)
    {
        if (a.StartsWith(b, StringComparison.OrdinalIgnoreCase) || b.StartsWith(a, StringComparison.OrdinalIgnoreCase)) return true;
        return TwoLetters(a) is { } x && x == TwoLetters(b);
    }

    /// <summary>The ISO 639-1 code of a language's two- or three-letter code; null when not known.</summary>
    static string? TwoLetters(string code)
    {
        var c = code.Trim().ToLowerInvariant();
        var dash = c.IndexOfAny(new[] { '-', '_' });
        if (dash > 0) c = c.Substring(0, dash);
        if (c.Length == 2) return c;
        if (ThreeLetters.TryGetValue(c, out var two)) return two;
        try
        {
            foreach (var culture in System.Globalization.CultureInfo.GetCultures(System.Globalization.CultureTypes.NeutralCultures))
                if (culture.ThreeLetterISOLanguageName == c) return culture.TwoLetterISOLanguageName;
        }
        catch (Exception)
        {
            // No culture data on this TV: the table's.
        }
        return null;
    }

    /// <summary>The bibliographic codes Matroska and ffprobe use where .NET knows the terminological ones.</summary>
    static readonly Dictionary<string, string> ThreeLetters = new Dictionary<string, string>
    {
        ["eng"] = "en", ["jpn"] = "ja", ["fre"] = "fr", ["fra"] = "fr", ["ger"] = "de", ["deu"] = "de", ["spa"] = "es",
        ["ita"] = "it", ["por"] = "pt", ["rus"] = "ru", ["chi"] = "zh", ["zho"] = "zh", ["kor"] = "ko", ["ara"] = "ar",
        ["hin"] = "hi", ["dut"] = "nl", ["nld"] = "nl", ["swe"] = "sv", ["tur"] = "tr", ["pol"] = "pl", ["gre"] = "el",
        ["ell"] = "el", ["cze"] = "cs", ["ces"] = "cs", ["heb"] = "he", ["tha"] = "th", ["vie"] = "vi", ["ind"] = "id",
    };

    /// <summary>
    /// What the player says of its streams, read once for each source and again when its picture
    /// changes: Info asks every second, and a TV asked that often while it played closed the app.
    /// </summary>
    EngineStats? stats;
    int statsFor = -1;

    public EngineStats? Stats()
    {
        var p = player;
        if (p == null || !prepared) return null;
        // The streams are read once; the network each time.
        if (stats != null && statsFor == generation) return stats with { Bandwidth = Network() };
        Profile.Enter("player: its streams (Info)");
        int? width = null, height = null, bitrate = null, audioBitrate = null, channels = null;
        double? fps = null;
        string? codec = null, audioCodec = null;
        var asked = Profile.Start();
        try
        {
            var v = p.StreamInfo.GetVideoProperties();
            if (v.Size.Width > 0)
            {
                width = v.Size.Width;
                height = v.Size.Height;
                fps = v.Fps > 0 ? v.Fps : null;
                bitrate = v.BitRate > 0 ? v.BitRate : null;
            }
            codec = p.StreamInfo.GetVideoCodec();
        }
        catch (Exception)
        {
            // Sound only.
        }
        try
        {
            var a = p.StreamInfo.GetAudioProperties();
            channels = a.Channels > 0 ? a.Channels : null;
            audioBitrate = a.BitRate > 0 ? a.BitRate : null;
            audioCodec = p.StreamInfo.GetAudioCodec();
        }
        catch (Exception)
        {
            // No sound, or not known.
        }
        Profile.Leave();
        Slow("its stats", asked);
        var total = (bitrate ?? 0) + (audioBitrate ?? 0);
        var read = new EngineStats
        {
            Width = width,
            Height = height,
            FrameRate = fps,
            Codec = string.IsNullOrEmpty(codec) ? null : codec,
            Bitrate = total > 0 ? total : null,
            AudioCodec = string.IsNullOrEmpty(audioCodec) ? null : audioCodec,
            AudioChannels = channels,
        };
        // Kept once the picture is known; a source still starting is asked again.
        if (width != null || audioCodec != null)
        {
            stats = read;
            statsFor = generation;
        }
        return read with { Bandwidth = Network() };
    }

    void StartPlayer(int mine)
    {
        var p = player;
        var w = state;
        if (p == null || w == null || mine != generation) return;
        try
        {
            Profile.Enter("player: start");
            if (p.State != Media.PlayerState.Playing) p.Start();
            Profile.Leave();
            Profile.Mark("engine: started");
        }
        catch (Exception e)
        {
            Post(mine, () => Fail(ErrorOf(e)));
            return;
        }
        if (started) return;
        started = true;
        if (watch == null)
        {
            watch = new Timer(PlaybackWatch.EveryMs);
            watch.Tick += (s, e) =>
            {
                Watch();
                return true;
            };
        }
        // Often until the picture moves (the screen's black lifts then), then every half second.
        watch.Interval = w.ReadEveryMs;
        watch.Start();
    }

    /// <summary>
    /// The position read for <see cref="PlaybackWatch"/> to weigh, as often as it says: Tizen's
    /// player says how full its buffer is (BufferingProgressChanged), not that it waits, and a
    /// stream plays on under 100 %: taken as waiting, a channel went to its spinner within seconds,
    /// and Live TV tuned it again after 5.
    /// </summary>
    void Watch()
    {
        var p = player;
        var w = state;
        if (p == null || w == null || !started || w.Status == EngineStatus.Ended || watchAsked) return;
        watchAsked = true;
        _ = Watch(p, w, generation, w.Epoch);
    }

    bool watchAsked;

    /// <summary>The position read off the UI thread (<see cref="OffUi"/>), then weighed on it.</summary>
    async Task Watch(Media.Player p, PlaybackWatch w, int mine, long epoch)
    {
        long read;
        try
        {
            read = await OffUi("its position (watch)", () => p.State == Media.PlayerState.Playing ? p.GetPlayPosition() : -1);
        }
        catch (Exception)
        {
            return;
        }
        finally
        {
            watchAsked = false;
        }
        if (mine != generation || player != p || !started) return;
#if PROFILE
        var counted = w.CountsFromPlaylist;
#endif
        w.Read(epoch, read, playing: read >= 0);
#if PROFILE
        if (counted == null && w.CountsFromPlaylist is bool fromPlaylist)
            Profile.Mark($"engine: the player counts from {(fromPlaylist ? "its playlist" : "the film")} (it says {read} ms, the playlist begins at {w.WindowStart} ms)");
#endif
        ReadEvery(w);
    }

    /// <summary>The position read as often as the watch says now.</summary>
    void ReadEvery(PlaybackWatch w)
    {
        if (watch != null && watch.Interval != w.ReadEveryMs) watch.Interval = w.ReadEveryMs;
    }

#if PROFILE
    /// <summary>A seek's clock and where it went: how long until the picture moves again.</summary>
    long seekClock, seekGoal;
#endif
    Timer? watch;

    void Picture(int width, int height)
    {
        Profile.Mark($"engine: picture {width}x{height}");
        if (width <= 0 || height <= 0) return;
        hasVideo = true;
        Place(width, height);
        VideoSize?.Invoke(width, height);
    }

    /// <summary>The picture where <see cref="PictureArea"/> puts it, as the bars do; the TV's own letterboxing where it can't be told.</summary>
    void Place(int width, int height)
    {
        var p = player;
        if (p == null || !video) return;
        try
        {
            var (sw, sh) = PictureArea.Screen;
            var (x, y, w, h) = PictureArea.Of(width, height, sw, sh);
            Profile.Enter("player: its region");
            p.DisplaySettings.Mode = Media.PlayerDisplayMode.Roi;
            p.DisplaySettings.SetRoi(new Media.Rectangle(x, y, w, h));
            Profile.Leave();
        }
        catch (Exception)
        {
            try
            {
                p.DisplaySettings.Mode = Media.PlayerDisplayMode.LetterBox;
            }
            catch (Exception)
            {
                // Between states: its own.
            }
        }
    }

    /// <summary>
    /// The TV's player says the source ended. Now and then it said so in the middle of a film, after
    /// seeks: taken as the end, the film was reported watched and the next one played. Said more than
    /// <see cref="EarlyEndMs"/> before the film's end, it is opened again where it was, once; said
    /// again there, it is the end (a film shorter than the item says).
    /// </summary>
    void Completed()
    {
        var at = PositionMs;
        var duration = DurationMs;
        if (duration > 0 && duration - at > EarlyEndMs && Math.Abs(at - earlyEndAt) > EarlyEndMs)
        {
            earlyEndAt = at;
            Profile.Mark($"engine: the end said at {at / 1000} s of {duration / 1000} s: opened again there");
            Reopen(at);
            return;
        }
        state?.Ended();
    }

    const long EarlyEndMs = 60_000;

    /// <summary>Where an end said too soon was last opened again from.</summary>
    long earlyEndAt = -1;

    void Fail(EngineError error)
    {
        // Once a source: the TV's player says a failure as its error and in the call it failed in.
        if (failedAt == generation) return;
        failedAt = generation;
        playingNow = false;
        Failed?.Invoke(error);
    }

    /// <summary>The status is <paramref name="next"/>: said once it changes.</summary>
    void Say(int mine, EngineStatus next)
    {
        if (mine != generation || status == next) return;
        status = next;
        Profile.Mark($"engine: {next}");
        Post(mine, () => StatusChanged?.Invoke(next));
    }

    /// <summary>It plays (asked to, prepared, with data, not at its end) or stopped playing: said once it changes.</summary>
    void SayPlaying(int mine, bool now)
    {
        if (mine != generation || now == playingNow) return;
        playingNow = now;
        Post(mine, () => PlayingChanged?.Invoke(now));
    }

    /// <summary>Runs <paramref name="action"/> on the UI thread later, unless the source changed meanwhile.</summary>
    void Post(int mine, Action action)
    {
        void Run()
        {
            if (mine == generation && !released) action();
        }
        if (ui != null) ui.Post(_ => Run(), null);
        else Run();
    }

    static bool? HasPicture(Media.Player p)
    {
        try
        {
            var size = p.StreamInfo.GetVideoProperties().Size;
            if (size.Width > 0 && size.Height > 0) return true;
        }
        catch (Exception)
        {
            // No video stream.
        }
        try
        {
            p.StreamInfo.GetAudioProperties();
            return false;
        }
        catch (Exception)
        {
            return null;
        }
    }

    /// <summary>The player let go: stopped, unprepared and disposed.</summary>
    /// <summary>
    /// The players being let go, one after another: the next player claims the video plane only
    /// once they have. Shared by every engine, as the TV has one video plane.
    /// </summary>
    static Task Releasing = Task.CompletedTask;

    /// <summary>How long the window goes without its alpha before a player takes the display: a few frames.</summary>
    const int OpaqueMs = 120;

    /// <summary>The window is without its alpha: a player showing a picture is being let go, or the next is starting.</summary>
    static bool opaque;

    /// <summary>The prepare of the player playing, while it runs.</summary>
    Task? preparing;

    void Close()
    {
        var p = player;
        NetTrace.Stop();
        window?.Dispose();
        window = null;
        var prepare = preparing;
        watch?.Stop();
        player = null;
        preparing = null;
        prepared = false;
        if (p == null) return;
        var cover = false;
        if (video && !opaque)
        {
            // The picture goes once the window is drawn without its alpha: stopped at once, the
            // video plane emptied before the screen's black was drawn, and the TV's menu showed
            // through for a moment as a channel changed.
            Profile.Enter("window: its alpha off (close)");
            Window.Instance.SetTransparency(false);
            Profile.Leave();
            opaque = cover = true;
        }
        Releasing = Release(p, prepare, Releasing, cover);
    }

    /// <summary>
    /// Lets <paramref name="p"/> go. One still preparing can't be stopped: let go then, it went
    /// on preparing and held the video plane, and the next source played with sound and no
    /// picture (the TV's menu under a channel or a film). So it goes once its prepare has ended.
    /// </summary>
    static async Task Release(Media.Player p, Task? prepare, Task before, bool cover)
    {
        await before;
        // The window drawn without its alpha first (Close).
        if (cover) await Task.Delay(OpaqueMs);
        if (prepare != null)
        {
            try
            {
                await prepare;
            }
            catch (Exception)
            {
                // It failed: there is nothing to stop.
            }
        }
        // The calls off the UI asked before (a position, a seek) done first: none meets the player let go.
        await OffUi("let go after", () => true);
        // On the UI thread, though it holds it 100 to 300 ms: let go from another, the player, bound
        // to the window, held the UI 8 s as a film was left, and the app died moments later.
        var releasing = Profile.Start();
        try
        {
            Profile.Enter("player: stop (release)");
            if (p.State is Media.PlayerState.Playing or Media.PlayerState.Paused) p.Stop();
            Profile.Leave();
            Profile.Enter("player: unprepare (release)");
            if (p.State != Media.PlayerState.Idle) p.Unprepare();
            Profile.Leave();
        }
        catch (Exception)
        {
            // Already stopped.
        }
        try
        {
            Profile.Enter("player: dispose (release)");
            p.Dispose();
            Profile.Leave();
        }
        catch (Exception)
        {
            // Gone already.
        }
        Profile.Took("engine: the player let go of", releasing);
        // No player waiting to take the display (it would turn the alpha back on itself): as it was,
        // with its alpha while a player still covers the screen; the shell made it opaque if not.
        if (cover && opening == 0 && opaque)
        {
            opaque = false;
            Profile.Enter("window: its alpha on (release)");
            if (Covered?.Invoke() != false) Window.Instance.SetTransparency(true);
            Profile.Leave();
        }
    }

    /// <summary>Whether a player covers the screen (the shell's): the window has its alpha only then.</summary>
    public static Func<bool>? Covered { get; set; }

    /// <summary>Players waiting for the ones before to be let go.</summary>
    static int opening;

    /// <summary>What the TV's player said, in Core's terms.</summary>
    static EngineError ErrorOf(Media.PlayerError error) => error switch
    {
        Media.PlayerError.AudioCodecNotSupported => new EngineError.Unsupported("the TV can't decode its sound", Audio: true),
        Media.PlayerError.VideoCodecNotSupported => new EngineError.Unsupported("the TV can't decode its picture"),
        Media.PlayerError.NotSupportedFile => new EngineError.Unsupported("the TV can't open this file", Container: true),
        Media.PlayerError.DrmNotPermitted => new EngineError.Unsupported("it is protected"),
        Media.PlayerError.ConnectionFailed => new EngineError.Network("the connection failed"),
        // Tizen's player gives no status: a missing file is the nearest it says to one.
        Media.PlayerError.NoSuchFile => new EngineError.Http(404, "the server has nothing at its address"),
        _ => new EngineError.Other(error.ToString()),
    };

    /// <summary>A call that threw: Tizen says the error in the exception's type and message.</summary>
    static EngineError ErrorOf(Exception e)
    {
        var message = e.Message ?? "";
        if (e is NotSupportedException || message.IndexOf("NotSupported", StringComparison.OrdinalIgnoreCase) >= 0)
        {
            var audio = message.IndexOf("Audio", StringComparison.OrdinalIgnoreCase) >= 0;
            var file = message.IndexOf("File", StringComparison.OrdinalIgnoreCase) >= 0 || message.IndexOf("Format", StringComparison.OrdinalIgnoreCase) >= 0;
            return new EngineError.Unsupported(message, Audio: audio, Container: file && !audio);
        }
        if (message.IndexOf("Connection", StringComparison.OrdinalIgnoreCase) >= 0) return new EngineError.Network(message);
        return new EngineError.Other(message);
    }
}
