using System;
using System.Collections.Generic;
using System.Linq;
using Pley.Tv.Core.Api;
using Pley.Tv.Core.Ui;

namespace Pley.Tv.Core.Player;

/// <summary>
/// Skip Intro and Skip Credits, as the web does them (apps/web/src/player/SkipButton.tsx; the
/// Android app's Markers). Nothing is skipped by itself: the button only offers it.
/// </summary>
public static class Markers
{
    /// <summary>The button goes this long before the segment ends.</summary>
    public const long TailMs = 1_000;

    /// <summary>Skip Intro lands this long before the intro ends, so its last moment plays.</summary>
    public const long IntroLeadMs = 750;

    /// <summary>The intro and credits of the file playing (the item's first file when it isn't known yet).</summary>
    public static IReadOnlyList<Marker> ForFile(ItemDetail item, long? fileId)
    {
        var id = fileId ?? item.Files.FirstOrDefault()?.Id;
        if (id == null) return new Marker[0];
        return (item.Markers ?? new Marker[0]).Where(m => m.FileId == id && m.Kind is "intro" or "credits").ToList();
    }

    /// <summary>The marker the playhead is in: from its start until a second before its end.</summary>
    public static Marker? Active(IReadOnlyList<Marker> markers, long positionMs) =>
        markers.FirstOrDefault(m => positionMs >= m.StartMs && positionMs < m.EndMs - TailMs);

    /// <summary>Where skipping <paramref name="marker"/> lands: just before an intro ends, at the end of credits.</summary>
    public static long SkipTarget(Marker marker)
    {
        var lead = marker.Kind == "intro" ? Math.Min(IntroLeadMs, (marker.EndMs - marker.StartMs) / 2) : 0;
        return marker.EndMs - lead;
    }

    /// <summary>"Skip Intro" or "Skip Credits".</summary>
    public static string SkipLabel(Marker marker) => marker.Kind == "intro" ? SessionText.SkipIntro : SessionText.SkipCredits;

    /// <summary>"Intro" or "Credits", under the scrubber's span of the marker playing.</summary>
    public static string Name(Marker marker) => marker.Kind == "intro" ? PlayerText.Intro : PlayerText.Credits;

    /// <summary>Identifies a segment, to tell entering it again apart from entering another.</summary>
    public static string KeyOf(Marker marker) => $"{marker.FileId}:{marker.Kind}:{marker.StartMs}";
}

/// <summary>
/// Entering credits brings up Up Next, once per segment: seeking back into them later lets them
/// play (the Android app's CreditsGate).
/// </summary>
public sealed class CreditsGate
{
    readonly HashSet<string> announced = new HashSet<string>();

    /// <summary>The credits <paramref name="marker"/> the playhead just entered for the first time; null otherwise.</summary>
    public Marker? Entered(Marker? marker)
    {
        if (marker == null || marker.Kind != "credits") return null;
        return announced.Add(Markers.KeyOf(marker)) ? marker : null;
    }

    /// <summary>A new item: every segment may be announced again.</summary>
    public void Reset() => announced.Clear();
}

/// <summary>
/// What plays after an item (the Android app's NextUp; apps/web/src/lib/videoQueue.ts): the
/// queue's next video when playing through a playlist or collection, else the next episode;
/// movies and extras have none.
/// </summary>
public static class NextUp
{
    /// <summary>Up Next plays by itself after this many seconds.</summary>
    public const int CountdownSeconds = 10;

    /// <summary>The width Up Next's still is asked for at.</summary>
    public const int ImageWidth = 480;

    /// <summary>Whether <paramref name="kind"/> played from <paramref name="queue"/> can have something next, before asking.</summary>
    public static bool HasNext(string kind, PlayQueue? queue) => queue != null || kind == "episode";

    /// <summary>The first video after <paramref name="itemId"/> in <paramref name="items"/>; null when it is the last, or isn't there.</summary>
    public static ItemSummary? NextIn(IReadOnlyList<ItemSummary> items, long itemId)
    {
        var videos = items.Where(i => Format.IsPlayableVideo(i.Kind)).ToList();
        var at = videos.FindIndex(i => i.Id == itemId);
        return at < 0 || at + 1 >= videos.Count ? null : videos[at + 1];
    }

    /// <summary>Where <paramref name="next"/> starts: from the start when watched, else its resume point.</summary>
    public static long StartOf(ItemSummary next) => next.UserState.Watched ? 0 : next.UserState.PositionMs;

    /// <summary>"S1 E2" for an episode, null otherwise.</summary>
    public static string? LabelOf(ItemSummary next) =>
        next.Kind == "episode" && Format.EpisodeLabel(next) is { Length: > 0 } label ? label : null;
}

/// <summary>What plays next, during its card (the Android app's UpNextUi).</summary>
/// <param name="Heading">"Up next · S1 E2", or "Up next".</param>
/// <param name="ImageUrl">Absolute address of its still (16:9), asked for at <see cref="NextUp.ImageWidth"/>.</param>
/// <param name="SecondsLeft">Seconds before it plays by itself; null when it won't (autoplay off, or the sleep timer holds it).</param>
public sealed record UpNextCard(long ItemId, string Title, string Heading, string? ImageUrl, int? SecondsLeft);
