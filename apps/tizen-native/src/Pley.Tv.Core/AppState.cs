using System;
using System.Collections.Generic;
using System.Linq;
using System.Security.Cryptography;
using System.Text.Json;
using System.Text.Json.Serialization;
using System.Threading.Tasks;
using Pley.Tv.Core.Api;
using Pley.Tv.Core.Auth;
using Pley.Tv.Core.Net;
using Pley.Tv.Core.Player;

namespace Pley.Tv.Core;

/// <summary>Subtitle backgrounds.</summary>
public enum SubtitleBackground
{
    None,
    DarkBox,
}

/// <summary>Subtitle typefaces: the system's sans serif (the default), serif and monospace, and its casual hand where it has one.</summary>
public enum SubtitleFont
{
    Sans,
    Serif,
    Monospace,
    Casual,
}

/// <summary>Sending a sound format to the receiver as it is, or decoding it here.</summary>
public enum PassthroughMode
{
    Auto,
    On,
    Off,
}

/// <summary>How big the cards of a grid are: more small ones, or fewer large ones.</summary>
public enum GridDensity
{
    Small,
    Medium,
    Large,
}

/// <summary>Matching the TV's display mode to the video.</summary>
public enum FrameRateMatching
{
    /// <summary>For videos of 10 minutes or more, following the TV's own setting.</summary>
    Auto,
    Always,
    Off,
}

/// <summary>
/// What a person sets on this TV (Settings), not the profile's: those live on the server. The
/// Android app's settings, as a TV has them; the defaults are what the app does without being asked.
/// </summary>
public sealed record Settings
{
    /// <summary>Automatic: the original, lower only when it doesn't play well (docs/QUALITY.md).</summary>
    public const string Automatic = "auto";

    /// <summary>Subtitle sizes offered: 3.8, 4.8, 6 and 7.6 % of the video's height.</summary>
    public static readonly IReadOnlyList<float> SubtitleSizes = new[] { 0.038f, 0.048f, 0.06f, 0.076f };

    /// <summary>Subtitle positions offered: at the bottom, then raised by 5, 10 and 15 % of the video's height.</summary>
    public static readonly IReadOnlyList<float> SubtitleRaises = new[] { 0f, 0.05f, 0.1f, 0.15f };

    /// <summary>
    /// The sound formats with a passthrough choice: Dolby Digital, Dolby Digital Plus, Atmos (in
    /// Dolby Digital Plus), TrueHD, DTS and DTS-HD.
    /// </summary>
    public static readonly IReadOnlyList<string> PassthroughFormats = new[] { "ac3", "eac3", "eac3-joc", "truehd", "dts", "dts-hd" };

    /// <summary>The quality last picked ("auto", "Original" or a preset's id); Automatic until one is.</summary>
    public string Quality { get; init; } = Automatic;
    public FrameRateMatching FrameRateMatching { get; init; } = FrameRateMatching.Auto;
    /// <summary>Per format of <see cref="PassthroughFormats"/>; a format left out is Auto.</summary>
    public IReadOnlyDictionary<string, PassthroughMode> Passthrough { get; init; } = new Dictionary<string, PassthroughMode>();
    /// <summary>Under Advanced.</summary>
    public bool Tunneling { get; init; }
    /// <summary>A share of the video's height: one of <see cref="SubtitleSizes"/>.</summary>
    public float SubtitleSize { get; init; } = 0.048f;
    public SubtitleBackground SubtitleBackground { get; init; } = SubtitleBackground.None;
    public SubtitleFont SubtitleFont { get; init; } = SubtitleFont.Sans;
    /// <summary>How far text subtitles sit above the bottom, a share of the video's height: one of <see cref="SubtitleRaises"/>.</summary>
    public float SubtitleRaise { get; init; }
    /// <summary>Up Next plays after its countdown.</summary>
    public bool AutoplayNext { get; init; } = true;
    /// <summary>The compact readout over the video while the controls are hidden.</summary>
    public bool PlaybackDetails { get; init; }
    public GridDensity GridDensity { get; init; } = GridDensity.Medium;
    /// <summary>Move less, whatever the TV says.</summary>
    public bool ReduceMotion { get; init; }

    /// <summary>The passthrough choice for <paramref name="format"/> ("eac3"…).</summary>
    public PassthroughMode PassthroughOf(string format) => Passthrough.TryGetValue(format, out var mode) ? mode : PassthroughMode.Auto;

    /// <summary>These settings as they may be: a size or a raise not offered is the nearest one offered, a format not offered goes.</summary>
    public Settings Allowed() => this with
    {
        Quality = string.IsNullOrWhiteSpace(Quality) ? Automatic : Quality,
        Passthrough = (Passthrough ?? new Dictionary<string, PassthroughMode>())
            .Where(p => PassthroughFormats.Contains(p.Key) && p.Value != PassthroughMode.Auto)
            .ToDictionary(p => p.Key, p => p.Value),
        SubtitleSize = SubtitleSizes.OrderBy(s => Math.Abs(s - SubtitleSize)).First(),
        SubtitleRaise = SubtitleRaises.OrderBy(s => Math.Abs(s - SubtitleRaise)).First(),
    };
}

/// <summary>A server this TV signed in to or chose, newest first on the server list.</summary>
/// <param name="Id">The server's own id, when it was found by broadcast: the same server at a new address.</param>
public sealed record SavedServer(string Url, string Name, long LastUsed, string? Id = null);

/// <summary>The first screen, from what the TV kept (no network): the Android app's StartGate.</summary>
public enum StartPage
{
    Servers,
    Link,
    Profiles,
    /// <summary>"Who's watching?", opened on the PIN pad of the profile the TV was left on.</summary>
    Pin,
    Home,
}

/// <summary>What the TV app gives the shared state: its client id, its name, and where it keeps things.</summary>
public sealed record AppPlatform(string Name, string ClientId, IKeyValue Storage)
{
    /// <summary>The app's version, for its User-Agent.</summary>
    public string Version { get; init; } = "?";
    /// <summary>The TV's Tizen version ("9.0"), for the User-Agent: the server shows it on the app's row.</summary>
    public string? OsVersion { get; init; }
    /// <summary>The TV's model, for the User-Agent: two TVs of a household tell apart.</summary>
    public string? Model { get; init; }
    /// <summary>Seals the tokens where they are kept; without one they are kept as they are.</summary>
    public ITokenVault? TokenVault { get; init; }
    /// <summary>Makes the HTTP handlers; Core's own (<see cref="Transports.DefaultHandler"/>) without.</summary>
    public HandlerFactory? Handlers { get; init; }

    /// <summary>How every request introduces the app.</summary>
    public string UserAgent => UserAgents.Of(Version, OsVersion, Model);
}

/// <summary>
/// The app's state: the server signed in to, its API and tokens, who is watching, the
/// libraries, the settings, Home as last seen (drawn first at the next launch), and the
/// server's events. It lives on the UI thread, where its awaits come back to.
/// </summary>
public sealed class AppState
{
    const string ServerKey = "pley.server";
    const string SettingsKey = "pley.settings";
    /// <summary>"Always start as": one profile for the whole TV, as on Android.</summary>
    const string AlwaysStartAsKey = "pley.alwaysStartAs";
    /// <summary>How many profiles the server's household has, as last heard; kept per server.</summary>
    const string HouseholdKey = "pley.household";
    const string ServersKey = "pley.servers";
    /// <summary>The household's profiles, as last heard; kept per server.</summary>
    const string ProfilesKey = "pley.householdProfiles";
    /// <summary>The profile last used on a server, and the last few used there, newest first.</summary>
    const string ProfileKey = "pley.profile";
    const string RecentProfilesKey = "pley.recentProfiles";
    /// <summary>Home, who is watching and their libraries, as last heard, per server and profile: drawn before the server answers.</summary>
    const string HomeKey = "pley.home";
    const string MeKey = "pley.me";
    const string LibrariesKey = "pley.libraries";
    /// <summary>Profiles per server whose Home and libraries stay saved.</summary>
    const int ProfilesSaved = 3;
    /// <summary>Item details kept for pages and playbacks; the least recently used go first.</summary>
    const int ItemsKept = 50;
    /// <summary>A detail older than this is shown while it is asked for again; reconnecting asks again for what is older.</summary>
    public const long RevalidateMs = 30_000;
    /// <summary>Watch state changes come in bursts (an episode, its season and show; a play reaching 90 %): gathered this long.</summary>
    const long StateGatherMs = 300;
    /// <summary>A library's changes come in bursts during a scan: gathered this long.</summary>
    const long LibraryGatherMs = 1_500;
    /// <summary>How long "Pley is offline" shows: as long as an error's toast.</summary>
    const long OfflineNoticeMs = 6_000;

    static readonly JsonSerializerOptions Camel = new JsonSerializerOptions { PropertyNamingPolicy = JsonNamingPolicy.CamelCase };
    static readonly JsonSerializerOptions SettingsJson = new JsonSerializerOptions
    {
        PropertyNamingPolicy = JsonNamingPolicy.CamelCase,
        Converters = { new JsonStringEnumConverter() },
    };

    readonly AppPlatform platform;
    readonly Transport http;
    readonly EventConnect connect;
    readonly IScheduler scheduler;
    readonly IKeyValue store;
    readonly List<Action> leavers = new List<Action>();
    readonly List<Action<ServerEvent>> eventListeners = new List<Action<ServerEvent>>();
    readonly Dictionary<long, (ItemDetail Detail, long At)> items = new Dictionary<long, (ItemDetail, long)>();
    readonly LinkedList<long> itemOrder = new LinkedList<long>();
    readonly HashSet<long> stateIds = new HashSet<long>();
    readonly HashSet<long> stateParents = new HashSet<long>();
    readonly Dictionary<long, IDisposable> libraryTimers = new Dictionary<long, IDisposable>();
    Tokens? tokens;
    EventStream? events;
    IDisposable? stateTimer, noticeTimer;
    /// <summary>The profile signed in on the server in use, when known: whose saved Home and libraries show.</summary>
    long? profile;
    /// <summary>
    /// <see cref="Home"/> as JSON, as saved: the server's answer the same as Home shows (coming
    /// back to Home, or after a launch from what was saved) is neither drawn nor saved again.
    /// </summary>
    string? homeJson;
    /// <summary>Playbacks on top: Home and the pages wait to ask again until they end (their own reports change the watch state too).</summary>
    int holds;
    /// <summary>The libraries' order changes on their way to the server, one at a time.</summary>
    Task orderQueue = Task.CompletedTask;
    /// <summary>When the events last brought everything up to date (<see cref="IScheduler.Elapsed"/>).</summary>
    long? lastSync;

    /// <param name="http">Sends the app's requests; Core's transport over <see cref="AppPlatform.Handlers"/> without.</param>
    public AppState(AppPlatform platform, Transport? http = null, EventConnect? connect = null, IScheduler? scheduler = null)
    {
        this.platform = platform;
        this.http = Transports.WithUserAgent(http ?? Transports.Create(platform.Handlers), platform.UserAgent);
        this.connect = connect ?? EventStream.Over(platform.Handlers);
        this.scheduler = scheduler ?? new ContextScheduler();
        store = platform.Storage;
        Settings = new Signal<Settings>(Read<Settings>(SettingsKey, SettingsJson)?.Allowed() ?? new Settings());
        var server = store.Get(ServerKey);
        if (!string.IsNullOrEmpty(server)) Use(server!);
    }

    public Signal<string?> Server { get; } = new Signal<string?>(null);

    /// <summary>Where this TV keeps things, for what keeps its own (the player's subtitle offsets).</summary>
    public IKeyValue Storage => store;
    public Signal<Api.Api?> Api { get; } = new Signal<Api.Api?>(null);
    public Signal<User?> Me { get; } = new Signal<User?>(null);
    public Signal<IReadOnlyList<Library>?> Libraries { get; } = new Signal<IReadOnlyList<Library>?>(null);
    public Signal<IReadOnlyList<HouseholdProfile>?> Profiles { get; } = new Signal<IReadOnlyList<HouseholdProfile>?>(null);
    public Signal<Settings> Settings { get; }
    public Signal<IReadOnlyList<Hub>?> Home { get; } = new Signal<IReadOnlyList<Hub>?>(null);
    public Signal<Exception?> HomeError { get; } = new Signal<Exception?>(null);
    /// <summary>The last try at who is watching and the libraries failed (and none were saved): the library pages offer Retry.</summary>
    public Signal<Exception?> AccountError { get; } = new Signal<Exception?>(null);
    /// <summary>Bumped by the server's events, so screens that list things fetch again.</summary>
    public Signal<int> Changes { get; } = new Signal<int>(0);
    /// <summary>
    /// Bumped when this app changed a playlist (made, renamed, deleted, added to, reordered): the
    /// playlists' pages fetch again. The server sends no event for them.
    /// </summary>
    public Signal<int> PlaylistChanges { get; } = new Signal<int>(0);
    /// <summary>The libraries being scanned now, from the server's events and the libraries' <c>scanning</c>.</summary>
    public Signal<IReadOnlyList<ScanState>> Scans { get; } = new Signal<IReadOnlyList<ScanState>>(new ScanState[0]);
    /// <summary>The server signed the TV out: show sign-in.</summary>
    public Signal<bool> SignedOut { get; } = new Signal<bool>(false);
    /// <summary>Whether the TV shows the app (not another app, its home screen or standby).</summary>
    public Signal<bool> Visible { get; } = new Signal<bool>(true);
    /// <summary>The server's events are connected: what changes comes as it happens, and nothing need be polled.</summary>
    public Signal<bool> Streaming { get; } = new Signal<bool>(false);
    /// <summary>The server's events connected (each time): nothing is replayed, so what they would have said is asked again.</summary>
    public event Action? Connected;
    /// <summary>
    /// The server answered a screen nothing (the Android app's TvOfflineState): what shows is what
    /// was saved. Raised by a load the network failed, lowered by the server's events connecting
    /// or a screen's fresh answer after a failure. A refusal (a 502 among them) is an answer.
    /// </summary>
    public Signal<bool> Offline { get; } = new Signal<bool>(false);
    /// <summary>"Pley is offline" shows, for 6 s from going offline; then the rail's avatar keeps a mark.</summary>
    public Signal<bool> OfflineNotice { get; } = new Signal<bool>(false);
    /// <summary>
    /// "Who's watching?" was answered in this launch (a profile picked, a PIN checked, or a
    /// sign-in approved on a phone): the current profile's PIN isn't asked again until the app
    /// starts over.
    /// </summary>
    public bool ProfileGatePassed { get; set; }

    /// <summary>The app's transport: every request introduces the app (<see cref="AppPlatform.UserAgent"/>).</summary>
    public Transport Http => http;
    public AppPlatform Platform => platform;
    public Tokens? TokenStore => tokens;
    public bool SignedIn => tokens?.SignedIn == true;

    T? Read<T>(string key, JsonSerializerOptions? options = null) where T : class => Parse<T>(store.Get(key), options);

    static T? Parse<T>(string? text, JsonSerializerOptions? options = null) where T : class
    {
        try
        {
            return string.IsNullOrEmpty(text) ? null : JsonSerializer.Deserialize<T>(text!, options ?? ApiJson.Options);
        }
        catch (JsonException)
        {
            return null;
        }
    }

    void Write<T>(string key, T value, JsonSerializerOptions? options = null) => store.Set(key, JsonSerializer.Serialize(value, options ?? ApiJson.Options));

    /// <summary>The first screen, from what the TV kept (the tokens aren't opened for it).</summary>
    public StartPage Start() => StartOf(Server.Value != null, tokens?.Kept == true, HouseholdSize(), AlwaysStartAs(), Me.Value);

    /// <summary>
    /// The first screen (the Android app's startKey): no server, Servers; no sign-in, Link; otherwise
    /// Home as the profile the TV was left on (<paramref name="me"/>, as last heard), but never past its
    /// PIN. In a household of several profiles, one with a PIN asks first: "Who's watching?", or its pad
    /// with "Always start as"; and so does one the TV doesn't know has none.
    /// </summary>
    public static StartPage StartOf(bool server, bool signedIn, int household, long? alwaysStartAs, User? me)
    {
        if (!server) return StartPage.Servers;
        if (!signedIn) return StartPage.Link;
        if (household <= 1) return StartPage.Home;
        if (me is { HasPin: false }) return StartPage.Home;
        if (alwaysStartAs == null || me == null) return StartPage.Profiles;
        return StartPage.Pin;
    }

    Tokens NewTokens(string issuer, Action<SignOutReason>? onSignedOut = null) =>
        new Tokens(issuer, platform.ClientId, store, http, onSignedOut, () => scheduler.Now, platform.TokenVault);

    /// <summary>The tokens of <paramref name="issuer"/>: the server in use's own, so a sign-in kept there is the one it uses.</summary>
    public Tokens TokensFor(string issuer) => issuer == Server.Value && tokens != null ? tokens : NewTokens(issuer);

    /// <summary>Signing in with a code, its tokens kept for the server they come from.</summary>
    public DeviceSignIn DeviceSignIn() => new DeviceSignIn(http, platform.ClientId, TokensFor, () => scheduler.Now);

    /// <summary>Signs in to <paramref name="server"/> from now on (its tokens, if the TV has them), showing what was saved from it.</summary>
    public void Use(string server)
    {
        var home = Addresses.Origin(server);
        if (home == null) return;
        if (Server.Value == home && Api.Value != null)
        {
            // Signed in again to the same server: the next sign-out has to be a change, to be heard.
            SignedOut.Value = false;
            return;
        }
        events?.Stop();
        events = null;
        if (Server.Value != null && Server.Value != home) LeaveServer();
        Server.Value = home;
        store.Set(ServerKey, home);
        tokens = NewTokens(home, _ => scheduler.Post(() =>
        {
            // Told by whichever request found out, on whichever thread: the screens hear it here.
            SignedOut.Value = true;
            events?.Stop();
        }));
        Api.Value = new Api.Api(home, tokens, http);
        SignedOut.Value = false;
        profile = long.TryParse(store.Get($"{ProfileKey}.{home}"), out var id) ? id : null;
        ShowSaved();
    }

    /// <summary>The servers this TV chose, most recently used first.</summary>
    public IReadOnlyList<SavedServer> SavedServers() =>
        (Read<List<SavedServer>>(ServersKey, Camel) ?? new List<SavedServer>())
            .Where(s => s != null && !string.IsNullOrEmpty(s.Url)).OrderByDescending(s => s.LastUsed).ToList();

    /// <summary>
    /// Keeps <paramref name="url"/> on the server list, as used now; it replaces a row with the
    /// same address or the same <paramref name="id"/> (a server found at a new address).
    /// </summary>
    public void RememberServer(string url, string name, string? id = null)
    {
        var home = Addresses.Origin(url);
        if (home == null) return;
        var list = SavedServers().Where(s => s.Url != home && (id == null || s.Id != id)).ToList();
        list.Insert(0, new SavedServer(home, name, scheduler.Now, id));
        Write(ServersKey, list, Camel);
    }

    /// <summary>Whether the TV has a sign-in for <paramref name="url"/> (not opened: it may still turn out unreadable).</summary>
    public bool HasTokens(string url)
    {
        var home = Addresses.Origin(url);
        return home != null && NewTokens(home).Kept;
    }

    /// <summary>
    /// This TV's own id for its playbacks ("tizen-1f3a…"), kept for good: the server ends a
    /// client's earlier sessions when it starts another, so two TVs of a household must never share one.
    /// </summary>
    public string InstallId()
    {
        var id = store.Get("pley.installId");
        if (string.IsNullOrEmpty(id))
        {
            var bytes = new byte[16];
            using (var random = RandomNumberGenerator.Create()) random.GetBytes(bytes);
            id = $"{platform.Name}-{string.Concat(bytes.Select(b => b.ToString("x2")))}";
            store.Set("pley.installId", id);
        }
        return id!;
    }

    /// <summary>The profile "Always start as" names, for the whole TV; null asks who's watching.</summary>
    public long? AlwaysStartAs() => long.TryParse(store.Get(AlwaysStartAsKey), out var id) ? id : null;

    public void SetAlwaysStartAs(long? id)
    {
        if (id == null) store.Remove(AlwaysStartAsKey);
        else store.Set(AlwaysStartAsKey, id.Value.ToString(System.Globalization.CultureInfo.InvariantCulture));
    }

    /// <summary>How many profiles the household had when last asked (1 while unknown).</summary>
    public int HouseholdSize() => int.TryParse(store.Get($"{HouseholdKey}.{Server.Value ?? ""}"), out var n) && n > 0 ? n : 1;

    /// <summary>The household as last fetched, for "Who's watching?" before the server answers.</summary>
    public IReadOnlyList<HouseholdProfile> SavedHousehold() => Read<List<HouseholdProfile>>($"{ProfilesKey}.{Server.Value ?? ""}") ?? new List<HouseholdProfile>();

    public void KeepHousehold(IReadOnlyList<HouseholdProfile> profiles)
    {
        Profiles.Value = profiles;
        var server = Server.Value ?? "";
        store.Set($"{HouseholdKey}.{server}", profiles.Count.ToString(System.Globalization.CultureInfo.InvariantCulture));
        Write($"{ProfilesKey}.{server}", profiles);
    }

    /// <summary><paramref name="fn"/> runs when what plays must end: a profile switch, signing out, another server, the app closing.</summary>
    public void OnLeave(Action fn) => leavers.Add(fn);

    /// <summary>Ends what plays in the background before the profile, the sign-in, the server or the app goes.</summary>
    public void EndPlayback()
    {
        foreach (var fn in leavers.ToList()) fn();
    }

    /// <summary>
    /// The app acts as another profile now, <paramref name="to"/> when the screen knows which:
    /// what was saved for it shows at once, and the server is asked for the rest.
    /// </summary>
    public async Task Switched(long? to = null)
    {
        EndPlayback();
        HomeError.Value = null;
        AccountError.Value = null;
        ForgetItems();
        profile = to;
        if (to != null && Server.Value != null) KeepProfile(Server.Value, to.Value);
        // The events start over with the new profile's token, so none of the last one's come.
        if (events?.Running == true)
        {
            events.Stop();
            events.Start();
        }
        ShowSaved();
        await LoadAccount();
        _ = LoadHome();
    }

    // ---- what was saved, per server and profile

    string SavedKey(string key, string server, long id) => $"{key}.{server}.{id.ToString(System.Globalization.CultureInfo.InvariantCulture)}";

    /// <summary>Home, who is watching and their libraries as last heard from the profile in use: none when it isn't known.</summary>
    void ShowSaved()
    {
        var server = Server.Value;
        if (server == null || profile == null)
        {
            Home.Value = null;
            Me.Value = null;
            Libraries.Value = null;
            return;
        }
        var id = profile.Value;
        // What was saved is trusted only in shape: a store another build wrote, or a damaged one, is left out.
        homeJson = store.Get(SavedKey(HomeKey, server, id));
        Home.Value = Parse<List<Hub>>(homeJson);
        Me.Value = Read<User>(SavedKey(MeKey, server, id)) is { Id: > 0 } me ? me : null;
        Libraries.Value = Read<List<Library>>(SavedKey(LibrariesKey, server, id));
    }

    /// <summary>Keeps <paramref name="value"/> for the profile in use, when it is known.</summary>
    void Save<T>(string key, T value)
    {
        if (Server.Value != null && profile != null) Write(SavedKey(key, Server.Value, profile.Value), value);
    }

    /// <summary>Keeps <paramref name="json"/>, already written, for the profile in use, when it is known.</summary>
    void SaveJson(string key, string json)
    {
        if (Server.Value != null && profile != null) store.Set(SavedKey(key, Server.Value, profile.Value), json);
    }

    IReadOnlyList<long> RecentProfiles(string server) => Read<List<long>>($"{RecentProfilesKey}.{server}", Camel) ?? new List<long>();

    /// <summary><paramref name="id"/> is the profile in use on <paramref name="server"/>: the last three keep what was saved for them.</summary>
    void KeepProfile(string server, long id)
    {
        store.Set($"{ProfileKey}.{server}", id.ToString(System.Globalization.CultureInfo.InvariantCulture));
        var recent = RecentProfiles(server).Where(p => p != id).ToList();
        recent.Insert(0, id);
        foreach (var old in recent.Skip(ProfilesSaved)) ForgetSaved(server, old);
        Write($"{RecentProfilesKey}.{server}", recent.Take(ProfilesSaved).ToList(), Camel);
    }

    void ForgetSaved(string server, long id)
    {
        store.Remove(SavedKey(HomeKey, server, id));
        store.Remove(SavedKey(MeKey, server, id));
        store.Remove(SavedKey(LibrariesKey, server, id));
        foreach (var key in Requests.AppRequests.KeptKeys) store.Remove(SavedKey(key, server, id));
    }

    /// <summary>Everything saved from <paramref name="server"/>: signed out of it, or it was forgotten.</summary>
    void ForgetSaved(string server)
    {
        var ids = RecentProfiles(server).ToList();
        if (long.TryParse(store.Get($"{ProfileKey}.{server}"), out var current)) ids.Add(current);
        foreach (var id in ids.Distinct()) ForgetSaved(server, id);
        store.Remove($"{RecentProfilesKey}.{server}");
        store.Remove($"{ProfileKey}.{server}");
    }

    /// <summary>Forgets a server of the list: its sign-in ends, and what was saved from it goes.</summary>
    public async Task Forget(string url)
    {
        var home = Addresses.Origin(url);
        if (home == null) return;
        Write(ServersKey, SavedServers().Where(s => s.Url != home).ToList(), Camel);
        var those = home == Server.Value && tokens != null ? tokens : NewTokens(home);
        if (home == Server.Value) ForgetServer();
        ForgetSaved(home);
        store.Remove($"{HouseholdKey}.{home}");
        store.Remove($"{ProfilesKey}.{home}");
        try
        {
            await those.SignOut();
        }
        catch (Exception)
        {
            // Forgotten here whatever the server says.
        }
    }

    /// <summary>Leaves the server in use (Change server): what was saved from it stays for when it is chosen again.</summary>
    public void ForgetServer()
    {
        EndPlayback();
        events?.Stop();
        events = null;
        store.Remove(ServerKey);
        LeaveServer();
        Server.Value = null;
        Api.Value = null;
        tokens = null;
        profile = null;
    }

    /// <summary>What was shown from the server in use goes: another one comes, or none.</summary>
    void LeaveServer()
    {
        EndPlayback();
        ForgetItems();
        foreach (var t in libraryTimers.Values) t.Dispose();
        libraryTimers.Clear();
        Home.Value = null;
        HomeError.Value = null;
        AccountError.Value = null;
        Me.Value = null;
        Libraries.Value = null;
        Profiles.Value = null;
        Scans.Value = new ScanState[0];
        Offline.Value = false;
        OfflineNotice.Value = false;
        ProfileGatePassed = false;
        lastSync = null;
    }

    public async Task SignOutNow()
    {
        events?.Stop();
        // What plays ends while the sign-in that made it still works.
        EndPlayback();
        if (tokens != null) await tokens.SignOut();
        if (Server.Value != null) ForgetSaved(Server.Value);
        profile = null;
        Home.Value = null;
        Me.Value = null;
        Libraries.Value = null;
        ForgetItems();
        SignedOut.Value = true;
    }

    public void UpdateSettings(Func<Settings, Settings> change)
    {
        Settings.Value = change(Settings.Value).Allowed();
        Write(SettingsKey, Settings.Value, SettingsJson);
    }

    // ---- what the server has

    /// <summary>
    /// Who is watching, the libraries and the household, after sign-in or a profile switch; kept,
    /// so the rail and the library pages have them before the server answers next time. A failure
    /// is kept for the library pages' Retry, and the server out of reach shows as such.
    /// </summary>
    public async Task LoadAccount()
    {
        var api = Api.Value;
        if (api == null) return;
        User me;
        List<Library> libraries;
        try
        {
            var meTask = api.Me();
            var librariesTask = api.Libraries();
            me = await meTask;
            libraries = await librariesTask;
        }
        catch (Exception e)
        {
            if (Api.Value == api)
            {
                AccountError.Value = e;
                if (Errors.Unreachable(e)) ServerAway();
            }
            throw;
        }
        // Another server or sign-in since: the answer was the one before's.
        if (Api.Value != api) return;
        var failedBefore = AccountError.Value != null;
        AccountError.Value = null;
        if (profile != me.Id && Server.Value != null)
        {
            // The server acts as another profile than the one saved (switched on another device, or a first sign-in).
            var first = profile == null;
            profile = me.Id;
            KeepProfile(Server.Value, me.Id);
            if (first && Home.Value != null) Save(HomeKey, Home.Value);
        }
        Me.Value = me;
        Save(MeKey, me);
        KeepLibraries(libraries);
        if (failedBefore) ServerAnswered();
        _ = KeepProfiles(api);
    }

    async Task KeepProfiles(Api.Api api)
    {
        try
        {
            KeepHousehold(await api.Profiles());
        }
        catch (Exception)
        {
            // Asked again next time.
        }
    }

    void KeepLibraries(IReadOnlyList<Library> libraries)
    {
        Libraries.Value = libraries;
        Save(LibrariesKey, libraries);
        HeardLibraries(libraries);
    }

    async Task LoadLibrariesQuietly()
    {
        var api = Api.Value;
        if (api == null) return;
        try
        {
            var libraries = await api.Libraries();
            if (Api.Value == api) KeepLibraries(libraries);
        }
        catch (Exception)
        {
            // Asked again with the next change, or the next time the pages ask.
        }
    }

    /// <summary>
    /// The profile's own order of its libraries (<paramref name="ids"/>, each once), or the order
    /// everyone sees again (null), on every list of them on all its devices. Changes go one at a
    /// time, in the order they were made; the server's answer is the list from then on, and Home
    /// asks again, since its rows of each library follow the order.
    /// </summary>
    public Task<List<Library>> OrderLibraries(IReadOnlyList<long>? ids)
    {
        async Task<List<Library>> Run(Task before)
        {
            try
            {
                await before;
            }
            catch (Exception)
            {
                // The change before failed: this one goes anyway.
            }
            var api = Api.Value ?? throw new SignedOutException();
            var who = Me.Value?.Id;
            var libraries = ids != null ? await api.SortLibraries(ids) : await api.ForgetLibraryOrder();
            // Another profile, sign-in or server since: the answer was the one before's.
            if (Api.Value == api && Me.Value?.Id == who)
            {
                KeepLibraries(libraries);
                _ = LoadHome();
            }
            return libraries;
        }
        var run = Run(orderQueue);
        orderQueue = run;
        return run;
    }

    public async Task LoadHome()
    {
        var api = Api.Value;
        if (api == null) return;
        var who = profile;
        try
        {
            var hubs = (await api.Home()).Hubs;
            // Another server or profile since: the answer was the one before's.
            if (Api.Value != api || profile != who) return;
            var failedBefore = HomeError.Value != null;
            // Lists aren't equal by their rows: the same answer, as JSON, leaves Home as it is
            // rather than making all its rows again (a third of a second on the slower TVs).
            var json = JsonSerializer.Serialize(hubs, ApiJson.Options);
            if (Home.Value == null || json != homeJson)
            {
                homeJson = json;
                Home.Value = hubs;
                SaveJson(HomeKey, json);
            }
            HomeError.Value = null;
            if (failedBefore) ServerAnswered();
        }
        catch (Exception e)
        {
            if (Api.Value != api || profile != who) return;
            HomeError.Value = e;
            if (Errors.Unreachable(e)) ServerAway();
        }
    }

    // ---- the server out of reach

    /// <summary>
    /// A screen's load got no answer: the notice shows for 6 s (again each time the server goes),
    /// then the rail's mark. The event stream, reconnecting by itself, brings the screens back
    /// once the server answers; so does a screen's own fresh answer.
    /// </summary>
    public void ServerAway()
    {
        if (Offline.Value) return;
        Offline.Value = true;
        OfflineNotice.Value = true;
        noticeTimer?.Dispose();
        noticeTimer = scheduler.After(OfflineNoticeMs, () =>
        {
            noticeTimer = null;
            OfflineNotice.Value = false;
        });
    }

    /// <summary>The server answers again: its events connected, or a screen that had failed got a fresh answer.</summary>
    public void ServerAnswered()
    {
        if (!Offline.Value) return;
        noticeTimer?.Dispose();
        noticeTimer = null;
        Offline.Value = false;
        OfflineNotice.Value = false;
    }

    // ---- item details

    /// <summary>
    /// The detail of item <paramref name="id"/> the app saw lately, to start a page or a playback
    /// without waiting; null when none is kept. The server's events take out what changed; one
    /// older than <see cref="RevalidateMs"/> (<see cref="CachedItemAge"/>) is shown while the page
    /// asks for it again.
    /// </summary>
    public ItemDetail? CachedItem(long id)
    {
        if (!items.TryGetValue(id, out var kept)) return null;
        itemOrder.Remove(id);
        itemOrder.AddLast(id);
        return kept.Detail;
    }

    /// <summary>How long ago the kept detail of item <paramref name="id"/> came from the server, in ms; null when none is kept.</summary>
    public long? CachedItemAge(long id) => items.TryGetValue(id, out var kept) ? scheduler.Elapsed - kept.At : null;

    /// <summary>Whether the kept detail of item <paramref name="id"/> should be asked for again: none, or older than <see cref="RevalidateMs"/>.</summary>
    public bool CachedItemStale(long id) => !(CachedItemAge(id) < RevalidateMs);

    public async Task<ItemDetail> Item(long id)
    {
        var detail = await Api.Value!.Item(id);
        KeepItem(detail);
        return detail;
    }

    /// <summary>Keeps a detail fetched elsewhere (an item page) for the next page or playback.</summary>
    public void KeepItem(ItemDetail detail)
    {
        itemOrder.Remove(detail.Id);
        itemOrder.AddLast(detail.Id);
        items[detail.Id] = (detail, scheduler.Elapsed);
        while (items.Count > ItemsKept)
        {
            items.Remove(itemOrder.First!.Value);
            itemOrder.RemoveFirst();
        }
    }

    void ForgetItems()
    {
        items.Clear();
        itemOrder.Clear();
    }

    /// <summary>The kept details that <paramref name="match"/> go.</summary>
    void ForgetItems(Func<ItemDetail, long, bool> match)
    {
        foreach (var id in items.Keys.ToList())
        {
            var (detail, at) = items[id];
            if (!match(detail, at)) continue;
            items.Remove(id);
            itemOrder.Remove(id);
        }
    }

    /// <summary>
    /// Tracks were picked for <paramref name="detail"/>'s title (the whole show for an episode),
    /// or the profile's defaults changed (null): the details kept name the tracks they started
    /// with before. <paramref name="detail"/>, with the choice in it, is kept; the others go.
    /// </summary>
    public void TracksChanged(ItemDetail? detail)
    {
        ForgetItems();
        if (detail != null) KeepItem(detail);
    }

    /// <summary>
    /// A playback is on top (true) or has ended (false). Meanwhile Home and the pages wait to ask
    /// again; once it ends they do, and the details kept go, since its own reports moved resume
    /// points and watched marks.
    /// </summary>
    public void HoldRefresh(bool on)
    {
        holds = Math.Max(0, holds + (on ? 1 : -1));
        if (on || holds > 0) return;
        ForgetItems();
        _ = LoadHome();
        Changes.Value++;
    }

    /// <summary>Home and the pages that list things ask again; while a playback is on top, once it ends.</summary>
    void Refresh()
    {
        if (holds > 0) return;
        _ = LoadHome();
        // Who is watching and the libraries, when the last try failed or none came yet.
        if (AccountError.Value != null || Libraries.Value == null) _ = LoadAccountQuietly();
        Changes.Value++;
    }

    async Task LoadAccountQuietly()
    {
        try
        {
            await LoadAccount();
        }
        catch (Exception)
        {
            // Kept in AccountError for the library pages' Retry.
        }
    }

    // ---- scans

    /// <summary>The scans now <paramref name="next"/>; one that ends has changed what Home shows, so Home asks once more.</summary>
    void UpdateScans(IReadOnlyList<ScanState> next)
    {
        var was = Scans.Value.Count > 0;
        Scans.Value = next;
        if (was && next.Count == 0 && holds == 0) _ = LoadHome();
    }

    void Scan(long libraryId, float? progress) =>
        UpdateScans(Scans.Value.Where(s => s.LibraryId != libraryId).Append(new ScanState(libraryId, progress)).ToList());

    /// <summary>A fresh list of libraries: which are scanning now.</summary>
    void HeardLibraries(IReadOnlyList<Library> libraries)
    {
        var scanning = new HashSet<long>(libraries.Where(l => l.Scanning).Select(l => l.Id));
        var kept = Scans.Value.Where(s => scanning.Contains(s.LibraryId)).ToList();
        var added = scanning.Where(id => kept.All(s => s.LibraryId != id)).Select(id => new ScanState(id, null));
        var next = kept.Concat(added).ToList();
        if (next.Count != Scans.Value.Count || Scans.Value.Except(next).Any()) UpdateScans(next);
    }

    // ---- the server's events

    public IDisposable OnEvent(Action<ServerEvent> listener)
    {
        eventListeners.Add(listener);
        return new Unlisten(() => eventListeners.Remove(listener));
    }

    sealed class Unlisten : IDisposable
    {
        readonly Action undo;

        public Unlisten(Action undo) => this.undo = undo;

        public void Dispose() => undo();
    }

    /// <summary>
    /// Follows the server's events while the app is shown and signed in; a stream the server refused
    /// starts again here. The stream makes this TV a receiver of the profile's remotes' casts
    /// (<see cref="Casts.ReceiverQuery"/>).
    /// </summary>
    public void Listen(bool on)
    {
        var server = Server.Value;
        if (!on || server == null || tokens?.SignedIn != true)
        {
            events?.Stop();
            // Scans are heard from the events only: none while they aren't followed.
            Scans.Value = new ScanState[0];
            return;
        }
        events ??= new EventStream(server, tokens, HandleEvent, Reconnected, connect, scheduler, platform.UserAgent,
            onDrop: () => Streaming.Value = false,
            query: () =>
            {
                receivesLive = liveTv;
                return Casts.ReceiverQuery(InstallId(), receivesLive);
            });
        events.Start();
    }

    /// <summary>Whether Live TV shows for the profile, and whether the stream last opened told the remotes this TV takes channels.</summary>
    bool liveTv, receivesLive;

    /// <summary>Live TV shows for the profile now, or no longer does: the stream opens again to tell the remotes.</summary>
    public void TakesLive(bool live)
    {
        liveTv = live;
        if (events?.Running == true && live != receivesLive)
        {
            events.Stop();
            events.Start();
        }
    }

    void Reconnected()
    {
        Streaming.Value = true;
        Connected?.Invoke();
        // Nothing is replayed: what is older than 30 s, or anything after the server was out of reach, is asked again.
        var away = Offline.Value;
        ServerAnswered();
        var now = scheduler.Elapsed;
        if (away || lastSync == null || now - lastSync.Value >= RevalidateMs)
        {
            lastSync = now;
            ForgetItems((_, at) => away || now - at >= RevalidateMs);
            Refresh();
        }
    }

    void HandleEvent(ServerEvent e)
    {
        switch (e.Type)
        {
            case "scan_started":
                Scan(e.LibraryId!.Value, null);
                break;
            case "scan_progress":
                Scan(e.LibraryId!.Value, e.Total > 0 ? Math.Max(0f, Math.Min(1f, (float)e.Processed / e.Total)) : null);
                break;
            case "scan_finished":
                UpdateScans(Scans.Value.Where(s => s.LibraryId != e.LibraryId).ToList());
                break;
            case "library_changed":
            {
                // Changes come in bursts during scans: each library's once, 1.5 s after its last.
                var id = e.LibraryId!.Value;
                if (libraryTimers.TryGetValue(id, out var waiting)) waiting.Dispose();
                libraryTimers[id] = scheduler.After(LibraryGatherMs, () =>
                {
                    libraryTimers.Remove(id);
                    LibraryChanged(id);
                });
                break;
            }
            case "libraries_changed":
                // Which titles this profile may see may have changed with them: whatever shows them, and who the profile is.
                ForgetItems();
                _ = LoadAccountQuietly();
                if (holds == 0)
                {
                    _ = LoadHome();
                    Changes.Value++;
                }
                break;
            case "watchlist_changed":
                ForgetItems();
                Refresh();
                break;
            case "user_state_changed":
                // The item, its parent and grandparent; with `children`, everything under the first as well.
                foreach (var id in e.ItemIds ?? new long[0]) stateIds.Add(id);
                if (e.Children == true && e.ItemIds is { Count: > 0 }) stateParents.Add(e.ItemIds[0]);
                stateTimer ??= scheduler.After(StateGatherMs, ApplyStateChanges);
                break;
        }
        foreach (var l in eventListeners.ToList()) l(e);
    }

    /// <summary>A library's items changed: their details go, and its listings, the libraries and Home ask again.</summary>
    void LibraryChanged(long libraryId)
    {
        ForgetItems((d, _) => d.LibraryId == libraryId);
        if (holds > 0) return;
        _ = LoadHome();
        _ = LoadLibrariesQuietly();
        Changes.Value++;
    }

    /// <summary>
    /// The watch state changes of the last 0.3 s: their details go, and Home and the pages ask
    /// again once, with the library changes waiting (a play that ends sends both).
    /// </summary>
    void ApplyStateChanges()
    {
        stateTimer = null;
        bool Under(long? p) => p != null && stateParents.Contains(p.Value);
        ForgetItems((d, _) => stateIds.Contains(d.Id) || Under(d.ParentId) || Under(d.GrandparentId));
        stateIds.Clear();
        stateParents.Clear();
        var libraries = libraryTimers.Keys.ToList();
        foreach (var id in libraries)
        {
            libraryTimers[id].Dispose();
            ForgetItems((d, _) => d.LibraryId == id);
        }
        libraryTimers.Clear();
        Refresh();
        if (libraries.Count > 0 && holds == 0) _ = LoadLibrariesQuietly();
    }
}
