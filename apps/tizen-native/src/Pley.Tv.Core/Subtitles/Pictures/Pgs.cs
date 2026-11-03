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

    internal sealed class PaletteEntry
    {
        public int Index, Y, Cr, Cb, A;
    }

    internal sealed class Palette
    {
        public int Id;
        public List<PaletteEntry> Entries = new List<PaletteEntry>();
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
        public byte[] Rle = new byte[0];
    }

    sealed class Pending
    {
        public int Width, Height;
        public List<byte[]> Parts = new List<byte[]>();
    }

    readonly IReadOnlyList<byte[]> samples;
    readonly DisplaySet?[] sets;
    readonly FrameCache cache = new FrameCache();

    public PgsDecoder(IReadOnlyList<byte[]> samples)
    {
        this.samples = samples;
        sets = new DisplaySet?[samples.Count];
    }

    internal static DisplaySet ParseDisplaySet(byte[] data)
    {
        var set = new DisplaySet();
        var at = 0;
        while (at + 3 <= data.Length)
        {
            // Some muxers keep the "PG" header of the .sup format: 2 magic bytes, PTS and DTS.
            if (PictureColors.At(data, at) == 0x50 && PictureColors.At(data, at + 1) == 0x47) at += 10;
            var type = PictureColors.At(data, at);
            var size = (PictureColors.At(data, at + 1) << 8) | PictureColors.At(data, at + 2);
            var body = PictureColors.Slice(data, at + 3, Math.Min(data.Length, at + 3 + size));
            at += 3 + size;
            switch (type)
            {
                case Pcs:
                    set.Pcs = ReadPcs(body);
                    break;
                case Pds:
                {
                    if (body.Length < 2) break;
                    var palette = new Palette { Id = body[0] };
                    for (var i = 2; i + 5 <= body.Length; i += 5)
                        palette.Entries.Add(new PaletteEntry { Index = body[i], Y = body[i + 1], Cr = body[i + 2], Cb = body[i + 3], A = body[i + 4] });
                    set.Palettes.Add(palette);
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
            if (type == End) break;
        }
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
        var count = b[10];
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

    DisplaySet Set(int index) => sets[index] ??= ParseDisplaySet(samples[index]);

    public long? OwnDurationMs(int index) => null;

    public PictureFrame? Decode(int index) => cache.Get(index, Compose);

    PictureFrame? Compose(int index)
    {
        var pcs = Set(index).Pcs;
        if (pcs == null) return null;
        if (pcs.Objects.Count == 0) return new PictureFrame(pcs.Width, pcs.Height, new SubtitleBitmap[0]);
        // Palettes and objects as they stand at `index`: from the last epoch start or acquisition point.
        var from = index;
        while (from > 0)
        {
            var state = Set(from).Pcs?.State ?? 0;
            if ((state & 0xc0) != 0) break;
            from--;
        }
        var palettes = new Dictionary<int, Dictionary<int, PaletteEntry>>();
        var objects = new Dictionary<int, ObjectData>();
        var pending = new Dictionary<int, Pending>();
        for (var i = from; i <= index; i++)
        {
            var s = Set(i);
            foreach (var p in s.Palettes)
            {
                if (!palettes.TryGetValue(p.Id, out var palette)) palettes[p.Id] = palette = new Dictionary<int, PaletteEntry>();
                foreach (var e in p.Entries) palette[e.Index] = e;
            }
            foreach (var o in s.Objects)
            {
                if (o.First) pending[o.Id] = new Pending { Width = o.Width, Height = o.Height };
                if (!pending.TryGetValue(o.Id, out var part)) continue;
                part.Parts.Add(o.Data);
                if (o.Last)
                {
                    objects[o.Id] = new ObjectData { Width = part.Width, Height = part.Height, Rle = Join(part.Parts) };
                    pending.Remove(o.Id);
                }
            }
        }
        if (!palettes.TryGetValue(pcs.PaletteId, out var chosen)) return null;
        var hd = pcs.Height > 576;
        var rgba = new byte[256 * 4];
        foreach (var entry in chosen)
        {
            var e = entry.Value;
            var (r, g, b) = PictureColors.YcbcrToRgb(e.Y, e.Cb, e.Cr, hd);
            var at = entry.Key * 4;
            rgba[at] = r;
            rgba[at + 1] = g;
            rgba[at + 2] = b;
            rgba[at + 3] = (byte)e.A;
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
            var indexes = DecodeRle(obj.Rle, obj.Width, obj.Height);
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
