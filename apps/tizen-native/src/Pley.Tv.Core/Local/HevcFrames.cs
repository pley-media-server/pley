using System;
using Pley.Tv.Core.Api;

namespace Pley.Tv.Core.Local;

/// <summary>What a film's first HEVC frames say of its picture, whatever its container left out.</summary>
/// <param name="Mastering">HDR10's static metadata (mastering display, light levels): a PQ picture.</param>
/// <param name="Hlg">The alternative transfer message names HLG.</param>
/// <param name="Hdr10Plus">HDR10+'s dynamic metadata.</param>
/// <param name="DolbyVision">Dolby Vision's RPU, in the frames.</param>
/// <param name="EnhancementLayer">Dolby Vision's enhancement layer in the same track: profile 7.</param>
public sealed record HevcMarks(bool Mastering, bool Hlg, bool Hdr10Plus, bool DolbyVision, bool EnhancementLayer)
{
    public bool Any => Mastering || Hlg || Hdr10Plus || DolbyVision;
}

/// <summary>
/// Reads the NAL units of a film's first HEVC frames (length-prefixed, as Matroska and MP4 hold
/// them) for what its container may not say: a muxer that writes no color, an older one that
/// writes no Dolby Vision configuration. SEI messages are walked as the standard lays them out
/// (types and sizes in 255 steps, emulation bytes taken out), not matched as bytes.
/// </summary>
public static class HevcFrames
{
    const int PrefixSei = 39, SuffixSei = 40, DolbyRpu = 62, DolbyEl = 63;
    const int MasteringDisplay = 137, LightLevel = 144, AlternativeTransfer = 147, RegisteredT35 = 4;

    /// <summary>The marks in <paramref name="bytes"/>, a stretch of the file from its first frames.</summary>
    public static HevcMarks Scan(byte[] bytes)
    {
        bool mastering = false, hlg = false, hdr10plus = false, rpu = false, el = false;
        // A unit is its 4-byte length, then its 2-byte header: type in bits 1-6, layer and temporal id after.
        for (var i = 0; i + 6 <= bytes.Length; i++)
        {
            var header = bytes[i + 4];
            if ((header & 0x80) != 0 || (bytes[i + 5] & 7) == 0) continue;
            var type = (header >> 1) & 0x3f;
            if (type != PrefixSei && type != SuffixSei && type != DolbyRpu && type != DolbyEl) continue;
            var length = ((long)bytes[i] << 24) | ((long)bytes[i + 1] << 16) | ((long)bytes[i + 2] << 8) | bytes[i + 3];
            if (length < 3 || length > 1 << 20) continue;
            var end = (int)Math.Min(bytes.Length, i + 4 + length);
            switch (type)
            {
                case DolbyRpu:
                    // Its payload opens with the RPU's prefix, 25.
                    if (i + 6 < bytes.Length && bytes[i + 6] == 0x19) rpu = true;
                    break;
                case DolbyEl:
                    el = true;
                    break;
                default:
                    Sei(Unescaped(bytes, i + 6, end), ref mastering, ref hlg, ref hdr10plus);
                    break;
            }
        }
        return new HevcMarks(mastering, hlg, hdr10plus, rpu, el && rpu);
    }

    static void Sei(byte[] p, ref bool mastering, ref bool hlg, ref bool hdr10plus)
    {
        var at = 0;
        // Messages until the trailing bits (0x80) or the end.
        while (at < p.Length && p[at] != 0x80)
        {
            var type = 0;
            while (at < p.Length && p[at] == 0xff)
            {
                type += 255;
                at++;
            }
            if (at >= p.Length) return;
            type += p[at++];
            var size = 0;
            while (at < p.Length && p[at] == 0xff)
            {
                size += 255;
                at++;
            }
            if (at >= p.Length) return;
            size += p[at++];
            if (at + size > p.Length) return;
            switch (type)
            {
                case MasteringDisplay when size == 24:
                case LightLevel when size == 4:
                    mastering = true;
                    break;
                case AlternativeTransfer when size >= 1:
                    if (p[at] == 18) hlg = true;
                    break;
                case RegisteredT35 when size >= 6:
                    // From the USA (0xB5), Samsung's (0x003C), HDR10+'s own (0x0001, application 4).
                    if (p[at] == 0xb5 && p[at + 1] == 0x00 && p[at + 2] == 0x3c && p[at + 3] == 0x00 && p[at + 4] == 0x01 && p[at + 5] == 0x04) hdr10plus = true;
                    break;
            }
            at += size;
        }
    }

    /// <summary>A unit's payload without the emulation bytes (00 00 03 → 00 00).</summary>
    static byte[] Unescaped(byte[] b, int from, int to)
    {
        var o = new byte[Math.Max(0, to - from)];
        var n = 0;
        var zeros = 0;
        for (var i = from; i < to; i++)
        {
            if (zeros >= 2 && b[i] == 3)
            {
                zeros = 0;
                continue;
            }
            zeros = b[i] == 0 ? zeros + 1 : 0;
            o[n++] = b[i];
        }
        Array.Resize(ref o, n);
        return o;
    }

    /// <summary>
    /// <paramref name="video"/> with what its frames say added to what its container said: a
    /// container's word stands where it gave one; the frames fill in the rest.
    /// </summary>
    public static MediaStream Apply(MediaStream video, HevcMarks m)
    {
        var transfer = video.ColorTransfer ?? (m.Mastering || m.Hdr10Plus || m.DolbyVision && !m.Hlg ? "smpte2084" : m.Hlg ? "arib-std-b67" : null);
        var dv = video.DvProfile != null || m.DolbyVision;
        // Without a configuration record, the profile follows from the layers: an enhancement layer is 7,
        // a PQ base 8.1, an HLG base 8.4, no base metadata 5.
        var profile = video.DvProfile ?? (m.DolbyVision ? m.EnhancementLayer ? 7 : 8 : (int?)null);
        var compat = video.DvBlCompat ?? (m.DolbyVision ? m.EnhancementLayer ? 6 : m.Hlg ? 4 : m.Mastering || m.Hdr10Plus ? 1 : 0 : (int?)null);
        if (profile == 8 && compat == 0) profile = 5;
        var plus = video.Hdr10plus == true || m.Hdr10Plus;
        var range = dv ? "dolby_vision"
            : transfer == "smpte2084" ? (plus ? "hdr10plus" : "hdr10")
            : transfer == "arib-std-b67" ? "hlg"
            : video.DynamicRange ?? "sdr";
        return video with
        {
            ColorTransfer = transfer,
            ColorPrimaries = video.ColorPrimaries ?? (transfer is "smpte2084" or "arib-std-b67" ? "bt2020" : null),
            DynamicRange = range,
            Hdr = range != "sdr",
            Hdr10plus = plus ? true : video.Hdr10plus,
            DvProfile = dv ? profile : null,
            DvBlCompat = dv ? compat : null,
        };
    }
}
