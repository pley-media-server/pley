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
        var fileTitle = Strings.T("item.mediaInfo.file");
        if (info.File is { } file) return new[] { new MediaFactBlock(fileTitle, File(file)) }.Concat(Streams(file)).ToList();
        var rows = info.Rows.Select(r => new MediaFact(LabelOf(r.Label), r.Value)).ToList();
        var streams = info.Streams.Select(s => new MediaFactBlock(
            ItemFormat.Join(" · ", KindName(s.Kind), s.Language, s.Title),
            new[] { new MediaFact(Strings.T("item.mediaInfo.codec"), s.Codec), new MediaFact("", s.Details) }));
        return new[] { new MediaFactBlock(fileTitle, rows) }.Concat(streams).ToList();
    }

    /// <summary>Name, container, size, duration and overall bitrate.</summary>
    public static IReadOnlyList<MediaFact> File(MediaFile f) => Facts(
        (Strings.T("item.mediaInfo.name"), ItemPages.FileName(f.Path)),
        (Strings.T("item.mediaInfo.container"), f.Container?.ToUpperInvariant()),
        (Strings.T("item.mediaInfo.size"), f.Size > 0 ? ItemFormat.Bytes(f.Size) : null),
        (Strings.T("item.mediaInfo.duration"), f.DurationMs > 0 ? Format.Duration(f.DurationMs) : null),
        (Strings.T("item.mediaInfo.overallBitrate"), ItemFormat.Bitrate(f.Bitrate)));

    /// <summary>Codec, profile and level, size, frame rate, bit depth, HDR and Dolby Vision, colour, bitrate.</summary>
    public static IReadOnlyList<MediaFact> Video(MediaStream s)
    {
        var rate = ItemFormat.FrameRateLabel(s.FrameRate);
        return Facts(
            (Strings.T("item.mediaInfo.codec"), ItemFormat.CodecLabel(s.Codec)),
            (Strings.T("item.mediaInfo.profile"), ItemFormat.ProfileLevelLabel(s)),
            (Strings.T("item.mediaInfo.resolution"), ItemFormat.DimensionsLabel(s.Width, s.Height)),
            (Strings.T("item.mediaInfo.frameRate"), rate != null && s.Interlaced == true ? Strings.T("item.mediaInfo.interlaced", ("rate", rate)) : rate),
            (Strings.T("item.mediaInfo.bitDepth"), ItemFormat.BitDepthLabel(s)),
            ("HDR", ItemFormat.HdrFormatLabel(s)),
            ("Dolby Vision", ItemFormat.DolbyVisionDetails(s)),
            (Strings.T("item.mediaInfo.primaries"), ItemFormat.ColorPrimariesLabel(s.ColorPrimaries)),
            (Strings.T("item.mediaInfo.transfer"), ItemFormat.ColorTransferLabel(s.ColorTransfer)),
            (Strings.T("item.mediaInfo.matrix"), ItemFormat.ColorMatrixLabel(s.ColorSpace)),
            (Strings.T("item.mediaInfo.bitrate"), ItemFormat.Bitrate(s.Bitrate)),
            (Strings.T("item.mediaInfo.title"), s.Title));
    }

    /// <summary>Codec and profile, channels, sample rate, language, title, bitrate, default and forced.</summary>
    public static IReadOnlyList<MediaFact> Audio(MediaStream s) => Facts(
        (Strings.T("item.mediaInfo.codec"), ItemFormat.CodecLabel(s.Codec)),
        (Strings.T("item.mediaInfo.profile"), s.Profile),
        (Strings.T("item.mediaInfo.channels"), Channels(s)),
        (Strings.T("item.mediaInfo.sampleRate"), ItemFormat.SampleRateLabel(s.SampleRate)),
        (Strings.T("item.mediaInfo.language"), Language(s)),
        (Strings.T("item.mediaInfo.title"), s.Title),
        (Strings.T("item.mediaInfo.bitrate"), ItemFormat.Bitrate(s.Bitrate)),
        (Strings.T("item.mediaInfo.flags"), Flags(s)));

    /// <summary>Format (text or picture), language, title, default, forced and beside the media.</summary>
    public static IReadOnlyList<MediaFact> Subtitle(MediaStream s) => Facts(
        (Strings.T("item.mediaInfo.format"), SubtitleFormat(s)),
        (Strings.T("item.mediaInfo.language"), Language(s)),
        (Strings.T("item.mediaInfo.title"), s.Title),
        (Strings.T("item.mediaInfo.flags"), Flags(s)));

    /// <summary>Every stream of <paramref name="f"/>, each under its kind ("Video", "Audio 2"), in kind then index order.</summary>
    public static IReadOnlyList<MediaFactBlock> Streams(MediaFile f) =>
        f.Streams.Where(s => Array.IndexOf(Kinds, s.Kind) >= 0)
            .OrderBy(s => Array.IndexOf(Kinds, s.Kind)).ThenBy(s => s.Index)
            .GroupBy(s => s.Kind)
            .SelectMany(g =>
            {
                var streams = g.ToList();
                var name = KindName(g.Key);
                return streams.Select((s, i) => new MediaFactBlock(streams.Count > 1 ? Strings.T("item.mediaInfo.numbered", ("kind", name), ("n", i + 1)) : name, Of(s)));
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
        if (layout == null || SameLayout(layout, s.Channels, count)) return count;
        return $"{count} ({layout})";
    }

    /// <summary>
    /// Whether ffprobe's <paramref name="layout"/> only says what the count's label does: "5.1" for
    /// 5.1, and "mono" and "stereo", which are its words whatever the language shown.
    /// </summary>
    static bool SameLayout(string layout, int? channels, string count) =>
        string.Equals(layout, count, StringComparison.OrdinalIgnoreCase)
        || (channels == 1 && string.Equals(layout, "mono", StringComparison.OrdinalIgnoreCase))
        || (channels == 2 && string.Equals(layout, "stereo", StringComparison.OrdinalIgnoreCase));

    /// <summary>"English", else the code as the file has it.</summary>
    public static string? Language(MediaStream s) => ItemFormat.LanguageName(s.Language) ?? (string.IsNullOrWhiteSpace(s.Language) ? null : s.Language);

    /// <summary>"SRT, text" or "PGS, picture".</summary>
    public static string SubtitleFormat(MediaStream s) =>
        ItemFormat.Join(", ", ItemFormat.CodecLabel(s.Codec), s.TextBased switch
        {
            true => Strings.T("item.mediaInfo.text"),
            false => Strings.T("item.mediaInfo.picture"),
            null => null,
        });

    static string Flags(MediaStream s) => ItemFormat.Join(", ",
        s.Default ? Strings.T("item.format.default") : null,
        s.Forced ? Strings.T("item.format.forced") : null,
        s.External == true ? Strings.T("item.mediaInfo.besideTheMedia") : null);

    static string KindName(string kind) => kind switch
    {
        "video" => Strings.T("item.mediaInfo.video"),
        "audio" => Strings.T("item.mediaInfo.audio"),
        _ => Strings.T("item.mediaInfo.subtitles"),
    };

    static string LabelOf(InfoLabel label) => label switch
    {
        InfoLabel.File => Strings.T("item.mediaInfo.name"),
        InfoLabel.Container => Strings.T("item.mediaInfo.container"),
        InfoLabel.Size => Strings.T("item.mediaInfo.size"),
        InfoLabel.Duration => Strings.T("item.mediaInfo.duration"),
        InfoLabel.Bitrate => Strings.T("item.mediaInfo.overallBitrate"),
        InfoLabel.Video => Strings.T("item.mediaInfo.video"),
        _ => Strings.T("item.mediaInfo.audio"),
    };

    /// <summary>The lines whose value is known and not empty.</summary>
    static IReadOnlyList<MediaFact> Facts(params (string Label, string? Value)[] lines) =>
        lines.Where(l => !string.IsNullOrWhiteSpace(l.Value)).Select(l => new MediaFact(l.Label, l.Value!)).ToList();
}
