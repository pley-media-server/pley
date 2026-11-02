using System.Collections.Generic;
using Tizen.NUI;

namespace Pley.Tv.Tizen.Kit;

/// <summary>
/// Animations nothing else holds, held while they run. One kept only by a local was collected
/// while it ran, and the engine's next call into it closed the app (a film's title fading as it
/// began, a tile's ring as focus moved).
/// </summary>
static class Held
{
    static readonly HashSet<Animation> Running = new HashSet<Animation>();

    /// <summary>Plays <paramref name="animation"/>, held until it finishes; not for one that loops.</summary>
    public static Animation PlayHeld(this Animation animation)
    {
        if (Running.Add(animation)) animation.Finished += (s, e) => Running.Remove(animation);
        animation.Play();
        return animation;
    }
}
