using System;
using System.Collections.Generic;
using System.Linq;
using Pley.Tv.Core.Api;

namespace Pley.Tv.Core.Ui;

/// <summary>One line of a readout: a muted label and its value.</summary>
public sealed record MediaFact(string Label, string Value);

/// <summary>Facts under a title: "File", "Video", "Audio 2".</summary>
public sealed record MediaFactBlock(string Title, IReadOnlyList<MediaFact> Facts);

/// <summary>
/// Files and streams in full, as the item page's Media info lists them (the Android app's
/// MediaFacts.kt): everything the server's media info says, in <see cref="ItemFormat"/>'s words.
/// Facts that aren't known are left out.
/// </summary>
public static class MediaFacts
{
    static readonly string[] Kinds = { "video", "audio", "subtitle" };

    /// <summary>The blocks of a file in Media info: the file's facts, then each stream's; a file the page has no detail of falls back to its short rows.</summary>
    public static IReadOnlyList<MediaFactBlock> Blocks(MediaFileInfo info)
    {
        if (info.File is { } file) return new[] { new MediaFactBlock("File", File(file)) }.Concat(Streams(file)).ToList();
        var rows = info.Rows.Select(r => new MediaFact(LabelOf(r.Label), r.Value)).ToList();
        var streams = info.Streams.Select(s => new MediaFactBlock(
            ItemFormat.Join(" · ", KindName(s.Kind), s.Language, s.Title),
            new[] { new MediaFact("Codec", s.Codec), new MediaFact("", s.Details) }));
        return new[] { new MediaFactBlock("File", rows) }.Concat(streams).ToList();
    }

    /// <summary>Name, container, size, duration and overall bitrate.</summary>
    public static IReadOnlyList<MediaFact> File(MediaFile f) => Facts(
        ("Name", ItemPages.FileName(f.Path)),
        ("Container", f.Container?.ToUpperInvariant()),
        ("Size", f.Size > 0 ? ItemFormat.Bytes(f.Size) : null),
        ("Duration", f.DurationMs > 0 ? Format.Duration(f.DurationMs) : null),
        ("Overall bitrate", ItemFormat.Bitrate(f.Bitrate)));

    /// <summary>Codec, profile and level, size, frame rate, bit depth, HDR and Dolby Vision, colour, bitrate.</summary>
    public static IReadOnlyList<MediaFact> Video(MediaStream s)
    {
        var rate = ItemFormat.FrameRateLabel(s.FrameRate);
        return Facts(
            ("Codec", ItemFormat.CodecLabel(s.Codec)),
            ("Profile", ItemFormat.ProfileLevelLabel(s)),
            ("Resolution", ItemFormat.DimensionsLabel(s.Width, s.Height)),
            ("Frame rate", rate != null && s.Interlaced == true ? $"{rate}, interlaced" : rate),
            ("Bit depth", ItemFormat.BitDepthLabel(s)),
            ("HDR", ItemFormat.HdrFormatLabel(s)),
            ("Dolby Vision", ItemFormat.DolbyVisionDetails(s)),
            ("Primaries", ItemFormat.ColorPrimariesLabel(s.ColorPrimaries)),
            ("Transfer", ItemFormat.ColorTransferLabel(s.ColorTransfer)),
            ("Matrix", ItemFormat.ColorMatrixLabel(s.ColorSpace)),
            ("Bitrate", ItemFormat.Bitrate(s.Bitrate)),
            ("Title", s.Title));
    }

    /// <summary>Codec and profile, channels, sample rate, language, title, bitrate, default and forced.</summary>
    public static IReadOnlyList<MediaFact> Audio(MediaStream s) => Facts(
        ("Codec", ItemFormat.CodecLabel(s.Codec)),
        ("Profile", s.Profile),
        ("Channels", Channels(s)),
        ("Sample rate", ItemFormat.SampleRateLabel(s.SampleRate)),
        ("Language", Language(s)),
        ("Title", s.Title),
        ("Bitrate", ItemFormat.Bitrate(s.Bitrate)),
        ("Flags", Flags(s)));

    /// <summary>Format (text or picture), language, title, default, forced and beside the media.</summary>
    public static IReadOnlyList<MediaFact> Subtitle(MediaStream s) => Facts(
        ("Format", SubtitleFormat(s)),
        ("Language", Language(s)),
        ("Title", s.Title),
        ("Flags", Flags(s)));

    /// <summary>Every stream of <paramref name="f"/>, each under its kind ("Video", "Audio 2"), in kind then index order.</summary>
    public static IReadOnlyList<MediaFactBlock> Streams(MediaFile f) =>
        f.Streams.Where(s => Array.IndexOf(Kinds, s.Kind) >= 0)
            .OrderBy(s => Array.IndexOf(Kinds, s.Kind)).ThenBy(s => s.Index)
            .GroupBy(s => s.Kind)
            .SelectMany(g =>
            {
                var streams = g.ToList();
                var name = KindName(g.Key);
                return streams.Select((s, i) => new MediaFactBlock(streams.Count > 1 ? $"{name} {i + 1}" : name, Of(s)));
            })
            .ToList();

    /// <summary><see cref="Video"/>, <see cref="Audio"/> or <see cref="Subtitle"/>, by the stream's kind.</summary>
    public static IReadOnlyList<MediaFact> Of(MediaStream s) => s.Kind switch
    {
        "video" => Video(s),
        "audio" => Audio(s),
        _ => Subtitle(s),
    };

    /// <summary>"5.1 (5.1(side))", "Stereo".</summary>
    public static string? Channels(MediaStream s)
    {
        var count = ItemFormat.ChannelsLabel(s.Channels);
        var layout = string.IsNullOrWhiteSpace(s.ChannelLayout) ? null : s.ChannelLayout;
        if (count.Length == 0) return layout;
        if (layout == null || string.Equals(layout, count, StringComparison.OrdinalIgnoreCase)) return count;
        return $"{count} ({layout})";
    }

    /// <summary>"English", else the code as the file has it.</summary>
    public static string? Language(MediaStream s) => ItemFormat.LanguageName(s.Language) ?? (string.IsNullOrWhiteSpace(s.Language) ? null : s.Language);

    /// <summary>"SRT, text" or "PGS, picture".</summary>
    public static string SubtitleFormat(MediaStream s) =>
        ItemFormat.Join(", ", ItemFormat.CodecLabel(s.Codec), s.TextBased switch
        {
            true => "text",
            false => "picture",
            null => null,
        });

    static string Flags(MediaStream s) => ItemFormat.Join(", ", s.Default ? "Default" : null, s.Forced ? "Forced" : null, s.External == true ? "Beside the media" : null);

    static string KindName(string kind) => kind switch
    {
        "video" => "Video",
        "audio" => "Audio",
        _ => "Subtitles",
    };

    static string LabelOf(InfoLabel label) => label switch
    {
        InfoLabel.File => "Name",
        InfoLabel.Container => "Container",
        InfoLabel.Size => "Size",
        InfoLabel.Duration => "Duration",
        InfoLabel.Bitrate => "Overall bitrate",
        InfoLabel.Video => "Video",
        _ => "Audio",
    };

    /// <summary>The lines whose value is known and not empty.</summary>
    static IReadOnlyList<MediaFact> Facts(params (string Label, string? Value)[] lines) =>
        lines.Where(l => !string.IsNullOrWhiteSpace(l.Value)).Select(l => new MediaFact(l.Label, l.Value!)).ToList();
}
