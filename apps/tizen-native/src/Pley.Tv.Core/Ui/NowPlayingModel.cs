using System;
using System.Collections.Generic;
using System.Linq;
using Pley.Tv.Core.Player;

namespace Pley.Tv.Core.Ui;

/// <summary>A line of the queue: its <paramref name="Position"/> in the play order, and whether it is the one playing.</summary>
/// <param name="Subtitle">"Artist · Album".</param>
/// <param name="Duration">"3:12"; empty when unknown.</param>
/// <param name="ArtPath">The album art's path, as the server gave it; null without.</param>
public sealed record QueueRow(int Position, long Id, string Title, string Subtitle, string Duration, bool Current, string? ArtPath)
{
    /// <summary>What the line's number column shows: its place, from 1.</summary>
    public string Number => (Position + 1).ToString(System.Globalization.CultureInfo.InvariantCulture);
}

/// <summary>Now Playing: the track (null when nothing plays), and the queue.</summary>
public sealed record NowPlayingState(NowPlaying? Now, IReadOnlyList<QueueRow> Queue, string Origin);

/// <summary>
/// Now Playing (design spec §4.10; the Android app's NowPlayingViewModel and TvNowPlayingScreen):
/// the album art at 300 dp at (96, 58), and beside it 32 dp on the title (page title), the byline
/// (body large, muted), a 22 dp status line (<see cref="Status"/>), the scrubber (24 dp tall, a 4 dp
/// track, the buffered part at 45 %, the played part in the accent, a 10 dp thumb growing to 12
/// with a ring when focused; the times under it, "−" before what is left), then Shuffle, Previous,
/// Play/Pause (64 dp, a 32 dp glyph, the first focus), Next and the sleep timer as round buttons
/// 16 dp apart, then "Up next" and the queue (track lines 44 dp at least, 2 dp apart, from the
/// track playing on). Over a faint copy of the art at 20 %. Focused, the scrubber's Left and Right
/// seek 10 s (<see cref="SeekBy"/>) and OK plays or pauses. After two minutes without a key the
/// page dims to 30 % (<see cref="DimAfterMs"/>), and the next key wakes it. Nothing playing:
/// "Nothing is playing", with Back.
/// </summary>
public sealed class NowPlayingModel : IDisposable
{
    /// <summary>Left and Right on the scrubber.</summary>
    public const long SeekMs = 10_000;

    /// <summary>Two minutes without a key, then the screen dims.</summary>
    public const long DimAfterMs = 120_000;

    /// <summary>How bright the dimmed screen stays.</summary>
    public const float Dimmed = 0.3f;

    /// <summary>The position is read this often while it plays.</summary>
    public const long PositionEveryMs = 500;

    readonly MusicPlayer player;
    readonly string origin;
    readonly List<IDisposable> watches = new List<IDisposable>();

    public NowPlayingModel(MusicPlayer player, string origin)
    {
        this.player = player;
        this.origin = origin;
        State = new Signal<NowPlayingState>(Compute());
        watches.Add(player.Now.Watch(_ => State.Value = Compute()));
        watches.Add(player.Queue.Watch(_ => State.Value = Compute()));
    }

    public Signal<NowPlayingState> State { get; }

    /// <summary>The music's sleep timer.</summary>
    public Signal<SleepState> Sleep => player.Sleep;

    /// <summary>Where the track is: read while drawing.</summary>
    public MusicPosition Position() => player.Position();

    public void Toggle() => player.Toggle();

    public void Next() => player.Next();

    public void Previous() => player.Previous();

    public void SeekTo(long positionMs) => player.SeekTo(positionMs);

    /// <summary>The scrubber's Left (-1) and Right (1): 10 s back or on, within the track.</summary>
    public void SeekBy(int direction)
    {
        var p = player.Position();
        var duration = p.DurationMs > 0 ? p.DurationMs : State.Value.Now?.DurationMs ?? 0;
        var to = p.PositionMs + Math.Sign(direction) * SeekMs;
        player.SeekTo(Math.Max(0, duration > 0 ? Math.Min(to, duration) : to));
    }

    /// <summary>Plays the queue's track at <paramref name="position"/>.</summary>
    public void JumpTo(int position) => player.JumpTo(position);

    public void SetShuffle(bool shuffle) => player.SetShuffle(shuffle);

    /// <summary>The sleep button: during the notice "Keep listening", else the next choice, round the list.</summary>
    public void StepSleep() => player.StepSleep();

    /// <summary>
    /// What the sleep button says: "Keep listening" during the notice, "Sleep timer" while off,
    /// else "Sleep timer: 42 min left" or "Sleep timer: End of this track".
    /// </summary>
    public string SleepLabel
    {
        get
        {
            var s = Sleep.Value;
            if (s.SecondsLeft != null) return SleepText.Keep(music: true);
            return s.Set is SleepChoice.Off ? SleepText.Title : MusicText.Sleep(SleepText.Title, SleepText.Summary(s, music: true));
        }
    }

    /// <summary>Shuffle's label: "Shuffle", or "Stop shuffling" while it shuffles.</summary>
    public string ShuffleLabel => State.Value.Now?.Shuffle == true ? MusicText.ShuffleOff : MusicText.Shuffle;

    /// <summary>Play/Pause's label and glyph: Pause while it plays or is about to.</summary>
    public bool ShowsPause => State.Value.Now is { } n && (n.Playing || (n.Loading && n.Error == null));

    /// <summary>
    /// The status line: why it doesn't play (in danger text), "Loading…", or the sleep timer's
    /// "Pausing in 30 s" (subtle); empty otherwise.
    /// </summary>
    public string Status
    {
        get
        {
            var n = State.Value.Now;
            if (n == null) return "";
            if (n.Error is { } e) return MusicText.Error(e);
            if (n.Loading) return MusicText.Loading;
            return Sleep.Value.SecondsLeft is { } s ? SleepText.Notice(s) : "";
        }
    }

    /// <summary>The scrubber's times: "1:05" played and "−2:07" left.</summary>
    public (string Played, string Left) Times(MusicPosition p)
    {
        var duration = p.DurationMs > 0 ? p.DurationMs : State.Value.Now?.DurationMs ?? 0;
        return (PlayerText.Time(p.PositionMs), PlayerText.Remaining(p.PositionMs, duration));
    }

    public void Dispose()
    {
        foreach (var w in watches) w.Dispose();
        watches.Clear();
    }

    NowPlayingState Compute()
    {
        var queue = player.Queue.Value;
        return new NowPlayingState(player.Now.Value, queue.Tracks.Select((t, i) => new QueueRow(
            i,
            t.Id,
            t.Title,
            string.Join(" · ", new[] { t.GrandparentTitle, t.ParentTitle }.Where(s => !string.IsNullOrWhiteSpace(s))),
            t.DurationMs is > 0 ? PlayerText.Time(t.DurationMs.Value) : "",
            i == queue.Index,
            MusicPlayer.ArtPath(t))).ToList(), origin);
    }
}
