using System;
using System.Globalization;
using Pley.Tv.Core.Api;

namespace Pley.Tv.Core.Player;

/// <summary>
/// What the playback says to people: the Android app's PlayerTexts, from strings_session.xml and
/// strings_player_extras.xml, in the language shown (<c>player.json</c>; English word for word).
/// </summary>
public static class SessionText
{
    public static string QualityOriginal => Strings.T("player.quality.original");

    /// <summary>The audio chosen isn't a track the TV's player offers (TrueHD): what plays instead.</summary>
    public static string AudioNotOffered(string chosen, string playing) => Strings.T("player.audio.notOffered", ("chosen", chosen), ("playing", playing));
    public static string QualityAutomatic => Strings.T("player.quality.automatic");
    public static string AutomaticHint => Strings.T("player.quality.automaticHint");
    public static string LoweredNetwork => Strings.T("player.quality.loweredNetwork");
    public static string LoweredDevice => Strings.T("player.quality.loweredDevice");
    public static string SkipIntro => Strings.T("player.markers.skipIntro");
    public static string SkipCredits => Strings.T("player.markers.skipCredits");
    public static string LastEpisode => Strings.T("player.upNext.lastEpisode");
    /// <summary>A file on a drive whose picture never moved: nothing converts it, as a server would.</summary>
    public static string LocalStuck => Strings.T("storage.playback.stuck");
    public static string LastInQueue => Strings.T("player.upNext.lastInQueue");
    public static string SubtitlesOff => Strings.T("player.subtitles.off");
    public static string SubtitlesFailed => Strings.T("player.subtitles.failed");
    public static string MethodDirect => Strings.T("player.method.direct");
    public static string MethodRemux => Strings.T("player.method.remux");
    public static string MethodTranscode => Strings.T("player.method.transcode");
    public static string MethodRemuxAudio => Strings.T("player.method.remuxAudio");
    public static string FallbackUnplayable => Strings.T("player.fallback.unplayable");
    public static string FallbackNotSmooth => Strings.T("player.fallback.notSmooth");
    public static string SlowServer => Strings.T("player.quality.slowServer");
    public static string RowVideo => Strings.T("player.details.video");
    public static string RowAudio => Strings.T("player.details.audio");
    public static string RowBitrate => Strings.T("player.details.bitrate");
    public static string RowEncoder => Strings.T("player.details.encoder");
    public static string RowHdr => Strings.T("player.details.shownAs");
    public static string RowSubtitles => Strings.T("player.details.subtitles");
    /// <summary>How fast the server makes the stream, as every app calls it.</summary>
    public static string RowSpeed => Strings.T("player.details.speed");
    /// <summary>How fast the TV's whole network receives, which Tizen's player doesn't say of its own downloads.</summary>
    public static string RowNetwork => Strings.T("player.details.network");
    public static string BurnedIn => Strings.T("player.details.burnedIn");
    public static string SpeedPassthrough => Strings.T("player.speed.passthrough");
    public static string ErrorUnplayable => Strings.T("player.errors.unplayable");
    public static string ErrorStuck => Strings.T("player.errors.stuck");
    public static string ErrorShort => Strings.T("player.errors.short");
    public static string ErrorStopped => Strings.T("player.errors.stopped");
    public static string ErrorNetwork => Strings.T("player.errors.network");
    public static string NoFile => Strings.T("player.errors.noFile");
    public static string StoppedByAdmin => Strings.T("player.stopped.byAdmin");
    public static string StoppedElsewhere => Strings.T("player.stopped.elsewhere");

    /// <summary>A quality by the menu's name: "Original", "Automatic", "8 Mbps 1080p" (in French "8 Mbit/s 1080p", as Settings and the web write it).</summary>
    public static string Quality(QualityPreset preset) => preset.Id switch
    {
        QualityPreset.AutomaticId => QualityAutomatic,
        "30m" => Preset(30, "4K"),
        "15m" => Preset(15, "4K"),
        "20m" => Preset(20, "1080p"),
        "12m" => Preset(12, "1080p"),
        "8m" => Preset(8, "1080p"),
        "4m" => Preset(4, "720p"),
        "2m" => Preset(2, "480p"),
        _ => QualityOriginal,
    };

    /// <summary>A quality as offered for a file of <paramref name="fileLines"/>: one taller than the file is named by the file's lines ("8 Mbps 720p").</summary>
    public static string Quality(QualityPreset preset, int? fileLines)
    {
        if (preset.MaxHeight is not int most || preset.MaxBitrateKbps is not int kbps || fileLines is not int lines || lines >= most) return Quality(preset);
        return Preset(kbps / 1000, LinesLabel(lines));
    }

    /// <summary>A converted quality's name: its megabits a second and its lines, "8 Mbps 1080p".</summary>
    static string Preset(int mbps, string lines) => Strings.T("player.quality.preset", ("mbps", mbps), ("lines", lines));

    /// <summary>Lines as qualities name them: "4K", "1080p", "720p".</summary>
    public static string LinesLabel(int lines) => lines >= 2160 ? "4K" : lines.ToString(CultureInfo.InvariantCulture) + "p";

    /// <summary>The menu's row while Automatic plays <paramref name="playing"/>: "Automatic · Original".</summary>
    public static string Automatic(string playing) => Strings.T("player.quality.automaticPlaying", ("quality", playing));

    /// <summary>Automatic's hint while it plays <paramref name="playing"/>, below the original: "Now 8 Mbps 1080p".</summary>
    public static string AutomaticNow(string playing) => Strings.T("player.quality.automaticNow", ("quality", playing));

    /// <summary>Said once when Automatic goes down to the quality named <paramref name="label"/>.</summary>
    public static string LoweredNotice(string label) => Strings.T("player.quality.lowered", ("quality", label));

    /// <summary>"Up next · S1 E2", or "Up next".</summary>
    public static string UpNext(string? label) =>
        string.IsNullOrEmpty(label) ? Strings.T("player.upNext.title") : Strings.T("player.upNext.episode", ("episode", label));

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
        _ => Strings.T("player.errors.failed", ("details", EngineCode(detail))),
    };

    static string EngineCode(string? detail)
    {
        var code = (detail ?? "").Split(':')[0].Trim();
        return code.Length > 0 ? code : "?";
    }

    /// <summary>The server out of reach, for <paramref name="reason"/> ("no answer in time"): what to check.</summary>
    public static string Unreachable(string reason) => Strings.T("common.errors.offlineBody", ("reason", Errors.AsSentence(reason)));

    /// <summary>The server stopped the playback: who did, then their message on a line of its own.</summary>
    public static string Stopped(string? message, bool byAdmin)
    {
        var who = byAdmin ? StoppedByAdmin : StoppedElsewhere;
        return string.IsNullOrWhiteSpace(message) ? who : who + "\n" + message!.Trim();
    }

    /// <summary>"SDR (tone-mapped with libplacebo)".</summary>
    public static string ToneMapped(string tonemap) => Strings.T("player.details.toneMapped", ("method", tonemap));

    /// <summary>"2.5× real time", "2.5× real time · 40 s ready".</summary>
    public static string Speed(string speed, int readySeconds) =>
        readySeconds > 0 ? Strings.T("player.details.realTimeReady", ("speed", speed), ("seconds", readySeconds)) : Strings.T("player.details.realTime", ("speed", speed));
}
