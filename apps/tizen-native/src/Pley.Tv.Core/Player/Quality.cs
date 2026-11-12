using System;
using System.Collections.Generic;
using System.Linq;
using Pley.Tv.Core.Api;

namespace Pley.Tv.Core.Player;

/// <summary>
/// A quality the player offers (docs/QUALITY.md; the Android app's QualityPreset): the original,
/// Automatic, or a transcode capped at <see cref="MaxBitrateKbps"/> and <see cref="MaxHeight"/>.
/// <see cref="Id"/> is what the server's <c>auto_quality</c> and the settings use ("8m").
/// Automatic is a choice, never what plays: it plays the original or one of the transcodes.
/// </summary>
public sealed record QualityPreset(string Id, int? MaxBitrateKbps, int? MaxHeight)
{
    public const string OriginalId = "original";
    public const string AutomaticId = "auto";

    public bool Original => Id == OriginalId;

    /// <summary>Automatic: the original, lower only when it doesn't play well.</summary>
    public bool Automatic => Id == AutomaticId;

    /// <summary>How it is asked for: the original lets the server choose (direct play when it can), every other quality is a transcode.</summary>
    public string Mode => Original || Automatic ? "auto" : "transcode";
}

/// <summary>The menu's qualities, in its order, and which of them a file is offered (the Android app's Qualities).</summary>
public static class Qualities
{
    public static readonly QualityPreset Original = new QualityPreset(QualityPreset.OriginalId, null, null);
    public static readonly QualityPreset Automatic = new QualityPreset(QualityPreset.AutomaticId, null, null);

    /// <summary>The transcodes, in the menu's order: the two 4K ones, then 1080p and below.</summary>
    public static readonly IReadOnlyList<QualityPreset> Transcodes = new[]
    {
        new QualityPreset("30m", 30_000, 2160),
        new QualityPreset("15m", 15_000, 2160),
        new QualityPreset("20m", 20_000, 1080),
        new QualityPreset("12m", 12_000, 1080),
        new QualityPreset("8m", 8_000, 1080),
        new QualityPreset("4m", 4_000, 720),
        new QualityPreset("2m", 2_000, 480),
    };

    /// <summary>The menu, as on every app: Original, Automatic, then the transcodes.</summary>
    public static readonly IReadOnlyList<QualityPreset> All = new[] { Original, Automatic }.Concat(Transcodes).ToList();

    /// <summary>A quality is offered for a file's bitrate when it is at most this share of it.</summary>
    public const double Lighter = 0.8;

    /// <summary>The lowest quality Automatic plays.</summary>
    public static QualityPreset Lowest => Transcodes[Transcodes.Count - 1];

    /// <summary>The preset with <paramref name="id"/>, else the original.</summary>
    public static QualityPreset ById(string? id) => All.FirstOrDefault(q => q.Id == id) ?? Original;

    /// <summary>What the settings keep for <paramref name="preset"/>: "Original", or its id ("auto", "8m").</summary>
    public static string IdOf(QualityPreset preset) => preset.Original ? "Original" : preset.Id;

    /// <summary>A setting's value: a preset's id ("8m", "auto"), or "Original" as the settings say it.</summary>
    public static QualityPreset ByIdOrLabel(string? value) =>
        value == null ? Original : All.FirstOrDefault(q => string.Equals(q.Id, value, StringComparison.OrdinalIgnoreCase)) ?? Original;

    /// <summary>
    /// The qualities worth offering for <paramref name="file"/> (docs/QUALITY.md): the original and
    /// Automatic, then those that make it smaller, with fewer lines or at most 80 % of its bitrate (a
    /// copy a little lighter plays no better). Two that come to the same bitrate and lines for it
    /// show once, and they go by the lines they give the file, then bitrate. Every one when its
    /// bitrate and size are unknown.
    /// </summary>
    public static IReadOnlyList<QualityPreset> ForFile(MediaFile? file)
    {
        double? kbps = file?.Bitrate > 0 ? file.Bitrate.Value / 1000.0 : null;
        var lines = LinesClassOf(file);
        if (kbps == null && lines == null) return All;
        var seen = new HashSet<(int, int?)>();
        var smaller = Transcodes.Where(q =>
            ((kbps != null && (q.MaxBitrateKbps ?? 0) <= kbps * Lighter) || (lines != null && (q.MaxHeight ?? 0) < lines))
            && seen.Add((q.MaxBitrateKbps ?? 0, LinesFor(q, lines)))).ToList();
        // By the lines they give the file, then bitrate: a 4K file keeps the table's order.
        var sorted = smaller.OrderByDescending(q => LinesFor(q, lines) ?? 0).ThenByDescending(q => q.MaxBitrateKbps ?? 0);
        return new[] { Original, Automatic }.Concat(sorted).ToList();
    }

    /// <summary>
    /// The qualities the menu offers for <paramref name="file"/>: those worth offering
    /// (<see cref="ForFile"/>), or the original alone on a server that converts no video
    /// (<see cref="PlaybackInfo.OriginalOnly"/>), which has no other to give.
    /// </summary>
    public static IReadOnlyList<QualityPreset> Offered(MediaFile? file, bool originalOnly) =>
        originalOnly ? new[] { Original } : ForFile(file);

    /// <summary>The lines <paramref name="preset"/> gives a file of <paramref name="fileLines"/>: never more than the file's.</summary>
    public static int? LinesFor(QualityPreset preset, int? fileLines)
    {
        if (preset.MaxHeight is not int most) return fileLines;
        return fileLines != null && fileLines < most ? fileLines : most;
    }

    /// <summary>
    /// The quality chosen for <paramref name="file"/>: <paramref name="remembered"/> when the file is
    /// offered it, Automatic when nothing was ever chosen, else the original (an 8 Mbps cap on a 5
    /// Mbps file plays the original, as on the web).
    /// </summary>
    public static QualityPreset Choose(MediaFile? file, string? remembered)
    {
        if (string.IsNullOrWhiteSpace(remembered)) return Automatic;
        var wanted = ByIdOrLabel(remembered);
        return ForFile(file).FirstOrDefault(q => q.Id == wanted.Id) ?? Original;
    }

    /// <summary>The file's bitrate in kbps; null when unknown.</summary>
    public static int? FileKbps(MediaFile? file) => file?.Bitrate > 0 ? (int)(file.Bitrate.Value / 1000) : null;

    /// <summary>The file's lines; null when unknown.</summary>
    public static int? LinesOf(MediaFile? file) =>
        file?.Height > 0 ? file.Height : file?.Streams.FirstOrDefault(s => s.Kind == "video")?.Height is int h && h > 0 ? h : null;

    /// <summary>
    /// The file's lines as qualities name them (2160, 1080, 720 or 480), by its width or height as
    /// the server tells them apart: a 1920×800 film is 1080p. Null when neither is known.
    /// </summary>
    public static int? LinesClassOf(MediaFile? file)
    {
        var video = file?.Streams.FirstOrDefault(s => s.Kind == "video");
        var w = file?.Width ?? video?.Width ?? 0;
        var h = LinesOf(file) ?? 0;
        if (w <= 0 && h <= 0) return null;
        if (w >= 3800 || h >= 2100) return 2160;
        if (w >= 1800 || h >= 1000) return 1080;
        if (w >= 1200 || h >= 700) return 720;
        return 480;
    }

    /// <summary>
    /// What the server converts <paramref name="file"/> at when it converts the original
    /// (apps/server/src/playback.rs, closest_quality): its own size's quality, with the highest
    /// bitrate up to 1.5 times the file's, or that size's lowest.
    /// </summary>
    public static QualityPreset Closest(MediaFile? file)
    {
        var lines = LinesClassOf(file) ?? 1080;
        double? cap = file?.Bitrate > 0 ? file.Bitrate.Value * 1.5 / 1000.0 : null;
        var tier = Transcodes.Where(q => q.MaxHeight == lines).OrderByDescending(q => q.MaxBitrateKbps).ToList();
        return tier.FirstOrDefault(q => cap == null || (q.MaxBitrateKbps ?? 0) <= cap) ?? tier.LastOrDefault() ?? ById("8m");
    }

    /// <summary>"1080p · 8.2 Mbps": what a file is, for the Original row's hint; null when nothing is known.</summary>
    public static string? FileHint(MediaFile? file)
    {
        if (file == null) return null;
        var text = Ui.ItemFormat.Join(" · ", Ui.ItemFormat.ResolutionLabel(file.Width, file.Height), Ui.ItemFormat.Bitrate(file.Bitrate));
        return text.Length > 0 ? text : null;
    }
}
