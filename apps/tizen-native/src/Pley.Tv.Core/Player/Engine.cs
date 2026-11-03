using System;
using System.Collections.Generic;

namespace Pley.Tv.Core.Player;

/// <summary>Where the TV's player is with what it was given.</summary>
public enum EngineStatus
{
    Idle,
    /// <summary>Waiting for data.</summary>
    Buffering,
    /// <summary>It has data and plays, or would if it weren't paused.</summary>
    Ready,
    /// <summary>It played to the end.</summary>
    Ended,
}

/// <summary>The video as it will arrive, so a TV can match its display mode before the first frame; null fields aren't known.</summary>
/// <param name="Range">"sdr", "hdr10", "hdr10plus", "hlg" or "dolby_vision": a conversion's is the server's to choose, so "sdr".</param>
public sealed record EngineVideo(string Codec, int? Width, int? Height, double? FrameRate, string Range, int? DvProfile, int BitDepth);

/// <summary>What the TV's player is given: an absolute address (keyed: it carries its own key), and whether it is a stream.</summary>
/// <param name="Live">A live channel's HLS: it starts at the live edge, whatever <paramref name="StartMs"/> says.</param>
/// <param name="DurationHintMs">How long the server says it is, for a player that knows only once it opened.</param>
/// <param name="Mime">The container's MIME type for a file played as it is; null for a stream or when not known.</param>
/// <param name="Video">The video as it will arrive (a film's or an episode's); null when not known.</param>
/// <param name="AudioCodec">The audio stream's codec when played as it is (passed through or decoded); null when the server converts it.</param>
public sealed record EngineSource(string Url, bool Hls, bool Live = false, long StartMs = 0, long? DurationHintMs = null,
    string? Mime = null, EngineVideo? Video = null, string? AudioCodec = null);

/// <summary>What went wrong in the TV's player, in the terms the controllers decide on.</summary>
public abstract record EngineError(string Detail)
{
    /// <summary>
    /// It can't decode or open this: <paramref name="Audio"/> when the audio failed,
    /// <paramref name="Container"/> when the file did (a video player falls back to a remux for those).
    /// </summary>
    public sealed record Unsupported(string Detail, bool Audio = false, bool Container = false) : EngineError(Detail);

    /// <summary>The server answered a playlist, segment or file with <paramref name="Status"/>.</summary>
    public sealed record Http(int Status, string Detail) : EngineError(Detail);

    /// <summary>410: stopped from elsewhere, with what the server said.</summary>
    public sealed record Stopped(string? Message) : EngineError(Message ?? "");

    /// <summary>A stream's playlist couldn't be read.</summary>
    public sealed record Playlist(string Detail) : EngineError(Detail);

    /// <summary>The network failed under it.</summary>
    public sealed record Network(string Detail) : EngineError(Detail);

    public sealed record Other(string Detail) : EngineError(Detail);
}

/// <summary>
/// The TV's own player as Live TV and music drive it (packages/tv's PlayerEngine): the Tizen app
/// wraps Tizen.Multimedia.Player in it, tests a fake. Its events come on the UI thread, after the
/// call that caused them has returned.
/// </summary>
public interface IMediaEngine
{
    event Action<EngineStatus>? StatusChanged;

    /// <summary>The first picture (or, with sound only, the first sound) of a source is out.</summary>
    event Action? Started;

    /// <summary>It plays, or stopped playing (paused, waiting, ended).</summary>
    event Action<bool>? PlayingChanged;

    event Action<EngineError>? Failed;

    /// <summary>Opens <paramref name="source"/> and starts at its start; plays at once when <paramref name="play"/>.</summary>
    void Prepare(EngineSource source, bool play);

    void Play();

    void Pause();

    void SeekTo(long ms);

    /// <summary>Stops and closes the source; another can be prepared.</summary>
    void Stop();

    /// <summary>Gone for good.</summary>
    void Release();

    long PositionMs { get; }

    /// <summary>0 while unknown.</summary>
    long DurationMs { get; }

    /// <summary>How far it has data; the position when the player doesn't say.</summary>
    long BufferedMs { get; }

    bool Playing { get; }

    /// <summary>Whether what plays has a picture; null until known.</summary>
    bool? HasVideo { get; }
}

/// <summary>An audio track as the engine knows it, in the file's order.</summary>
public sealed record EngineTrack(string? Language, string? Codec, int? Channels);

/// <summary>
/// What the engine can say about what it plays, for the Info panel's "This TV" (the web TV app's
/// EngineStats). Every field is optional: Tizen's player says little.
/// </summary>
public sealed record EngineStats
{
    /// <summary>The video decoder's name.</summary>
    public string? Decoder { get; init; }
    public bool? Hardware { get; init; }
    public int? Width { get; init; }
    public int? Height { get; init; }
    public double? FrameRate { get; init; }
    public int? BitDepth { get; init; }
    /// <summary>The codec arriving, as a MIME type then a codec string: "video/hevc hvc1.2.4.L153.B0".</summary>
    public string? Codec { get; init; }
    /// <summary>"HDR10", "Dolby Vision", "HLG", "SDR"… as the engine reports it.</summary>
    public string? Hdr { get; init; }
    /// <summary>Bits per second of the stream as it arrives.</summary>
    public long? Bitrate { get; init; }
    /// <summary>Bits per second the network brings.</summary>
    public long? Bandwidth { get; init; }
    public string? AudioCodec { get; init; }
    public int? AudioChannels { get; init; }
    /// <summary>"Passed through as E-AC-3", "Decoded to PCM"…</summary>
    public string? AudioOutput { get; init; }
    /// <summary>Frames dropped so far, of <see cref="Frames"/>.</summary>
    public long? Dropped { get; init; }
    /// <summary>Every frame the decoder had so far, dropped ones included.</summary>
    public long? Frames { get; init; }
    /// <summary>Lines the engine adds as they are ("Label: value"), when it has more to say.</summary>
    public IReadOnlyList<string> Extra { get; init; } = new string[0];
}

/// <summary>
/// The TV's player as the video player drives it: <see cref="IMediaEngine"/>, and what films and
/// episodes need beyond Live TV and music (track switching, speeds, frame counts, stats). The
/// Tizen app implements it over Tizen.Multimedia.Player; Live TV and music need only the base.
/// </summary>
public interface IVideoEngine : IMediaEngine
{
    /// <summary>False when <see cref="IMediaEngine.BufferedMs"/> only repeats the position (Tizen's player says a percentage at most).</summary>
    bool KnowsBuffer { get; }

    IReadOnlyList<EngineTrack> AudioTracks();

    /// <summary>Switches to the audio track at <paramref name="ordinal"/> among the file's audio streams, checked against <paramref name="language"/>; false when it can't.</summary>
    bool SelectAudio(int ordinal, string? language);

    /// <summary>The place among <see cref="AudioTracks"/> of the track playing; null when it doesn't say.</summary>
    int? PlayingAudio();

    /// <summary>The speeds it plays with their sound, 1 among them; empty when only at normal speed (Speed isn't offered then).</summary>
    IReadOnlyList<double> Speeds { get; }

    /// <summary>Plays at <paramref name="rate"/> times normal speed, from now and for the sources it opens after; false when it can't.</summary>
    bool SetSpeed(double rate);

    /// <summary>Dropped and shown frames so far; null when the engine doesn't count them.</summary>
    FrameCounts? FrameCounts();

    /// <summary>How fast the network brings the stream, kbps; null when it can't say.</summary>
    long? BandwidthKbps();

    EngineStats? Stats();
}
