using System;
using System.Collections.Generic;

namespace Pley.Tv.Core.Player;

/// <summary>The playback speeds offered (the Android app's PlaybackSpeeds): 0.5× to 2×, 1× normal, for one title at a time.</summary>
public static class PlaybackSpeeds
{
    public static readonly IReadOnlyList<double> All = new[] { 0.5, 0.75, 1, 1.25, 1.5, 2 };

    /// <summary>"1×", "1.25×", "0.5×" (in French "1,25×", as the web writes it).</summary>
    public static string Label(double speed) => speed.ToString("0.##", PlayerText.Numbers) + "×";
}

/// <summary>The audio delay's range and step (the Android app's AudioDelays).</summary>
public static class AudioDelays
{
    public const long MaxMs = 5_000;
    public const long StepMs = 50;

    /// <summary><paramref name="ms"/> within ±5 s, on a 50 ms step.</summary>
    public static long Clamp(long ms) => Math.Max(-MaxMs, Math.Min(MaxMs, SubtitleSync.Round(ms / (double)StepMs) * StepMs));

    /// <summary>"+0.25 s", "−1.5 s", "0 s": the sound later (+) or earlier (−) than the picture.</summary>
    public static string Label(long ms)
    {
        if (ms == 0) return Strings.T("player.units.seconds", ("value", "0"));
        var seconds = (Math.Abs(ms) / 1000m).ToString("0.###", PlayerText.Numbers);
        return Strings.T("player.units.seconds", ("value", (ms > 0 ? "+" : "−") + seconds));
    }
}
