using System;
using System.Collections.Generic;
using System.Threading;
using System.Threading.Tasks;
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
                    if (p.State is Media.PlayerState.Playing or Media.PlayerState.Paused) lastPosition = p.GetPlayPosition();
                }
                catch (Exception)
                {
                    // Between states: the last one known.
                }
            }
            return pendingSeek ?? lastPosition;
        }
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

    public long? BandwidthKbps() => null;

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
            Window.Instance.SetTransparency(true);
            opaque = false;
        }
        try
        {
            if (video) p.Display = new Media.Display(Window.Instance);
            p.ErrorOccurred += (_, e) => Post(mine, () => Fail(ErrorOf(e.Error)));
            p.PlaybackCompleted += (_, _) => Post(mine, Completed);
            p.PlaybackInterrupted += (_, _) => Post(mine, () => Fail(new EngineError.Other("the TV took its player for something else")));
            p.VideoStreamChanged += (_, e) => Post(mine, () => Picture(e.Size.Width, e.Size.Height));
            p.SetSource(new Media.MediaUriSource(s.Url));
            if (s.Live) Buffer(p);
#if PROFILE
            p.BufferingProgressChanged += (_, e) => Profile.Mark($"engine: buffered {e.Percent} %");
#endif
        }
        catch (Exception e)
        {
            Post(mine, () => Fail(ErrorOf(e)));
            return;
        }
        try
        {
            preparing = p.PrepareAsync();
            await preparing;
            if (mine != generation) return;
            prepared = true;
            Profile.Mark("engine: prepared");
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
            // A file's timeline is the file's: a conversion that starts there still seeks there. Live starts at its edge.
            var start = pendingSeek ?? (s.Live ? 0 : s.StartMs);
            if (start >= 1000)
            {
                await p.SetPlayPositionAsync((int)start, false);
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

    /// <summary>A live channel plays once this much is buffered (the Android app's live threshold).</summary>
    const int LiveStartBufferMs = 500;

    /// <summary>After a stall, it plays again once this much is buffered (the Android app's).</summary>
    const int LiveRestartBufferMs = 1_500;

    /// <summary>
    /// A live channel's buffering as the Android app's: left to the TV, its player waited seconds
    /// after each stall, and stalled again soon after.
    /// </summary>
    static void Buffer(Media.Player p)
    {
        try
        {
            var before = p.BufferingTime;
            Profile.Mark($"engine: the TV's own buffering {before.PreBufferMillisecond} ms, {before.ReBufferMillisecond} ms after a stall");
            p.BufferingTime = new Media.PlayerBufferingTime(LiveStartBufferMs, LiveRestartBufferMs);
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
        wanted = false;
        var p = player;
        if (p == null || !prepared) return;
        try
        {
            if (p.State == Media.PlayerState.Playing) p.Pause();
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
        pendingSeek = to;
        if (!prepared) return;
        var mine = generation;
        _ = Seek(p, to, mine);
    }

    async Task Seek(Media.Player p, long ms, int mine)
    {
        try
        {
            await p.SetPlayPositionAsync((int)ms, false);
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

    public bool SelectAudio(int ordinal, string? language)
    {
        var p = player;
        if (p == null || !prepared) return false;
        try
        {
            var info = p.AudioTrackInfo;
            if (ordinal < 0 || ordinal >= info.GetCount()) return false;
            if (language != null)
            {
                var code = info.GetLanguageCode(ordinal);
                if (!string.IsNullOrEmpty(code) && !Same(code, language)) return false;
            }
            info.Selected = ordinal;
            return true;
        }
        catch (Exception)
        {
            return false;
        }
    }

    /// <summary>Languages as two or three letters ("en", "eng"): the same when one begins the other.</summary>
    static bool Same(string a, string b) =>
        a.StartsWith(b, StringComparison.OrdinalIgnoreCase) || b.StartsWith(a, StringComparison.OrdinalIgnoreCase);

    public EngineStats? Stats()
    {
        var p = player;
        if (p == null || !prepared) return null;
        int? width = null, height = null, bitrate = null, audioBitrate = null, channels = null;
        double? fps = null;
        string? codec = null, audioCodec = null;
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
        var total = (bitrate ?? 0) + (audioBitrate ?? 0);
        return new EngineStats
        {
            Width = width,
            Height = height,
            FrameRate = fps,
            Codec = string.IsNullOrEmpty(codec) ? null : codec,
            Bitrate = total > 0 ? total : null,
            AudioCodec = string.IsNullOrEmpty(audioCodec) ? null : audioCodec,
            AudioChannels = channels,
        };
    }

    void StartPlayer(int mine)
    {
        var p = player;
        if (p == null || mine != generation) return;
        try
        {
            if (p.State != Media.PlayerState.Playing) p.Start();
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
            playing = p.State == Media.PlayerState.Playing;
            position = playing ? lastPosition = p.GetPlayPosition() : lastPosition;
        }
        catch (Exception)
        {
            return;
        }
        if (!moved)
        {
            // Started: the position has moved on from where it began, so pictures come.
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
    long firstAt = -1;
    long watchedAt = -1;
    int stillFor;
    Timer? watch;

    void Picture(int width, int height)
    {
        Profile.Mark($"engine: picture {width}x{height}");
        if (width <= 0 || height <= 0) return;
        hasVideo = true;
        VideoSize?.Invoke(width, height);
    }

    void Completed()
    {
        SetStatus(generation, EngineStatus.Ended);
    }

    void Fail(EngineError error)
    {
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
            Window.Instance.SetTransparency(false);
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
        // Off the UI thread: stopping and unpreparing wait on the TV's player, and held the UI for
        // 100 to 300 ms as each channel changed. Back on it after, for the window.
        await Task.Run(() =>
        {
            try
            {
                if (p.State is Media.PlayerState.Playing or Media.PlayerState.Paused) p.Stop();
                if (p.State != Media.PlayerState.Idle) p.Unprepare();
            }
            catch (Exception)
            {
                // Already stopped.
            }
            try
            {
                p.Dispose();
            }
            catch (Exception)
            {
                // Gone already.
            }
        });
        // No player waiting to take the display (it would turn the alpha back on itself): as it was,
        // with its alpha while a player still covers the screen; the shell made it opaque if not.
        if (cover && opening == 0 && opaque)
        {
            opaque = false;
            if (Covered?.Invoke() != false) Window.Instance.SetTransparency(true);
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
