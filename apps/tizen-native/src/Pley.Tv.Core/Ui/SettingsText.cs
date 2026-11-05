using System;
using System.Collections.Generic;
using System.Linq;
using Pley.Tv.Core.Player;

namespace Pley.Tv.Core.Ui;

/// <summary>What Settings and the Me page say (the Android app's strings_tv_account.xml and strings_tv_shell.xml's settings).</summary>
public static class SettingsText
{
    public const string Title = "Settings";

    // The categories.
    public const string Profile = "Profile";
    public const string Playback = "Playback";
    public const string Audio = "Audio";
    public const string Display = "Subtitles and display";
    public const string Server = "Server";
    public const string Diagnostics = "Diagnostics";
    public const string About = "About";
    public const string SignOut = "Sign out";

    // Profile (the Me page).
    public const string RoleManaged = "Managed profile";
    public const string SwitchProfile = "Switch profile";
    public const string AudioLanguage = "Audio language";
    public const string Subtitles = "Subtitles";
    public const string SubtitleLanguage = "Subtitle language";
    public const string FileDefault = "The file's default";
    public const string SameAsAudio = "Same as the audio language";
    public const string ChangeOnWeb = "change on the web";
    public const string ReadOnly = "This server doesn't let the app change these: change them on the web";
    public const string Saved = "Saved";
    public const string SubtitlesOff = "Off";
    public const string SubtitlesOffHint = "Subtitles stay off until you turn them on.";
    public const string SubtitlesForced = "Only for foreign dialogue";
    public const string SubtitlesForcedHint = "Only forced subtitles, for signs and foreign dialogue.";
    public const string SubtitlesForeign = "When the audio is in another language";
    public const string SubtitlesForeignHint = "Full subtitles come on when the audio isn't in your subtitle language.";
    public const string SubtitlesAlways = "Always";
    public const string SubtitlesAlwaysHint = "Subtitles in your language come on whenever there are some.";
    public const string History = "Watch history";
    public const string SortLibraries = "Sort libraries";
    public const string SortLibrariesValue = "Your order, on all your devices";
    public const string AlwaysStartAs = "Always start as";
    public const string AskWho = "Ask who's watching";
    public const string ManageWeb = "Manage your account on the web";
    public const string ManageWebValue = "Name, picture, password, PIN, two-step, devices and apps";
    public const string WebTitle = "Manage your account on the web";
    public const string WebBody = "Scan the code with your phone, or go to this address on a phone or computer on the same network:";
    public const string Close = "Close";
    public const string SignOutTitle = "Sign out?";
    public const string Cancel = "Cancel";

    // Playback.
    public const string Quality = "Quality";
    public const string QualityOriginal = "Original";
    public const string QualityOriginalHint = "Full quality. May buffer on a slow connection.";
    public const string QualityAutomatic = "Automatic";
    public const string QualityAutomaticHint = "Dynamically adjust to conditions.";
    public const string Autoplay = "Play the next episode by itself";
    public const string AutoplayHint = "After a 10 second countdown";
    public const string PlaybackDetails = "Playback details";
    public const string PlaybackDetailsHint = "Show how a video plays (direct, remux or transcode) over it";
    public const string ReduceMotion = "Reduce motion";
    public const string ReduceMotionHint = "No zooming, sliding or fading: things change in place";

    // Audio.
    public const string PassthroughIntro = "Send surround sound to your TV or receiver, or decode it here.";
    public const string PassthroughAuto = "Auto";
    public const string PassthroughAutoHint = "As the TV or receiver says it plays";
    public const string PassthroughOn = "On";
    public const string PassthroughOnHint = "Always pass it through";
    public const string PassthroughOff = "Off";
    public const string PassthroughOffHint = "Never: Pley converts it";

    // Subtitles and display.
    public const string SubtitleSize = "Subtitle size";
    public const string SubtitleBackground = "Subtitle background";
    public const string SubtitleFont = "Subtitle font";
    public const string SubtitlePosition = "Subtitle position";

    // Server, Diagnostics, About.
    public const string ServerValue = "The server this TV uses";
    public const string ServerUnknown = "No server";
    public const string SwitchServer = "Switch server";
    public const string OpenDiagnostics = "Open diagnostics";
    public const string OpenDiagnosticsValue = "What this TV decodes and shows, and why";
    public const string Version = "Version";
    public const string Licences = "Open-source licences";
    public const string Credits = "Credits";

    /// <summary>"You'll be signed out of 192.168.1.20:7539."</summary>
    public static string SignOutBody(string server) => $"You'll be signed out of {HostPort(server)}.";

    /// <summary>The Sign out category's words above its row.</summary>
    public static string SignOutIntro(string server) => $"Sign this TV out of {HostPort(server)}. You'll need a new link code to sign in again.";

    /// <summary>"http://192.168.1.20:7539" → "192.168.1.20:7539".</summary>
    public static string HostPort(string server)
    {
        var at = server.IndexOf("://", StringComparison.Ordinal);
        return (at >= 0 ? server.Substring(at + 3) : server).TrimEnd('/');
    }

    public static string CategoryLabel(SettingsCategory c) => c switch
    {
        SettingsCategory.Profile => Profile,
        SettingsCategory.Playback => Playback,
        SettingsCategory.Audio => Audio,
        SettingsCategory.Display => Display,
        SettingsCategory.Server => Server,
        SettingsCategory.Diagnostics => Diagnostics,
        SettingsCategory.About => About,
        _ => SignOut,
    };

    /// <summary>A category's icon, by Lucide's name.</summary>
    public static string CategoryIcon(SettingsCategory c) => c switch
    {
        SettingsCategory.Profile => "user-round",
        SettingsCategory.Playback => "play",
        SettingsCategory.Audio => "audio-lines",
        SettingsCategory.Display => "captions",
        SettingsCategory.Server => "server",
        SettingsCategory.Diagnostics => "activity",
        SettingsCategory.About => "info",
        _ => "log-out",
    };

    /// <summary>A subtitle mode's name ("off", "forced", "foreign", "always"); anything else is forced, the server's default.</summary>
    public static string ModeLabel(string mode) => mode switch
    {
        "off" => SubtitlesOff,
        "foreign" => SubtitlesForeign,
        "always" => SubtitlesAlways,
        _ => SubtitlesForced,
    };

    public static string ModeHint(string mode) => mode switch
    {
        "off" => SubtitlesOffHint,
        "foreign" => SubtitlesForeignHint,
        "always" => SubtitlesAlwaysHint,
        _ => SubtitlesForcedHint,
    };

    /// <summary>"Original", "Automatic", "8 Mbps 1080p", "30 Mbps 4K", as the player's menu says them.</summary>
    public static string QualityLabel(string? value)
    {
        var preset = Qualities.ByIdOrLabel(value);
        if (preset.Original) return QualityOriginal;
        if (preset.Automatic) return QualityAutomatic;
        var lines = preset.MaxHeight switch
        {
            2160 => "4K",
            null => "",
            var h => $"{h}p",
        };
        return $"{(preset.MaxBitrateKbps ?? 0) / 1000} Mbps {lines}";
    }

    /// <summary>What Original and Automatic do, under their names; null for the transcodes, whose names say it.</summary>
    public static string? QualityHint(string? value)
    {
        var preset = Qualities.ByIdOrLabel(value);
        return preset.Original ? QualityOriginalHint : preset.Automatic ? QualityAutomaticHint : null;
    }

    /// <summary>A sound format's name: "Dolby Digital Plus", "DTS-HD and DTS:X".</summary>
    public static string FormatLabel(string format) => format switch
    {
        "ac3" => "Dolby Digital",
        "eac3" => "Dolby Digital Plus",
        "eac3-joc" => "Dolby Atmos",
        "truehd" => "Dolby TrueHD",
        "dts" => "DTS",
        _ => "DTS-HD and DTS:X",
    };

    public static string PassthroughLabel(PassthroughMode mode) => mode switch
    {
        PassthroughMode.On => PassthroughOn,
        PassthroughMode.Off => PassthroughOff,
        _ => PassthroughAuto,
    };

    public static string PassthroughHint(PassthroughMode mode) => mode switch
    {
        PassthroughMode.On => PassthroughOnHint,
        PassthroughMode.Off => PassthroughOffHint,
        _ => PassthroughAutoHint,
    };

    /// <summary>"Small", "Normal", "Large", "Huge", by the size's place among those offered.</summary>
    public static string SizeLabel(float size) => IndexOf(Settings.SubtitleSizes, size) switch
    {
        0 => "Small",
        2 => "Large",
        3 => "Huge",
        _ => "Normal",
    };

    public static string BackgroundLabel(SubtitleBackground background) => background == Core.SubtitleBackground.DarkBox ? "Dark box" : "None";

    public static string FontLabel(SubtitleFont font) => font switch
    {
        Core.SubtitleFont.Serif => "Serif",
        Core.SubtitleFont.Monospace => "Monospace",
        Core.SubtitleFont.Casual => "Casual",
        _ => "Sans serif",
    };

    /// <summary>"Bottom", "Raised", "Higher", "Highest", by the raise's place among those offered.</summary>
    public static string RaiseLabel(float raise) => IndexOf(Settings.SubtitleRaises, raise) switch
    {
        1 => "Raised",
        2 => "Higher",
        3 => "Highest",
        _ => "Bottom",
    };

    static int IndexOf(IReadOnlyList<float> list, float value)
    {
        for (var i = 0; i < list.Count; i++)
            if (Math.Abs(list[i] - value) < 0.0005f) return i;
        return -1;
    }
}

/// <summary>
/// The languages offered as audio and subtitle defaults, as the web offers them (ISO 639-2/B
/// codes, the form ffprobe reports), sorted by their English names (the Android app's TrackLanguages).
/// </summary>
public static class TrackLanguages
{
    public static readonly IReadOnlyList<string> Codes = new[]
    {
        "ara", "bul", "cat", "chi", "cze", "dan", "dut", "eng", "fin", "fre", "ger", "gre", "heb", "hin", "hrv", "hun",
        "ind", "ita", "jpn", "kor", "nor", "per", "pol", "por", "rum", "rus", "slo", "spa", "srp", "swe", "tha", "tur",
        "ukr", "vie",
    };

    /// <summary>Code and name, by name.</summary>
    public static readonly IReadOnlyList<(string Code, string Name)> All =
        Codes.Select(c => (c, ItemFormat.LanguageName(c) ?? c)).OrderBy(l => l.Item2, StringComparer.Ordinal).ToList();

    /// <summary><paramref name="code"/>'s name; null for none.</summary>
    public static string? NameOf(string? code) =>
        code == null ? null : All.Where(l => l.Code == code).Select(l => l.Name).FirstOrDefault() ?? ItemFormat.LanguageName(code);
}
