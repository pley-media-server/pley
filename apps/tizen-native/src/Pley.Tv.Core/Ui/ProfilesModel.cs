using System;
using System.Collections.Generic;
using System.Linq;
using System.Text;
using System.Threading.Tasks;
using Pley.Tv.Core.Api;
using Pley.Tv.Core.Auth;
using Pley.Tv.Core.Net;

namespace Pley.Tv.Core.Ui;

/// <summary>A profile on "Who's watching?".</summary>
/// <param name="AvatarUrl">The picture's full address, or null for the initial on <paramref name="Color"/>.</param>
/// <param name="Color">"#e07a5f"; null: picked from the name, as the web does.</param>
/// <param name="Needs">What switching to it takes: "none", "pin", "password" or "locked".</param>
/// <param name="Current">The profile the app uses now.</param>
public sealed record ProfileTile(long Id, string Name, string? AvatarUrl, string? Color, string Needs, bool Current)
{
    /// <summary>A lock badge: switching to it takes its PIN.</summary>
    public bool Locked => Needs == "pin";

    public string Initial
    {
        get
        {
            var trimmed = Name.Trim();
            return trimmed.Length > 0 ? char.ToUpperInvariant(trimmed[0]).ToString() : "?";
        }
    }

    /// <summary><paramref name="profile"/> as a tile, its picture's address made whole against <paramref name="origin"/>.</summary>
    public static ProfileTile Of(HouseholdProfile profile, string origin)
    {
        var url = profile.AvatarUrl;
        if (string.IsNullOrEmpty(url)) url = null;
        else if (!url!.StartsWith("http://", StringComparison.Ordinal) && !url.StartsWith("https://", StringComparison.Ordinal))
            url = origin.TrimEnd('/') + "/" + url.TrimStart('/');
        return new ProfileTile(profile.Id, profile.Name, url, string.IsNullOrEmpty(profile.AvatarColor) ? null : profile.AvatarColor, profile.Takes, profile.Current);
    }
}

/// <summary>The web's profile colours (apps/web/src/components/Avatar.tsx; the Android app's ProfileColors).</summary>
public static class ProfileColors
{
    static readonly uint[] Palette = { 0x38BDF8, 0xA78BFA, 0xF472B6, 0xFB923C, 0xFACC15, 0x4ADE80, 0x2DD4BF, 0xF87171 };

    /// <summary>The profile's own colour as 0xRRGGBB, or one picked from its name.</summary>
    public static uint Of(string name, string? chosen)
    {
        var parsed = chosen == null ? null : Parse(chosen);
        if (parsed != null) return parsed.Value;
        ulong hash = 0;
        var i = 0;
        while (i < name.Length)
        {
            hash = (hash * 31 + name[i]) & 0xFFFFFFFFUL;
            // A step per code point, as the web's for…of; the first unit of a pair is what counts.
            i += char.IsHighSurrogate(name[i]) && i + 1 < name.Length && char.IsLowSurrogate(name[i + 1]) ? 2 : 1;
        }
        return Palette[hash % (ulong)Palette.Length];
    }

    /// <summary>Whether text on <paramref name="rgb"/> is dark (Bg) rather than white: the web's luminance test, on unconverted channels.</summary>
    public static bool DarkTextOn(uint rgb)
    {
        float r = (rgb >> 16 & 0xff) / 255f, g = (rgb >> 8 & 0xff) / 255f, b = (rgb & 0xff) / 255f;
        return 0.2126f * r + 0.7152f * g + 0.0722f * b > 0.45f;
    }

    static uint? Parse(string hex)
    {
        var digits = hex.StartsWith("#", StringComparison.Ordinal) ? hex.Substring(1) : hex;
        if ((digits.Length != 3 && digits.Length != 6) || !digits.All(Uri.IsHexDigit)) return null;
        var full = digits.Length == 3 ? string.Concat(digits.Select(c => new string(c, 2))) : digits;
        return Convert.ToUInt32(full, 16);
    }
}

/// <summary>Up to 8 digits; OK from 4 (PINs are 4 to 8 digits long). Never shown, never kept.</summary>
public sealed class PinEntry
{
    public const int Min = 4, Max = 8;
    readonly StringBuilder digits = new StringBuilder();

    public int Length => digits.Length;
    public bool Complete => digits.Length >= Min;

    /// <summary>Adds <paramref name="digit"/> (0 to 9); false once there are 8.</summary>
    public bool Add(int digit)
    {
        if (digit < 0 || digit > 9 || digits.Length >= Max) return false;
        digits.Append((char)('0' + digit));
        return true;
    }

    public bool Delete()
    {
        if (digits.Length == 0) return false;
        digits.Length--;
        return true;
    }

    public void Clear() => digits.Clear();

    /// <summary>The PIN, once; the entry is empty after.</summary>
    public string Take()
    {
        var pin = digits.ToString();
        Clear();
        return pin;
    }

    public override string ToString() => $"PinEntry({Length} digits)";
}

/// <summary>What "Who's watching?" shows.</summary>
public abstract record ProfilesStep
{
    /// <summary>The household's profiles.</summary>
    public sealed record Pick : ProfilesStep;

    /// <summary>
    /// The PIN pad for <paramref name="Profile"/>: <paramref name="Digits"/> typed, the server's
    /// <paramref name="Error"/> after a refusal (the dots cleared; <paramref name="Shakes"/> counts
    /// them, for the shake), <paramref name="LockedOut"/> when the PIN is locked or the profile
    /// can't be used. <paramref name="Confirm"/>: the profile is the one in use (a TV starting as it).
    /// </summary>
    public sealed record Pin(ProfileTile Profile, int Digits = 0, string? Error = null, int Shakes = 0, bool LockedOut = false, bool Busy = false, bool Confirm = false) : ProfilesStep;

    /// <summary>The profile needs its password, so a phone approves a new code where its owner picks it; id 0: any profile.</summary>
    public sealed record Approve(ProfileTile Profile) : ProfilesStep;
}

public sealed record ProfilesState
{
    public string? Server { get; init; }
    public bool Loading { get; init; } = true;
    public IReadOnlyList<ProfileTile> Profiles { get; init; } = Array.Empty<ProfileTile>();
    /// <summary>The profile to focus first: the last one used, so OK continues.</summary>
    public long? Focus { get; init; }
    /// <summary>False without the <c>profiles</c> scope: "Sign in as another profile" instead.</summary>
    public bool CanSwitch { get; init; } = true;
    public ProfilesStep Step { get; init; } = new ProfilesStep.Pick();
    /// <summary>On the picker: the list couldn't load, or a switch failed.</summary>
    public string? Error { get; init; }
    /// <summary>The profile being switched to.</summary>
    public long? Busy { get; init; }
}

/// <summary>How a PIN check of the profile in use went.</summary>
public enum PinCheck
{
    Confirmed,
    /// <summary>The server can't check a PIN for the profile the app uses: go on without it.</summary>
    Unsupported,
}

/// <summary>What "Who's watching?" reads and changes: the app's state, or a fake in tests.</summary>
public interface IProfilesSource
{
    string? Server { get; }

    /// <summary>Who the app is now.</summary>
    User? Me { get; }

    /// <summary>The household as last seen; empty when never.</summary>
    IReadOnlyList<HouseholdProfile> Saved();

    /// <summary>Whether this sign-in may switch profiles: the server offers it and allowed it.</summary>
    bool CanSwitch();

    /// <summary>The household (GET /api/oauth/profiles), kept for next time.</summary>
    Task<IReadOnlyList<HouseholdProfile>> Load();

    /// <summary>Switches to <paramref name="id"/>, with <paramref name="pin"/> when it needs one; throws <see cref="TokenException"/> when refused.</summary>
    Task SwitchTo(long id, string? pin);

    /// <summary>Checks the PIN of the profile in use; throws <see cref="TokenException"/> when it is refused.</summary>
    Task<PinCheck> ConfirmPin(long id, string pin);
}

/// <summary>"Who's watching?"'s source in the app: <see cref="AppState"/> and its tokens.</summary>
public sealed class AppProfilesSource : IProfilesSource
{
    readonly AppState app;

    public AppProfilesSource(AppState app) => this.app = app;

    public string? Server => app.Server.Value;

    public User? Me => app.Me.Value;

    public IReadOnlyList<HouseholdProfile> Saved() => app.SavedHousehold();

    public bool CanSwitch() => app.TokenStore?.Scopes().Contains("profiles") == true;

    public async Task<IReadOnlyList<HouseholdProfile>> Load()
    {
        var api = app.Api.Value ?? throw new SignedOutException();
        // Who the app is, for the current profile's PIN; alongside the list.
        var me = app.Me.Value == null ? app.LoadAccount() : Task.CompletedTask;
        var list = await api.Profiles();
        await me;
        // Profiles without a real id can't be switched to: they aren't shown (the Android app's parseList).
        var shown = list.Where(p => p.Id > 0).Select(p => p with { Name = Plain.Of(p.Name) }).ToList();
        app.KeepHousehold(shown);
        return shown;
    }

    public async Task SwitchTo(long id, string? pin)
    {
        var tokens = app.TokenStore ?? throw new SignedOutException();
        await tokens.SwitchProfile(id, pin);
        try
        {
            await app.Switched(id);
        }
        catch (Exception)
        {
            // The switch is made: who is watching and the libraries failed to load after it, which
            // AppState keeps for the pages' Retry (AccountError), as Android's switch doesn't wait for them.
        }
    }

    public async Task<PinCheck> ConfirmPin(long id, string pin)
    {
        var tokens = app.TokenStore ?? throw new SignedOutException();
        try
        {
            await tokens.SwitchProfile(id, pin);
            return PinCheck.Confirmed;
        }
        catch (TokenException e) when (e.OAuth?.Error == "invalid_request" && e.OAuth.Reason == null)
        {
            return PinCheck.Unsupported;
        }
    }
}

/// <summary>
/// "Who's watching?" and the PIN pad (the Android app's ProfilesViewModel, as a TV): the household
/// from GET /api/oauth/profiles (shown at once from what was saved), and the switch-profile grant
/// with a PIN when the profile has one. Wrong PINs clear the dots and show the server's sentence;
/// nothing is retried by itself (100 wrong PINs in a row turn a PIN off). A profile that needs its
/// password is approved on a phone.
///
/// <c>gate</c>: a cold start, where continuing as a PIN profile asks its PIN (servers that can't
/// check it let it through). <c>pin</c>: a start as the profile in use ("Always start as"), whose
/// pad opens first; Back shows the profiles.
/// </summary>
public sealed class ProfilesModel
{
    static string LockedText => Strings.T("profiles.errors.locked");

    readonly IProfilesSource source;
    readonly bool gate;
    readonly PinEntry pin = new PinEntry();
    IReadOnlyList<HouseholdProfile> list = Array.Empty<HouseholdProfile>();
    /// <summary>The pad of the profile in use opens as soon as the profiles show it with a PIN.</summary>
    bool padFirst;

    public ProfilesModel(IProfilesSource source, bool gate, bool pin = false)
    {
        this.source = source;
        this.gate = gate;
        padFirst = gate && pin;
    }

    public Signal<ProfilesState> State { get; } = new Signal<ProfilesState>(new ProfilesState());

    /// <summary>A profile was chosen: Home.</summary>
    public event Action? Done;

    /// <summary>The server signed the app out: sign in again.</summary>
    public event Action? SignedOut;

    /// <summary>
    /// A refused switch in a few words, by the server's reason (the Android app's Profiles.message).
    /// Throttled, the server's own sentence says how long to wait.
    /// </summary>
    public static string Message(OAuthError? error) => error?.Reason switch
    {
        "wrong_pin" => Strings.T("profiles.errors.wrongPin"),
        "throttled" => error.Description ?? Strings.T("profiles.errors.throttled"),
        "pin_locked" => Strings.T("profiles.errors.pinLocked"),
        "browser" => Strings.T("profiles.errors.browser"),
        "not_allowed" => Strings.T("profiles.errors.notAllowed"),
        _ => error?.Error switch
        {
            "unauthorized_client" => Strings.T("profiles.errors.unauthorizedClient"),
            "invalid_request" => error.Description ?? Strings.T("profiles.errors.notAllowed"),
            _ => error?.Description ?? Strings.T("profiles.errors.refused"),
        },
    };

    /// <summary>Shows the saved household at once, then asks the server.</summary>
    public Task Load() => LoadNow();

    /// <summary>Loads the household again (Retry).</summary>
    public Task Reload() => LoadNow();

    async Task LoadNow()
    {
        var origin = source.Server;
        if (origin == null)
        {
            SignedOut?.Invoke();
            return;
        }
        Update(s => s with { Server = origin, Error = null });
        var saved = source.Saved();
        if (saved.Count > 0 && list.Count == 0) Show(origin, saved);
        if (!source.CanSwitch())
        {
            Update(s => s with { Loading = false, CanSwitch = false });
            if (saved.Count == 0) return;
        }
        try
        {
            Show(origin, await source.Load());
        }
        catch (SignedOutException)
        {
            SignedOut?.Invoke();
        }
        catch (Exception e) when (e is NetworkException or ApiException or AuthException)
        {
            Update(s => s with { Loading = false, Error = list.Count == 0 ? Errors.Message(e) : null });
        }
    }

    void Show(string origin, IReadOnlyList<HouseholdProfile> profiles)
    {
        list = profiles;
        // A saved list may be from before a switch: who the app is now says which is current.
        var me = source.Me;
        if (me != null && !profiles.Any(p => p.Id == me.Id)) me = null;
        var tiles = profiles.Select(p =>
        {
            var tile = ProfileTile.Of(p, origin);
            if (me == null) return tile;
            return p.Id == me.Id ? tile with { Current = true, Needs = me.HasPin ? "pin" : "none" } : tile with { Current = false };
        }).ToList();
        var pad = FirstPad(tiles);
        Update(s => s with
        {
            Loading = false,
            Profiles = tiles,
            Focus = s.Focus != null && tiles.Any(t => t.Id == s.Focus) ? s.Focus : tiles.FirstOrDefault(t => t.Current)?.Id ?? tiles.FirstOrDefault()?.Id,
            Step = pad != null && s.Step is ProfilesStep.Pick ? new ProfilesStep.Pin(pad, Confirm: true) : s.Step,
        });
    }

    /// <summary>The profile in use, the first time <paramref name="tiles"/> show it with a PIN, when its pad comes first.</summary>
    ProfileTile? FirstPad(IReadOnlyList<ProfileTile> tiles)
    {
        if (!padFirst) return null;
        var current = tiles.FirstOrDefault(t => t.Current && t.Locked);
        if (current != null) padFirst = false;
        return current;
    }

    /// <summary>The profile <paramref name="id"/> was chosen.</summary>
    public Task Pick(long id)
    {
        var tile = State.Value.Profiles.FirstOrDefault(p => p.Id == id);
        if (tile == null || State.Value.Busy != null) return Task.CompletedTask;
        Update(s => s with { Focus = id, Error = null });
        if (tile.Current)
        {
            var hasPin = source.Me is { } me && me.Id == id && me.HasPin;
            if (gate && hasPin) OpenPad(tile, confirm: true);
            else Finish();
            return Task.CompletedTask;
        }
        switch (tile.Needs)
        {
            case "none":
                return SwitchTo(tile, null);
            case "pin":
                OpenPad(tile, confirm: false);
                break;
            case "password":
                Update(s => s with { Step = new ProfilesStep.Approve(tile) });
                break;
            default:
                pin.Clear();
                Update(s => s with { Step = new ProfilesStep.Pin(tile, LockedOut: true, Error: Errors.AsSentence(LockedText)) });
                break;
        }
        return Task.CompletedTask;
    }

    /// <summary>"Sign in as another profile" (no <c>profiles</c> scope): a phone approves a new code.</summary>
    public void SignInAgain()
    {
        var me = State.Value.Profiles.FirstOrDefault(p => p.Current) ?? new ProfileTile(0, "", null, null, "password", true);
        Update(s => s with { Step = new ProfilesStep.Approve(me with { Id = 0 }) });
    }

    void OpenPad(ProfileTile tile, bool confirm)
    {
        pin.Clear();
        Update(s => s with { Step = new ProfilesStep.Pin(tile, Confirm: confirm) });
    }

    // ---- the PIN pad

    public void Digit(int d) => EditPin(() => pin.Add(d));

    public void Delete() => EditPin(pin.Delete);

    void EditPin(Func<bool> change)
    {
        if (State.Value.Step is not ProfilesStep.Pin step || step.LockedOut || step.Busy) return;
        if (change()) Update(s => s with { Step = step with { Digits = pin.Length } });
    }

    /// <summary>OK: sends the PIN (4 digits at least).</summary>
    public Task Submit()
    {
        if (State.Value.Step is not ProfilesStep.Pin step || step.LockedOut || step.Busy || !pin.Complete) return Task.CompletedTask;
        var typed = pin.Take();
        Update(s => s with { Step = step with { Digits = 0, Busy = true, Error = null } });
        return step.Confirm ? Confirm(step.Profile, typed) : SwitchTo(step.Profile, typed);
    }

    /// <summary>Back on the pad, "Use another profile", or leaving the approval: the picker again.</summary>
    public void BackToPick()
    {
        pin.Clear();
        Update(s => s with { Step = new ProfilesStep.Pick(), Busy = null });
    }

    // ---- switching

    async Task SwitchTo(ProfileTile tile, string? typed)
    {
        Update(s => s with { Busy = tile.Id });
        try
        {
            await source.SwitchTo(tile.Id, typed);
            Update(s => s with { Busy = null });
            Finish();
        }
        catch (SignedOutException)
        {
            Update(s => s with { Busy = null });
            SignedOut?.Invoke();
        }
        catch (TokenException e)
        {
            Update(s => s with { Busy = null });
            Refused(tile, e);
        }
        catch (UnreachableException e)
        {
            Update(s => s with { Busy = null });
            Unreachable(e);
        }
        catch (NetworkException e)
        {
            Update(s => s with { Busy = null });
            Unreachable(e);
        }
        catch (AuthException e)
        {
            Update(s => s with { Busy = null });
            Failed(tile, e.Message);
        }
    }

    async Task Confirm(ProfileTile tile, string typed)
    {
        try
        {
            await source.ConfirmPin(tile.Id, typed);
            Finish();
        }
        catch (SignedOutException)
        {
            SignedOut?.Invoke();
        }
        catch (TokenException e)
        {
            // Locked by the server's reason, as Refused has it: the words are the language shown's.
            Wrong(tile, Message(e.OAuth), lockedOut: e.OAuth?.Reason == "pin_locked");
        }
        catch (Exception e) when (e is NetworkException or UnreachableException)
        {
            Unreachable(e);
        }
    }

    /// <summary>The server refused the switch: by its reason.</summary>
    void Refused(ProfileTile tile, TokenException e)
    {
        var oauth = e.OAuth;
        switch (oauth?.Reason)
        {
            case "browser":
                Update(s => s with { Step = new ProfilesStep.Approve(tile) });
                break;
            case "pin_locked":
                Wrong(tile, Message(oauth), lockedOut: true);
                break;
            case "throttled":
                Wrong(tile, Message(oauth), lockedOut: false);
                break;
            case "wrong_pin":
                if (tile.Needs == "pin" || State.Value.Step is ProfilesStep.Pin) Wrong(tile, Message(oauth), lockedOut: false);
                // The list was older than the profile's new PIN.
                else OpenPad(tile with { Needs = "pin" }, confirm: false);
                break;
            default:
                Failed(tile, Message(oauth));
                break;
        }
    }

    void Wrong(ProfileTile tile, string message, bool lockedOut)
    {
        pin.Clear();
        Update(s =>
        {
            var step = s.Step as ProfilesStep.Pin ?? new ProfilesStep.Pin(tile);
            return s with { Step = step with { Digits = 0, Busy = false, Error = Errors.AsSentence(message), Shakes = step.Shakes + 1, LockedOut = lockedOut } };
        });
    }

    /// <summary>The server couldn't be asked: not a wrong PIN, so the pad doesn't shake; the PIN can be typed again.</summary>
    void Unreachable(Exception e)
    {
        var cause = e is UnreachableException { InnerException: NetworkException n } ? n : e;
        var message = Errors.AsSentence(Strings.T("profiles.errors.unreachable", ("reason", Errors.Describe(cause))));
        pin.Clear();
        Update(s => s.Step is ProfilesStep.Pin step
            ? s with { Step = step with { Digits = 0, Busy = false, Error = message } }
            : s with { Error = message });
    }

    void Failed(ProfileTile tile, string message)
    {
        if (State.Value.Step is ProfilesStep.Pin) Wrong(tile, message, lockedOut: false);
        else Update(s => s with { Error = Errors.AsSentence(message) });
    }

    void Finish()
    {
        pin.Clear();
        Done?.Invoke();
    }

    void Update(Func<ProfilesState, ProfilesState> change) => State.Value = change(State.Value);
}
