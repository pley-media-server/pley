using System;
using System.Diagnostics;

namespace Pley.Tv.Core;

/// <summary>
/// Moments in Core a profiling build marks (the Tizen app's Kit/Profile.cs listens). Any other
/// build is compiled without them: each <see cref="Note"/> is left out where it's made, its text
/// with it, and <see cref="On"/> is always false.
/// </summary>
public static class Marks
{
#if PROFILE
    /// <summary>Hears each mark; set by a profiling build.</summary>
    public static Action<string>? Listener { get; set; }

    public static bool On => Listener != null;
#else
    public static bool On => false;
#endif

    /// <summary>Marks <paramref name="what"/>; build the text after checking <see cref="On"/>.</summary>
    [Conditional("PROFILE")]
    public static void Note(string what)
    {
#if PROFILE
        Listener?.Invoke(what);
#endif
    }
}
