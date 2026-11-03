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

/// <summary>What the decoders share: colours, byte reads, bounds, and their cache of decoded frames.</summary>
public static class PictureColors
{
    /// <summary>
    /// A picture's side at most, in pixels (a PGS object, a DVB region): subtitles are drawn for 4K
    /// screens at most, and a file's sizes are outside input.
    /// </summary>
    public const int MaxSide = 4096;

    /// <summary>
    /// The most pixels a frame decodes, all its pictures together: a 4K screen's worth. A file can
    /// place a 4096² picture many times over, and each pixel costs 4 bytes once coloured.
    /// </summary>
    public const int MaxPixels = 3840 * 2160;

    /// <summary>
    /// The most work decoding a frame takes, in pixels filled or drawn, a byte read costing as many
    /// pixels as it takes as long: eight 4K screens' worth, where a real frame takes a fraction of one.
    /// </summary>
    public const long MaxWork = 8L * MaxPixels;

    /// <summary>
    /// The segments read of one sample: a real display set has a handful, where a crafted one could
    /// hold 150,000 in a MiB, each kept as an object.
    /// </summary>
    public const int MaxSegments = 256;

    /// <summary>
    /// The most display sets a frame is decoded from, back to the last that holds everything it
    /// needs (an epoch start, an acquisition point): real tracks have one every few, or every few
    /// hundred in a long palette animation, and without that, each frame would read the whole track.
    /// </summary>
    public const int MaxEpoch = 4096;

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

    /// <inheritdoc cref="At(byte[], long)"/>
    internal static int At(ByteView data, long i) => i >= 0 && i < data.Length ? data[(int)i] : 0;

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
/// Bytes in place in an array, clamped to it, as a JavaScript <c>subarray</c>: a sample's segments
/// are read where they are, as copies of a crafted sample's would come to many times its size.
/// </summary>
public readonly struct ByteView
{
    readonly byte[] array;
    readonly int start;

    public ByteView(byte[] array, int start, int end)
    {
        this.array = array;
        this.start = Math.Max(0, Math.Min(array.Length, start));
        Length = Math.Max(0, Math.Min(array.Length, end) - this.start);
    }

    public ByteView(byte[] array) : this(array, 0, array.Length)
    {
    }

    public int Length { get; }

    public byte this[int i] => (uint)i < (uint)Length ? array[start + i] : throw new IndexOutOfRangeException();

    /// <summary>Bytes <paramref name="from"/> to <paramref name="to"/> of these, clamped to them.</summary>
    public ByteView Slice(int from, int to) => new ByteView(array, start + Math.Max(0, Math.Min(Length, from)), start + Math.Max(0, Math.Min(Length, to)));

    public void CopyTo(byte[] destination, int at) => Buffer.BlockCopy(array, start, destination, at, Length);

    public byte[] ToArray()
    {
        var copy = new byte[Length];
        CopyTo(copy, 0);
        return copy;
    }
}

/// <summary>
/// The frames decoded last: a few around the playhead are enough, as seeking decodes again. The
/// oldest goes first. A crafted file's frames can each be a 4K screen's worth, so they are kept by
/// bytes too, but the frame just decoded always stays: it is the one showing.
/// </summary>
internal sealed class FrameCache
{
    /// <summary>Frames kept at most, and the most bytes they hold together.</summary>
    public const int Kept = 25;
    public const long KeptBytes = 128L << 20;

    readonly Dictionary<int, PictureFrame?> frames = new Dictionary<int, PictureFrame?>();
    readonly Queue<int> order = new Queue<int>();
    readonly int most;
    readonly long mostBytes;
    long bytes;

    public FrameCache(int most = Kept, long mostBytes = KeptBytes)
    {
        this.most = most;
        this.mostBytes = mostBytes;
    }

    public bool Has(int index) => frames.ContainsKey(index);

    public PictureFrame? Get(int index, Func<int, PictureFrame?> make)
    {
        if (frames.TryGetValue(index, out var frame)) return frame;
        frame = make(index);
        frames[index] = frame;
        order.Enqueue(index);
        bytes += Bytes(frame);
        while (order.Count > 1 && (order.Count > most || bytes > mostBytes))
        {
            var dropped = order.Dequeue();
            bytes -= Bytes(frames[dropped]);
            frames.Remove(dropped);
        }
        return frame;
    }

    static long Bytes(PictureFrame? frame)
    {
        long sum = 0;
        if (frame != null)
            foreach (var b in frame.Bitmaps) sum += b.Rgba.Length;
        return sum;
    }
}
