using System;
using System.Collections.Generic;

namespace Pley.Tv.Core.Subtitles.Pictures;

/// <summary>
/// Blu-ray subtitles (HDMV PGS, <c>S_HDMV/PGS</c>): each sample is one display set of segments: a
/// presentation composition (PCS) placing objects on the plane, windows (WDS), palettes (PDS) and
/// run-length coded objects (ODS). An epoch start or acquisition point holds everything it needs;
/// other display sets may reuse palettes and objects defined since.
/// </summary>
public sealed class PgsDecoder : IPictureDecoder
{
    const int Pds = 0x14;
    const int Ods = 0x15;
    const int Pcs = 0x16;
    const int Wds = 0x17;
    const int End = 0x80;

    internal sealed class Crop
    {
        public int X, Y, W, H;
    }

    internal sealed class Placed
    {
        public int Id, X, Y;
        public Crop? Crop;
    }

    internal sealed class Composition
    {
        public int Width, Height;
        /// <summary>0x00 normal, 0x40 acquisition point, 0x80 epoch start.</summary>
        public int State;
        public int PaletteId;
        public List<Placed> Objects = new List<Placed>();
    }

    /// <summary>The objects a composition places at most: Blu-ray's two, as FFmpeg reads them.</summary>
    const int MaxPlaced = 2;

    /// <summary>
    /// What reading a byte of a display set costs against <see cref="PictureColors.MaxWork"/>, next
    /// to filling a pixel: reading it, and its objects' run-lengths later, take about 4 times as long.
    /// </summary>
    const int ReadCost = 4;

    internal sealed class Palette
    {
        public int Id;
        /// <summary>Its entries as they come, 5 bytes each (index, Y, Cr, Cb, alpha), one for each index at most.</summary>
        public byte[] Entries = new byte[0];
    }

    internal sealed class ObjectPart
    {
        public int Id, Width, Height;
        public bool First, Last;
        public byte[] Data = new byte[0];
    }

    internal sealed class DisplaySet
    {
        public Composition? Pcs;
        public List<Palette> Palettes = new List<Palette>();
        public List<ObjectPart> Objects = new List<ObjectPart>();
    }

    sealed class ObjectData
    {
        public int Width, Height;
        /// <summary>Its run-length data, in the parts it came in.</summary>
        public List<byte[]> Parts = new List<byte[]>();
    }

    readonly IReadOnlyList<byte[]> samples;
    readonly FrameCache cache = new FrameCache();

    public PgsDecoder(IReadOnlyList<byte[]> samples) => this.samples = samples;

    /// <summary>
    /// Calls <paramref name="each"/> with the type and body of a display set's segments,
    /// <see cref="PictureColors.MaxSegments"/> at most, up to its end.
    /// </summary>
    static void EachSegment(byte[] data, Action<int, int, int> each)
    {
        var at = 0;
        for (var n = 0; n < PictureColors.MaxSegments && at + 3 <= data.Length; n++)
        {
            // Some muxers keep the "PG" header of the .sup format: 2 magic bytes, PTS and DTS.
            if (PictureColors.At(data, at) == 0x50 && PictureColors.At(data, at + 1) == 0x47) at += 10;
            var type = PictureColors.At(data, at);
            var size = (PictureColors.At(data, at + 1) << 8) | PictureColors.At(data, at + 2);
            var start = at + 3;
            at = start + size;
            each(type, start, Math.Min(data.Length, at));
            if (type == End) return;
        }
    }

    /// <summary>The state of a display set's composition, read without the rest of it.</summary>
    static int StateOf(byte[] data)
    {
        var state = 0;
        EachSegment(data, (type, start, end) =>
        {
            if (type == Pcs) state = end - start >= 11 ? data[start + 7] : 0;
        });
        return state;
    }

    internal static DisplaySet ParseDisplaySet(byte[] data)
    {
        var set = new DisplaySet();
        EachSegment(data, (type, start, end) =>
        {
            var body = PictureColors.Slice(data, start, end);
            switch (type)
            {
                case Pcs:
                    set.Pcs = ReadPcs(body);
                    break;
                case Pds:
                {
                    if (body.Length < 2) break;
                    var entries = Math.Min((body.Length - 2) / 5, 256);
                    set.Palettes.Add(new Palette { Id = body[0], Entries = PictureColors.Slice(body, 2, 2 + entries * 5) });
                    break;
                }
                case Ods:
                {
                    if (body.Length < 4) break;
                    var id = (body[0] << 8) | body[1];
                    var flags = body[3];
                    var first = (flags & 0x80) != 0;
                    var last = (flags & 0x40) != 0;
                    if (first)
                    {
                        if (body.Length < 11) break;
                        var width = (body[7] << 8) | body[8];
                        var height = (body[9] << 8) | body[10];
                        set.Objects.Add(new ObjectPart { Id = id, First = first, Last = last, Width = width, Height = height, Data = PictureColors.Slice(body, 11, body.Length) });
                    }
                    else
                    {
                        set.Objects.Add(new ObjectPart { Id = id, First = first, Last = last, Data = PictureColors.Slice(body, 4, body.Length) });
                    }
                    break;
                }
                case Wds:
                case End:
                default:
                    break;
            }
        });
        return set;
    }

    static Composition? ReadPcs(byte[] b)
    {
        if (b.Length < 11) return null;
        var pcs = new Composition
        {
            Width = (b[0] << 8) | b[1],
            Height = (b[2] << 8) | b[3],
            State = b[7],
            PaletteId = b[9],
        };
        var count = Math.Min((int)b[10], MaxPlaced);
        var at = 11;
        for (var i = 0; i < count && at + 8 <= b.Length; i++)
        {
            var placed = new Placed
            {
                Id = (b[at] << 8) | b[at + 1],
                X = (b[at + 4] << 8) | b[at + 5],
                Y = (b[at + 6] << 8) | b[at + 7],
            };
            // 0x80 says a crop follows; 0x40 is forced.
            var cropped = (b[at + 3] & 0x80) != 0;
            at += 8;
            if (cropped && at + 8 <= b.Length)
            {
                placed.Crop = new Crop
                {
                    X = (b[at] << 8) | b[at + 1],
                    Y = (b[at + 2] << 8) | b[at + 3],
                    W = (b[at + 4] << 8) | b[at + 5],
                    H = (b[at + 6] << 8) | b[at + 7],
                };
                at += 8;
            }
            pcs.Objects.Add(placed);
        }
        return pcs;
    }

    /// <summary>Decodes PGS run-length data into palette indexes, <c>width × height</c>.</summary>
    public static byte[] DecodeRle(byte[] rle, int width, int height)
    {
        var output = new byte[width * height];
        var x = 0;
        var y = 0;
        var i = 0;
        while (i < rle.Length && y < height)
        {
            var b = rle[i++];
            var color = 0;
            var count = 1;
            if (b != 0)
            {
                color = b;
            }
            else
            {
                var flags = PictureColors.At(rle, i++);
                if (flags == 0)
                {
                    // End of line.
                    x = 0;
                    y++;
                    continue;
                }
                var isLong = (flags & 0x40) != 0;
                count = isLong ? ((flags & 0x3f) << 8) | PictureColors.At(rle, i++) : flags & 0x3f;
                color = (flags & 0x80) != 0 ? PictureColors.At(rle, i++) : 0;
            }
            var row = y * width;
            var end = Math.Min(width, x + count);
            if (color != 0)
                for (var px = x; px < end; px++) output[row + px] = (byte)color;
            x = end;
        }
        return output;
    }

    public long? OwnDurationMs(int index) => null;

    public PictureFrame? Decode(int index) => cache.Get(index, Compose);

    /// <summary>Display sets are read again for each frame decoded, rather than kept: a crafted track's would take many times its size.</summary>
    PictureFrame? Compose(int index)
    {
        var current = ParseDisplaySet(samples[index]);
        var pcs = current.Pcs;
        if (pcs == null) return null;
        if (pcs.Objects.Count == 0) return new PictureFrame(pcs.Width, pcs.Height, new SubtitleBitmap[0]);
        // Palettes and objects as they stand at `index`: from the last epoch start or acquisition point.
        var from = index;
        while (from > 0 && index - from < PictureColors.MaxEpoch && (StateOf(samples[from]) & 0xc0) == 0) from--;
        // Each palette's entries by index: whether set, then Y, Cr, Cb and alpha.
        var palettes = new Dictionary<int, byte[]>();
        var objects = new Dictionary<int, ObjectData>();
        var pending = new Dictionary<int, ObjectData>();
        var work = PictureColors.MaxWork;
        for (var i = from; i <= index && work > 0; i++)
        {
            var data = samples[i];
            work -= (1L + data.Length) * ReadCost;
            var s = i == index ? current : ParseDisplaySet(data);
            foreach (var p in s.Palettes)
            {
                if (!palettes.TryGetValue(p.Id, out var palette)) palettes[p.Id] = palette = new byte[256 * 5];
                for (var e = 0; e + 5 <= p.Entries.Length; e += 5)
                {
                    var at = p.Entries[e] * 5;
                    palette[at] = 1;
                    for (var k = 1; k < 5; k++) palette[at + k] = p.Entries[e + k];
                }
            }
            foreach (var o in s.Objects)
            {
                if (o.First) pending[o.Id] = new ObjectData { Width = o.Width, Height = o.Height };
                if (!pending.TryGetValue(o.Id, out var part)) continue;
                part.Parts.Add(o.Data);
                if (o.Last)
                {
                    objects[o.Id] = part;
                    pending.Remove(o.Id);
                }
            }
        }
        if (!palettes.TryGetValue(pcs.PaletteId, out var chosen)) return null;
        var hd = pcs.Height > 576;
        var rgba = new byte[256 * 4];
        for (var i = 0; i < 256; i++)
        {
            var at = i * 5;
            if (chosen[at] == 0) continue;
            var (r, g, b) = PictureColors.YcbcrToRgb(chosen[at + 1], chosen[at + 3], chosen[at + 2], hd);
            rgba[i * 4] = r;
            rgba[i * 4 + 1] = g;
            rgba[i * 4 + 2] = b;
            rgba[i * 4 + 3] = chosen[at + 4];
        }
        var bitmaps = new List<SubtitleBitmap>();
        // Each placement is a picture of its own, the same object's too: together, no more than a frame's pixels.
        var pixelsLeft = PictureColors.MaxPixels;
        foreach (var placed in pcs.Objects)
        {
            if (!objects.TryGetValue(placed.Id, out var obj) || obj.Width == 0 || obj.Height == 0) continue;
            if (obj.Width > PictureColors.MaxSide || obj.Height > PictureColors.MaxSide) continue;
            pixelsLeft -= obj.Width * obj.Height;
            if (pixelsLeft < 0) break;
            var indexes = DecodeRle(Join(obj.Parts), obj.Width, obj.Height);
            var crop = placed.Crop ?? new Crop { X = 0, Y = 0, W = obj.Width, H = obj.Height };
            var w = Math.Max(0, Math.Min(crop.W, obj.Width - crop.X));
            var h = Math.Max(0, Math.Min(crop.H, obj.Height - crop.Y));
            if (w == 0 || h == 0) continue;
            var pixels = new byte[w * h * 4];
            for (var row = 0; row < h; row++)
            {
                var src = (crop.Y + row) * obj.Width + crop.X;
                var dst = row * w * 4;
                for (var col = 0; col < w; col++)
                {
                    var c = indexes[src + col] * 4;
                    var o = dst + col * 4;
                    pixels[o] = rgba[c];
                    pixels[o + 1] = rgba[c + 1];
                    pixels[o + 2] = rgba[c + 2];
                    pixels[o + 3] = rgba[c + 3];
                }
            }
            bitmaps.Add(new SubtitleBitmap(placed.X, placed.Y, w, h, pixels));
        }
        return new PictureFrame(pcs.Width, pcs.Height, bitmaps);
    }

    static byte[] Join(List<byte[]> parts)
    {
        if (parts.Count == 1) return parts[0];
        var length = 0;
        foreach (var p in parts) length += p.Length;
        var joined = new byte[length];
        var at = 0;
        foreach (var p in parts)
        {
            Buffer.BlockCopy(p, 0, joined, at, p.Length);
            at += p.Length;
        }
        return joined;
    }
}
