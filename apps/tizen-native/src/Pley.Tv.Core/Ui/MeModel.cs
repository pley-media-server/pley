using System;
using System.Linq;
using System.Text.Json;
using System.Threading.Tasks;
using Pley.Tv.Core.Api;
using Pley.Tv.Core.Auth;

namespace Pley.Tv.Core.Ui;

/// <summary>Who the app acts as, as the Me page shows it (GET /api/auth/me).</summary>
/// <param name="AvatarUrl">The picture's full address; null without one.</param>
/// <param name="AvatarColor">"#38bdf8", the circle behind the initial.</param>
/// <param name="Managed">A managed profile (no password, reached by switching from a home member).</param>
/// <param name="CanSwitch">The sign-in may switch profiles (the "profiles" scope).</param>
/// <param name="Server">The server's address, "http://192.168.1.20:7539".</param>
public sealed record MeProfile(long Id, string Name, string? AvatarUrl, string? AvatarColor, bool Managed, bool CanSwitch, string Server)
{
    /// <summary>The line under the name: "Managed profile · 192.168.1.20:7539"; an account needs no word for what it is.</summary>
    public string Line => string.Join(" · ", new[] { Managed ? SettingsText.RoleManaged : null, SettingsText.HostPort(Server) }.Where(s => s != null));
}

/// <summary>The profile's audio and subtitle defaults; the mode is "off", "forced", "foreign" or "always".</summary>
public sealed record TrackDefaults(string? AudioLanguage, string? SubtitleLanguage, string SubtitleMode);

/// <summary>The profile's track defaults and whether this app may change them.</summary>
/// <param name="Editable">False: shown read-only, to change on the web (an older server, or no "play" scope).</param>
public sealed record TrackDefaultsState(TrackDefaults Defaults, bool Editable, bool Saving = false)
{
    /// <summary>Subtitles that need a language to read: "When the audio is in another language" and "Always".</summary>
    public bool ReadsSubtitles => Defaults.SubtitleMode is "foreign" or "always";
}

public sealed record MeState(MeProfile? Profile = null, TrackDefaultsState? TrackDefaults = null, bool SigningOut = false);

/// <summary>Which track default a change was of.</summary>
public enum TrackField
{
    AudioLanguage,
    SubtitleMode,
    SubtitleLanguage,
}

/// <summary>What the Me page tells its screen once.</summary>
public abstract record MeEvent
{
    /// <summary>"Saved".</summary>
    public sealed record Saved(TrackField Field) : MeEvent;

    /// <summary>The server doesn't let the app change track defaults: they show read-only now.</summary>
    public sealed record ReadOnly : MeEvent;

    public sealed record Failed(Exception Error) : MeEvent;

    /// <summary>Signed out of <paramref name="Server"/>: back to signing in to it (its link page), or choosing one.</summary>
    public sealed record SignedOut(string? Server) : MeEvent;
}

/// <summary>What the Me page reads and changes: the app's state, or a fake in tests.</summary>
public interface IMeSource
{
    /// <summary>The server signed in to.</summary>
    string? Server { get; }

    /// <summary>Who is watching; null until known.</summary>
    Signal<User?> Me { get; }

    /// <summary>PUT /api/me/track-defaults: the profile as the server has it after.</summary>
    Task<User> SaveTrackDefaults(TrackDefaults defaults);

    /// <summary>The track defaults changed: item details kept name the tracks they started with before.</summary>
    void TracksChanged();

    /// <summary>Ends the sign-in on the server (what plays ends first, the music too) and forgets what was saved from it.</summary>
    Task SignOut();
}

/// <summary>
/// The Me page, Settings' Profile (the Android app's MeViewModel): who is signed in, the
/// profile's audio and subtitle defaults (PUT /api/me/track-defaults), and signing out, which
/// ends the sign-in on the server, deletes what was saved from it and stops the music.
/// Profiles are switched on "Who's watching?" (<see cref="ProfilesModel"/>).
/// </summary>
public sealed class MeModel : IDisposable
{
    readonly IMeSource source;
    readonly IScheduler scheduler;
    readonly IDisposable watch;
    bool readOnly, saving, signingOut, gone;

    public MeModel(IMeSource source, IScheduler scheduler)
    {
        this.source = source;
        this.scheduler = scheduler;
        watch = source.Me.Watch(_ => Compute());
    }

    public Signal<MeState> State { get; } = new Signal<MeState>(new MeState());

    public event Action<MeEvent>? Told;

    public void SetAudioLanguage(string? code) => Save(TrackField.AudioLanguage, d => d with { AudioLanguage = code });

    /// <summary>"off", "forced", "foreign" or "always".</summary>
    public void SetSubtitleMode(string mode) => Save(TrackField.SubtitleMode, d => d with { SubtitleMode = mode });

    public void SetSubtitleLanguage(string? code) => Save(TrackField.SubtitleLanguage, d => d with { SubtitleLanguage = code });

    /// <summary>Signs out of the server: the music stops, the sign-in ends there, what was saved from it goes.</summary>
    public void SignOut()
    {
        if (signingOut) return;
        signingOut = true;
        Compute();
        var server = source.Server;
        Plays.When(scheduler, source.SignOut, () =>
        {
            signingOut = false;
            Compute();
            Told?.Invoke(new MeEvent.SignedOut(server));
        }, e =>
        {
            signingOut = false;
            Compute();
            Told?.Invoke(new MeEvent.Failed(e));
        });
    }

    public void Dispose()
    {
        gone = true;
        watch.Dispose();
    }

    void Save(TrackField field, Func<TrackDefaults, TrackDefaults> change)
    {
        var current = State.Value.TrackDefaults;
        if (current == null || !current.Editable || saving) return;
        var next = change(current.Defaults);
        if (next == current.Defaults) return;
        var before = source.Me.Value;
        saving = true;
        // At once on screen; put back if the server refuses.
        if (before != null) source.Me.Value = before with { AudioLanguage = next.AudioLanguage, SubtitleLanguage = next.SubtitleLanguage, SubtitleMode = next.SubtitleMode };
        Compute();
        Plays.When(scheduler, () => source.SaveTrackDefaults(next), me =>
        {
            saving = false;
            if (!gone && me.Id > 0) source.Me.Value = me;
            source.TracksChanged();
            Compute();
            Told?.Invoke(new MeEvent.Saved(field));
        }, e =>
        {
            saving = false;
            source.Me.Value = before;
            if (e is ApiException { Status: 403 or 404 or 405 })
            {
                readOnly = true;
                Compute();
                Told?.Invoke(new MeEvent.ReadOnly());
                return;
            }
            Compute();
            Told?.Invoke(new MeEvent.Failed(e));
        });
    }

    void Compute()
    {
        if (gone) return;
        var me = source.Me.Value;
        var server = (source.Server ?? "").TrimEnd('/');
        var scopes = (me?.Scope ?? "").Split(new[] { ' ' }, StringSplitOptions.RemoveEmptyEntries);
        State.Value = new MeState(
            me == null ? null : new MeProfile(
                me.Id,
                me.Name,
                string.IsNullOrEmpty(me.AvatarUrl) ? null : Artwork.Absolute(me.AvatarUrl!, server),
                me.AvatarColor,
                me.Managed == true,
                scopes.Contains("profiles"),
                server),
            me == null ? null : new TrackDefaultsState(new TrackDefaults(me.AudioLanguage, me.SubtitleLanguage, me.SubtitleMode), !readOnly && MayChange(scopes), saving),
            signingOut);
    }

    /// <summary>Changing track defaults needs the "play" scope; a sign-in whose scopes aren't known may try.</summary>
    static bool MayChange(string[] scopes) => scopes.Length == 0 || scopes.Contains("play");
}

/// <summary>The Me page's source in the app: <see cref="AppState"/> and its API.</summary>
public sealed class AppMeSource : IMeSource
{
    readonly AppState app;

    public AppMeSource(AppState app) => this.app = app;

    public string? Server => app.Server.Value;

    public Signal<User?> Me => app.Me;

    public async Task<User> SaveTrackDefaults(TrackDefaults defaults)
    {
        var api = app.Api.Value ?? throw new SignedOutException();
        var answer = await api.SetTrackDefaults(defaults.AudioLanguage, defaults.SubtitleLanguage, defaults.SubtitleMode);
        // The profile as the server has it now; an answer without one keeps the change made here.
        try
        {
            return answer is { ValueKind: JsonValueKind.Object } o ? JsonSerializer.Deserialize<User>(o.GetRawText(), ApiJson.Options) ?? new User() : new User();
        }
        catch (JsonException)
        {
            return new User();
        }
    }

    public void TracksChanged() => app.TracksChanged(null);

    public Task SignOut() => app.SignOutNow();
}
