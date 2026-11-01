using System;

namespace Pley.Tv.Core.Player;

/// <summary>
/// Left and Right in the player (the Android app's TvSeeker, timed as YouTube's TV app; TV
/// addendum §5.2, design spec §2.7):
/// <list type="bullet">
/// <item>A press moves the target 10 s. Presses in a row add up ("+30 s") and seek once, when
/// they settle (<see cref="Settle"/>): the player waits for OK there, <see cref="WaitMs"/> at most,
/// and the media keys' jumps <see cref="SettleMs"/>, so a few quick presses don't start the
/// server's conversion again for each.</item>
/// <item>A held key scrubs: the target runs faster the longer the key is held, from
/// <see cref="HoldStartRate"/>, doubling every <see cref="HoldDoublesMs"/>. Let go, it waits as
/// presses do (<see cref="Release"/>).</item>
/// </list>
/// Times are the keys' own, in ms; the UI thread only.
/// </summary>
public sealed class Seeker
{
    /// <summary>A press of Left or Right.</summary>
    public const long StepMs = 10_000;
    /// <summary>The media keys' jumps in a row seek once, this long after the last.</summary>
    public const long SettleMs = 400;
    /// <summary>A seek with Left and Right waits for OK, and goes by itself this long after the last key.</summary>
    public const long WaitMs = 5_000;
    /// <summary>The seek badge shows this long after the last press.</summary>
    public const long BadgeMs = 600;
    /// <summary>OK this soon after a seek confirms it instead of pausing: a seek far off takes a moment to show.</summary>
    public const long ConfirmMs = 2_000;
    /// <summary>Seeks stop short of the end, so the end isn't reached by seeking.</summary>
    public const long EndMarginMs = 1_000;

    /// <summary>A held key's speed as it starts, in ms of video per ms held.</summary>
    public const double HoldStartRate = 20;
    /// <summary>A held key's speed doubles this often.</summary>
    public const long HoldDoublesMs = 1_500;
    /// <summary>The speed it stops at: at least this, or a whole video in <see cref="FullSweepMs"/>.</summary>
    public const double HoldTopRate = 300;
    /// <summary>At its top speed, a whole video goes by in about this long.</summary>
    public const long FullSweepMs = 10_000;
    /// <summary>A gap between repeats longer than this counts as this (a stalled frame isn't a jump).</summary>
    const long MaxGapMs = 200;

    readonly Func<long> position;
    readonly Func<long> duration;
    readonly Action<long> seek;
    bool confirmable;
    long asked;
    long holdStart;
    long lastEvent;

    public Seeker(Func<long> position, Func<long> duration, Action<long> seek)
    {
        this.position = position;
        this.duration = duration;
        this.seek = seek;
    }

    /// <summary>Where the seek will land, while one is coming: the scrubber shows it.</summary>
    public long? Target { get; private set; }

    /// <summary>A key is held: scrubbing.</summary>
    public bool Holding { get; private set; }

    /// <summary>How far the presses in a row go, for the seek badge ("+30 s"); null when none.</summary>
    public long? BadgeAt { get; private set; }

    /// <summary>Counts the presses: the badge and the settling wait start again from each.</summary>
    public int Presses { get; private set; }

    /// <summary>Counts the seeks made: OK confirms one for <see cref="ConfirmMs"/> after.</summary>
    public int Seeks { get; private set; }

    /// <summary>A press of Left (<paramref name="direction"/> −1) or Right (+1), or a jump of <paramref name="stepMs"/>.</summary>
    public void Press(int direction, long stepMs = StepMs)
    {
        long from;
        if (Target is long t) from = t;
        else
        {
            from = position();
            asked = 0;
        }
        // The badge says what was asked for ("−20 s"), even where the start or end cuts it short.
        asked += direction * stepMs;
        Target = Clamp(from + direction * stepMs);
        BadgeAt = asked;
        Presses++;
    }

    /// <summary>The key repeats at <paramref name="eventTime"/>: it's held, and the target runs on.</summary>
    public void Repeat(int direction, long eventTime)
    {
        if (!Holding)
        {
            Target ??= position();
            Holding = true;
            holdStart = eventTime;
            lastEvent = eventTime;
            BadgeAt = null;
            return;
        }
        var dt = Math.Max(0, Math.Min(MaxGapMs, eventTime - lastEvent));
        lastEvent = eventTime;
        var rate = RateAt(eventTime - holdStart);
        Target = Clamp((Target ?? position()) + direction * (long)(rate * dt));
    }

    /// <summary>
    /// The key is let go: a scrub waits where it got to, as presses do, settling from now (it
    /// counts as a press). Returns whether a key was held.
    /// </summary>
    public bool Release()
    {
        if (!Holding) return false;
        Holding = false;
        Presses++;
        return true;
    }

    /// <summary><see cref="SettleMs"/> after press number <paramref name="press"/>: seeks, unless more presses came or a key is held.</summary>
    public void Settle(int press)
    {
        if (press != Presses || Holding) return;
        Commit();
    }

    /// <summary>The badge has shown long enough.</summary>
    public void HideBadge(int press)
    {
        if (press == Presses) BadgeAt = null;
    }

    /// <summary>
    /// OK pressed while a seek is coming, or just after one: as on most TV players, OK after Left
    /// and Right says "go there", not "pause". A coming seek is made now. Returns whether OK
    /// confirmed a seek; if not, it plays or pauses as usual.
    /// </summary>
    public bool Confirm()
    {
        if (Target != null)
        {
            Holding = false;
            Commit();
            // Confirmed: the next OK pauses.
            confirmable = false;
            return true;
        }
        var confirmed = confirmable;
        confirmable = false;
        return confirmed;
    }

    /// <summary><see cref="ConfirmMs"/> after seek number <paramref name="seekNumber"/>: OK plays and pauses again.</summary>
    public void Settled(int seekNumber)
    {
        if (seekNumber == Seeks) confirmable = false;
    }

    /// <summary>Forgets a seek on its way (the playback changed under it).</summary>
    public void Cancel()
    {
        Target = null;
        Holding = false;
        BadgeAt = null;
        confirmable = false;
    }

    void Commit()
    {
        if (Target is not long to) return;
        Target = null;
        seek(to);
        confirmable = true;
        Seeks++;
    }

    long Clamp(long ms)
    {
        var d = duration();
        return d > 0 ? Math.Max(0, Math.Min(ms, Math.Max(0, d - EndMarginMs))) : Math.Max(0, ms);
    }

    /// <summary>How fast a held key scrubs, in ms of video per ms held: smoothly faster the longer it's held, up to a top that grows with the video.</summary>
    public double RateAt(long heldMs) =>
        Math.Min(HoldStartRate * Math.Pow(2, heldMs / (double)HoldDoublesMs), Math.Max(HoldTopRate, duration() / (double)FullSweepMs));
}
