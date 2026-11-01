using System;
using System.Collections.Generic;

namespace Pley.Tv.Core.Subtitles.Pictures;

/// <summary>One bitmap of a picture subtitle, placed on its plane.</summary>
public sealed class SubtitleBitmap
{
    public SubtitleBitmap(int x, int y, int width, int height, byte[] rgba)
    {
        X = x;
        Y = y;
        Width = width;
        Height = height;
        Rgba = rgba;
    }

    public int X { get; }
    public int Y { get; }
    public int Width { get; }
    public int Height { get; }
    /// <summary>RGBA, <c>Width × Height × 4</c> bytes, not premultiplied.</summary>
    public byte[] Rgba { get; }
}

/// <summary>
/// What picture subtitles become once decoded (packages/picture-subtitles): bitmaps placed on the
/// subtitle plane (1920×1080 for a Blu-ray's PGS, 720×576 for a DVD's VobSub), drawn over the
/// whole screen at the same fractions.
/// </summary>
public sealed class PictureFrame
{
    public PictureFrame(int planeWidth, int planeHeight, IReadOnlyList<SubtitleBitmap> bitmaps)
    {
        PlaneWidth = planeWidth;
        PlaneHeight = planeHeight;
        Bitmaps = bitmaps;
    }

    /// <summary>The plane the bitmaps are placed on.</summary>
    public int PlaneWidth { get; }
    public int PlaneHeight { get; }
    /// <summary>Nothing to show: the screen clears.</summary>
    public IReadOnlyList<SubtitleBitmap> Bitmaps { get; }
}

/// <summary>A decoder for one track's samples, which may need earlier samples to decode a later one.</summary>
public interface IPictureDecoder
{
    /// <summary>The frame sample <paramref name="index"/> shows; null when it can't be decoded.</summary>
    PictureFrame? Decode(int index);

    /// <summary>
    /// How long sample <paramref name="index"/> shows when the sample itself says (VobSub's stop
    /// command, DVB's page time-out), else null: until the next sample.
    /// </summary>
    long? OwnDurationMs(int index);
}

/// <summary>What the decoders share: colours, byte reads, and their cache of decoded frames.</summary>
public static class PictureColors
{
    /// <summary>Limited-range YCbCr to RGB: BT.709 for HD planes, BT.601 for SD (as FFmpeg's decoders).</summary>
    public static (byte R, byte G, byte B) YcbcrToRgb(int y, int cb, int cr, bool hd)
    {
        var yy = 1.164383 * (y - 16);
        var u = cb - 128;
        var v = cr - 128;
        double r, g, b;
        if (hd)
        {
            r = yy + 1.792741 * v;
            g = yy - 0.213249 * u - 0.532909 * v;
            b = yy + 2.112402 * u;
        }
        else
        {
            r = yy + 1.596027 * v;
            g = yy - 0.391762 * u - 0.812968 * v;
            b = yy + 2.017232 * u;
        }
        return (Round(r), Round(g), Round(b));
    }

    /// <summary>Clamped to a byte, rounding half up as JavaScript's <c>Math.round</c>.</summary>
    static byte Round(double x) => x < 0 ? (byte)0 : x > 255 ? (byte)255 : (byte)Math.Floor(x + 0.5);

    /// <summary>Clamped to a byte as a <c>Uint8ClampedArray</c> stores it: half to even.</summary>
    internal static byte Clamp(double x) => x <= 0 ? (byte)0 : x >= 255 ? (byte)255 : (byte)Math.Round(x, MidpointRounding.ToEven);

    /// <summary>The byte at <paramref name="i"/>, 0 past the end (as JavaScript reads an undefined one in arithmetic).</summary>
    internal static int At(byte[] data, long i) => i >= 0 && i < data.Length ? data[i] : 0;

    /// <summary>Bytes <paramref name="start"/> to <paramref name="end"/>, clamped to the array (as <c>subarray</c>).</summary>
    internal static byte[] Slice(byte[] data, long start, long end)
    {
        var s = (int)Math.Max(0, Math.Min(data.Length, start));
        var e = (int)Math.Max(s, Math.Min(data.Length, end));
        var copy = new byte[e - s];
        Buffer.BlockCopy(data, s, copy, 0, copy.Length);
        return copy;
    }
}

/// <summary>
/// The frames decoded last: a few around the playhead are enough, as seeking decodes again. The
/// oldest goes first.
/// </summary>
internal sealed class FrameCache
{
    const int Kept = 24;

    readonly Dictionary<int, PictureFrame?> frames = new Dictionary<int, PictureFrame?>();
    readonly Queue<int> order = new Queue<int>();

    public PictureFrame? Get(int index, Func<int, PictureFrame?> make)
    {
        if (frames.TryGetValue(index, out var frame)) return frame;
        frame = make(index);
        if (frames.Count > Kept) frames.Remove(order.Dequeue());
        frames[index] = frame;
        order.Enqueue(index);
        return frame;
    }
}
