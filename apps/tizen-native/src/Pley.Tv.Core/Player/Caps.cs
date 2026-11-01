using System;
using System.Collections.Generic;
using System.Globalization;
using System.Linq;
using System.Text.Json;
using System.Text.RegularExpressions;
using Pley.Tv.Core.Api;

namespace Pley.Tv.Core.Player;

/// <summary>A video decoder in a capabilities report (version 2).</summary>
public sealed record VideoDecoderCaps
{
    public string Codec { get; init; } = "";
    public bool Hardware { get; init; }
    public IReadOnlyList<string> Profiles { get; init; } = new string[0];
    public int? MaxWidth { get; init; }
    public int? MaxHeight { get; init; }
    public double? MaxFps { get; init; }
    public int? MaxBitDepth { get; init; }
    /// <summary>Numbered as ffprobe numbers it (HEVC 5.1 = 153, H.264 5.1 = 51).</summary>
    public int? MaxLevel { get; init; }
}

/// <summary>A Dolby Vision profile the device decodes, up to a level.</summary>
public sealed record DolbyVisionCaps
{
    public int Profile { get; init; }
    public int? MaxLevel { get; init; }
}

/// <summary>The screen's size, for choosing among a title's versions.</summary>
public sealed record DisplayCaps
{
    public int Width { get; init; }
    public int Height { get; init; }
}

/// <summary>
/// The parts of the capabilities report (docs/API.md, <c>ClientCaps</c>) that the player reads to
/// choose a file and to tell what the server will do: the Android app's ClientCaps. The report
/// itself goes to the server as <see cref="TizenCaps"/> made it; this is that report, read back.
/// </summary>
public sealed record ClientCaps
{
    public IReadOnlyList<string> Containers { get; init; } = new string[0];
    public IReadOnlyList<string> VideoCodecs { get; init; } = new string[0];
    public IReadOnlyList<string> AudioCodecs { get; init; } = new string[0];
    public bool Hdr { get; init; }
    public bool DolbyVision { get; init; }
    public int CapsVersion { get; init; }
    public IReadOnlyList<VideoDecoderCaps> Video { get; init; } = new VideoDecoderCaps[0];
    /// <summary>"hdr10", "hdr10plus", "hlg", "dolby_vision".</summary>
    public IReadOnlyList<string> HdrTypes { get; init; } = new string[0];
    public IReadOnlyList<DolbyVisionCaps> DolbyVisionProfiles { get; init; } = new DolbyVisionCaps[0];
    public IReadOnlyList<string> AudioDecode { get; init; } = new string[0];
    public IReadOnlyList<string> AudioPassthrough { get; init; } = new string[0];
    public IReadOnlyList<string> SubtitleCodecs { get; init; } = new string[0];
    /// <summary>The player draws picture subtitles from the server's .mks of them.</summary>
    public bool PictureSubtitlesSideload { get; init; }
    public DisplayCaps? Display { get; init; }

    /// <summary>The report as it goes to the server (<see cref="TizenCaps.Of"/>), read as the API's JSON.</summary>
    public static ClientCaps Of(IReadOnlyDictionary<string, object?> report) =>
        JsonSerializer.Deserialize<ClientCaps>(JsonSerializer.Serialize(report), ApiJson.Options) ?? new ClientCaps();
}

/// <summary>Whether a video plays as it is here, and if not, what the server will do about it.</summary>
public enum CapsDecisionKind
{
    Direct,
    /// <summary>Dolby Vision this device can't decode, on an HDR10 screen: the server sends the HDR10 base layer.</summary>
    RemuxBaseLayer,
    /// <summary>The screen can't show its HDR: the server converts it.</summary>
    ServerToneMaps,
    Unsupported,
}

public sealed record CapsDecision(CapsDecisionKind Kind, string? Reason = null)
{
    public static readonly CapsDecision Direct = new CapsDecision(CapsDecisionKind.Direct);
    public static readonly CapsDecision RemuxBaseLayer = new CapsDecision(CapsDecisionKind.RemuxBaseLayer);
    public static readonly CapsDecision ServerToneMaps = new CapsDecision(CapsDecisionKind.ServerToneMaps);

    public static CapsDecision Unsupported(string reason) => new CapsDecision(CapsDecisionKind.Unsupported, reason);
}

/// <summary>
/// The client's side of the server's version 2 rules (the Android app's DefaultPlaybackCapsPolicy):
/// what the server will do with a file for a report, and which of a title's versions plays best.
/// Pure.
/// </summary>
public static class CapsPolicy
{
    const double RateSlack = 0.01;

    // Dolby Vision base-layer compatibility ids (ffprobe's dv_bl_signal_compatibility_id).
    const int BlHdr10 = 1;
    const int BlSdr = 2;
    const int BlHlg = 4;
    const int BlBluRay = 6;

    /// <summary>Profiles ffprobe and the decoders name alike: missing from a decoder's list, they refuse a stream.</summary>
    static readonly HashSet<string> KnownProfiles = new HashSet<string>
    {
        "main", "main 10", "main still picture", "rext", "baseline", "constrained baseline", "high", "high 10",
        "high 4:2:2", "high 4:4:4 predictive", "profile 0", "profile 1", "profile 2", "profile 3", "professional",
    };

    static readonly Regex PixFmtDepth = new Regex("(1[02])(?:le|be)$", RegexOptions.CultureInvariant);

    /// <summary>The main video stream of a file: its default, else its first.</summary>
    public static MediaStream? VideoOf(MediaFile? file)
    {
        var videos = (file?.Streams ?? new MediaStream[0]).Where(s => s.Kind == "video").ToList();
        return videos.FirstOrDefault(s => s.Default) ?? videos.FirstOrDefault();
    }

    /// <summary>The video's dynamic range as the server reads it: "sdr", "hdr10", "hdr10plus", "hlg" or "dolby_vision".</summary>
    public static string RangeOf(MediaStream? video)
    {
        if (video == null) return "sdr";
        if (!string.IsNullOrEmpty(video.DynamicRange) && video.DynamicRange != "unknown") return video.DynamicRange!;
        if (video.DvProfile != null) return "dolby_vision";
        if (video.ColorTransfer == "smpte2084") return "hdr10";
        if (video.ColorTransfer == "arib-std-b67") return "hlg";
        return "sdr";
    }

    /// <summary>What happens to <paramref name="file"/> here.</summary>
    public static CapsDecision Decide(MediaFile file, ClientCaps caps)
    {
        var video = VideoOf(file);
        if (video == null) return CapsDecision.Direct;
        var types = caps.HdrTypes;
        var range = RangeOf(video);
        switch (range)
        {
            case "dolby_vision":
                return DolbyVision(video, caps);
            case "hdr10":
            case "hdr10plus":
            case "hlg":
                var shown = range == "hlg" ? types.Contains("hlg")
                    : range == "hdr10plus" ? types.Contains("hdr10plus") || types.Contains("hdr10")
                    : types.Contains("hdr10");
                return shown ? Fits(video, video.Codec, caps, hdr: true) : CapsDecision.ServerToneMaps;
            default:
                return Fits(video, video.Codec, caps, hdr: false);
        }
    }

    /// <summary>
    /// The version of <paramref name="item"/> that plays best here: one that plays as it is, then
    /// one no taller than the screen, then the tallest, then (playing as they are) the richest
    /// dynamic range, then the highest bitrate. Null without files.
    /// </summary>
    public static MediaFile? BestFile(ItemDetail item, ClientCaps caps)
    {
        int? screen = caps.Display is { } d ? Math.Min(d.Width, d.Height) : null;
        return item.Files
            .Select(f => (File: f, Decision: Decide(f, caps)))
            .OrderBy(p => Rank(p.Decision))
            .ThenBy(p => screen == null || Height(p.File) <= screen ? 0 : 1)
            .ThenByDescending(p => Height(p.File))
            .ThenByDescending(p => p.Decision.Kind == CapsDecisionKind.Direct ? RangeRank(RangeOf(VideoOf(p.File))) : 0)
            .ThenByDescending(p => p.File.Bitrate ?? 0)
            .Select(p => p.File)
            .FirstOrDefault();
    }

    static CapsDecision DolbyVision(MediaStream video, ClientCaps caps)
    {
        var profile = video.DvProfile;
        var types = caps.HdrTypes;
        var decoded = caps.DolbyVisionProfiles.FirstOrDefault(p => p.Profile == profile);
        if (types.Contains("dolby_vision") && decoded != null)
        {
            var level = video.DvLevel;
            var max = decoded.MaxLevel;
            return level != null && max != null && level > max
                ? CapsDecision.Unsupported($"Dolby Vision level {level} is above this decoder's {max}")
                : CapsDecision.Direct;
        }
        var baseCodec = BaseCodec(profile, video.Codec);
        switch (profile)
        {
            // No base layer to fall back to.
            case 5:
                return CapsDecision.ServerToneMaps;
            case 7:
                return types.Contains("hdr10") ? CapsDecision.RemuxBaseLayer : CapsDecision.ServerToneMaps;
        }
        // The Blu-ray's HDR10 id in profile 8 (a profile 7 converted elsewhere): HDR10.
        var said = profile == 8 && video.DvBlCompat == BlBluRay ? null : video.DvBlCompat;
        var compat = said ?? (profile == 8 ? BlHdr10 : (int?)null);
        return compat switch
        {
            BlHdr10 => types.Contains("hdr10") ? Fits(video, baseCodec, caps, hdr: true) : CapsDecision.ServerToneMaps,
            BlHlg => types.Contains("hlg") ? Fits(video, baseCodec, caps, hdr: true) : CapsDecision.ServerToneMaps,
            BlSdr => Fits(video, baseCodec, caps, hdr: false),
            _ => CapsDecision.ServerToneMaps,
        };
    }

    /// <summary>Whether a decoder of <paramref name="codec"/> takes the stream's size, rate, depth, profile and level.</summary>
    static CapsDecision Fits(MediaStream video, string codec, ClientCaps caps, bool hdr)
    {
        var entries = caps.Video.Where(e => string.Equals(e.Codec, codec, StringComparison.OrdinalIgnoreCase)).ToList();
        if (entries.Count == 0) return CapsDecision.Unsupported($"No decoder for {codec} here");
        var depth = BitDepth(video);
        if (hdr && !entries.Any(e => e.Hardware && (e.MaxBitDepth ?? 8) >= 10)) return CapsDecision.Unsupported($"No 10-bit hardware decoder for {codec} here");
        if (entries.Any(e => Takes(e, video, depth))) return CapsDecision.Direct;
        var size = string.Join("×", new[] { video.Width, video.Height }.Where(v => v != null));
        var fps = video.FrameRate != null ? " at " + video.FrameRate.Value.ToString("F3", CultureInfo.InvariantCulture) + " fps" : "";
        var profile = video.Profile != null ? ", " + video.Profile : "";
        return CapsDecision.Unsupported($"{codec} {size}{fps}, {depth}-bit{profile}, is beyond this device's decoders");
    }

    static bool Takes(VideoDecoderCaps entry, MediaStream video, int depth)
    {
        if (video.Width is int w && video.Height is int h && entry.MaxWidth is int maxW && entry.MaxHeight is int maxH)
        {
            var asIs = w <= maxW && h <= maxH;
            var turned = h <= maxW && w <= maxH;
            if (!asIs && !turned) return false;
        }
        if (video.FrameRate is double fps && entry.MaxFps is double maxFps && fps > maxFps + RateSlack) return false;
        if (depth > (entry.MaxBitDepth ?? 8)) return false;
        if (video.Level is int level && level > 0 && entry.MaxLevel is int maxLevel && level > maxLevel) return false;
        var profile = video.Profile;
        if (profile != null && entry.Profiles.Count > 0 && !entry.Profiles.Any(p => string.Equals(p, profile, StringComparison.OrdinalIgnoreCase)))
        {
            // ffprobe and the decoders name some profiles differently; only refuse those known to be missing.
            if (KnownProfiles.Contains(profile.ToLowerInvariant())) return false;
        }
        return true;
    }

    /// <summary>The codec under a Dolby Vision profile: AVC for 9, AV1 for 10, HEVC for the rest.</summary>
    static string BaseCodec(int? profile, string codec) => profile switch
    {
        9 => "h264",
        10 => "av1",
        _ => string.Equals(codec, "dvhe", StringComparison.OrdinalIgnoreCase) || string.Equals(codec, "dvh1", StringComparison.OrdinalIgnoreCase) ? "hevc" : codec,
    };

    /// <summary>The stream's bit depth, else from its pixel format ("yuv420p10le") or profile ("Main 10").</summary>
    public static int BitDepth(MediaStream video)
    {
        if (video.BitDepth is int depth && depth > 0) return depth;
        var m = PixFmtDepth.Match(video.PixFmt ?? "");
        if (m.Success) return int.Parse(m.Groups[1].Value, CultureInfo.InvariantCulture);
        return (video.Profile ?? "").Contains("10") ? 10 : 8;
    }

    static int Height(MediaFile file) => VideoOf(file)?.Height ?? file.Height ?? 0;

    static int Rank(CapsDecision d) => d.Kind switch
    {
        CapsDecisionKind.Direct => 0,
        CapsDecisionKind.RemuxBaseLayer => 1,
        CapsDecisionKind.ServerToneMaps => 2,
        _ => 3,
    };

    static int RangeRank(string range) => range switch
    {
        "dolby_vision" => 4,
        "hdr10plus" => 3,
        "hdr10" => 2,
        "hlg" => 1,
        _ => 0,
    };
}
