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
    EngineSource? source;
    /// <summary>The source playing: events of an earlier one are dropped.</summary>
    int generation;
    /// <summary>The source whose failure was said (<see cref="generation"/>'s value then).</summary>
    int failedAt = -1;
    EngineStatus status = EngineStatus.Idle;
    bool prepared, wanted, started, released, playingNow;
    bool? hasVideo;
    long lastPosition, durationMs;
    long? pendingSeek;

    /// <param name="video">Whether it shows a picture on the video plane; music plays without.</param>
    public TizenMediaEngine(bool video = true) => this.video = video;

    public event Action<EngineStatus>? StatusChanged;
    public event Action? Started;
    public event Action<bool>? PlayingChanged;
    public event Action<EngineError>? Failed;

    /// <summary>The picture's size in pixels, once known and when it changes: a screen draws black where the letterbox leaves the plane empty.</summary>
    public event Action<int, int>? VideoSize;

    public long PositionMs
    {
        get
        {
            var p = player;
            if (p != null && prepared && pendingSeek == null)
            {
                try
                {
                    var asked = Profile.Start();
                    Profile.Enter("player: its position");
                    if (p.State is Media.PlayerState.Playing or Media.PlayerState.Paused) lastPosition = p.GetPlayPosition();
                    Profile.Leave();
                    Slow("its position", asked);
                }
                catch (Exception)
                {
                    // Between states: the last one known.
                }
            }
            return pendingSeek ?? lastPosition;
        }
    }

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
        pendingSeek = null;
        lastPosition = source.Live ? 0 : Math.Max(0, source.StartMs);
        var p = new Media.Player();
        player = p;
        Profile.Mark($"engine: {(source.Hls ? "HLS" : source.Live ? "live stream" : "file")} given, preparing");
        if (source.Url.StartsWith("http", StringComparison.OrdinalIgnoreCase)) NetTrace.Start();
        NetTrace.Playing(source.Url, () => lastPosition);
        SetStatus(mine, EngineStatus.Buffering);
        _ = Open(p, source, mine);
    }

    async Task Open(Media.Player p, EngineSource s, int mine)
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
                Window.Instance.SetTransparency(false);
                opaque = true;
                await Task.Delay(OpaqueMs);
                if (mine != generation) return;
            }
            Profile.Enter("window: its alpha on");
            Window.Instance.SetTransparency(true);
            Profile.Leave();
            opaque = false;
        }
        try
        {
            Profile.Enter("player: set up (display, source)");
            if (video) p.Display = new Media.Display(Window.Instance);
            p.ErrorOccurred += (_, e) => Post(mine, () => Fail(ErrorOf(e.Error)));
            p.PlaybackCompleted += (_, _) => Post(mine, Completed);
            p.PlaybackInterrupted += (_, _) => Post(mine, () => Fail(new EngineError.Other("the TV took its player for something else")));
            p.VideoStreamChanged += (_, e) => Post(mine, () =>
            {
                stats = null;
                Picture(e.Size.Width, e.Size.Height);
            });
            p.SetSource(new Media.MediaUriSource(s.Url));
            Buffer(p, s);
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
        try
        {
            Profile.Enter("player: prepare (the call)");
            preparing = p.PrepareAsync();
            Profile.Leave();
            await preparing;
            if (mine != generation) return;
            prepared = true;
            Profile.Mark("engine: prepared");
            Profile.Enter("player: after prepare (display mode, streams, duration)");
            if (video)
            {
                p.DisplaySettings.Mode = Media.PlayerDisplayMode.LetterBox;
                try
                {
                    var size = p.StreamInfo.GetVideoProperties().Size;
                    if (size.Width > 0 && size.Height > 0) Picture(size.Width, size.Height);
                }
                catch (Exception)
                {
                    // No picture, or not known yet: the stream says when it is.
                }
            }
            hasVideo ??= HasPicture(p);
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
            try
            {
                durationMs = Math.Max(0, p.StreamInfo.GetDuration());
            }
            catch (Exception)
            {
                // A live stream has none.
            }
            Profile.Leave();
            // A file's timeline is the file's: a conversion that starts there still seeks there. Live starts at its edge.
            var start = pendingSeek ?? (s.Live ? 0 : s.StartMs);
            if (start >= 1000)
            {
                Profile.Mark($"engine: going to {start / 1000} s");
                Profile.Enter("player: seek (the call)");
                var going = p.SetPlayPositionAsync((int)start, false);
                Profile.Leave();
                await going;
                Profile.Mark("engine: there");
                if (mine != generation) return;
                lastPosition = start;
            }
            pendingSeek = null;
            SetStatus(mine, EngineStatus.Ready);
            if (wanted) StartPlayer(mine);
        }
        catch (Exception e)
        {
            if (mine == generation) Post(mine, () => Fail(ErrorOf(e)));
        }
    }

    /// <summary>
    /// What is buffered before playing, and before playing again after a stall, as the Android app's
    /// BufferPlan has it: 0.5 s and 1.5 s from a server at home, 1 s and 2 s from elsewhere; a live
    /// channel the first, wherever it comes from, as it airs.
    /// </summary>
    const int StartHomeMs = 500, RestartHomeMs = 1_500, StartRemoteMs = 1_000, RestartRemoteMs = 2_000;

    /// <summary>
    /// The source's buffering as the Android app's. Left to the TV, a channel waited seconds after
    /// each stall and stalled again soon after, and a film took 0.5 s more to show its first picture.
    /// </summary>
    static void Buffer(Media.Player p, EngineSource s)
    {
        // A file on a drive isn't streamed, and has no buffering to set: once its buffering was
        // only read, the TV's player said Playing and stood at 0 for good.
        if (!s.Url.StartsWith("http", StringComparison.OrdinalIgnoreCase)) return;
        try
        {
            var before = p.BufferingTime;
            Profile.Mark($"engine: the TV's own buffering {before.PreBufferMillisecond} ms, {before.ReBufferMillisecond} ms after a stall");
            var home = s.Live || Addresses.AtHome(s.Url);
            p.BufferingTime = new Media.PlayerBufferingTime(home ? StartHomeMs : StartRemoteMs, home ? RestartHomeMs : RestartRemoteMs);
        }
        catch (Exception)
        {
            // Not settable on this TV: its own.
        }
    }

    public void Play()
    {
        wanted = true;
        if (prepared && status != EngineStatus.Ended) StartPlayer(generation);
    }

    public void Pause()
    {
        Profile.Mark("engine: pause asked");
        wanted = false;
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
        UpdatePlaying(generation);
    }

    public void SeekTo(long ms)
    {
        var to = Math.Max(0, ms);
        var p = player;
        if (p == null) return;
        Profile.Mark($"engine: seek to {to} ms asked");
        pendingSeek = to;
        if (!prepared) return;
        var mine = generation;
        _ = Seek(p, to, mine);
    }

    async Task Seek(Media.Player p, long ms, int mine)
    {
        try
        {
            Profile.Enter("player: seek (the call)");
            var going = p.SetPlayPositionAsync((int)ms, false);
            Profile.Leave();
            await going;
        }
        catch (Exception)
        {
            // A seek the player refuses leaves it where it was.
        }
        if (mine != generation || pendingSeek != ms) return;
        pendingSeek = null;
        lastPosition = ms;
        if (status == EngineStatus.Ended)
        {
            // Back from the end: it can play again.
            SetStatus(mine, EngineStatus.Ready);
            if (wanted) StartPlayer(mine);
        }
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
        if (p == null || mine != generation) return;
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
        if (!started)
        {
            started = true;
            if (watch == null)
            {
                watch = new Timer(WatchMs);
                watch.Tick += (s, e) =>
                {
                    Watch();
                    return true;
                };
            }
            watchedAt = -1;
            stillFor = 0;
            moved = false;
            firstAt = -1;
            // Often until it starts, so the picture shows as soon as it moves; then every half second.
            watch.Interval = StartWatchMs;
            watch.Start();
            // Started once the position moves (Watch), not now: Start returns before the first
            // picture, and the screen's black lifting then showed what was under the app.
            SetStatus(mine, EngineStatus.Buffering);
        }
        UpdatePlaying(mine);
    }

    /// <summary>
    /// Whether it waits for data, once playing. Tizen's player says how full its buffer is
    /// (BufferingProgressChanged), not that it waits: a stream plays on under 100 %, and taken as
    /// waiting, a channel went to its spinner within seconds, and Live TV tuned it again after 5.
    /// So it waits when its position stands still for <see cref="StillMs"/> while it should play,
    /// and plays again once the position moves.
    /// </summary>
    void Watch()
    {
        var p = player;
        if (p == null || !started || status == EngineStatus.Ended) return;
        bool playing;
        long position;
        try
        {
            var asked = Profile.Start();
            Profile.Enter("player: its position (watch)");
            playing = p.State == Media.PlayerState.Playing;
            position = playing ? lastPosition = p.GetPlayPosition() : lastPosition;
            Profile.Leave();
            Slow("its position (watch)", asked);
        }
        catch (Exception)
        {
            return;
        }
        if (!moved)
        {
            // Started: the position has moved on from where it began, so pictures come.
#if PROFILE
            if (++startWatches % 25 == 0) Profile.Mark($"engine: waiting for the picture to move: {p.State}, at {position} ms");
#endif
            if (!playing) return;
            if (firstAt < 0) firstAt = position;
            if (position < firstAt + FirstMoveMs) return;
            moved = true;
            Profile.Mark("engine: picture moving (black lifts)");
            watch!.Interval = WatchMs;
            watchedAt = position;
            stillFor = 0;
            Post(generation, () => Started?.Invoke());
            SetStatus(generation, EngineStatus.Ready);
            return;
        }
        if (!wanted || !playing || position != watchedAt)
        {
            watchedAt = position;
            stillFor = 0;
            if (status == EngineStatus.Buffering) SetStatus(generation, EngineStatus.Ready);
            return;
        }
        stillFor += (int)WatchMs;
        if (stillFor >= StillMs) SetStatus(generation, EngineStatus.Buffering);
    }

    const uint WatchMs = 500;
    /// <summary>How often the position is read until it first moves: the screen's black lifts then.</summary>
    const uint StartWatchMs = 40;
    const int StillMs = 1500;
    /// <summary>How far the position moves from where it began before the source counts as started.</summary>
    const long FirstMoveMs = 80;
    bool moved;
#if PROFILE
    int startWatches;
#endif
    long firstAt = -1;
    long watchedAt = -1;
    int stillFor;
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

    void Completed()
    {
        SetStatus(generation, EngineStatus.Ended);
    }

    void Fail(EngineError error)
    {
        // Once a source: the TV's player says a failure as its error and in the call it failed in.
        if (failedAt == generation) return;
        failedAt = generation;
        playingNow = false;
        Failed?.Invoke(error);
    }

    /// <summary>The status is <paramref name="next"/>: said once it changes, with whether it plays.</summary>
    void SetStatus(int mine, EngineStatus next)
    {
        if (mine != generation || status == next) return;
        status = next;
        Post(mine, () => StatusChanged?.Invoke(next));
        UpdatePlaying(mine);
    }

    /// <summary>It plays when asked to, prepared, with data and not at its end.</summary>
    void UpdatePlaying(int mine)
    {
        if (mine != generation) return;
        var now = wanted && prepared && started && status == EngineStatus.Ready;
        if (now == playingNow) return;
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
        // On the UI thread, though it holds it 100 to 300 ms: let go from another, the player, bound
        // to the window, held the UI 8 s as a film was left, and the app died moments later.
        var releasing = Profile.Start();
        try
        {
            Profile.Enter("player: stop and unprepare (release)");
            if (p.State is Media.PlayerState.Playing or Media.PlayerState.Paused) p.Stop();
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
