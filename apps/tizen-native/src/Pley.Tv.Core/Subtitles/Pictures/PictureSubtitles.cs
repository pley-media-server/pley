using System.Collections.Generic;
using System.Linq;

namespace Pley.Tv.Core.Subtitles.Pictures;

/// <summary>
/// A picture track ready to show: a sample shows from its time until the next sample, or for its
/// own duration when it carries one (VobSub's stop command, DVB's page time-out, a Matroska block
/// duration); an empty sample clears. Decoded only when shown.
/// </summary>
public sealed class PictureSubtitles
{
    /// <summary>Codec ids of the picture subtitles this app draws, and the server's (ffprobe's) names for them.</summary>
    public static readonly IReadOnlyDictionary<string, string> Codecs = new Dictionary<string, string>
    {
        ["S_HDMV/PGS"] = "hdmv_pgs_subtitle",
        ["S_VOBSUB"] = "dvd_subtitle",
        ["S_DVBSUB"] = "dvb_subtitle",
    };

    readonly PictureTrack track;
    readonly IPictureDecoder decoder;
    readonly long[] times;

    public PictureSubtitles(PictureTrack track, IPictureDecoder decoder)
    {
        this.track = track;
        this.decoder = decoder;
        times = track.Samples.Select(s => s.TimeMs).ToArray();
    }

    public static IPictureDecoder? DecoderFor(PictureTrack track)
    {
        var data = track.Samples.Select(s => s.Data).ToList();
        return track.CodecId switch
        {
            "S_HDMV/PGS" => new PgsDecoder(data),
            "S_VOBSUB" => new VobSubDecoder(data, track.CodecPrivate),
            "S_DVBSUB" => new DvbDecoder(data),
            _ => null,
        };
    }

    /// <summary>The track's subtitles; null for a codec this app can't decode.</summary>
    public static PictureSubtitles? Of(PictureTrack track) => DecoderFor(track) is IPictureDecoder decoder ? new PictureSubtitles(track, decoder) : null;

    /// <summary>The sample showing at <paramref name="ms"/> (track time), or -1.</summary>
    public int SampleAt(long ms)
    {
        var i = StartingAfter(ms) - 1;
        if (i < 0) return -1;
        return ms < EndOf(i) ? i : -1;
    }

    /// <summary>When sample <paramref name="i"/> stops showing; <see cref="long.MaxValue"/> when nothing stops it.</summary>
    public long EndOf(int i)
    {
        var start = times[i];
        var next = i + 1 < times.Length ? times[i + 1] : long.MaxValue;
        var own = track.Samples[i].DurationMs ?? decoder.OwnDurationMs(i);
        return own is long d && d > 0 ? System.Math.Min(next, start + d) : next;
    }

    /// <summary>What shows at <paramref name="ms"/>: a frame (never empty) and its sample, or null when nothing does.</summary>
    public (int Index, PictureFrame Frame)? FrameAt(long ms)
    {
        var i = SampleAt(ms);
        if (i < 0) return null;
        var frame = decoder.Decode(i);
        return frame != null && frame.Bitmaps.Count > 0 ? (i, frame) : null;
    }

    /// <summary>When what shows changes after <paramref name="ms"/>; null when nothing does any more.</summary>
    public long? NextChange(long ms)
    {
        var i = SampleAt(ms);
        if (i >= 0)
        {
            var end = EndOf(i);
            return end == long.MaxValue ? null : end;
        }
        var lo = StartingAfter(ms);
        return lo < times.Length ? times[lo] : null;
    }

    /// <summary>The first sample starting after <paramref name="ms"/>: the one before it is the last starting at or before.</summary>
    int StartingAfter(long ms)
    {
        int lo = 0, hi = times.Length;
        while (lo < hi)
        {
            var mid = (lo + hi) >> 1;
            if (times[mid] <= ms) lo = mid + 1;
            else hi = mid;
        }
        return lo;
    }
}
