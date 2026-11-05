using System;
using System.Linq;
using System.Text.Json;
using System.Text.Json.Serialization;
using System.Threading;
using System.Threading.Tasks;
using Pley.Tv.Core.Net;

namespace Pley.Tv.Core.Auth;

/// <summary>
/// A code for signing in a TV (RFC 8628): shown as <see cref="UserCode"/> with
/// <see cref="VerificationUri"/> and a QR code of <see cref="VerificationUriComplete"/>, and
/// approved on a phone or computer at the server's /link page.
/// </summary>
/// <param name="DeviceCodeValue">What the TV polls with. Never shown.</param>
/// <param name="UserCode">As shown: <c>BCDF-GHJK</c>.</param>
/// <param name="ExpiresAt">When the code stops working, in ms since the epoch.</param>
public sealed record DeviceCode(
    string Server,
    string Issuer,
    [property: JsonPropertyName("deviceCode")] string DeviceCodeValue,
    string UserCode,
    string VerificationUri,
    string? VerificationUriComplete,
    long ExpiresAt,
    int IntervalSeconds,
    string TokenEndpoint,
    string RevocationEndpoint,
    string Scope)
{
    // Never print the device code.
    public override string ToString() => $"DeviceCode {{ Server = {Server}, UserCode = {UserCode} }}";
}

/// <summary>Where a code stands.</summary>
public abstract record DevicePoll
{
    public sealed record Waiting : DevicePoll;

    /// <summary>Signed in at <paramref name="Issuer"/>, as the profile the approver picked: the tokens are kept already.</summary>
    public sealed record SignedIn(string Issuer) : DevicePoll;

    public sealed record Denied : DevicePoll;

    public sealed record Expired : DevicePoll;

    /// <summary>The server can't be reached: polling goes on by itself in <paramref name="RetryInMs"/>.</summary>
    public sealed record Unreachable(long RetryInMs) : DevicePoll;

    public sealed record Failed(string Message) : DevicePoll;
}

/// <summary>
/// The device authorization grant (RFC 8628 §3) against a Pley server: POST client_id and scope
/// to the metadata's device_authorization_endpoint for a code, then ask the token endpoint with
/// it until someone answers. The tokens it gets go through the scope guard, and replace (and
/// end) the ones the TV held for that server. The Android app's <c>DeviceGrant</c>.
/// </summary>
public sealed class DeviceSignIn
{
    /// <summary>What this app asks for (docs/API.md, "Scopes"), as the Android app does.</summary>
    public static readonly string[] AllScopes = { "library", "play", "playlists", "profiles" };

    /// <summary>
    /// What it asks for too, only where the server lists it in <c>scopes_supported</c>: asking for
    /// movies and shows, and approving them on an administrator's account. A server that doesn't
    /// know a scope refuses the whole sign-in for it.
    /// </summary>
    public static readonly string[] ListedScopes = { "requests", "approvals" };

    /// <summary>RFC 8628 §3.2: without an interval, 5 seconds.</summary>
    public const int DefaultIntervalSeconds = 5;

    /// <summary>What a slow_down adds to this wait and every later one (§3.5).</summary>
    public const long SlowDownMs = 5_000;

    /// <summary>The waits after network failures in a row.</summary>
    public static readonly long[] BackoffMs = { 2_000, 4_000, 8_000, 16_000, 30_000 };

    static string Unreadable => Strings.T("signin.errors.unreadable");

    readonly Transport http;
    readonly string clientId;
    readonly Func<string, Tokens> tokensOf;
    readonly Func<long> clock;
    readonly Func<long, CancellationToken, Task> sleep;

    /// <param name="tokensOf">The tokens of a server, by its issuer: where an approved code's tokens are kept.</param>
    public DeviceSignIn(Transport http, string clientId, Func<string, Tokens> tokensOf, Func<long>? clock = null, Func<long, CancellationToken, Task>? sleep = null)
    {
        this.http = http;
        this.clientId = clientId;
        this.tokensOf = tokensOf;
        this.clock = clock ?? (() => DateTimeOffset.UtcNow.ToUnixTimeMilliseconds());
        this.sleep = sleep ?? ((ms, cancel) => Task.Delay(TimeSpan.FromMilliseconds(ms), cancel));
    }

    /// <summary>
    /// The scope to ask for: what the server offers of Pley's own, and those of
    /// <see cref="ListedScopes"/> it lists. Servers without profile switching don't know "profiles",
    /// nor older ones "requests", and would refuse the whole sign-in for them; one that offers none
    /// of the first is asked for "library".
    /// </summary>
    public static string ScopeFor(ServerMetadata metadata)
    {
        var offered = metadata.ScopesSupported;
        var asked = offered == null ? AllScopes : AllScopes.Where(offered.Contains).ToArray();
        var listed = offered == null ? Array.Empty<string>() : ListedScopes.Where(offered.Contains);
        return string.Join(" ", (asked.Length == 0 ? new[] { "library" } : asked).Concat(listed));
    }

    public async Task<DeviceCode> Start(string server)
    {
        var home = Addresses.Origin(server) ?? throw new AuthException(Metadata.NotAnAddress);
        PlainHttp.Check(home);
        var metadata = await Metadata.Fetch(http, home).ConfigureAwait(false);
        if (!metadata.SignsInWithCodes) throw new AuthException(Metadata.NoCodes);
        var scope = ScopeFor(metadata);
        JsonElement? answer;
        try
        {
            answer = await TokenRequests.PostForm(http, metadata.DeviceAuthorizationEndpoint!, TokenRequests.Fields(
                ("client_id", clientId),
                ("scope", scope))).ConfigureAwait(false);
        }
        catch (NetworkException e)
        {
            throw new UnreachableException(e);
        }
        return ParseDeviceCode(answer, home, metadata, scope, clock());
    }

    /// <summary>
    /// Asks whether <paramref name="code"/> was approved, every interval (slower when the server
    /// says so), until it is signed in, denied or expired; <paramref name="onUpdate"/> hears each
    /// change. Approved, its tokens are kept before it says so. Stops early, answering Waiting,
    /// when <paramref name="stop"/> is cancelled.
    /// </summary>
    public async Task<DevicePoll> Poll(DeviceCode code, Action<DevicePoll> onUpdate, CancellationToken stop)
    {
        long interval = (code.IntervalSeconds > 0 ? code.IntervalSeconds : DefaultIntervalSeconds) * 1000L;
        var failures = 0;
        var troubled = false;
        onUpdate(new DevicePoll.Waiting());
        DevicePoll Done(DevicePoll state)
        {
            onUpdate(state);
            return state;
        }
        long Backoff() => Math.Max(interval, BackoffMs[Math.Min(failures, BackoffMs.Length) - 1]);
        while (true)
        {
            var wait = failures == 0 ? interval : Backoff();
            try
            {
                await sleep(Math.Min(wait, Math.Max(0, code.ExpiresAt - clock())), stop).ConfigureAwait(false);
            }
            catch (OperationCanceledException)
            {
                return new DevicePoll.Waiting();
            }
            if (stop.IsCancellationRequested) return new DevicePoll.Waiting();
            if (clock() >= code.ExpiresAt) return Done(new DevicePoll.Expired());
            DevicePoll? outcome = null;
            try
            {
                var answer = await TokenRequests.PostForm(http, code.TokenEndpoint, TokenRequests.Fields(
                    ("grant_type", Metadata.DeviceCodeGrant),
                    ("device_code", code.DeviceCodeValue),
                    ("client_id", clientId))).ConfigureAwait(false);
                failures = 0;
                outcome = await Keep(code, answer).ConfigureAwait(false);
            }
            catch (TokenException e)
            {
                failures = 0;
                switch (e.OAuth?.Error)
                {
                    case "authorization_pending":
                        break;
                    case "slow_down":
                        interval += SlowDownMs;
                        break;
                    case "access_denied":
                        outcome = new DevicePoll.Denied();
                        break;
                    case "expired_token":
                        outcome = new DevicePoll.Expired();
                        break;
                    // invalid_grant: the code is unknown, or was collected already.
                    case "invalid_grant":
                        outcome = new DevicePoll.Failed(Strings.T("signin.errors.codeSpent"));
                        break;
                    default:
                        outcome = new DevicePoll.Failed(e.Message);
                        break;
                }
            }
            catch (NetworkException)
            {
                failures++;
            }
            catch (AuthException e)
            {
                // Refused by the app itself: plain http beyond the home network.
                outcome = new DevicePoll.Failed(e.Message);
            }
            if (stop.IsCancellationRequested && outcome is not DevicePoll.SignedIn) return new DevicePoll.Waiting();
            if (outcome != null) return Done(outcome);
            if (failures > 0)
            {
                troubled = true;
                onUpdate(new DevicePoll.Unreachable(Backoff()));
            }
            else if (troubled)
            {
                troubled = false;
                onUpdate(new DevicePoll.Waiting());
            }
        }
    }

    /// <summary>Keeps the tokens in <paramref name="answer"/>: what to show.</summary>
    async Task<DevicePoll> Keep(DeviceCode code, JsonElement? answer)
    {
        try
        {
            var parsed = TokenRequests.ParseTokenResponse(answer, code.Scope);
            var fresh = TokenRequests.Stored(parsed, code.Issuer, code.TokenEndpoint, code.RevocationEndpoint, clock());
            await tokensOf(code.Issuer).Install(fresh, code.Scope).ConfigureAwait(false);
            return new DevicePoll.SignedIn(code.Issuer);
        }
        catch (AuthException e)
        {
            // Unreadable, more than was asked (ended already), or not kept.
            return new DevicePoll.Failed(e.Message);
        }
    }

    /// <summary>The code in <paramref name="body"/>, the answer of <paramref name="server"/>'s device authorization endpoint (§3.2).</summary>
    public static DeviceCode ParseDeviceCode(JsonElement? body, string server, ServerMetadata metadata, string scope, long now)
    {
        var bad = new AuthException(Unreadable);
        if (body is not { ValueKind: JsonValueKind.Object } o) throw bad;
        var deviceCode = Json.String(o, "device_code");
        if (string.IsNullOrEmpty(deviceCode)) throw bad;
        var userCode = Shown(Json.String(o, "user_code") ?? "");
        if (userCode.Length == 0) throw bad;
        var uri = Link(Json.String(o, "verification_uri")) ?? throw bad;
        // The page to open must be this server's own: a code typed elsewhere would sign someone else's TV in.
        if (Addresses.Origin(uri) != metadata.Issuer) throw new AuthException(Strings.T("signin.errors.approveElsewhere"));
        var complete = Link(Json.String(o, "verification_uri_complete"));
        if (complete != null && Addresses.Origin(complete) != metadata.Issuer) complete = null;
        // Without a lifetime the code can't be shown honestly: Pley's servers always send one.
        var expiresIn = (long)(Json.Number(o, "expires_in") ?? 0);
        if (expiresIn <= 0) throw bad;
        var given = Json.Number(o, "interval");
        var interval = given is > 0 ? (int)given.Value : DefaultIntervalSeconds;
        return new DeviceCode(
            server,
            metadata.Issuer,
            deviceCode!,
            userCode,
            uri,
            complete,
            now + expiresIn * 1000,
            interval > 0 ? interval : DefaultIntervalSeconds,
            metadata.TokenEndpoint,
            metadata.RevocationEndpoint,
            scope);
    }

    /// <summary>A user code as people read it: 8 letters as <c>BCDF-GHJK</c>, anything else as sent.</summary>
    public static string Shown(string code)
    {
        var letters = new string(code.ToUpperInvariant().Where(char.IsLetterOrDigit).ToArray());
        return letters.Length == 8 ? letters.Substring(0, 4) + "-" + letters.Substring(4) : Plain.Of(code);
    }

    /// <summary><paramref name="text"/> when it is a web address to show (http or https, with a host), else null.</summary>
    static string? Link(string? text)
    {
        var clean = Plain.Of(text);
        if (clean.Length == 0 || clean != text || clean.Any(char.IsWhiteSpace)) return null;
        if (!Uri.TryCreate(clean, UriKind.Absolute, out var uri)) return null;
        return (uri.Scheme == "http" || uri.Scheme == "https") && uri.Host.Length > 0 ? clean : null;
    }
}
