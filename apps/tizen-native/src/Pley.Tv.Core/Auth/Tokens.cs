using System;
using System.Collections.Generic;
using System.Linq;
using System.Text.Json;
using System.Text.Json.Serialization;
using System.Threading;
using System.Threading.Tasks;
using Pley.Tv.Core.Net;

namespace Pley.Tv.Core.Auth;

/// <summary>Where small things are kept between launches.</summary>
public interface IKeyValue
{
    string? Get(string key);
    void Set(string key, string value);
    void Remove(string key);
}

/// <summary>
/// Seals what is kept of a sign-in, so the tokens aren't readable where the app keeps its
/// other things (the Android app's Vault, whose key lives in the Android Keystore). Each value
/// is bound to <c>aad</c>, the server it belongs to, so one server's tokens can't be passed off
/// as another's.
/// </summary>
public interface ITokenVault
{
    /// <summary><paramref name="plain"/> sealed with <paramref name="aad"/>; throws when it can't be.</summary>
    byte[] Seal(byte[] plain, string aad);

    /// <summary>What <see cref="Seal"/> sealed with <paramref name="aad"/>; null when it wasn't, or the key is gone.</summary>
    byte[]? Open(byte[] sealedValue, string aad);
}

/// <summary>Keeps tokens as they are: for tests, and until the TV gives a vault of its own.</summary>
public sealed class PlainVault : ITokenVault
{
    public byte[] Seal(byte[] plain, string aad) => plain;

    public byte[]? Open(byte[] sealedValue, string aad) => sealedValue;
}

/// <summary>The tokens of one server, as kept.</summary>
/// <param name="ExpiresAt">When the access token ends, in ms since the epoch.</param>
/// <param name="Proof">The server's id and this sign-in's proof key; absent from servers before them.</param>
public sealed record StoredTokens(
    string Issuer,
    string AccessToken,
    string? RefreshToken,
    long ExpiresAt,
    string Scope,
    string TokenEndpoint,
    string RevocationEndpoint,
    ServerProof? Proof = null)
{
    // Never print the tokens.
    public override string ToString() => $"StoredTokens {{ Issuer = {Issuer}, Scope = {Scope} }}";
}

/// <summary>A token endpoint's error (RFC 6749 §5.2), with Pley's <c>reason</c> for profile switches.</summary>
public sealed record OAuthError(string Error, string? Description, string? Reason);

/// <summary>A token answer (RFC 6749 §5.1).</summary>
/// <param name="Proof">What tells the server from another device later; absent from servers before it.</param>
public sealed record TokenResponse(
    string AccessToken,
    string? RefreshToken,
    long ExpiresIn,
    string Scope,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] ServerProof? Proof = null)
{
    public override string ToString() => $"TokenResponse {{ ExpiresIn = {ExpiresIn}, Scope = {Scope} }}";
}

/// <summary>The token endpoint said no: in the server's words, or its error code, or its status.</summary>
public class TokenException : AuthException
{
    public TokenException(int status, OAuthError? oauth) : base(oauth?.Description ?? oauth?.Error ?? Strings.T("common.errors.refused", ("status", status)))
    {
        Status = status;
        OAuth = oauth;
    }

    public int Status { get; }
    public OAuthError? OAuth { get; }
}

public class SignedOutException : AuthException
{
    public SignedOutException() : base(Strings.T("signin.errors.signedOut"))
    {
    }
}

/// <summary>Why the TV is signed out.</summary>
public enum SignOutReason
{
    /// <summary>The server ended the sign-in.</summary>
    Ended,
    /// <summary>A token answer held more than this app asks for; its grant was ended.</summary>
    WiderScope,
    /// <summary>The tokens were kept but can't be read anymore (the vault's key went, or what was kept is damaged).</summary>
    Unreadable,
}

/// <summary>The Android app's token requests: reading and sending them.</summary>
public static class TokenRequests
{
    public static string WiderScopeMessage => Strings.T("signin.errors.widerScope");

    /// <summary>Why tokens couldn't be sealed: they aren't used, and the person signs in again.</summary>
    public static string Unkept => Strings.T("signin.errors.unkept");

    /// <summary>Pley's access tokens last an hour; an answer without expires_in is read as that.</summary>
    const long DefaultExpiresIn = 3600;

    static string Unreadable => Strings.T("signin.errors.unreadable");

    public static TokenResponse ParseTokenResponse(JsonElement? body, string askedScope)
    {
        if (body is not { ValueKind: JsonValueKind.Object } r) throw new AuthException(Unreadable);
        var access = Json.String(r, "access_token");
        if (string.IsNullOrEmpty(access)) throw new AuthException(Unreadable);
        // RFC 6749 §5.1: token_type is required, and Pley's apps take bearer tokens only.
        if (!string.Equals(Json.String(r, "token_type"), "Bearer", StringComparison.OrdinalIgnoreCase)) throw new AuthException(Unreadable);
        long expiresIn = DefaultExpiresIn;
        if (Json.Field(r, "expires_in") != null)
        {
            var given = Json.Number(r, "expires_in");
            if (given is not > 0) throw new AuthException(Unreadable);
            expiresIn = (long)given.Value;
            if (expiresIn <= 0) throw new AuthException(Unreadable);
        }
        var refresh = Json.String(r, "refresh_token");
        var scope = Json.String(r, "scope");
        return new TokenResponse(
            access!,
            string.IsNullOrEmpty(refresh) ? null : refresh,
            expiresIn,
            // Without a scope, the server issued what was asked (RFC 6749 §5.1).
            string.IsNullOrEmpty(scope) ? askedScope : scope!,
            ServerProofs.Of(r));
    }

    public static OAuthError? ParseOAuthError(JsonElement? body)
    {
        if (body is not { ValueKind: JsonValueKind.Object } r) return null;
        var error = Json.String(r, "error");
        if (string.IsNullOrEmpty(error)) return null;
        // The description is shown as it is: one short line without control characters.
        var description = Plain.Of(Json.String(r, "error_description"));
        var reason = Json.String(r, "reason");
        return new OAuthError(error!, description.Length == 0 ? null : description, string.IsNullOrEmpty(reason) ? null : reason);
    }

    /// <summary>The scope names in <paramref name="answer"/> that <paramref name="asked"/> doesn't hold.</summary>
    public static IReadOnlyList<string> ExtraScopes(string answer, string asked)
    {
        var have = asked.Split(' ').Where(s => s.Length > 0).ToList();
        return answer.Split(' ').Where(s => s.Length > 0 && !have.Contains(s)).Distinct().ToList();
    }

    /// <summary>
    /// POSTs <paramref name="fields"/> to a token endpoint, without following a redirect (codes and
    /// tokens go to the address the metadata named, or nowhere) and never over plain http beyond
    /// the home network; the answer's JSON, or a <see cref="TokenException"/>.
    /// </summary>
    public static async Task<JsonElement?> PostForm(Transport http, string url, IEnumerable<KeyValuePair<string, string>> fields)
    {
        PlainHttp.Check(url);
        var res = await http(new HttpRequest
        {
            Method = "POST",
            Url = url,
            Headers = new Dictionary<string, string> { ["Content-Type"] = "application/x-www-form-urlencoded", ["Accept"] = "application/json" },
            Body = Transports.Form(fields),
            ResponseType = ResponseType.Json,
            TimeoutMs = Metadata.ReadTimeoutMs,
        }).ConfigureAwait(false);
        if (res.Status < 200 || res.Status >= 300) throw new TokenException(res.Status, ParseOAuthError(res.Json()));
        return res.Json();
    }

    /// <summary>
    /// The tokens of <paramref name="answer"/> as kept. A refresh or a profile switch is the same
    /// sign-in, whose key never changes: <paramref name="before"/>'s proof stays, whatever the
    /// answer says, and an answer brings one only to a sign-in without.
    /// </summary>
    public static StoredTokens Stored(TokenResponse answer, string issuer, string tokenEndpoint, string revocationEndpoint, long now, StoredTokens? before = null) =>
        new StoredTokens(issuer, answer.AccessToken, answer.RefreshToken, now + answer.ExpiresIn * 1000, answer.Scope, tokenEndpoint, revocationEndpoint,
            before?.Proof ?? answer.Proof);

    /// <summary>Ends the grant of <paramref name="tokens"/> on its server; trouble is ignored (the person can sign the TV out in Account → Apps).</summary>
    public static async Task Revoke(Transport http, string clientId, StoredTokens tokens)
    {
        // The refresh token ends the whole grant; the access token too, when there is no other.
        var token = tokens.RefreshToken ?? tokens.AccessToken;
        try
        {
            await PostForm(http, tokens.RevocationEndpoint, Fields(("token", token), ("client_id", clientId))).ConfigureAwait(false);
        }
        catch (NetworkException)
        {
        }
        catch (AuthException)
        {
        }
    }

    internal static List<KeyValuePair<string, string>> Fields(params (string Key, string Value)[] fields) =>
        fields.Select(f => new KeyValuePair<string, string>(f.Key, f.Value)).ToList();
}

/// <summary>
/// The tokens of one server: the access token while it works, a new pair from the refresh token
/// when it doesn't. Refreshes happen one at a time (the server ends the sign-in when a refresh
/// token is used twice), and the new refresh token is saved before the new access token is used.
/// They are kept sealed by the vault (<see cref="ITokenVault"/>). No token leaves before the
/// server at the address has proved it is the one signed in to (<see cref="ServerProofs"/>).
/// </summary>
public sealed class Tokens
{
    /// <summary>How long before its end an access token is refreshed.</summary>
    const long EarlyMs = 60_000;

    /// <summary>How long a refreshed token counts as just issued: refused within this, the sign-in is over.</summary>
    public const long JustIssuedMs = 60_000;

    readonly SemaphoreSlim queue = new SemaphoreSlim(1, 1);
    readonly object kept = new object();
    readonly object proofs = new object();
    readonly IKeyValue store;
    readonly ITokenVault vault;
    readonly Transport http;
    readonly Action<SignOutReason> onSignedOut;
    readonly Func<long> clock;
    readonly Func<int, byte[]> random;
    /// <summary>The access token the last refresh or profile switch gave, and when.</summary>
    (string Token, long At)? lastIssued;
    /// <summary>What was read last, and as what, so requests don't each open the vault.</summary>
    (string Text, StoredTokens Tokens)? opened;
    /// <summary>When, and on which network (<see cref="ServerProofs.Network"/>), the server last proved itself; the proof under way.</summary>
    (long At, int Network)? provedAt;
    Task? proving;

    /// <param name="random">The challenges' random bytes.</param>
    public Tokens(string issuer, string clientId, IKeyValue store, Transport http, Action<SignOutReason>? onSignedOut = null, Func<long>? clock = null,
        ITokenVault? vault = null, Func<int, byte[]>? random = null)
    {
        Issuer = issuer;
        ClientId = clientId;
        this.store = store;
        this.http = http;
        this.vault = vault ?? new PlainVault();
        this.onSignedOut = onSignedOut ?? (_ => { });
        this.clock = clock ?? (() => DateTimeOffset.UtcNow.ToUnixTimeMilliseconds());
        this.random = random ?? ServerProofs.RandomBytes;
    }

    public string Issuer { get; }
    public string ClientId { get; }

    string Key => $"pley.tokens.{Issuer}";

    /// <summary>
    /// The tokens kept, opened; null when signed out. Tokens kept but unreadable (the vault's key
    /// went, or a field is missing) are removed, and the TV is signed out with
    /// <see cref="SignOutReason.Unreadable"/>.
    /// </summary>
    public StoredTokens? Load()
    {
        bool unreadable;
        StoredTokens? tokens;
        lock (kept)
        {
            var text = store.Get(Key);
            if (string.IsNullOrEmpty(text))
            {
                opened = null;
                return null;
            }
            if (opened is { } o && o.Text == text) return o.Tokens;
            tokens = Open(text!);
            unreadable = tokens == null;
            if (unreadable) store.Remove(Key);
            opened = tokens == null ? null : (text!, tokens);
        }
        if (unreadable) onSignedOut(SignOutReason.Unreadable);
        return tokens;
    }

    /// <summary>Whether tokens are kept, without opening them: for choosing the first screen. They may still turn out unreadable.</summary>
    public bool Kept => !string.IsNullOrEmpty(store.Get(Key));

    StoredTokens? Open(string text)
    {
        try
        {
            byte[]? plain;
            try
            {
                plain = vault.Open(Convert.FromBase64String(text), Issuer);
            }
            catch (Exception e) when (e is not FormatException)
            {
                // The vault failed (its key went with a reset): as unreadable as a damaged value.
                plain = null;
            }
            if (plain == null) return null;
            using var doc = JsonDocument.Parse(plain);
            var o = doc.RootElement;
            var issuer = Json.String(o, "issuer");
            var access = Json.String(o, "accessToken");
            var refresh = Json.Field(o, "refreshToken");
            var expiresAt = Json.Number(o, "expiresAt");
            var scope = Json.String(o, "scope");
            var tokenEndpoint = Json.String(o, "tokenEndpoint");
            var revocationEndpoint = Json.String(o, "revocationEndpoint");
            // Every field is checked: tokens kept by another build, or damaged, aren't used.
            if (issuer != Issuer || string.IsNullOrEmpty(access) || expiresAt == null || scope == null ||
                string.IsNullOrEmpty(tokenEndpoint) || string.IsNullOrEmpty(revocationEndpoint) ||
                (refresh != null && refresh.Value.ValueKind is not (JsonValueKind.String or JsonValueKind.Null)))
            {
                return null;
            }
            var refreshToken = refresh is { ValueKind: JsonValueKind.String } r ? r.GetString() : null;
            var proof = ServerProofs.Of(Json.String(o, "serverId"), Json.String(o, "proofId"), Json.String(o, "proofKey"));
            return new StoredTokens(issuer!, access!, string.IsNullOrEmpty(refreshToken) ? null : refreshToken, (long)expiresAt.Value, scope, tokenEndpoint!, revocationEndpoint!,
                proof);
        }
        catch (Exception e) when (e is FormatException or JsonException or ArgumentException)
        {
            return null;
        }
    }

    /// <summary>Keeps <paramref name="tokens"/>, sealed; when that fails, they aren't used.</summary>
    void Save(StoredTokens tokens)
    {
        var fields = new Dictionary<string, object?>
        {
            ["issuer"] = tokens.Issuer,
            ["accessToken"] = tokens.AccessToken,
            ["refreshToken"] = tokens.RefreshToken,
            ["expiresAt"] = tokens.ExpiresAt,
            ["scope"] = tokens.Scope,
            ["tokenEndpoint"] = tokens.TokenEndpoint,
            ["revocationEndpoint"] = tokens.RevocationEndpoint,
        };
        if (tokens.Proof is { } proof)
        {
            fields["serverId"] = proof.ServerId;
            fields["proofId"] = proof.ProofId;
            fields["proofKey"] = proof.ProofKey;
        }
        var json = JsonSerializer.SerializeToUtf8Bytes(fields);
        byte[] sealedValue;
        try
        {
            sealedValue = vault.Seal(json, Issuer);
        }
        catch (Exception e) when (e is not AuthException)
        {
            throw new AuthException(TokenRequests.Unkept, e);
        }
        var text = Convert.ToBase64String(sealedValue);
        lock (kept)
        {
            opened = null;
            store.Set(Key, text);
            opened = (text, tokens);
        }
    }

    void Wipe()
    {
        lock (kept)
        {
            store.Remove(Key);
            opened = null;
        }
        lock (proofs) provedAt = null;
    }

    public bool SignedIn => Load() != null;

    public IReadOnlyList<string> Scopes() => (Load()?.Scope ?? "").Split(' ').Where(s => s.Length > 0).ToList();

    /// <summary>Runs <paramref name="work"/> after every earlier refresh or switch: one at a time.</summary>
    async Task<T> Serial<T>(Func<Task<T>> work)
    {
        await queue.WaitAsync().ConfigureAwait(false);
        try
        {
            return await work().ConfigureAwait(false);
        }
        finally
        {
            queue.Release();
        }
    }

    Task Serial(Func<Task> work) => Serial(async () =>
    {
        await work().ConfigureAwait(false);
        return true;
    });

    /// <summary>
    /// Completes once the server at this address has proved it is the one signed in to, within the
    /// last half hour and on this network, or now when <paramref name="now"/>: no token leaves
    /// before. A sign-in from a server without proofs has nothing to check.
    /// </summary>
    async Task Proved(StoredTokens tokens, bool now = false)
    {
        if (tokens.Proof is not { } proof) return;
        lock (proofs)
        {
            // A refresh token, a PIN or a sign-out waits for a proof of its own.
            if (now) provedAt = null;
        }
        // A proof under way counts once it ends, unless the TV changed networks meanwhile: looked
        // at again then, as Android does.
        while (true)
        {
            Task under;
            lock (proofs)
            {
                if (RecentlyProved()) return;
                under = proving ??= Prove(proof, ServerProofs.Network);
            }
            await under.ConfigureAwait(false);
        }
    }

    // A clock set back makes an age below zero: proved again.
    bool RecentlyProved() => provedAt is { } p && p.Network == ServerProofs.Network && clock() - p.At is >= 0 and < ServerProofs.TtlMs;

    async Task Prove(ServerProof proof, int network)
    {
        // Out of the lock: the proof under way is known before its request goes.
        await Task.Yield();
        try
        {
            await ServerProofs.Prove(http, Issuer, proof, random).ConfigureAwait(false);
            lock (proofs) provedAt = (clock(), network);
        }
        finally
        {
            lock (proofs) proving = null;
        }
    }

    /// <summary>Whether the server proves it holds <paramref name="tokens"/>' key now (none: nothing to check).</summary>
    async Task<bool> Proves(StoredTokens tokens)
    {
        if (tokens.Proof is not { } proof) return true;
        try
        {
            await ServerProofs.Prove(http, Issuer, proof, random).ConfigureAwait(false);
            return true;
        }
        catch (NetworkException)
        {
            return false;
        }
    }

    /// <summary>A request to the server got no answer: the next token waits for a new proof (the TV may be on another network).</summary>
    public void DoubtServer()
    {
        lock (proofs) provedAt = null;
    }

    /// <summary>The access token to send, refreshed when it ends within a minute; null when signed out.</summary>
    public async Task<string?> AccessToken()
    {
        var tokens = Load();
        if (tokens == null) return null;
        // A refresh waits for a proof of its own: one before it would be wasted.
        if (tokens.ExpiresAt - clock() <= EarlyMs) return await Refresh(tokens.AccessToken).ConfigureAwait(false);
        await Proved(tokens).ConfigureAwait(false);
        return tokens.AccessToken;
    }

    /// <summary>
    /// A new access token in place of <paramref name="stale"/> (the one the server refused, or that
    /// is about to end). When another caller refreshed meanwhile, its token is used and nothing is sent.
    /// </summary>
    public Task<string> Refresh(string? stale) => Serial(async () =>
    {
        var tokens = Load() ?? throw new SignedOutException();
        if (tokens.AccessToken != stale && tokens.ExpiresAt - clock() > EarlyMs)
        {
            // Refreshed by another caller meanwhile, maybe on another network since.
            await Proved(tokens).ConfigureAwait(false);
            return tokens.AccessToken;
        }
        if (tokens.RefreshToken == null) throw SignedOut();
        // The refresh token goes only to a server that proves itself now.
        await Proved(tokens, now: true).ConfigureAwait(false);
        JsonElement? answer;
        try
        {
            answer = await TokenRequests.PostForm(http, tokens.TokenEndpoint, TokenRequests.Fields(
                ("grant_type", "refresh_token"),
                ("refresh_token", tokens.RefreshToken),
                ("client_id", ClientId))).ConfigureAwait(false);
        }
        catch (TokenException e) when (e.OAuth?.Error == "invalid_grant")
        {
            throw SignedOut();
        }
        var parsed = TokenRequests.ParseTokenResponse(answer, tokens.Scope);
        var fresh = TokenRequests.Stored(parsed, Issuer, tokens.TokenEndpoint, tokens.RevocationEndpoint, clock(), tokens);
        if (TokenRequests.ExtraScopes(parsed.Scope, tokens.Scope).Count > 0) throw await Widened(fresh).ConfigureAwait(false);
        // A refresh token is good once: the new one is kept before anything else.
        Save(fresh);
        lastIssued = (fresh.AccessToken, clock());
        return fresh.AccessToken;
    });

    /// <summary>
    /// The server answered invalid_token to <paramref name="sent"/>. When it is the token a refresh
    /// gave moments ago, another refresh won't do better: the sign-in is over, and this answers
    /// true. False when <paramref name="sent"/> is older.
    /// </summary>
    public Task<bool> RefusedAfterRefresh(string sent) => Serial(() =>
    {
        var last = lastIssued;
        if (last == null || last.Value.Token != sent || clock() - last.Value.At > JustIssuedMs) return Task.FromResult(false);
        if (Load()?.AccessToken != sent) return Task.FromResult(false);
        End(SignOutReason.Ended);
        return Task.FromResult(true);
    });

    /// <summary>
    /// Moves the sign-in to another profile of the household (the switch-profile grant of Pley's
    /// own apps), or checks the PIN of the one it acts as. A wrong PIN changes nothing.
    /// </summary>
    public Task SwitchProfile(long profile, string? pin) => Serial(async () =>
    {
        var tokens = Load() ?? throw new SignedOutException();
        if (tokens.RefreshToken == null) throw SignedOut();
        var fields = TokenRequests.Fields(
            ("grant_type", Metadata.SwitchProfileGrant),
            ("client_id", ClientId),
            ("refresh_token", tokens.RefreshToken),
            ("profile", profile.ToString(System.Globalization.CultureInfo.InvariantCulture)));
        if (pin != null) fields.Add(new KeyValuePair<string, string>("pin", pin));
        await Proved(tokens, now: true).ConfigureAwait(false);
        JsonElement? answer;
        try
        {
            answer = await TokenRequests.PostForm(http, tokens.TokenEndpoint, fields).ConfigureAwait(false);
        }
        // Without a reason the refresh token itself is spent or unknown.
        catch (TokenException e) when (e.OAuth?.Error == "invalid_grant" && e.OAuth.Reason == null)
        {
            throw SignedOut();
        }
        var parsed = TokenRequests.ParseTokenResponse(answer, tokens.Scope);
        var fresh = TokenRequests.Stored(parsed, Issuer, tokens.TokenEndpoint, tokens.RevocationEndpoint, clock(), tokens);
        if (TokenRequests.ExtraScopes(parsed.Scope, tokens.Scope).Count > 0) throw await Widened(fresh).ConfigureAwait(false);
        Save(fresh);
        lastIssued = (fresh.AccessToken, clock());
    });

    /// <summary>
    /// Ends this app's sign-in on the server (RFC 7009), then forgets the tokens whatever it
    /// answered. A server that can't prove itself gets no token: the person can sign the TV out in
    /// Account → Apps.
    /// </summary>
    public Task SignOut() => Serial(async () =>
    {
        var tokens = Load();
        try
        {
            if (tokens != null && await Proves(tokens).ConfigureAwait(false)) await TokenRequests.Revoke(http, ClientId, tokens).ConfigureAwait(false);
        }
        finally
        {
            Wipe();
        }
    });

    /// <summary>Keeps the tokens of a sign-in that just finished, after the scope guard; the sign-in they replace on this server is ended.</summary>
    public Task Install(StoredTokens fresh, string askedScope) => Serial(async () =>
    {
        if (fresh.Issuer != Issuer) throw new InvalidOperationException("tokens of another server");
        if (TokenRequests.ExtraScopes(fresh.Scope, askedScope).Count > 0)
        {
            await TokenRequests.Revoke(http, ClientId, fresh).ConfigureAwait(false);
            throw new AuthException(TokenRequests.WiderScopeMessage);
        }
        var previous = Load();
        Save(fresh);
        // A new sign-in, a new key: the next token waits for its proof.
        lock (proofs) provedAt = null;
        if (previous == null || previous.RefreshToken == fresh.RefreshToken) return;
        // The sign-in before goes only to the server that proves it holds its key.
        if (await Proves(previous).ConfigureAwait(false)) await TokenRequests.Revoke(http, ClientId, previous).ConfigureAwait(false);
    });

    SignedOutException SignedOut()
    {
        End(SignOutReason.Ended);
        return new SignedOutException();
    }

    void End(SignOutReason reason)
    {
        Wipe();
        lastIssued = null;
        onSignedOut(reason);
    }

    /// <summary>A refresh or switch gave more than this sign-in holds: its grant is ended and the app signed out.</summary>
    async Task<AuthException> Widened(StoredTokens fresh)
    {
        await TokenRequests.Revoke(http, ClientId, fresh).ConfigureAwait(false);
        Wipe();
        onSignedOut(SignOutReason.WiderScope);
        return new AuthException(TokenRequests.WiderScopeMessage);
    }
}
