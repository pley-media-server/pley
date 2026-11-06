using System;
using System.Collections.Generic;
using System.Linq;
using System.Threading.Tasks;
using Pley.Tv.Core.Api;
using Pley.Tv.Core.Player;
using Pley.Tv.Core.Ui;
using Xunit;

namespace Pley.Tv.Core.Tests;

/// <summary>The Me page's data in a test: who is signed in, and what saving and signing out answer.</summary>
sealed class FakeAccount : IMeSource, ISettingsSource
{
    public string? Server { get; set; } = "http://192.168.1.20:7539/";
    public Signal<User?> Me { get; } = new Signal<User?>(null);
    public Signal<Settings> Settings { get; } = new Signal<Settings>(new Settings());
    public List<TrackDefaults> Sent { get; } = new List<TrackDefaults>();
    /// <summary>What saving throws; null: it answers the profile with the change.</summary>
    public Exception? SaveFailure { get; set; }
    public TaskCompletionSource<bool>? Saving { get; set; }
    public int TracksChanges { get; private set; }
    public int SignOuts { get; private set; }
    public List<HouseholdProfile> Profiles { get; } = new List<HouseholdProfile>();
    public long? StartAs { get; set; }

    public async Task<User> SaveTrackDefaults(TrackDefaults defaults)
    {
        Sent.Add(defaults);
        if (Saving != null) await Saving.Task;
        if (SaveFailure != null) throw SaveFailure;
        return Me.Value! with { AudioLanguage = defaults.AudioLanguage, SubtitleLanguage = defaults.SubtitleLanguage, SubtitleMode = defaults.SubtitleMode, DisplayName = "from the server" };
    }

    /// <summary>The languages saved, in order.</summary>
    public List<string?> LanguagesSent { get; } = new List<string?>();

    public async Task<User> SaveLanguage(string? language)
    {
        LanguagesSent.Add(language);
        if (Saving != null) await Saving.Task;
        if (SaveFailure != null) throw SaveFailure;
        return Me.Value! with { Language = language, DisplayName = "from the server" };
    }

    public void TracksChanged() => TracksChanges++;

    public Task SignOut()
    {
        SignOuts++;
        return Task.CompletedTask;
    }

    public void UpdateSettings(Func<Settings, Settings> change) => Settings.Value = change(Settings.Value).Allowed();

    public IReadOnlyList<HouseholdProfile> Household() => Profiles;

    public long? AlwaysStartAs() => StartAs;

    public void SetAlwaysStartAs(long? id) => StartAs = id;

    public string Version => "0.1.0";

    public IReadOnlyList<Fact> DeviceFacts => new[] { new Fact("Model", "QE55S95D") };

    public string? ServerLanguage { get; set; }

    public IReadOnlyList<string> TvLanguages { get; set; } = new[] { "en-US" };

    public string? DeviceLanguage { get; set; }
}

/// <summary>The Me page: the Android app's MeViewModelTest.</summary>
public class MeModelTests
{
    readonly FakeAccount account = new FakeAccount();
    readonly FakeScheduler scheduler = new FakeScheduler();

    static User Ann(string scope = "library play profiles") => new User
    {
        Id = 7, Name = "Ann", AvatarUrl = "/api/users/7/avatar", Managed = true, Scope = scope, AudioLanguage = "eng", SubtitleMode = "forced",
    };

    [Fact]
    public void The_profile_shows_who_is_signed_in_and_what_the_sign_in_may_do()
    {
        account.Me.Value = Ann();
        var me = new MeModel(account, scheduler);
        var profile = me.State.Value.Profile!;
        Assert.Equal("http://192.168.1.20:7539/api/users/7/avatar", profile.AvatarUrl);
        Assert.True(profile.CanSwitch);
        Assert.Equal("Managed profile · 192.168.1.20:7539", profile.Line);
        Assert.True(me.State.Value.TrackDefaults!.Editable);
        Assert.False(me.State.Value.TrackDefaults.ReadsSubtitles);
        // Without the play scope, the defaults are changed on the web.
        account.Me.Value = Ann("library");
        Assert.False(me.State.Value.TrackDefaults!.Editable);
        Assert.False(me.State.Value.Profile!.CanSwitch);
    }

    [Fact]
    public void A_track_default_shows_at_once_and_the_servers_answer_is_kept()
    {
        account.Me.Value = Ann();
        var me = new MeModel(account, scheduler);
        var told = new List<MeEvent>();
        me.Told += told.Add;
        account.Saving = new TaskCompletionSource<bool>();
        me.SetSubtitleMode("always");
        Assert.Equal("always", me.State.Value.TrackDefaults!.Defaults.SubtitleMode);
        Assert.True(me.State.Value.TrackDefaults.Saving);
        Assert.True(me.State.Value.TrackDefaults.ReadsSubtitles);
        // One change at a time.
        me.SetAudioLanguage("fre");
        Assert.Single(account.Sent);
        account.Saving.SetResult(true);
        Assert.Equal("from the server", account.Me.Value!.DisplayName);
        Assert.Equal(new MeEvent.Saved(TrackField.SubtitleMode), Assert.Single(told));
        Assert.Equal(1, account.TracksChanges);
        // The same value again sends nothing.
        account.Saving = null;
        me.SetSubtitleMode("always");
        Assert.Single(account.Sent);
    }

    [Fact]
    public void A_server_that_refuses_shows_them_read_only_and_a_failure_puts_them_back()
    {
        account.Me.Value = Ann();
        var me = new MeModel(account, scheduler);
        var told = new List<MeEvent>();
        me.Told += told.Add;
        account.SaveFailure = new ApiException(500, "internal", "it broke");
        me.SetAudioLanguage("fre");
        Assert.Equal("eng", me.State.Value.TrackDefaults!.Defaults.AudioLanguage);
        Assert.IsType<MeEvent.Failed>(told.Last());
        account.SaveFailure = new ApiException(404, "not_found", "no such route");
        me.SetAudioLanguage("fre");
        Assert.IsType<MeEvent.ReadOnly>(told.Last());
        Assert.False(me.State.Value.TrackDefaults!.Editable);
        Assert.Equal("eng", account.Me.Value!.AudioLanguage);
    }

    [Fact]
    public void Signing_out_ends_the_sign_in_and_says_which_server_it_was()
    {
        account.Me.Value = Ann();
        var me = new MeModel(account, scheduler);
        var told = new List<MeEvent>();
        me.Told += told.Add;
        me.SignOut();
        Assert.Equal(1, account.SignOuts);
        Assert.Equal(new MeEvent.SignedOut("http://192.168.1.20:7539/"), Assert.Single(told));
    }
}

/// <summary>Settings on TV: the Android app's TvSettingsScreen and SettingsViewModel, as data.</summary>
public class SettingsModelTests
{
    readonly FakeAccount account = new FakeAccount();
    readonly FakeScheduler scheduler = new FakeScheduler();

    SettingsModel Model(string scope = "library play profiles", string mode = "forced")
    {
        account.Me.Value = new User { Id = 7, Name = "Ann", Scope = scope, AudioLanguage = "eng", SubtitleMode = mode };
        return new SettingsModel(account, new MeModel(account, scheduler));
    }

    static IReadOnlyList<string> Keys(IEnumerable<SettingsItem> items) => items.Select(i => i.Key).ToList();

    static SettingsItem.Row Row(SettingsModel m, SettingsCategory c, string key) => (SettingsItem.Row)m.Items(c).First(i => i.Key == key);

    [Fact]
    public void The_categories_are_androids_in_its_order()
    {
        Assert.Equal(
            new[] { "Profile", "Playback", "Audio", "Subtitles and display", "Server", "Diagnostics", "About", "Sign out" },
            SettingsModel.Categories.Select(SettingsText.CategoryLabel));
        Assert.Equal("log-out", SettingsText.CategoryIcon(SettingsCategory.SignOut));
    }

    [Fact]
    public void Profile_has_switching_the_track_defaults_and_the_ways_to_the_rest()
    {
        var m = Model();
        Assert.Equal(new[] { "profile-header", "row-switch", "row-audio", "row-subtitles", "row-language", "row-history", "row-sort-libraries", "row-web" },
            Keys(m.Items(SettingsCategory.Profile)));
        Assert.Equal("row-switch", m.FirstRow(SettingsCategory.Profile));
        Assert.Equal("English", Row(m, SettingsCategory.Profile, "row-audio").Value);
        Assert.Equal("Only for foreign dialogue", Row(m, SettingsCategory.Profile, "row-subtitles").Value);
        Assert.Equal(new SettingAction.Restart(new PageKey.Profiles()), Row(m, SettingsCategory.Profile, "row-switch").Action);

        // Subtitles in a language to read add their language; a household adds Always start as.
        account.Profiles.AddRange(new[] { new HouseholdProfile { Id = 7, Name = "Ann" }, new HouseholdProfile { Id = 8, Name = "Bo" } });
        m = Model(mode: "foreign");
        Assert.Equal(new[] { "profile-header", "row-switch", "row-audio", "row-subtitles", "row-subtitle-language", "row-language", "row-history", "row-sort-libraries", "row-start-as", "row-web" },
            Keys(m.Items(SettingsCategory.Profile)));
        Assert.Equal("Same as the audio language", Row(m, SettingsCategory.Profile, "row-subtitle-language").Value);
        Assert.Equal("Profile used last", Row(m, SettingsCategory.Profile, "row-start-as").Value);

        // Without the play scope the defaults say so and open the web.
        m = Model(scope: "library");
        Assert.Equal("English · change on the web", Row(m, SettingsCategory.Profile, "row-audio").Value);
        Assert.Equal(new SettingAction.Web("/account"), Row(m, SettingsCategory.Profile, "row-audio").Action);
        Assert.Equal("row-audio", m.FirstRow(SettingsCategory.Profile));
    }

    [Fact]
    public void The_other_categories_have_their_rows()
    {
        var m = Model();
        // Matching the refresh rate and tunneled playback are Android's own: not on a Samsung TV.
        Assert.Equal(new[] { "row-quality", "row-autoplay", "row-details" }, Keys(m.Items(SettingsCategory.Playback)));
        Assert.Equal("Automatic", Row(m, SettingsCategory.Playback, "row-quality").Value);
        Assert.True(Row(m, SettingsCategory.Playback, "row-autoplay").Checked);
        Assert.Equal(7, m.Items(SettingsCategory.Audio).Count);
        Assert.Equal("Dolby Atmos", Row(m, SettingsCategory.Audio, "row-passthrough-eac3-joc").Label);
        Assert.Equal("Auto", Row(m, SettingsCategory.Audio, "row-passthrough-dts-hd").Value);
        Assert.Equal("Normal", Row(m, SettingsCategory.Display, "row-subtitle-size").Value);
        Assert.Equal("Bottom", Row(m, SettingsCategory.Display, "row-subtitle-position").Value);
        Assert.Equal("192.168.1.20:7539", Row(m, SettingsCategory.Server, "row-server").Label);
        Assert.Equal(new SettingAction.Restart(new PageKey.Servers()), Row(m, SettingsCategory.Server, "row-switch-server").Action);
        Assert.Equal(new SettingAction.Open(new DiagnosticsPage()), Row(m, SettingsCategory.Diagnostics, "row-diagnostics").Action);
        Assert.Equal("QE55S95D", Assert.Single(((SettingsItem.Facts)m.Items(SettingsCategory.Diagnostics)[1]).List).Value);
        Assert.Equal("0.1.0", Row(m, SettingsCategory.About, "row-version").Value);
        // The version, the licences under their heading, and TMDB's credit under its own, as TMDB's terms ask.
        Assert.Equal(SettingsModel.Licences.Count + 4, m.Items(SettingsCategory.About).Count);
        Assert.Equal("This product uses the TMDB API but is not endorsed or certified by TMDB.", Row(m, SettingsCategory.About, "row-tmdb").Value);
        var signOut = m.Items(SettingsCategory.SignOut);
        Assert.Equal("Sign this TV out of 192.168.1.20:7539. You'll need a new link code to sign in again.", ((SettingsItem.Intro)signOut[0]).Text);
        Assert.True(((SettingsItem.Row)signOut[1]).Danger);
        Assert.Equal("You'll be signed out of 192.168.1.20:7539.", SettingsText.SignOutBody("http://192.168.1.20:7539"));
    }

    [Fact]
    public void A_choice_opens_its_panel_on_the_one_in_force_and_saves_at_once()
    {
        var m = Model();
        var quality = m.Panel(new SettingPick.Quality());
        Assert.Equal(
            new[] { "Original", "Automatic", "30 Mbps 4K", "15 Mbps 4K", "20 Mbps 1080p", "12 Mbps 1080p", "8 Mbps 1080p", "4 Mbps 720p", "2 Mbps 480p" },
            quality.Choices.Select(c => c.Label));
        Assert.Equal(1, quality.Focus);
        Assert.Equal("Dynamically adjust to conditions.", quality.Choices[1].Hint);
        m.Choose(new SettingPick.Quality(), 0);
        Assert.Equal("Original", account.Settings.Value.Quality);
        Assert.Equal(0, m.Panel(new SettingPick.Quality()).Focus);
        m.Choose(new SettingPick.Quality(), 6);
        Assert.Equal("8m", account.Settings.Value.Quality);
        Assert.Equal("row-quality", quality.Pick.RowKey);

        m.Choose(new SettingPick.Passthrough("truehd"), 1);
        Assert.Equal(PassthroughMode.On, account.Settings.Value.PassthroughOf("truehd"));
        Assert.Equal("On", Row(m, SettingsCategory.Audio, "row-passthrough-truehd").Value);
        m.Choose(new SettingPick.SubtitleSize(), 3);
        Assert.Equal("Huge", Row(m, SettingsCategory.Display, "row-subtitle-size").Value);
        m.Choose(new SettingPick.SubtitleBackground(), 1);
        Assert.Equal(SubtitleBackground.DarkBox, account.Settings.Value.SubtitleBackground);

        var modes = m.Panel(new SettingPick.SubtitleMode());
        Assert.Equal(new[] { "Off", "Only for foreign dialogue", "When the audio is in another language", "Always" }, modes.Choices.Select(c => c.Label));
        Assert.Equal(1, modes.Focus);
        var languages = m.Panel(new SettingPick.AudioLanguage());
        Assert.Equal("The file's default", languages.Choices[0].Label);
        Assert.Equal("English", languages.Choices[languages.Focus].Label);
        m.Choose(new SettingPick.AudioLanguage(), 0);
        Assert.Null(account.Sent.Last().AudioLanguage);
    }

    [Fact]
    public void Language_is_the_profiles_own_or_Automatic_and_says_what_that_is_here()
    {
        var m = Model();
        var row = Row(m, SettingsCategory.Profile, "row-language");
        Assert.Equal(("Language", "languages"), (row.Label, row.Icon));
        Assert.Equal(new SettingAction.Pick(new SettingPick.Language()), row.Action);
        // Automatic is the TV's language, in itself, before the server's.
        Assert.Equal("Automatic (English)", row.Value);
        account.TvLanguages = new[] { "de-DE", "es-MX" };
        account.ServerLanguage = "fr";
        Assert.Equal("Automatic (Español)", Row(m, SettingsCategory.Profile, "row-language").Value);
        // A TV in a language Pley doesn't have: the server's.
        account.TvLanguages = new[] { "de-DE" };
        Assert.Equal("Automatic (Français)", Row(m, SettingsCategory.Profile, "row-language").Value);
        // The one picked on this TV with the globe, before the TV's.
        account.DeviceLanguage = "en";
        Assert.Equal("Automatic (English)", Row(m, SettingsCategory.Profile, "row-language").Value);
        account.DeviceLanguage = null;

        var panel = m.Panel(new SettingPick.Language());
        Assert.Equal("Language", panel.Title);
        Assert.Equal(new[] { "Automatic (Français)", "English", "Français", "Español" }, panel.Choices.Select(c => c.Label));
        Assert.Equal(0, panel.Focus);
        Assert.Equal("row-language", panel.Pick.RowKey);

        // Choosing waits for the server's answer, which becomes who is watching: the app follows it.
        account.Saving = new TaskCompletionSource<bool>();
        m.Choose(new SettingPick.Language(), 3);
        Assert.Equal(new string?[] { "es" }, account.LanguagesSent);
        Assert.Null(account.Me.Value!.Language);
        Assert.True(m.State.Value.Me.Language!.Saving);
        // One change at a time.
        m.Choose(new SettingPick.Language(), 2);
        Assert.Single(account.LanguagesSent);
        account.Saving.SetResult(true);
        Assert.Equal(("es", "from the server"), (account.Me.Value!.Language, account.Me.Value.DisplayName));
        Assert.Equal("Español", Row(m, SettingsCategory.Profile, "row-language").Value);
        Assert.Equal(3, m.Panel(new SettingPick.Language()).Focus);
        // The server's again.
        account.Saving = null;
        m.Choose(new SettingPick.Language(), 0);
        Assert.Equal(new string?[] { "es", null }, account.LanguagesSent);
        Assert.Null(account.Me.Value!.Language);
    }

    [Fact]
    public void Language_is_changed_on_the_web_without_the_play_scope_or_the_servers_route()
    {
        var m = Model(scope: "library");
        var row = Row(m, SettingsCategory.Profile, "row-language");
        Assert.Equal("Automatic (English) · change on the web", row.Value);
        Assert.Equal(new SettingAction.Web("/account"), row.Action);

        m = Model();
        var told = new List<string>();
        m.Told += told.Add;
        account.SaveFailure = new ApiException(500, "internal", "it broke");
        m.Choose(new SettingPick.Language(), 2);
        Assert.Equal("It broke.", told.Last());
        Assert.Null(account.Me.Value!.Language);
        Assert.True(m.State.Value.Me.Language!.Editable);
        account.SaveFailure = new ApiException(404, "not_found", "no such route");
        m.Choose(new SettingPick.Language(), 2);
        Assert.False(m.State.Value.Me.Language!.Editable);
        Assert.Equal(new SettingAction.Web("/account"), Row(m, SettingsCategory.Profile, "row-language").Action);
    }

    [Fact]
    public void Always_start_as_names_the_households_profiles()
    {
        account.Profiles.AddRange(new[] { new HouseholdProfile { Id = 7, Name = "Ann" }, new HouseholdProfile { Id = 8, Name = "Bo" } });
        var m = Model();
        var panel = m.Panel(new SettingPick.AlwaysStartAs());
        Assert.Equal(new[] { "Profile used last", "Ann", "Bo" }, panel.Choices.Select(c => c.Label));
        m.Choose(new SettingPick.AlwaysStartAs(), 2);
        Assert.Equal(8L, account.StartAs);
        Assert.Equal("Bo", Row(m, SettingsCategory.Profile, "row-start-as").Value);
        Assert.Equal(2, m.Panel(new SettingPick.AlwaysStartAs()).Focus);
    }

    [Fact]
    public void Switches_toggle_and_the_me_pages_events_become_messages()
    {
        var m = Model();
        m.Toggle(SettingSwitch.ReduceMotion);
        Assert.True(account.Settings.Value.ReduceMotion);
        Assert.True(Row(m, SettingsCategory.Display, "row-reduce-motion").Checked);
        var told = new List<string>();
        string? signedOut = "none";
        m.Told += told.Add;
        m.SignedOut += s => signedOut = s;
        m.Choose(new SettingPick.SubtitleMode(), 3);
        Assert.Equal(new[] { "Saved" }, told);
        m.SignOut();
        Assert.Equal("http://192.168.1.20:7539/", signedOut);
    }
}

/// <summary>Diagnostics, and Settings' passthrough in what the TV reports.</summary>
public class DiagnosticsTests
{
    readonly FakeScheduler scheduler = new FakeScheduler();

    static readonly TizenFacts Facts = new TizenFacts { Tizen = 8, Year = 2024, Model = "QE55S95D", OutputMode = "DOLBY_DIGITAL_PLUS" };

    [Fact]
    public void Diagnostics_says_what_the_tv_is_and_what_it_plays()
    {
        var m = new DiagnosticsModel("0.1.0", () => Task.FromResult(Facts), scheduler);
        var s = m.State.Value;
        Assert.False(s.Probing);
        Assert.Equal(new Fact("App", "Pley 0.1.0"), s.Device[0]);
        Assert.Contains(new Fact("System", "Tizen 8.0 (2024 models)"), s.Device);
        Assert.Contains(new Fact("Tier", "high"), s.Device);
        Assert.Equal("HDR: HDR10, HDR10+, HLG · no Samsung TV shows Dolby Vision", s.Lines[3]);
        Assert.Equal("Sent to the receiver: ac3, eac3 (sound output DOLBY_DIGITAL_PLUS)", s.Lines[6]);
        Assert.Equal("Streams: HLS ts, fmp4", s.Lines.Last());
    }

    [Fact]
    public void A_check_that_failed_can_be_made_again()
    {
        var fail = true;
        var m = new DiagnosticsModel("0.1.0", () => fail ? Task.FromException<TizenFacts>(new InvalidOperationException("no")) : Task.FromResult(Facts), scheduler);
        Assert.NotNull(m.State.Value.Error);
        Assert.Empty(m.State.Value.Lines);
        fail = false;
        m.Probe();
        Assert.Null(m.State.Value.Error);
        Assert.NotEmpty(m.State.Value.Lines);
    }

    [Fact]
    public void Passthrough_choices_add_and_take_out_formats_the_sound_output_says()
    {
        Assert.Equal(new[] { "ac3", "eac3" }, (string[])TizenCaps.Of(Facts)["audio_passthrough"]!);
        var off = TizenCaps.Of(Facts, new Dictionary<string, PassthroughMode> { ["ac3"] = PassthroughMode.Off, ["truehd"] = PassthroughMode.On, ["dts-hd"] = PassthroughMode.On });
        Assert.Equal(new[] { "eac3", "truehd", "dts" }, (string[])off["audio_passthrough"]!);
        // Off for every Dolby Digital Plus, Atmos included: none of it goes to the receiver.
        var none = TizenCaps.Of(Facts, new Dictionary<string, PassthroughMode> { ["ac3"] = PassthroughMode.Off, ["eac3"] = PassthroughMode.Off, ["eac3-joc"] = PassthroughMode.Off });
        Assert.Empty((string[])none["audio_passthrough"]!);
        Assert.Equal(6, none["audio_channels"]);
    }
}
