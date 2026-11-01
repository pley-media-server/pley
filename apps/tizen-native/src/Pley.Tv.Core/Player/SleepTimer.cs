using System;
using System.Collections.Generic;

namespace Pley.Tv.Core.Player;

/// <summary>What the sleep timer is set to. Not remembered between playbacks.</summary>
public abstract record SleepChoice
{
    public sealed record Off : SleepChoice;

    public sealed record Minutes(int Count) : SleepChoice;

    /// <summary>The end of the episode, film or track playing.</summary>
    public sealed record EndOfItem : SleepChoice;

    /// <summary>The menu's choices, in order.</summary>
    public static readonly IReadOnlyList<SleepChoice> All = new SleepChoice[]
    {
        new Off(), new Minutes(15), new Minutes(30), new Minutes(45), new Minutes(60), new Minutes(90), new EndOfItem(),
    };
}

/// <summary>Where the sleep timer stands.</summary>
/// <param name="SecondsLeft">Seconds before it pauses, during the notice (its last 30 s); null otherwise.</param>
/// <param name="MinutesLeft">Minutes before it pauses, rounded up, for the menu; null when off or unknown.</param>
/// <param name="Fired">It paused the playback, which hasn't played since.</param>
public sealed record SleepState(SleepChoice? Choice = null, int? SecondsLeft = null, int? MinutesLeft = null, bool Fired = false)
{
    public SleepChoice Set => Choice ?? new SleepChoice.Off();
}

/// <summary>How the sleep timer's menus say things, for music and video alike (the Android app's SleepTexts).</summary>
public static class SleepText
{
    public const string Title = "Sleep timer";

    /// <summary>A choice: "Off", "30 min", "End of this episode" (music: "End of this track").</summary>
    public static string Choice(SleepChoice choice, bool music = false) => choice switch
    {
        SleepChoice.Minutes m => $"{m.Count} min",
        SleepChoice.EndOfItem => music ? "End of this track" : "End of this episode",
        _ => "Off",
    };

    /// <summary>What the menu row says now: "Off", "42 min left", "End of this episode".</summary>
    public static string Summary(SleepState state, bool music = false) =>
        state.Set is SleepChoice.Minutes && state.MinutesLeft is { } minutes ? $"{minutes} min left" : Choice(state.Set, music);

    /// <summary>"Pausing in 30 s".</summary>
    public static string Notice(int secondsLeft) => $"Pausing in {secondsLeft} s";

    public static string Keep(bool music = false) => music ? "Keep listening" : "Keep watching";
}

/// <summary>
/// The sleep timer of a video or of music (the Android app's SleepTimer): after the minutes
/// chosen, or at the end of what plays, it pauses (<c>onFire</c>). 30 s before, its state says so
/// once a second for a notice; <see cref="Extend"/> ("Keep listening") adds the same time again.
/// Time comes from the scheduler's monotonic clock. UI thread only.
/// </summary>
public sealed class SleepTimer
{
    /// <summary>The notice shows this long before the timer pauses.</summary>
    public const long NoticeMs = 30_000;

    /// <summary>On TV, a video paused by the timer this long, the player leaves.</summary>
    public const long LeaveAfterMs = 10 * 60_000;

    const long SecondMs = 1_000;
    const long MinuteMs = 60_000;

    readonly IScheduler scheduler;
    readonly Func<long?> itemLeftMs;
    readonly Func<object?> itemKey;
    readonly Action onFire;
    IDisposable? job;
    /// <summary>When a timer of minutes ends, on the monotonic clock.</summary>
    long deadline;
    /// <summary>"Keep listening" at the end of this item: the timer waits for the next one's end.</summary>
    object? spared;

    /// <param name="itemLeftMs">How long the item playing has left, in ms of playback; null while unknown.</param>
    /// <param name="itemKey">Tells items apart: the one playing now.</param>
    public SleepTimer(IScheduler scheduler, Func<long?> itemLeftMs, Func<object?> itemKey, Action onFire)
    {
        this.scheduler = scheduler;
        this.itemLeftMs = itemLeftMs;
        this.itemKey = itemKey;
        this.onFire = onFire;
    }

    public Signal<SleepState> State { get; } = new Signal<SleepState>(new SleepState());

    public void Set(SleepChoice choice)
    {
        job?.Dispose();
        job = null;
        spared = null;
        State.Value = new SleepState(choice);
        switch (choice)
        {
            case SleepChoice.Off:
                return;
            case SleepChoice.Minutes m:
                deadline = scheduler.Elapsed + m.Count * MinuteMs;
                break;
        }
        Run();
    }

    /// <summary>Adds the same time again: the minutes chosen, or until the end of the next item.</summary>
    public void Extend()
    {
        switch (State.Value.Set)
        {
            case SleepChoice.Off:
                return;
            case SleepChoice.Minutes m:
                deadline += m.Count * MinuteMs;
                break;
            default:
                spared = itemKey();
                break;
        }
        State.Value = State.Value with { SecondsLeft = null };
        Run();
    }

    /// <summary>Whether the timer ends with the item <paramref name="key"/>: what comes next waits.</summary>
    public bool EndsWith(object? key) => State.Value.Set is SleepChoice.EndOfItem && !Equals(key, spared);

    /// <summary>The item <paramref name="key"/> played to its end: true when that is the timer's end, which then fires (the caller holds what comes next).</summary>
    public bool ItemEnded(object? key)
    {
        if (!EndsWith(key)) return false;
        Fire();
        return true;
    }

    /// <summary>It plays again: the pause the timer made is over.</summary>
    public void Resumed()
    {
        if (State.Value.Fired) State.Value = State.Value with { Fired = false };
    }

    public void Cancel()
    {
        job?.Dispose();
        job = null;
        State.Value = new SleepState();
    }

    void Run()
    {
        job?.Dispose();
        var choice = State.Value.Set;
        var key = itemKey();
        long? left = choice switch
        {
            SleepChoice.Minutes => deadline - scheduler.Elapsed,
            SleepChoice.EndOfItem => Equals(key, spared) ? null : itemLeftMs(),
            _ => null,
        };
        if (choice is SleepChoice.Off) return;
        if (spared != null && !Equals(key, spared)) spared = null;
        if (left == null) Publish(null, null);
        else if (choice is SleepChoice.Minutes && left <= 0)
        {
            Fire();
            return;
        }
        // The end of the item fires through ItemEnded, when it truly ends.
        else if (left <= 0) Publish(null, null);
        else if (left <= NoticeMs) Publish((int)((left.Value + SecondMs - 1) / SecondMs), 1);
        else Publish(null, (int)((left.Value + MinuteMs - 1) / MinuteMs));
        job = scheduler.After(Wait(choice, left), Run);
    }

    /// <summary>Until the next second during the notice; else until the notice or the next minute, an item's end looked at each second.</summary>
    static long Wait(SleepChoice choice, long? left)
    {
        if (left == null || left <= 0 || choice is SleepChoice.EndOfItem) return SecondMs;
        if (left <= NoticeMs) return left.Value % SecondMs is var s && s > 0 ? s : SecondMs;
        var m = left.Value % MinuteMs;
        return Math.Min(left.Value - NoticeMs, m > 0 ? m : MinuteMs);
    }

    void Publish(int? secondsLeft, int? minutesLeft) => State.Value = State.Value with { SecondsLeft = secondsLeft, MinutesLeft = minutesLeft };

    void Fire()
    {
        job?.Dispose();
        job = null;
        spared = null;
        State.Value = new SleepState(Fired: true);
        onFire();
    }
}
