using System;
using System.Collections.Generic;

namespace Pley.Tv.Core.Ui;

/// <summary>
/// D-pad rule 5, held keys (the Android app's KeyRepeatGate). A press moves focus once. Held for
/// 180 ms, focus moves on the app's own clock (<see cref="Tick"/>) at a flat pace: sooner and more
/// evenly than the TV's repeats, which begin after 400 ms, come unevenly, and only say the key is
/// still down. A row scrolls under a held key as one continuous slide at that pace, the PSP's XMB
/// way, until <see cref="HoldEnded"/>; a page's rows step, each glide lasting a little over the
/// time between moves (<see cref="GlideMs"/>), so each runs into the next. Backdrops and details
/// wait until focus has rested 400 ms (<see cref="Rested"/>).
/// NUI's key events carry no repeat count: a key that goes down again before coming up repeats.
/// </summary>
public sealed class KeyRepeatGate
{
    /// <summary>How long a key is down before focus starts moving by itself.</summary>
    public const long HoldDelayMs = 180;
    /// <summary>The time between moves while a key is held: about 22 a second.</summary>
    public const long MoveIntervalMs = 45;
    /// <summary>How long focus rests before backdrops and details follow it.</summary>
    public const long RestMs = 400;
    /// <summary>Repeats come every 50 ms; this long without one means the key is up.</summary>
    const long LostKeyUpMs = 500;

    static readonly HashSet<string> Directions = new HashSet<string> { "Up", "Down", "Left", "Right" };

    readonly Func<long> clock;
    readonly HashSet<string> down = new HashSet<string>();
    long lastMove = long.MinValue / 2;
    long nextMove;
    long lastKey = long.MinValue / 2;
    /// <summary>When any key last went down, or the app came back to the front.</summary>
    long lastTouch = long.MinValue / 2;
    long pressedAt;
    string? held;
    /// <summary>The direction pressed and not let go yet: held once <see cref="HoldDelayMs"/> has passed.</summary>
    string? pressed;

    /// <param name="clock">Milliseconds; by default since the gate was made.</param>
    public KeyRepeatGate(Func<long>? clock = null)
    {
        var watch = System.Diagnostics.Stopwatch.StartNew();
        this.clock = clock ?? (() => watch.ElapsedMilliseconds);
    }

    /// <summary>Whether a direction key is held down and moving focus by itself now.</summary>
    public bool Holding { get; private set; }

    /// <summary>Whether a direction key is down: <see cref="Tick"/> is worth calling.</summary>
    public bool Down => pressed != null || Holding;

    /// <summary>The time between moves while a key is held.</summary>
    public long Interval => MoveIntervalMs;

    /// <summary>How long since a key last went down (or <see cref="Touched"/>): news waits for the remote to rest, so it never takes a press meant for the page.</summary>
    public long QuietFor() => clock() - lastTouch;

    /// <summary>The app came back to the front: as a press, so what waits for the remote to rest waits from now.</summary>
    public void Touched() => lastTouch = clock();

    /// <summary>A hold ended: the key came up, or its key-up was lost. A row sliding under it settles.</summary>
    public event Action? HoldEnded;

    /// <summary>How long a held move's glide lasts: a little over <see cref="Interval"/>, so the next starts before it ends.</summary>
    public int GlideMs => (int)(Interval * 5 / 4);

    /// <summary>
    /// Whether a key event goes through: true lets it act. A press of a direction does; its repeats
    /// don't, as <see cref="Tick"/> moves focus while it is held. Only direction keys are gated.
    /// </summary>
    public bool Admit(string key, bool isDown)
    {
        if (isDown) lastTouch = clock();
        if (!Directions.Contains(key)) return true;
        var now = clock();
        if (isDown)
        {
            lastKey = now;
            if (down.Add(key))
            {
                Holding = false;
                held = null;
                pressed = key;
                pressedAt = now;
                lastMove = now;
                return true;
            }
            if (!Holding || held != key) Hold(key, now);
            return false;
        }
        down.Remove(key);
        if (pressed == key) pressed = null;
        var wasHolding = Holding;
        Holding = false;
        held = null;
        lastMove = now;
        if (wasHolding) HoldEnded?.Invoke();
        return true;
    }

    /// <summary>
    /// Called every frame or so while a key is down: the direction to move focus now, or null.
    /// Moves come <see cref="Interval"/> apart on average, whenever the frames fall.
    /// </summary>
    public string? Tick()
    {
        ExpireHold();
        var now = clock();
        // Down long enough: the hold begins, before the TV's own repeats would.
        if (!Holding && pressed != null && down.Contains(pressed) && now - pressedAt >= HoldDelayMs) Hold(pressed, now);
        if (!Holding || held == null || now < nextMove) return null;
        // Kept to the pace: a late frame doesn't push the next move later, nor does a stall bring a burst.
        nextMove = Math.Max(nextMove + Interval, now + Interval / 2);
        lastMove = now;
        return held;
    }

    void Hold(string key, long now)
    {
        Holding = true;
        held = key;
        // The first held move is due at once.
        nextMove = now;
    }

    /// <summary>Whether focus has rested: no key held, and nothing moved for <see cref="RestMs"/>.</summary>
    public bool Rested()
    {
        ExpireHold();
        return !Holding && clock() - lastMove >= RestMs;
    }

    /// <summary>Milliseconds until focus has rested: 0 once it has; while a key is held, a short poll.</summary>
    public long UntilRest()
    {
        ExpireHold();
        if (Holding) return 50;
        return Math.Max(0, lastMove + RestMs - clock());
    }

    /// <summary>The app lost the key-up (focus left it mid-hold): the hold is over.</summary>
    void ExpireHold()
    {
        if (Down && clock() - lastKey > LostKeyUpMs)
        {
            var wasHolding = Holding;
            Holding = false;
            held = null;
            pressed = null;
            down.Clear();
            lastMove = lastKey;
            if (wasHolding) HoldEnded?.Invoke();
        }
    }
}
