using System.Collections.Generic;
using Pley.Tv.Core.Api;

namespace Pley.Tv.Core.Player;

public enum PhaseKind
{
    /// <summary>Asking the server and loading.</summary>
    Preparing,
    Buffering,
    Playing,
    Paused,
    /// <summary>The end, with nothing next: the player leaves.</summary>
    Ended,
    /// <summary>It stopped: <see cref="PlayerPhase.Message"/> says why.</summary>
    Error,
}

/// <summary>Where a playback stands (the Android app's PlayerPhase).</summary>
/// <param name="Message">An error's sentence for people.</param>
/// <param name="CanRetry">An error Try again may help with.</param>
/// <param name="Title">An error's heading: "Can't play this item", "Can't reach your server" or "Playback stopped".</param>
public sealed record PlayerPhase(PhaseKind Kind, string? Message = null, bool CanRetry = false, string? Title = null)
{
    public static readonly PlayerPhase Preparing = new PlayerPhase(PhaseKind.Preparing);
    public static readonly PlayerPhase Buffering = new PlayerPhase(PhaseKind.Buffering);
    public static readonly PlayerPhase Playing = new PlayerPhase(PhaseKind.Playing);
    public static readonly PlayerPhase Paused = new PlayerPhase(PhaseKind.Paused);
    public static readonly PlayerPhase Ended = new PlayerPhase(PhaseKind.Ended);

    /// <param name="title">The heading; "Can't play this item" (<see cref="PlayerText.ErrorTitle"/>) without.</param>
    public static PlayerPhase Error(string message, bool canRetry, string? title = null) => new PlayerPhase(PhaseKind.Error, message, canRetry, title ?? PlayerText.ErrorTitle);

    /// <summary>Meant to play (Play/Pause shows Pause): playing, or on its way to.</summary>
    public bool WantsToPlay => Kind is PhaseKind.Playing or PhaseKind.Buffering or PhaseKind.Preparing;
}

/// <summary>An audio or subtitle track as the panels show it.</summary>
/// <param name="Index">The server's stream index; null for "Off".</param>
/// <param name="BurnIn">Subtitles shown as pictures, which the server draws onto the video.</param>
public sealed record TrackChoice(int? Index, string Label, bool Selected, bool BurnIn = false);

/// <summary>A quality in the Quality panel.</summary>
/// <param name="Id">"original", "auto", "8m"…: what <see cref="PlayerModel.SelectQuality"/> takes.</param>
/// <param name="Hint">The original's "1080p · 8.2 Mbps", Automatic's "Dynamically adjust to conditions" or "Now 8 Mbps 1080p".</param>
public sealed record QualityChoice(string Id, string Label, string? Hint, bool Selected);

/// <summary>A chapter, for the Chapters panel and the scrubber's gaps.</summary>
/// <param name="ImageUrl">Absolute address of its picture, when the file has one.</param>
public sealed record ChapterMark(string? Title, long StartMs, long EndMs, string? ImageUrl);

/// <summary>A message at the top right: <paramref name="Ms"/> on screen, in danger text when <paramref name="Error"/>.</summary>
public sealed record PlayerToast(string Text, long Ms, bool Error = false);

/// <summary>Seek previews of the file playing, once the server has them.</summary>
public sealed record TrickplaySet(long FileId, TrickplayInfo Info);

/// <summary>
/// What the player screen shows, but for the clock and the subtitles, which change often and have
/// signals of their own (the Android app's PlayerUiState).
/// </summary>
public sealed record PlayerState
{
    public PlayerPhase Phase { get; init; } = PlayerPhase.Preparing;
    /// <summary>The item playing; 0 until known.</summary>
    public long ItemId { get; init; }
    /// <summary>"movie", "episode", "extra"; null until its details come.</summary>
    public string? Kind { get; init; }
    /// <summary>A movie's "Title (Year)", an episode's show.</summary>
    public string Title { get; init; } = "";
    /// <summary>An episode's "S1 · E2 · Title"; null for anything else.</summary>
    public string? Subtitle { get; init; }
    public IReadOnlyList<TrackChoice> Audio { get; init; } = new TrackChoice[0];
    /// <summary>Off first, then the file's tracks; empty when the file has none.</summary>
    public IReadOnlyList<TrackChoice> Subtitles { get; init; } = new TrackChoice[0];
    /// <summary>The quality chosen, as Info says it: "Original", "8 Mbps 1080p", "Automatic · Original".</summary>
    public string Quality { get; init; } = SessionText.QualityOriginal;
    public IReadOnlyList<QualityChoice> Qualities { get; init; } = new QualityChoice[0];
    /// <summary>"Direct play", "Direct stream", "Transcoding": the top bar's badge; null before the server answered.</summary>
    public string? Method { get; init; }
    /// <summary>Why it plays this way when the player chose it: "The original didn't play smoothly on this device."</summary>
    public string? Note { get; init; }
    /// <summary>The player switched to a transcode by itself: Quality offers "Play original".</summary>
    public bool CanPlayOriginal { get; init; }
    /// <summary>The server can't convert fast enough: a lower quality is suggested.</summary>
    public bool SuggestLowerQuality { get; init; }
    /// <summary>"Skip Intro" or "Skip Credits" while one is showing.</summary>
    public string? SkipLabel { get; init; }
    /// <summary>The playing file's intro and credits, for the scrubber.</summary>
    public IReadOnlyList<Marker> Markers { get; init; } = new Marker[0];
    public IReadOnlyList<ChapterMark> Chapters { get; init; } = new ChapterMark[0];
    /// <summary>Seek previews, once the server has them (<see cref="PlayerModel.Preview"/>).</summary>
    public TrickplaySet? Trickplay { get; init; }
    /// <summary>What plays next, during its card.</summary>
    public UpNextCard? UpNext { get; init; }
    /// <summary>How it plays, while the details are asked for (<see cref="PlayerModel.SetDetailsOpen"/>).</summary>
    public PlaybackDetails? Details { get; init; }
    /// <summary>How far the subtitles are moved: positive shows them later.</summary>
    public long SubtitleOffsetMs { get; init; }
    /// <summary>The subtitles chosen can be moved: text drawn by the app.</summary>
    public bool SubtitleSync { get; init; }
    /// <summary>The subtitles chosen are text, which the subtitles' look (size, background, font, position) is for: pictures keep their own.</summary>
    public bool SubtitleStyled { get; init; }
    /// <summary>The playback speed, 1 normal, for this title only.</summary>
    public double Speed { get; init; } = 1;
    /// <summary>The speeds the TV's player plays with their sound; empty when only normal speed (Speed isn't offered then).</summary>
    public IReadOnlyList<double> Speeds { get; init; } = new double[0];
    public SleepState Sleep { get; init; } = new SleepState();
    /// <summary>The server stopped this playback: the error says so, and the screen leaves after <see cref="PlayerText.StoppedLeaveMs"/>.</summary>
    public bool StoppedByServer { get; init; }
    /// <summary>The server stopped answering mid-playback: the player keeps its place and asks again ("Reconnecting…").</summary>
    public bool Reconnecting { get; init; }
    /// <summary>Time to the first frame of the session playing, in ms; null before it shows.</summary>
    public long? FirstFrameMs { get; init; }
    /// <summary>Times the video stopped to wait for more once it played, seeks aside (Info's Stalls).</summary>
    public int Stalls { get; init; }
}

/// <summary>Where the playhead is: changes on every tick, so the screen follows it on its own.</summary>
public sealed record PlayerClock(long PositionMs, long DurationMs, long BufferedMs);
