using System;
using System.Collections.Generic;
using System.Linq;
using System.Text;
using Pley.Tv.Core.Api;

namespace Pley.Tv.Core.Local;

/// <summary>
/// Reads an MP4 or QuickTime file's movie box (moov) for a file played from a drive, as
/// <see cref="MatroskaHead"/> reads a Matroska file's head: its length, its tracks as ffprobe
/// names them (codec, size, color, Dolby Vision, language, channels, Atmos) and its chapters
/// (Nero's chpl). The moov is found box by box, at the start or the end of the file.
/// </summary>
public static class Mp4Head
{
    /// <summary>The most of a moov read: a film's sample tables are a few MB.</summary>
    const int MoovMost = 32 * 1024 * 1024;
    /// <summary>The first frames read for HDR10+'s mark.</summary>
    const int FramesMost = 4 * 1024 * 1024;

    /// <summary>The facts of the file <paramref name="read"/> gives (offset, count), <paramref name="length"/> bytes long; null when it isn't MP4.</summary>
    public static LocalFacts? Read(Func<long, int, byte[]> read, long length)
    {
        long at = 0;
        byte[]? moov = null;
        var boxes = 0;
        while (at + 8 <= length && boxes++ < 1000)
        {
            var head = read(at, 16);
            if (head.Length < 8) break;
            long size = U32(head, 0);
            var type = Encoding.ASCII.GetString(head, 4, 4);
            var header = 8;
            if (size == 1 && head.Length >= 16)
            {
                size = (long)U64(head, 8);
                header = 16;
            }
            else if (size == 0) size = length - at;
            if (size < header) break;
            if (boxes == 1 && type != "ftyp" && type != "moov" && type != "free" && type != "wide" && type != "mdat") return null;
            if (type == "moov")
            {
                if (size > MoovMost) return null;
                moov = read(at + header, (int)(size - header));
                break;
            }
            at += size;
        }
        if (moov == null) return null;

        long timescale = 1000, duration = 0;
        var streams = new List<MediaStream>();
        var chapters = new List<Chapter>();
        long? firstVideoChunk = null;
        foreach (var (type, from, to) in Boxes(moov, 0, moov.Length))
        {
            if (type == "mvhd")
            {
                var v = moov[from];
                timescale = U32(moov, from + (v == 1 ? 20 : 12));
                duration = v == 1 ? (long)U64(moov, from + 24) : U32(moov, from + 16);
            }
            else if (type == "trak" && Track(moov, from, to, streams.Count) is { } t)
            {
                streams.Add(t.Stream);
                if (t.Stream.Kind == "video" && firstVideoChunk == null) firstVideoChunk = t.FirstChunk;
            }
            else if (type == "udta") chapters = Chapters(moov, from, to);
        }
        var durationMs = timescale > 0 && duration > 0 ? duration * 1000 / timescale : (long?)null;
        // What the frames say of the picture, which the movie box may leave out: from the first chunk.
        var video = streams.FindIndex(s => s.Kind == "video" && s.Codec == "hevc");
        if (video >= 0 && firstVideoChunk is long chunk) streams[video] = HevcFrames.Apply(streams[video], HevcFrames.Scan(read(chunk, FramesMost)));
        for (var i = 0; i < chapters.Count; i++)
            chapters[i] = chapters[i] with { EndMs = i + 1 < chapters.Count ? chapters[i + 1].StartMs : durationMs ?? chapters[i].StartMs };
        // The first of each kind is the one a file starts with.
        foreach (var kind in new[] { "video", "audio" })
        {
            var first = streams.FindIndex(s => s.Kind == kind);
            if (first >= 0) streams[first] = streams[first] with { Default = true };
        }
        return new LocalFacts(durationMs, streams, chapters);
    }

    sealed record Found(MediaStream Stream, long? FirstChunk);

    static Found? Track(byte[] b, int from, int to, int index)
    {
        string? handler = null, codec = null, language = null, name = null;
        int? width = null, height = null, channels = null, rate = null;
        long transfer = 2, primaries = 2, matrix = 2;
        int? dvProfile = null, dvLevel = null, dvCompat = null;
        var atmos = false;
        long? chunk = null;
        void Walk(int f, int t)
        {
            foreach (var (type, s, e) in Boxes(b, f, t))
            {
                switch (type)
                {
                    case "mdia":
                    case "minf":
                    case "stbl":
                        Walk(s, e);
                        break;
                    case "mdhd":
                        // Its language: three letters, five bits each, after the times.
                        var v = b[s];
                        var at = s + (v == 1 ? 32 : 20);
                        if (at + 2 <= e)
                        {
                            var packed = (b[at] << 8) | b[at + 1];
                            var code = new string(new[] { (char)(((packed >> 10) & 31) + 0x60), (char)(((packed >> 5) & 31) + 0x60), (char)((packed & 31) + 0x60) });
                            if (code != "und" && code.All(char.IsLetter)) language = code;
                        }
                        break;
                    case "hdlr":
                        if (s + 12 <= e) handler = Encoding.ASCII.GetString(b, s + 8, 4);
                        if (e - s > 24)
                        {
                            var label = Encoding.UTF8.GetString(b, s + 24, e - s - 24).Trim('\0', ' ');
                            // Muxers' own labels ("SoundHandler", "VideoHandler") aren't titles.
                            if (label.Length > 0 && !label.EndsWith("Handler", StringComparison.Ordinal) && !label.Contains("Media Handler")) name = label;
                        }
                        break;
                    case "stsd":
                        if (s + 16 > e) break;
                        var entry = s + 8;
                        var entrySize = (int)U32(b, entry);
                        codec = Encoding.ASCII.GetString(b, entry + 4, 4);
                        var entryEnd = Math.Min(e, entry + entrySize);
                        if (handler == "vide" && entry + 8 + 78 <= entryEnd)
                        {
                            width = (b[entry + 32] << 8) | b[entry + 33];
                            height = (b[entry + 34] << 8) | b[entry + 35];
                            foreach (var (ct, cs, ce) in Boxes(b, entry + 86, entryEnd))
                            {
                                if (ct == "colr" && cs + 10 <= ce && Encoding.ASCII.GetString(b, cs, 4) == "nclx")
                                {
                                    primaries = (b[cs + 4] << 8) | b[cs + 5];
                                    transfer = (b[cs + 6] << 8) | b[cs + 7];
                                    matrix = (b[cs + 8] << 8) | b[cs + 9];
                                }
                                else if ((ct == "dvcC" || ct == "dvvC") && cs + 5 <= ce)
                                {
                                    dvProfile = (b[cs + 2] >> 1) & 0x7f;
                                    dvLevel = ((b[cs + 2] & 1) << 5) | (b[cs + 3] >> 3);
                                    dvCompat = b[cs + 4] >> 4;
                                }
                            }
                        }
                        else if (handler == "soun" && entry + 8 + 28 <= entryEnd)
                        {
                            channels = (b[entry + 24] << 8) | b[entry + 25];
                            rate = (b[entry + 32] << 8) | b[entry + 33];
                            foreach (var (ct, cs, ce) in Boxes(b, entry + 36, entryEnd))
                            {
                                if (ct == "dec3") atmos = Joc(b, cs, ce);
                                else if (ct == "dac3" && cs + 3 <= ce) channels = Ac3Channels(b[cs + 1] >> 3 & 7, b[cs + 1] >> 2 & 1);
                            }
                        }
                        break;
                    case "stco":
                        if (chunk == null && s + 12 <= e && U32(b, s + 4) > 0) chunk = U32(b, s + 8);
                        break;
                    case "co64":
                        if (chunk == null && s + 16 <= e && U32(b, s + 4) > 0) chunk = (long)U64(b, s + 8);
                        break;
                }
            }
        }
        Walk(from, to);
        var kind = handler switch { "vide" => "video", "soun" => "audio", "sbtl" or "text" or "subt" => "subtitle", _ => null };
        if (kind == null || codec == null) return null;
        var ffmpeg = CodecName(codec);
        if (kind == "video" && dvProfile != null && transfer == 2) transfer = dvCompat switch { 1 or 6 => 16, 4 => 18, _ => 2 };
        var range = kind != "video" ? null
            : dvProfile != null ? "dolby_vision"
            : transfer == 16 ? "hdr10"
            : transfer == 18 ? "hlg"
            : "sdr";
        return new Found(new MediaStream
        {
            Index = index,
            Kind = kind,
            Codec = ffmpeg,
            Language = language,
            Title = name,
            Width = width,
            Height = height,
            Channels = channels,
            SampleRate = rate > 0 ? rate : null,
            Profile = atmos ? "Dolby Digital Plus + Dolby Atmos" : null,
            DynamicRange = range,
            Hdr = kind == "video" ? range != "sdr" : null,
            DvProfile = dvProfile,
            DvLevel = dvLevel,
            DvBlCompat = dvCompat,
            ColorTransfer = kind != "video" ? null : transfer switch { 16 => "smpte2084", 18 => "arib-std-b67", 1 => "bt709", _ => null },
            ColorPrimaries = kind != "video" ? null : primaries switch { 9 => "bt2020", 1 => "bt709", _ => null },
            ColorSpace = kind != "video" ? null : matrix switch { 9 => "bt2020nc", 1 => "bt709", _ => null },
            TextBased = kind == "subtitle" ? true : null,
        }, chunk);
    }

    /// <summary>E-AC-3's dec3 box: whether it carries Atmos (JOC), the flag after its substreams.</summary>
    static bool Joc(byte[] b, int from, int to)
    {
        if (from + 2 > to) return false;
        var substreams = (b[from + 1] & 7) + 1;
        var at = from + 2;
        for (var i = 0; i < substreams && at + 3 <= to; i++)
        {
            var dependents = (b[at + 2] >> 1) & 15;
            at += dependents > 0 ? 4 : 3;
        }
        return at + 2 <= to && (b[at] & 1) == 1;
    }

    static int Ac3Channels(int acmod, int lfe) => new[] { 2, 1, 2, 3, 3, 4, 4, 5 }[acmod] + lfe;

    /// <summary>Nero's chapters (udta/chpl): each start, in 100 ns, and its title.</summary>
    static List<Chapter> Chapters(byte[] b, int from, int to)
    {
        var chapters = new List<Chapter>();
        foreach (var (type, s, e) in Boxes(b, from, to))
        {
            if (type != "chpl" || s + 9 > e) continue;
            var version = b[s];
            var at = s + 4 + (version == 1 ? 4 : 0);
            int count = b[at++];
            for (var i = 0; i < count && at + 9 <= e; i++)
            {
                var start = (long)U64(b, at) / 10_000;
                int length = b[at + 8];
                at += 9;
                if (at + length > e) break;
                var title = Encoding.UTF8.GetString(b, at, length);
                at += length;
                chapters.Add(new Chapter { StartMs = start, Title = string.IsNullOrWhiteSpace(title) ? null : title });
            }
        }
        chapters.Sort((x, y) => x.StartMs.CompareTo(y.StartMs));
        return chapters;
    }

    /// <summary>An MP4 sample entry's code as ffprobe names the codec.</summary>
    public static string CodecName(string fourcc) => fourcc switch
    {
        "hvc1" or "hev1" or "dvh1" or "dvhe" => "hevc",
        "avc1" or "avc3" or "dva1" or "dvav" => "h264",
        "av01" or "dav1" => "av1",
        "vp09" => "vp9",
        "mp4v" => "mpeg4",
        "mp4a" => "aac",
        "ac-3" => "ac3",
        "ec-3" => "eac3",
        "ac-4" => "ac4",
        "Opus" => "opus",
        "fLaC" => "flac",
        ".mp3" => "mp3",
        "dtsc" or "dtsh" or "dtsl" or "dtse" => "dts",
        "mlpa" => "truehd",
        "tx3g" => "mov_text",
        "wvtt" => "webvtt",
        _ => fourcc.Trim().ToLowerInvariant(),
    };

    /// <summary>The boxes between <paramref name="from"/> and <paramref name="to"/>: each one's type and its content's bounds.</summary>
    static IEnumerable<(string Type, int From, int To)> Boxes(byte[] b, int from, int to)
    {
        var at = from;
        while (at + 8 <= to)
        {
            long size = U32(b, at);
            var header = 8;
            if (size == 1 && at + 16 <= to)
            {
                size = (long)U64(b, at + 8);
                header = 16;
            }
            else if (size == 0) size = to - at;
            if (size < header || at + size > to) yield break;
            yield return (Encoding.ASCII.GetString(b, at + 4, 4), at + header, (int)(at + size));
            at += (int)size;
        }
    }

    static long U32(byte[] b, int at) => at + 4 <= b.Length ? ((long)b[at] << 24) | ((long)b[at + 1] << 16) | ((long)b[at + 2] << 8) | b[at + 3] : 0;

    static ulong U64(byte[] b, int at) => at + 8 <= b.Length ? ((ulong)U32(b, at) << 32) | (ulong)U32(b, at + 4) : 0;
}
