using System;
using System.Collections.Generic;
using System.Globalization;
using System.Linq;
using System.Text.RegularExpressions;
using Pley.Tv.Core.Api;
using Pley.Tv.Core.Ui;

namespace Pley.Tv.Core.Player;

/// <summary>How subtitles reach the screen.</summary>
public enum SubtitleDelivery
{
    /// <summary>The server's WebVTT, drawn by the app.</summary>
    Server,
    /// <summary>The server's .mks of a picture track, drawn by the app.</summary>
    ServerPictures,
    /// <summary>Drawn into the picture by the server's transcode.</summary>
    BurnedIn,
}

/// <summary>One line of the details: "Video", "HEVC 4K HDR10 → H.264 1080p SDR".</summary>
public sealed record DetailRow(string Label, string Value);

/// <summary>How the video plays, while the details are open (the Android app's PlaybackDetailsUi).</summary>
/// <param name="Method">"Direct play", "Direct stream", "Transcoding", "Direct stream · Audio transcoding".</param>
/// <param name="Why">Why, in a sentence or two; null for a direct play that needs no reason.</param>
/// <param name="Pipeline">Where a transcode runs ("GPU", "CPU"…), for its badge; null otherwise.</param>
/// <param name="Video">The streams in play: the file's video, the audio and the subtitles chosen.</param>
/// <param name="Transcode">The transcode's live numbers (remux and transcode); null in direct play or before they come.</param>
public sealed record PlaybackDetails(
    string Method,
    string? Why,
    string? Pipeline,
    IReadOnlyList<DetailRow> Rows,
    string PlaybackMethod,
    MediaFile? File,
    MediaStream? Video,
    MediaStream? Audio,
    MediaStream? Subtitle,
    SubtitleDelivery? SubtitleDelivery,
    TranscodeInfo? Transcode);

/// <summary>
/// How a video plays, for the Info panel (the Android app's PlaybackDetails;
/// apps/web/src/player/PlaybackDetails.tsx): the way, why in a sentence, then what goes in and
/// what comes out, with a transcode's live numbers.
/// </summary>
public static class Details
{
    static readonly Dictionary<string, string> Encoders = new Dictionary<string, string>
    {
        ["libx264"] = "x264", ["libx265"] = "x265",
        ["h264_nvenc"] = "NVIDIA NVENC", ["hevc_nvenc"] = "NVIDIA NVENC",
        ["h264_qsv"] = "Intel Quick Sync", ["hevc_qsv"] = "Intel Quick Sync",
        ["h264_amf"] = "AMD AMF", ["hevc_amf"] = "AMD AMF",
        ["h264_vaapi"] = "VA-API", ["hevc_vaapi"] = "VA-API",
        ["h264_videotoolbox"] = "VideoToolbox", ["hevc_videotoolbox"] = "VideoToolbox",
        ["h264_vulkan"] = "Vulkan", ["hevc_vulkan"] = "Vulkan",
        ["h264_v4l2m2m"] = "V4L2",
    };

    /// <summary>Where a transcode runs, by the transcoder's name for it ("hybrid": "GPU and CPU"); one not known as it is.</summary>
    static string Pipeline(string pipeline) => pipeline switch
    {
        "gpu" => "GPU",
        "hybrid" => Strings.T("player.details.pipeline.hybrid"),
        "cpu" => "CPU",
        "copy" => Strings.T("player.details.pipeline.copy"),
        _ => pipeline,
    };

    static readonly Dictionary<int, string> Channels = new Dictionary<int, string> { [1] = "1.0", [2] = "2.0", [3] = "2.1", [6] = "5.1", [7] = "6.1", [8] = "7.1" };
    static readonly Regex Size = new Regex(@"(\d+)×(\d+)", RegexOptions.CultureInvariant);

    /// <summary>A video stream as "HEVC 4K Dolby Vision P8.1", "HEVC 4K Dolby Vision P7 FEL".</summary>
    public static string VideoLabel(MediaStream? v) =>
        v == null ? "" : ItemFormat.Join(" ", ItemFormat.CodecLabel(v.Codec), ItemFormat.ResolutionLabel(v.Width, v.Height), ItemFormat.DynamicRangeLabel(v));

    /// <summary>An audio stream as "TrueHD 7.1".</summary>
    public static string AudioLabel(MediaStream? a)
    {
        if (a == null) return "";
        string? channels = a.Channels is int c ? Channels.TryGetValue(c, out var name) ? name : Strings.T("player.details.channels", ("n", c)) : null;
        return ItemFormat.Join(" ", ItemFormat.CodecLabel(a.Codec), channels);
    }

    /// <summary>The transcoder's "H.264 1920×1080" as "H.264 1080p".</summary>
    public static string Simplified(string text) =>
        Size.Replace(text, m => ItemFormat.ResolutionLabel(int.Parse(m.Groups[1].Value, CultureInfo.InvariantCulture), int.Parse(m.Groups[2].Value, CultureInfo.InvariantCulture)) ?? m.Value);

    /// <summary>The encoder and where it runs: "NVIDIA NVENC (GPU)", "x264 veryfast (CPU)".</summary>
    public static string EncoderLabel(TranscodeInfo t)
    {
        var name = t.Encoder != null ? Encoders.TryGetValue(t.Encoder, out var known) ? known : t.Encoder : null;
        var where = t.Pipeline != null && t.Pipeline != "copy" ? $"({Pipeline(t.Pipeline)})" : null;
        return ItemFormat.Join(" ", name, t.Preset, where);
    }

    /// <summary>
    /// What the screen gets of HDR video: the live transcode's tone mapping when there is one, else
    /// what the server said ("HDR10 (Dolby Vision base layer)"), else null (SDR, or an older server).
    /// </summary>
    public static string? ShownAs(PlaybackInfo info, TranscodeInfo? live) => live?.Tonemap is { } tonemap ? SessionText.ToneMapped(tonemap) : info.VideoRange;

    /// <summary>
    /// How <paramref name="subtitle"/> reaches the screen in <paramref name="info"/>. This app draws
    /// text from the server's WebVTT, in direct play too (the TV's player draws none); pictures it
    /// can't draw (<paramref name="sideload"/> false) the server burns in.
    /// </summary>
    public static SubtitleDelivery DeliveryOf(PlaybackInfo info, MediaStream subtitle, bool sideload)
    {
        if (info.BurnSubtitles && info.SubtitleIndex == subtitle.Index) return SubtitleDelivery.BurnedIn;
        if (!PlaybackPlan.IsImage(subtitle)) return SubtitleDelivery.Server;
        return sideload ? SubtitleDelivery.ServerPictures : SubtitleDelivery.BurnedIn;
    }

    /// <summary>
    /// The details of <paramref name="info"/> playing <paramref name="file"/> with audio
    /// <paramref name="audioIndex"/>. <paramref name="note"/> says why it plays this way when the
    /// player knows better than the server (a fallback); <paramref name="live"/> is GET
    /// /api/playback/{session}'s transcode.
    /// </summary>
    public static PlaybackDetails Build(PlaybackInfo info, MediaFile? file, int? audioIndex, string? note, long positionMs, TranscodeInfo? live, MediaStream? subtitle = null, bool sideload = false)
    {
        var transcode = info.Method == "direct" ? null : live;
        var streams = file?.Streams ?? new MediaStream[0];
        var video = streams.FirstOrDefault(s => s.Kind == "video");
        var audios = streams.Where(s => s.Kind == "audio").ToList();
        var want = audioIndex ?? info.AudioIndex;
        var audio = audios.FirstOrDefault(s => s.Index == want) ?? audios.FirstOrDefault(s => s.Default) ?? audios.FirstOrDefault();
        // The file as it is needs no reason; a fallback's says why it plays so. Nothing more specific known: the method says it.
        var reasons = string.Join(" ", info.Reasons ?? new string[0]);
        var why = info.Method == "direct" ? note : note ?? (reasons.Length > 0 ? reasons : null) ?? info.Reason;
        const string pending = "…";
        var videoIn = transcode?.VideoIn is { } vin ? Simplified(vin) : VideoLabel(video);
        var audioIn = transcode?.AudioIn ?? AudioLabel(audio);
        var converting = info.Method != "direct";
        var videoRow = converting && transcode?.Video != "copy"
            ? $"{videoIn} → " + (transcode?.VideoOut is { } vout ? Simplified(vout) + (transcode.Tonemap != null ? " SDR" : "") : pending)
            : videoIn;
        var audioRow = converting && transcode?.Audio != "copy" ? $"{audioIn} → {transcode?.AudioOut ?? pending}" : audioIn;
        var bitrate = ItemFormat.Bitrate(file?.Bitrate);
        var bitrateRow = transcode?.BitrateOut != null && bitrate.Length > 0 ? $"{bitrate} → {ItemFormat.Bitrate(transcode.BitrateOut)}" : bitrate;
        var ahead = transcode != null ? (int)Math.Max(0, (transcode.TranscodedMs - positionMs) / 1000) : 0;

        var rows = new List<DetailRow>();
        if (video != null) rows.Add(new DetailRow(SessionText.RowVideo, videoRow));
        if (audio != null) rows.Add(new DetailRow(SessionText.RowAudio, audioRow));
        if (bitrateRow.Length > 0) rows.Add(new DetailRow(SessionText.RowBitrate, bitrateRow));
        if (transcode?.Video == "transcode" && transcode.Encoder != null) rows.Add(new DetailRow(SessionText.RowEncoder, EncoderLabel(transcode)));
        if (video != null && ShownAs(info, transcode) is { } shown) rows.Add(new DetailRow(SessionText.RowHdr, shown));
        if (transcode?.SubtitleBurn == true) rows.Add(new DetailRow(SessionText.RowSubtitles, SessionText.BurnedIn));
        if (transcode?.Speed is double speed && speed > 0) rows.Add(new DetailRow(SessionText.RowSpeed, SessionText.Speed(speed.ToString("F1", PlayerText.Numbers), ahead)));
        string? pipeline = transcode?.Pipeline != null && transcode.Video == "transcode" ? Pipeline(transcode.Pipeline) : null;
        return new PlaybackDetails(
            SessionText.Method(info), why, pipeline, rows, info.Method, file, video, audio, subtitle,
            subtitle != null ? DeliveryOf(info, subtitle, sideload) : null, transcode);
    }
}

/// <summary>What the readout is made of: the details, the engine's stats and the session's own counts, read at most once a second.</summary>
/// <param name="BufferedAheadMs">How far the engine has loaded past the playhead.</param>
/// <param name="Quality">"Original", "Automatic · 8 Mbps 1080p".</param>
/// <param name="Stalls">Times the video stopped to wait for more once it played, seeks aside.</param>
/// <param name="FirstFrameMs">Time to the first frame of the session playing.</param>
public sealed record ReadoutInput(PlaybackDetails? Details = null, EngineStats? Stats = null, long BufferedAheadMs = 0, string Quality = "", int Stalls = 0, long? FirstFrameMs = null);

/// <summary>
/// The stats for nerds, in words (the Android app's TvReadout): how the server sends it and why,
/// the file, each stream in play in full, and what this TV does with them. Tizen's player says
/// little of itself: the display's mode and HDR output aren't read, and the TV's lines are those
/// the engine gives.
/// </summary>
public static class Readout
{
    public static string HowItPlays => Strings.T("player.readout.howItPlays");
    public static string ThisTv => Strings.T("player.readout.thisTv");
    /// <summary>The Info panel reads its numbers this often.</summary>
    public const long ReadMs = 1_000;

    /// <summary>The Info panel's sections; empty until the server answered (then <see cref="PlayerText.InfoWaiting"/> shows alone).</summary>
    public static IReadOnlyList<MediaFactBlock> Sections(ReadoutInput input)
    {
        var sections = new List<MediaFactBlock>();
        var d = input.Details;
        var stats = input.Stats;
        if (d != null)
        {
            sections.Add(new MediaFactBlock(HowItPlays, Playback(d, input.Quality, stats)));
            if (d.File != null) sections.Add(new MediaFactBlock(Strings.T("player.readout.file"), MediaFacts.File(d.File)));
            if (d.Video != null)
            {
                var facts = MediaFacts.Video(d.Video).ToList();
                if (CurrentBitrate(d, stats) is { } now) facts.Add(new MediaFact(Strings.T("player.readout.currentBitrate"), now));
                sections.Add(new MediaFactBlock(Strings.T("player.readout.video"), facts));
            }
            if (d.Audio != null)
            {
                var facts = MediaFacts.Audio(d.Audio).ToList();
                if (stats?.AudioOutput is { } output) facts.Add(new MediaFact(Strings.T("player.readout.output"), output));
                sections.Add(new MediaFactBlock(Strings.T("player.readout.audio"), facts));
            }
            sections.Add(new MediaFactBlock(Strings.T("player.readout.subtitles"), Subtitles(d)));
        }
        var device = Device(stats, input);
        if (device.Count > 0) sections.Add(new MediaFactBlock(ThisTv, device));
        return sections;
    }

    /// <summary>Playback details over the video (Settings → Playback details): one line each for the way, the streams, and this TV.</summary>
    public static IReadOnlyList<MediaFact> Compact(ReadoutInput input)
    {
        var lines = new List<MediaFact>();
        var d = input.Details;
        var stats = input.Stats;
        if (d != null)
        {
            lines.Add(new MediaFact(Strings.T("player.readout.playback"), d.Method + (d.Pipeline != null ? " · " + d.Pipeline : "")));
            if (d.Video is { } v)
            {
                var codec = ItemFormat.CodecLabel(v.Codec) + (v.Profile != null ? " " + v.Profile : "") + (ItemFormat.LevelLabel(v.Codec, v.Level) is { } level ? " L" + level : "");
                lines.Add(new MediaFact(Strings.T("player.readout.video"), ItemFormat.Join(" · ",
                    codec,
                    v.Width > 0 && v.Height > 0 ? $"{v.Width}×{v.Height}" : null,
                    ItemFormat.FrameRateLabel(v.FrameRate),
                    ItemFormat.BitDepthLabel(v),
                    ItemFormat.HdrFormatLabel(v))));
            }
            if (TranscodeTarget(d) is { } target) lines.Add(new MediaFact(Strings.T("player.readout.convertsTo"), target));
            if (d.Audio is { } a) lines.Add(new MediaFact(Strings.T("player.readout.audio"), ItemFormat.Join(" · ", ItemFormat.AudioFormatLabel(a), ItemFormat.SampleRateLabel(a.SampleRate), MediaFacts.Language(a), stats?.AudioOutput)));
            lines.Add(new MediaFact(Strings.T("player.readout.subtitles"), d.Subtitle is { } s ? ItemFormat.Join(" · ", MediaFacts.Language(s), MediaFacts.SubtitleFormat(s)) : Strings.T("player.readout.off")));
            var bitrates = new[] { ItemFormat.Bitrate(d.File?.Bitrate), CurrentBitrate(d, stats) ?? "" }.Where(b => b.Length > 0).Distinct().ToList();
            if (bitrates.Count > 0) lines.Add(new MediaFact(Strings.T("player.readout.bitrate"), string.Join(" → ", bitrates)));
        }
        if (stats != null)
        {
            if (stats.Decoder != null) lines.Add(new MediaFact(Strings.T("player.readout.decoder"), Decoder(stats.Decoder, stats.Hardware)));
            if (stats.Dropped is long dropped && stats.Frames is long frames) lines.Add(new MediaFact(Strings.T("player.readout.dropped"), DroppedOf(dropped, frames)));
            var buffer = Strings.T("player.readout.ahead", ("seconds", input.BufferedAheadMs / 1000));
            lines.Add(new MediaFact(Strings.T("player.readout.buffer"), buffer + (stats.Bandwidth is long bw ? " · " + ItemFormat.Bitrate(bw) : "")));
        }
        return lines;
    }

    /// <summary>What a remux or transcode makes: "H.264 1080p · AAC 2.0", from the transcoder's numbers; null when nothing is converted.</summary>
    public static string? TranscodeTarget(PlaybackDetails d)
    {
        var t = d.Transcode;
        if (t == null || d.PlaybackMethod == "direct") return null;
        var text = ItemFormat.Join(" · ", t.Video == "transcode" && t.VideoOut != null ? Details.Simplified(t.VideoOut) : null, t.Audio == "transcode" ? t.AudioOut : null);
        return text.Length > 0 ? text : null;
    }

    /// <summary>A MIME type as people know the codec: "video/hevc" is HEVC, "audio/eac3-joc" is E-AC-3 JOC (Atmos).</summary>
    public static string CodecName(string mime) => mime.ToLowerInvariant() switch
    {
        "video/hevc" => "HEVC",
        "video/avc" => "H.264",
        "video/dolby-vision" => "Dolby Vision",
        "video/av01" => "AV1",
        "video/x-vnd.on2.vp9" => "VP9",
        "video/mpeg2" => "MPEG-2",
        "audio/ac3" => "AC-3",
        "audio/eac3" => "E-AC-3",
        "audio/eac3-joc" => "E-AC-3 JOC (Atmos)",
        "audio/true-hd" => "TrueHD",
        "audio/vnd.dts" => "DTS",
        "audio/vnd.dts.hd" => "DTS-HD",
        "audio/mp4a-latm" => "AAC",
        "audio/opus" => "Opus",
        "audio/flac" => "FLAC",
        "audio/raw" => "PCM",
        _ => mime.IndexOf('/') is var slash && slash >= 0 ? mime.Substring(slash + 1) : mime,
    };

    static List<MediaFact> Playback(PlaybackDetails d, string quality, EngineStats? stats)
    {
        var facts = new List<MediaFact> { new MediaFact(Strings.T("player.readout.playback"), d.Method + (d.Pipeline != null ? " · " + d.Pipeline : "")) };
        if (!string.IsNullOrWhiteSpace(d.Why)) facts.Add(new MediaFact(Strings.T("player.readout.why"), d.Why!));
        facts.AddRange(d.Rows.Select(r => new MediaFact(r.Label, r.Value)));
        // Beside the server's speed and the bitrate: a server keeping up whose video stalls is a
        // network that can't carry it.
        if (stats?.Bandwidth is long bandwidth) facts.Add(new MediaFact(SessionText.RowNetwork, ItemFormat.Bitrate(bandwidth)));
        if (!string.IsNullOrWhiteSpace(quality)) facts.Add(new MediaFact(Strings.T("player.readout.quality"), quality));
        return facts;
    }

    static IReadOnlyList<MediaFact> Subtitles(PlaybackDetails d)
    {
        if (d.Subtitle == null) return new[] { new MediaFact(Strings.T("player.readout.track"), Strings.T("player.readout.off")) };
        var shown = d.SubtitleDelivery switch
        {
            SubtitleDelivery.Server => Strings.T("player.readout.fromServerText"),
            SubtitleDelivery.ServerPictures => Strings.T("player.readout.fromServerPictures"),
            SubtitleDelivery.BurnedIn => Strings.T("player.readout.burnedInByServer"),
            _ => null,
        };
        var facts = MediaFacts.Subtitle(d.Subtitle).ToList();
        if (shown != null) facts.Add(new MediaFact(Strings.T("player.readout.shown"), shown));
        return facts;
    }

    static List<MediaFact> Device(EngineStats? stats, ReadoutInput input)
    {
        var facts = new List<MediaFact>();
        if (stats == null) return facts;
        if (stats.Decoder != null) facts.Add(new MediaFact(Strings.T("player.readout.videoDecoder"), Decoder(stats.Decoder, stats.Hardware)));
        var codec = stats.Codec;
        var picture = ItemFormat.Join(" · ",
            stats.Width != null && stats.Height != null ? $"{stats.Width}×{stats.Height}" : null,
            ItemFormat.FrameRateLabel(stats.FrameRate),
            stats.BitDepth is int depth ? Strings.T("player.readout.bitDepth", ("n", depth)) : null,
            stats.Hdr,
            codec != null ? CodecName(codec.Split(' ')[0]) : null);
        if (picture.Length > 0) facts.Add(new MediaFact(Strings.T("player.readout.decoded"), picture));
        // What arrives: "hvc1.2.4.L153.B0", "dvhe.08.06", after the MIME type.
        if (codec != null && codec.IndexOf(' ') is var space && space > 0 && codec.Substring(space + 1).Trim() is { Length: > 0 } codecString) facts.Add(new MediaFact(Strings.T("player.readout.codecString"), codecString));
        if (stats.Dropped is long dropped && stats.Frames is long frames) facts.Add(new MediaFact(Strings.T("player.readout.droppedFrames"), DroppedOf(dropped, frames)));
        facts.Add(new MediaFact(Strings.T("player.readout.bufferedAhead"), Strings.T("player.units.seconds", ("value", input.BufferedAheadMs / 1000))));
        facts.Add(new MediaFact(Strings.T("player.readout.stalls"), input.Stalls.ToString(CultureInfo.InvariantCulture)));
        if (input.FirstFrameMs is long first) facts.Add(new MediaFact(Strings.T("player.readout.firstFrame"), Strings.T("player.units.milliseconds", ("value", first))));
        // The engine's own lines, "Label: value".
        foreach (var line in stats.Extra)
        {
            var at = line.IndexOf(": ", StringComparison.Ordinal);
            facts.Add(at > 0 ? new MediaFact(line.Substring(0, at), line.Substring(at + 2)) : new MediaFact("", line));
        }
        return facts;
    }

    /// <summary>"10 of 1000": dropped of every frame the decoder had.</summary>
    static string DroppedOf(long dropped, long frames) => Strings.T("player.readout.droppedOf", ("dropped", dropped), ("frames", frames));

    /// <summary>The bitrate arriving now: the stream's as the engine measures it, or the transcode's aim; in direct play the file's own.</summary>
    static string? CurrentBitrate(PlaybackDetails d, EngineStats? stats)
    {
        if (stats?.Bitrate is long bitrate) return ItemFormat.Bitrate(bitrate);
        if (d.Transcode?.BitrateOut is long aim) return ItemFormat.Bitrate(aim);
        return d.PlaybackMethod == "direct" ? Strings.T("player.readout.asInFile") : null;
    }

    /// <summary>The decoder, and whether it is the hardware's: "c2.hevc (hardware)"; as it is when that isn't known.</summary>
    static string Decoder(string decoder, bool? hardware) => hardware switch
    {
        true => Strings.T("player.readout.hardware", ("decoder", decoder)),
        false => Strings.T("player.readout.software", ("decoder", decoder)),
        null => decoder,
    };
}
