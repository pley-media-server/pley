using System;

namespace Pley.Tv.Core;

/// <summary>
/// Moments in Core a profiling build marks (the Tizen app's Kit/Profile.cs listens): nothing
/// listens in any other build, so a mark costs a check of <see cref="On"/>.
/// </summary>
public static class Marks
{
    /// <summary>Hears each mark; set by a profiling build only.</summary>
    public static Action<string>? Listener { get; set; }

    public static bool On => Listener != null;

    /// <summary>Marks <paramref name="what"/>; build the text after checking <see cref="On"/>.</summary>
    public static void Note(string what) => Listener?.Invoke(what);
}
