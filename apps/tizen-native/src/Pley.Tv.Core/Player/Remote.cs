using System;
using System.Collections.Generic;
using System.Linq;
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
/// A video's tracks, as much as the remote's audio and subtitles need. <paramref name="Session"/> is
/// the session it plays now, null while it has none; <paramref name="Audio"/> and
/// <paramref name="Subtitles"/> are the stream indexes of its file's tracks;
/// <paramref name="AudioIndex"/> is the audio chosen (null while not known) and
/// <paramref name="SubtitleIndex"/> the subtitles (-1: off).
/// </summary>
public sealed record TrackPlayer(string? Session, IReadOnlyList<int> Audio, IReadOnlyList<int> Subtitles, int? AudioIndex, int SubtitleIndex);

/// <summary>What a video does with the remote's audio or subtitles.</summary>
public enum TrackAct
{
    None,
    /// <summary>To the audio track <see cref="TrackAction.Index"/>.</summary>
    Audio,
    /// <summary>To the subtitles <see cref="TrackAction.Index"/>; -1 turns them off.</summary>
    Subtitles,
}

/// <summary>
/// What a video does with the remote's audio or subtitles: switch to <paramref name="Index"/> as its
/// own track panels do, and report the tracks it plays then; or nothing, saying again what it plays
/// (<paramref name="Report"/>) when the pick is the one it has, so a remote a little behind settles.
/// </summary>
public sealed record TrackAction(TrackAct Act, bool Report, int Index = 0)
{
    public static readonly TrackAction None = new TrackAction(TrackAct.None, false);

    /// <summary>Chosen already: said again.</summary>
    public static readonly TrackAction Same = new TrackAction(TrackAct.None, true);
}

/// <summary>
/// A video's qualities, as much as the remote's quality needs: <paramref name="Session"/> is the
/// session it plays now, null while it has none; <paramref name="Offered"/> the ids its Quality menu
/// offers for the file; <paramref name="Picked"/> the one picked there.
/// </summary>
public sealed record QualityPlayer(string? Session, IReadOnlyList<string> Offered, string Picked);

/// <summary>
/// What a video does with the remote's quality: switch to <paramref name="Id"/> as picking it in its
/// Quality menu does, and report; or nothing (<paramref name="Id"/> null), saying again what it plays
/// (<paramref name="Report"/>) when the pick is the one it has, so a remote a little behind settles.
/// </summary>
public sealed record QualityAction(string? Id, bool Report)
{
    public static readonly QualityAction None = new QualityAction(null, false);

    /// <summary>Picked already: said again.</summary>
    public static readonly QualityAction Same = new QualityAction(null, true);
}

/// <summary>
/// The profile's remote, on another of its devices (docs/API.md, "Remote control"): what each
/// player here obeys, and what it does with a command. The TV obeys and shows no remote of its
/// own. The rules are the Android app's and the web's, pinned for both Samsung apps by
/// packages/tv/test/fixtures/remote.json, and tracks.json for the tracks.
/// </summary>
public static class Remote
{
    static readonly string[] VideoCommands = { "play", "pause", "seek", "skip" };
    static readonly string[] VideoWithNext = { "play", "pause", "seek", "skip", "next" };

    /// <summary>What music obeys: everything, as the buttons do it.</summary>
    public static readonly IReadOnlyList<string> MusicControls = new[] { "play", "pause", "seek", "skip", "next", "previous" };

    /// <summary>Live is watched as it airs here: no pause or seek, only channel up and down.</summary>
    static readonly string[] LiveCommands = { "next", "previous" };

    /// <summary>
    /// What a video obeys: next only when something may come after it; audio and subtitles when its
    /// file has tracks of them (<paramref name="audio"/> and <paramref name="subtitles"/> count them),
    /// which a remote picks from; the quality when the player has a Quality menu for it
    /// (<paramref name="quality"/>: not for a file on a drive, which nothing converts).
    /// </summary>
    public static IReadOnlyList<string> VideoControls(bool hasNext, int audio = 0, int subtitles = 0, bool quality = false)
    {
        var controls = new List<string>(hasNext ? VideoWithNext : VideoCommands);
        if (audio > 0) controls.Add("audio");
        if (subtitles > 0) controls.Add("subtitles");
        if (quality) controls.Add("quality");
        return controls;
    }

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
    /// The tracks' rule: a command for another session is someone else's player's; audio to one of
    /// the file's audio tracks switches to it, and subtitles to one of its subtitles, or -1 to none;
    /// a pick that is the one chosen already is said again; a track the file doesn't have (or a
    /// command without one) does nothing.
    /// </summary>
    public static TrackAction Track(TrackPlayer p, ServerEvent c)
    {
        if (p.Session == null || c.SessionId != p.Session || c.StreamIndex is not int index) return TrackAction.None;
        switch (c.Command)
        {
            case "audio":
                if (!p.Audio.Contains(index)) return TrackAction.None;
                return index == p.AudioIndex ? TrackAction.Same : new TrackAction(TrackAct.Audio, true, index);
            case "subtitles":
                if (index != -1 && !p.Subtitles.Contains(index)) return TrackAction.None;
                return index == p.SubtitleIndex ? TrackAction.Same : new TrackAction(TrackAct.Subtitles, true, index);
            default:
                return TrackAction.None;
        }
    }

    /// <summary>
    /// The quality's rule: a command for another session is someone else's player's; a quality its
    /// Quality menu offers for the file switches to it, as picking it there does; the one picked
    /// already is said again; one it doesn't offer (or a command without one) does nothing.
    /// </summary>
    public static QualityAction Quality(QualityPlayer p, ServerEvent c)
    {
        if (p.Session == null || c.SessionId != p.Session || c.Command != "quality" || c.Quality is not string id) return QualityAction.None;
        if (!p.Offered.Contains(id)) return QualityAction.None;
        return id == p.Picked ? QualityAction.Same : new QualityAction(id, true);
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
