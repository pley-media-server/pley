using System.Collections.Generic;
using Tizen.NUI;
using Tizen.NUI.BaseComponents;

namespace Pley.Tv.Tizen.Kit;

/// <summary>
/// A block of labels drawn again in place (a page's hero): the new one goes in over the old, and
/// the old comes out a few frames later. Emptied and filled again, the labels showed nothing for a
/// frame while their text was laid out, and the hero blinked as a page came back from a film.
/// </summary>
sealed class Redrawn
{
    /// <summary>How long the old block stays under the new one: a few frames, for the new text to be drawn.</summary>
    const uint SwapMs = 60;

    readonly View parent;
    /// <summary>The blocks on their way out, with their timers (held, or a timer collected never ticks).</summary>
    readonly List<Timer> going = new List<Timer>();

    public Redrawn(View parent)
    {
        this.parent = parent;
        Current = new View();
        parent.Add(Current);
    }

    /// <summary>The block shown.</summary>
    public View Current { get; private set; }

    /// <summary>A new, empty block to fill, shown at once; the one before goes once it is drawn.</summary>
    public View Next()
    {
        var old = Current;
        Current = new View();
        parent.Add(Current);
        var timer = new Timer(SwapMs);
        timer.Tick += (_, _) =>
        {
            going.Remove(timer);
            parent.Remove(old);
            old.Dispose();
            return false;
        };
        going.Add(timer);
        timer.Start();
        return Current;
    }

    /// <summary>The page goes: blocks on their way out go with it.</summary>
    public void Stop()
    {
        foreach (var timer in going) timer.Stop();
        going.Clear();
    }
}
