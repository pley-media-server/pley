using System;
using System.Collections.Generic;
using System.Linq;
using System.Text;

namespace Pley.Tv.Core.Subtitles.Pictures;

/// <summary>One sample of a picture track: a display set (PGS, DVB) or a subpicture unit (VobSub).</summary>
public sealed class PictureSample
{
    public PictureSample(long timeMs, long? durationMs, byte[] data)
    {
        TimeMs = timeMs;
        DurationMs = durationMs;
        Data = data;
    }

    public long TimeMs { get; }
    /// <summary>From the block group; null when the sample shows until the next one.</summary>
    public long? DurationMs { get; }
    public byte[] Data { get; }
}

/// <summary>The one track of a <c>.mks</c> file: its codec, its private data, and its samples by time.</summary>
public sealed class PictureTrack
{
    public PictureTrack(string codecId, byte[]? codecPrivate, IReadOnlyList<PictureSample> samples)
    {
        CodecId = codecId;
        CodecPrivate = codecPrivate;
        Samples = samples;
    }

    /// <summary><c>S_HDMV/PGS</c>, <c>S_VOBSUB</c> or <c>S_DVBSUB</c>.</summary>
    public string CodecId { get; }
    public byte[]? CodecPrivate { get; }
    public IReadOnlyList<PictureSample> Samples { get; }
}

/// <summary>What isn't a Matroska file of a picture track this app can read.</summary>
public sealed class MatroskaException : Exception
{
    public MatroskaException(string message) : base(message)
    {
    }
}

/// <summary>
/// The Matroska files the server makes of a picture subtitle track
/// (<c>/api/files/{id}/subtitles/{n}.mks</c>, docs/API.md): one track of PGS, VobSub or DVB,
/// copied as it is, with the original's times from the start of the file. Only what that needs
/// is read: the track's codec and private data, and every block with its time and duration.
/// </summary>
public static class Mks
{
    const long Ebml = 0x1a45dfa3;
    const long Segment = 0x18538067;
    const long Info = 0x1549a966;
    const long TimecodeScale = 0x2ad7b1;
    const long Tracks = 0x1654ae6b;
    const long TrackEntry = 0xae;
    const long TrackNumber = 0xd7;
    const long TrackType = 0x83;
    const long CodecID = 0x86;
    const long CodecPrivate = 0x63a2;
    const long ContentEncodings = 0x6d80;
    const long ContentEncoding = 0x6240;
    const long ContentCompression = 0x5034;
    const long ContentCompAlgo = 0x4254;
    const long ContentCompSettings = 0x4255;
    const long Cluster = 0x1f43b675;
    const long Timecode = 0xe7;
    const long SimpleBlock = 0xa3;
    const long BlockGroup = 0xa0;
    const long Block = 0xa1;
    const long BlockDuration = 0x9b;

    /// <summary>Elements whose children are read; everything else is skipped by its size.</summary>
    static readonly HashSet<long> Masters = new HashSet<long>
    {
        Segment, Info, Tracks, TrackEntry, ContentEncodings, ContentEncoding, ContentCompression, Cluster, BlockGroup,
    };

    /// <summary>The Segment's children: where an element of unknown size inside one ends.</summary>
    static readonly HashSet<long> LevelOne = new HashSet<long> { 0x114d9b74, Info, Tracks, Cluster, 0x1c53bb6b, 0x1043a770, 0x1254c367, 0x1941a469 };

    const long SubtitleTrack = 0x11;

    /// <summary>
    /// The samples read of a track: a film's subtitles have a few thousand, a day's broadcast tens
    /// of thousands. A crafted file of tiny blocks could hold millions, each kept as an object many
    /// times its size.
    /// </summary>
    public const int MaxSamples = 100_000;

    /// <summary>
    /// How deep elements are read: the deepest this reader needs is 6 (a track's compression), and
    /// a crafted file nesting thousands deep would run the stack out, which ends the app.
    /// </summary>
    const int MaxDepth = 8;

    /// <summary>The tracks read: the server's files have one.</summary>
    const int MaxTracks = 64;

    /// <summary>
    /// Reads a <c>.mks</c> file, or as much of one as there is: a file cut short gives the samples
    /// before the cut. Throws <see cref="MatroskaException"/> for anything that isn't one.
    /// </summary>
    public static PictureTrack ReadPictureTrack(byte[] bytes) => new Walker(bytes).Read();

    sealed class Track
    {
        public long Number;
        public long Type;
        public string CodecId = "";
        public byte[]? CodecPrivate;
        /// <summary>Bytes stripped from the start of every frame (ContentCompAlgo 3).</summary>
        public byte[]? Stripped;
        /// <summary>Compressed with something this reader can't undo (zlib).</summary>
        public bool Compressed;
    }

    sealed class RawBlock
    {
        public long Track;
        public long Time;
        public long? Duration;
        public byte[] Data = new byte[0];
    }

    sealed class Walker
    {
        readonly byte[] bytes;
        long scaleNs = 1_000_000;
        readonly List<Track> tracks = new List<Track>();
        readonly List<RawBlock> raw = new List<RawBlock>();
        Track? track;
        long compAlgo;
        byte[]? compSettings;
        long clusterTime;
        RawBlock? groupBlock;
        long? groupDuration;

        public Walker(byte[] bytes) => this.bytes = bytes;

        public PictureTrack Read()
        {
            var header = ElementAt(0, bytes.Length);
            if (header == null || header.Value.Id != Ebml) throw new MatroskaException("not a Matroska file");
            var segmentStart = header.Value.DataStart + (header.Value.Size ?? 0);
            Walk(segmentStart, bytes.Length, false, 0);

            var subtitle = tracks.FirstOrDefault(t => t.Type == SubtitleTrack) ?? tracks.FirstOrDefault();
            if (subtitle == null) throw new MatroskaException("no track");
            if (subtitle.Compressed) throw new MatroskaException("compressed subtitle frames");
            double ToMs(long ticks) => ticks * (double)scaleNs / 1_000_000;
            var samples = new List<PictureSample>();
            foreach (var b in raw)
            {
                if (b.Track != subtitle.Number) continue;
                var data = subtitle.Stripped != null ? Concat(subtitle.Stripped, b.Data) : b.Data;
                samples.Add(new PictureSample(RoundMs(ToMs(b.Time)), b.Duration is long d ? RoundMs(ToMs(d)) : null, data));
            }
            return new PictureTrack(subtitle.CodecId, subtitle.CodecPrivate, samples.OrderBy(s => s.TimeMs).ToList());
        }

        void EndGroup()
        {
            if (groupBlock != null && raw.Count < MaxSamples)
            {
                groupBlock.Duration = groupDuration;
                raw.Add(groupBlock);
            }
            groupBlock = null;
            groupDuration = null;
        }

        /// <summary>Reads the elements from <paramref name="start"/> to <paramref name="end"/>, <paramref name="depth"/> deep; where it stopped (an unknown-size cluster ends at the next level-1 element).</summary>
        long Walk(long start, long end, bool unknownSize, int depth)
        {
            var at = start;
            while (at < end)
            {
                var found = ElementAt(at, end);
                if (found == null) return end;
                var el = found.Value;
                if (unknownSize && LevelOne.Contains(el.Id)) return at;
                // A file still being made (a partial answer, `X-Pley-Complete: 0`)
                // ends inside an element: a block cut short isn't read as a sample.
                if (el.Size is long whole && el.DataStart + whole > bytes.Length && !Masters.Contains(el.Id)) return end;
                var dataEnd = el.Size is long size ? Math.Min(end, el.DataStart + size) : end;
                if (el.Id == Cluster) clusterTime = 0;
                if (el.Id == TrackEntry) track = new Track();
                if (el.Id == ContentCompression)
                {
                    compAlgo = 0;
                    compSettings = null;
                }
                var next = el.Size is long s ? el.DataStart + s : end;
                if (Masters.Contains(el.Id))
                {
                    if (depth < MaxDepth)
                    {
                        var stopped = Walk(el.DataStart, dataEnd, el.Size == null && el.Id != Segment, depth + 1);
                        if (el.Size == null) next = stopped;
                    }
                }
                else
                {
                    // Read in place: only what is kept is copied.
                    var from = (int)el.DataStart;
                    var to = (int)dataEnd;
                    switch (el.Id)
                    {
                        case TimecodeScale:
                            scaleNs = Uint(bytes, from, to) is var scale && scale != 0 ? scale : 1_000_000;
                            break;
                        case TrackNumber:
                            if (track != null) track.Number = Uint(bytes, from, to);
                            break;
                        case TrackType:
                            if (track != null) track.Type = Uint(bytes, from, to);
                            break;
                        case CodecID:
                            if (track != null) track.CodecId = Ascii(bytes, from, to);
                            break;
                        case CodecPrivate:
                            if (track != null) track.CodecPrivate = PictureColors.Slice(bytes, from, to);
                            break;
                        case ContentCompAlgo:
                            compAlgo = Uint(bytes, from, to);
                            break;
                        case ContentCompSettings:
                            compSettings = PictureColors.Slice(bytes, from, to);
                            break;
                        case Timecode:
                            clusterTime = Uint(bytes, from, to);
                            break;
                        case SimpleBlock:
                            if (raw.Count < MaxSamples && BlockOf(bytes, from, to, clusterTime) is RawBlock simple) raw.Add(simple);
                            break;
                        case Block:
                            groupBlock = raw.Count < MaxSamples ? BlockOf(bytes, from, to, clusterTime) : null;
                            break;
                        case BlockDuration:
                            groupDuration = Uint(bytes, from, to);
                            break;
                    }
                }
                if (el.Id == ContentCompression && track != null)
                {
                    if (compAlgo == 3) track.Stripped = compSettings;
                    else track.Compressed = true;
                }
                if (el.Id == TrackEntry && track != null)
                {
                    if (tracks.Count < MaxTracks) tracks.Add(track);
                    track = null;
                }
                if (el.Id == BlockGroup) EndGroup();
                at = next;
            }
            return end;
        }

        Element? ElementAt(long at, long end)
        {
            var id = Vint(bytes, at, false);
            if (id == null || at + id.Value.Length > end) return null;
            var size = Vint(bytes, at + id.Value.Length, true);
            if (size == null) return null;
            var dataStart = at + id.Value.Length + size.Value.Length;
            if (dataStart > end) return null;
            return new Element(id.Value.Value, size.Value.Unknown ? null : size.Value.Value, dataStart);
        }
    }

    /// <param name="Size">null: unknown size (to the end of the parent).</param>
    readonly record struct Element(long Id, long? Size, long DataStart);

    readonly record struct VarInt(long Value, int Length, bool Unknown);

    /// <summary>A block's track, absolute time in ticks and its one frame (subtitles are never laced).</summary>
    static RawBlock? BlockOf(byte[] bytes, int from, int to, long clusterTime)
    {
        // The track number is read within the block's own bytes.
        var v = Vint(bytes, from, true, to);
        if (v == null || v.Value.Length + 3 > to - from) return null;
        var at = from + v.Value.Length;
        var relative = (bytes[at] << 8) | bytes[at + 1];
        var signed = (relative & 0x8000) != 0 ? relative - 0x10000 : relative;
        var flags = bytes[at + 2];
        var lacing = (flags >> 1) & 3;
        var start = at + 3;
        // Laced subtitle frames don't occur in practice: keep the first frame's bytes as they are.
        if (lacing != 0) start = Math.Min(to, start + 1);
        return new RawBlock { Track = v.Value.Value, Time = clusterTime + signed, Data = PictureColors.Slice(bytes, start, to) };
    }

    /// <summary>
    /// An EBML variable-length integer at <paramref name="at"/>: an element id keeps its length
    /// marker (<paramref name="stripMarker"/> false), a size or track number loses it.
    /// </summary>
    static VarInt? Vint(byte[] bytes, long at, bool stripMarker, long? limit = null)
    {
        var end = limit ?? bytes.Length;
        if (at < 0 || at >= end) return null;
        int first = bytes[at];
        if (first == 0) return null;
        var length = 1;
        var mask = 0x80;
        while ((first & mask) == 0)
        {
            mask >>= 1;
            length++;
        }
        if (at + length > end) return null;
        long value = stripMarker ? first & (mask - 1) : first;
        var allOnes = (first & (mask - 1)) == mask - 1;
        for (var i = 1; i < length; i++)
        {
            var b = bytes[at + i];
            if (b != 0xff) allOnes = false;
            value = unchecked(value * 256 + b);
        }
        return new VarInt(value, length, stripMarker && allOnes);
    }

    static long Uint(byte[] bytes, int from, int to)
    {
        long value = 0;
        for (var i = from; i < to; i++) value = unchecked(value * 256 + bytes[i]);
        return value;
    }

    /// <summary>A codec id: 64 characters are far more than any has.</summary>
    static string Ascii(byte[] bytes, int from, int to)
    {
        var s = new StringBuilder();
        for (var i = from; i < to && i - from < 64 && bytes[i] != 0; i++) s.Append((char)bytes[i]);
        return s.ToString();
    }

    static byte[] Concat(byte[] a, byte[] b)
    {
        var joined = new byte[a.Length + b.Length];
        Buffer.BlockCopy(a, 0, joined, 0, a.Length);
        Buffer.BlockCopy(b, 0, joined, a.Length, b.Length);
        return joined;
    }

    /// <summary>Rounded half up, as JavaScript's <c>Math.round</c>.</summary>
    static long RoundMs(double ms) => (long)Math.Floor(ms + 0.5);
}
