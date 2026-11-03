using System;
using System.Collections.Generic;
using System.Linq;
using System.Security.Cryptography;
using Pley.Tv.Core.Api;
using Pley.Tv.Core.Ui;

namespace Pley.Tv.Core.Player;

/// <summary>How this TV is listed for casting: an id it makes once and keeps, its name ("Samsung TV"), its platform.</summary>
public sealed record CastIdentity(string Id, string Name, string Platform);

/// <summary>A command for this TV, read from its event stream.</summary>
public abstract record CastCommand
{
    /// <summary>Play this; <paramref name="PositionMs"/> null: where the profile left off, without asking.</summary>
    public sealed record Play(long ItemId, long? FileId, long? PositionMs) : CastCommand;

    /// <summary>The video player pauses, resumes, seeks (to <paramref name="PositionMs"/>, null for the others) or stops.</summary>
    public sealed record Control(string Action, long? PositionMs) : CastCommand;
}

/// <summary>What the video player is doing, as the cast report reads it.</summary>
/// <param name="Ended">The title played to its end (Up Next may show): it plays nothing now.</param>
public sealed record CastView(long ItemId, PhaseKind Phase, long PositionMs, long DurationMs, bool Ended);

/// <summary>What the TV says it plays: an item, where it is, its length and its state; <see cref="ItemId"/> null plays nothing.</summary>
public sealed record CastReport(long? ItemId, long PositionMs = 0, long? DurationMs = null, string State = "playing")
{
    public static readonly CastReport Nothing = new CastReport((long?)null);

    /// <summary>The body of PUT /api/cast/devices/{id}/state.</summary>
    public Dictionary<string, object?> Body() => ItemId == null
        ? new Dictionary<string, object?> { ["item_id"] = null }
        : new Dictionary<string, object?> { ["item_id"] = ItemId, ["position_ms"] = PositionMs, ["duration_ms"] = DurationMs, ["state"] = State };
}

/// <summary>
/// Being cast to (docs/API.md, "Casting to another device"): a phone or a browser of the profile
/// sends this TV a movie, an episode or an extra, then pauses, seeks and stops it, and sees what it
/// plays. The TV is listed while its event stream is open, plays what it is sent as if it had been
/// picked here, and says what its video player does. TVs don't cast themselves. The web TV app has
/// the same rules (fixtures/cast.json).
/// </summary>
public static class Cast
{
    /// <summary>While a video plays, what it plays is said this often.</summary>
    public const long ReportMs = 10_000;
    /// <summary>Positions and lengths the server takes: up to a week.</summary>
    const long MaxMs = 7L * 24 * 3600 * 1000;
    static readonly string[] Actions = { "pause", "resume", "seek", "stop" };

    /// <summary>This app as a device to cast to, with its id.</summary>
    public static CastIdentity Identity(string id) => new CastIdentity(id, "Samsung TV", "samsung");

    /// <summary>A new device id: 32 hex digits, random.</summary>
    public static string NewId()
    {
        var bytes = new byte[16];
        using (var random = RandomNumberGenerator.Create()) random.GetBytes(bytes);
        return string.Concat(bytes.Select(b => b.ToString("x2")));
    }

    /// <summary>Whether <paramref name="id"/> is one <see cref="NewId"/> makes.</summary>
    public static bool IsId(string? id) => id != null && id.Length == 32 && id.All(c => c is >= '0' and <= '9' or >= 'a' and <= 'f');

    /// <summary>The event stream's query that lists the TV for casting while it is open; "" without.</summary>
    public static string Query(CastIdentity? cast) => cast == null
        ? ""
        : $"?cast_device={Uri.EscapeDataString(cast.Id)}&cast_name={Uri.EscapeDataString(cast.Name)}&cast_platform={Uri.EscapeDataString(cast.Platform)}";

    /// <summary>
    /// The command <paramref name="e"/> gives the TV listed as <paramref name="deviceId"/>; null for
    /// any other event, another device's, or one that lacks what its kind needs (a seek without its
    /// position). Events that don't read as theirs never come (<see cref="SseReader"/>).
    /// </summary>
    public static CastCommand? Command(ServerEvent e, string deviceId)
    {
        if (e.DeviceId != deviceId || e.PositionMs is < 0) return null;
        switch (e.Type)
        {
            case "cast_play":
                if (e.ItemId is not > 0 || e.FileId is <= 0) return null;
                return new CastCommand.Play(e.ItemId.Value, e.FileId, e.PositionMs);
            case "cast_control":
                if (e.Action == null || Array.IndexOf(Actions, e.Action) < 0 || (e.Action == "seek" && e.PositionMs == null)) return null;
                return new CastCommand.Control(e.Action, e.Action == "seek" ? e.PositionMs : null);
            default:
                return null;
        }
    }

    /// <summary>
    /// What the player plays, said to the server: the item, where it is, how long it is and whether it
    /// plays, waits or is paused; nothing while there is no player, after the title's end or an error.
    /// </summary>
    public static CastReport Report(CastView? view)
    {
        if (view == null || view.Ended || view.ItemId <= 0 || view.Phase is PhaseKind.Ended or PhaseKind.Error) return CastReport.Nothing;
        var state = view.Phase switch
        {
            PhaseKind.Playing => "playing",
            PhaseKind.Paused => "paused",
            _ => "buffering",
        };
        return new CastReport(view.ItemId, Clamp(view.PositionMs), view.DurationMs > 0 ? Clamp(view.DurationMs) : null, state);
    }

    static long Clamp(long ms) => Math.Min(MaxMs, Math.Max(0, ms));

    /// <summary>
    /// Whether <paramref name="next"/> says something <paramref name="last"/> (what was said last;
    /// null before anything, as nothing playing) didn't: another item, or another state. A position
    /// moving on is said every 10 s, and after a seek.
    /// </summary>
    public static bool Due(CastReport? last, CastReport next)
    {
        var was = last ?? CastReport.Nothing;
        if (was.ItemId != next.ItemId) return true;
        return was.ItemId != null && was.State != next.State;
    }

    /// <summary>
    /// The player's request for a title cast to the TV: at the position it was sent with, else where
    /// the profile left off (<paramref name="detail"/>'s resume point, from the start once watched),
    /// as the item page's Play has it, without asking.
    /// </summary>
    public static PlayRequest Request(CastCommand.Play play, ItemDetail? detail)
    {
        var start = play.PositionMs ?? (detail == null || detail.UserState.Watched ? 0 : detail.UserState.PositionMs);
        return new PlayRequest(play.ItemId, Math.Max(0, start), FromStart: start == 0 && play.PositionMs != null, FileId: play.FileId);
    }
}

/// <summary>
/// The reports of one video player (PUT /api/cast/devices/{id}/state): as it starts (buffering,
/// then playing), pauses, resumes, waits and seeks, every 10 s while it plays, and nothing playing
/// as it closes or the title ends; the next episode is a new report. Best effort: what fails is
/// dropped where it is sent.
/// </summary>
public sealed class CastReporter
{
    readonly Func<CastView?> view;
    readonly Action<CastReport> send;
    readonly IScheduler scheduler;
    CastReport? last;
    IDisposable? timer;
    bool closed;

    public CastReporter(Func<CastView?> view, Action<CastReport> send, IScheduler scheduler)
    {
        this.view = view;
        this.send = send;
        this.scheduler = scheduler;
    }

    /// <summary>The player changed: said when it plays another item, or plays, waits or pauses now.</summary>
    public void Changed()
    {
        if (closed) return;
        var next = Cast.Report(view());
        if (Cast.Due(last, next)) Say(next);
    }

    /// <summary>Said at once: after a seek, or when the event stream connects again (the server knows nothing of it then).</summary>
    public void Now()
    {
        if (!closed) Say(Cast.Report(view()));
    }

    /// <summary>The player closed: it plays nothing, unless that was said already.</summary>
    public void Close()
    {
        if (closed) return;
        closed = true;
        timer?.Dispose();
        timer = null;
        if (last?.ItemId != null) send(CastReport.Nothing);
        last = CastReport.Nothing;
    }

    void Say(CastReport report)
    {
        last = report;
        send(report);
        timer?.Dispose();
        timer = report.ItemId != null && report.State == "playing"
            ? scheduler.After(Cast.ReportMs, () =>
            {
                timer = null;
                Now();
            })
            : null;
    }
}
