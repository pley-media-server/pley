using System;
using System.Collections.Generic;

namespace Pley.Tv.Core.Local;

/// <summary>A decoded picture: its size and RGBA bytes (Width × Height × 4).</summary>
public sealed record StillPicture(int Width, int Height, byte[] Rgba);

/// <summary>
/// The first picture of an MPEG-2 video stream, when it's an I-picture: what a DVD menu's still
/// is (one picture, which the TV's player won't show alone), and how an animated menu begins.
/// Intra pictures only, 4:2:0, frame pictures, as ISO/IEC 13818-2 decodes them: the variable-length
/// tables are its annex B's, the quantisation and mismatch control its 7.4, an 8×8 inverse DCT.
/// Anything else (MPEG-1, field pictures, a P- or B-picture first) gives null.
/// </summary>
public static class MpegStill
{
    /// <summary>The picture <paramref name="es"/> (a video elementary stream, from its sequence header) starts with; null when it can't be decoded here.</summary>
    public static StillPicture? Decode(byte[] es)
    {
        try
        {
            return new Decoder(es).Run();
        }
        catch (Exception e) when (e is IndexOutOfRangeException or InvalidOperationException or ArgumentException)
        {
            return null;
        }
    }

    /// <summary>The default intra quantiser matrix, in zigzag order (7.3.2).</summary>
    static readonly byte[] DefaultIntra =
    {
        8, 16, 16, 19, 16, 19, 22, 22, 22, 22, 22, 22, 26, 24, 26, 27,
        27, 27, 26, 26, 26, 26, 27, 27, 27, 29, 29, 29, 34, 34, 34, 29,
        29, 29, 27, 27, 29, 29, 32, 32, 34, 34, 37, 38, 37, 35, 35, 34,
        35, 38, 38, 40, 40, 40, 48, 48, 46, 46, 56, 56, 58, 69, 69, 83,
    };

    static readonly byte[] Zigzag =
    {
        0, 1, 8, 16, 9, 2, 3, 10, 17, 24, 32, 25, 18, 11, 4, 5,
        12, 19, 26, 33, 40, 48, 41, 34, 27, 20, 13, 6, 7, 14, 21, 28,
        35, 42, 49, 56, 57, 50, 43, 36, 29, 22, 15, 23, 30, 37, 44, 51,
        58, 59, 52, 45, 38, 31, 39, 46, 53, 60, 61, 54, 47, 55, 62, 63,
    };

    static readonly byte[] Alternate =
    {
        0, 8, 16, 24, 1, 9, 2, 10, 17, 25, 32, 40, 48, 56, 57, 49,
        41, 33, 26, 18, 3, 11, 4, 12, 19, 27, 34, 42, 50, 58, 35, 43,
        51, 59, 20, 28, 5, 13, 6, 14, 21, 29, 36, 44, 52, 60, 37, 45,
        53, 61, 22, 30, 7, 15, 23, 31, 38, 46, 54, 62, 39, 47, 55, 63,
    };

    /// <summary>quantiser_scale for q_scale_type 1 (table 7-6).</summary>
    static readonly byte[] NonLinear =
    {
        0, 1, 2, 3, 4, 5, 6, 7, 8, 10, 12, 14, 16, 18, 20, 22,
        24, 28, 32, 36, 40, 44, 48, 52, 56, 64, 72, 80, 88, 96, 104, 112,
    };

    // ---- annex B's tables, as "code value" (and "code run level" for the coefficients)

    /// <summary>B-1, macroblock_address_increment; 0 is the escape (+33).</summary>
    static readonly string[] AddressIncrement =
    {
        "1 1", "011 2", "010 3", "0011 4", "0010 5", "00011 6", "00010 7", "0000111 8", "0000110 9",
        "00001011 10", "00001010 11", "00001001 12", "00001000 13", "00000111 14", "00000110 15",
        "0000010111 16", "0000010110 17", "0000010101 18", "0000010100 19", "0000010011 20", "0000010010 21",
        "00000100011 22", "00000100010 23", "00000100001 24", "00000100000 25", "00000011111 26", "00000011110 27",
        "00000011101 28", "00000011100 29", "00000011011 30", "00000011010 31", "00000011001 32", "00000011000 33",
        "00000001000 0",
    };

    /// <summary>B-12, dct_dc_size_luminance.</summary>
    static readonly string[] DcLuma =
    {
        "100 0", "00 1", "01 2", "101 3", "110 4", "1110 5", "11110 6", "111110 7", "1111110 8", "11111110 9",
        "111111110 10", "111111111 11",
    };

    /// <summary>B-13, dct_dc_size_chrominance.</summary>
    static readonly string[] DcChroma =
    {
        "00 0", "01 1", "10 2", "110 3", "1110 4", "11110 5", "111110 6", "1111110 7", "11111110 8", "111111110 9",
        "1111111110 10", "1111111111 11",
    };

    /// <summary>The coefficient codes B-14 and B-15 share, 14 bits and longer.</summary>
    static readonly string[] LongCoefficients =
    {
        "00000000011111 0 16", "00000000011110 0 17", "00000000011101 0 18", "00000000011100 0 19",
        "00000000011011 0 20", "00000000011010 0 21", "00000000011001 0 22", "00000000011000 0 23",
        "00000000010111 0 24", "00000000010110 0 25", "00000000010101 0 26", "00000000010100 0 27",
        "00000000010011 0 28", "00000000010010 0 29", "00000000010001 0 30", "00000000010000 0 31",
        "000000000011000 0 32", "000000000010111 0 33", "000000000010110 0 34", "000000000010101 0 35",
        "000000000010100 0 36", "000000000010011 0 37", "000000000010010 0 38", "000000000010001 0 39",
        "000000000010000 0 40", "000000000011111 1 8", "000000000011110 1 9", "000000000011101 1 10",
        "000000000011100 1 11", "000000000011011 1 12", "000000000011010 1 13", "000000000011001 1 14",
        "0000000000010011 1 15", "0000000000010010 1 16", "0000000000010001 1 17", "0000000000010000 1 18",
        "0000000000010100 6 3", "0000000000011010 11 2", "0000000000011001 12 2", "0000000000011000 13 2",
        "0000000000010111 14 2", "0000000000010110 15 2", "0000000000010101 16 2", "0000000000011111 27 1",
        "0000000000011110 28 1", "0000000000011101 29 1", "0000000000011100 30 1", "0000000000011011 31 1",
    };

    /// <summary>B-14, DCT coefficients table zero (after the first coefficient): "10" ends the block, "000001" escapes.</summary>
    static readonly string[] TableZero =
    {
        "10 eob", "000001 esc", "11 0 1", "011 1 1", "0100 0 2", "0101 2 1", "00101 0 3", "00111 3 1", "00110 4 1",
        "000110 1 2", "000111 5 1", "000101 6 1", "000100 7 1", "0000110 0 4", "0000100 2 2", "0000111 8 1", "0000101 9 1",
        "00100110 0 5", "00100001 0 6", "00100101 1 3", "00100100 3 2", "00100111 10 1", "00100011 11 1", "00100010 12 1", "00100000 13 1",
        "0000001010 0 7", "0000001100 1 4", "0000001011 2 3", "0000001111 4 2", "0000001001 5 2", "0000001110 14 1", "0000001101 15 1", "0000001000 16 1",
        "000000011101 0 8", "000000011000 0 9", "000000010011 0 10", "000000010000 0 11", "000000011011 1 5", "000000010100 2 4",
        "000000011100 3 3", "000000010010 4 3", "000000011110 6 2", "000000010101 7 2", "000000010001 8 2", "000000011111 17 1",
        "000000011010 18 1", "000000011001 19 1", "000000010111 20 1", "000000010110 21 1",
        "0000000011010 0 12", "0000000011001 0 13", "0000000011000 0 14", "0000000010111 0 15", "0000000010110 1 6",
        "0000000010101 1 7", "0000000010100 2 5", "0000000010011 3 4", "0000000010010 5 3", "0000000010001 9 2",
        "0000000010000 10 2", "0000000011111 22 1", "0000000011110 23 1", "0000000011101 24 1", "0000000011100 25 1", "0000000011011 26 1",
    };

    /// <summary>B-15, DCT coefficients table one (intra_vlc_format 1): "0110" ends the block, "000001" escapes.</summary>
    static readonly string[] TableOne =
    {
        "0110 eob", "000001 esc", "10 0 1", "010 1 1", "110 0 2", "00101 2 1", "0111 0 3", "00111 3 1", "000110 4 1",
        "00110 1 2", "000111 5 1", "0000110 6 1", "0000100 7 1", "11100 0 4", "0000111 2 2", "0000101 8 1", "1111000 9 1",
        "11101 0 5", "000101 0 6", "1111001 1 3", "00100110 3 2", "1111010 10 1", "00100001 11 1", "00100101 12 1", "00100100 13 1",
        "000100 0 7", "00100111 1 4", "11111100 2 3", "11111101 4 2", "000000100 5 2", "000000101 14 1", "000000111 15 1", "0000001101 16 1",
        "1111011 0 8", "1111100 0 9", "00100011 0 10", "00100010 0 11", "00100000 1 5", "0000001100 2 4",
        "000000011100 3 3", "000000010010 4 3", "000000011110 6 2", "000000010101 7 2", "000000010001 8 2", "000000011111 17 1",
        "000000011010 18 1", "000000011001 19 1", "000000010111 20 1", "000000010110 21 1",
        "11111010 0 12", "11111011 0 13", "11111110 0 14", "11111111 0 15", "0000000010110 1 6",
        "0000000010101 1 7", "0000000010100 2 5", "0000000010011 3 4", "0000000010010 5 3", "0000000010001 9 2",
        "0000000010000 10 2", "0000000011111 22 1", "0000000011110 23 1", "0000000011101 24 1", "0000000011100 25 1", "0000000011011 26 1",
    };

    /// <summary>A table looked up by the next 16 bits: each entry its value (or run and level) and its code's length; length 0 for no code.</summary>
    sealed class Vlc
    {
        public readonly short[] Value = new short[1 << 16];
        public readonly byte[] Level = new byte[1 << 16];
        public readonly byte[] Length = new byte[1 << 16];

        public Vlc(IEnumerable<string> codes, IEnumerable<string>? more = null)
        {
            foreach (var line in more == null ? codes : Concat(codes, more))
            {
                var parts = line.Split(' ');
                var code = parts[0];
                short value;
                byte level = 0;
                if (parts[1] == "eob") value = -1;
                else if (parts[1] == "esc") value = -2;
                else
                {
                    value = short.Parse(parts[1]);
                    if (parts.Length > 2) level = byte.Parse(parts[2]);
                }
                var bits = Convert.ToInt32(code, 2);
                var free = 16 - code.Length;
                for (var i = 0; i < 1 << free; i++)
                {
                    var at = (bits << free) | i;
                    Value[at] = value;
                    Level[at] = level;
                    Length[at] = (byte)code.Length;
                }
            }
        }

        static IEnumerable<string> Concat(IEnumerable<string> a, IEnumerable<string> b)
        {
            foreach (var x in a) yield return x;
            foreach (var x in b) yield return x;
        }
    }

    static Vlc? increments, dcLuma, dcChroma, zero, one;
    static float[]? cosines;

    static Vlc Increments => increments ??= new Vlc(AddressIncrement);
    static Vlc LumaSizes => dcLuma ??= new Vlc(DcLuma);
    static Vlc ChromaSizes => dcChroma ??= new Vlc(DcChroma);
    static Vlc Zero => zero ??= new Vlc(TableZero, LongCoefficients);
    static Vlc One => one ??= new Vlc(TableOne, LongCoefficients);

    /// <summary>cos((2x + 1)uπ/16) × C(u) / 2, by [x * 8 + u].</summary>
    static float[] Cosines
    {
        get
        {
            if (cosines != null) return cosines;
            var c = new float[64];
            for (var x = 0; x < 8; x++)
                for (var u = 0; u < 8; u++)
                    c[x * 8 + u] = (float)((u == 0 ? Math.Sqrt(0.5) : 1) * Math.Cos((2 * x + 1) * u * Math.PI / 16) / 2);
            return cosines = c;
        }
    }

    sealed class Decoder
    {
        readonly byte[] b;
        long bit;
        int width, height, mbWidth, mbHeight;
        readonly byte[] intra = (byte[])DefaultIntra.Clone();
        int dcPrecision, structure = 3, qScaleType, intraVlc, alternateScan;
        bool framePredFrameDct = true, concealment, mpeg2;
        byte[] y = Array.Empty<byte>(), cb = Array.Empty<byte>(), cr = Array.Empty<byte>();
        readonly int[] dcPred = new int[3];
        readonly int[] block = new int[64];
        readonly float[] work = new float[64];

        public Decoder(byte[] es) => b = es;

        public StillPicture? Run()
        {
            var picture = false;
            var slices = 0;
            for (var at = 0; at + 4 <= b.Length;)
            {
                if (b[at] != 0 || b[at + 1] != 0 || b[at + 2] != 1)
                {
                    at++;
                    continue;
                }
                var code = b[at + 3];
                bit = (at + 4) * 8L;
                if (code == 0xb3) Sequence();
                else if (code == 0xb5) Extension(picture);
                else if (code == 0x00)
                {
                    // A second picture: the first is done.
                    if (picture) break;
                    if (width == 0 || !mpeg2) return null;
                    Bits(10);
                    if (Bits(3) != 1) return null;
                    picture = true;
                    y = new byte[mbWidth * 16 * mbHeight * 16];
                    cb = new byte[mbWidth * 8 * mbHeight * 8];
                    cr = new byte[cb.Length];
                }
                else if (code >= 0x01 && code <= 0xaf && picture)
                {
                    if (structure != 3 || concealment) return null;
                    Slice(code);
                    slices++;
                }
                else if (code == 0xb7 && picture) break;
                at = (int)((bit + 7) / 8);
                if (at < 0 || at > b.Length) break;
            }
            return picture && slices > 0 ? Rgba() : null;
        }

        void Sequence()
        {
            width = Bits(12);
            height = Bits(12);
            Bits(4 + 4 + 18 + 1 + 10 + 1);
            if (Bits(1) == 1)
                for (var i = 0; i < 64; i++) intra[i] = (byte)Bits(8);
            mbWidth = (width + 15) / 16;
            mbHeight = (height + 15) / 16;
        }

        void Extension(bool picture)
        {
            var id = Bits(4);
            if (id == 1)
            {
                // Sequence extension: an MPEG-2 stream; 4:2:0 only.
                mpeg2 = true;
                Bits(8 + 1);
                if (Bits(2) != 1) throw new InvalidOperationException("not 4:2:0");
            }
            else if (id == 3)
            {
                if (Bits(1) == 1)
                    for (var i = 0; i < 64; i++) intra[i] = (byte)Bits(8);
            }
            else if (id == 8 && picture)
            {
                Bits(16);
                dcPrecision = Bits(2);
                structure = Bits(2);
                Bits(1);
                framePredFrameDct = Bits(1) == 1;
                concealment = Bits(1) == 1;
                qScaleType = Bits(1);
                intraVlc = Bits(1);
                alternateScan = Bits(1);
            }
        }

        void Slice(int row)
        {
            var mbRow = row - 1;
            if (mbRow >= mbHeight) return;
            var qCode = Bits(5);
            // Extra slice information.
            while (Bits(1) == 1) Bits(8);
            for (var c = 0; c < 3; c++) dcPred[c] = 1 << (dcPrecision + 7);
            var column = -1;
            while (Peek(23) != 0)
            {
                var increment = 0;
                while (true)
                {
                    var v = Code(Increments);
                    if (v == 0) increment += 33;
                    else
                    {
                        increment += v;
                        break;
                    }
                }
                column += increment;
                if (column >= mbWidth) throw new InvalidOperationException("past the row");
                // I-picture macroblock types: "1" intra, "01" intra with a quantiser.
                var quant = Bits(1) == 0;
                if (quant && Bits(1) != 1) throw new InvalidOperationException("not an intra macroblock");
                var fieldDct = false;
                if (!framePredFrameDct) fieldDct = Bits(1) == 1;
                if (quant) qCode = Bits(5);
                var scale = qScaleType == 1 ? NonLinear[qCode] : qCode * 2;
                for (var k = 0; k < 6; k++)
                {
                    Block(k < 4 ? 0 : k - 3, scale);
                    Put(k, mbRow, column, fieldDct);
                }
            }
        }

        void Block(int component, int scale)
        {
            Array.Clear(block, 0, 64);
            var size = Code(component == 0 ? LumaSizes : ChromaSizes);
            var diff = 0;
            if (size > 0)
            {
                var v = Bits(size);
                diff = v < 1 << (size - 1) ? v - (1 << size) + 1 : v;
            }
            dcPred[component] += diff;
            block[0] = dcPred[component] * (8 >> dcPrecision);
            var sum = block[0];
            var scan = alternateScan == 1 ? Alternate : Zigzag;
            var table = intraVlc == 1 ? One : Zero;
            var n = 0;
            while (true)
            {
                var peek = Peek(16);
                var length = table.Length[peek];
                if (length == 0) throw new InvalidOperationException("no such coefficient");
                var run = table.Value[peek];
                int level;
                bit += length;
                if (run == -1) break;
                if (run == -2)
                {
                    run = (short)Bits(6);
                    level = Bits(12);
                    if (level >= 2048) level -= 4096;
                    if (level == 0 || level == -2048) throw new InvalidOperationException("bad escape");
                }
                else
                {
                    level = table.Level[peek];
                    if (Bits(1) == 1) level = -level;
                }
                n += run + 1;
                if (n > 63) throw new InvalidOperationException("past the block");
                var at = scan[n];
                var f = level * intra[ZigzagIndex(at)] * scale * 2 / 32;
                f = Math.Max(-2048, Math.Min(2047, f));
                block[at] = f;
                sum += f;
            }
            // Mismatch control (7.4.4).
            if ((sum & 1) == 0) block[63] ^= 1;
            Idct();
        }

        /// <summary>Where a coefficient's position (row × 8 + column) is in zigzag order: the matrices are sent that way.</summary>
        static int ZigzagIndex(int position) => InverseZigzag[position];

        static readonly byte[] InverseZigzag = Inverse(Zigzag);

        static byte[] Inverse(byte[] scan)
        {
            var inverse = new byte[64];
            for (var i = 0; i < 64; i++) inverse[scan[i]] = (byte)i;
            return inverse;
        }

        void Idct()
        {
            var c = Cosines;
            // Rows, then columns.
            for (var v = 0; v < 8; v++)
                for (var x = 0; x < 8; x++)
                {
                    float s = 0;
                    for (var u = 0; u < 8; u++) s += c[x * 8 + u] * block[v * 8 + u];
                    work[v * 8 + x] = s;
                }
            for (var x = 0; x < 8; x++)
                for (var yy = 0; yy < 8; yy++)
                {
                    float s = 0;
                    for (var v = 0; v < 8; v++) s += c[yy * 8 + v] * work[v * 8 + x];
                    block[yy * 8 + x] = (int)Math.Round(s);
                }
        }

        /// <summary>Block <paramref name="k"/> of the macroblock into its plane.</summary>
        void Put(int k, int mbRow, int mbColumn, bool fieldDct)
        {
            byte[] plane;
            int stride, x0, y0, step = 1;
            if (k < 4)
            {
                plane = y;
                stride = mbWidth * 16;
                x0 = mbColumn * 16 + (k & 1) * 8;
                if (fieldDct)
                {
                    // Field DCT: blocks 0 and 1 the top field's lines, 2 and 3 the bottom's.
                    y0 = mbRow * 16 + (k >> 1);
                    step = 2;
                }
                else y0 = mbRow * 16 + (k >> 1) * 8;
            }
            else
            {
                plane = k == 4 ? cb : cr;
                stride = mbWidth * 8;
                x0 = mbColumn * 8;
                y0 = mbRow * 8;
            }
            for (var r = 0; r < 8; r++)
            {
                var row = (y0 + r * step) * stride + x0;
                for (var col = 0; col < 8; col++) plane[row + col] = (byte)Math.Max(0, Math.Min(255, block[r * 8 + col]));
            }
        }

        StillPicture Rgba()
        {
            var rgba = new byte[width * height * 4];
            var ys = mbWidth * 16;
            var cs = mbWidth * 8;
            for (var row = 0; row < height; row++)
                for (var col = 0; col < width; col++)
                {
                    // BT.601, studio range.
                    var l = 1.164f * (y[row * ys + col] - 16);
                    var u = cb[(row >> 1) * cs + (col >> 1)] - 128;
                    var v = cr[(row >> 1) * cs + (col >> 1)] - 128;
                    var o = (row * width + col) * 4;
                    rgba[o] = Clamp(l + 1.596f * v);
                    rgba[o + 1] = Clamp(l - 0.813f * v - 0.391f * u);
                    rgba[o + 2] = Clamp(l + 2.018f * u);
                    rgba[o + 3] = 255;
                }
            return new StillPicture(width, height, rgba);
        }

        static byte Clamp(float x) => (byte)Math.Max(0, Math.Min(255, (int)(x + 0.5f)));

        int Code(Vlc table)
        {
            var peek = Peek(16);
            var length = table.Length[peek];
            if (length == 0) throw new InvalidOperationException("no such code");
            bit += length;
            return table.Value[peek];
        }

        /// <summary>The next <paramref name="n"/> bits (at most 25), zeros past the end.</summary>
        int Peek(int n)
        {
            var at = (int)(bit >> 3);
            uint w = 0;
            for (var i = 0; i < 4; i++) w = (w << 8) | (at + i < b.Length ? b[at + i] : 0u);
            return (int)((w << (int)(bit & 7)) >> (32 - n));
        }

        int Bits(int n)
        {
            var v = Peek(n);
            bit += n;
            return v;
        }
    }
}
