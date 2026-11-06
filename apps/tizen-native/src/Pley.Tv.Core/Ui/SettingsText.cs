using System;
using System.Collections.Generic;
using System.Linq;
using Pley.Tv.Core.Player;

namespace Pley.Tv.Core.Ui;

/// <summary>
/// What Settings and the Me page say (the Android app's strings_tv_account.xml and
/// strings_tv_shell.xml's settings), in the language shown (Locales/*/settings.json).
/// </summary>
public static class SettingsText
{
    public static string Title => Strings.T("settings.title");

    // The categories.
    public static string Profile => Strings.T("settings.categories.profile");
    public static string Playback => Strings.T("settings.categories.playback");
    public static string Audio => Strings.T("settings.categories.audio");
    public static string Display => Strings.T("settings.categories.display");
    public static string Server => Strings.T("settings.categories.server");
    public static string Diagnostics => Strings.T("settings.categories.diagnostics");
    public static string About => Strings.T("settings.categories.about");
    public static string SignOut => Strings.T("settings.categories.signOut");

    // Profile (the Me page).
    public static string RoleManaged => Strings.T("settings.profile.roleManaged");
    public static string SwitchProfile => Strings.T("settings.profile.switchProfile");
    public static string AudioLanguage => Strings.T("settings.profile.audioLanguage");
    public static string Subtitles => Strings.T("settings.profile.subtitles");
    public static string SubtitleLanguage => Strings.T("settings.profile.subtitleLanguage");
    public static string FileDefault => Strings.T("settings.profile.fileDefault");
    public static string SameAsAudio => Strings.T("settings.profile.sameAsAudio");
    public static string ChangeOnWeb => Strings.T("settings.profile.changeOnWeb");
    public static string ReadOnly => Strings.T("settings.profile.readOnly");
    public static string Saved => Strings.T("settings.profile.saved");
    public static string SubtitlesOff => Strings.T("settings.subtitleModes.off");
    public static string SubtitlesOffHint => Strings.T("settings.subtitleModes.offHint");
    public static string SubtitlesForced => Strings.T("settings.subtitleModes.forced");
    public static string SubtitlesForcedHint => Strings.T("settings.subtitleModes.forcedHint");
    public static string SubtitlesForeign => Strings.T("settings.subtitleModes.foreign");
    public static string SubtitlesForeignHint => Strings.T("settings.subtitleModes.foreignHint");
    public static string SubtitlesAlways => Strings.T("settings.subtitleModes.always");
    public static string SubtitlesAlwaysHint => Strings.T("settings.subtitleModes.alwaysHint");
    public static string History => Strings.T("settings.profile.history");
    public static string SortLibraries => Strings.T("settings.profile.sortLibraries");
    public static string SortLibrariesValue => Strings.T("settings.profile.sortLibrariesValue");
    public static string AlwaysStartAs => Strings.T("settings.profile.alwaysStartAs");
    public static string UsedLast => Strings.T("settings.profile.usedLast");
    public static string ManageWeb => Strings.T("settings.profile.manageWeb");
    public static string ManageWebValue => Strings.T("settings.profile.manageWebValue");
    public static string WebTitle => Strings.T("settings.web.title");
    public static string WebBody => Strings.T("settings.web.body");
    public static string Close => Strings.T("common.actions.close");
    public static string SignOutTitle => Strings.T("settings.signOut.title");
    public static string Cancel => Strings.T("common.actions.cancel");

    // The language, in the catalogs (Locales/*/settings.json).
    public static string Language => Strings.T("settings.language.label");

    /// <summary>"Automatic (Français)": the profile has no language of its own, and this is what that is here.</summary>
    public static string Automatic(string language) => Strings.T("settings.language.automatic", ("language", language));

    // Playback.
    public static string Quality => Strings.T("settings.playback.quality");
    public static string QualityOriginal => Strings.T("settings.playback.original");
    public static string QualityOriginalHint => Strings.T("settings.playback.originalHint");
    public static string QualityAutomatic => Strings.T("settings.playback.automatic");
    public static string QualityAutomaticHint => Strings.T("settings.playback.automaticHint");
    public static string Autoplay => Strings.T("settings.playback.autoplay");
    public static string AutoplayHint => Strings.T("settings.playback.autoplayHint");
    public static string PlaybackDetails => Strings.T("settings.playback.details");
    public static string PlaybackDetailsHint => Strings.T("settings.playback.detailsHint");
    public static string ReduceMotion => Strings.T("settings.display.reduceMotion");
    public static string ReduceMotionHint => Strings.T("settings.display.reduceMotionHint");

    // Audio.
    public static string PassthroughIntro => Strings.T("settings.audio.intro");
    public static string PassthroughAuto => Strings.T("settings.audio.auto");
    public static string PassthroughAutoHint => Strings.T("settings.audio.autoHint");
    public static string PassthroughOn => Strings.T("settings.audio.on");
    public static string PassthroughOnHint => Strings.T("settings.audio.onHint");
    public static string PassthroughOff => Strings.T("settings.audio.off");
    public static string PassthroughOffHint => Strings.T("settings.audio.offHint");

    // Subtitles and display.
    public static string SubtitleSize => Strings.T("settings.display.subtitleSize");
    public static string SubtitleBackground => Strings.T("settings.display.subtitleBackground");
    public static string SubtitleFont => Strings.T("settings.display.subtitleFont");
    public static string SubtitlePosition => Strings.T("settings.display.subtitlePosition");

    // Server, Diagnostics, About.
    public static string ServerValue => Strings.T("settings.server.value");
    public static string ServerUnknown => Strings.T("settings.server.unknown");
    public static string SwitchServer => Strings.T("settings.server.switch");
    public static string OpenDiagnostics => Strings.T("settings.diagnostics.open");
    public static string OpenDiagnosticsValue => Strings.T("settings.diagnostics.openValue");
    public static string Version => Strings.T("settings.about.version");
    public static string Licences => Strings.T("settings.about.licences");
    public static string Credits => Strings.T("settings.about.credits");

    /// <summary>"You'll be signed out of 192.168.1.20:7539."</summary>
    public static string SignOutBody(string server) => Strings.T("settings.signOut.body", ("server", HostPort(server)));

    /// <summary>The Sign out category's words above its row.</summary>
    public static string SignOutIntro(string server) => Strings.T("settings.signOut.intro", ("server", HostPort(server)));

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
        return Strings.T("settings.playback.preset", ("mbps", (preset.MaxBitrateKbps ?? 0) / 1000), ("lines", lines));
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
        _ => Strings.T("settings.audio.dtsHd"),
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
        0 => Strings.T("settings.display.sizes.small"),
        2 => Strings.T("settings.display.sizes.large"),
        3 => Strings.T("settings.display.sizes.huge"),
        _ => Strings.T("settings.display.sizes.normal"),
    };

    public static string BackgroundLabel(SubtitleBackground background) =>
        background == Core.SubtitleBackground.DarkBox ? Strings.T("settings.display.backgrounds.darkBox") : Strings.T("settings.display.backgrounds.none");

    public static string FontLabel(SubtitleFont font) => font switch
    {
        Core.SubtitleFont.Serif => Strings.T("settings.display.fonts.serif"),
        Core.SubtitleFont.Monospace => Strings.T("settings.display.fonts.monospace"),
        Core.SubtitleFont.Casual => Strings.T("settings.display.fonts.casual"),
        _ => Strings.T("settings.display.fonts.sans"),
    };

    /// <summary>"Bottom", "Raised", "Higher", "Highest", by the raise's place among those offered.</summary>
    public static string RaiseLabel(float raise) => IndexOf(Settings.SubtitleRaises, raise) switch
    {
        1 => Strings.T("settings.display.positions.raised"),
        2 => Strings.T("settings.display.positions.higher"),
        3 => Strings.T("settings.display.positions.highest"),
        _ => Strings.T("settings.display.positions.bottom"),
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
/// codes, the form ffprobe reports), sorted by their names in the language shown (the Android
/// app's TrackLanguages): worked out each time, so a change of language names and sorts them anew.
/// </summary>
public static class TrackLanguages
{
    public static readonly IReadOnlyList<string> Codes = new[]
    {
        "ara", "bul", "cat", "chi", "cze", "dan", "dut", "eng", "fin", "fre", "ger", "gre", "heb", "hin", "hrv", "hun",
        "ind", "ita", "jpn", "kor", "nor", "per", "pol", "por", "rum", "rus", "slo", "spa", "srp", "swe", "tha", "tur",
        "ukr", "vie",
    };

    /// <summary>Code and name, by name in the language shown's order.</summary>
    public static IReadOnlyList<(string Code, string Name)> All =>
        Codes.Select(c => (c, ItemFormat.LanguageName(c) ?? c)).OrderBy(l => l.Item2, StringComparer.Create(Strings.Culture, ignoreCase: false)).ToList();

    /// <summary><paramref name="code"/>'s name; null for none.</summary>
    public static string? NameOf(string? code) => code == null ? null : ItemFormat.LanguageName(code);
}
