using System;
using System.Globalization;
using System.Text.RegularExpressions;

namespace Pley.Tv.Core.Player;

/// <summary>
/// How the player writes times and titles (the Android app's TvPlayerText), and the TV player's
/// words (strings_tv_player.xml, strings_tv_player_extras.xml), word for word.
/// </summary>
public static class PlayerText
{
    // The control row.
    public const string Play = "Play";
    public const string Pause = "Pause";
    public const string Back10 = "Back 10 seconds";
    public const string Forward10 = "Forward 10 seconds";
    public const string Previous = "Previous";
    public const string Next = "Next";
    public const string Subtitles = "Subtitles";
    public const string Audio = "Audio";
    public const string Quality = "Quality";
    public const string Chapters = "Chapters";
    public const string Info = "Info";
    public const string Sleep = "Sleep timer";
    public const string Speed = "Speed";

    // The scrubber and Up Next.
    public const string Intro = "Intro";
    public const string Credits = "Credits";
    public const string PlayNow = "Play now";
    public const string Cancel = "Cancel";

    // Panels.
    public const string SubtitlesOffToast = "Subtitles off";
    public const string PlayOriginal = "Play original";
    public const string PlayOriginalHint = "Try the original again";
    public const string InfoTitle = "Playback details";
    public const string InfoWaiting = "Asking the server…";
    public const string Sync = "Sync";
    public const string SyncHint = "OK resets";
    public const string AudioDelay = "Audio delay";

    // Buffering and errors.
    public const string Buffering = "Loading";
    public const string Reconnecting = "Reconnecting…";
    public const string ErrorTitle = "Can't play this item";
    public const string OfflineTitle = "Can't reach your server";
    public const string StoppedTitle = "Playback stopped";
    public const string TryAgain = "Try again";
    public const string BackLabel = "Back";

    /// <summary>Toasts show this long (design spec §2.10); errors longer; the CAPTIONS key's shorter.</summary>
    public const long ToastMs = 3_500, ErrorToastMs = 6_000, CaptionsToastMs = 2_000;

    /// <summary>The controls hide after this long without a key, unless paused or in a panel.</summary>
    public const long HideAfterMs = 5_000;

    /// <summary>How long a playback the server stopped shows why, before the player leaves by itself.</summary>
    public const long StoppedLeaveMs = 20_000;

    /// <summary>Next goes to a chapter starting at least this far ahead.</summary>
    public const long ChapterSlackMs = 1_000;

    /// <summary>Previous restarts the chapter playing when it is this far in, else goes to the one before.</summary>
    public const long RestartChapterMs = 3_000;

    /// <summary>"Ends at 10:42 PM": the time is the screen's to write, as the TV writes times.</summary>
    public static string EndsAt(string time) => $"Ends at {time}";

    /// <summary>"Next in 7", under Play now while Up Next counts down.</summary>
    public static string NextIn(int seconds) => $"Next in {seconds.ToString(CultureInfo.InvariantCulture)}";

    /// <summary>"Subtitles: English (SRT)", when the CAPTIONS key turns them on.</summary>
    public static string SubtitlesToast(string label) => $"Subtitles: {label}";

    /// <summary>A transcode the player chose, toasted: the session's note, then where the way back is.</summary>
    public static string TranscodeToast(string note) => $"{note} Play original is in Quality.";

    /// <summary>A server that can't convert fast enough, toasted (as an error).</summary>
    public static string SlowServerToast(string note) => $"{note} Choose one in Quality.";

    /// <summary>"32 minutes 40 seconds of 1 hour 2 minutes": the scrubber, as a screen reader says it.</summary>
    public static string Position(long elapsedMs, long durationMs) => $"{Spoken(elapsedMs)} of {Spoken(durationMs)}";

    /// <summary>How screen readers say a time: "32 minutes 40 seconds", "1 hour 2 minutes".</summary>
    public static string Spoken(long ms)
    {
        var total = Math.Max(0, ms) / 1000;
        var h = total / 3600;
        var m = total % 3600 / 60;
        var s = total % 60;
        var parts = new System.Collections.Generic.List<string>();
        if (h > 0) parts.Add(h == 1 ? "1 hour" : $"{h} hours");
        if (m > 0) parts.Add(m == 1 ? "1 minute" : $"{m} minutes");
        if (s > 0 || parts.Count == 0) parts.Add(s == 1 ? "1 second" : $"{s} seconds");
        return string.Join(" ", parts);
    }

    /// <summary>The chapter playing at <paramref name="positionMs"/>; null outside them all.</summary>
    public static int? ChapterAt(System.Collections.Generic.IReadOnlyList<ChapterMark> chapters, long positionMs)
    {
        for (var i = chapters.Count - 1; i >= 0; i--)
        {
            if (positionMs < chapters[i].StartMs) continue;
            return positionMs < Math.Max(chapters[i].EndMs, chapters[i].StartMs + 1) ? i : null;
        }
        return null;
    }

    /// <summary>A chapter's name in the Chapters panel: its title, else "Chapter 3".</summary>
    public static string ChapterTitle(ChapterMark chapter, int index) =>
        NameOf(chapter) ?? $"Chapter {(index + 1).ToString(CultureInfo.InvariantCulture)}";

    /// <summary>The seek preview's chapter line: "Chapter 3", a title that names itself so, or "Chapter 3 · The Heist".</summary>
    public static string ChapterLine(ChapterMark chapter, int index)
    {
        var number = (index + 1).ToString(CultureInfo.InvariantCulture);
        var name = NameOf(chapter);
        if (name == null) return $"Chapter {number}";
        return name.StartsWith("Chapter", StringComparison.OrdinalIgnoreCase) ? name : $"Chapter {number} · {name}";
    }

    /// <summary>
    /// A chapter's own name: its title, unless it has none or the title is only where it starts
    /// ("00:06:19.212", as mkvmerge names chapters), which says nothing a viewer can use.
    /// </summary>
    static string? NameOf(ChapterMark chapter) =>
        string.IsNullOrWhiteSpace(chapter.Title) || TimeName.IsMatch(chapter.Title!.Trim()) ? null : chapter.Title;

    static readonly Regex TimeName = new Regex(@"^\d{1,2}(:\d{2}){1,2}([.,]\d+)?$", RegexOptions.CultureInvariant);

    /// <summary>
    /// Where Next goes (the Android app's TvPlayerScreen): an episode plays what comes next (null
    /// here: <see cref="PlayerModel.PlayNext"/>); else the first chapter starting more than a
    /// second past <paramref name="positionMs"/>; null when there is none.
    /// </summary>
    public static long? NextChapterStart(System.Collections.Generic.IReadOnlyList<ChapterMark> chapters, long positionMs)
    {
        foreach (var c in chapters)
            if (c.StartMs > positionMs + ChapterSlackMs) return c.StartMs;
        return null;
    }

    /// <summary>
    /// Where Previous goes: the start of the chapter playing when it is more than 3 s in (or it is
    /// the first), else the chapter before's; the start of the video outside the chapters.
    /// </summary>
    public static long PreviousTarget(System.Collections.Generic.IReadOnlyList<ChapterMark> chapters, long positionMs)
    {
        if (ChapterAt(chapters, positionMs) is not int current) return 0;
        return positionMs - chapters[current].StartMs > RestartChapterMs || current == 0 ? chapters[current].StartMs : chapters[current - 1].StartMs;
    }

    /// <summary>"4:05", "32:40", "1:02:03".</summary>
    public static string Time(long ms)
    {
        var total = Math.Max(0, ms) / 1000;
        var h = total / 3600;
        var m = total % 3600 / 60;
        var s = total % 60;
        return h > 0
            ? string.Format(CultureInfo.InvariantCulture, "{0}:{1:00}:{2:00}", h, m, s)
            : string.Format(CultureInfo.InvariantCulture, "{0}:{1:00}", m, s);
    }

    /// <summary>What is left, with a real minus: "−25:20".</summary>
    public static string Remaining(long positionMs, long durationMs) => "−" + Time(Math.Max(0, durationMs - positionMs));

    static readonly Regex EpisodeNumber = new Regex(@"\bS(\d+) E(\d+)\b");

    /// <summary>An episode's line as the player shows it: "S1 E2 — Pilot" becomes "S1 · E2 · Pilot".</summary>
    public static string? EpisodeLine(string? subtitle)
    {
        if (string.IsNullOrWhiteSpace(subtitle)) return null;
        return EpisodeNumber.Replace(subtitle, "S$1 · E$2").Replace(" — ", " · ");
    }

    /// <summary>The player's title: an episode's show, a film with its year ("Heat (1995)").</summary>
    public static string Title(Api.ItemSummary item) =>
        item.Kind == "episode" ? item.GrandparentTitle ?? item.Title
        : item.Year != null ? $"{item.Title} ({item.Year.Value.ToString(CultureInfo.InvariantCulture)})"
        : item.Title;

    /// <summary>The line under an episode's title: "S2 · E5 · The Long Night"; null for anything else.</summary>
    public static string? Line(Api.ItemSummary item)
    {
        if (item.Kind != "episode") return null;
        var parts = new System.Collections.Generic.List<string>();
        if (item.ParentIndex != null) parts.Add("S" + item.ParentIndex.Value.ToString(CultureInfo.InvariantCulture));
        if (item.Index != null) parts.Add("E" + item.Index.Value.ToString(CultureInfo.InvariantCulture));
        if (item.Title.Length > 0) parts.Add(item.Title);
        return string.Join(" · ", parts);
    }

    /// <summary>"−10 s"'s number: whole seconds, never 0 for a real step.</summary>
    public static int Seconds(long ms) => (int)((Math.Abs(ms) + 500) / 1000);

    /// <summary>The seek badge: "+30 s", "−10 s".</summary>
    public static string Badge(long ms) => (ms < 0 ? "−" : "+") + Seconds(ms).ToString(CultureInfo.InvariantCulture) + " s";
}
