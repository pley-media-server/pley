using System;
using System.Collections.Generic;
using Pley.Tv.Core.Api;

namespace Pley.Tv.Core.Player;

/// <summary>What a player plays, for the remote: a title, music, or a live channel.</summary>
public enum RemoteKind
{
    Video,
    Music,
    Live,
}

/// <summary>
/// What a player is doing, as much as a command of the remote needs.
/// <paramref name="Session"/> is the session it plays now, null while it has none;
/// <paramref name="DurationMs"/> is null while the length isn't known; <paramref name="HasNext"/>:
/// something may come after it (an episode, the queue's next); <paramref name="Index"/>: the
/// track's place in the music's play order; <paramref name="HasOrder"/>: a live player has an
/// order of channels to go through.
/// </summary>
public sealed record RemotePlayer(
    RemoteKind Kind,
    string? Session,
    bool Playing = false,
    long PositionMs = 0,
    long? DurationMs = null,
    bool HasNext = false,
    int Index = 0,
    bool HasOrder = false);

/// <summary>What a player does with a command of the remote.</summary>
public enum RemoteAct
{
    None,
    Play,
    Pause,
    /// <summary>To <see cref="RemoteAction.PositionMs"/>.</summary>
    Seek,
    Next,
    Previous,
    /// <summary>Channel up (<see cref="RemoteAction.By"/> 1) or down (-1).</summary>
    Channel,
}

/// <summary>
/// What a player does with a command, and whether it tells the server where it is at once
/// (<paramref name="Report"/>), so the remote sees it: a play, pause or seek does (a pause as any
/// pause does); another track, title or channel tells the server with its own session. A play
/// while playing, or a pause while paused, changes nothing but says where it is again, so a remote
/// a little behind settles.
/// </summary>
public sealed record RemoteAction(RemoteAct Act, bool Report, long PositionMs = 0, int By = 0)
{
    public static readonly RemoteAction None = new RemoteAction(RemoteAct.None, false);

    /// <summary>Playing or paused already: said again.</summary>
    public static readonly RemoteAction SaidAgain = new RemoteAction(RemoteAct.None, true);
}

/// <summary>
/// The profile's remote, on another of its devices (docs/API.md, "Remote control"): what each
/// player here obeys, and what it does with a command. The TV obeys and shows no remote of its
/// own. The rules are the Android app's and the web's, pinned for both Samsung apps by
/// packages/tv/test/fixtures/remote.json.
/// </summary>
public static class Remote
{
    static readonly string[] VideoCommands = { "play", "pause", "seek", "skip" };
    static readonly string[] VideoWithNext = { "play", "pause", "seek", "skip", "next" };

    /// <summary>What music obeys: everything, as the buttons do it.</summary>
    public static readonly IReadOnlyList<string> MusicControls = new[] { "play", "pause", "seek", "skip", "next", "previous" };

    /// <summary>Live is watched as it airs here: no pause or seek, only channel up and down.</summary>
    static readonly string[] LiveCommands = { "next", "previous" };

    /// <summary>What a video obeys: next only when something may come after it.</summary>
    public static IReadOnlyList<string> VideoControls(bool hasNext) => hasNext ? VideoWithNext : VideoCommands;

    /// <summary>What a live channel obeys: channel up and down when the player has an order to go through, else nothing but Stop.</summary>
    public static IReadOnlyList<string> LiveControls(bool hasOrder) => hasOrder ? LiveCommands : Array.Empty<string>();

    /// <summary>
    /// The rule every receiver follows: a command for another session is someone else's player's;
    /// play while playing and pause while paused say again what it does; a seek stays within the title and a skip counts from where it is; next with nothing next,
    /// or previous on a video, does nothing; previous on music starts the track again past 3 s or
    /// on the first track, else plays the one before; a live channel's next and previous are
    /// channel up and down. A command the player didn't declare does nothing.
    /// </summary>
    public static RemoteAction Action(RemotePlayer p, ServerEvent c)
    {
        if (p.Session == null || c.SessionId != p.Session) return RemoteAction.None;
        if (p.Kind == RemoteKind.Live)
        {
            if (!p.HasOrder) return RemoteAction.None;
            return c.Command switch
            {
                "next" => new RemoteAction(RemoteAct.Channel, false, By: 1),
                "previous" => new RemoteAction(RemoteAct.Channel, false, By: -1),
                _ => RemoteAction.None,
            };
        }
        switch (c.Command)
        {
            case "play":
                return p.Playing ? RemoteAction.SaidAgain : new RemoteAction(RemoteAct.Play, true);
            case "pause":
                return p.Playing ? new RemoteAction(RemoteAct.Pause, true) : RemoteAction.SaidAgain;
            case "seek":
                return c.PositionMs is long at ? new RemoteAction(RemoteAct.Seek, true, Within(at, p.DurationMs)) : RemoteAction.None;
            case "skip":
                return c.OffsetMs is long by ? new RemoteAction(RemoteAct.Seek, true, Within(p.PositionMs + by, p.DurationMs)) : RemoteAction.None;
            case "next":
                return p.HasNext ? new RemoteAction(RemoteAct.Next, false) : RemoteAction.None;
            case "previous":
                if (p.Kind == RemoteKind.Video) return RemoteAction.None;
                return RestartsTrack(p.PositionMs, p.Index) ? new RemoteAction(RemoteAct.Seek, true, 0) : new RemoteAction(RemoteAct.Previous, false);
            default:
                // A command a newer server knows.
                return RemoteAction.None;
        }
    }

    /// <summary>
    /// Previous starts the track again (rather than playing the one before) past 3 s, or on the
    /// first track: the remote's and music's own button (<see cref="MusicQueue.Previous"/>).
    /// </summary>
    public static bool RestartsTrack(long positionMs, int index) => positionMs > MusicQueue.RestartAfterMs || index == 0;

    /// <summary>From the start to the end; past the start only while the length isn't known.</summary>
    static long Within(long ms, long? durationMs)
    {
        var start = Math.Max(0, ms);
        return durationMs is long d && d > 0 ? Math.Min(start, d) : start;
    }
}
