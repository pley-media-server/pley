using System;
using System.Collections.Generic;
using System.Linq;
using System.Threading.Tasks;
using Pley.Tv.Core.Api;
using Pley.Tv.Core.Auth;
using Pley.Tv.Core.Player;
using Pley.Tv.Core.Ui;

namespace Pley.Tv.Core.Tests;

/// <summary>The TV's player in a test: it does what it is told at once, and a test moves it on (the web TV app's fake engine).</summary>
sealed class FakeVideoEngine : IVideoEngine
{
    public event Action<EngineStatus>? StatusChanged;
    public event Action? Started;
    public event Action<bool>? PlayingChanged;
    public event Action<EngineError>? Failed;

    public List<EngineSource> Prepared { get; } = new List<EngineSource>();
    public List<long> Seeks { get; } = new List<long>();
    public List<(int Ordinal, string? Language)> AudioSwitches { get; } = new List<(int, string?)>();
    public List<double> SpeedsSet { get; } = new List<double>();
    public bool Released { get; private set; }
    public int Stops { get; private set; }

    public long PositionMs { get; set; }
    public long DurationMs { get; set; }
    public long BufferedMs { get; set; }
    public bool KnowsBuffer => true;
    public EngineStatus Status { get; set; } = EngineStatus.Idle;
    public bool Playing { get; set; }
    public bool? HasVideo => true;
    public IReadOnlyList<double> Speeds { get; set; } = new double[0];
    public FrameCounts? Frames { get; set; }
    public EngineStats? StatsGiven { get; set; }

    /// <summary>Whether the player still listens.</summary>
    public bool Heard => StatusChanged != null;

    public void Prepare(EngineSource source, bool play)
    {
        Prepared.Add(source);
        PositionMs = source.StartMs;
        Playing = play;
        Status = EngineStatus.Buffering;
    }

    public void Play() => Playing = true;

    public void Pause() => Playing = false;

    public void SeekTo(long ms)
    {
        Seeks.Add(ms);
        PositionMs = ms;
    }

    public void Stop()
    {
        Stops++;
        Status = EngineStatus.Idle;
        Playing = false;
    }

    public void Release() => Released = true;

    /// <summary>The audio tracks the player lists; none, as an engine that doesn't say, unless a test gives them.</summary>
    public List<EngineTrack> Tracks { get; } = new List<EngineTrack>();

    /// <summary>The listed track playing, as the TV's player says.</summary>
    public int? PlayingTrack { get; set; }

    public IReadOnlyList<EngineTrack> AudioTracks() => Tracks;

    public int? PlayingAudio() => PlayingTrack;

    public bool SelectAudio(int ordinal, string? language)
    {
        AudioSwitches.Add((ordinal, language));
        return true;
    }

    public bool SetSpeed(double rate)
    {
        SpeedsSet.Add(rate);
        return Speeds.Contains(rate) || rate == 1;
    }

    public FrameCounts? FrameCounts() => Frames;

    public long? BandwidthKbps() => null;

    public EngineStats? Stats() => StatsGiven;

    /// <summary>The source opened and plays: its first frame shows.</summary>
    public void Show()
    {
        Status = EngineStatus.Ready;
        Started?.Invoke();
        StatusChanged?.Invoke(EngineStatus.Ready);
        PlayingChanged?.Invoke(Playing);
    }

    public void Tell(EngineStatus status)
    {
        Status = status;
        StatusChanged?.Invoke(status);
    }

    public void Fail(EngineError error) => Failed?.Invoke(error);
}

/// <summary>Reports sent at once, in order: a test reads them as they were made.</summary>
sealed class InstantReporter : IReporter
{
    public void Send(Func<Task> report)
    {
        try
        {
            report().GetAwaiter().GetResult();
        }
        catch (Exception)
        {
            // Best effort, as the app's.
        }
    }

    public Task Flush() => Task.CompletedTask;
}

/// <summary>The app around the player in a test: items, sessions the server makes, and what was sent.</summary>
sealed class FakePlayerSource : IPlayerSource
{
    readonly List<Action<ServerEvent>> listeners = new List<Action<ServerEvent>>();
    int sessions;

    public bool Local { get; set; }
    public string Server => ItemKit.Origin;
    public string ClientId => "tizen-test";
    public long? ProfileId { get; set; } = 1;
    public IKeyValue Storage { get; } = new MemoryKeyValue();
    public Settings Settings { get; set; } = new Settings();
    public Dictionary<long, ItemDetail> Items { get; } = new Dictionary<long, ItemDetail>();
    public IReadOnlyDictionary<string, object?>? Report { get; set; } = TizenCaps.Of(new TizenFacts { OutputMode = "DOLBY_DIGITAL_PLUS" });

    /// <summary>The sessions asked for, in order.</summary>
    public List<IReadOnlyDictionary<string, object?>> Bodies { get; } = new List<IReadOnlyDictionary<string, object?>>();
    /// <summary>How the server answers a body; direct play of the file asked for without.</summary>
    public Func<IReadOnlyDictionary<string, object?>, PlaybackInfo>? Answer { get; set; }
    public Exception? StartFailure { get; set; }
    public List<(long ItemId, long Position, string State, string Session)> Reports { get; } = new List<(long, long, string, string)>();
    /// <summary>The tracks each of <see cref="Reports"/> said play, in the same order.</summary>
    public List<PlayingTracks?> ReportTracks { get; } = new List<PlayingTracks?>();
    /// <summary>The quality each of <see cref="Reports"/> said is picked, in the same order.</summary>
    public List<string?> ReportQualities { get; } = new List<string?>();
    public List<string> Stopped { get; } = new List<string>();
    /// <summary>A session's status; null: it answers with no transcode.</summary>
    public Func<string, SessionStatus>? Status { get; set; }
    public Exception? StatusFailure { get; set; }
    public Dictionary<string, SubtitleText> Vtt { get; } = new Dictionary<string, SubtitleText>();
    /// <summary>The server's .mks of picture tracks by address, wherever they are asked from (<c>at</c>); one missing is answered with <see cref="MksFailure"/>.</summary>
    public Dictionary<string, SubtitlePictures> Mks { get; } = new Dictionary<string, SubtitlePictures>();
    /// <summary>How a .mks that isn't there fails: a 404, as a server without them.</summary>
    public Exception MksFailure { get; set; } = new ApiException(404, "not_found", "no such track");
    public List<string> MksAsked { get; } = new List<string>();
    public Func<long, TrickplayInfo?> TrickplayAnswer { get; set; } = _ => null;
    public int TrickplayAsks { get; private set; }
    public Dictionary<long, ItemSummary?> NextOf { get; } = new Dictionary<long, ItemSummary?>();
    public Dictionary<long, IReadOnlyList<ItemSummary>> Queues { get; } = new Dictionary<long, IReadOnlyList<ItemSummary>>();
    public List<(long ItemId, long? FileId, int? Audio, int? Subtitle)> Streams { get; } = new List<(long, long?, int?, int?)>();

    public void UpdateSettings(Func<Settings, Settings> change) => Settings = change(Settings);

    public string Url(string path) => Artwork.Absolute(path, Server);

    public IReadOnlyDictionary<string, object?>? Caps() => Report;

    public ItemDetail? Saved(long id) => null;

    public Task<ItemDetail> Item(long id) => Items.TryGetValue(id, out var item) ? Task.FromResult(item) : Task.FromException<ItemDetail>(new ApiException(404, "not_found", "no such item"));

    public Task<PlaybackInfo> StartPlayback(IReadOnlyDictionary<string, object?> body)
    {
        Bodies.Add(body);
        if (StartFailure != null) return Task.FromException<PlaybackInfo>(StartFailure);
        sessions++;
        var answer = Answer?.Invoke(body) ?? Direct((long)body["file_id"]!);
        return Task.FromResult(answer with { SessionId = $"s{sessions}" });
    }

    /// <summary>Direct play of <paramref name="fileId"/>, at a keyed address.</summary>
    public static PlaybackInfo Direct(long fileId, long durationMs = 2_700_000) => new PlaybackInfo
    {
        Method = "direct",
        Url = $"/api/media/k3y/files/{fileId}/stream",
        FileId = fileId,
        DurationMs = durationMs,
    };

    /// <summary>A transcode of <paramref name="fileId"/>, at a keyed address.</summary>
    public static PlaybackInfo Stream(long fileId, string method = "transcode", long durationMs = 2_700_000) => new PlaybackInfo
    {
        Method = method,
        Url = "/api/media/k3y/playback/s/master.m3u8",
        Hls = true,
        FileId = fileId,
        DurationMs = durationMs,
    };

    public Task StopPlayback(string session)
    {
        Stopped.Add(session);
        return Task.CompletedTask;
    }

    public Task Progress(long itemId, long positionMs, long? durationMs, string state, string session, PlayingTracks? tracks = null, string? quality = null)
    {
        Reports.Add((itemId, positionMs, state, session));
        ReportTracks.Add(tracks);
        ReportQualities.Add(quality);
        return Task.CompletedTask;
    }

    public Task<SessionStatus> PlaybackStatus(string session, int timeoutMs)
    {
        if (StatusFailure != null) return Task.FromException<SessionStatus>(StatusFailure);
        return Task.FromResult(Status?.Invoke(session) ?? new SessionStatus { SessionId = session });
    }

    public Task Reachable(int timeoutMs) => StatusFailure != null ? Task.FromException(StatusFailure) : Task.CompletedTask;

    public Task<SubtitleText> Subtitles(string url) =>
        Vtt.TryGetValue(url, out var text) ? Task.FromResult(text) : Task.FromException<SubtitleText>(new ApiException(404, "not_found", "no such track"));

    public Task<SubtitlePictures> PictureSubtitles(string url)
    {
        MksAsked.Add(url);
        return Mks.TryGetValue(url.Split('?')[0], out var mks) ? Task.FromResult(mks) : Task.FromException<SubtitlePictures>(MksFailure);
    }

    public Task<TrickplayInfo?> Trickplay(long fileId)
    {
        TrickplayAsks++;
        return Task.FromResult(TrickplayAnswer(fileId));
    }

    public string TrickplaySheetUrl(long fileId, int sheet, string version) => $"{Server}/api/files/{fileId}/trickplay/{sheet}.jpg?v={version}";

    public Task<ItemSummary?> Next(long itemId) => Task.FromResult(NextOf.TryGetValue(itemId, out var next) ? next : null);

    public Task<IReadOnlyList<ItemSummary>> Queue(PlayQueue queue) => Task.FromResult(Queues.TryGetValue(queue is PlayQueue.Playlist p ? p.PlaylistId : ((PlayQueue.Collection)queue).CollectionId, out var items) ? items : new ItemSummary[0]);

    public Task SetStreams(long itemId, long? fileId, int? audio, int? subtitle)
    {
        Streams.Add((itemId, fileId, audio, subtitle));
        return Task.CompletedTask;
    }

    public void TracksChosen(ItemDetail detail)
    {
    }

    public IDisposable OnEvent(Action<ServerEvent> listener)
    {
        listeners.Add(listener);
        return new Off(() => listeners.Remove(listener));
    }

    public void Emit(ServerEvent e)
    {
        foreach (var l in listeners.ToList()) l(e);
    }

    sealed class Off : IDisposable
    {
        readonly Action off;

        public Off(Action off) => this.off = off;

        public void Dispose() => off();
    }
}

/// <summary>A player with its fakes, and the items it plays.</summary>
sealed class PlayerHarness
{
    public readonly FakeScheduler Clock = new FakeScheduler();
    public readonly FakePlayerSource Source = new FakePlayerSource();
    public readonly FakeVideoEngine Engine = new FakeVideoEngine();
    public readonly List<PlayerToast> Toasts = new List<PlayerToast>();
    public PlayerModel Model = null!;

    public static readonly MediaStream Video = new MediaStream { Index = 0, Kind = "video", Codec = "hevc", Width = 1920, Height = 1080, BitDepth = 10, Profile = "Main 10", Default = true };
    public static readonly MediaStream English = new MediaStream { Index = 1, Kind = "audio", Codec = "eac3", Language = "eng", Default = true, Channels = 6 };
    public static readonly MediaStream French = new MediaStream { Index = 2, Kind = "audio", Codec = "ac3", Language = "fre", Channels = 6 };
    public static readonly MediaStream Srt = new MediaStream { Index = 3, Kind = "subtitle", Codec = "subrip", Language = "eng", TextBased = true };
    public static readonly MediaStream Pgs = new MediaStream { Index = 4, Kind = "subtitle", Codec = "hdmv_pgs_subtitle", Language = "fre", TextBased = false };

    public static MediaFile FileOf(long id, long durationMs = 2_700_000) => new MediaFile
    {
        Id = id, Path = $"/m/{id}.mkv", Container = "mkv", Bitrate = 10_000_000, Width = 1920, Height = 1080, DurationMs = durationMs,
        Streams = new[] { Video, English, French, Srt, Pgs },
    };

    public static ItemDetail Episode(long id, int index, long durationMs = 2_700_000, IReadOnlyList<Marker>? markers = null) => new ItemDetail
    {
        Id = id, LibraryId = 1, Kind = "episode", Title = $"Episode {index}", GrandparentTitle = "Ashfall", ParentIndex = 1, Index = index,
        DurationMs = durationMs, Files = new[] { FileOf(id * 10, durationMs) }, Markers = markers,
    };

    public static ItemDetail Movie(long id = 1) => new ItemDetail
    {
        Id = id, LibraryId = 1, Kind = "movie", Title = "Heat", Year = 1995, DurationMs = 2_700_000, Files = new[] { FileOf(id * 10) },
    };

    public PlayerHarness Add(ItemDetail item)
    {
        Source.Items[item.Id] = item;
        return this;
    }

    /// <summary>Starts the player on <paramref name="request"/>; the engine shows the first frame when <paramref name="show"/>.</summary>
    public PlayerModel Start(PlayRequest request, bool show = true)
    {
        Model = new PlayerModel(request, Source, () => Engine, Clock, new InstantReporter());
        Model.Toast += Toasts.Add;
        Model.Start();
        if (show && Engine.Prepared.Count > 0) Engine.Show();
        return Model;
    }

    /// <summary>The playhead moves on by <paramref name="ms"/> while the clock does.</summary>
    public void Play(long ms)
    {
        var until = Clock.Elapsed + ms;
        while (Clock.Elapsed < until)
        {
            var step = Math.Min(PlayerModel.TickMs, until - Clock.Elapsed);
            if (Engine.Playing && Engine.Status == EngineStatus.Ready) Engine.PositionMs += step;
            Clock.Advance(step);
        }
    }

    /// <summary>The media plays to its end (a second short of the server's duration).</summary>
    public void End()
    {
        Engine.PositionMs = Model.Clock.Value.DurationMs - 1_000;
        Engine.Tell(EngineStatus.Ended);
    }

    public IReadOnlyDictionary<string, object?> LastBody => Source.Bodies.Last();

    public PlayerState State => Model.State.Value;
}
