using System.Collections.Generic;
using System.Linq;

namespace Pley.Tv.Core.Player;

/// <summary>
/// What a Samsung TV says of itself that decides what it plays (apps/tizen/src/facts.ts): Samsung's
/// own calls answer only per codec, so the report comes from the model year's specification (the
/// Tizen version tells the year), refined by these.
/// </summary>
public sealed record TizenFacts
{
    public double Tizen { get; init; } = 9;
    public int Year { get; init; } = 2025;
    public string Model { get; init; } = "Samsung TV";
    public bool Uhd { get; init; } = true;
    public bool Uhd8k { get; init; }
    public bool Hdr { get; init; } = true;
    /// <summary>Whether it decodes AV1, when the TV says; null: by the year.</summary>
    public bool? Av1 { get; init; }
    public bool? Ac4 { get; init; }
    /// <summary>The sound output: "PCM", "DOLBY", "DOLBY_DIGITAL_PLUS", "DTS"; null when unknown (nothing passed through).</summary>
    public string? OutputMode { get; init; }
    /// <summary>
    /// The app draws picture subtitles (PGS, VobSub, DVB) from the server's .mks, as the web app
    /// does on every Samsung TV. Off, the server burns them into a stream.
    /// </summary>
    public bool PictureSubtitles { get; init; } = true;

    static readonly (double Version, int Year)[] Years =
    {
        (10, 2026), (9, 2025), (8, 2024), (7, 2023), (6.5, 2022), (6, 2021), (5.5, 2020), (5, 2019), (4, 2018), (3, 2017), (2.4, 2016), (2.3, 2015),
    };

    /// <summary>The model year of a Tizen version (each year ships one).</summary>
    public static int YearOf(double tizen)
    {
        foreach (var (version, year) in Years)
            if (tizen >= version) return year;
        return 2015;
    }
}

/// <summary>
/// What a Samsung TV plays, reported to the server (capabilities version 2) so it sends files as
/// they are whenever the TV's player takes them: the web app's tizenCaps (apps/tizen/src/caps.ts),
/// the same report for the same TV. Picture subtitles are claimed when the app draws them
/// (<see cref="TizenFacts.PictureSubtitles"/>). Samsung TVs show no Dolby Vision. As JSON for the
/// play request.
/// </summary>
public static class TizenCaps
{
    /// <summary>The sound every Samsung TV decodes itself, whatever its year (later years add more).</summary>
    public static readonly IReadOnlyList<string> DecodedAudio = new[] { "aac", "ac3", "eac3", "mp3", "mp2", "flac", "vorbis", "pcm_s16le", "pcm_s24le" };

    /// <summary>
    /// The highest audio sample rate the TV decodes: Samsung's media specifications list 8 to 48 kHz
    /// for every codec, and AAC 5.1 at 96 kHz played as it is comes out garbled, its player saying nothing.
    /// </summary>
    public const int MostSampleRate = 48_000;

    static readonly string[] Containers = { "mkv", "mp4", "m4v", "mov", "ts", "m2ts", "mpegts", "webm", "avi", "mpg", "mpeg", "mp3", "m4a", "aac", "flac", "ogg", "oga", "wav", "mka" };

    /// <summary>Subtitles the app draws itself as WebVTT from the server; pictures come from their .mks.</summary>
    static readonly string[] TextSubtitles = { "subrip", "ass", "ssa", "webvtt", "mov_text" };

    /// <summary>Picture subtitles, drawn from their .mks when the app can (<see cref="TizenFacts.PictureSubtitles"/>).</summary>
    static readonly string[] PictureSubtitles = { "hdmv_pgs_subtitle", "dvd_subtitle", "dvb_subtitle" };

    // ffprobe's levels: H.264 4.2 = 42, 5.1 = 51; HEVC 4.1 = 123, 5.1 = 153, 5.2 = 156, 6.1 = 183.

    /// <param name="choices">
    /// Settings' passthrough choice per sound format (<see cref="Settings.PassthroughFormats"/>):
    /// On sends a format to the receiver whatever the TV's sound output says, Off never does
    /// (the server converts it); Auto, or a format left out, follows the sound output.
    /// </param>
    public static Dictionary<string, object?> Of(TizenFacts f, IReadOnlyDictionary<string, PassthroughMode>? choices = null)
    {
        var y = f.Year;
        var video = new List<Dictionary<string, object?>>();
        var h264 = new[] { "Constrained Baseline", "Baseline", "Main", "High" };
        if (f.Uhd) video.Add(Decoder("h264", h264, 3840, 2160, 60, 8, 51));
        video.Add(Decoder("h264", h264, 1920, 1080, y >= 2021 ? 120 : 60, 8, 42));
        var hevc = new[] { "Main", "Main 10" };
        if (f.Uhd8k && y >= 2019) video.Add(Decoder("hevc", hevc, 7680, 4320, 60, 10, 183));
        if (f.Uhd) video.Add(Decoder("hevc", hevc, 4096, 2160, 60, 10, y >= 2025 ? 156 : 153));
        else video.Add(Decoder("hevc", hevc, 1920, 1080, 60, 10, 123));
        var (bigW, bigH) = f.Uhd ? (3840, 2160) : (1920, 1080);
        video.Add(Decoder("vp9", y >= 2018 ? new[] { "Profile 0", "Profile 2" } : new[] { "Profile 0" }, bigW, bigH, 60, y >= 2018 ? 10 : 8));
        // AV1 from 2020's chips.
        if ((f.Av1 ?? y >= 2020) && y >= 2020)
            video.Add(f.Uhd8k ? Decoder("av1", new[] { "Main" }, 7680, 4320, 60, 10) : Decoder("av1", new[] { "Main" }, bigW, bigH, 60, 10));
        video.Add(Decoder("mpeg2video", null, 1920, 1080, 60, 8));
        video.Add(Decoder("mpeg4", null, 1920, 1080, 60, 8));

        var hdrTypes = new List<string>();
        if (f.Hdr)
        {
            hdrTypes.Add("hdr10");
            if (y >= 2017) hdrTypes.Add("hdr10plus");
            if (y >= 2018) hdrTypes.Add("hlg");
        }

        // Sound: what the TV decodes, what goes to a receiver as it is.
        var decode = new List<string>(DecodedAudio);
        if (y >= 2018) decode.Add("opus");
        if (y >= 2018 && f.Ac4 != false) decode.Add("ac4");
        // DTS until 2017: Samsung dropped it from 2018 sets on.
        var dtsDecoded = y <= 2017;
        if (dtsDecoded) decode.Add("dts");
        var mode = (f.OutputMode ?? "").ToUpperInvariant();
        var formats = new List<string>();
        if (mode == "DOLBY_DIGITAL_PLUS") formats.AddRange(new[] { "ac3", "eac3", "eac3-joc" });
        else if (mode == "DOLBY") formats.Add("ac3");
        else if (mode == "DTS" && dtsDecoded) formats.Add("dts");
        foreach (var choice in choices ?? new Dictionary<string, PassthroughMode>())
        {
            if (choice.Value == PassthroughMode.On && !formats.Contains(choice.Key)) formats.Add(choice.Key);
            else if (choice.Value == PassthroughMode.Off) formats.Remove(choice.Key);
        }
        // Atmos travels in E-AC-3 and DTS-HD in DTS: the server knows them by their codec.
        var passthrough = PassthroughCodecs.Where(c => formats.Any(x => CodecOf(x) == c)).ToList();

        var display = f.Uhd8k ? (7680, 4320) : f.Uhd ? (3840, 2160) : (1920, 1080);
        return new Dictionary<string, object?>
        {
            ["containers"] = Containers,
            ["video_codecs"] = video.Select(v => (string)v["codec"]!).Distinct().ToArray(),
            ["audio_codecs"] = decode.Concat(passthrough).Distinct().ToArray(),
            ["hdr"] = hdrTypes.Count > 0,
            ["dolby_vision"] = false,
            ["caps_version"] = 2,
            ["video"] = video,
            ["hdr_types"] = hdrTypes,
            ["dolby_vision_profiles"] = new object[0],
            ["audio_decode"] = decode,
            ["audio_passthrough"] = passthrough.Distinct().ToArray(),
            ["audio_channels"] = passthrough.Count > 0 ? 8 : 6,
            ["audio_max_sample_rate"] = MostSampleRate,
            // Pictures it can't draw aren't claimed: the server burns them in instead.
            ["subtitle_codecs"] = f.PictureSubtitles ? TextSubtitles.Concat(PictureSubtitles).ToArray() : TextSubtitles,
            ["audio_track_switching"] = true,
            ["picture_subtitles_sideload"] = f.PictureSubtitles,
            // One size per stream (no variants): the TV's player may go black moving between sizes (docs/QUALITY.md).
            ["hls"] = new Dictionary<string, object?> { ["master_playlist"] = true, ["segments"] = y >= 2020 ? new[] { "ts", "fmp4" } : new[] { "ts" } },
            ["display"] = new Dictionary<string, object?> { ["width"] = display.Item1, ["height"] = display.Item2, ["refresh_rates"] = new[] { 50, 60 } },
            ["device"] = new Dictionary<string, object?> { ["form"] = "tv", ["tier"] = Tier(f), ["fire_tv"] = false, ["model"] = f.Model },
        };
    }

    static readonly string[] PassthroughCodecs = { "ac3", "eac3", "truehd", "dts" };

    /// <summary>
    /// Audio a file can hold that the TV's player leaves out of its tracks: a 2025 S95F listed a
    /// film's E-AC-3 and not its TrueHD, so the player's first track was the file's second.
    /// </summary>
    public static readonly IReadOnlyList<string> PlayerLeavesOut = new[] { "truehd" };

    static string CodecOf(string format) => format switch
    {
        "eac3-joc" => "eac3",
        "dts-hd" => "dts",
        _ => format,
    };

    /// <summary>Low for 2018 sets, mid until 2021, high after.</summary>
    public static string Tier(TizenFacts f) => f.Year <= 2018 ? "low" : f.Year <= 2021 ? "mid" : "high";

    static Dictionary<string, object?> Decoder(string codec, string[]? profiles, int w, int h, int fps, int depth, int? level = null)
    {
        var d = new Dictionary<string, object?> { ["codec"] = codec, ["hardware"] = true };
        if (profiles != null) d["profiles"] = profiles;
        d["max_width"] = w;
        d["max_height"] = h;
        d["max_fps"] = fps;
        d["max_bit_depth"] = depth;
        if (level != null) d["max_level"] = level;
        return d;
    }
}
