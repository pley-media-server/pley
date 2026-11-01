using System;
using System.Collections.Generic;
using System.Globalization;
using System.Linq;
using System.Text;
using System.Text.Json;
using System.Text.RegularExpressions;
using System.Threading.Tasks;
using Pley.Tv.Core.Api;
using Pley.Tv.Core.Auth;
using Pley.Tv.Core.Subtitles.Pictures;

namespace Pley.Tv.Core.Player;

/// <summary>A run of a cue's text in one style.</summary>
public sealed record CueSpan(string Text, bool Italic = false, bool Bold = false, bool Underline = false);

/// <summary>
/// One WebVTT cue as the server writes it (<c>/api/files/{id}/subtitles/{n}.vtt</c>): its times,
/// its lines of styled text (only <c>&lt;i&gt;</c>, <c>&lt;b&gt;</c> and <c>&lt;u&gt;</c> survive), and
/// the settings that place it.
/// </summary>
/// <param name="Line">A line number (<paramref name="SnapToLines"/>) or a percentage from the top; null: the last line.</param>
/// <param name="Position">Percentage of the width the cue's box is placed at; null: centred.</param>
/// <param name="Align">"start", "center", "end", "left" or "right".</param>
/// <param name="Size">Percentage of the width the cue's box takes; null: 100.</param>
public sealed record Cue(long StartMs, long EndMs, IReadOnlyList<IReadOnlyList<CueSpan>> Lines, double? Line = null, bool SnapToLines = true, double? Position = null, string Align = "center", double? Size = null)
{
    /// <summary>The text without its styling, lines joined by "\n".</summary>
    public string Text => string.Join("\n", Lines.Select(l => string.Concat(l.Select(s => s.Text))));

    /// <summary>
    /// The text as NUI's TextLabel markup (<c>EnableMarkup</c>): bold and italic as font tags, the
    /// text escaped, lines by "\n". Underline is left out: older TVs' markup doesn't know it.
    /// </summary>
    public string Markup()
    {
        var b = new StringBuilder();
        for (var i = 0; i < Lines.Count; i++)
        {
            if (i > 0) b.Append('\n');
            foreach (var span in Lines[i])
            {
                var text = span.Text.Replace("&", "&amp;").Replace("<", "&lt;").Replace(">", "&gt;");
                if (!span.Bold && !span.Italic)
                {
                    b.Append(text);
                    continue;
                }
                b.Append("<font");
                if (span.Bold) b.Append(" weight='bold'");
                if (span.Italic) b.Append(" slant='italic'");
                b.Append('>').Append(text).Append("</font>");
            }
        }
        return b.ToString();
    }
}

/// <summary>WebVTT as the server writes it, read into cues (the web TV app's subs/vtt.ts).</summary>
public static class WebVtt
{
    static readonly Regex Timing = new Regex(@"^\s*((?:\d+:)?\d{1,2}:\d{2}[.,]\d{1,3})\s+-->\s+((?:\d+:)?\d{1,2}:\d{2}[.,]\d{1,3})(.*)$", RegexOptions.CultureInvariant);
    static readonly Regex Blocks = new Regex(@"\n{2,}", RegexOptions.CultureInvariant);
    static readonly Regex Token = new Regex(@"<(\/?)([a-zA-Z]+|\d[\d:.]*)[^>]*>|&(#x[0-9a-fA-F]+|#\d+|[a-zA-Z]+);|\n|[^<&\n]+|[<&]", RegexOptions.CultureInvariant);
    static readonly Regex LineSetting = new Regex(@"^(-?\d+(?:\.\d+)?)(%)?$", RegexOptions.CultureInvariant);
    static readonly Regex Percent = new Regex(@"^(\d+(?:\.\d+)?)%$", RegexOptions.CultureInvariant);

    static readonly Dictionary<string, string> Entities = new Dictionary<string, string>
    {
        ["amp"] = "&", ["lt"] = "<", ["gt"] = ">", ["nbsp"] = " ", ["lrm"] = "‎", ["rlm"] = "‏", ["quot"] = "\"", ["apos"] = "'",
    };

    /// <summary>"01:02:03.456" (or "02:03,456") in milliseconds.</summary>
    public static long TimeMs(string time)
    {
        double seconds = 0;
        foreach (var part in time.Replace(',', '.').Split(':')) seconds = seconds * 60 + double.Parse(part, CultureInfo.InvariantCulture);
        return (long)Math.Round(seconds * 1000);
    }

    /// <summary>The cues of <paramref name="text"/>, by start then end; blocks that aren't cues, and empty or backwards ones, are skipped.</summary>
    public static IReadOnlyList<Cue> Parse(string text)
    {
        var cues = new List<Cue>();
        var normal = text.TrimStart('﻿').Replace("\r\n", "\n").Replace('\r', '\n');
        foreach (var block in Blocks.Split(normal))
        {
            var lines = block.Split('\n');
            var at = Array.FindIndex(lines, l => l.IndexOf("-->", StringComparison.Ordinal) >= 0);
            if (at < 0) continue;
            var timing = Timing.Match(lines[at]);
            if (!timing.Success) continue;
            var body = string.Join("\n", lines.Skip(at + 1)).Trim();
            if (body.Length == 0) continue;
            var start = TimeMs(timing.Groups[1].Value);
            var end = TimeMs(timing.Groups[2].Value);
            if (end <= start) continue;
            cues.Add(WithSettings(new Cue(start, end, Spans(body)), timing.Groups[3].Value));
        }
        return cues.OrderBy(c => c.StartMs).ThenBy(c => c.EndMs).ToList();
    }

    static Cue WithSettings(Cue cue, string settings)
    {
        foreach (var setting in settings.Trim().Split(new[] { ' ', '\t' }, StringSplitOptions.RemoveEmptyEntries))
        {
            var colon = setting.IndexOf(':');
            if (colon <= 0 || colon == setting.Length - 1) continue;
            var name = setting.Substring(0, colon);
            var value = setting.Substring(colon + 1).Split(',')[0];
            switch (name)
            {
                case "line":
                    var line = LineSetting.Match(value);
                    if (line.Success) cue = cue with { Line = double.Parse(line.Groups[1].Value, CultureInfo.InvariantCulture), SnapToLines = !line.Groups[2].Success };
                    break;
                case "position":
                    var position = Percent.Match(value);
                    if (position.Success) cue = cue with { Position = double.Parse(position.Groups[1].Value, CultureInfo.InvariantCulture) };
                    break;
                case "size":
                    var size = Percent.Match(value);
                    if (size.Success) cue = cue with { Size = double.Parse(size.Groups[1].Value, CultureInfo.InvariantCulture) };
                    break;
                case "align":
                    if (value is "start" or "center" or "end" or "left" or "right") cue = cue with { Align = value };
                    else if (value == "middle") cue = cue with { Align = "center" };
                    break;
            }
        }
        return cue;
    }

    /// <summary>
    /// A cue's text as lines of styled runs: entities decoded, <c>&lt;i&gt;</c>, <c>&lt;b&gt;</c> and
    /// <c>&lt;u&gt;</c> kept (closed when the cue forgets to), voice and class spans and timestamps dropped.
    /// </summary>
    public static IReadOnlyList<IReadOnlyList<CueSpan>> Spans(string body)
    {
        var lines = new List<IReadOnlyList<CueSpan>>();
        var line = new List<CueSpan>();
        var open = new List<string>();
        void Add(string text)
        {
            if (text.Length == 0) return;
            var span = new CueSpan(text, open.Contains("i"), open.Contains("b"), open.Contains("u"));
            if (line.Count > 0 && line[line.Count - 1] is var last && (last.Italic, last.Bold, last.Underline) == (span.Italic, span.Bold, span.Underline))
                line[line.Count - 1] = last with { Text = last.Text + text };
            else line.Add(span);
        }
        foreach (Match m in Token.Matches(body))
        {
            var token = m.Value;
            if (token == "\n")
            {
                lines.Add(line);
                line = new List<CueSpan>();
            }
            else if (m.Groups[2].Success)
            {
                var tag = m.Groups[2].Value.ToLowerInvariant();
                if (tag is not ("i" or "b" or "u")) continue;
                if (m.Groups[1].Value.Length > 0)
                {
                    var at = open.LastIndexOf(tag);
                    if (at >= 0) open.RemoveAt(at);
                }
                else open.Add(tag);
            }
            else if (m.Groups[3].Success) Add(Entity(m.Groups[3].Value) ?? token);
            else Add(token);
        }
        lines.Add(line);
        return lines;
    }

    static string? Entity(string name)
    {
        if (name[0] != '#') return Entities.TryGetValue(name.ToLowerInvariant(), out var ch) ? ch : null;
        var hex = name.Length > 1 && (name[1] == 'x' || name[1] == 'X');
        if (!int.TryParse(hex ? name.Substring(2) : name.Substring(1), hex ? NumberStyles.HexNumber : NumberStyles.Integer, CultureInfo.InvariantCulture, out var code)) return null;
        return code > 0 && code < 0x110000 && (code < 0xd800 || code > 0xdfff) ? char.ConvertFromUtf32(code) : null;
    }
}

/// <summary>
/// The cues of one subtitle track, by time (the Android app's SubtitleIndex, the web TV app's
/// CueIndex). A track still being read out of its file grows: each answer holds every cue found
/// so far, and <see cref="Merge"/> keeps whichever reaches further, so an answer that arrives late
/// never takes cues away.
/// </summary>
public sealed class SubtitleIndex
{
    public static readonly SubtitleIndex Empty = new SubtitleIndex(new Cue[0]);

    readonly List<Cue> cues;
    /// <summary>The longest cue: how far back a lookup has to look.</summary>
    readonly long longest;

    public SubtitleIndex(IEnumerable<Cue> cues)
    {
        this.cues = cues.OrderBy(c => c.StartMs).ThenBy(c => c.EndMs).ToList();
        longest = this.cues.Count == 0 ? 0 : this.cues.Max(c => c.EndMs - c.StartMs);
        EndMs = this.cues.Count == 0 ? 0 : this.cues.Max(c => c.EndMs);
    }

    /// <summary>Cues in all.</summary>
    public int Size => cues.Count;

    /// <summary>Where the last cue ends; 0 without cues.</summary>
    public long EndMs { get; }

    /// <summary>The cues showing at <paramref name="ms"/>, earliest first.</summary>
    public IReadOnlyList<Cue> At(long ms)
    {
        var shown = new List<Cue>();
        for (var i = FirstStartingAtOrAfter(ms - longest); i < cues.Count; i++)
        {
            var c = cues[i];
            if (c.StartMs > ms) break;
            if (ms < c.EndMs) shown.Add(c);
        }
        return shown;
    }

    /// <summary>When what shows changes after <paramref name="ms"/>: the next start or end; null when nothing changes any more.</summary>
    public long? NextChangeAfter(long ms)
    {
        long? next = null;
        for (var i = FirstStartingAtOrAfter(ms - longest); i < cues.Count; i++)
        {
            var c = cues[i];
            if (c.StartMs > ms)
            {
                if (next == null || c.StartMs < next) next = c.StartMs;
                break;
            }
            if (c.EndMs > ms && (next == null || c.EndMs < next)) next = c.EndMs;
        }
        return next;
    }

    /// <summary><paramref name="more"/>, a later answer of the same track, unless it reaches less far than this one.</summary>
    public SubtitleIndex Merge(SubtitleIndex more) => more.EndMs > EndMs || (more.EndMs == EndMs && more.Size > Size) ? more : this;

    int FirstStartingAtOrAfter(long ms)
    {
        int lo = 0, hi = cues.Count;
        while (lo < hi)
        {
            var mid = (lo + hi) >> 1;
            if (cues[mid].StartMs < ms) lo = mid + 1;
            else hi = mid;
        }
        return lo;
    }
}

/// <summary>
/// Subtitles moved in time (the subtitle panel's Sync; the Android app's SubtitleSync): in 0.1 s
/// steps from −10 s to +10 s; positive shows them later (a subtitle at 10 s shows at 10.5 s with +0.5 s).
/// </summary>
public static class SubtitleSync
{
    public const long StepMs = 100;
    public const long MaxMs = 10_000;

    /// <summary><paramref name="offsetMs"/> rounded to a step and kept within ±10 s.</summary>
    public static long Clamp(long offsetMs) => Math.Max(-MaxMs, Math.Min(MaxMs, Round(offsetMs / (double)StepMs) * StepMs));

    /// <summary>Rounds half up, as Kotlin's and JavaScript's <c>round</c> do (−2.5 is −2).</summary>
    internal static long Round(double value) => (long)Math.Floor(value + 0.5);

    /// <summary>The track's own time when the playhead is at <paramref name="positionMs"/>: a cue made for t shows at t + <paramref name="offsetMs"/>.</summary>
    public static long TrackTime(long positionMs, long offsetMs) => positionMs - offsetMs;

    /// <summary>How long until what <paramref name="index"/> shows changes, from the playhead at <paramref name="positionMs"/>; null when nothing changes any more.</summary>
    public static long? UntilChange(SubtitleIndex index, long positionMs, long offsetMs)
    {
        var at = TrackTime(positionMs, offsetMs);
        return index.NextChangeAfter(at) - at;
    }

    /// <summary>"+0.5 s", "−1.2 s", "0.0 s".</summary>
    public static string Label(long offsetMs)
    {
        var sign = offsetMs > 0 ? "+" : offsetMs < 0 ? "−" : "";
        return sign + (Math.Abs(offsetMs) / 1000.0).ToString("F1", CultureInfo.InvariantCulture) + " s";
    }
}

/// <summary>
/// Subtitle offsets by profile and file, kept on this TV and never sent to the server (the web TV
/// app's OffsetStore): under the file's absolute address and the profile
/// (<c>&lt;server&gt;/api/files/&lt;id&gt;#u&lt;profile&gt;</c>), as subtitles one person finds late
/// may suit another. Without a profile nothing is read or kept; the 300 files set last are kept.
/// </summary>
public sealed class SubtitleOffsets
{
    const string Key = "pley.subtitleOffsets";
    const int Kept = 300;

    readonly IKeyValue store;
    readonly Func<long?> profile;

    public SubtitleOffsets(IKeyValue store, Func<long?> profile)
    {
        this.store = store;
        this.profile = profile;
    }

    /// <summary>The offset of file <paramref name="fileId"/> on <paramref name="server"/> for the profile; 0 when none was set.</summary>
    public long Get(string server, long fileId)
    {
        if (KeyOf(server, fileId) is not string key) return 0;
        foreach (var (k, v) in Read())
            if (k == key) return SubtitleSync.Clamp(v);
        return 0;
    }

    /// <summary>Remembers <paramref name="ms"/> for the file and the profile; 0 forgets it.</summary>
    public void Set(string server, long fileId, long ms)
    {
        if (KeyOf(server, fileId) is not string key) return;
        var entries = Read().Where(e => e.Key != key).ToList();
        var value = SubtitleSync.Clamp(ms);
        if (value != 0) entries.Add((key, value));
        try
        {
            store.Set(Key, JsonSerializer.Serialize(entries.Skip(Math.Max(0, entries.Count - Kept)).Select(e => new object[] { e.Key, e.Value })));
        }
        catch (Exception)
        {
            // Storage full or unavailable: the offset lasts as long as the player.
        }
    }

    string? KeyOf(string server, long fileId) => profile() is long p ? $"{server.TrimEnd('/')}/api/files/{fileId}#u{p}" : null;

    List<(string Key, long Value)> Read()
    {
        var entries = new List<(string, long)>();
        try
        {
            using var doc = JsonDocument.Parse(store.Get(Key) ?? "[]");
            if (doc.RootElement.ValueKind != JsonValueKind.Array) return entries;
            foreach (var e in doc.RootElement.EnumerateArray())
            {
                if (e.ValueKind != JsonValueKind.Array || e.GetArrayLength() != 2) continue;
                var k = e[0];
                var v = e[1];
                if (k.ValueKind == JsonValueKind.String && v.ValueKind == JsonValueKind.Number && v.TryGetInt64(out var ms)) entries.Add((k.GetString()!, ms));
            }
        }
        catch (Exception)
        {
            // Unreadable: as if none were kept.
        }
        return entries;
    }
}

/// <summary>
/// The subtitles this TV draws (the web TV app's SubtitlePresenter, the Android app's
/// ServerSubtitles): the server's WebVTT, asked for again every 3 s while the track is still being
/// read out of its file, each answer merged; or the server's .mks of a picture track (PGS, VobSub,
/// DVB), asked for again the same way, its pictures decoded only when shown. What shows is worked
/// out at the playhead moved by the offset, again at each change (20 to 250 ms away, at the
/// playback's speed; 500 ms when nothing changes any more).
/// </summary>
public sealed class SubtitlePresenter
{
    /// <summary>A partial answer is asked for again this often.</summary>
    public const long PartialRetryMs = 3_000;

    /// <summary>
    /// A partial picture track is asked for again after <see cref="PartialRetryMs"/>, then twice as
    /// long each time up to this: each answer is the whole .mks so far, tens of MB for a film, and
    /// read every 3 s for the minutes the server takes to make it, the app stopped answering in the
    /// collections that followed. Deviation from Android, which asks every 3 s.
    /// </summary>
    public const long PicturesRetryMostMs = 60_000;
    const long MinWaitMs = 20, MaxWaitMs = 250, IdleWaitMs = 500;

    readonly IScheduler scheduler;
    readonly Func<string, Task<SubtitleText>> fetch;
    readonly Func<string, Task<SubtitlePictures>>? fetchPictures;
    readonly Func<long> position;
    readonly Func<long> offset;
    readonly Func<double> speed;
    readonly Action onFailed;
    readonly Action? onPicturesUnavailable;
    IDisposable? retry;
    IDisposable? tick;
    SubtitleIndex? index;
    PictureSubtitles? pictures;
    bool showingPictures;
    string shownKey = "";
    int generation;
    bool active = true;

    /// <param name="fetch">The WebVTT at an absolute address, and whether it is whole.</param>
    /// <param name="onFailed">The track couldn't be loaded: said once per choice.</param>
    /// <param name="fetchPictures">The .mks of a picture track at an absolute address, and whether it is whole.</param>
    /// <param name="onPicturesUnavailable">The server can't make picture subtitles (404 from an older server, 503 from an older transcoder).</param>
    public SubtitlePresenter(IScheduler scheduler, Func<string, Task<SubtitleText>> fetch, Func<long> position, Func<long> offset, Func<double> speed, Action onFailed,
        Func<string, Task<SubtitlePictures>>? fetchPictures = null, Action? onPicturesUnavailable = null)
    {
        this.scheduler = scheduler;
        this.fetch = fetch;
        this.position = position;
        this.offset = offset;
        this.speed = speed;
        this.onFailed = onFailed;
        this.fetchPictures = fetchPictures;
        this.onPicturesUnavailable = onPicturesUnavailable;
    }

    /// <summary>The cues showing now, earliest first; empty when none.</summary>
    public Signal<IReadOnlyList<Cue>> Shown { get; } = new Signal<IReadOnlyList<Cue>>(new Cue[0]);

    /// <summary>The picture showing now, never empty; null when none.</summary>
    public Signal<PictureFrame?> Picture { get; } = new Signal<PictureFrame?>(null);

    /// <summary>The address of the track shown; null when none.</summary>
    public string? Showing { get; private set; }

    /// <summary>Whether the track shown is the .mks of a picture track.</summary>
    public bool ShowingPictures => Showing != null && showingPictures;

    /// <summary>Shows the server's WebVTT at <paramref name="url"/>; the same track again keeps what was fetched.</summary>
    public void Show(string url)
    {
        if (Showing == url && !showingPictures)
        {
            Refresh();
            return;
        }
        Reset(url, false);
        Clear();
        Fetch(url, generation);
    }

    /// <summary>Shows the server's .mks of a picture track at <paramref name="url"/>; the same track again keeps what was fetched.</summary>
    public void ShowPictures(string url)
    {
        if (Showing == url && showingPictures)
        {
            Refresh();
            return;
        }
        Reset(url, true);
        Clear();
        FetchPictures(url, generation, PartialRetryMs);
    }

    public void Hide()
    {
        Reset(null, false);
        Clear();
    }

    /// <summary>The position jumped, or the offset changed: look again now.</summary>
    public void Refresh()
    {
        tick?.Dispose();
        tick = null;
        Update();
    }

    /// <summary>Stops the clock while nothing plays for a while; <see cref="Resume"/> starts it again.</summary>
    public void Pause()
    {
        active = false;
        tick?.Dispose();
        tick = null;
    }

    public void Resume()
    {
        active = true;
        Refresh();
    }

    void Reset(string? url, bool isPictures)
    {
        generation++;
        retry?.Dispose();
        tick?.Dispose();
        retry = null;
        tick = null;
        Showing = url;
        showingPictures = isPictures;
        index = null;
        pictures = null;
        shownKey = "";
    }

    void Fetch(string url, int at)
    {
        Ui.Plays.When(scheduler, () => fetch(url), answer =>
        {
            if (at != generation) return;
            var parsed = new SubtitleIndex(WebVtt.Parse(answer.Text));
            var merged = index?.Merge(parsed) ?? parsed;
            if (merged != index)
            {
                index = merged;
                Refresh();
            }
            if (!answer.Complete) retry = scheduler.After(PartialRetryMs, () => Fetch(url, at));
        }, _ =>
        {
            if (at != generation) return;
            // Choosing the track again tries again.
            Showing = null;
            onFailed();
        });
    }

    void FetchPictures(string url, int at, long waitMs)
    {
        if (fetchPictures == null)
        {
            Showing = null;
            onPicturesUnavailable?.Invoke();
            return;
        }
        Ui.Plays.When(scheduler, () => ReadPictures(fetchPictures, url), answer =>
        {
            if (at != generation) return;
            var subs = PictureSubtitles.Of(answer.Track);
            if (subs == null)
            {
                // A codec this app can't decode: the server burns it in instead.
                Showing = null;
                onPicturesUnavailable?.Invoke();
                return;
            }
            // Each answer holds every picture found so far: the later one replaces the earlier.
            pictures = subs;
            Refresh();
            if (!answer.Complete) retry = scheduler.After(waitMs, () => FetchPictures(url, at, Math.Min(waitMs * 2, PicturesRetryMostMs)));
        }, e =>
        {
            if (at != generation) return;
            Showing = null;
            if (e is ApiException { Status: 404 or 503 }) onPicturesUnavailable?.Invoke();
            else onFailed();
        });
    }

    /// <summary>The .mks fetched and read, off the UI thread: a film's track can be megabytes.</summary>
    static async Task<(PictureTrack Track, bool Complete)> ReadPictures(Func<string, Task<SubtitlePictures>> fetch, string url)
    {
        var answer = await fetch(url).ConfigureAwait(false);
        if (Marks.On) Marks.Note($"subtitles: the pictures' .mks, {answer.Bytes.Length / 1024} KB{(answer.Complete ? "" : ", not whole yet")}");
        return (Mks.ReadPictureTrack(answer.Bytes), answer.Complete);
    }

    void Update()
    {
        if (Showing == null || !active) return;
        var pos = position();
        var shift = offset();
        var at = SubtitleSync.TrackTime(pos, shift);
        long? change;
        if (index != null)
        {
            var cues = index.At(at);
            Publish(cues, null, string.Join(",", cues.Select(c => $"{c.StartMs}-{c.EndMs}")));
            change = SubtitleSync.UntilChange(index, pos, shift);
        }
        else if (pictures != null)
        {
            var shown = pictures.FrameAt(at);
            Publish(new Cue[0], shown?.Frame, shown is { } s ? $"p{s.Index}" : "");
            change = pictures.NextChange(at) - at;
        }
        else return;
        var rate = speed() > 0 ? speed() : 1;
        // Media time: at 2× a change comes in half the time.
        var wait = change is long until ? Math.Max(MinWaitMs, Math.Min(MaxWaitMs, (long)(until / rate))) : IdleWaitMs;
        tick = scheduler.After(wait, () =>
        {
            tick = null;
            Update();
        });
    }

    void Clear() => Publish(new Cue[0], null, "");

    void Publish(IReadOnlyList<Cue> cues, PictureFrame? picture, string key)
    {
        if (key == shownKey && key.Length > 0) return;
        shownKey = key;
        if (cues.Count > 0 || Shown.Value.Count > 0) Shown.Value = cues;
        Picture.Value = picture;
    }
}
