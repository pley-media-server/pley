using System;
using System.Collections.Generic;
using System.Linq;

namespace Pley.Tv.Core.Subtitles.Pictures;

/// <summary>One segment of a DVB display set: its type and its body, in place in the sample.</summary>
public sealed class DvbSegment
{
    public DvbSegment(int type, ByteView body)
    {
        Type = type;
        Body = body;
    }

    public int Type { get; }
    public ByteView Body { get; }
}

/// <summary>
/// Broadcast subtitles (DVB, ETSI EN 300 743, <c>S_DVBSUB</c>): each sample is a display set of
/// segments. A page composition places regions on the display; each region has a colour table
/// (CLUT) and objects drawn into it as 2, 4 or 8-bit run-length coded pixels, a field at a time.
/// A page in "normal case" only updates what an acquisition point or mode change defined, so a
/// sample is decoded from the last of those.
/// </summary>
public sealed class DvbDecoder : IPictureDecoder
{
    const int PageSegment = 0x10;
    const int RegionSegment = 0x11;
    const int ClutSegment = 0x12;
    const int ObjectSegment = 0x13;
    const int DisplaySegment = 0x14;

    static readonly int[] Depths = { 0, 2, 4, 8 };

    /// <summary>The objects a region lists at most: a real one has a few, one for each line of text.</summary>
    const int MaxRegionObjects = 64;

    /// <summary>The regions a page places at most: a real one places a few, each of them a picture to draw.</summary>
    const int MaxPlaced = 32;

    /// <summary>
    /// What reading a byte costs against <see cref="PictureColors.MaxWork"/>, next to filling a
    /// pixel: a segment's (a colour table's entries) about 16 times as long, and a pixel string's,
    /// walked a bit at a time, about 32. A segment costs 512 besides, a few hundred nanoseconds to
    /// read and keep.
    /// </summary>
    const int ReadCost = 16;
    const int WalkCost = 32;
    const int SegmentCost = 512;

    /// <summary>
    /// What drawing an object into a place costs against <see cref="PictureColors.MaxWork"/>,
    /// besides its bytes and pixels: a real frame draws a few, where a crafted one could draw an
    /// empty object millions of times. Looking through a region for an object's places costs 32,
    /// and each place it lists 32 more.
    /// </summary>
    const int DrawCost = 4096;
    const int ScanCost = 32;

    sealed class PageRegion
    {
        public int Id, X, Y;
    }

    sealed class Page
    {
        public int TimeoutS;
        /// <summary>0 normal case, 1 acquisition point, 2 mode change.</summary>
        public int State;
        public List<PageRegion> Regions = new List<PageRegion>();
    }

    sealed class RegionObject
    {
        public int Id, X, Y;
    }

    sealed class Region
    {
        public int Width, Height;
        /// <summary>2, 4 or 8 bits a pixel.</summary>
        public int Depth;
        public int Clut;
        public bool Fill;
        public int Background;
        public List<RegionObject> Objects = new List<RegionObject>();
        public byte[] Pixels = new byte[0];
    }

    readonly IReadOnlyList<byte[]> samples;
    readonly FrameCache cache = new FrameCache();

    public DvbDecoder(IReadOnlyList<byte[]> samples) => this.samples = samples;

    /// <summary>A display set's segments, <see cref="PictureColors.MaxSegments"/> at most.</summary>
    public static List<DvbSegment> Segments(byte[] data)
    {
        var segments = new List<DvbSegment>();
        // data_identifier 0x20 and subtitle_stream_id 0x00 open a PES data field.
        var at = data.Length >= 2 && data[0] == 0x20 && data[1] == 0x00 ? 2 : 0;
        while (segments.Count < PictureColors.MaxSegments && at + 6 <= data.Length && data[at] == 0x0f)
        {
            var type = data[at + 1];
            var length = (data[at + 4] << 8) | data[at + 5];
            segments.Add(new DvbSegment(type, new ByteView(data, at + 6, at + 6 + length)));
            at += 6 + length;
        }
        return segments;
    }

    /// <summary>The time-out and state of a display set's first page, read without the rest of it.</summary>
    static (int TimeoutS, int State)? PageHead(byte[] data)
    {
        var at = data.Length >= 2 && data[0] == 0x20 && data[1] == 0x00 ? 2 : 0;
        for (var n = 0; n < PictureColors.MaxSegments && at + 6 <= data.Length && data[at] == 0x0f; n++)
        {
            var length = (data[at + 4] << 8) | data[at + 5];
            if (data[at + 1] == PageSegment)
            {
                var end = Math.Min(data.Length, at + 6 + length);
                var flags = at + 7 < end ? data[at + 7] : 0;
                return (at + 6 < end ? data[at + 6] : 0, (flags >> 2) & 3);
            }
            at += 6 + length;
        }
        return null;
    }

    static Page ReadPage(ByteView b)
    {
        var page = new Page { TimeoutS = PictureColors.At(b, 0), State = (PictureColors.At(b, 1) >> 2) & 3 };
        for (var i = 2; i + 6 <= b.Length && page.Regions.Count < MaxPlaced; i += 6)
            page.Regions.Add(new PageRegion { Id = b[i], X = (b[i + 2] << 8) | b[i + 3], Y = (b[i + 4] << 8) | b[i + 5] });
        return page;
    }

    /// <summary>The colour tables of ETSI EN 300 743 §10 for pixels a CLUT doesn't define.</summary>
    static byte[] DefaultClut(int depth)
    {
        var n = 1 << depth;
        var table = new byte[n * 4];
        void Set(int i, double r, double g, double b, double a)
        {
            table[i * 4] = PictureColors.Clamp(r);
            table[i * 4 + 1] = PictureColors.Clamp(g);
            table[i * 4 + 2] = PictureColors.Clamp(b);
            table[i * 4 + 3] = PictureColors.Clamp(a);
        }
        if (depth == 2)
        {
            Set(0, 0, 0, 0, 0);
            Set(1, 255, 255, 255, 255);
            Set(2, 0, 0, 0, 255);
            Set(3, 127, 127, 127, 255);
            return table;
        }
        if (depth == 4)
        {
            for (var i = 0; i < 16; i++)
            {
                var v = i < 8 ? 255 : 127;
                Set(i, (i & 1) != 0 ? v : 0, (i & 2) != 0 ? v : 0, (i & 4) != 0 ? v : 0, i == 0 ? 0 : 255);
            }
            return table;
        }
        for (var i = 0; i < 256; i++)
        {
            double r = (i & 0x01) != 0 ? 85 : 0;
            double g = (i & 0x02) != 0 ? 85 : 0;
            double b = (i & 0x04) != 0 ? 85 : 0;
            var hi = i & 0x88;
            if (i < 8) Set(i, r * 3, g * 3, b * 3, i == 0 ? 0 : 64);
            else if (hi == 0x00) Set(i, r + ((i & 0x10) != 0 ? 170 : 0), g + ((i & 0x20) != 0 ? 170 : 0), b + ((i & 0x40) != 0 ? 170 : 0), 255);
            else if (hi == 0x08) Set(i, r + ((i & 0x10) != 0 ? 170 : 0), g + ((i & 0x20) != 0 ? 170 : 0), b + ((i & 0x40) != 0 ? 170 : 0), 127);
            else if (hi == 0x80) Set(i, 127 + r / 2 + ((i & 0x10) != 0 ? 43 : 0), 127 + g / 2 + ((i & 0x20) != 0 ? 43 : 0), 127 + b / 2 + ((i & 0x40) != 0 ? 43 : 0), 255);
            else Set(i, r / 2 + ((i & 0x10) != 0 ? 43 : 0), g / 2 + ((i & 0x20) != 0 ? 43 : 0), b / 2 + ((i & 0x40) != 0 ? 43 : 0), 255);
        }
        return table;
    }

    static byte[][] DefaultCluts() => new[] { DefaultClut(2), DefaultClut(4), DefaultClut(8) };

    sealed class Bits
    {
        readonly ByteView data;
        readonly int end;
        int at;
        int bit;

        public Bits(ByteView data, int at, int end)
        {
            this.data = data;
            this.at = at;
            this.end = end;
        }

        public bool Done => at >= end;

        public int Read(int n)
        {
            var v = 0;
            for (var i = 0; i < n; i++)
            {
                var b = at < end ? data[at] : 0;
                v = (v << 1) | ((b >> (7 - bit)) & 1);
                if (++bit == 8)
                {
                    bit = 0;
                    at++;
                }
            }
            return v;
        }

        /// <summary>The byte after the string: strings end on a byte.</summary>
        public int Align()
        {
            if (bit != 0)
            {
                bit = 0;
                at++;
            }
            return at;
        }
    }

    /// <summary>Runs of one pixel string: <c>(count, pixel)</c> pairs, until its end code.</summary>
    static void PixelString(Bits bits, int depth, Action<int, int> emit)
    {
        while (true)
        {
            if (bits.Done) return;
            if (depth == 2)
            {
                var v = bits.Read(2);
                if (v != 0) emit(1, v);
                else if (bits.Read(1) != 0)
                {
                    var run = bits.Read(3) + 3;
                    emit(run, bits.Read(2));
                }
                else if (bits.Read(1) != 0) emit(1, 0);
                else
                {
                    var s = bits.Read(2);
                    if (s == 0) return;
                    if (s == 1) emit(2, 0);
                    else if (s == 2)
                    {
                        var run = bits.Read(4) + 12;
                        emit(run, bits.Read(2));
                    }
                    else
                    {
                        var run = bits.Read(8) + 29;
                        emit(run, bits.Read(2));
                    }
                }
            }
            else if (depth == 4)
            {
                var v = bits.Read(4);
                if (v != 0) emit(1, v);
                else if (bits.Read(1) == 0)
                {
                    var run = bits.Read(3);
                    if (run == 0) return;
                    emit(run + 2, 0);
                }
                else if (bits.Read(1) == 0)
                {
                    var run = bits.Read(2) + 4;
                    emit(run, bits.Read(4));
                }
                else
                {
                    var s = bits.Read(2);
                    if (s == 0) emit(1, 0);
                    else if (s == 1) emit(2, 0);
                    else if (s == 2)
                    {
                        var run = bits.Read(4) + 9;
                        emit(run, bits.Read(4));
                    }
                    else
                    {
                        var run = bits.Read(8) + 25;
                        emit(run, bits.Read(4));
                    }
                }
            }
            else
            {
                var v = bits.Read(8);
                if (v != 0) emit(1, v);
                else if (bits.Read(1) == 0)
                {
                    var run = bits.Read(7);
                    if (run == 0) return;
                    emit(run, 0);
                }
                else
                {
                    var run = bits.Read(7);
                    emit(run, bits.Read(8));
                }
            }
        }
    }

    /// <summary>The default map tables of 2-bit codes to 4 and 8 bits, and of 4-bit codes to 8.</summary>
    static readonly int[] Map24 = { 0x0, 0x7, 0x8, 0xf };
    static readonly int[] Map28 = { 0x00, 0x77, 0x88, 0xff };
    static readonly int[] Map48 = Enumerable.Range(0, 16).Select(i => i * 0x11).ToArray();

    /// <summary>
    /// Draws one field of an object's pixel data into <paramref name="region"/> at (<paramref name="ox"/>,
    /// <paramref name="oy"/>), every other line from <paramref name="first"/>, up to the region's bottom; the work it
    /// took, bytes walked and pixels written.
    /// </summary>
    static long DrawField(ByteView data, Region region, int ox, int oy, int first)
    {
        long written = 0;
        var at = 0;
        var x = ox;
        var y = oy + first;
        // Map tables, for codes shallower than the region: the defaults until the data gives its own.
        var map24 = Map24;
        var map28 = Map28;
        var map48 = Map48;
        while (at < data.Length && y < region.Height)
        {
            var type = data[at++];
            if (type == 0xf0)
            {
                x = ox;
                y += 2;
                continue;
            }
            if (type == 0x20)
            {
                map24 = new[] { PictureColors.At(data, at) >> 4, PictureColors.At(data, at) & 0xf, PictureColors.At(data, at + 1) >> 4, PictureColors.At(data, at + 1) & 0xf };
                at += 2;
                continue;
            }
            if (type == 0x21)
            {
                map28 = new[] { PictureColors.At(data, at), PictureColors.At(data, at + 1), PictureColors.At(data, at + 2), PictureColors.At(data, at + 3) };
                at += 4;
                continue;
            }
            if (type == 0x22)
            {
                map48 = new int[16];
                for (var i = 0; i < 8; i++)
                {
                    map48[i * 2] = PictureColors.At(data, at + i) >> 4;
                    map48[i * 2 + 1] = PictureColors.At(data, at + i) & 0xf;
                }
                at += 8;
                continue;
            }
            var depth = type == 0x10 ? 2 : type == 0x11 ? 4 : type == 0x12 ? 8 : 0;
            if (depth == 0) break;
            var bits = new Bits(data, at, data.Length);
            int Map(int p)
            {
                if (depth == region.Depth) return p;
                if (depth == 2) return region.Depth == 4 ? map24[p] : map28[p];
                if (depth == 4 && region.Depth == 8) return map48[p];
                // Deeper codes than the region: keep the low bits.
                return p & ((1 << region.Depth) - 1);
            }
            PixelString(bits, depth, (count, pixel) =>
            {
                if (y < region.Height)
                {
                    var start = Math.Max(0, x);
                    var stop = Math.Min(region.Width, x + count);
                    var value = (byte)Map(pixel);
                    var row = y * region.Width;
                    for (var px = start; px < stop; px++) region.Pixels[row + px] = value;
                    if (stop > start) written += stop - start;
                }
                x += count;
            });
            at = bits.Align();
        }
        return (long)at * WalkCost + written;
    }

    public long? OwnDurationMs(int index)
    {
        var page = PageHead(samples[index]);
        return page != null && page.Value.TimeoutS > 0 ? page.Value.TimeoutS * 1000L : null;
    }

    public PictureFrame? Decode(int index) => cache.Get(index, Render);

    /// <summary>Display sets are read again for each frame decoded, rather than kept: a crafted track's would take many times its size.</summary>
    PictureFrame? Render(int index)
    {
        var from = index;
        while (from > 0 && index - from < PictureColors.MaxEpoch && (PageHead(samples[from])?.State ?? 0) == 0) from--;
        var width = 720;
        var height = 576;
        Page? page = null;
        // By id, in the order they were first defined (a Map's).
        var regions = new Dictionary<int, Region>();
        var regionOrder = new List<int>();
        // The pixels of the regions held, all together.
        var held = 0;
        var cluts = new Dictionary<int, byte[][]>();
        // Bytes read, pixels filled and drawn: a crafted epoch could fill a 4K region, or draw an
        // object into each of thousands of places, in every segment of every display set. A frame
        // that would take more than MaxWork isn't decoded: what it leaves out could be any of what
        // it shows.
        var work = PictureColors.MaxWork;
        for (var i = from; i <= index && work > 0; i++)
        {
            foreach (var seg in Segments(samples[i]))
            {
                var b = seg.Body;
                work -= SegmentCost + (long)b.Length * ReadCost;
                if (work <= 0) break;
                switch (seg.Type)
                {
                    case DisplaySegment:
                        if (b.Length >= 5)
                        {
                            width = ((b[1] << 8) | b[2]) + 1;
                            height = ((b[3] << 8) | b[4]) + 1;
                        }
                        break;
                    case PageSegment:
                        page = ReadPage(b);
                        if (page.State != 0)
                        {
                            regions.Clear();
                            regionOrder.Clear();
                            held = 0;
                        }
                        break;
                    case RegionSegment:
                    {
                        if (b.Length < 10) break;
                        var id = b[0];
                        var fill = (b[1] & 0x08) != 0;
                        var w = (b[2] << 8) | b[3];
                        var h = (b[4] << 8) | b[5];
                        if (w > PictureColors.MaxSide || h > PictureColors.MaxSide) break;
                        // Up to 256 regions are kept at once: together, no more than a frame's pixels.
                        var before = regions.TryGetValue(id, out var kept) ? kept.Pixels.Length : 0;
                        if (held - before + w * h > PictureColors.MaxPixels) break;
                        var code = (b[6] >> 2) & 7;
                        var depth = code < Depths.Length ? Depths[code] : 4;
                        var clut = b[7];
                        var background = depth == 8 ? b[8] : depth == 4 ? b[9] >> 4 : (b[9] >> 2) & 3;
                        var objects = new List<RegionObject>();
                        for (var o = 10; o + 6 <= b.Length && objects.Count < MaxRegionObjects;)
                        {
                            var objectType = b[o + 2] >> 6;
                            objects.Add(new RegionObject { Id = (b[o] << 8) | b[o + 1], X = ((b[o + 2] & 0x0f) << 8) | b[o + 3], Y = ((b[o + 4] & 0x0f) << 8) | b[o + 5] });
                            o += objectType == 1 || objectType == 2 ? 8 : 6;
                        }
                        if (!regions.TryGetValue(id, out var region) || region.Width != w || region.Height != h || region.Depth != depth)
                        {
                            work -= w * h;
                            held += w * h - before;
                            region = new Region { Width = w, Height = h, Depth = depth, Clut = clut, Fill = fill, Background = background, Objects = objects, Pixels = new byte[w * h] };
                            if (!regions.ContainsKey(id)) regionOrder.Add(id);
                            regions[id] = region;
                        }
                        else
                        {
                            region.Clut = clut;
                            region.Fill = fill;
                            region.Background = background;
                            region.Objects = objects;
                        }
                        if (fill)
                        {
                            work -= w * h;
                            for (var p = 0; p < region.Pixels.Length; p++) region.Pixels[p] = (byte)background;
                        }
                        break;
                    }
                    case ClutSegment:
                    {
                        if (b.Length < 2) break;
                        var id = b[0];
                        if (!cluts.TryGetValue(id, out var tables)) tables = DefaultCluts();
                        for (var at = 2; at + 2 <= b.Length;)
                        {
                            var entry = b[at];
                            var flags = b[at + 1];
                            int y, cr, cb, t;
                            if ((flags & 0x01) != 0)
                            {
                                if (at + 6 > b.Length) break;
                                y = b[at + 2];
                                cr = b[at + 3];
                                cb = b[at + 4];
                                t = b[at + 5];
                                at += 6;
                            }
                            else
                            {
                                if (at + 4 > b.Length) break;
                                y = b[at + 2] & 0xfc;
                                cr = ((b[at + 2] & 0x03) << 6) | ((b[at + 3] >> 2) & 0x30);
                                cb = (b[at + 3] << 2) & 0xf0;
                                t = (b[at + 3] & 0x03) * 0x55;
                                at += 4;
                            }
                            // Y 0 is transparent whatever T says.
                            var (r, g, bl) = PictureColors.YcbcrToRgb(y, cb, cr, height > 576);
                            var alpha = (byte)(y == 0 ? 0 : 255 - t);
                            void Put(byte[] table, int i)
                            {
                                if (i * 4 + 3 >= table.Length) return;
                                table[i * 4] = r;
                                table[i * 4 + 1] = g;
                                table[i * 4 + 2] = bl;
                                table[i * 4 + 3] = alpha;
                            }
                            if ((flags & 0x80) != 0) Put(tables[0], entry);
                            if ((flags & 0x40) != 0) Put(tables[1], entry);
                            if ((flags & 0x20) != 0) Put(tables[2], entry);
                        }
                        cluts[id] = tables;
                        break;
                    }
                    case ObjectSegment:
                    {
                        if (b.Length < 7) break;
                        var id = (b[0] << 8) | b[1];
                        var coding = (b[2] >> 2) & 3;
                        if (coding != 0) break;
                        var topLength = (b[3] << 8) | b[4];
                        var bottomLength = (b[5] << 8) | b[6];
                        var top = b.Slice(7, 7 + topLength);
                        var bottom = bottomLength != 0 ? b.Slice(7 + topLength, 7 + topLength + bottomLength) : top;
                        foreach (var regionId in regionOrder)
                        {
                            var region = regions[regionId];
                            work -= (1L + region.Objects.Count) * ScanCost;
                            foreach (var o in region.Objects)
                            {
                                if (work <= 0) break;
                                if (o.Id != id) continue;
                                work -= DrawCost + DrawField(top, region, o.X, o.Y, 0) + DrawField(bottom, region, o.X, o.Y, 1);
                            }
                        }
                        break;
                    }
                }
            }
        }
        if (page == null || work <= 0) return null;
        var bitmaps = new List<SubtitleBitmap>();
        // A page may place a region any number of times, each a picture of its own.
        var pixelsLeft = PictureColors.MaxPixels;
        foreach (var placed in page.Regions)
        {
            if (!regions.TryGetValue(placed.Id, out var region) || region.Width == 0 || region.Height == 0) continue;
            pixelsLeft -= region.Width * region.Height;
            if (pixelsLeft < 0) break;
            var table = (cluts.TryGetValue(region.Clut, out var own) ? own : DefaultCluts())[region.Depth == 2 ? 0 : region.Depth == 4 ? 1 : 2];
            var rgba = new byte[region.Width * region.Height * 4];
            var visible = false;
            for (var i = 0; i < region.Pixels.Length; i++)
            {
                var c = region.Pixels[i] * 4;
                // A pixel past a smaller table (an 8-bit value in a 2-bit region) has no colour.
                if (c + 3 >= table.Length) continue;
                var o = i * 4;
                rgba[o] = table[c];
                rgba[o + 1] = table[c + 1];
                rgba[o + 2] = table[c + 2];
                rgba[o + 3] = table[c + 3];
                if (table[c + 3] != 0) visible = true;
            }
            if (visible) bitmaps.Add(new SubtitleBitmap(placed.X, placed.Y, region.Width, region.Height, rgba));
        }
        return new PictureFrame(width, height, bitmaps);
    }
}
