using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Text;
using System.Threading.Tasks;
using Pley.Tv.Core.Api;
using Pley.Tv.Core.Auth;
using Pley.Tv.Core.Local;
using Pley.Tv.Core.Player;
using Pley.Tv.Core.Ui;
using Xunit;

namespace Pley.Tv.Core.Tests;

/// <summary>Videos on a drive plugged into the TV: their folders, heads, subtitle files and resume points.</summary>
public class LocalTests
{
    // ---- a Matroska file, built element by element

    static byte[] El(long id, params byte[][] children) => El(id, children.SelectMany(c => c).ToArray());

    static byte[] El(long id, byte[] data)
    {
        var idBytes = new List<byte>();
        for (var v = id; v > 0; v >>= 8) idBytes.Insert(0, (byte)(v & 0xff));
        // An 8-byte size: any length fits.
        var size = new byte[8];
        size[0] = 0x01;
        var n = (long)data.Length;
        for (var i = 7; i >= 1; i--, n >>= 8) size[i] = (byte)(n & 0xff);
        return idBytes.Concat(size).Concat(data).ToArray();
    }

    static byte[] U(long v) => BitConverter.GetBytes(v).Reverse().ToArray();

    static byte[] F(double v) => BitConverter.GetBytes(v).Reverse().ToArray();

    static byte[] S(string s) => Encoding.UTF8.GetBytes(s);

    static byte[] Track(long type, string codec, string? language = null, string? name = null, bool isDefault = true, byte[]? extra = null)
    {
        var parts = new List<byte[]> { El(0xd7, U(1)), El(0x83, U(type)), El(0x86, S(codec)), El(0x88, U(isDefault ? 1 : 0)) };
        if (language != null) parts.Add(El(0x22b59c, S(language)));
        if (name != null) parts.Add(El(0x536e, S(name)));
        if (extra != null) parts.Add(extra);
        return El(0xae, parts.ToArray());
    }

    static byte[] Film(bool chaptersAtEnd = false, bool hdr = false)
    {
        var ebml = El(0x1a45dfa3, El(0x4282, S("matroska")));
        var info = El(0x1549a966, El(0x2ad7b1, U(1_000_000)), El(0x4489, F(7_380_000)));
        var tracks = El(0x1654ae6b,
            Track(1, "V_MPEGH/ISO/HEVC", extra: El(0xe0, El(0xb0, U(1920)), El(0xba, U(800)),
                hdr ? El(0x55b0, El(0x55ba, U(16)), El(0x55bb, U(9)), El(0x55b2, U(10))) : new byte[0])),
            Track(2, "A_TRUEHD", "eng", "Atmos", extra: El(0xe1, El(0x9f, U(8)))),
            Track(2, "A_AC3", "eng", "Compatibility", isDefault: false, extra: El(0xe1, El(0x9f, U(6)))),
            Track(2, "A_AAC", "rus", isDefault: false),
            Track(0x11, "S_TEXT/UTF8", "eng"));
        var chapters = El(0x1043a770, El(0x45b9,
            El(0xb6, El(0x91, U(0)), El(0x80, El(0x85, S("Opening")))),
            El(0xb6, El(0x91, U(600_000_000_000)), El(0x80, El(0x85, S("The Road"))))));
        // A frame with HDR10+'s metadata mark in it.
        var cluster = El(0x1f43b675, El(0xe7, U(0)), hdr ? El(0xa3, Hdr10PlusFrame()) : new byte[0]);
        if (!chaptersAtEnd) return ebml.Concat(El(0x18538067, info, tracks, chapters, cluster)).ToArray();
        // FFmpeg's way: Chapters after the clusters, found through the SeekHead.
        var before = new List<byte[]> { info, tracks, cluster };
        // The SeekHead's own size is fixed (8-byte sizes), so the chapters' offset can be worked out first.
        byte[] SeekHead(long at) => El(0x114d9b74, El(0x4dbb, El(0x53ab, U(0x1043a770)), El(0x53ac, U(at))));
        var head = SeekHead(0);
        var at = head.Length + before.Sum(b => b.Length);
        return ebml.Concat(El(0x18538067, SeekHead(at), info, tracks, cluster, chapters)).ToArray();
    }

    /// <summary>A SimpleBlock of track 1 holding one HEVC unit: a prefix SEI with HDR10+'s T.35 message.</summary>
    static byte[] Hdr10PlusFrame()
    {
        var sei = new byte[] { 0x4e, 0x01, 0x04, 0x08, 0xb5, 0x00, 0x3c, 0x00, 0x01, 0x04, 0x01, 0x40, 0x80 };
        var unit = BitConverter.GetBytes(sei.Length).Reverse().Concat(sei).ToArray();
        return new byte[] { 0x81, 0, 0, 0x80 }.Concat(unit).ToArray();
    }

    static LocalFacts Read(byte[] file, int headBytes) =>
        MatroskaHead.Read(file.Take(headBytes).ToArray(), (offset, count) => file.Skip((int)offset).Take(count).ToArray())!;

    [Fact]
    public void A_Matroska_head_gives_its_length_tracks_and_chapters()
    {
        var facts = Read(Film(), int.MaxValue);
        Assert.Equal(7_380_000, facts.DurationMs);
        Assert.Equal(new[] { "video", "audio", "audio", "audio", "subtitle" }, facts.Streams.Select(s => s.Kind));
        Assert.Equal(new[] { "hevc", "truehd", "ac3", "aac", "subrip" }, facts.Streams.Select(s => s.Codec));
        Assert.Equal(Enumerable.Range(0, 5), facts.Streams.Select(s => s.Index));
        Assert.Equal((1920, 800), (facts.Streams[0].Width, facts.Streams[0].Height));
        Assert.Equal(8, facts.Streams[1].Channels);
        Assert.Equal("Atmos", facts.Streams[1].Title);
        Assert.Equal(new[] { true, false, false }, facts.Streams.Where(s => s.Kind == "audio").Select(s => s.Default));
        Assert.Equal("rus", facts.Streams[3].Language);
        Assert.Equal(new[] { "Opening", "The Road" }, facts.Chapters.Select(c => c.Title));
        // A chapter without its end ends at the next one's start, the last at the film's end.
        Assert.Equal(new[] { (0L, 600_000L), (600_000L, 7_380_000L) }, facts.Chapters.Select(c => (c.StartMs, c.EndMs)));
        Assert.Equal("sdr", facts.Streams[0].DynamicRange);
    }

    [Fact]
    public void A_Matroska_head_says_HDR10_and_HDR10plus()
    {
        var facts = Read(Film(hdr: true), int.MaxValue);
        var video = facts.Streams[0];
        Assert.Equal(("hdr10plus", "smpte2084", "bt2020", 10, true), (video.DynamicRange, video.ColorTransfer, video.ColorPrimaries, video.BitDepth ?? 0, video.Hdr10plus ?? false));
        Assert.Equal("HDR10+", ItemFormat.DynamicRangeLabel(video));
    }

    [Fact]
    public void Chapters_after_the_clusters_are_found_through_the_SeekHead()
    {
        var film = Film(chaptersAtEnd: true);
        // The head stops before the chapters.
        var facts = Read(film, film.Length - 60);
        Assert.Equal(new[] { "Opening", "The Road" }, facts.Chapters.Select(c => c.Title));
        Assert.Equal(5, facts.Streams.Count);
    }

    [Fact]
    public void A_file_that_isnt_Matroska_reads_as_nothing()
    {
        var mp4 = Encoding.ASCII.GetBytes("\0\0\0\x20ftypisom");
        Assert.Null(MatroskaHead.Read(mp4, (_, _) => new byte[0]));
    }

    /// <summary>A film with a text track (number 5) of two subtitles, in two clusters the Cues list.</summary>
    static byte[] Subtitled(string codec = "S_TEXT/UTF8", byte[]? first = null)
    {
        var ebml = El(0x1a45dfa3, El(0x4282, S("matroska")));
        var info = El(0x1549a966, El(0x2ad7b1, U(1_000_000)), El(0x4489, F(60_000)));
        var tracks = El(0x1654ae6b, El(0xae, El(0xd7, U(5)), El(0x83, U(0x11)), El(0x86, S(codec)), El(0x22b59c, S("eng"))));
        // Block: track 5 (0x85), a relative time, flags, the text.
        byte[] Block(short rel, byte[] text) => new byte[] { 0x85, (byte)(rel >> 8), (byte)rel, 0 }.Concat(text).ToArray();
        var timecode1 = El(0xe7, U(1_000));
        var cluster1 = El(0x1f43b675, timecode1, El(0xa0, El(0xa1, Block(500, first ?? S("Hello"))), El(0x9b, U(2_000))));
        var cluster2 = El(0x1f43b675, El(0xe7, U(10_000)), El(0xa3, Block(0, S("There"))));
        byte[] SeekHead(long cues) => El(0x114d9b74, El(0x4dbb, El(0x53ab, U(0x1c53bb6b)), El(0x53ac, U(cues))));
        var seekLength = SeekHead(0).Length;
        long c1 = seekLength + info.Length + tracks.Length, c2 = c1 + cluster1.Length, cuesAt = c2 + cluster2.Length;
        // Each block sits after its cluster's timecode.
        var rel = (long)timecode1.Length;
        byte[] Point(long time, long cluster) => El(0xbb, El(0xb3, U(time)), El(0xb7, El(0xf7, U(5)), El(0xf1, U(cluster)), El(0xf0, U(rel))));
        var cues = El(0x1c53bb6b, Point(1_500, c1), Point(10_000, c2));
        return ebml.Concat(El(0x18538067, SeekHead(cuesAt), info, tracks, cluster1, cluster2, cues)).ToArray();
    }

    [Fact]
    public void A_text_track_is_read_out_of_the_file_through_its_Cues()
    {
        var film = Subtitled();
        var facts = Read(film, int.MaxValue);
        Assert.NotNull(facts.CuesAt);
        var track = facts.Tracks.Single();
        Assert.Equal((5L, "S_TEXT/UTF8"), (track.Number, track.CodecId));
        var blocks = MatroskaSubtitles.Read((o, n) => film.Skip((int)o).Take(n).ToArray(), facts, track);
        Assert.Equal(new[] { (1_500L, (long?)2_000L), (10_000L, null) }, blocks.Select(b => (b.StartMs, b.DurationMs)));
        var cues = WebVtt.Parse(MatroskaSubtitles.WebVtt(blocks, track.CodecId));
        Assert.Equal(new[] { ("Hello", 1_500L, 3_500L), ("There", 10_000L, 14_000L) }, cues.Select(c => (c.Text, c.StartMs, c.EndMs)));
    }

    [Fact]
    public void An_ASS_track_loses_its_fields_and_styling()
    {
        var film = Subtitled("S_TEXT/ASS", S(@"1,0,Default,,0,0,0,,{\i1}Hello{\i0}\Nthere"));
        var facts = Read(film, int.MaxValue);
        var blocks = MatroskaSubtitles.Read((o, n) => film.Skip((int)o).Take(n).ToArray(), facts, facts.Tracks.Single());
        Assert.Equal("Hello\nthere", WebVtt.Parse(MatroskaSubtitles.WebVtt(blocks, "S_TEXT/ASS"))[0].Text);
    }

    [Fact]
    public void A_picture_tracks_blocks_are_read_out_as_they_are()
    {
        var film = Subtitled("S_HDMV/PGS", new byte[] { 0x16, 0x00, 0x00 });
        var facts = Read(film, int.MaxValue);
        Assert.Equal("hdmv_pgs_subtitle", facts.Streams.Single().Codec);
        var blocks = MatroskaSubtitles.Read((o, n) => film.Skip((int)o).Take(n).ToArray(), facts, facts.Tracks.Single());
        Assert.Equal(new byte[] { 0x16, 0x00, 0x00 }, blocks[0].Data);
        Assert.Equal(new[] { 1_500L, 10_000L }, blocks.Select(b => b.StartMs));
    }

    [Fact]
    public void A_Dolby_Vision_file_without_its_colour_says_HDR10plus_when_its_frames_carry_it()
    {
        // Profile 8.1 (dvcC: version 1.0, profile 8 level 6, base layer 1), no Colour element, HDR10+ in the first frame.
        var dvcc = new byte[] { 1, 0, (8 << 1) | 0, 6 << 3, 1 << 4, 0, 0, 0 };
        var ebml = El(0x1a45dfa3, El(0x4282, S("matroska")));
        var tracks = El(0x1654ae6b, El(0xae, El(0xd7, U(1)), El(0x83, U(1)), El(0x86, S("V_MPEGH/ISO/HEVC")),
            El(0x41e4, El(0x41e7, U(0x64766343)), El(0x41ed, dvcc))));
        var cluster = El(0x1f43b675, El(0xe7, U(0)), El(0xa3, Hdr10PlusFrame()));
        var film = ebml.Concat(El(0x18538067, tracks, cluster)).ToArray();
        var video = Read(film, int.MaxValue).Streams.Single();
        Assert.Equal(("dolby_vision", 8, 1, "smpte2084", true), (video.DynamicRange, video.DvProfile ?? 0, video.DvBlCompat ?? 0, video.ColorTransfer, video.Hdr10plus ?? false));
    }

    // ---- an MP4, box by box

    static byte[] Box(string type, params byte[][] content)
    {
        var body = content.SelectMany(c => c).ToArray();
        var size = BitConverter.GetBytes(body.Length + 8).Reverse();
        return size.Concat(Encoding.ASCII.GetBytes(type)).Concat(body).ToArray();
    }

    static byte[] Be(int v, int bytes) => BitConverter.GetBytes(v).Reverse().Skip(4 - bytes).ToArray();

    static byte[] Mp4()
    {
        var ftyp = Box("ftyp", S("isom"), Be(0, 4));
        var mvhd = Box("mvhd", Be(0, 4), Be(0, 4), Be(0, 4), Be(1000, 4), Be(6_000_000, 4), new byte[80]);
        byte[] Mdhd(string lang)
        {
            var packed = ((lang[0] - 0x60) << 10) | ((lang[1] - 0x60) << 5) | (lang[2] - 0x60);
            return Box("mdhd", Be(0, 4), Be(0, 4), Be(0, 4), Be(1000, 4), Be(6_000_000, 4), Be(packed, 2), Be(0, 2));
        }
        byte[] Hdlr(string type) => Box("hdlr", Be(0, 4), Be(0, 4), S(type), new byte[12], new byte[] { 0 });
        // A video entry: 8 reserved and reference, 16 more, its size, 50 more, then its boxes.
        var hvc1 = Box("hvc1", new byte[8], new byte[16], Be(3840, 2), Be(1600, 2), new byte[50],
            Box("colr", S("nclx"), Be(9, 2), Be(16, 2), Be(9, 2), new byte[] { 0 }));
        var video = Box("trak", Box("mdia", Mdhd("und"), Hdlr("vide"), Box("minf", Box("stbl", Box("stsd", Be(0, 4), Be(1, 4), hvc1)))));
        // An audio entry: E-AC-3 5.1 with Atmos (dec3: one substream, no dependents, then the JOC flag).
        var ec3 = Box("ec-3", new byte[8], new byte[8], Be(6, 2), Be(16, 2), new byte[4], Be(48000, 2), Be(0, 2),
            Box("dec3", new byte[] { 0x0c, 0x00, 0x20, 0x0f, 0x00, 0x01, 0x10 }));
        var audio = Box("trak", Box("mdia", Mdhd("eng"), Hdlr("soun"), Box("minf", Box("stbl", Box("stsd", Be(0, 4), Be(1, 4), ec3)))));
        // Nero's chapters: two, at 0 and 10 minutes, in 100 ns.
        byte[] Chapter(long start, string title) => BitConverter.GetBytes(start * 10_000).Reverse().Concat(new[] { (byte)title.Length }).Concat(S(title)).ToArray();
        var udta = Box("udta", Box("chpl", new byte[] { 1, 0, 0, 0 }, new byte[4], new byte[] { 2 }, Chapter(0, "Andy's Room"), Chapter(600_000, "Pizza Planet")));
        var mdat = Box("mdat", new byte[64]);
        // Not fast-started: the movie box after the media.
        return ftyp.Concat(mdat).Concat(Box("moov", mvhd, video, audio, udta)).ToArray();
    }

    [Fact]
    public void An_MP4_movie_box_gives_its_length_tracks_HDR_Atmos_and_chapters()
    {
        var file = Mp4();
        var facts = Mp4Head.Read((o, n) => file.Skip((int)o).Take(n).ToArray(), file.Length)!;
        Assert.Equal(6_000_000, facts.DurationMs);
        Assert.Equal(new[] { "video", "audio" }, facts.Streams.Select(s => s.Kind));
        var video = facts.Streams[0];
        Assert.Equal(("hevc", 3840, 1600, "hdr10", "smpte2084", "bt2020"), (video.Codec, video.Width ?? 0, video.Height ?? 0, video.DynamicRange, video.ColorTransfer, video.ColorPrimaries));
        var sound = facts.Streams[1];
        Assert.Equal(("eac3", "eng", 6, true), (sound.Codec, sound.Language, sound.Channels ?? 0, ItemFormat.IsAtmos(sound)));
        Assert.Equal(new[] { ("Andy's Room", 0L, 600_000L), ("Pizza Planet", 600_000L, 6_000_000L) }, facts.Chapters.Select(c => (c.Title ?? "", c.StartMs, c.EndMs)));
    }

    // ---- folders

    [Fact]
    public void Names_sort_as_a_person_sorts_them()
    {
        var names = new List<string> { "Part 10.mkv", "part 2.mkv", "Part 1.mkv", "Alpha.mkv" };
        names.Sort(LocalFolder.Natural);
        Assert.Equal(new[] { "Alpha.mkv", "Part 1.mkv", "part 2.mkv", "Part 10.mkv" }, names);
    }

    [Theory]
    [InlineData("$RECYCLE.BIN", false)]
    [InlineData(".device_info_8cdc30de", false)]
    [InlineData("System Volume Information", false)]
    [InlineData("Ad Astra", true)]
    public void Hidden_and_system_entries_are_left_out(string name, bool shown) => Assert.Equal(shown, LocalFolder.Shown(name));

    [Fact]
    public void A_folder_lists_its_folders_then_its_videos()
    {
        var dir = Directory.CreateDirectory(Path.Combine(Path.GetTempPath(), "pley-local-" + Guid.NewGuid().ToString("N"))).FullName;
        try
        {
            Directory.CreateDirectory(Path.Combine(dir, "Season 10"));
            Directory.CreateDirectory(Path.Combine(dir, "Season 2"));
            Directory.CreateDirectory(Path.Combine(dir, "$RECYCLE.BIN"));
            foreach (var f in new[] { "Film.mkv", "Film.en.srt", "notes.txt", ".hidden.mkv", "Clip.MP4" }) File.WriteAllText(Path.Combine(dir, f), "x");
            var listed = LocalFolder.List(dir);
            // Only what plays: subtitle files, notes and hidden files aren't shown.
            Assert.Equal(new[] { "Season 2", "Season 10", "Clip.MP4", "Film.mkv" }, listed.Select(e => e.Name));
            Assert.Equal(new[] { true, true, false, false }, listed.Select(e => e.Folder));
        }
        finally
        {
            Directory.Delete(dir, true);
        }
    }

    [Fact]
    public void Subtitle_files_beside_a_video_say_their_language_and_kind()
    {
        var found = LocalFolder.Sidecars("/media/Ad Astra.mkv", new[]
        {
            "Ad Astra.mkv", "Ad Astra.srt", "Ad Astra.en.forced.srt", "Ad Astra.English.SDH.srt", "Ad Astra.fr.vtt", "Ad Astra 2.srt", "Other.srt", "Ad Astra.ass",
        });
        Assert.Equal(new[] { "Ad Astra.en.forced.srt", "Ad Astra.English.SDH.srt", "Ad Astra.fr.vtt", "Ad Astra.srt" }, found.Select(s => Path.GetFileName(s.Path)));
        Assert.Equal(new string?[] { "en", "eng", "fr", null }, found.Select(s => s.Language));
        Assert.Equal(new[] { true, false, false, false }, found.Select(s => s.Forced));
        Assert.Equal("SDH", found[1].Title);
        Assert.Equal(new[] { "subrip", "subrip", "webvtt", "subrip" }, found.Select(s => s.Codec));
    }

    [Fact]
    public void SubRip_becomes_WebVTT_the_app_draws()
    {
        var vtt = LocalFolder.ToWebVtt("﻿1\r\n00:00:01,500 --> 00:00:03,000\r\nHello\r\n\r\n2\r\n00:01:02,250 --> 00:01:04,000\r\nThere\r\n");
        Assert.StartsWith("WEBVTT\n\n", vtt);
        var cues = WebVtt.Parse(vtt);
        Assert.Equal(new[] { 1_500L, 62_250L }, cues.Select(c => c.StartMs));
        Assert.Equal("Hello", cues[0].Text);
    }

    [Fact]
    public void A_subtitle_file_not_in_UTF8_is_read_as_Windows1252()
    {
        Assert.Equal("café", LocalFolder.Decode(new byte[] { 0x63, 0x61, 0x66, 0xe9 }));
        Assert.Equal("café", LocalFolder.Decode(Encoding.UTF8.GetBytes("café")));
        // Its curly quotes, dash and euro, which Latin-1 would make control characters.
        Assert.Equal("“Hi” – 5€", LocalFolder.Decode(new byte[] { 0x93, 0x48, 0x69, 0x94, 0x20, 0x96, 0x20, 0x35, 0x80 }));
    }

    [Fact]
    public void A_subtitle_file_in_UTF16_or_UTF32_is_read_by_its_byte_order_mark()
    {
        const string text = "Café 日本 \U0001F3AC";
        Assert.Equal(text, LocalFolder.Decode(new byte[] { 0xFF, 0xFE }.Concat(Encoding.Unicode.GetBytes(text)).ToArray()));
        Assert.Equal(text, LocalFolder.Decode(new byte[] { 0xFE, 0xFF }.Concat(Encoding.BigEndianUnicode.GetBytes(text)).ToArray()));
        Assert.Equal(text, LocalFolder.Decode(new byte[] { 0xFF, 0xFE, 0, 0 }.Concat(new UTF32Encoding(false, false).GetBytes(text)).ToArray()));
        Assert.Equal(text, LocalFolder.Decode(new byte[] { 0, 0, 0xFE, 0xFF }.Concat(new UTF32Encoding(true, false).GetBytes(text)).ToArray()));
        // A lone surrogate and an odd last byte don't stop the rest.
        Assert.Equal("�A", LocalFolder.Decode(new byte[] { 0xFF, 0xFE, 0x00, 0xD8, 0x41, 0x00, 0x42 }));
    }

    [Fact]
    public void A_subtitle_file_in_UTF16_without_its_mark_is_read_by_its_NUL_bytes()
    {
        const string text = "1\n00:00:01,000 --> 00:00:02,000\nCafé 日本\n";
        Assert.Equal(text, LocalFolder.Decode(Encoding.Unicode.GetBytes(text)));
        Assert.Equal(text, LocalFolder.Decode(Encoding.BigEndianUnicode.GetBytes(text)));
        // Text in a single-byte encoding has no NULs: never taken for UTF-16.
        const string plain = "1\n00:00:01,000 --> 00:00:02,000\nHello\n";
        Assert.Equal(plain, LocalFolder.Decode(Encoding.UTF8.GetBytes(plain)));
    }

    // ---- playing one

    sealed class App : IPlayerSource
    {
        public bool Local => false;
        public string Server => "https://pley.example";
        public string ClientId => "tizen-test";
        public long? ProfileId => 1;
        public IKeyValue Storage { get; } = new MemoryKeyValue();
        public Settings Settings => new Settings();
        public void UpdateSettings(Func<Settings, Settings> change) { }
        public string Url(string path) => path;
        public IReadOnlyDictionary<string, object?>? Caps() => TizenCaps.Of(new TizenFacts());
        public ItemDetail? Saved(long id) => null;
        public Task<ItemDetail> Item(long id) => throw new NotSupportedException();
        public Task<PlaybackInfo> StartPlayback(IReadOnlyDictionary<string, object?> body) => throw new NotSupportedException();
        public Task StopPlayback(string session) => throw new NotSupportedException();
        public Task Progress(long itemId, long positionMs, long? durationMs, string state, string session, PlayingTracks? tracks = null) => throw new NotSupportedException();
        public Task<SessionStatus> PlaybackStatus(string session, int timeoutMs) => throw new NotSupportedException();
        public Task Reachable(int timeoutMs) => throw new NotSupportedException();
        public Task<SubtitleText> Subtitles(string url) => throw new NotSupportedException();
        public Task<SubtitlePictures> PictureSubtitles(string url) => throw new NotSupportedException();
        public Task<TrickplayInfo?> Trickplay(long fileId) => throw new NotSupportedException();
        public string TrickplaySheetUrl(long fileId, int sheet, string version) => "";
        public Task<ItemSummary?> Next(long itemId) => throw new NotSupportedException();
        public Task<IReadOnlyList<ItemSummary>> Queue(PlayQueue queue) => throw new NotSupportedException();
        public Task SetStreams(long itemId, long? fileId, int? audio, int? subtitle) => throw new NotSupportedException();
        public void TracksChosen(ItemDetail detail) { }
        public IDisposable OnEvent(Action<ServerEvent> listener) => throw new NotSupportedException();
    }

    [Fact]
    public async Task A_film_on_a_drive_plays_as_it_is_with_its_subtitle_files_and_a_track_the_TV_decodes()
    {
        var dir = Directory.CreateDirectory(Path.Combine(Path.GetTempPath(), "pley-local-" + Guid.NewGuid().ToString("N"))).FullName;
        try
        {
            var film = Path.Combine(dir, "Mad Max.mkv");
            File.WriteAllBytes(film, Film());
            File.WriteAllText(Path.Combine(dir, "Mad Max.en.srt"), "1\n00:00:01,000 --> 00:00:02,000\nWitness me\n");
            var source = new LocalPlayerSource(new App(), film);
            var detail = await source.Item(LocalPlayerSource.ItemId);
            Assert.Equal("Mad Max", detail.Title);
            var file = detail.Files.Single();
            // The subtitle in the file isn't drawn; the file beside it is, after the file's own streams.
            Assert.Equal(new[] { "video", "audio", "audio", "audio", "subtitle" }, file.Streams.Select(s => s.Kind));
            Assert.Equal(("subrip", "en", 4), (file.Streams[4].Codec, file.Streams[4].Language, file.Streams[4].Index));
            Assert.Equal(2, file.Chapters.Count);

            var info = await source.StartPlayback(new Dictionary<string, object?> { ["mode"] = "auto" });
            Assert.Equal(("direct", film, false), (info.Method, info.Url, info.Hls));
            // TrueHD is the default, but no Samsung TV decodes it: the AC-3 beside it plays.
            Assert.Equal(2, info.AudioIndex);
            Assert.Equal(Path.Combine(dir, "Mad Max.en.srt"), info.Subtitles.Single().Url);
            var text = await source.Subtitles(info.Subtitles[0].Url!);
            Assert.Equal("Witness me", WebVtt.Parse(text.Text).Single().Text);

            // Nothing converts it.
            await Assert.ThrowsAsync<LocalPlaybackException>(() => source.StartPlayback(new Dictionary<string, object?> { ["mode"] = "transcode" }));
        }
        finally
        {
            Directory.Delete(dir, true);
        }
    }

    [Fact]
    public async Task The_player_offers_a_drive_films_tracks_and_subtitle_files_and_its_HDR()
    {
        var dir = Directory.CreateDirectory(Path.Combine(Path.GetTempPath(), "pley-local-" + Guid.NewGuid().ToString("N"))).FullName;
        try
        {
            var film = Path.Combine(dir, "Mad Max.mkv");
            File.WriteAllBytes(film, Film(hdr: true));
            File.WriteAllText(Path.Combine(dir, "Mad Max.en.srt"), "1\n00:00:01,000 --> 00:00:02,000\nWitness me\n");
            var h = new PlayerHarness();
            var model = new PlayerModel(new PlayRequest(LocalPlayerSource.ItemId, 0, FromStart: false), new LocalPlayerSource(h.Source, film), () => h.Engine, h.Clock, new InstantReporter());
            model.Start();
            for (var i = 0; i < 200 && h.Engine.Prepared.Count == 0; i++)
            {
                await Task.Delay(10);
                h.Clock.Advance(1);
            }
            var s = model.State.Value;
            Assert.Equal(new[] { "Off", "English" }, s.Subtitles.Select(t => t.Label.Split(' ')[0]));
            Assert.Equal(3, s.Audio.Count);
            Assert.Empty(s.Qualities);
            Assert.Equal(2, s.Chapters.Count);
            // Info opens on the file's details at once: nothing is asked of a server.
            model.SetDetailsOpen(true);
            var details = model.State.Value.Details;
            Assert.NotNull(details);
            Assert.Equal("HDR10+", details!.Rows.FirstOrDefault(r => r.Label == SessionText.RowHdr)?.Value);
            Assert.Equal(film, h.Engine.Prepared.Single().Url);
        }
        finally
        {
            Directory.Delete(dir, true);
        }
    }

    [Fact]
    public async Task Where_a_video_stopped_is_kept_on_the_TV_until_it_was_watched()
    {
        var app = new App();
        var source = new LocalPlayerSource(app, "/media/USB/Film.mkv");
        await source.Progress(LocalPlayerSource.ItemId, 600_000, 7_200_000, "stopped", "local");
        Assert.Equal(600_000, LocalPlayerSource.ResumeOf(app.Storage, "/media/USB/Film.mkv"));
        Assert.Equal(0, LocalPlayerSource.ResumeOf(app.Storage, "/media/USB/Other.mkv"));
        await source.Progress(LocalPlayerSource.ItemId, 7_000_000, 7_200_000, "stopped", "local");
        Assert.Equal(0, LocalPlayerSource.ResumeOf(app.Storage, "/media/USB/Film.mkv"));
    }

    [Fact]
    public void The_drive_shows_in_the_rail_while_one_is_plugged_in()
    {
        var libraries = new List<Library> { new Library { Id = 1, Name = "Movies", Kind = "movies" } };
        Assert.DoesNotContain(Rail.Items(libraries), i => i.Slot == RailSlot.Storage);
        var items = Rail.Items(libraries, nowPlaying: true, storage: true);
        Assert.Equal(new[] { "now-playing", "storage" }, items.Skip(items.Count - 2).Select(i => i.Id));
        Assert.Equal(("usb", "USB drive"), (items.Last().Icon, items.Last().Label));
        Assert.Equal("storage", Rail.Current(new PageKey.Storage("/opt/media/USBDriveA1/Films"), items));
    }
}
