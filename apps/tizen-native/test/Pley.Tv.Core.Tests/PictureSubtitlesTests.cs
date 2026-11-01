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

    /// <summary>A display set placing a <paramref name="w"/>×<paramref name="h"/> object at (x, y) on a 1920×1080 plane, with palette entry 1 white.</summary>
    public static byte[] PgsDisplaySet(int state, bool objects, int x = 100, int y = 900, int[]? rle = null, int w = 4, int h = 2)
    {
        var pcs = Cat(U16(1920), U16(1080), new[] { 0x10 }, U16(1), new[] { state, 0x00, 0x00, objects ? 1 : 0 });
        if (objects) pcs = Cat(pcs, U16(0), new[] { 0, 0x00 }, U16(x), U16(y));
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

    /// <summary>An EBML element.</summary>
    static int[] El(long id, int[] body)
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
        var track = El(0xae, Cat(El(0xd7, new[] { 1 }), El(0x83, new[] { 0x11 }), El(0x86, Str(codecId)), codecPrivate != null ? El(0x63a2, codecPrivate) : new int[0]));
        var cluster = new List<int>(El(0xe7, new[] { 0 }));
        foreach (var b in blocks)
        {
            var data = b.Data.Select(v => (int)v).ToArray();
            var block = Cat(new[] { 0x81 }, U16(b.Time), new[] { 0x00 }, data);
            if (b.Duration is int d) cluster.AddRange(El(0xa0, Cat(El(0xa1, block), El(0x9b, U16(d)))));
            else cluster.AddRange(El(0xa3, Cat(new[] { 0x81 }, U16(b.Time), new[] { 0x80 }, data)));
        }
        var segment = El(0x18538067, Cat(El(0x1549a966, El(0x2ad7b1, new[] { 0x0f, 0x42, 0x40 })), El(0x1654ae6b, track), El(0x1f43b675, cluster.ToArray())));
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
}

public class VobSubTests
{
    static readonly byte[] Idx = PictureKit.Bytes(PictureKit.Str("size: 720x480\npalette: 000000, ffffff, 808080, ff0000, 00ff00, 0000ff, 000000, 000000, 000000, 000000, 000000, 000000, 000000, 000000, 000000, 000000\n"));

    /// <summary>An SPU of a 4×2 picture at (10, 400): top line pattern colour, bottom line emphasis 1, shown for 2 s.</summary>
    static byte[] Spu()
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
            new[] { 0x05, 0x00, 0xa0, 0x0d, 0x19, 0x01, 0x91 }, // x 10..13, y 400..401
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
}
