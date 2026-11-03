using System;
using System.Globalization;
using Pley.Tv.Core.Api;

namespace Pley.Tv.Core.Player;

/// <summary>
/// What the playback says to people: the Android app's PlayerTexts, from strings_session.xml and
/// strings_player_extras.xml, word for word.
/// </summary>
public static class SessionText
{
    public const string QualityOriginal = "Original";

    /// <summary>The audio chosen isn't a track the TV's player offers (TrueHD): what plays instead.</summary>
    public static string AudioNotOffered(string chosen, string playing) => $"This TV's player doesn't offer {chosen}: {playing} plays";
    public const string QualityAutomatic = "Automatic";
    public const string AutomaticHint = "Dynamically adjust to conditions";
    public const string LoweredNetwork = "Lowered for your connection";
    public const string LoweredDevice = "Lowered for this device";
    public const string SkipIntro = "Skip Intro";
    public const string SkipCredits = "Skip Credits";
    public const string LastEpisode = "This is the last episode";
    /// <summary>A file on a drive whose picture never moved: nothing converts it, as a server would.</summary>
    public const string LocalStuck = "This TV doesn't play this file: its picture never started. With no server to convert it, try another copy.";
    public const string LastInQueue = "This is the last one";
    public const string SubtitlesOff = "Off";
    public const string SubtitlesFailed = "These subtitles couldn't be loaded.";
    public const string MethodDirect = "Direct play";
    public const string MethodRemux = "Direct stream";
    public const string MethodTranscode = "Transcoding";
    public const string MethodRemuxAudio = "Direct stream · Audio transcoding";
    public const string FallbackUnplayable = "This device couldn't play the original file.";
    public const string FallbackNotSmooth = "The original didn't play smoothly on this device.";
    public const string SlowServer = "The server can't keep up with this video. Try a lower quality.";
    public const string RowVideo = "Video";
    public const string RowAudio = "Audio";
    public const string RowBitrate = "Bitrate";
    public const string RowEncoder = "Encoder";
    public const string RowHdr = "Shown as";
    public const string RowSubtitles = "Subtitles";
    /// <summary>How fast the server makes the stream, as every app calls it.</summary>
    public const string RowSpeed = "Speed";
    /// <summary>How fast the TV's whole network receives, which Tizen's player doesn't say of its own downloads.</summary>
    public const string RowNetwork = "TV network speed";
    public const string BurnedIn = "Burned in";
    public const string SpeedPassthrough = "Not while the sound goes to your receiver as it is: it plays at its own speed";
    public const string ErrorUnplayable = "This video can't be played on this device.";
    public const string ErrorStuck = "This video doesn't play on this device: it stops at the same spot each time.";
    public const string ErrorShort = "This video ends before the point it was to play from.";
    public const string ErrorStopped = "Playback stopped and couldn't get going again.";
    public const string ErrorNetwork = "Check that the server is on and that this device is on its network.";
    public const string NoFile = "This item has no file to play.";
    public const string StoppedByAdmin = "Playback was stopped by the server's administrator.";
    public const string StoppedElsewhere = "Playback was stopped from another device.";

    /// <summary>A quality by the menu's name: "Original", "Automatic", "8 Mbps 1080p".</summary>
    public static string Quality(QualityPreset preset) => preset.Id switch
    {
        QualityPreset.AutomaticId => QualityAutomatic,
        "30m" => "30 Mbps 4K",
        "15m" => "15 Mbps 4K",
        "20m" => "20 Mbps 1080p",
        "12m" => "12 Mbps 1080p",
        "8m" => "8 Mbps 1080p",
        "4m" => "4 Mbps 720p",
        "2m" => "2 Mbps 480p",
        _ => QualityOriginal,
    };

    /// <summary>A quality as offered for a file of <paramref name="fileLines"/>: one taller than the file is named by the file's lines ("8 Mbps 720p").</summary>
    public static string Quality(QualityPreset preset, int? fileLines)
    {
        if (preset.MaxHeight is not int most || preset.MaxBitrateKbps is not int kbps || fileLines is not int lines || lines >= most) return Quality(preset);
        return $"{(kbps / 1000).ToString(CultureInfo.InvariantCulture)} Mbps {LinesLabel(lines)}";
    }

    /// <summary>Lines as qualities name them: "4K", "1080p", "720p".</summary>
    public static string LinesLabel(int lines) => lines >= 2160 ? "4K" : lines.ToString(CultureInfo.InvariantCulture) + "p";

    /// <summary>The menu's row while Automatic plays <paramref name="playing"/>: "Automatic · Original".</summary>
    public static string Automatic(string playing) => $"Automatic · {playing}";

    /// <summary>Automatic's hint while it plays <paramref name="playing"/>, below the original: "Now 8 Mbps 1080p".</summary>
    public static string AutomaticNow(string playing) => $"Now {playing}";

    /// <summary>Said once when Automatic goes down to the quality named <paramref name="label"/>.</summary>
    public static string LoweredNotice(string label) => $"Quality lowered to {label}";

    /// <summary>"Up next · S1 E2", or "Up next".</summary>
    public static string UpNext(string? label) => string.IsNullOrEmpty(label) ? "Up next" : $"Up next · {label}";

    /// <summary>"Direct play", "Direct stream", "Transcoding", "Direct stream · Audio transcoding".</summary>
    public static string Method(PlaybackInfo info) => info.Method switch
    {
        "remux" => info.AudioConverted ? MethodRemuxAudio : MethodRemux,
        "transcode" => MethodTranscode,
        _ => MethodDirect,
    };

    public static string Fallback(FallbackReason reason) => reason == FallbackReason.Unplayable ? FallbackUnplayable : FallbackNotSmooth;

    /// <summary>Why playback gave up, for people. An engine's failure names its first words: "Playback failed (ERROR_CODE_DECODING_FAILED).".</summary>
    public static string Failure(Failure failure, string? detail) => failure switch
    {
        Player.Failure.Unplayable => ErrorUnplayable,
        Player.Failure.Stuck => ErrorStuck,
        Player.Failure.Short => ErrorShort,
        Player.Failure.Stopped => ErrorStopped,
        Player.Failure.Network => ErrorNetwork,
        _ => $"Playback failed ({EngineCode(detail)}).",
    };

    static string EngineCode(string? detail)
    {
        var code = (detail ?? "").Split(':')[0].Trim();
        return code.Length > 0 ? code : "?";
    }

    /// <summary>The server out of reach, for <paramref name="reason"/> ("no answer in time"): what to check.</summary>
    public static string Unreachable(string reason) => $"{Errors.AsSentence(reason)} Check that the server is on and that this device is on its network.";

    /// <summary>The server stopped the playback: who did, then their message on a line of its own.</summary>
    public static string Stopped(string? message, bool byAdmin)
    {
        var who = byAdmin ? StoppedByAdmin : StoppedElsewhere;
        return string.IsNullOrWhiteSpace(message) ? who : who + "\n" + message!.Trim();
    }

    /// <summary>"SDR (tone-mapped with libplacebo)".</summary>
    public static string ToneMapped(string tonemap) => $"SDR (tone-mapped with {tonemap})";

    /// <summary>"2.5× real time", "2.5× real time · 40 s ready".</summary>
    public static string Speed(string speed, int readySeconds) =>
        readySeconds > 0 ? $"{speed}× real time · {readySeconds.ToString(CultureInfo.InvariantCulture)} s ready" : $"{speed}× real time";
}
