using System;
using System.Collections.Generic;
using System.Linq;
using Pley.Tv.Core.Subtitles.Pictures;
using Xunit;

namespace Pley.Tv.Core.Tests;

/// <summary>Picture subtitles in the server's .mks, decoded (packages/picture-subtitles/test/pictures.test.ts).</summary>
static class PictureKit
{
    public static int[] U16(int v) => new[] { (v >> 8) & 0xff, v & 0xff };

    public static int[] Cat(params IEnumerable<int>[] parts) => parts.SelectMany(p => p).ToArray();

    public static byte[] Bytes(IEnumerable<int> values) => values.Select(v => (byte)v).ToArray();

    public static int[] Str(string s) => s.Select(c => (int)c).ToArray();

    static int[] PgsSegment(int type, int[] body) => Cat(new[] { type }, U16(body.Length), body);

    /// <summary>
    /// A display set placing a <paramref name="w"/>×<paramref name="h"/> object at (x, y) on a 1920×1080 plane, with
    /// palette entry 1 white; or as the PCS's own entries <paramref name="placed"/> say.
    /// </summary>
    public static byte[] PgsDisplaySet(int state, bool objects, int x = 100, int y = 900, int[]? rle = null, int w = 4, int h = 2, int[][]? placed = null)
    {
        placed ??= objects ? new[] { Cat(U16(0), new[] { 0, 0x00 }, U16(x), U16(y)) } : new int[0][];
        var pcs = Cat(U16(1920), U16(1080), new[] { 0x10 }, U16(1), new[] { state, 0x00, 0x00, placed.Length }, Cat(placed));
        var output = new List<int>(PgsSegment(0x16, pcs));
        if (rle != null)
        {
            output.AddRange(PgsSegment(0x17, Cat(new[] { 1, 0 }, U16(x), U16(y), U16(w), U16(h))));
            // Palette 0: entry 1 white (Y 235, Cr 128, Cb 128), opaque; entry 2 half-transparent black.
            output.AddRange(PgsSegment(0x14, new[] { 0, 0, 1, 235, 128, 128, 255, 2, 16, 128, 128, 128 }));
            var data = Cat(U16(w), U16(h), rle);
            var length = data.Length;
            output.AddRange(PgsSegment(0x15, Cat(U16(0), new[] { 0, 0xc0, (length >> 16) & 0xff, (length >> 8) & 0xff, length & 0xff }, data)));
        }
        output.AddRange(PgsSegment(0x80, new int[0]));
        return Bytes(output);
    }

    /// <summary>An epoch start placing one <paramref name="w"/>×<paramref name="h"/> object <paramref name="times"/> times on a 1920×1080 plane; its pixels are all transparent.</summary>
    public static byte[] PgsPlaced(int w, int h, int times)
    {
        var pcs = Cat(U16(1920), U16(1080), new[] { 0x10 }, U16(1), new[] { 0x80, 0x00, 0x00, times });
        for (var i = 0; i < times; i++) pcs = Cat(pcs, U16(0), new[] { 0, 0x00 }, U16(i * 10), U16(0));
        var data = Cat(U16(w), U16(h));
        return Bytes(Cat(
            PgsSegment(0x16, pcs),
            PgsSegment(0x14, new[] { 0, 0, 1, 235, 128, 128, 255 }),
            PgsSegment(0x15, Cat(U16(0), new[] { 0, 0xc0, 0, 0, data.Length }, data)),
            PgsSegment(0x80, new int[0])));
    }

    /// <summary>The sizes of the pictures of <paramref name="frame"/>.</summary>
    public static (int, int)[] Sizes(PictureFrame? frame) => frame!.Bitmaps.Select(b => (b.Width, b.Height)).ToArray();

    /// <summary>An EBML element.</summary>
    public static int[] El(long id, int[] body)
    {
        var idBytes = new List<int>();
        for (var v = id; v > 0; v /= 256) idBytes.Insert(0, (int)(v & 0xff));
        var size = body.Length;
        // 8-byte sizes keep the builder simple.
        var sizeBytes = new[] { 0x01, 0, 0, 0, (size >> 24) & 0xff, (size >> 16) & 0xff, (size >> 8) & 0xff, size & 0xff };
        return Cat(idBytes, sizeBytes, body);
    }

    public sealed record MksBlock(int Time, byte[] Data, int? Duration = null);

    public static byte[] Mks(string codecId, IEnumerable<MksBlock> blocks, int[]? codecPrivate = null)
    {
        var cluster = new List<int>();
        foreach (var b in blocks)
        {
            var data = b.Data.Select(v => (int)v).ToArray();
            var block = Cat(new[] { 0x81 }, U16(b.Time), new[] { 0x00 }, data);
            if (b.Duration is int d) cluster.AddRange(El(0xa0, Cat(El(0xa1, block), El(0x9b, U16(d)))));
            else cluster.AddRange(El(0xa3, Cat(new[] { 0x81 }, U16(b.Time), new[] { 0x80 }, data)));
        }
        return MksFile(TrackEntry(codecId, codecPrivate != null ? El(0x63a2, codecPrivate) : null), cluster.ToArray());
    }

    /// <summary>A subtitle track entry, track 1, with <paramref name="more"/> of its children after its codec.</summary>
    public static int[] TrackEntry(string codecId, int[]? more = null) => El(0xae, Cat(El(0xd7, new[] { 1 }), El(0x83, new[] { 0x11 }), El(0x86, Str(codecId)), more ?? new int[0]));

    /// <summary>A file of these track entries and one cluster holding <paramref name="cluster"/> after its time.</summary>
    public static byte[] MksFile(int[] tracks, int[] cluster)
    {
        var segment = El(0x18538067, Cat(El(0x1549a966, El(0x2ad7b1, new[] { 0x0f, 0x42, 0x40 })), El(0x1654ae6b, tracks), El(0x1f43b675, Cat(El(0xe7, new[] { 0 }), cluster))));
        return Bytes(Cat(El(0x1a45dfa3, El(0x4282, Str("matroska"))), segment));
    }

    public static int[] Rgba(SubtitleBitmap b, int from, int count) => b.Rgba.Skip(from).Take(count).Select(v => (int)v).ToArray();
}

public class PgsTests
{
    [Fact]
    public void Decodes_runs_line_ends_and_long_runs()
    {
        // Line 1: 2 × colour 1, 2 × colour 0. Line 2: 4 × colour 2 (short coloured run).
        var rle = PictureKit.Bytes(new[] { 1, 1, 0, 0x02, 0, 0, 0, 0x84, 2, 0, 0 });
        Assert.Equal(new byte[] { 1, 1, 0, 0, 2, 2, 2, 2 }, PgsDecoder.DecodeRle(rle, 4, 2));
        var isLong = PictureKit.Bytes(new[] { 0, 0xc1, 0x00, 7, 0, 0 });
        Assert.False(PgsDecoder.DecodeRle(isLong, 300, 1).All(p => p == 7));
        Assert.True(PgsDecoder.DecodeRle(isLong, 300, 1).Take(256).All(p => p == 7));
    }

    [Fact]
    public void Places_an_object_on_the_plane_in_its_palettes_colours()
    {
        var set = PictureKit.PgsDisplaySet(0x80, true, rle: new[] { 1, 1, 0, 0x02, 0, 0, 0, 0x84, 2, 0, 0 });
        var frame = new PgsDecoder(new[] { set }).Decode(0)!;
        Assert.Equal(1920, frame.PlaneWidth);
        Assert.Equal(1080, frame.PlaneHeight);
        var b = Assert.Single(frame.Bitmaps);
        Assert.Equal((100, 900, 4, 2), (b.X, b.Y, b.Width, b.Height));
        Assert.Equal(new[] { 255, 255, 255, 255 }, PictureKit.Rgba(b, 0, 4));
        Assert.Equal(0, b.Rgba[3 + 4 * 2]);
        Assert.Equal(new[] { 0, 0, 0, 128 }, PictureKit.Rgba(b, 16, 4));
    }

    [Fact]
    public void Decodes_a_normal_case_display_set_from_its_epochs_definitions()
    {
        var start = PictureKit.PgsDisplaySet(0x80, true, rle: new[] { 1, 1, 1, 1, 0, 0, 1, 1, 1, 1, 0, 0 });
        var clear = PictureKit.PgsDisplaySet(0x00, false);
        var again = PictureKit.PgsDisplaySet(0x00, true, x: 200, y: 950);
        var decoder = new PgsDecoder(new[] { start, clear, again });
        Assert.Empty(decoder.Decode(1)!.Bitmaps);
        var reused = decoder.Decode(2)!.Bitmaps[0];
        Assert.Equal((200, 950), (reused.X, reused.Y));
        Assert.Equal(255, reused.Rgba[3]);
    }

    [Fact]
    public void Crops_an_object_only_where_its_place_says_so_and_shows_a_forced_one_whole()
    {
        static (int, int, int, int)[] Boxes(params int[][] placed) =>
            new PgsDecoder(new[] { PictureKit.PgsDisplaySet(0x80, true, rle: new[] { 1, 1, 1, 1, 0, 0, 1, 1, 1, 1, 0, 0 }, placed: placed) })
                .Decode(0)!.Bitmaps.Select(b => (b.X, b.Y, b.Width, b.Height)).ToArray();
        // Cropped (0x80) to 2×1 from (1, 1).
        Assert.Equal(new[] { (100, 900, 2, 1) }, Boxes(PictureKit.Cat(PictureKit.U16(0), new[] { 0, 0x80 }, PictureKit.U16(100), PictureKit.U16(900), PictureKit.U16(1), PictureKit.U16(1), PictureKit.U16(2), PictureKit.U16(1))));
        // Forced (0x40), with a second place after it: no crop to read.
        Assert.Equal(new[] { (100, 900, 4, 2), (200, 950, 4, 2) }, Boxes(
            PictureKit.Cat(PictureKit.U16(0), new[] { 0, 0x40 }, PictureKit.U16(100), PictureKit.U16(900)),
            PictureKit.Cat(PictureKit.U16(0), new[] { 0, 0x00 }, PictureKit.U16(200), PictureKit.U16(950))));
    }

    [Fact]
    public void Decodes_a_frames_worth_of_pixels_at_most()
    {
        static (int, int)[] Sizes(byte[] set) => PictureKit.Sizes(new PgsDecoder(new[] { set }).Decode(0));
        // One 2048×2048 object placed three times: twice would pass a 4K screen's pixels.
        Assert.Equal(new[] { (2048, 2048) }, Sizes(PictureKit.PgsPlaced(2048, 2048, 3)));
        // A Blu-ray's whole plane, twice, is well within; twice its height, twice, just so.
        Assert.Equal(new[] { (1920, 1080), (1920, 1080) }, Sizes(PictureKit.PgsPlaced(1920, 1080, 2)));
        Assert.Equal(new[] { (1920, 2160), (1920, 2160) }, Sizes(PictureKit.PgsPlaced(1920, 2160, 2)));
    }

    [Fact]
    public void Places_two_objects_at_most_as_Blu_ray_does()
    {
        static (int, int)[] Sizes(byte[] set) => PictureKit.Sizes(new PgsDecoder(new[] { set }).Decode(0));
        Assert.Equal(2, Sizes(PictureKit.PgsPlaced(1920, 2160, 3)).Length);
        Assert.Equal(2, Sizes(PictureKit.PgsPlaced(1, 1, 255)).Length);
    }

    [Fact]
    public void Reads_256_segments_of_a_display_set_at_most_and_256_entries_of_a_palette()
    {
        var rle = new[] { 1, 1, 1, 1, 0, 0, 1, 1, 1, 1, 0, 0 };
        // The composition, then empty windows, then the palette and object.
        byte[] Padded(int windows)
        {
            var set = PictureKit.PgsDisplaySet(0x80, true, rle: rle);
            var pcs = 3 + ((set[1] << 8) | set[2]);
            return set.Take(pcs).Concat(Enumerable.Repeat(new byte[] { 0x17, 0, 0 }, windows).SelectMany(w => w)).Concat(set.Skip(pcs)).ToArray();
        }
        // 252 windows: the object is the 256th segment; 253, the 257th.
        Assert.Single(new PgsDecoder(new[] { Padded(252) }).Decode(0)!.Bitmaps);
        Assert.Empty(new PgsDecoder(new[] { Padded(253) }).Decode(0)!.Bitmaps);
        // Entry 1 opaque white, then 255 more, then entry 1 again, transparent: not read.
        var entries = PictureKit.Cat(new[] { 1, 235, 128, 128, 255 }, Enumerable.Repeat(new[] { 2, 16, 128, 128, 255 }, 255).SelectMany(e => e), new[] { 1, 235, 128, 128, 0 });
        var composition = PictureKit.PgsDisplaySet(0x80, true);
        var segment = (int type, int[] body) => PictureKit.Cat(new[] { type }, PictureKit.U16(body.Length), body);
        var withPalette = PictureKit.Bytes(PictureKit.Cat(
            composition.Take(composition.Length - 3).Select(b => (int)b),
            segment(0x14, PictureKit.Cat(new[] { 0, 1 }, entries)),
            segment(0x15, PictureKit.Cat(PictureKit.U16(0), new[] { 0, 0xc0, 0, 0, 16 }, PictureKit.U16(4), PictureKit.U16(2), rle)),
            segment(0x80, new int[0])));
        Assert.Equal(255, new PgsDecoder(new[] { withPalette }).Decode(0)!.Bitmaps[0].Rgba[3]);
    }

    [Fact]
    public void Decodes_a_frame_from_4096_display_sets_back_at_most()
    {
        var start = PictureKit.PgsDisplaySet(0x80, true, rle: new[] { 1, 1, 1, 1, 0, 0, 1, 1, 1, 1, 0, 0 });
        var again = PictureKit.PgsDisplaySet(0x00, true);
        var decoder = new PgsDecoder(new[] { start }.Concat(Enumerable.Repeat(again, 4097)).ToArray());
        Assert.Single(decoder.Decode(4096)!.Bitmaps);
        Assert.Null(decoder.Decode(4097));
    }

    [Fact]
    public void Counts_each_segment_read()
    {
        // Display sets of 256 segments, 254 of them empty windows: 512 each, 400 sets come to 54 million, 600 to 81.
        var start = PictureKit.PgsDisplaySet(0x80, true, rle: new[] { 1, 1, 1, 1, 0, 0, 1, 1, 1, 1, 0, 0 });
        var again = PictureKit.PgsDisplaySet(0x00, true);
        var windows = again.Take(again.Length - 3).Concat(Enumerable.Repeat(new byte[] { 0x17, 0, 0 }, 254).SelectMany(w => w)).Concat(new byte[] { 0x80, 0, 0 }).ToArray();
        int? After(int n) => new PgsDecoder(new[] { start }.Concat(Enumerable.Repeat(windows, n)).ToArray()).Decode(n)?.Bitmaps.Count;
        Assert.Equal(1, After(400));
        Assert.Null(After(600));
    }

    [Fact]
    public void Doesnt_decode_a_frame_whose_display_sets_take_more_than_eight_4K_screens_work()
    {
        // Each display set padded to 1 MiB: 15 come to 63 million, past 66 with the 16th.
        static byte[] Padded(byte[] set)
        {
            var padded = new byte[1 << 20];
            set.CopyTo(padded, 0);
            return padded;
        }
        var start = Padded(PictureKit.PgsDisplaySet(0x80, true, rle: new[] { 1, 1, 1, 1, 0, 0, 1, 1, 1, 1, 0, 0 }));
        var again = Padded(PictureKit.PgsDisplaySet(0x00, true));
        var decoder = new PgsDecoder(new[] { start }.Concat(Enumerable.Repeat(again, 15)).ToArray());
        Assert.Single(decoder.Decode(14)!.Bitmaps);
        Assert.Null(decoder.Decode(15));
    }

    [Fact]
    public void Decodes_objects_of_4096_pixels_a_side_at_most()
    {
        static (int, int)[] Sizes(byte[] set) => PictureKit.Sizes(new PgsDecoder(new[] { set }).Decode(0));
        Assert.Equal(new[] { (4096, 1) }, Sizes(PictureKit.PgsPlaced(4096, 1, 1)));
        Assert.Empty(Sizes(PictureKit.PgsPlaced(4097, 1, 1)));
        Assert.Empty(Sizes(PictureKit.PgsPlaced(1, 4097, 1)));
    }
}

public class FrameCacheTests
{
    static PictureFrame Frame(int bytes) => new PictureFrame(1920, 1080, new[] { new SubtitleBitmap(0, 0, 1, 1, new byte[bytes]) });

    [Fact]
    public void Keeps_the_frames_decoded_last_within_their_bytes_and_their_number_always_the_one_just_decoded()
    {
        var cache = new FrameCache(25, 300);
        foreach (var i in new[] { 0, 1, 2 }) cache.Get(i, _ => Frame(101));
        Assert.Equal(new[] { false, true, true }, new[] { 0, 1, 2 }.Select(cache.Has));
        // A frame over the bound alone is still kept: it is the one showing.
        var big = Frame(301);
        Assert.Same(big, cache.Get(3, _ => big));
        Assert.Equal(new[] { false, false, true }, new[] { 1, 2, 3 }.Select(cache.Has));
        Assert.Same(big, cache.Get(3, _ => Frame(1)));
        // Frames with nothing to show are kept by number.
        for (var i = 10; i < 40; i++) cache.Get(i, _ => null);
        Assert.Equal(new[] { false, false, true, true }, new[] { 3, 14, 15, 39 }.Select(cache.Has));
    }
}

public class MatroskaTests
{
    static readonly byte[] Set = PictureKit.PgsDisplaySet(0x80, true, rle: new[] { 1, 1, 1, 1, 0, 0, 1, 1, 1, 1, 0, 0 });

    [Fact]
    public void Reads_the_track_its_samples_times_and_durations()
    {
        var track = Mks.ReadPictureTrack(PictureKit.Mks("S_HDMV/PGS", new[]
        {
            new PictureKit.MksBlock(1000, Set),
            new PictureKit.MksBlock(3000, PictureKit.PgsDisplaySet(0, false)),
            new PictureKit.MksBlock(5000, Set, Duration: 1500),
        }));
        Assert.Equal("S_HDMV/PGS", track.CodecId);
        Assert.Equal(new (long, long?)[] { (1000, null), (3000, null), (5000, 1500) }, track.Samples.Select(s => (s.TimeMs, s.DurationMs)));
        var subs = PictureSubtitles.Of(track)!;
        Assert.Null(subs.FrameAt(500));
        Assert.Equal(0, subs.FrameAt(1500)!.Value.Index);
        Assert.Null(subs.FrameAt(3200));
        Assert.Equal(2, subs.FrameAt(6000)!.Value.Index);
        Assert.Null(subs.FrameAt(6600));
        Assert.Equal(3000, subs.NextChange(1500));
        Assert.Equal(5000, subs.NextChange(4000));
        Assert.Equal(6500, subs.NextChange(5100));
    }

    [Fact]
    public void Reads_a_file_cut_short_up_to_the_cut_without_the_sample_cut_in_two()
    {
        var whole = PictureKit.Mks("S_HDMV/PGS", new[]
        {
            new PictureKit.MksBlock(1000, Set),
            new PictureKit.MksBlock(3000, PictureKit.PgsDisplaySet(0, false)),
            new PictureKit.MksBlock(5000, Set),
        });
        // Up to the middle of the last block.
        var cut = whole.Take(whole.Length - Set.Length / 2).ToArray();
        var track = Mks.ReadPictureTrack(cut);
        Assert.Equal(new long[] { 1000, 3000 }, track.Samples.Select(s => s.TimeMs));
        Assert.Equal(Set, track.Samples[0].Data);
        // Cut inside the track list: no track yet.
        var e = Assert.Throws<MatroskaException>(() => Mks.ReadPictureTrack(whole.Take(60).ToArray()));
        Assert.Contains("no track", e.Message);
    }

    [Fact]
    public void Refuses_what_isnt_matroska()
    {
        var e = Assert.Throws<MatroskaException>(() => Mks.ReadPictureTrack(new byte[] { 1, 2, 3, 4 }));
        Assert.Contains("not a Matroska file", e.Message);
    }

    [Fact]
    public void Reads_100000_samples_of_a_track_at_most()
    {
        var blocks = Enumerable.Range(0, Mks.MaxSamples + 5).Select(i => new PictureKit.MksBlock(i % 60_000, new byte[] { 0 }));
        Assert.Equal(Mks.MaxSamples, Mks.ReadPictureTrack(PictureKit.Mks("S_HDMV/PGS", blocks)).Samples.Count);
    }

    [Fact]
    public void Reads_a_sample_of_16_MiB_at_most()
    {
        // A block of `size` bytes after the file, built without going through its bytes one by one.
        static byte[] Big(int size)
        {
            var head = PictureKit.Mks("S_HDMV/PGS", new PictureKit.MksBlock[0]);
            var n = 4 + size;
            var file = new byte[head.Length + 9 + n];
            head.CopyTo(file, 0);
            new byte[] { 0xa3, 0x01, 0, 0, 0, (byte)(n >> 24), (byte)(n >> 16), (byte)(n >> 8), (byte)n, 0x81, 0, 0, 0x80 }.CopyTo(file, head.Length);
            return file;
        }
        Assert.Single(Mks.ReadPictureTrack(Big(16 << 20)).Samples);
        Assert.Empty(Mks.ReadPictureTrack(Big((16 << 20) + 1)).Samples);
    }

    [Fact]
    public void Reads_elements_8_deep_at_most_however_deep_they_go()
    {
        // A block in `groups` block groups, each in the one before: the block is 8 deep with 6.
        static byte[] Nested(int groups)
        {
            var inner = PictureKit.El(0xa1, PictureKit.Cat(new[] { 0x81 }, PictureKit.U16(0), new[] { 0x00, 1 }));
            for (var i = 0; i < groups; i++) inner = PictureKit.El(0xa0, inner);
            return PictureKit.MksFile(PictureKit.TrackEntry("S_HDMV/PGS"), inner);
        }
        Assert.Single(Mks.ReadPictureTrack(Nested(6)).Samples);
        Assert.Empty(Mks.ReadPictureTrack(Nested(7)).Samples);
        // 100,000 deep, where reading every level would run the stack out and end the app.
        const int n = 100_000;
        var block = new[] { 0xa1, 0x85, 0x81, 0, 0, 0x00, 1 };
        var nested = new int[n * 9 + block.Length];
        for (var i = 0; i < n; i++)
        {
            var size = (n - 1 - i) * 9 + block.Length;
            new[] { 0xa0, 0x01, 0, 0, 0, (size >> 24) & 0xff, (size >> 16) & 0xff, (size >> 8) & 0xff, size & 0xff }.CopyTo(nested, i * 9);
        }
        block.CopyTo(nested, n * 9);
        Assert.Empty(Mks.ReadPictureTrack(PictureKit.MksFile(PictureKit.TrackEntry("S_HDMV/PGS"), nested)).Samples);
    }

    [Fact]
    public void Reads_64_tracks_at_most_a_codec_ids_first_64_characters_and_a_tracks_first_64_KiB_of_private_data()
    {
        static int[] Other(int n) => Enumerable.Range(0, n).SelectMany(_ => PictureKit.El(0xae, PictureKit.Cat(PictureKit.El(0xd7, new[] { 2 }), PictureKit.El(0x83, new[] { 0x01 }), PictureKit.El(0x86, PictureKit.Str("V_OTHER"))))).ToArray();
        // The subtitle track after 63 others is read; after 64, the first track stands in for it.
        Assert.Equal("S_HDMV/PGS", Mks.ReadPictureTrack(PictureKit.MksFile(PictureKit.Cat(Other(63), PictureKit.TrackEntry("S_HDMV/PGS")), new int[0])).CodecId);
        Assert.Equal("V_OTHER", Mks.ReadPictureTrack(PictureKit.MksFile(PictureKit.Cat(Other(64), PictureKit.TrackEntry("S_HDMV/PGS")), new int[0])).CodecId);
        Assert.Equal(64, Mks.ReadPictureTrack(PictureKit.Mks("S_" + new string('X', 100), new PictureKit.MksBlock[0])).CodecId.Length);
        Assert.Equal(64 << 10, Mks.ReadPictureTrack(PictureKit.Mks("S_VOBSUB", new PictureKit.MksBlock[0], Enumerable.Repeat(0x20, 70_000).ToArray())).CodecPrivate!.Length);
    }

    [Fact]
    public void Takes_more_than_1_KiB_stripped_from_each_frame_for_compression_it_cant_undo()
    {
        static byte[] Stripping(int[] settings)
        {
            var compression = PictureKit.El(0x6d80, PictureKit.El(0x6240, PictureKit.El(0x5034, PictureKit.Cat(PictureKit.El(0x4254, new[] { 3 }), PictureKit.El(0x4255, settings)))));
            return PictureKit.MksFile(PictureKit.TrackEntry("S_HDMV/PGS", compression), PictureKit.El(0xa3, PictureKit.Cat(new[] { 0x81 }, PictureKit.U16(0), new[] { 0x80, 9 })));
        }
        Assert.Equal(new byte[] { 1, 2, 9 }, Mks.ReadPictureTrack(Stripping(new[] { 1, 2 })).Samples[0].Data);
        Assert.Single(Mks.ReadPictureTrack(Stripping(Enumerable.Repeat(1, 1024).ToArray())).Samples);
        var e = Assert.Throws<MatroskaException>(() => Mks.ReadPictureTrack(Stripping(Enumerable.Repeat(1, 1025).ToArray())));
        Assert.Contains("compressed", e.Message);
    }
}

public class VobSubTests
{
    static readonly byte[] Idx = PictureKit.Bytes(PictureKit.Str("size: 720x480\npalette: 000000, ffffff, 808080, ff0000, 00ff00, 0000ff, 000000, 000000, 000000, 000000, 000000, 000000, 000000, 000000, 000000, 000000\n"));

    /// <summary>
    /// An SPU of a 4×2 picture at (10, 400): top line pattern colour, bottom line emphasis 1, shown
    /// for 2 s; or of the corners <paramref name="area"/> says.
    /// </summary>
    static byte[] Spu(int[]? area = null)
    {
        // Nibble codes: 0x11 → 4 px... use 8-bit codes: 0001 00cc = count 4, colour cc.
        var top = new[] { 0x11, 0x00 }; // count 4 colour 1, then padding nibble byte align
        var bottom = new[] { 0x12, 0x00 }; // count 4 colour 2
        var pixels = PictureKit.Cat(top, bottom);
        var ctrlAt = 4 + pixels.Length;
        var stopSeqAt = ctrlAt + 4 + 1 + 3 + 3 + 7 + 5 + 1;
        var ctrl = PictureKit.Cat(
            PictureKit.U16(0), PictureKit.U16(stopSeqAt), new[] { 0x01 },
            new[] { 0x03, 0x32, 0x10 }, // colours: e2=3 e1=2 p=1 b=0
            new[] { 0x04, 0xff, 0xf0 }, // alpha: e2 15, e1 15, p 15, b 0
            new[] { 0x05 }, area ?? new[] { 0x00, 0xa0, 0x0d, 0x19, 0x01, 0x91 }, // x 10..13, y 400..401
            new[] { 0x06 }, PictureKit.U16(4), PictureKit.U16(6),
            new[] { 0xff });
        var stop = PictureKit.Cat(PictureKit.U16(175), PictureKit.U16(stopSeqAt), new[] { 0x02, 0xff });
        var all = PictureKit.Cat(pixels, ctrl, stop);
        return PictureKit.Bytes(PictureKit.Cat(PictureKit.U16(4 + all.Length), PictureKit.U16(ctrlAt), all));
    }

    [Fact]
    public void Reads_the_idx_header()
    {
        var h = VobSubDecoder.ParseIdx(Idx);
        Assert.Equal((720, 480), (h.Width, h.Height));
        Assert.Equal(((byte)255, (byte)255, (byte)255), h.Palette[1]);
        Assert.Equal(((byte)255, (byte)0, (byte)0), h.Palette[3]);
    }

    [Fact]
    public void Reads_control_sequences_place_colours_and_when_it_stops()
    {
        var s = VobSubDecoder.ParseSpu(Spu())!;
        Assert.Equal((10, 13, 400, 401), (s.X1, s.X2, s.Y1, s.Y2));
        Assert.Equal(new[] { 0, 1, 2, 3 }, s.Colors);
        Assert.Equal(new[] { 0, 15, 15, 15 }, s.Alpha);
        Assert.Equal((long)Math.Round(175 * 1024 / 90.0), s.StopMs);
    }

    [Fact]
    public void Decodes_interlaced_fields_in_the_palettes_colours()
    {
        var decoder = new VobSubDecoder(new[] { Spu() }, Idx);
        var frame = decoder.Decode(0)!;
        Assert.Equal((720, 480), (frame.PlaneWidth, frame.PlaneHeight));
        var b = frame.Bitmaps[0];
        Assert.Equal((10, 400, 4, 2), (b.X, b.Y, b.Width, b.Height));
        Assert.Equal(new[] { 255, 255, 255, 255 }, PictureKit.Rgba(b, 0, 4));
        Assert.Equal(new[] { 128, 128, 128, 255 }, PictureKit.Rgba(b, 16, 4));
        Assert.Equal((long)Math.Round(175 * 1024 / 90.0), decoder.OwnDurationMs(0));
    }

    [Fact]
    public void Reads_64_control_sequences_of_an_SPU_at_most()
    {
        // `n` sequences chained one to the next; the last says when it stops.
        static byte[] Chained(int n)
        {
            var spu = new List<int> { 0, 0, 0, 4 };
            for (var k = 0; k < n - 1; k++) spu.AddRange(PictureKit.Cat(new[] { 0, 0 }, PictureKit.U16(spu.Count + 5), new[] { 0xff }));
            spu.AddRange(PictureKit.Cat(PictureKit.U16(175), PictureKit.U16(spu.Count), new[] { 0x02, 0xff }));
            spu[0] = (spu.Count >> 8) & 0xff;
            spu[1] = spu.Count & 0xff;
            return PictureKit.Bytes(spu);
        }
        Assert.Equal((long)Math.Round(175 * 1024 / 90.0), VobSubDecoder.ParseSpu(Chained(64))!.StopMs);
        Assert.Null(VobSubDecoder.ParseSpu(Chained(65))!.StopMs);
    }

    [Fact]
    public void Reads_an_SPU_of_no_size_up_to_0xFFFF()
    {
        // Its one control sequence at 0xFFE0, its stop command past 0xFFFF or before it.
        static byte[] Late(int stopAt)
        {
            var data = new byte[0x10040];
            new byte[] { 0, 0, 0xff, 0xe0 }.CopyTo(data, 0);
            new byte[] { 0, 175, 0xff, 0xe0 }.CopyTo(data, 0xffe0);
            for (var i = 0xffe4; i < stopAt; i++) data[i] = 0x01;
            new byte[] { 0x02, 0xff }.CopyTo(data, stopAt);
            return data;
        }
        Assert.Equal((long)Math.Round(175 * 1024 / 90.0), VobSubDecoder.ParseSpu(Late(0xfff0))!.StopMs);
        Assert.Null(VobSubDecoder.ParseSpu(Late(0x10010))!.StopMs);
    }

    [Fact]
    public void Reads_the_idx_headers_first_64_KiB_and_16_colours()
    {
        var colours = string.Join(", ", Enumerable.Repeat("ffffff", 20));
        Assert.Equal(16, VobSubDecoder.ParseIdx(PictureKit.Bytes(PictureKit.Str($"palette: {colours}\n"))).Palette.Count);
        var late = PictureKit.Bytes(Enumerable.Repeat(0x20, 64 << 10).Concat(PictureKit.Str("size: 100x200\n")));
        var header = VobSubDecoder.ParseIdx(late);
        Assert.Equal((720, 576), (header.Width, header.Height));
    }

    [Fact]
    public void Refuses_a_picture_of_more_than_a_frames_pixels()
    {
        // x 0..3839, y 0..2159: a 4K screen's worth, just so.
        Assert.Equal(new[] { (3840, 2160) }, PictureKit.Sizes(new VobSubDecoder(new[] { Spu(new[] { 0x00, 0x0e, 0xff, 0x00, 0x08, 0x6f }) }, Idx).Decode(0)));
        // One line more.
        Assert.Null(new VobSubDecoder(new[] { Spu(new[] { 0x00, 0x0e, 0xff, 0x00, 0x08, 0x70 }) }, Idx).Decode(0));
    }
}

public class DvbTests
{
    static int[] DvbSegment(int type, int[] body) => PictureKit.Cat(new[] { 0x0f, type }, PictureKit.U16(1), PictureKit.U16(body.Length), body);

    /// <summary>A mode-change page with one 4-bit region of 4×2 at (60, 500), CLUT entry 1 red, object filled with entry 1.</summary>
    static byte[] DisplaySet()
    {
        var page = DvbSegment(0x10, PictureKit.Cat(new[] { 5, (0 << 4) | (2 << 2), 0, 0 }, PictureKit.U16(60), PictureKit.U16(500)));
        var region = DvbSegment(0x11, PictureKit.Cat(new[] { 0, 0x00 }, PictureKit.U16(4), PictureKit.U16(2), new[] { (2 << 5) | (2 << 2), 0, 0, 0 }, PictureKit.U16(0), new[] { (0 << 6) | 0, 0, 0, 0 }));
        // CLUT 0: entry 1 for 4-bit tables, full range: Y 81 Cr 240 Cb 90 (red), opaque.
        var clut = DvbSegment(0x12, new[] { 0, 0, 1, 0x40 | 0x01, 81, 240, 90, 0 });
        // 4-bit string: 4 × pixel 1 ("0001" ×4), then end of string (0000 0 000), end of line.
        var line = new[] { 0x11, 0x11, 0x11, 0x00, 0x00, 0xf0 };
        var obj = DvbSegment(0x13, PictureKit.Cat(PictureKit.U16(0), new[] { 0x00 }, PictureKit.U16(line.Length), PictureKit.U16(0), line));
        var end = DvbSegment(0x80, new int[0]);
        return PictureKit.Bytes(PictureKit.Cat(new[] { 0x20, 0x00 }, page, region, clut, obj, end, new[] { 0xff }));
    }

    [Fact]
    public void Splits_a_display_set_into_segments()
    {
        Assert.Equal(new[] { 0x10, 0x11, 0x12, 0x13, 0x80 }, DvbDecoder.Segments(DisplaySet()).Select(s => s.Type));
    }

    [Fact]
    public void Draws_objects_into_regions_placed_by_the_page()
    {
        var decoder = new DvbDecoder(new[] { DisplaySet() });
        var frame = decoder.Decode(0)!;
        Assert.Equal((720, 576), (frame.PlaneWidth, frame.PlaneHeight));
        var b = frame.Bitmaps[0];
        Assert.Equal((60, 500, 4, 2), (b.X, b.Y, b.Width, b.Height));
        var (r, g, bl) = PictureColors.YcbcrToRgb(81, 90, 240, false);
        Assert.Equal(new[] { (int)r, g, bl, 255 }, PictureKit.Rgba(b, 0, 4));
        // The bottom field is the top one: both lines drawn.
        Assert.Equal(255, b.Rgba[4 * 4 + 3]);
        Assert.Equal(5000, decoder.OwnDurationMs(0));
    }

    /// <summary>A mode-change page placing regions, each (id, x, y).</summary>
    static int[] Placing(params (int Id, int X, int Y)[] regions) =>
        DvbSegment(0x10, PictureKit.Cat(new[] { 5, 2 << 2 }, regions.SelectMany(r => PictureKit.Cat(new[] { r.Id, 0 }, PictureKit.U16(r.X), PictureKit.U16(r.Y)))));

    /// <summary>A 4-bit region filled with entry 1 of CLUT 0.</summary>
    static int[] Filled(int id, int w, int h) => DvbSegment(0x11, PictureKit.Cat(new[] { id, 0x08 }, PictureKit.U16(w), PictureKit.U16(h), new[] { 2 << 2, 0, 0, 1 << 4 }));

    /// <summary>CLUT 0 with entry 1 opaque red.</summary>
    static readonly int[] Red = DvbSegment(0x12, new[] { 0, 0, 1, 0x40 | 0x01, 81, 240, 90, 0 });

    /// <summary>The sizes of the pictures a display set of <paramref name="segs"/> shows; null when it isn't decoded.</summary>
    static (int, int)[]? Sizes(params int[][] segs)
    {
        var set = PictureKit.Bytes(PictureKit.Cat(new[] { 0x20, 0x00 }, PictureKit.Cat(segs), DvbSegment(0x80, new int[0]), new[] { 0xff }));
        var frame = new DvbDecoder(new[] { set }).Decode(0);
        return frame == null ? null : PictureKit.Sizes(frame);
    }

    [Fact]
    public void Decodes_a_frames_worth_of_pixels_at_most()
    {
        // One 3000×2000 region placed twice: twice would pass a 4K screen's pixels.
        Assert.Equal(new[] { (3000, 2000) }, Sizes(Placing((0, 0, 0), (0, 10, 10)), Filled(0, 3000, 2000), Red));
    }

    [Fact]
    public void Places_32_regions_at_most()
    {
        var places = Enumerable.Range(0, 1000).Select(i => (0, i % 700, 0)).ToArray();
        Assert.Equal(32, Sizes(Placing(places), Filled(0, 1, 1), Red)!.Length);
    }

    [Fact]
    public void Reads_256_segments_of_a_display_set_at_most()
    {
        var empty = Enumerable.Repeat(DvbSegment(0x16, new int[0]), 300).ToArray();
        var set = PictureKit.Bytes(PictureKit.Cat(new[] { 0x20, 0x00 }, Placing((0, 0, 0)), PictureKit.Cat(empty), Filled(0, 4, 2), Red, new[] { 0xff }));
        Assert.Equal(256, DvbDecoder.Segments(set).Count);
        Assert.Empty(new DvbDecoder(new[] { set }).Decode(0)!.Bitmaps);
        Assert.Equal(new[] { (4, 2) }, Sizes(new[] { Placing((0, 0, 0)) }.Concat(empty.Take(200)).Append(Filled(0, 4, 2)).Append(Red).ToArray()));
    }

    [Fact]
    public void Decodes_a_frame_from_4096_display_sets_back_at_most()
    {
        var start = PictureKit.Bytes(PictureKit.Cat(new[] { 0x20, 0x00 }, Placing((0, 0, 0)), Filled(0, 4, 2), Red, new[] { 0xff }));
        // Normal case: the page again.
        var again = PictureKit.Bytes(PictureKit.Cat(new[] { 0x20, 0x00 }, DvbSegment(0x10, PictureKit.Cat(new[] { 5, 0, 0, 0 }, PictureKit.U16(0), PictureKit.U16(0))), new[] { 0xff }));
        var decoder = new DvbDecoder(new[] { start }.Concat(Enumerable.Repeat(again, 4097)).ToArray());
        Assert.Single(decoder.Decode(4096)!.Bitmaps);
        Assert.Empty(decoder.Decode(4097)!.Bitmaps);
    }

    [Fact]
    public void Draws_an_object_in_the_first_64_places_a_region_lists_at_most()
    {
        // A 100×2 region listing object 1 at (0, 0) 64 times, then at (50, 0).
        var places = Enumerable.Repeat(PictureKit.Cat(PictureKit.U16(1), new[] { 0, 0, 0, 0 }), 64).Append(PictureKit.Cat(PictureKit.U16(1), new[] { 0, 50, 0, 0 }));
        var region = DvbSegment(0x11, PictureKit.Cat(new[] { 0, 0x00 }, PictureKit.U16(100), PictureKit.U16(2), new[] { 2 << 2, 0, 0, 0 }, places.SelectMany(p => p)));
        // One pixel of entry 1, then the end of the string and of the line.
        var line = new[] { 0x11, 0x10, 0x00, 0xf0 };
        var obj = DvbSegment(0x13, PictureKit.Cat(PictureKit.U16(1), new[] { 0x00 }, PictureKit.U16(line.Length), PictureKit.U16(0), line));
        var set = PictureKit.Bytes(PictureKit.Cat(new[] { 0x20, 0x00 }, Placing((0, 0, 0)), region, Red, obj, DvbSegment(0x80, new int[0]), new[] { 0xff }));
        var b = new DvbDecoder(new[] { set }).Decode(0)!.Bitmaps[0];
        Assert.Equal((255, 0), (b.Rgba[3], b.Rgba[50 * 4 + 3]));
    }

    [Fact]
    public void Doesnt_decode_a_frame_that_takes_more_than_eight_4K_screens_work()
    {
        // Region 0, 3840×2000, made and filled, then filled again and again; then region 1, the one placed.
        // Seven fills and the region make 8 × 7,680,000 pixels; eight, 9 ×, past 8 × 3840 × 2160.
        static int[][] Fills(int n) => Enumerable.Repeat(Filled(0, 3840, 2000), n).ToArray();
        Assert.Equal(new[] { (4, 2) }, Sizes(new[] { Placing((1, 0, 0)) }.Concat(Fills(7)).Append(Filled(1, 4, 2)).Append(Red).ToArray()));
        Assert.Null(Sizes(new[] { Placing((1, 0, 0)) }.Concat(Fills(8)).Append(Filled(1, 4, 2)).Append(Red).ToArray()));
    }

    [Fact]
    public void Counts_each_segment_read_and_each_region_looked_through_for_an_object()
    {
        // Region 0 placed and filled, then display sets of 254 empty segments: 512 each, 400 sets come to 52 million, 600 to 78.
        var normal = DvbSegment(0x10, PictureKit.Cat(new[] { 5, 0, 0, 0 }, PictureKit.U16(0), PictureKit.U16(0)));
        var start = PictureKit.Bytes(PictureKit.Cat(new[] { 0x20, 0x00 }, Placing((0, 0, 0)), Filled(0, 4, 2), Red, new[] { 0xff }));
        var empties = PictureKit.Bytes(PictureKit.Cat(new[] { 0x20, 0x00 }, normal, PictureKit.Cat(Enumerable.Repeat(DvbSegment(0x16, new int[0]), 254).ToArray()), new[] { 0xff }));
        int? After(int n) => new DvbDecoder(new[] { start }.Concat(Enumerable.Repeat(empties, n)).ToArray()).Decode(n)?.Bitmaps.Count;
        Assert.Equal(1, After(400));
        Assert.Null(After(600));
        // 250 regions, then display sets of 254 objects none lists: 250 regions at 32 each, 20 sets come to 44 million, 40 to 88.
        var regions = PictureKit.Cat(Enumerable.Range(1, 249).Select(k => DvbSegment(0x11, PictureKit.Cat(new[] { k, 0x00 }, PictureKit.U16(1), PictureKit.U16(1), new[] { 2 << 2, 0, 0, 0 }))).ToArray());
        var held = PictureKit.Bytes(PictureKit.Cat(new[] { 0x20, 0x00 }, Placing((0, 0, 0)), Filled(0, 4, 2), regions, Red, new[] { 0xff }));
        var objects = PictureKit.Bytes(PictureKit.Cat(new[] { 0x20, 0x00 }, normal, PictureKit.Cat(Enumerable.Repeat(DvbSegment(0x13, PictureKit.Cat(PictureKit.U16(0), new[] { 0x00 }, PictureKit.U16(0), PictureKit.U16(0))), 254).ToArray()), new[] { 0xff }));
        int? Scanned(int n) => new DvbDecoder(new[] { held }.Concat(Enumerable.Repeat(objects, n)).ToArray()).Decode(n)?.Bitmaps.Count;
        Assert.Equal(1, Scanned(20));
        Assert.Null(Scanned(40));
        // Colour tables of 65,000 bytes at 16 a byte: 60 come to 62 million, 70 to 73.
        var table = DvbSegment(0x12, PictureKit.Cat(new[] { 0, 0 }, Enumerable.Repeat(new[] { 1, 0x40, 0x80, 0x80 }, 16_249).SelectMany(e => e)));
        (int, int)[]? Tables(int n) => Sizes(new[] { Placing((0, 0, 0)), Filled(0, 4, 2) }.Concat(Enumerable.Repeat(table, n)).Append(Red).ToArray());
        Assert.Equal(new[] { (4, 2) }, Tables(60));
        Assert.Null(Tables(70));
    }

    [Fact]
    public void Counts_each_draw_of_an_object_an_empty_one_too()
    {
        // Regions 0 and 2, 1×1, each listing object 0 64 times; empty objects 0; then region 1, the one placed.
        // A draw costs 4096: 100 segments of 128 draws come to 52 million, 150 to 79, past 66.
        var places = Enumerable.Repeat(PictureKit.Cat(PictureKit.U16(0), new[] { 0, 0, 0, 0 }), 64).SelectMany(p => p).ToArray();
        int[] Region(int id) => DvbSegment(0x11, PictureKit.Cat(new[] { id, 0x00 }, PictureKit.U16(1), PictureKit.U16(1), new[] { 2 << 2, 0, 0, 0 }, places));
        static IEnumerable<int[]> Empty(int n) => Enumerable.Repeat(DvbSegment(0x13, PictureKit.Cat(PictureKit.U16(0), new[] { 0x00 }, PictureKit.U16(0), PictureKit.U16(0))), n);
        int[][] Set(int n) => new[] { Placing((1, 0, 0)), Region(0), Region(2), Red }.Concat(Empty(n)).Append(Filled(1, 4, 2)).ToArray();
        Assert.Equal(new[] { (4, 2) }, Sizes(Set(100)));
        Assert.Null(Sizes(Set(150)));
    }

    [Fact]
    public void Counts_walking_pixel_codes_as_more_work_than_filling_pixels_and_walks_no_line_below_a_region()
    {
        // A 1×h region listing object 1 64 times; the object, a string of 1-pixel codes that never ends; then region 1, the one placed.
        var places = Enumerable.Repeat(PictureKit.Cat(PictureKit.U16(1), new[] { 0, 0, 0, 0 }), 64).SelectMany(p => p).ToArray();
        int[] Region(int h) => DvbSegment(0x11, PictureKit.Cat(new[] { 0, 0x00 }, PictureKit.U16(1), PictureKit.U16(h), new[] { 2 << 2, 0, 0, 0 }, places));
        static int[] Object(int bytes)
        {
            var field = Enumerable.Repeat(0x11, bytes + 1).ToArray();
            return DvbSegment(0x13, PictureKit.Cat(PictureKit.U16(1), new[] { 0x00 }, PictureKit.U16(field.Length), PictureKit.U16(0), field));
        }
        Assert.Equal(new[] { (4, 2) }, Sizes(Placing((1, 0, 0)), Region(2), Red, Object(2_000), Filled(1, 4, 2)));
        Assert.Null(Sizes(Placing((1, 0, 0)), Region(2), Red, Object(20_000), Filled(1, 4, 2)));
        // One line high: the bottom field starts below it, and takes nothing.
        Assert.Equal(new[] { (4, 2) }, Sizes(Placing((1, 0, 0)), Region(1), Red, Object(20_000), Filled(1, 4, 2)));
    }

    [Fact]
    public void Keeps_a_frames_worth_of_regions_at_most_of_4096_pixels_a_side_at_most()
    {
        // The second 3000×2000 region would pass a 4K screen's pixels with the first: it isn't kept.
        Assert.Empty(Sizes(Placing((1, 0, 0)), Filled(0, 3000, 2000), Filled(1, 3000, 2000), Red)!);
        // A region made again counts once, at its new size.
        Assert.Equal(new[] { (3840, 2160) }, Sizes(Placing((0, 0, 0)), Filled(0, 3000, 2000), Filled(0, 3840, 2160), Red));
        Assert.Equal(new[] { (3000, 2000) }, Sizes(Placing((1, 0, 0)), Filled(0, 3840, 2160), Filled(0, 1, 1), Filled(1, 3000, 2000), Red));
        // A mode change lets go of the regions held.
        Assert.Equal(new[] { (3000, 2000) }, Sizes(Placing((0, 0, 0)), Filled(0, 3840, 2160), Placing((1, 0, 0)), Filled(1, 3000, 2000), Red));
        Assert.Equal(new[] { (4096, 1) }, Sizes(Placing((0, 0, 0)), Filled(0, 4096, 1), Red));
        Assert.Empty(Sizes(Placing((0, 0, 0)), Filled(0, 4097, 1), Red)!);
    }
}
