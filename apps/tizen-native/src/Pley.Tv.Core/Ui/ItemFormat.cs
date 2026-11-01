using System;
using System.Collections.Generic;
using System.Globalization;
using System.Linq;
using System.Text.RegularExpressions;
using Pley.Tv.Core.Api;

namespace Pley.Tv.Core.Ui;

/// <summary>
/// What files and streams are like, in words, and the dates of item pages: the parts of the
/// Android app's model/format (Media.kt, Technical.kt, Sizes.kt, Languages.kt, Times.kt) that
/// <see cref="Format"/> doesn't have, as the web's labels (apps/web/src/lib/format.ts,
/// pages/item/shared.tsx, VideoItemView.tsx) say them.
/// </summary>
public static class ItemFormat
{
    static readonly Dictionary<string, string> Codecs = new Dictionary<string, string>
    {
        ["h264"] = "H.264", ["hevc"] = "HEVC", ["h265"] = "HEVC", ["av1"] = "AV1", ["vp9"] = "VP9", ["vp8"] = "VP8",
        ["mpeg4"] = "MPEG-4", ["mpeg2video"] = "MPEG-2", ["mpeg1video"] = "MPEG-1", ["vc1"] = "VC-1", ["wmv3"] = "WMV",
        ["msmpeg4v3"] = "DivX", ["theora"] = "Theora", ["prores"] = "ProRes", ["aac"] = "AAC", ["ac3"] = "AC3",
        ["eac3"] = "EAC3", ["dts"] = "DTS", ["truehd"] = "TrueHD", ["mlp"] = "MLP", ["flac"] = "FLAC", ["alac"] = "ALAC",
        ["mp3"] = "MP3", ["mp2"] = "MP2", ["opus"] = "Opus", ["vorbis"] = "Vorbis", ["wmav2"] = "WMA",
        ["pcm_s16le"] = "PCM", ["pcm_s24le"] = "PCM", ["pcm_s32le"] = "PCM", ["pcm_f32le"] = "PCM", ["subrip"] = "SRT",
        ["srt"] = "SRT", ["ass"] = "ASS", ["ssa"] = "SSA", ["webvtt"] = "WebVTT", ["mov_text"] = "MOV Text",
        ["hdmv_pgs_subtitle"] = "PGS", ["pgssub"] = "PGS", ["dvd_subtitle"] = "VobSub", ["dvdsub"] = "VobSub",
        ["dvb_subtitle"] = "DVB", ["dvb_teletext"] = "Teletext", ["eia_608"] = "CEA-608",
    };

    /// <summary>ffprobe's codec name as people know it: "h264" → "H.264", "hdmv_pgs_subtitle" → "PGS"; others upper-cased.</summary>
    public static string CodecLabel(string? codec)
    {
        if (string.IsNullOrEmpty(codec)) return "";
        return Codecs.TryGetValue(codec!.ToLowerInvariant(), out var name) ? name : codec.ToUpperInvariant();
    }

    /// <summary>2 → "Stereo", 6 → "5.1", 8 → "7.1", 5 → "5ch"; the layout's name without a count.</summary>
    public static string ChannelsLabel(int? channels, string? layout = null) => channels switch
    {
        null or 0 => layout ?? "",
        1 => "Mono",
        2 => "Stereo",
        3 => "2.1",
        6 => "5.1",
        7 => "6.1",
        8 => "7.1",
        _ => $"{channels}ch",
    };

    /// <summary>"4K", "1440p", "1080p", "720p", "576p", "480p" or "SD", cropped pictures included; null without a size.</summary>
    public static string? ResolutionLabel(int? width, int? height)
    {
        var w = width ?? 0;
        var h = height ?? 0;
        if (w == 0 && h == 0) return null;
        if (w >= 3800 || h >= 2100) return "4K";
        if (w >= 2500 || h >= 1400) return "1440p";
        if (w >= 1800 || h >= 1000) return "1080p";
        if (w >= 1200 || h >= 700) return "720p";
        if (h >= 540) return "576p";
        if (h >= 470) return "480p";
        return "SD";
    }

    /// <summary><see cref="Format.DynamicRangeLabel"/> of a video stream: "Dolby Vision P8.1 + HDR10+" for a hybrid.</summary>
    public static string? DynamicRangeLabel(MediaStream? stream) =>
        stream == null ? null : Format.DynamicRangeLabel(stream.DynamicRange, stream.DvProfile, stream.DvBlCompat, stream.Hdr, stream.DvEl, stream.Hdr10plus);

    /// <summary>A video stream's dynamic range badges: "DV P8.1", and "HDR10+" too for Dolby Vision + HDR10+.</summary>
    public static IReadOnlyList<string> DynamicRangeBadges(MediaStream? stream)
    {
        if (stream == null) return Array.Empty<string>();
        var badge = Format.DynamicRangeShort(stream.DynamicRange, stream.DvProfile, stream.DvBlCompat, stream.Hdr, stream.DvEl);
        if (badge == null) return Array.Empty<string>();
        return stream.DynamicRange == "dolby_vision" && stream.Hdr10plus == true ? new[] { badge, "HDR10+" } : new[] { badge };
    }

    /// <summary>Dolby Atmos, as ffprobe names it in the stream's profile ("Dolby TrueHD + Dolby Atmos") or a title says.</summary>
    public static bool IsAtmos(MediaStream? stream) =>
        stream != null && stream.Kind == "audio"
        && ((stream.Profile ?? "").IndexOf("atmos", StringComparison.OrdinalIgnoreCase) >= 0
            || (stream.Title ?? "").IndexOf("atmos", StringComparison.OrdinalIgnoreCase) >= 0);

    /// <summary>An audio stream's format: "TrueHD Atmos 7.1", "EAC3 5.1", "AAC Stereo".</summary>
    public static string AudioFormatLabel(MediaStream stream) =>
        Join(" ", CodecLabel(stream.Codec), IsAtmos(stream) ? "Atmos" : "", ChannelsLabel(stream.Channels, stream.ChannelLayout));

    /// <summary>
    /// A track as the pickers list it. Audio: "English (EAC3 5.1)", "English – Commentary (AAC
    /// Stereo)". Subtitles: "French – SDH (SRT, Forced)", "Portuguese (BR) (PGS)". Video: "4K HEVC
    /// Dolby Vision P8.1 10-bit".
    /// </summary>
    public static string StreamLabel(MediaStream s)
    {
        var lang = LanguageName(s.Language);
        var title = string.IsNullOrEmpty(s.Title) ? null : s.Title;
        var extra = title != null && lang != null && title.ToLowerInvariant().IndexOf(lang.ToLowerInvariant(), StringComparison.Ordinal) < 0 ? $" – {title}" : "";
        switch (s.Kind)
        {
            case "audio":
            {
                var details = AudioFormatLabel(s);
                var name = title != null && lang == null ? title : lang ?? title ?? "Unknown";
                return name + extra + (details.Length > 0 ? $" ({details})" : "");
            }
            case "subtitle":
            {
                var name = lang ?? title ?? "Unknown";
                var flags = Join(", ", CodecLabel(s.Codec), IsForced(s) ? "Forced" : "", s.External == true ? "External" : "");
                return name + extra + (flags.Length > 0 ? $" ({flags})" : "");
            }
            default:
                return Join(" ", ResolutionLabel(s.Width, s.Height), CodecLabel(s.Codec), DynamicRangeLabel(s), s.BitDepth > 8 ? $"{s.BitDepth}-bit" : null);
        }
    }

    static readonly Regex Words = new Regex(@"[^\p{L}\p{N}]+", RegexOptions.CultureInvariant);

    /// <summary>
    /// Forced subtitles: flagged so, or only named so ("English (Forced)", "Signs &amp; Songs"), as
    /// the server counts them when it chooses.
    /// </summary>
    public static bool IsForced(MediaStream s) =>
        s.Forced || Words.Split((s.Title ?? "").ToLowerInvariant()).Any(w => w == "forced" || w == "signs");

    /// <summary>The audio track a file starts with: the profile's choice, else the default, else the first.</summary>
    public static MediaStream? StartingAudio(MediaFile file)
    {
        var audio = file.Streams.Where(s => s.Kind == "audio").ToList();
        return audio.FirstOrDefault(s => s.Index == file.SelectedAudio) ?? audio.FirstOrDefault(s => s.Default) ?? audio.FirstOrDefault();
    }

    /// <summary>
    /// The short quality badges of a file, in the web's order: resolution ("4K", "1080p", "720p",
    /// "SD"), dynamic range ("DV P8.1", "HDR10+", "HDR10", "HLG", "HDR"), then "Atmos", else "7.1"
    /// or "5.1".
    /// </summary>
    public static IReadOnlyList<string> QualityBadges(MediaFile? file)
    {
        if (file == null) return Array.Empty<string>();
        var video = file.Streams.FirstOrDefault(s => s.Kind == "video");
        var audio = file.Streams.FirstOrDefault(s => s.Kind == "audio" && s.Default) ?? file.Streams.FirstOrDefault(s => s.Kind == "audio");
        var channels = audio?.Channels ?? 0;
        var badges = new List<string>();
        if (Format.ResolutionBadge(file.Width ?? video?.Width, file.Height ?? video?.Height) is { } resolution) badges.Add(resolution);
        badges.AddRange(DynamicRangeBadges(video));
        if (IsAtmos(audio)) badges.Add("Atmos");
        else if (channels >= 8) badges.Add("7.1");
        else if (channels >= 6) badges.Add("5.1");
        return badges;
    }

    /// <summary>
    /// A version in the web's picker: "Final Cut · 4K · HEVC · MKV · 58.2 GB", else "Version 2". A
    /// size of 0 means unknown here.
    /// </summary>
    public static string VersionLabel(MediaFile file, int position)
    {
        var label = Join(" · ", file.Edition, ResolutionLabel(file.Width, file.Height), CodecLabel(file.VideoCodec),
            file.Container?.ToUpperInvariant(), file.Size > 0 ? Bytes(file.Size) : null);
        return label.Length > 0 ? label : $"Version {position + 1}";
    }

    /// <summary>A version in an item's Details: "4K · HEVC · Dolby Vision P8.1 · TrueHD Atmos 7.1 · 62.0 GB".</summary>
    public static string VersionDescription(MediaFile file)
    {
        var video = file.Streams.FirstOrDefault(s => s.Kind == "video");
        var audio = StartingAudio(file);
        return Join(" · ", file.Edition, ResolutionLabel(file.Width ?? video?.Width, file.Height ?? video?.Height),
            CodecLabel(file.VideoCodec ?? video?.Codec), DynamicRangeLabel(video), audio == null ? null : AudioFormatLabel(audio),
            file.Size > 0 ? Bytes(file.Size) : null);
    }

    // ---- sizes and rates, with a point for decimals whatever the language, like JavaScript's toFixed

    static readonly string[] ByteUnits = { "B", "KB", "MB", "GB", "TB" };

    /// <summary>1536 → "1.5 KB", 150 MiB → "150 MB": three significant digits, in units of 1024, up to TB.</summary>
    public static string Bytes(long? bytes)
    {
        if (bytes == null) return "";
        double v = bytes.Value;
        var i = 0;
        while (v >= 1024 && i < ByteUnits.Length - 1)
        {
            v /= 1024;
            i++;
        }
        return $"{Fixed(v, v >= 100 || i == 0 ? 0 : 1)} {ByteUnits[i]}";
    }

    /// <summary>Bits per second → "8.2 Mbps", "12 Mbps", "320 kbps"; empty without a rate.</summary>
    public static string Bitrate(long? bps)
    {
        if (bps == null || bps == 0) return "";
        if (bps >= 1_000_000) return $"{Fixed(bps.Value / 1_000_000.0, bps >= 10_000_000 ? 0 : 1)} Mbps";
        return $"{(long)Math.Floor(bps.Value / 1000.0 + 0.5)} kbps";
    }

    /// <summary><paramref name="value"/> with <paramref name="digits"/> decimals and a point.</summary>
    internal static string Fixed(double value, int digits) => value.ToString("F" + digits, CultureInfo.InvariantCulture);

    /// <summary><paramref name="value"/> with up to <paramref name="digits"/> decimals, trailing zeros dropped: 24.0 → "24", 23.976 → "23.976".</summary>
    static string Trimmed(double value, int digits) => Fixed(value, digits).TrimEnd('0').TrimEnd('.');

    // ---- technical details, for media info

    /// <summary>
    /// ffprobe's level as written: HEVC counts thirtieths (153 is 5.1), H.264 and VP9 tenths (41
    /// is 4.1), AV1 its seq_level_idx (13 is 5.1); null when unknown or for other codecs.
    /// </summary>
    public static string? LevelLabel(string? codec, int? level)
    {
        if (level == null || level <= 0) return null;
        return codec?.ToLowerInvariant() switch
        {
            "hevc" or "h265" => Trimmed(level.Value / 30.0, 1),
            "h264" or "vp9" => Trimmed(level.Value / 10.0, 1),
            "av1" => $"{2 + (level.Value >> 2)}.{level.Value & 3}",
            _ => null,
        };
    }

    /// <summary>"Main 10 · Level 5.1", "High · Level 4", "LC"; empty when neither is known.</summary>
    public static string ProfileLevelLabel(MediaStream s) =>
        Join(" · ", string.IsNullOrWhiteSpace(s.Profile) ? null : s.Profile, LevelLabel(s.Codec, s.Level) is { } l ? $"Level {l}" : null);

    /// <summary>23.976 → "23.976 fps", 24.0 → "24 fps"; null when unknown.</summary>
    public static string? FrameRateLabel(double? fps) => fps > 0 ? Trimmed(fps.Value, 3) + " fps" : null;

    /// <summary>48000 → "48 kHz", 44100 → "44.1 kHz"; null when unknown.</summary>
    public static string? SampleRateLabel(int? hz) => hz > 0 ? Trimmed(hz.Value / 1000.0, 1) + " kHz" : null;

    /// <summary>"yuv420p10le" → "4:2:0"; null for formats that aren't YUV.</summary>
    public static string? ChromaLabel(string? pixFmt)
    {
        var f = pixFmt?.ToLowerInvariant();
        if (f == null) return null;
        bool Starts(params string[] prefixes) => prefixes.Any(p => f.StartsWith(p, StringComparison.Ordinal));
        if (Starts("yuv420", "yuvj420", "nv12", "p010")) return "4:2:0";
        if (Starts("yuv422", "yuvj422")) return "4:2:2";
        if (Starts("yuv444", "yuvj444")) return "4:4:4";
        if (Starts("gray")) return "4:0:0";
        return null;
    }

    /// <summary>"10-bit · 4:2:0", from the stream's bit depth and pixel format; empty when neither is known.</summary>
    public static string BitDepthLabel(MediaStream s) => Join(" · ", s.BitDepth > 0 ? $"{s.BitDepth}-bit" : null, ChromaLabel(s.PixFmt));

    static readonly Dictionary<string, string> Primaries = new Dictionary<string, string>
    {
        ["bt709"] = "BT.709", ["bt2020"] = "BT.2020", ["smpte170m"] = "BT.601 NTSC", ["bt470bg"] = "BT.601 PAL",
        ["bt470m"] = "BT.470 M", ["smpte240m"] = "SMPTE 240M", ["smpte431"] = "DCI-P3", ["smpte432"] = "Display P3", ["film"] = "Film",
    };

    static readonly Dictionary<string, string> Transfers = new Dictionary<string, string>
    {
        ["smpte2084"] = "PQ (SMPTE ST 2084)", ["arib-std-b67"] = "HLG (ARIB STD-B67)", ["bt709"] = "BT.709",
        ["smpte170m"] = "BT.601", ["bt470bg"] = "BT.470 BG", ["bt2020-10"] = "BT.2020 (10-bit)", ["bt2020-12"] = "BT.2020 (12-bit)",
        ["iec61966-2-1"] = "sRGB", ["linear"] = "Linear", ["smpte240m"] = "SMPTE 240M",
    };

    static readonly Dictionary<string, string> Matrices = new Dictionary<string, string>
    {
        ["bt709"] = "BT.709", ["bt2020nc"] = "BT.2020 non-constant", ["bt2020c"] = "BT.2020 constant",
        ["smpte170m"] = "BT.601", ["bt470bg"] = "BT.601", ["ictcp"] = "ICtCp", ["gbr"] = "RGB", ["ycgco"] = "YCgCo",
        ["smpte240m"] = "SMPTE 240M", ["fcc"] = "FCC",
    };

    /// <summary>ffprobe's colour primaries as people know them: "bt2020" → "BT.2020"; others as they are; null when unknown.</summary>
    public static string? ColorPrimariesLabel(string? name) => ColorName(name, Primaries);

    /// <summary>ffprobe's transfer: "smpte2084" → "PQ (SMPTE ST 2084)", "arib-std-b67" → "HLG (ARIB STD-B67)".</summary>
    public static string? ColorTransferLabel(string? name) => ColorName(name, Transfers);

    /// <summary>ffprobe's colour space (the matrix): "bt2020nc" → "BT.2020 non-constant".</summary>
    public static string? ColorMatrixLabel(string? name) => ColorName(name, Matrices);

    static string? ColorName(string? name, Dictionary<string, string> names)
    {
        var n = name?.Trim().ToLowerInvariant();
        if (string.IsNullOrEmpty(n) || n == "unknown" || n == "reserved") return null;
        return names.TryGetValue(n!, out var known) ? known : n;
    }

    /// <summary>A Dolby Vision base layer's compatibility: 1 "HDR10", 2 "SDR", 4 "HLG", 6 "Blu-ray HDR10"; null for none or unknown.</summary>
    public static string? DvCompatLabel(int? blCompat) => blCompat switch
    {
        1 => "HDR10",
        2 => "SDR",
        4 => "HLG",
        6 => "Blu-ray HDR10",
        _ => null,
    };

    /// <summary>Dolby Vision in full: "Profile 8.1 · Level 6 · HDR10-compatible base layer"; null for other streams.</summary>
    public static string? DolbyVisionDetails(MediaStream s)
    {
        if (s.DynamicRange != "dolby_vision") return null;
        var compat = s.DvBlCompat;
        var profile = s.DvProfile is { } p ? $"Profile {p}" + (p >= 8 && compat > 0 ? $".{compat}" : "") : null;
        var baseLayer = compat == null ? null
            : compat == 0 ? "no compatible base layer"
            : DvCompatLabel(compat) is { } c ? $"{c}-compatible base layer" : null;
        var layer = s.DvProfile == 7 ? s.DvEl switch
        {
            "fel" => "full enhancement layer",
            "mel" => "minimal enhancement layer",
            _ => null,
        } : null;
        var text = Join(" · ", profile, s.DvLevel > 0 ? $"Level {s.DvLevel}" : null, baseLayer, layer);
        return text.Length > 0 ? text : null;
    }

    /// <summary>The HDR format of a video stream: "Dolby Vision P8.1 + HDR10+", "HDR10", "HLG", or "SDR".</summary>
    public static string HdrFormatLabel(MediaStream s) => DynamicRangeLabel(s) ?? "SDR";

    /// <summary>"3840×2160 (4K)"; null without a size.</summary>
    public static string? DimensionsLabel(int? width, int? height)
    {
        var w = width ?? 0;
        var h = height ?? 0;
        if (w <= 0 || h <= 0) return null;
        return $"{w}×{h}" + (ResolutionLabel(w, h) is { } r ? $" ({r})" : "");
    }

    // ---- languages, as apps/web/src/lib/languages.ts

    static readonly Dictionary<string, string> Languages = new Dictionary<string, string>
    {
        ["afr"] = "Afrikaans", ["alb"] = "Albanian", ["sqi"] = "Albanian", ["amh"] = "Amharic", ["ara"] = "Arabic",
        ["arm"] = "Armenian", ["hye"] = "Armenian", ["aze"] = "Azerbaijani", ["baq"] = "Basque", ["eus"] = "Basque",
        ["bel"] = "Belarusian", ["ben"] = "Bengali", ["bos"] = "Bosnian", ["bre"] = "Breton", ["bul"] = "Bulgarian",
        ["bur"] = "Burmese", ["mya"] = "Burmese", ["cat"] = "Catalan", ["chi"] = "Chinese", ["zho"] = "Chinese",
        ["cze"] = "Czech", ["ces"] = "Czech", ["dan"] = "Danish", ["dut"] = "Dutch", ["nld"] = "Dutch", ["eng"] = "English",
        ["est"] = "Estonian", ["fao"] = "Faroese", ["fil"] = "Filipino", ["fin"] = "Finnish", ["fre"] = "French",
        ["fra"] = "French", ["geo"] = "Georgian", ["kat"] = "Georgian", ["ger"] = "German", ["deu"] = "German",
        ["gle"] = "Irish", ["glg"] = "Galician", ["gre"] = "Greek", ["ell"] = "Greek", ["guj"] = "Gujarati",
        ["heb"] = "Hebrew", ["hin"] = "Hindi", ["hrv"] = "Croatian", ["hun"] = "Hungarian", ["ice"] = "Icelandic",
        ["isl"] = "Icelandic", ["ind"] = "Indonesian", ["ita"] = "Italian", ["jpn"] = "Japanese", ["kan"] = "Kannada",
        ["kaz"] = "Kazakh", ["khm"] = "Khmer", ["kor"] = "Korean", ["kur"] = "Kurdish", ["lao"] = "Lao", ["lat"] = "Latin",
        ["lav"] = "Latvian", ["lit"] = "Lithuanian", ["ltz"] = "Luxembourgish", ["mac"] = "Macedonian",
        ["mkd"] = "Macedonian", ["mal"] = "Malayalam", ["mao"] = "Maori", ["mri"] = "Maori", ["mar"] = "Marathi",
        ["may"] = "Malay", ["msa"] = "Malay", ["mlt"] = "Maltese", ["mon"] = "Mongolian", ["nep"] = "Nepali",
        ["nob"] = "Norwegian Bokmål", ["nno"] = "Norwegian Nynorsk", ["nor"] = "Norwegian", ["per"] = "Persian",
        ["fas"] = "Persian", ["pol"] = "Polish", ["por"] = "Portuguese", ["pan"] = "Punjabi", ["rum"] = "Romanian",
        ["ron"] = "Romanian", ["rus"] = "Russian", ["slo"] = "Slovak", ["slk"] = "Slovak", ["slv"] = "Slovenian",
        ["som"] = "Somali", ["spa"] = "Spanish", ["srp"] = "Serbian", ["swa"] = "Swahili", ["swe"] = "Swedish",
        ["tam"] = "Tamil", ["tel"] = "Telugu", ["tgl"] = "Tagalog", ["tha"] = "Thai", ["tib"] = "Tibetan",
        ["bod"] = "Tibetan", ["tur"] = "Turkish", ["ukr"] = "Ukrainian", ["urd"] = "Urdu", ["uzb"] = "Uzbek",
        ["vie"] = "Vietnamese", ["wel"] = "Welsh", ["cym"] = "Welsh", ["yid"] = "Yiddish", ["zul"] = "Zulu",
        ["yue"] = "Cantonese", ["cmn"] = "Mandarin", ["nan"] = "Min Nan", ["hat"] = "Haitian Creole",
        ["haw"] = "Hawaiian", ["ibo"] = "Igbo", ["yor"] = "Yoruba", ["hau"] = "Hausa", ["sin"] = "Sinhala",
        ["pus"] = "Pashto", ["tat"] = "Tatar", ["kir"] = "Kyrgyz", ["tuk"] = "Turkmen", ["tgk"] = "Tajik",
        ["und"] = "Unknown", ["mul"] = "Multiple", ["zxx"] = "No linguistic content", ["qaa"] = "Original",
        // ISO 639-1
        ["af"] = "Afrikaans", ["sq"] = "Albanian", ["am"] = "Amharic", ["ar"] = "Arabic", ["hy"] = "Armenian",
        ["az"] = "Azerbaijani", ["eu"] = "Basque", ["be"] = "Belarusian", ["bn"] = "Bengali", ["bs"] = "Bosnian",
        ["bg"] = "Bulgarian", ["my"] = "Burmese", ["ca"] = "Catalan", ["zh"] = "Chinese", ["cs"] = "Czech",
        ["da"] = "Danish", ["nl"] = "Dutch", ["en"] = "English", ["et"] = "Estonian", ["fi"] = "Finnish",
        ["fr"] = "French", ["ka"] = "Georgian", ["de"] = "German", ["ga"] = "Irish", ["gl"] = "Galician",
        ["el"] = "Greek", ["gu"] = "Gujarati", ["he"] = "Hebrew", ["hi"] = "Hindi", ["hr"] = "Croatian",
        ["hu"] = "Hungarian", ["is"] = "Icelandic", ["id"] = "Indonesian", ["it"] = "Italian", ["ja"] = "Japanese",
        ["kn"] = "Kannada", ["kk"] = "Kazakh", ["km"] = "Khmer", ["ko"] = "Korean", ["ku"] = "Kurdish", ["lo"] = "Lao",
        ["la"] = "Latin", ["lv"] = "Latvian", ["lt"] = "Lithuanian", ["lb"] = "Luxembourgish", ["mk"] = "Macedonian",
        ["ml"] = "Malayalam", ["mi"] = "Maori", ["mr"] = "Marathi", ["ms"] = "Malay", ["mt"] = "Maltese",
        ["mn"] = "Mongolian", ["ne"] = "Nepali", ["nb"] = "Norwegian Bokmål", ["nn"] = "Norwegian Nynorsk",
        ["no"] = "Norwegian", ["fa"] = "Persian", ["pl"] = "Polish", ["pt"] = "Portuguese", ["pa"] = "Punjabi",
        ["ro"] = "Romanian", ["ru"] = "Russian", ["sk"] = "Slovak", ["sl"] = "Slovenian", ["so"] = "Somali",
        ["es"] = "Spanish", ["sr"] = "Serbian", ["sw"] = "Swahili", ["sv"] = "Swedish", ["ta"] = "Tamil",
        ["te"] = "Telugu", ["tl"] = "Tagalog", ["th"] = "Thai", ["bo"] = "Tibetan", ["tr"] = "Turkish",
        ["uk"] = "Ukrainian", ["ur"] = "Urdu", ["uz"] = "Uzbek", ["vi"] = "Vietnamese", ["cy"] = "Welsh",
        ["yi"] = "Yiddish", ["zu"] = "Zulu",
    };

    /// <summary>"eng" → "English", "pt-BR" → "Portuguese (BR)"; an unknown code comes back upper-cased; null without one.</summary>
    public static string? LanguageName(string? code)
    {
        var raw = code?.Trim() ?? "";
        if (raw.Length == 0) return null;
        var parts = raw.ToLowerInvariant().Split('-', '_');
        if (!Languages.TryGetValue(parts[0], out var name)) return raw.Length <= 3 ? raw.ToUpperInvariant() : raw;
        return parts.Length > 1 ? $"{name} ({parts[1].ToUpperInvariant()})" : name;
    }

    // ---- titles and dates

    /// <summary>A season's own title, else "Specials" for season 0, else "Season 2".</summary>
    public static string SeasonTitle(string? title, int? index) =>
        !string.IsNullOrEmpty(title) ? title! : index == 0 ? "Specials" : index != null ? $"Season {index}" : "Season";

    /// <summary>"2023-04-01" → "Apr 1, 2023" (en-US) or "1 Apr 2023" (en-GB); what doesn't parse comes back as it is.</summary>
    public static string Date(string? date, CultureInfo? culture = null) => DateIn(date, culture, "MMM");

    /// <summary>"2024-03-12" → "March 12, 2024" (en-US) or "12 March 2024" (en-GB): air dates on item pages.</summary>
    public static string LongDate(string? date, CultureInfo? culture = null) => DateIn(date, culture, "MMMM");

    /// <summary>
    /// A day in the order <paramref name="culture"/> puts the day and the month, with its month
    /// names, as Java's medium and long styles do for the languages people use: .NET has no
    /// medium style of its own.
    /// </summary>
    static string DateIn(string? date, CultureInfo? culture, string month)
    {
        if (string.IsNullOrEmpty(date)) return "";
        if (!DateTime.TryParseExact(date, "yyyy-MM-dd", CultureInfo.InvariantCulture, DateTimeStyles.None, out var day)) return date!;
        var c = culture ?? CultureInfo.CurrentCulture;
        var dayFirst = c.DateTimeFormat.MonthDayPattern.TrimStart().StartsWith("d", StringComparison.Ordinal);
        return day.ToString(dayFirst ? $"d {month} yyyy" : $"{month} d, yyyy", c);
    }

    /// <summary>The parts that are neither null nor empty, joined.</summary>
    internal static string Join(string separator, params string?[] parts) => string.Join(separator, parts.Where(p => !string.IsNullOrEmpty(p)));
}
