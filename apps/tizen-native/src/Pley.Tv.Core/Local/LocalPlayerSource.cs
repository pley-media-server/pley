using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Threading;
using System.Threading.Tasks;
using Pley.Tv.Core.Api;
using Pley.Tv.Core.Net;
using Pley.Tv.Core.Auth;
using Pley.Tv.Core.Player;
using Pley.Tv.Core.Ui;

namespace Pley.Tv.Core.Local;

/// <summary>Why a file on a drive doesn't play: nothing converts it, as a server would.</summary>
public sealed class LocalPlaybackException : Exception
{
    public LocalPlaybackException(string message) : base(message)
    {
    }
}

/// <summary>
/// The player's source for a video on a drive: the file as it is on the TV's own player, its
/// tracks and chapters from its head (<see cref="MatroskaHead"/>), the subtitle files beside it
/// drawn as the server's WebVTT is, and where it stopped kept on the TV. What only a server does
/// (converting, seek previews, the next episode) is never there. The settings, the TV's
/// capabilities and its storage are the app's.
/// </summary>
public sealed class LocalPlayerSource : IPlayerSource
{
    /// <summary>The item id a file on a drive plays as: no server's item is negative.</summary>
    public const long ItemId = -1;

    /// <summary>Played this far through, it was watched: it starts at the beginning next time.</summary>
    const double WatchedShare = 0.95;

    /// <summary>
    /// The head read: Matroska's Info and Tracks come first, within a few KB, and the first frames
    /// (HDR10+'s mark) within a couple of MB after attachments such as fonts.
    /// </summary>
    const int HeadBytes = 2 * 1024 * 1024;

    readonly IPlayerSource app;
    readonly string path;
    /// <summary>What resume points and subtitle offsets are kept by: the file, or a disc image's title in it.</summary>
    readonly string key;
    readonly long fileId;
    /// <summary>A DVD title in the disc image at <see cref="path"/>, served to the player by <see cref="server"/>; null for a plain file.</summary>
    readonly DvdTitle? disc;
    readonly SpanServer? server;
    ItemDetail? detail;
    IReadOnlyList<Sidecar> sidecars = new Sidecar[0];
    LocalFacts? facts;
    /// <summary>Subtitle tracks of the file being read out of it, by stream index: what is read so far, and whether all is.</summary>
    readonly Dictionary<int, Extraction> extractions = new Dictionary<int, Extraction>();

    /// <summary>A subtitle track being read out of the file: its blocks as they come.</summary>
    sealed class Extraction
    {
        public readonly List<SubtitleBlock> Blocks = new List<SubtitleBlock>();
        public Task Done = Task.CompletedTask;
        public MatroskaTrack Track = null!;
        /// <summary>Done when the first block is read: an answer goes then, not after a wait.</summary>
        public TaskCompletionSource<bool> First = new TaskCompletionSource<bool>(TaskCreationOptions.RunContinuationsAsynchronously);
        /// <summary>The whole track is read: its answers say so.</summary>
        public bool Whole;
        /// <summary>The stretches of the film read so far, in ms: where the film is outside them, reading goes there.</summary>
        public readonly List<(long From, long To)> Read = new List<(long, long)>();
        /// <summary>The blocks' starts read, so one read twice is kept once.</summary>
        public readonly HashSet<long> Starts = new HashSet<long>();
        /// <summary>The reading under way, ended when the film jumps away from it.</summary>
        public CancellationTokenSource? Pass;

        /// <summary>Whether the reading has <paramref name="ms"/>: read already, or just ahead of where it is.</summary>
        public bool Covers(long ms)
        {
            lock (Blocks) return Read.Any(r => ms >= r.From - 2_000 && ms <= r.To + 8_000);
        }

        public IReadOnlyList<SubtitleBlock> Now()
        {
            lock (Blocks) return Blocks.OrderBy(b => b.StartMs).ToList();
        }
    }

    /// <param name="app">The app's source, for its settings, capabilities and storage.</param>
    public LocalPlayerSource(IPlayerSource app, string path)
    {
        this.app = app;
        this.path = path;
        key = path;
        fileId = FileIdOf(path);
    }

    /// <summary>A DVD's title in the disc image at <paramref name="image"/>: its cells, read out of the image, served to the player by <paramref name="server"/>.</summary>
    public LocalPlayerSource(IPlayerSource app, string image, DvdTitle title, SpanServer server)
    {
        this.app = app;
        path = image;
        key = DiscKey(image, title.Number);
        fileId = FileIdOf(key);
        disc = title;
        this.server = server;
    }

    /// <summary>What a disc image's title is kept by: the image and the title's number.</summary>
    public static string DiscKey(string image, int title) => $"{image}#title{title}";

    public bool Local => true;

    /// <summary>Subtitle offsets are kept under it, by the file.</summary>
    public string Server => "local";

    public string ClientId => app.ClientId;

    public long? ProfileId => app.ProfileId;

    public IKeyValue Storage => app.Storage;

    public Settings Settings => app.Settings;

    public void UpdateSettings(Func<Settings, Settings> change) => app.UpdateSettings(change);

    /// <summary>Addresses here are the drive's paths, as they are.</summary>
    public string Url(string path) => path;

    public IReadOnlyDictionary<string, object?>? Caps() => app.Caps();

    public ItemDetail? Saved(long id) => detail;

    public Task<ItemDetail> Item(long id) => Task.Run(() => detail ??= Describe());

    /// <summary>A stable id for the file, from its path: subtitle offsets and where it stopped are kept by it.</summary>
    public static long FileIdOf(string path)
    {
        // FNV-1a: string.GetHashCode differs between runs.
        ulong hash = 14695981039346656037;
        foreach (var c in path)
        {
            hash ^= c;
            hash *= 1099511628211;
        }
        return (long)(hash & 0x3fffffffffffffff);
    }

    ItemDetail Describe()
    {
        if (disc != null) return DescribeDisc(disc);
        var info = new FileInfo(path);
        // Matroska's head, else MP4's movie box, whatever the file is called.
        try
        {
            facts = MatroskaHead.Read(ReadAt(0, HeadBytes), ReadAt) ?? Mp4Head.Read(ReadAt, info.Length);
        }
        catch (Exception)
        {
            // Its tracks unknown: it plays with the ones the file starts with.
        }
        // Subtitles in the file are read out of it through its Cues; without them, only the files beside it show.
        var streams = (facts?.Streams ?? new MediaStream[0]).Where(s => s.Kind != "subtitle" || facts?.CuesAt != null).ToList();
        try
        {
            sidecars = LocalFolder.Sidecars(path, Directory.GetFiles(info.DirectoryName ?? "").Select(Path.GetFileName));
        }
        catch (Exception)
        {
            sidecars = new Sidecar[0];
        }
        var first = streams.Count;
        for (var i = 0; i < sidecars.Count; i++)
        {
            var s = sidecars[i];
            streams.Add(new MediaStream
            {
                Index = first + i,
                Kind = "subtitle",
                Codec = s.Codec,
                Language = s.Language,
                Title = s.Title,
                Forced = s.Forced,
                External = true,
                TextBased = true,
            });
        }
        var video = streams.FirstOrDefault(s => s.Kind == "video");
        if (Marks.On) Marks.Note($"local: {(facts == null ? "no Matroska head" : $"{facts.Streams.Count} streams, video {video?.DynamicRange ?? "none"} ({video?.ColorTransfer ?? "?"}, HDR10+ {video?.Hdr10plus == true}), {facts.Chapters.Count} chapters, Cues {(facts.CuesAt != null ? "yes" : "no")}")}");
        var audio = streams.FirstOrDefault(s => s.Kind == "audio");
        var position = Resume();
        return new ItemDetail
        {
            Id = ItemId,
            Kind = "movie",
            Title = LocalFolder.Title(path),
            DurationMs = facts?.DurationMs,
            UserState = new UserState { PositionMs = position },
            Files = new[]
            {
                new MediaFile
                {
                    Id = fileId,
                    Path = path,
                    Size = info.Length,
                    Container = info.Extension.TrimStart('.').ToLowerInvariant(),
                    DurationMs = facts?.DurationMs,
                    Width = video?.Width,
                    Height = video?.Height,
                    VideoCodec = video?.Codec,
                    AudioCodec = audio?.Codec,
                    Streams = streams,
                    Chapters = facts?.Chapters ?? new Chapter[0],
                },
            },
        };
    }

    /// <summary>
    /// A DVD title as an item: the disc's name and the title's number, its length, chapters,
    /// audio and subtitles.
    /// </summary>
    ItemDetail DescribeDisc(DvdTitle t)
    {
        // Its subtitles are read out of the VOBs as they're asked for (DvdSubtitles).
        var streams = t.Streams.ToList();
        var video = streams.FirstOrDefault(s => s.Kind == "video");
        return new ItemDetail
        {
            Id = ItemId,
            Kind = "movie",
            Title = $"{LocalFolder.Title(path)} · Title {t.Number}",
            DurationMs = t.DurationMs,
            UserState = new UserState { PositionMs = Resume() },
            Files = new[]
            {
                new MediaFile
                {
                    Id = fileId,
                    Path = path,
                    Size = t.Bytes,
                    Container = "mpeg",
                    DurationMs = t.DurationMs,
                    Width = video?.Width,
                    Height = video?.Height,
                    VideoCodec = video?.Codec,
                    AudioCodec = streams.FirstOrDefault(s => s.Kind == "audio")?.Codec,
                    Streams = streams,
                    Chapters = t.Chapters,
                },
            },
        };
    }

    byte[] ReadAt(long offset, int count)
    {
        using var f = new FileStream(path, FileMode.Open, FileAccess.Read, FileShare.ReadWrite, 4096);
        if (offset >= f.Length) return new byte[0];
        f.Seek(offset, SeekOrigin.Begin);
        var bytes = new byte[(int)Math.Min(count, f.Length - offset)];
        var read = 0;
        while (read < bytes.Length)
        {
            var n = f.Read(bytes, read, bytes.Length - read);
            if (n == 0) break;
            read += n;
        }
        if (read < bytes.Length) Array.Resize(ref bytes, read);
        return bytes;
    }

    /// <summary>
    /// The file as it is, from where it was asked: what a server would answer for direct play. A
    /// conversion asked for (the TV refused it, or a lower quality) can't be had.
    /// </summary>
    public Task<PlaybackInfo> StartPlayback(IReadOnlyDictionary<string, object?> body)
    {
        var mode = body.TryGetValue("mode", out var m) ? m as string : null;
        if (mode != null && mode != "auto")
            return Task.FromException<PlaybackInfo>(new LocalPlaybackException("This TV can't play this file as it is, and a file on a drive has no server to convert it"));
        var file = detail?.Files.FirstOrDefault();
        var asked = body.TryGetValue("audio_index", out var a) && a is int i ? i : (int?)null;
        return Task.FromResult(new PlaybackInfo
        {
            SessionId = "local",
            Method = "direct",
            // A disc's title is served out of its image, at an address the player opens.
            Url = disc != null && server != null ? server.Serve(path, disc.Spans) : path,
            FileId = fileId,
            DurationMs = file?.DurationMs ?? 0,
            AudioIndex = asked ?? AudioFor(file),
            VideoRange = ShownRange(PlaybackPlan.VideoStream(file)),
            Subtitles = (file?.Streams ?? new MediaStream[0]).Where(s => s.Kind == "subtitle").Select(s => new SubtitleTrack
            {
                Index = s.Index,
                Codec = s.Codec,
                Language = s.Language,
                Title = s.Title,
                Forced = s.Forced,
                External = s.External == true,
                TextBased = s.TextBased != false,
                // A file beside it, read whole; or the file's own track, read out of it.
                Url = s.External == true ? sidecars[s.Index - (file!.Streams.Count - sidecars.Count)].Path : s.TextBased != false ? EmbeddedPrefix + s.Index : null,
            }).ToList(),
        });
    }

    /// <summary>
    /// What the screen gets of the file's range: its own, but Dolby Vision's base layer, as Samsung
    /// TVs show no Dolby Vision (HDR10, HDR10+ with it, HLG for profile 8.4).
    /// </summary>
    static string? ShownRange(MediaStream? video)
    {
        if (video?.DynamicRange != "dolby_vision") return ItemFormat.DynamicRangeLabel(video);
        return video.DvBlCompat == 4 ? "HLG" : video.Hdr10plus == true ? "HDR10+" : video.DvBlCompat is 1 or 6 ? "HDR10" : null;
    }

    /// <summary>The track a server would choose: the default one the TV decodes, else the first it decodes, else the first.</summary>
    int? AudioFor(MediaFile? file)
    {
        var tracks = PlaybackPlan.AudioStreams(file);
        if (tracks.Count == 0) return null;
        var decoded = new HashSet<string>(
            app.Caps() is { } caps && caps.TryGetValue("audio_codecs", out var codecs) && codecs is IEnumerable<string> list ? list : TizenCaps.DecodedAudio);
        return (tracks.FirstOrDefault(t => t.Default && decoded.Contains(t.Codec))
            ?? tracks.FirstOrDefault(t => decoded.Contains(t.Codec))
            ?? tracks[0]).Index;
    }

    public Task StopPlayback(string session) => Task.CompletedTask;

    /// <summary>Where it stopped, kept on the TV; near the end, it was watched and starts over next time.</summary>
    public Task Progress(long itemId, long positionMs, long? durationMs, string state, string session)
    {
        var key = ResumeKey();
        if (durationMs is long d && d > 0 && positionMs >= d * WatchedShare) Storage.Remove(key);
        // "position,length": the folder's line shows how far through it is.
        else if (positionMs > 0) Storage.Set(key, string.Format(System.Globalization.CultureInfo.InvariantCulture, "{0},{1}", positionMs, durationMs ?? 0));
        return Task.CompletedTask;
    }

    /// <summary>Where <paramref name="path"/> stopped last, kept on the TV; 0 when it wasn't, or was watched.</summary>
    public static long ResumeOf(IKeyValue storage, string path) => Kept(storage, path).PositionMs;

    /// <summary>Where <paramref name="path"/> stopped and how long it is, as kept on the TV; zeros when not kept (its length unknown: 0).</summary>
    public static (long PositionMs, long DurationMs) Kept(IKeyValue storage, string path)
    {
        var parts = (storage.Get(KeyOf(path)) ?? "").Split(',');
        var c = System.Globalization.CultureInfo.InvariantCulture;
        long.TryParse(parts[0], System.Globalization.NumberStyles.Integer, c, out var position);
        long duration = 0;
        if (parts.Length > 1) long.TryParse(parts[1], System.Globalization.NumberStyles.Integer, c, out duration);
        return (position, duration);
    }

    long Resume() => ResumeOf(Storage, key);

    string ResumeKey() => KeyOf(key);

    static string KeyOf(string path) => $"local.resume.{FileIdOf(path)}";

    public Task<SessionStatus> PlaybackStatus(string session, int timeoutMs) =>
        Task.FromResult(new SessionStatus { SessionId = session, Method = "direct" });

    public Task Reachable(int timeoutMs) => Task.CompletedTask;

    /// <summary>The address of a text track of the file's own: "embedded:3".</summary>
    const string EmbeddedPrefix = "embedded:";

    /// <summary>How long an answer waits for a track being read out of the file before it gives what it has: less once a block is read.</summary>
    const int ExtractWaitMs = 1_000;

    /// <summary>
    /// A subtitle file beside the video, as WebVTT, whole; or a text track of the file's own, as
    /// much of it as is read out yet (asked again until all is).
    /// </summary>
    public Task<SubtitleText> Subtitles(string url)
    {
        if (!url.StartsWith(EmbeddedPrefix, StringComparison.Ordinal))
            return Task.Run(() => new SubtitleText(LocalFolder.ToWebVtt(LocalFolder.Decode(File.ReadAllBytes(url))), true));
        return Task.Run(async () =>
        {
            var x = Extract(int.Parse(url.Substring(EmbeddedPrefix.Length), System.Globalization.CultureInfo.InvariantCulture));
            await Task.WhenAny(x.Done, x.First.Task, Task.Delay(ExtractWaitMs)).ConfigureAwait(false);
            return new SubtitleText(MatroskaSubtitles.WebVtt(x.Now(), x.Track.CodecId), x.Done.IsCompleted);
        });
    }

    /// <summary>A picture track of the file's own (".../subtitles/4.mks"): as much as is read out yet, handed over as it is.</summary>
    public Task<SubtitlePictures> PictureSubtitles(string url)
    {
        var name = Path.GetFileNameWithoutExtension(url);
        if (!int.TryParse(name, System.Globalization.NumberStyles.Integer, System.Globalization.CultureInfo.InvariantCulture, out var index))
            return Task.FromException<SubtitlePictures>(new LocalPlaybackException("No such subtitle track"));
        return Task.Run(async () =>
        {
            var x = Extract(index);
            // The film is somewhere the reading isn't (a seek): it starts again there.
            if (disc != null && !x.Whole && Position?.Invoke() is long at && !x.Covers(at))
            {
                var nth = disc.Streams.Where(s => s.Kind == "subtitle").ToList().FindIndex(s => s.Index == index);
                if (nth >= 0 && nth < disc.SubpictureIds.Count) ReadDisc(x, disc, disc.SubpictureIds[nth], at);
            }
            await Task.WhenAny(x.Done, x.First.Task, Task.Delay(ExtractWaitMs)).ConfigureAwait(false);
            // The pictures as they are, no .mks made of them.
            var samples = x.Now().Select(b => new Subtitles.Pictures.PictureSample(b.StartMs, b.DurationMs, b.Data)).ToList();
            return new SubtitlePictures(new byte[0], disc != null ? x.Whole : x.Done.IsCompleted)
            {
                Track = new Subtitles.Pictures.PictureTrack(x.Track.CodecId, x.Track.Private, samples),
            };
        });
    }

    /// <summary>Where the film is, for reading what's near it first (a DVD's subtitles); set by the player.</summary>
    public Func<long>? Position { get; set; }

    /// <summary>Ends what is being read out of the file: the player let it go.</summary>
    public void Close() => closing.Cancel();

    readonly CancellationTokenSource closing = new CancellationTokenSource();

    /// <summary>
    /// A DVD title's subtitle stream <paramref name="index"/> read out of its VOBs, as the .mks of a
    /// VobSub track: the subpicture units, the disc's palette and the picture's size.
    /// </summary>
    Extraction ExtractDisc(DvdTitle t, int index)
    {
        var subtitles = t.Streams.Where(s => s.Kind == "subtitle").ToList();
        var nth = subtitles.FindIndex(s => s.Index == index);
        if (nth < 0 || nth >= t.SubpictureIds.Count) throw new LocalPlaybackException("No such subtitle track");
        var x = new Extraction { Track = new MatroskaTrack(index, 1, "S_VOBSUB", System.Text.Encoding.ASCII.GetBytes(t.Idx), null, null) };
        ReadDisc(x, t, t.SubpictureIds[nth], Position?.Invoke() ?? 0);
        return x;
    }

    /// <summary>
    /// Reads stream <paramref name="id"/> of the title from <paramref name="from"/> (the film's
    /// place), ending the reading before: a seek away from what's read starts it again there,
    /// keeping what was read.
    /// </summary>
    void ReadDisc(Extraction x, DvdTitle t, int id, long from)
    {
        x.Pass?.Cancel();
        var pass = CancellationTokenSource.CreateLinkedTokenSource(closing.Token);
        x.Pass = pass;
        x.First = new TaskCompletionSource<bool>(TaskCreationOptions.RunContinuationsAsynchronously);
        var first = x.First;
        var range = -1;
        long last = -1;
        x.Done = Task.Run(() =>
        {
            var took = Marks.On ? DateTime.UtcNow : default;
            using var f = new FileStream(path, FileMode.Open, FileAccess.Read, FileShare.ReadWrite, 4096);
            try
            {
                DvdSubtitles.Read(f, t, id, b =>
                {
                    lock (x.Blocks)
                    {
                        if (!x.Starts.Add(b.StartMs)) return;
                        x.Blocks.Add(b);
                    }
                    if (first.TrySetResult(true) && Marks.On) Marks.Note($"subtitles: the first DVD unit read is at {b.StartMs / 1000} s, the film at {(Position?.Invoke() ?? 0) / 1000} s");
                }, pass.Token, from, ms =>
                {
                    // A stretch read; a new one where the reading jumped back (a cell's start, the title's).
                    lock (x.Blocks)
                    {
                        if (range < 0 || ms < last - 1_000)
                        {
                            x.Read.Add((ms, ms));
                            range = x.Read.Count - 1;
                        }
                        else x.Read[range] = (x.Read[range].From, Math.Max(x.Read[range].To, ms));
                        last = ms;
                    }
                });
                x.Whole = true;
            }
            catch (OperationCanceledException)
            {
                // The film jumped away, or the player let go.
            }
            first.TrySetResult(false);
            if (Marks.On) Marks.Note($"subtitles: DVD stream {id} of title {t.Number} {(x.Whole ? "read" : "left")}, {x.Blocks.Count} units in {(DateTime.UtcNow - took).TotalMilliseconds:0} ms");
        });
    }

    /// <summary>The track <paramref name="index"/> read out of the file, started the first time it's asked for, on one handle to the file.</summary>
    Extraction Extract(int index)
    {
        lock (extractions)
        {
            if (extractions.TryGetValue(index, out var running)) return running;
            if (disc != null) return extractions[index] = ExtractDisc(disc, index);
            var track = facts?.Tracks.FirstOrDefault(t => t.Index == index) ?? throw new LocalPlaybackException("No such subtitle track");
            var x = new Extraction { Track = track };
            x.Done = Task.Run(() =>
            {
                using var f = new FileStream(path, FileMode.Open, FileAccess.Read, FileShare.ReadWrite, 4096);
                byte[] Read(long offset, int count)
                {
                    if (offset >= f.Length) return new byte[0];
                    f.Seek(offset, SeekOrigin.Begin);
                    var bytes = new byte[(int)Math.Min(count, f.Length - offset)];
                    var read = 0;
                    while (read < bytes.Length)
                    {
                        var n = f.Read(bytes, read, bytes.Length - read);
                        if (n == 0) break;
                        read += n;
                    }
                    if (read < bytes.Length) Array.Resize(ref bytes, read);
                    return bytes;
                }
                var took = Marks.On ? DateTime.UtcNow : default;
                MatroskaSubtitles.Read(Read, facts!, track, b =>
                {
                    lock (x.Blocks) x.Blocks.Add(b);
                    x.First.TrySetResult(true);
                }, closing.Token);
                if (Marks.On) Marks.Note($"subtitles: track {index} read out of the file, {x.Blocks.Count} blocks in {(DateTime.UtcNow - took).TotalMilliseconds:0} ms");
            });
            extractions[index] = x;
            return x;
        }
    }


    public Task<TrickplayInfo?> Trickplay(long fileId) => Task.FromResult<TrickplayInfo?>(null);

    public string TrickplaySheetUrl(long fileId, int sheet, string version) => "";

    public Task<ItemSummary?> Next(long itemId) => Task.FromResult<ItemSummary?>(null);

    public Task<IReadOnlyList<ItemSummary>> Queue(PlayQueue queue) => Task.FromResult<IReadOnlyList<ItemSummary>>(new ItemSummary[0]);

    public Task SetStreams(long itemId, long? fileId, int? audio, int? subtitle) => Task.CompletedTask;

    public void TracksChosen(ItemDetail detail)
    {
    }

    public IDisposable OnEvent(Action<ServerEvent> listener) => new Nothing();

    sealed class Nothing : IDisposable
    {
        public void Dispose()
        {
        }
    }
}
