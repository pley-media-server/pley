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

    /// <summary>Skip shows on its own this long from when it appears; then only with the controls.</summary>
    public const long SkipShownMs = 8_000;

    /// <summary>Credits ending this close to the end of their file run to it: only a studio's logo or black follows (the server's CREDITS_TAIL_MS).</summary>
    public const long CreditsTailMs = 15_000;

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

    /// <summary>Whether Skip shows, the playhead in a marker: on its own while its window lasts, else with the controls.</summary>
    public static bool SkipShows(bool inMarker, bool alone, bool controls) => inMarker && (alone || controls);

    /// <summary>
    /// Whether <paramref name="marker"/> is credits that run to the end of its file,
    /// <paramref name="durationMs"/> long (0 or null when unknown). Only those bring Up Next: its
    /// countdown would skip what follows other credits (an after-credits scene, a show's last
    /// minutes after music every episode shares), which bring Skip Credits alone.
    /// packages/tv/test/fixtures/markers.json pins it for both Samsung apps.
    /// </summary>
    public static bool ReachesEnd(Marker marker, long? durationMs) =>
        marker.Kind == "credits" && durationMs > 0 && durationMs - marker.EndMs <= CreditsTailMs;
}

/// <summary>
/// Entering credits that run to the end brings up Up Next, once per segment: seeking back into
/// them later lets them play (the Android app's CreditsGate).
/// </summary>
public sealed class CreditsGate
{
    readonly HashSet<string> announced = new HashSet<string>();

    /// <summary>
    /// The credits <paramref name="marker"/> the playhead just entered for the first time, in a file
    /// <paramref name="durationMs"/> long, when they run to its end; null otherwise.
    /// </summary>
    public Marker? Entered(Marker? marker, long? durationMs)
    {
        if (marker == null || !Markers.ReachesEnd(marker, durationMs)) return null;
        return announced.Add(Markers.KeyOf(marker)) ? marker : null;
    }

    /// <summary>A new item: every segment may be announced again.</summary>
    public void Reset() => announced.Clear();
}

/// <summary>
/// When Skip shows on its own: for <see cref="Markers.SkipShownMs"/> of the wall clock from the
/// playhead entering a marker (playing, seeking or starting into it), paused or not; after that only
/// with the controls, so it doesn't sit over the picture for a whole intro or the credits. Entering
/// a marker again, or another, starts afresh; a seek within the one it is in doesn't.
/// packages/tv/test/fixtures/skip-shown.json pins it for both Samsung apps.
/// </summary>
public sealed class SkipWindow
{
    string? key;
    long since;

    /// <summary>The marker the playhead is in at <paramref name="nowMs"/> (null for none): whether Skip shows on its own.</summary>
    public bool At(Marker? marker, long nowMs)
    {
        var now = marker != null ? Markers.KeyOf(marker) : null;
        if (now != key)
        {
            key = now;
            since = nowMs;
        }
        return key != null && nowMs - since < Markers.SkipShownMs;
    }

    /// <summary>Skip appears again in the marker the playhead is in (Up Next cancelled over its credits): a fresh window.</summary>
    public void Again(long nowMs) => since = nowMs;

    /// <summary>A new item: the next marker starts afresh, even the same one.</summary>
    public void Reset() => key = null;
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

/// <summary>
/// How much of Up Next's countdown has gone, from 0 to 1, for its ring (the Android app's
/// countdownFill). The model counts whole seconds; through each one the fill glides, linearly, to
/// where it stands at that second's end, so the ring moves without steps and is full as the
/// countdown reaches 0. A countdown that starts again jumps back at once rather than sliding back.
/// </summary>
public sealed class CountdownFill
{
    /// <summary>How long each glide takes: one second of the countdown.</summary>
    public const int SecondMs = 1000;

    float from, to;
    long startedMs;

    /// <summary>The countdown shows <paramref name="secondsLeft"/> at <paramref name="nowMs"/>.</summary>
    public void Set(int secondsLeft, long nowMs)
    {
        const int total = NextUp.CountdownSeconds;
        var start = Unit((total - secondsLeft) / (float)total);
        var end = Unit((total - secondsLeft + 1) / (float)total);
        var current = At(nowMs);
        // Ahead of this second (a countdown started again) or far behind it: from its place.
        if (current > end || current < start - 0.5f / total) current = start;
        from = current;
        to = end;
        startedMs = nowMs;
    }

    /// <summary>Back to empty, for a countdown that stopped.</summary>
    public void Reset() => from = to = 0;

    /// <summary>The fill at <paramref name="nowMs"/>.</summary>
    public float At(long nowMs) => from + (to - from) * Unit((nowMs - startedMs) / (float)SecondMs);

    /// <summary>Whether it still moves at <paramref name="nowMs"/>; once it rests, nothing needs drawing again.</summary>
    public bool Moving(long nowMs) => from != to && nowMs - startedMs < SecondMs;

    static float Unit(float value) => Math.Max(0f, Math.Min(1f, value));
}

/// <summary>What plays next, during its card (the Android app's UpNextUi).</summary>
/// <param name="Heading">"Up next · S1 E2", or "Up next".</param>
/// <param name="SecondsLeft">Seconds before it plays by itself; null when it won't (autoplay off, or the sleep timer holds it).</param>
public sealed record UpNextCard(long ItemId, string Title, string Heading, int? SecondsLeft);
