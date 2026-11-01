using System.Collections.Generic;

namespace Pley.Tv.Core.Api;

/// <summary>A session the server made for a play request (POST /api/playback; docs/API.md).</summary>
public sealed record PlaybackInfo
{
    public string SessionId { get; init; } = "";
    /// <summary>"direct", "remux" or "transcode".</summary>
    public string Method { get; init; } = "";
    /// <summary>Where the player reads it; with keyed URLs, an address that carries its own key.</summary>
    public string Url { get; init; } = "";
    public bool Hls { get; init; }
    public long FileId { get; init; }
    public long DurationMs { get; init; }
    /// <summary>The audio track that plays: the one asked for, else the server's choice for this player.</summary>
    public int? AudioIndex { get; init; }
    /// <summary>The subtitle burned in, only when <see cref="BurnSubtitles"/>.</summary>
    public int? SubtitleIndex { get; init; }
    public bool BurnSubtitles { get; init; }
    /// <summary>The audio is converted, also in a remux, where the video is copied.</summary>
    public bool AudioConverted { get; init; }
    /// <summary>Every subtitle track of the file, with the address of its WebVTT.</summary>
    public IReadOnlyList<SubtitleTrack> Subtitles { get; init; } = new SubtitleTrack[0];
    public IReadOnlyList<string>? Reasons { get; init; }
    public string? Reason { get; init; }
    /// <summary>
    /// The original converted: the closest quality players offer ("30m", "20m", "15m", "12m",
    /// "8m", "4m", "2m"); null when a quality was asked for, or nothing is converted.
    /// </summary>
    public string? AutoQuality { get; init; }
    /// <summary>What the screen gets of HDR video, by name: "HDR10", "SDR (tone-mapped)"…</summary>
    public string? VideoRange { get; init; }
    /// <summary>The stream comes in several sizes the player moves between by itself.</summary>
    public bool Adaptive { get; init; }
    /// <summary>The video is converted, and PUT /api/playback/{id}/bitrate changes its bitrate as it plays.</summary>
    public bool LiveBitrate { get; init; }
}

/// <summary>A subtitle track of the file being played, as a session lists it.</summary>
public sealed record SubtitleTrack
{
    public int Index { get; init; }
    public string Codec { get; init; } = "";
    public string? Language { get; init; }
    public string? Title { get; init; }
    public bool Default { get; init; }
    public bool Forced { get; init; }
    public bool External { get; init; }
    public bool TextBased { get; init; }
    /// <summary>WebVTT: "/api/files/7/subtitles/3.vtt"; null for subtitles shown as pictures.</summary>
    public string? Url { get; init; }
    /// <summary>Added on the server (found online, or a file uploaded), not beside the media.</summary>
    public bool Added { get; init; }
}

/// <summary>
/// The live numbers of a session's transcode, for the player's details (apps/server/src/playback.rs,
/// <c>TranscodeInfo</c>). Actions: "copy", "transcode", "none".
/// </summary>
public sealed record TranscodeInfo
{
    public string JobId { get; init; } = "";
    /// <summary>"starting", "running", "idle", "finished" or "failed".</summary>
    public string? State { get; init; }
    public string? Video { get; init; }
    public string? Audio { get; init; }
    /// <summary>"HEVC 3840×2160 Dolby Vision P8.1".</summary>
    public string? VideoIn { get; init; }
    /// <summary>"H.264 1920×1080".</summary>
    public string? VideoOut { get; init; }
    /// <summary>"EAC3 5.1".</summary>
    public string? AudioIn { get; init; }
    /// <summary>"AAC 2.0".</summary>
    public string? AudioOut { get; init; }
    /// <summary>"h264_nvenc", "libx264"…</summary>
    public string? Encoder { get; init; }
    /// <summary>The software encoder's preset now ("veryfast"…).</summary>
    public string? Preset { get; init; }
    public string? Decoder { get; init; }
    /// <summary>"gpu", "hybrid", "cpu" or "copy".</summary>
    public string? Pipeline { get; init; }
    /// <summary>"libplacebo", "opencl" or "zscale" when HDR is tone-mapped to SDR.</summary>
    public string? Tonemap { get; init; }
    public bool SubtitleBurn { get; init; }
    /// <summary>ffmpeg's speed: 2.5 is two and a half times real time.</summary>
    public double? Speed { get; init; }
    public double? Fps { get; init; }
    /// <summary>How far ffmpeg got, in ms of the media.</summary>
    public long TranscodedMs { get; init; }
    public int SegmentsReady { get; init; }
    public int SegmentsTotal { get; init; }
    /// <summary>What the transcode aims for, video and audio together, in bits per second.</summary>
    public long? BitrateOut { get; init; }
    public string? LastError { get; init; }
}

/// <summary>GET /api/playback/{session}: how the session plays now, with its transcode's live numbers.</summary>
public sealed record SessionStatus
{
    public string SessionId { get; init; } = "";
    public string Method { get; init; } = "";
    /// <summary>Null for direct play, or when the transcoder can't say.</summary>
    public TranscodeInfo? Transcode { get; init; }
}

/// <summary>The answer of PUT /api/playback/{session}/bitrate: the bitrate the stream goes on at, kbps.</summary>
public sealed record BitrateAnswer(long Bitrate);

/// <summary>A subtitle track's WebVTT, and whether it is whole (<c>X-Pley-Complete</c>: 0 while still being read out of its file).</summary>
public sealed record SubtitleText(string Text, bool Complete);

/// <summary>The server's .mks of a picture subtitle track, and whether it is whole (<c>X-Pley-Complete</c>: 0 while still being read out of its file).</summary>
public sealed record SubtitlePictures(byte[] Bytes, bool Complete);

/// <summary>
/// GET /api/files/{id}/trickplay: the seek-bar previews of a file, tiled into JPEG sheets.
/// Thumbnail k shows the video at k × <see cref="IntervalMs"/>; it is at column k % columns, row
/// (k / columns) % rows of sheet k / (columns × rows).
/// </summary>
public sealed record TrickplayInfo
{
    /// <summary>Identifies this set of sheets: the <c>v</c> of their addresses.</summary>
    public string Id { get; init; } = "";
    public long IntervalMs { get; init; }
    /// <summary>The size of one thumbnail, in pixels.</summary>
    public int Width { get; init; }
    public int Height { get; init; }
    public int Columns { get; init; }
    public int Rows { get; init; }
    /// <summary>Thumbnails in all.</summary>
    public int Count { get; init; }
    /// <summary>Sheets: "0.jpg" to "{sheets - 1}.jpg".</summary>
    public int Sheets { get; init; }
}
