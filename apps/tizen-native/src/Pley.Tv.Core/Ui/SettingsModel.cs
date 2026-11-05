using System;
using System.Collections.Generic;
using System.Linq;
using Pley.Tv.Core.Api;
using Pley.Tv.Core.Player;

namespace Pley.Tv.Core.Ui;

/// <summary>The TV settings' categories, top to bottom (design spec §4.8).</summary>
public enum SettingsCategory
{
    Profile,
    Playback,
    Audio,
    Display,
    Server,
    Diagnostics,
    About,
    /// <summary>Selected, it asks first (<see cref="SettingsText.SignOutTitle"/>).</summary>
    SignOut,
}

/// <summary>A switch of Settings, which Select toggles.</summary>
public enum SettingSwitch
{
    Autoplay,
    PlaybackDetails,
    ReduceMotion,
}

/// <summary>A choice made in a side panel (<see cref="SettingsModel.Panel"/>).</summary>
public abstract record SettingPick
{
    /// <summary>The row the panel gives focus back to.</summary>
    public abstract string RowKey { get; }

    public sealed record AudioLanguage : SettingPick
    {
        public override string RowKey => "row-audio";
    }

    public sealed record SubtitleMode : SettingPick
    {
        public override string RowKey => "row-subtitles";
    }

    public sealed record SubtitleLanguage : SettingPick
    {
        public override string RowKey => "row-subtitle-language";
    }

    public sealed record AlwaysStartAs : SettingPick
    {
        public override string RowKey => "row-start-as";
    }

    /// <summary>The language Pley is in for the profile (<see cref="MeModel.SetLanguage"/>).</summary>
    public sealed record Language : SettingPick
    {
        public override string RowKey => "row-language";
    }

    public sealed record Quality : SettingPick
    {
        public override string RowKey => "row-quality";
    }

    /// <summary>A sound format of <see cref="Settings.PassthroughFormats"/>.</summary>
    public sealed record Passthrough(string Format) : SettingPick
    {
        public override string RowKey => "row-passthrough-" + Format;
    }

    public sealed record SubtitleSize : SettingPick
    {
        public override string RowKey => "row-subtitle-size";
    }

    public sealed record SubtitleBackground : SettingPick
    {
        public override string RowKey => "row-subtitle-background";
    }

    public sealed record SubtitleFont : SettingPick
    {
        public override string RowKey => "row-subtitle-font";
    }

    public sealed record SubtitleRaise : SettingPick
    {
        public override string RowKey => "row-subtitle-position";
    }
}

/// <summary>What selecting a row does.</summary>
public abstract record SettingAction
{
    /// <summary>A row that only says something (the server, the version, a licence).</summary>
    public sealed record None : SettingAction;

    /// <summary>Opens the side panel of <paramref name="Choice"/> (<see cref="SettingsModel.Panel"/>).</summary>
    public sealed record Pick(SettingPick Choice) : SettingAction;

    /// <summary>Toggles the switch (<see cref="SettingsModel.Toggle"/>).</summary>
    public sealed record Toggle(SettingSwitch Switch) : SettingAction;

    /// <summary>Opens a page on the back stack: Watch history, Libraries, Diagnostics.</summary>
    public sealed record Open(PageKey Key) : SettingAction;

    /// <summary>Starts over at a page: Who's watching? (Switch profile), the servers (Switch server).</summary>
    public sealed record Restart(PageKey Key) : SettingAction;

    /// <summary>
    /// A page of Pley's web app (<paramref name="Path"/> on the server), since a TV opens no
    /// browser: a dialog with the address and its QR code (the Android app's TvWebDialog).
    /// </summary>
    public sealed record Web(string Path) : SettingAction;

    /// <summary>Asks "Sign out?", Cancel focused; Sign out (in danger text) calls <see cref="SettingsModel.SignOut"/>.</summary>
    public sealed record SignOut : SettingAction;
}

/// <summary>A line of a category's pane, top to bottom.</summary>
public abstract record SettingsItem
{
    public abstract string Key { get; }

    /// <summary>Profile's head: the avatar (48 dp), the name, and <see cref="MeProfile.Line"/>; 60 dp of room while it isn't known.</summary>
    public sealed record ProfileHeader(MeProfile? Profile) : SettingsItem
    {
        public override string Key => "profile-header";
    }

    /// <summary>
    /// A row (TvSettingRow): its icon by Lucide's name, the label in body, the value under it in
    /// caption, muted; a switch at its end when <paramref name="Checked"/> isn't null, else a
    /// chevron when <paramref name="Chevron"/>. Danger rows read in danger text.
    /// </summary>
    public sealed record Row(string RowKey, string Label, SettingAction Action, string? Value = null, string? Icon = null, bool? Checked = null,
        bool Chevron = false, bool Danger = false, bool Enabled = true) : SettingsItem
    {
        public override string Key => RowKey;
    }

    /// <summary>A heading over rows (label, subtle; 20 dp in, 16 above, 6 below): "Advanced", "Open-source licences".</summary>
    public sealed record Heading(string Text) : SettingsItem
    {
        public override string Key => "heading:" + Text;
    }

    /// <summary>Words above a pane's rows (body, muted; 20 dp in, 8 below).</summary>
    public sealed record Intro(string Text) : SettingsItem
    {
        public override string Key => "intro:" + Text;
    }

    /// <summary>The TV's facts under "Open diagnostics": label (caption, muted, 150 dp) and value, 6 dp apart, 20 dp in and 12 below the row.</summary>
    public sealed record Facts(IReadOnlyList<Fact> List) : SettingsItem
    {
        public override string Key => "facts";
    }
}

/// <summary>One choice of a side panel: its label, what it does (under it), and whether it is the one in force.</summary>
public sealed record SettingChoice(string Label, string? Hint, bool Checked);

/// <summary>
/// A choice made in a side panel (TvChoicePanel): its title, one line per choice with the current
/// one checked and focused first (<paramref name="Focus"/>); choosing closes it, Back too, and
/// focus goes back to the row that opened it (<see cref="SettingPick.RowKey"/>).
/// </summary>
public sealed record ChoicePanel(SettingPick Pick, string Title, IReadOnlyList<SettingChoice> Choices, int Focus);

/// <summary>Settings as the screen draws them: the TV's settings, the Me page and the household.</summary>
public sealed record SettingsState(Settings Settings, MeState Me, IReadOnlyList<HouseholdProfile> Profiles, long? AlwaysStartAs);

/// <summary>What Settings reads and changes besides the Me page: the app's state, or a fake in tests.</summary>
public interface ISettingsSource
{
    Signal<Settings> Settings { get; }

    void UpdateSettings(Func<Settings, Settings> change);

    /// <summary>The household as the TV last saw it, for "Always start as"; empty when unknown.</summary>
    IReadOnlyList<HouseholdProfile> Household();

    long? AlwaysStartAs();

    void SetAlwaysStartAs(long? id);

    /// <summary>The app's version: "0.1.0".</summary>
    string Version { get; }

    /// <summary>The TV's facts under "Open diagnostics".</summary>
    IReadOnlyList<Fact> DeviceFacts { get; }

    /// <summary>The server's language, which "Server default" stands for; null when it has none, or hasn't said.</summary>
    string? ServerLanguage { get; }

    /// <summary>The TV's languages, which "Server default" stands for when the server has none.</summary>
    IReadOnlyList<string> TvLanguages { get; }
}

/// <summary>Settings' source in the app: <see cref="AppState"/>, with the TV's facts the Tizen app reads.</summary>
public sealed class AppSettingsSource : ISettingsSource
{
    readonly AppState app;

    public AppSettingsSource(AppState app, IReadOnlyList<Fact> deviceFacts)
    {
        this.app = app;
        DeviceFacts = deviceFacts;
    }

    public Signal<Settings> Settings => app.Settings;

    public void UpdateSettings(Func<Settings, Settings> change) => app.UpdateSettings(change);

    public IReadOnlyList<HouseholdProfile> Household() => app.Profiles.Value ?? app.SavedHousehold();

    public long? AlwaysStartAs() => app.AlwaysStartAs();

    public void SetAlwaysStartAs(long? id) => app.SetAlwaysStartAs(id);

    public string Version => app.Platform.Version;

    public IReadOnlyList<Fact> DeviceFacts { get; }

    public string? ServerLanguage => app.ServerLanguage;

    public IReadOnlyList<string> TvLanguages => app.TvLanguages;
}

/// <summary>
/// The TV's settings (the Android app's TvSettingsScreen, with its SettingsViewModel and
/// MeViewModel): in two panes, the categories on the left (Profile, Playback, Audio, Subtitles
/// and display, Server, Diagnostics, About, Sign out; 256 dp at x = 96, rows 2 dp apart) and the
/// focused category's rows on the right (x = 376 to 912; rows 44 dp at least, 20 dp in, 2 dp
/// apart, a pill filled with the accent when focused). Focus on a category shows its rows
/// (<see cref="Items"/>); Right or Select goes into them, at <see cref="FirstRow"/>. Choices open
/// a side panel (<see cref="Panel"/>, <see cref="Choose"/>); switches toggle on Select. What the
/// page tells (<see cref="Told"/>) shows for <see cref="MessageMs"/> at the top right.
///
/// Left out of the TV's Android settings, as a Samsung TV can't have them: matching the refresh
/// rate (Android's display modes), tunneled playback (Android's video tunneling), and software
/// video decoding (Android's emulators in debug builds).
/// </summary>
public sealed class SettingsModel : IDisposable
{
    /// <summary>How long a message ("Saved") shows.</summary>
    public const long MessageMs = 3_500;

    /// <summary>The categories' column, and where the rows start (design spec §4.8).</summary>
    public const float CategoriesX = 96, CategoriesWidth = 256, PaneX = 376, PaneRight = 912;

    readonly ISettingsSource source;
    readonly List<IDisposable> watches = new List<IDisposable>();

    public SettingsModel(ISettingsSource source, MeModel me)
    {
        this.source = source;
        Me = me;
        State = new Signal<SettingsState>(Snapshot());
        watches.Add(source.Settings.Watch(_ => State.Value = Snapshot()));
        watches.Add(me.State.Watch(_ => State.Value = Snapshot()));
        me.Told += OnMe;
    }

    public MeModel Me { get; }

    public Signal<SettingsState> State { get; }

    /// <summary>A message to show a while: "Saved", why a change wasn't, or that the server lets these be changed on the web only.</summary>
    public event Action<string>? Told;

    /// <summary>Signed out of the server (its address; null when unknown): the app starts over at signing in to it.</summary>
    public event Action<string?>? SignedOut;

    public static IReadOnlyList<SettingsCategory> Categories { get; } = (SettingsCategory[])Enum.GetValues(typeof(SettingsCategory));

    void OnMe(MeEvent e)
    {
        switch (e)
        {
            case MeEvent.Saved:
            case MeEvent.LanguageSaved:
                Told?.Invoke(SettingsText.Saved);
                break;
            case MeEvent.ReadOnly:
                Told?.Invoke(SettingsText.ReadOnly);
                break;
            case MeEvent.Failed f:
                Told?.Invoke(Errors.Message(f.Error));
                break;
            case MeEvent.SignedOut s:
                SignedOut?.Invoke(s.Server);
                break;
        }
    }

    SettingsState Snapshot() => new SettingsState(source.Settings.Value, Me.State.Value, source.Household(), source.AlwaysStartAs());

    /// <summary>The row Select (or Right) on category <paramref name="c"/> goes to.</summary>
    public string FirstRow(SettingsCategory c) => c switch
    {
        SettingsCategory.Profile => State.Value.Me.Profile?.CanSwitch == true ? "row-switch" : "row-audio",
        SettingsCategory.Playback => "row-quality",
        SettingsCategory.Audio => "row-passthrough-" + Settings.PassthroughFormats[0],
        SettingsCategory.Display => "row-subtitle-size",
        SettingsCategory.Server => "row-server",
        SettingsCategory.Diagnostics => "row-diagnostics",
        SettingsCategory.About => "row-version",
        _ => "row-sign-out",
    };

    /// <summary>Category <paramref name="c"/>'s pane, top to bottom.</summary>
    public IReadOnlyList<SettingsItem> Items(SettingsCategory c)
    {
        var state = State.Value;
        var s = state.Settings;
        var me = state.Me;
        var server = me.Profile?.Server;
        var items = new List<SettingsItem>();
        switch (c)
        {
            case SettingsCategory.Profile:
            {
                items.Add(new SettingsItem.ProfileHeader(me.Profile));
                if (me.Profile?.CanSwitch == true)
                    items.Add(new SettingsItem.Row("row-switch", SettingsText.SwitchProfile, new SettingAction.Restart(new PageKey.Profiles()), Icon: "users", Chevron: true));
                var defaults = me.TrackDefaults;
                var editable = defaults?.Editable == true;
                var webOnly = editable ? "" : " · " + SettingsText.ChangeOnWeb;
                SettingAction Edit(SettingPick pick) => editable ? new SettingAction.Pick(pick) : new SettingAction.Web("/account");
                items.Add(new SettingsItem.Row("row-audio", SettingsText.AudioLanguage, Edit(new SettingPick.AudioLanguage()),
                    defaults == null ? null : (TrackLanguages.NameOf(defaults.Defaults.AudioLanguage) ?? SettingsText.FileDefault) + webOnly, "audio-lines"));
                items.Add(new SettingsItem.Row("row-subtitles", SettingsText.Subtitles, Edit(new SettingPick.SubtitleMode()),
                    defaults == null ? null : SettingsText.ModeLabel(defaults.Defaults.SubtitleMode) + webOnly, "captions"));
                if (defaults?.ReadsSubtitles == true)
                    items.Add(new SettingsItem.Row("row-subtitle-language", SettingsText.SubtitleLanguage, Edit(new SettingPick.SubtitleLanguage()),
                        (TrackLanguages.NameOf(defaults.Defaults.SubtitleLanguage) ?? SettingsText.SameAsAudio) + webOnly, "captions"));
                if (me.Language is { } language)
                    items.Add(new SettingsItem.Row("row-language", SettingsText.Language,
                        language.Editable ? new SettingAction.Pick(new SettingPick.Language()) : new SettingAction.Web("/account"),
                        LanguageLabel(language.Code) + (language.Editable ? "" : " · " + SettingsText.ChangeOnWeb), "languages"));
                items.Add(new SettingsItem.Row("row-history", SettingsText.History, new SettingAction.Open(new PageKey.History()), Icon: "history", Chevron: true));
                // The Libraries page sorts them; with three or fewer, the rail lists them and has no way there.
                items.Add(new SettingsItem.Row("row-sort-libraries", SettingsText.SortLibraries, new SettingAction.Open(new PageKey.Libraries()), SettingsText.SortLibrariesValue, "arrow-up-down", Chevron: true));
                if (state.Profiles.Count > 1)
                {
                    var chosen = state.Profiles.FirstOrDefault(p => p.Id == state.AlwaysStartAs)?.Name;
                    items.Add(new SettingsItem.Row("row-start-as", SettingsText.AlwaysStartAs, new SettingAction.Pick(new SettingPick.AlwaysStartAs()), chosen ?? SettingsText.AskWho, "user-round"));
                }
                items.Add(new SettingsItem.Row("row-web", SettingsText.ManageWeb, new SettingAction.Web("/account"), SettingsText.ManageWebValue, "app-window"));
                break;
            }
            case SettingsCategory.Playback:
                items.Add(new SettingsItem.Row("row-quality", SettingsText.Quality, new SettingAction.Pick(new SettingPick.Quality()), SettingsText.QualityLabel(s.Quality), "gauge"));
                items.Add(new SettingsItem.Row("row-autoplay", SettingsText.Autoplay, new SettingAction.Toggle(SettingSwitch.Autoplay), SettingsText.AutoplayHint, "skip-forward", s.AutoplayNext));
                items.Add(new SettingsItem.Row("row-details", SettingsText.PlaybackDetails, new SettingAction.Toggle(SettingSwitch.PlaybackDetails), SettingsText.PlaybackDetailsHint, "info", s.PlaybackDetails));
                break;
            case SettingsCategory.Audio:
                items.Add(new SettingsItem.Intro(SettingsText.PassthroughIntro));
                foreach (var format in Settings.PassthroughFormats)
                    items.Add(new SettingsItem.Row("row-passthrough-" + format, SettingsText.FormatLabel(format), new SettingAction.Pick(new SettingPick.Passthrough(format)),
                        SettingsText.PassthroughLabel(s.PassthroughOf(format)), "volume-2"));
                break;
            case SettingsCategory.Display:
                items.Add(new SettingsItem.Row("row-subtitle-size", SettingsText.SubtitleSize, new SettingAction.Pick(new SettingPick.SubtitleSize()), SettingsText.SizeLabel(s.SubtitleSize), "captions"));
                items.Add(new SettingsItem.Row("row-subtitle-background", SettingsText.SubtitleBackground, new SettingAction.Pick(new SettingPick.SubtitleBackground()), SettingsText.BackgroundLabel(s.SubtitleBackground), "square"));
                items.Add(new SettingsItem.Row("row-subtitle-font", SettingsText.SubtitleFont, new SettingAction.Pick(new SettingPick.SubtitleFont()), SettingsText.FontLabel(s.SubtitleFont), "pencil"));
                items.Add(new SettingsItem.Row("row-subtitle-position", SettingsText.SubtitlePosition, new SettingAction.Pick(new SettingPick.SubtitleRaise()), SettingsText.RaiseLabel(s.SubtitleRaise), "arrow-up"));
                items.Add(new SettingsItem.Row("row-reduce-motion", SettingsText.ReduceMotion, new SettingAction.Toggle(SettingSwitch.ReduceMotion), SettingsText.ReduceMotionHint, "sparkles", s.ReduceMotion));
                break;
            case SettingsCategory.Server:
                items.Add(new SettingsItem.Row("row-server", server != null ? SettingsText.HostPort(server) : SettingsText.ServerUnknown, new SettingAction.None(), SettingsText.ServerValue, "server"));
                items.Add(new SettingsItem.Row("row-switch-server", SettingsText.SwitchServer, new SettingAction.Restart(new PageKey.Servers()), Icon: "arrow-left", Chevron: true));
                break;
            case SettingsCategory.Diagnostics:
                items.Add(new SettingsItem.Row("row-diagnostics", SettingsText.OpenDiagnostics, new SettingAction.Open(new DiagnosticsPage()), SettingsText.OpenDiagnosticsValue, "activity", Chevron: true));
                items.Add(new SettingsItem.Facts(source.DeviceFacts));
                break;
            case SettingsCategory.About:
                items.Add(new SettingsItem.Row("row-version", SettingsText.Version, new SettingAction.None(), source.Version, "info"));
                items.Add(new SettingsItem.Heading(SettingsText.Licences));
                var i = 0;
                foreach (var (name, licence) in Licences)
                    items.Add(new SettingsItem.Row($"licence-{i++}", name, new SettingAction.None(), licence));
                // Discover's titles and pictures: TMDB's terms ask every app that shows them to say this.
                items.Add(new SettingsItem.Heading(SettingsText.Credits));
                items.Add(new SettingsItem.Row("row-tmdb", "TMDB", new SettingAction.None(), Requests.RequestText.CreditAbout));
                break;
            default:
                items.Add(new SettingsItem.Intro(SettingsText.SignOutIntro(server ?? "")));
                items.Add(new SettingsItem.Row("row-sign-out", SettingsText.SignOut, new SettingAction.SignOut(), Icon: "log-out", Danger: true, Enabled: !me.SigningOut));
                break;
        }
        return items;
    }

    /// <summary>What the native app is made of, with their licences (LICENSING.md).</summary>
    public static readonly IReadOnlyList<(string Name, string Licence)> Licences = new[]
    {
        ("Tizen .NET (NUI, Multimedia…)", "Apache-2.0"),
        (".NET Standard, System.Text.Json", "MIT"),
        ("Lucide icons", "ISC"),
        ("QR Code generator (Nayuki)", "MIT"),
        ("Roboto", "OFL-1.1"),
    };

    /// <summary>The side panel of <paramref name="pick"/>, its current choice checked and focused first.</summary>
    public ChoicePanel Panel(SettingPick pick)
    {
        var state = State.Value;
        var s = state.Settings;
        var defaults = state.Me.TrackDefaults?.Defaults;
        (string Title, IReadOnlyList<(string Label, string? Hint)> Choices, int Current) p = pick switch
        {
            SettingPick.AudioLanguage => (SettingsText.AudioLanguage, TrackChoices(SettingsText.FileDefault), LanguageIndex(defaults?.AudioLanguage)),
            SettingPick.SubtitleMode => (SettingsText.Subtitles, SubtitleModes.Select(m => (SettingsText.ModeLabel(m), (string?)SettingsText.ModeHint(m))).ToList(),
                Math.Max(0, Index(SubtitleModes, defaults?.SubtitleMode ?? "forced"))),
            SettingPick.SubtitleLanguage => (SettingsText.SubtitleLanguage, TrackChoices(SettingsText.SameAsAudio), LanguageIndex(defaults?.SubtitleLanguage)),
            SettingPick.Language => (SettingsText.Language, new[] { LanguageLabel(null) }.Concat(Languages.All.Select(Languages.NameOf)).Select(l => (l, (string?)null)).ToList(),
                state.Me.Language?.Code is { } code ? Index(Languages.All, code) + 1 : 0),
            SettingPick.AlwaysStartAs => (SettingsText.AlwaysStartAs,
                new[] { (SettingsText.AskWho, (string?)null) }.Concat(state.Profiles.Select(x => (x.Name, (string?)null))).ToList(),
                state.AlwaysStartAs is { } id ? state.Profiles.Select((x, n) => (x, n)).Where(x => x.x.Id == id).Select(x => x.n + 1).FirstOrDefault() : 0),
            SettingPick.Quality => (SettingsText.Quality, Qualities.All.Select(q => (SettingsText.QualityLabel(q.Id), SettingsText.QualityHint(q.Id))).ToList(),
                Math.Max(0, Index(Qualities.All, Qualities.ByIdOrLabel(s.Quality)))),
            SettingPick.Passthrough f => (SettingsText.FormatLabel(f.Format), Modes.Select(m => (SettingsText.PassthroughLabel(m), (string?)SettingsText.PassthroughHint(m))).ToList(),
                Index(Modes, s.PassthroughOf(f.Format))),
            SettingPick.SubtitleSize => (SettingsText.SubtitleSize, Settings.SubtitleSizes.Select(x => (SettingsText.SizeLabel(x), (string?)null)).ToList(), Math.Max(0, Index(Settings.SubtitleSizes, s.SubtitleSize))),
            SettingPick.SubtitleBackground => (SettingsText.SubtitleBackground, Backgrounds.Select(b => (SettingsText.BackgroundLabel(b), (string?)null)).ToList(), Index(Backgrounds, s.SubtitleBackground)),
            SettingPick.SubtitleFont => (SettingsText.SubtitleFont, Fonts.Select(x => (SettingsText.FontLabel(x), (string?)null)).ToList(), Math.Max(0, Index(Fonts, s.SubtitleFont))),
            _ => (SettingsText.SubtitlePosition, Settings.SubtitleRaises.Select(x => (SettingsText.RaiseLabel(x), (string?)null)).ToList(), Math.Max(0, Index(Settings.SubtitleRaises, s.SubtitleRaise))),
        };
        return new ChoicePanel(pick, p.Title, p.Choices.Select((c, n) => new SettingChoice(c.Label, c.Hint, n == p.Current)).ToList(), p.Current);
    }

    /// <summary>Makes choice <paramref name="index"/> of <paramref name="pick"/>'s panel; saved at once (the profile's on the server).</summary>
    public void Choose(SettingPick pick, int index)
    {
        if (index < 0) return;
        switch (pick)
        {
            case SettingPick.AudioLanguage:
                Me.SetAudioLanguage(LanguageAt(index));
                break;
            case SettingPick.SubtitleMode when index < SubtitleModes.Count:
                Me.SetSubtitleMode(SubtitleModes[index]);
                break;
            case SettingPick.SubtitleLanguage:
                Me.SetSubtitleLanguage(LanguageAt(index));
                break;
            case SettingPick.Language when index <= Languages.All.Count:
                Me.SetLanguage(index == 0 ? null : Languages.All[index - 1]);
                break;
            case SettingPick.AlwaysStartAs:
                var profiles = State.Value.Profiles;
                if (index > profiles.Count) return;
                source.SetAlwaysStartAs(index == 0 ? null : profiles[index - 1].Id);
                State.Value = Snapshot();
                break;
            case SettingPick.Quality when index < Qualities.All.Count:
                Update(s => s with { Quality = Qualities.IdOf(Qualities.All[index]) });
                break;
            case SettingPick.Passthrough f when index < Modes.Count:
                var mode = Modes[index];
                Update(s =>
                {
                    var next = s.Passthrough.ToDictionary(p => p.Key, p => p.Value);
                    next[f.Format] = mode;
                    return s with { Passthrough = next };
                });
                break;
            case SettingPick.SubtitleSize when index < Settings.SubtitleSizes.Count:
                Update(s => s with { SubtitleSize = Settings.SubtitleSizes[index] });
                break;
            case SettingPick.SubtitleBackground when index < Backgrounds.Count:
                Update(s => s with { SubtitleBackground = Backgrounds[index] });
                break;
            case SettingPick.SubtitleFont when index < Fonts.Count:
                Update(s => s with { SubtitleFont = Fonts[index] });
                break;
            case SettingPick.SubtitleRaise when index < Settings.SubtitleRaises.Count:
                Update(s => s with { SubtitleRaise = Settings.SubtitleRaises[index] });
                break;
        }
    }

    /// <summary>Toggles <paramref name="which"/>, saved at once.</summary>
    public void Toggle(SettingSwitch which) => Update(which switch
    {
        SettingSwitch.Autoplay => s => s with { AutoplayNext = !s.AutoplayNext },
        SettingSwitch.PlaybackDetails => s => s with { PlaybackDetails = !s.PlaybackDetails },
        _ => (Func<Settings, Settings>)(s => s with { ReduceMotion = !s.ReduceMotion }),
    });

    /// <summary>Signs out, once the person said yes to "Sign out?".</summary>
    public void SignOut() => Me.SignOut();

    public void Dispose()
    {
        Me.Told -= OnMe;
        foreach (var w in watches) w.Dispose();
        watches.Clear();
    }

    void Update(Func<Settings, Settings> change) => source.UpdateSettings(change);

    /// <summary>The order the web lists subtitle modes in.</summary>
    static readonly IReadOnlyList<string> SubtitleModes = new[] { "off", "forced", "foreign", "always" };

    static readonly IReadOnlyList<PassthroughMode> Modes = new[] { PassthroughMode.Auto, PassthroughMode.On, PassthroughMode.Off };

    static readonly IReadOnlyList<SubtitleBackground> Backgrounds = new[] { SubtitleBackground.None, SubtitleBackground.DarkBox };

    /// <summary>
    /// The subtitle fonts. Android offers Casual only where the phone has a casual typeface; the
    /// native app draws its subtitles in the TV's own fonts, which the screens decide on, so all
    /// four are offered here.
    /// </summary>
    static readonly IReadOnlyList<SubtitleFont> Fonts = new[] { SubtitleFont.Sans, SubtitleFont.Serif, SubtitleFont.Monospace, SubtitleFont.Casual };

    static IReadOnlyList<(string, string?)> TrackChoices(string none) =>
        new[] { (none, (string?)null) }.Concat(TrackLanguages.All.Select(l => (l.Name, (string?)null))).ToList();

    /// <summary>
    /// The Language row's value: the profile's language, named in itself ("Français"), or for
    /// none "Server default" with what that stands for: the server's language, or this TV's.
    /// </summary>
    string LanguageLabel(string? code)
    {
        if (code != null) return Languages.NameOf(code);
        var fallback = source.ServerLanguage is { } server
            ? Languages.NameOf(server)
            : SettingsText.TvLanguage(Languages.NameOf(Languages.Pick(null, null, source.TvLanguages)));
        return SettingsText.ServerDefault(fallback);
    }

    static int LanguageIndex(string? code)
    {
        if (code == null) return 0;
        var all = TrackLanguages.All;
        for (var i = 0; i < all.Count; i++)
            if (all[i].Code == code) return i + 1;
        return 0;
    }

    static string? LanguageAt(int index)
    {
        var all = TrackLanguages.All;
        return index == 0 || index > all.Count ? null : all[index - 1].Code;
    }

    static int Index<T>(IReadOnlyList<T> list, T value)
    {
        for (var i = 0; i < list.Count; i++)
            if (EqualityComparer<T>.Default.Equals(list[i], value)) return i;
        return -1;
    }
}

/// <summary>Diagnostics' page, opened from Settings.</summary>
public sealed record DiagnosticsPage : PageKey;
