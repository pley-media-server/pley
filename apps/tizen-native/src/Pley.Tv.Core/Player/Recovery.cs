using System.Collections.Generic;
using System.Linq;

namespace Pley.Tv.Core.Player;

/// <summary>Why a fallback plays something other than the original.</summary>
public enum FallbackReason
{
    /// <summary>The device couldn't play the original file.</summary>
    Unplayable,
    /// <summary>The original didn't play smoothly here.</summary>
    NotSmooth,
}

/// <summary>Why playback gave up.</summary>
public enum Failure
{
    /// <summary>Neither the original nor a conversion plays here.</summary>
    Unplayable,
    /// <summary>It stops at the same spot each time.</summary>
    Stuck,
    /// <summary>The media ends before the point it was to play from.</summary>
    Short,
    /// <summary>It stopped and couldn't get going again.</summary>
    Stopped,
    /// <summary>The server couldn't be reached.</summary>
    Network,
    /// <summary>The engine failed in a way no new session helps with.</summary>
    Engine,
}

/// <summary>What the recovery ladder says to do.</summary>
public abstract record RecoveryAction
{
    /// <summary>A new session at the position; <paramref name="Mode"/> is asked for from now on when set.</summary>
    public sealed record NewSession(string? Mode, FallbackReason? Reason) : RecoveryAction;

    /// <summary>Ask the server how its transcode is doing, then call <see cref="Recovery.AfterServerCheck"/>.</summary>
    public sealed record CheckServer : RecoveryAction;

    /// <summary>The server can't keep up: offer a lower quality, keep playing.</summary>
    public sealed record SuggestLowerQuality : RecoveryAction;

    public sealed record Fail(Failure Failure, string? Detail) : RecoveryAction;
}

/// <summary>
/// The recovery ladder (the Android app's Recovery, the web's VideoPlayer.tsx):
/// <list type="bullet">
/// <item>Direct play that the device can't decode or open: a remux when the video is H.264 or
/// HEVC and only the container or the audio failed, else a transcode; each once.</item>
/// <item>404 or 410 ("playback session expired") or an unreadable playlist: a new session at the
/// position, at most 3 times.</item>
/// <item>Network errors: a new session, at most 3.</item>
/// <item>A 12 s stall: a direct play that has data but doesn't move becomes a transcode; a stream
/// asks the server first (<see cref="RecoveryAction.CheckServer"/>); a stream that keeps
/// stopping gets one new session, a starving one up to 3.</item>
/// <item>Media that ends where it started, long before its end: a direct play becomes a
/// transcode, a stream gets one new session, then it fails. It never counts as the end.</item>
/// </list>
/// Budgets start over after 5 s of playback and on Retry. The UI thread only.
/// </summary>
public sealed class Recovery
{
    public const long RecoveredAfterMs = 5_000;
    public const int MaxRecreate = 3;
    public const int MaxNetwork = 3;
    public const int MaxStuck = 1;
    public const int MaxShort = 1;

    /// <summary>Videos a remux copies, which the device's decoders take in another container.</summary>
    static readonly HashSet<string> CopyableVideo = new HashSet<string> { "h264", "hevc" };

    bool remuxTried;
    bool transcodeTried;
    long? playedFrom;

    /// <summary>The mode asked for instead of the quality's, after a fallback; null without one.</summary>
    public string? FallbackMode { get; private set; }

    /// <summary>Why <see cref="FallbackMode"/> is set.</summary>
    public FallbackReason? FallbackReason { get; private set; }

    /// <summary>The automatic switch for dropped frames happened (once per item).</summary>
    public bool StruggleSwitched { get; private set; }

    /// <summary>New sessions for expired sessions and starving stalls.</summary>
    public int Recreate { get; private set; }

    /// <summary>New sessions after network errors.</summary>
    public int Network { get; private set; }

    /// <summary>New sessions for media that arrives and doesn't play.</summary>
    public int Stuck { get; private set; }

    /// <summary>New sessions for media that ends short of where it was to play.</summary>
    public int Short { get; private set; }

    /// <summary>Where playback is now: 5 s played on from where it last (re)started means the trouble is over.</summary>
    public void OnPosition(long positionMs)
    {
        if (playedFrom is not long from || positionMs < from) playedFrom = positionMs;
        else if (positionMs - from >= RecoveredAfterMs)
        {
            Recreate = 0;
            Network = 0;
            Stuck = 0;
            Short = 0;
            playedFrom = positionMs;
        }
    }

    /// <summary>A seek or a new session: progress counts from where it lands.</summary>
    public void OnJump() => playedFrom = null;

    /// <summary>What to do about <paramref name="error"/> in a session played as <paramref name="method"/>, whose video is <paramref name="videoCodec"/>.</summary>
    public RecoveryAction Decide(EngineError error, string method, string? videoCodec) => error switch
    {
        EngineError.Unsupported u => Fallback(method, videoCodec, u.Audio || u.Container, u.Detail),
        EngineError.Http h => h.Status is 404 or 410 ? Expired(h.Detail) : NetworkError(h.Detail),
        EngineError.Playlist p => Expired(p.Detail),
        EngineError.Network n => NetworkError(n.Detail),
        // The session handles a stop before asking: this is only a fallback.
        EngineError.Stopped s => new RecoveryAction.Fail(Failure.Stopped, s.Detail),
        _ => new RecoveryAction.Fail(Failure.Engine, error.Detail),
    };

    RecoveryAction Fallback(string method, string? videoCodec, bool partial, string detail)
    {
        var copyable = videoCodec != null && CopyableVideo.Contains(videoCodec.ToLowerInvariant());
        if (method == "direct" && partial && copyable && !remuxTried)
        {
            remuxTried = true;
            return SwitchTo("remux", Player.FallbackReason.Unplayable);
        }
        if (method != "transcode" && !transcodeTried)
        {
            transcodeTried = true;
            return SwitchTo("transcode", Player.FallbackReason.Unplayable);
        }
        return new RecoveryAction.Fail(Failure.Unplayable, detail);
    }

    RecoveryAction SwitchTo(string mode, FallbackReason reason)
    {
        FallbackMode = mode;
        FallbackReason = reason;
        return new RecoveryAction.NewSession(mode, reason);
    }

    RecoveryAction Expired(string detail)
    {
        if (Recreate >= MaxRecreate) return new RecoveryAction.Fail(Failure.Stopped, detail);
        Recreate++;
        return new RecoveryAction.NewSession(null, null);
    }

    RecoveryAction NetworkError(string detail)
    {
        if (Network >= MaxNetwork) return new RecoveryAction.Fail(Failure.Network, detail);
        Network++;
        return new RecoveryAction.NewSession(null, null);
    }

    /// <summary>
    /// Playback stood still for 12 s while meant to play. <paramref name="starving"/> when it waits
    /// for data (nothing buffered ahead), rather than for a decoder.
    /// </summary>
    public RecoveryAction OnStall(string method, bool starving)
    {
        if (!starving && method == "direct" && !transcodeTried)
        {
            transcodeTried = true;
            return SwitchTo("transcode", Player.FallbackReason.NotSmooth);
        }
        if (method != "direct") return new RecoveryAction.CheckServer();
        return AfterStall(starving);
    }

    /// <summary>The server's transcode runs at <paramref name="speed"/> (null when it can't say): too slow, or try a new session.</summary>
    public RecoveryAction AfterServerCheck(double? speed, bool starving) =>
        speed is double s && s < 1.0 ? new RecoveryAction.SuggestLowerQuality() : AfterStall(starving);

    RecoveryAction AfterStall(bool starving)
    {
        if (!starving)
        {
            // Media that arrives and doesn't play: one new session, the same again would only loop.
            if (Stuck >= MaxStuck) return new RecoveryAction.Fail(Failure.Stuck, null);
            Stuck++;
            return new RecoveryAction.NewSession(null, null);
        }
        if (Recreate >= MaxRecreate) return new RecoveryAction.Fail(Failure.Stopped, null);
        Recreate++;
        return new RecoveryAction.NewSession(null, null);
    }

    /// <summary>
    /// The media ended right where it was started (or sought to), long before the end the server
    /// gives: it doesn't hold that part. A direct play is asked again as a transcode, the server
    /// reading what the device can't; a stream gets one new session; then it fails.
    /// </summary>
    public RecoveryAction OnEndedShort(string method)
    {
        if (method == "direct" && !transcodeTried)
        {
            transcodeTried = true;
            return SwitchTo("transcode", Player.FallbackReason.Unplayable);
        }
        if (Short >= MaxShort) return new RecoveryAction.Fail(Failure.Short, null);
        Short++;
        return new RecoveryAction.NewSession(null, null);
    }

    /// <summary>
    /// The decoder drops frames: one switch to a transcode per item, with "Play original" offered;
    /// null when it switched already or transcodes.
    /// </summary>
    public RecoveryAction? OnStruggling(string method)
    {
        if (StruggleSwitched || method == "transcode") return null;
        StruggleSwitched = true;
        transcodeTried = true;
        return SwitchTo("transcode", Player.FallbackReason.NotSmooth);
    }

    /// <summary>"Play original" after a fallback: the quality's own mode again (dropped frames don't switch twice).</summary>
    public void PlayOriginal()
    {
        FallbackMode = null;
        FallbackReason = null;
    }

    /// <summary>Another quality chosen: its own mode, as on the web.</summary>
    public void QualityChanged() => PlayOriginal();

    /// <summary>Retry after an error: every new-session budget again.</summary>
    public void Retry()
    {
        Recreate = 0;
        Network = 0;
        Stuck = 0;
        Short = 0;
        playedFrom = null;
    }

    /// <summary>Another item: everything starts over.</summary>
    public void NewItem()
    {
        Retry();
        FallbackMode = null;
        FallbackReason = null;
        StruggleSwitched = false;
        remuxTried = false;
        transcodeTried = false;
    }
}

/// <summary>The decoder's frame counts: shown, dropped, and the most dropped in a row (0 when the engine can't say).</summary>
public sealed record FrameCounts(long Rendered, long Dropped, int MaxConsecutiveDropped = 0);

/// <summary>
/// Whether the video decoder keeps up (the Android app's StruggleMeter): it struggles when it
/// drops over 1 % of its frames in 10 s of playback, or 10 frames in a row. Fed the engine's
/// counts every few seconds while playing.
/// </summary>
public sealed class StruggleMeter
{
    public const long WindowMs = 10_000;
    public const double MaxShare = 0.01;
    public const int MaxRun = 10;

    readonly List<(long At, FrameCounts Counts)> samples = new List<(long, FrameCounts)>();

    /// <summary>Takes the counts at <paramref name="atMs"/> (ms of playback); true when the decoder struggles.</summary>
    public bool Sample(long atMs, FrameCounts counts)
    {
        if (samples.Count > 0)
        {
            var last = samples[samples.Count - 1].Counts;
            // The counts start over with each decoder.
            if (counts.Rendered < last.Rendered || counts.Dropped < last.Dropped) samples.Clear();
        }
        var baseline = samples.Count > 0 ? samples[0].Counts : null;
        samples.Add((atMs, counts));
        if (baseline != null && counts.MaxConsecutiveDropped >= MaxRun && counts.MaxConsecutiveDropped > baseline.MaxConsecutiveDropped) return true;
        while (samples.Count > 2 && samples[1].At <= atMs - WindowMs) samples.RemoveAt(0);
        var (fromMs, from) = samples[0];
        if (atMs - fromMs < WindowMs) return false;
        var dropped = counts.Dropped - from.Dropped;
        var shown = counts.Rendered - from.Rendered;
        var total = dropped + shown;
        return total > 0 && (double)dropped / total > MaxShare;
    }

    /// <summary>A new decoder, a seek or a pause: the next window starts afresh.</summary>
    public void Reset() => samples.Clear();
}
