using System;
using System.Collections.Generic;
using System.Globalization;
using System.Text;
using System.Text.RegularExpressions;

namespace Pley.Tv.Core.Subtitles.Pictures;

/// <summary>The <c>.idx</c> header of a VobSub track: the plane's size and the 16 colours of its palette.</summary>
public sealed class VobSubHeader
{
    public VobSubHeader(int width, int height, IReadOnlyList<(byte R, byte G, byte B)> palette)
    {
        Width = width;
        Height = height;
        Palette = palette;
    }

    public int Width { get; }
    public int Height { get; }
    /// <summary>16 RGB colours.</summary>
    public IReadOnlyList<(byte R, byte G, byte B)> Palette { get; }
}

/// <summary>The control sequences of a subpicture unit: where it goes, its colours and alpha, and when it shows and stops.</summary>
public sealed class Spu
{
    public int X1, Y1, X2 = -1, Y2 = -1;
    /// <summary>Palette entries for pixel values 0-3.</summary>
    public int[] Colors = { 0, 1, 2, 3 };
    /// <summary>Alpha 0-15 for pixel values 0-3.</summary>
    public int[] Alpha = { 0, 15, 15, 15 };
    public int Top, Bottom;
    /// <summary>From the stop command, in ms; null without one.</summary>
    public long? StopMs;
    public long StartMs;
}

/// <summary>
/// DVD subtitles (VobSub, <c>S_VOBSUB</c>): each sample is one subpicture unit (SPU): two
/// interlaced fields of 2-bit run-length pixels, then control sequences that say when it shows and
/// stops, which 4 of the 16 palette colours its pixels take, their alpha, and where it goes. The
/// palette and the plane's size come from the track's private data, the <c>.idx</c> header
/// (<c>size: 720x480</c>, <c>palette: 000000, ffffff, …</c>).
/// </summary>
public sealed class VobSubDecoder : IPictureDecoder
{
    static readonly Regex Size = new Regex(@"size:\s*(\d+)\s*x\s*(\d+)", RegexOptions.IgnoreCase | RegexOptions.CultureInvariant);
    static readonly Regex PaletteLine = new Regex(@"palette:\s*([0-9a-f,\s]+)", RegexOptions.IgnoreCase | RegexOptions.CultureInvariant);

    readonly IReadOnlyList<byte[]> samples;
    readonly VobSubHeader header;
    readonly Spu?[] spus;
    readonly bool[] parsed;
    readonly FrameCache cache = new FrameCache();

    public VobSubDecoder(IReadOnlyList<byte[]> samples, byte[]? codecPrivate)
    {
        this.samples = samples;
        header = ParseIdx(codecPrivate);
        spus = new Spu?[samples.Count];
        parsed = new bool[samples.Count];
    }

    /// <summary>An SPU's bytes at most: its size is a 16-bit field.</summary>
    const int MaxSpuBytes = 0xffff;

    /// <summary>The control sequences read of an SPU: a real one has two or three.</summary>
    const int MaxSequences = 64;

    public static VobSubHeader ParseIdx(byte[]? codecPrivate)
    {
        // The header is a KiB or two; its palette, 16 colours.
        var text = codecPrivate != null ? Latin1(codecPrivate, Math.Min(codecPrivate.Length, 64 << 10)) : "";
        var size = Size.Match(text);
        var palette = new List<(byte, byte, byte)>();
        var p = PaletteLine.Match(text);
        if (p.Success)
        {
            foreach (var hex in p.Groups[1].Value.Split(new[] { ',' }, 16))
            {
                if (LeadingHex(hex.Trim()) is long v) palette.Add(((byte)((v >> 16) & 0xff), (byte)((v >> 8) & 0xff), (byte)(v & 0xff)));
            }
        }
        // A DVD's default greys when the header has none.
        while (palette.Count < 16) palette.Add(palette.Count % 2 == 1 ? ((byte)255, (byte)255, (byte)255) : ((byte)0, (byte)0, (byte)0));
        return new VobSubHeader(size.Success ? Number(size.Groups[1].Value, 720) : 720, size.Success ? Number(size.Groups[2].Value, 576) : 576, palette);
    }

    /// <summary>The hex digits <paramref name="s"/> starts with, as <c>parseInt(s, 16)</c> reads them; null without any.</summary>
    static long? LeadingHex(string s)
    {
        long value = 0;
        var digits = 0;
        foreach (var c in s)
        {
            int d = c >= '0' && c <= '9' ? c - '0' : c >= 'a' && c <= 'f' ? c - 'a' + 10 : c >= 'A' && c <= 'F' ? c - 'A' + 10 : -1;
            if (d < 0) break;
            value = unchecked(value * 16 + d);
            digits++;
        }
        return digits > 0 ? value : null;
    }

    static int Number(string digits, int fallback) => int.TryParse(digits, NumberStyles.None, CultureInfo.InvariantCulture, out var n) ? n : fallback;

    /// <summary>Reads the control sequences of an SPU; null when it is cut short.</summary>
    public static Spu? ParseSpu(byte[] data)
    {
        if (data.Length < 4) return null;
        var size = (data[0] << 8) | data[1];
        var ctrl = (data[2] << 8) | data[3];
        var end = Math.Min(size != 0 ? size : MaxSpuBytes, data.Length);
        var spu = new Spu();
        var seq = ctrl;
        var seen = new HashSet<int>();
        while (seq + 4 <= end && seen.Count < MaxSequences && seen.Add(seq))
        {
            // Delays count in 1024 ticks of a 90 kHz clock.
            var delayMs = (long)Math.Floor(((data[seq] << 8) | data[seq + 1]) * 1024 / 90.0 + 0.5);
            var next = (data[seq + 2] << 8) | data[seq + 3];
            var at = seq + 4;
            var done = false;
            while (at < end && !done)
            {
                var cmd = data[at++];
                switch (cmd)
                {
                    case 0x00: // forced start
                    case 0x01: // start
                        spu.StartMs = delayMs;
                        break;
                    case 0x02: // stop
                        spu.StopMs = delayMs;
                        break;
                    case 0x03:
                        if (at + 2 > end) return null;
                        spu.Colors = new[] { data[at + 1] & 0x0f, data[at + 1] >> 4, data[at] & 0x0f, data[at] >> 4 };
                        at += 2;
                        break;
                    case 0x04:
                        if (at + 2 > end) return null;
                        spu.Alpha = new[] { data[at + 1] & 0x0f, data[at + 1] >> 4, data[at] & 0x0f, data[at] >> 4 };
                        at += 2;
                        break;
                    case 0x05:
                        if (at + 6 > end) return null;
                        spu.X1 = (data[at] << 4) | (data[at + 1] >> 4);
                        spu.X2 = ((data[at + 1] & 0x0f) << 8) | data[at + 2];
                        spu.Y1 = (data[at + 3] << 4) | (data[at + 4] >> 4);
                        spu.Y2 = ((data[at + 4] & 0x0f) << 8) | data[at + 5];
                        at += 6;
                        break;
                    case 0x06:
                        if (at + 4 > end) return null;
                        spu.Top = (data[at] << 8) | data[at + 1];
                        spu.Bottom = (data[at + 2] << 8) | data[at + 3];
                        at += 4;
                        break;
                    case 0xff:
                        done = true;
                        break;
                    default:
                        // An unknown command: its length isn't known, so the sequence ends here.
                        done = true;
                        break;
                }
            }
            if (next == seq) break;
            seq = next;
        }
        return spu;
    }

    /// <summary>One field's lines (every other line of the picture) into <paramref name="output"/>, starting at line <paramref name="first"/>.</summary>
    static void DecodeField(byte[] data, int offset, int end, int width, int height, int first, byte[] output)
    {
        var nibble = (long)offset * 2;
        var limit = (long)end * 2;
        int Read()
        {
            if (nibble >= limit) return 0;
            var b = data[nibble >> 1];
            var v = (nibble & 1) != 0 ? b & 0x0f : b >> 4;
            nibble++;
            return v;
        }
        for (var y = first; y < height; y += 2)
        {
            var x = 0;
            while (x < width)
            {
                var v = Read();
                if (v < 0x4)
                {
                    v = (v << 4) | Read();
                    if (v < 0x10)
                    {
                        v = (v << 4) | Read();
                        if (v < 0x40) v = (v << 4) | Read();
                    }
                }
                var color = (byte)(v & 3);
                var count = v >> 2;
                if (count == 0) count = width - x; // To the end of the line.
                var stop = Math.Min(width, x + count);
                var row = y * width;
                for (var px = x; px < stop; px++) output[row + px] = color;
                x = stop;
                if (nibble >= limit) break;
            }
            // Lines start on a byte.
            if ((nibble & 1) != 0) nibble++;
        }
    }

    Spu? SpuOf(int index)
    {
        if (!parsed[index])
        {
            spus[index] = ParseSpu(samples[index]);
            parsed[index] = true;
        }
        return spus[index];
    }

    public long? OwnDurationMs(int index)
    {
        var s = SpuOf(index);
        return s != null && s.StopMs is long stop && stop > s.StartMs ? stop - s.StartMs : null;
    }

    public PictureFrame? Decode(int index) => cache.Get(index, Render);

    PictureFrame? Render(int index)
    {
        var spu = SpuOf(index);
        if (spu == null) return null;
        var width = spu.X2 - spu.X1 + 1;
        var height = spu.Y2 - spu.Y1 + 1;
        if (width <= 0 || height <= 0) return new PictureFrame(header.Width, header.Height, new SubtitleBitmap[0]);
        // Its corners allow 4096² pixels: more than a frame's isn't decoded.
        if (width * height > PictureColors.MaxPixels) return null;
        var data = samples[index];
        var size = (data[0] << 8) | data[1];
        var ctrl = (data[2] << 8) | data[3];
        var end = Math.Min(ctrl != 0 ? ctrl : size != 0 ? size : MaxSpuBytes, data.Length);
        var pixels = new byte[width * height];
        DecodeField(data, spu.Top, end, width, height, 0, pixels);
        DecodeField(data, spu.Bottom, end, width, height, 1, pixels);
        var lut = new byte[16];
        for (var v = 0; v < 4; v++)
        {
            var c = header.Palette[spu.Colors[v]];
            lut[v * 4] = c.R;
            lut[v * 4 + 1] = c.G;
            lut[v * 4 + 2] = c.B;
            lut[v * 4 + 3] = (byte)(spu.Alpha[v] * 17);
        }
        var rgba = new byte[width * height * 4];
        for (var i = 0; i < pixels.Length; i++)
        {
            var c = pixels[i] * 4;
            var o = i * 4;
            rgba[o] = lut[c];
            rgba[o + 1] = lut[c + 1];
            rgba[o + 2] = lut[c + 2];
            rgba[o + 3] = lut[c + 3];
        }
        return new PictureFrame(header.Width, header.Height, new[] { new SubtitleBitmap(spu.X1, spu.Y1, width, height, rgba) });
    }

    /// <summary>The first <paramref name="count"/> of <paramref name="bytes"/> as Latin-1 text.</summary>
    static string Latin1(byte[] bytes, int count)
    {
        var s = new StringBuilder(count);
        for (var i = 0; i < count; i++) s.Append((char)bytes[i]);
        return s.ToString();
    }
}
