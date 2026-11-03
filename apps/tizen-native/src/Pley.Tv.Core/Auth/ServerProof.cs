using System;
using System.Collections.Generic;
using System.Linq;
using System.Net.NetworkInformation;
using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using System.Text.RegularExpressions;
using System.Threading;
using System.Threading.Tasks;
using Pley.Tv.Core.Net;

namespace Pley.Tv.Core.Auth;

/// <summary>
/// What tells the server from another device at its address later (docs/API.md, "Telling
/// servers apart"): its id, and the key of this sign-in with which it proves it is the one
/// signed in to. The Android app's ServerProof.
/// </summary>
public sealed record ServerProof(string ServerId, string ProofId, string ProofKey)
{
    // Never print the key.
    public override string ToString() => $"ServerProof {{ ServerId = {ServerId}, ProofId = {ProofId} }}";
}

/// <summary>The server's proofs, as the app checks them (the Android app's ServerProofs).</summary>
public static class ServerProofs
{
    /// <summary>How long a server's proof counts, unless a request to it gets no answer or the network changes meanwhile.</summary>
    public const long TtlMs = 30 * 60_000;

    const int ChallengeBytes = 16;
    const int KeyBytes = 32;

    // \z, not $: a proof_id ending in a line break isn't one.
    static readonly Regex ProofId = new Regex(@"\A[1-9][0-9]{0,18}\z", RegexOptions.CultureInvariant);

    static int networks;

    /// <summary>Counts the times the TV may have changed networks: a proof counts on the network it was made on.</summary>
    public static int Network => Volatile.Read(ref networks);

    /// <summary>The TV may be on another network: every server proves itself again before its next token.</summary>
    public static void NetworkMayHaveChanged() => Interlocked.Increment(ref networks);

    /// <summary>Calls <see cref="NetworkMayHaveChanged"/> each time the TV's addresses change or its network comes or goes.</summary>
    public static void FollowNetworks()
    {
        try
        {
            NetworkChange.NetworkAddressChanged += (_, _) => NetworkMayHaveChanged();
            NetworkChange.NetworkAvailabilityChanged += (_, _) => NetworkMayHaveChanged();
        }
        catch (Exception e) when (e is PlatformNotSupportedException or NetworkInformationException)
        {
            // Coming back to the app, and a request that gets no answer, still ask for a new proof.
        }
    }

    /// <summary>The server's id and proof key in a token answer, when it has them and they are written as they should be.</summary>
    public static ServerProof? Of(JsonElement o) => Of(Json.String(o, "server_id"), Json.String(o, "proof_id"), Json.String(o, "proof_key"));

    internal static ServerProof? Of(string? serverId, string? proofId, string? proofKey) =>
        string.IsNullOrEmpty(serverId) || proofId == null || !ProofId.IsMatch(proofId) || proofKey == null || KeyOf(proofKey) == null
            ? null
            : new ServerProof(serverId!, proofId, proofKey);

    /// <summary>The 32 bytes of a proof key written in base64url without padding; null for anything else.</summary>
    internal static byte[]? KeyOf(string key) => FromBase64Url(key) is { Length: KeyBytes } bytes ? bytes : null;

    /// <summary>
    /// The proof of <paramref name="serverId"/>, asked at <paramref name="address"/>, for
    /// <paramref name="challenge"/> under <paramref name="key"/>, in base64url without padding.
    /// </summary>
    public static string Proof(byte[] key, string serverId, string address, string challenge)
    {
        using var hmac = new HMACSHA256(key);
        return Base64Url(hmac.ComputeHash(Encoding.UTF8.GetBytes($"pley-server-proof\n{serverId}\n{address}\n{challenge}")));
    }

    /// <summary><paramref name="n"/> random bytes from the system.</summary>
    public static byte[] RandomBytes(int n)
    {
        var bytes = new byte[n];
        using (var random = RandomNumberGenerator.Create()) random.GetBytes(bytes);
        return bytes;
    }

    /// <summary>
    /// Asks the server at <paramref name="issuer"/> to prove it holds <paramref name="proof"/>'s
    /// key. Throws a <see cref="NetworkException"/> when nothing answers or the server (or a proxy
    /// before it) fails for now, and one of <see cref="NetworkFailure.AnotherDevice"/> when what
    /// answers can't prove it.
    /// </summary>
    public static async Task Prove(Transport http, string issuer, ServerProof proof, Func<int, byte[]> random)
    {
        var key = KeyOf(proof.ProofKey) ?? throw new NetworkException(NetworkFailure.AnotherDevice);
        var challenge = Base64Url(random(ChallengeBytes));
        // The address as the server reads it from the request: the proof covers it.
        var address = Addresses.Origin(issuer) ?? issuer.TrimEnd('/');
        // No redirect is followed: a proof counts only from the address itself.
        var res = await http(new HttpRequest
        {
            Url = $"{address}/api/server?challenge={challenge}&proof_id={proof.ProofId}",
            Headers = new Dictionary<string, string> { ["Accept"] = "application/json" },
            ResponseType = ResponseType.Json,
        }).ConfigureAwait(false);
        if (res.Status >= 500) throw new NetworkException(NetworkFailure.ServerFailed);
        if (res.Status != 200 || res.Json() is not { ValueKind: JsonValueKind.Object } answer) throw new NetworkException(NetworkFailure.AnotherDevice);
        var given = Json.String(answer, "proof");
        if (Json.String(answer, "id") != proof.ServerId || given == null || !Same(given, Proof(key, proof.ServerId, address, challenge)))
        {
            throw new NetworkException(NetworkFailure.AnotherDevice);
        }
    }

    /// <summary>Whether <paramref name="a"/> and <paramref name="b"/> are the same, in a time that doesn't tell where they differ.</summary>
    static bool Same(string a, string b)
    {
        if (a.Length != b.Length) return false;
        var differ = 0;
        for (var i = 0; i < a.Length; i++) differ |= a[i] ^ b[i];
        return differ == 0;
    }

    /// <summary>base64url without padding (RFC 4648 §5).</summary>
    static string Base64Url(byte[] bytes) => Convert.ToBase64String(bytes).TrimEnd('=').Replace('+', '-').Replace('/', '_');

    /// <summary>The bytes of base64url without padding; null for anything else.</summary>
    static byte[]? FromBase64Url(string text)
    {
        if (text.Length % 4 == 1 || !text.All(c => c is >= 'A' and <= 'Z' or >= 'a' and <= 'z' or >= '0' and <= '9' or '-' or '_')) return null;
        var padded = text.Replace('-', '+').Replace('_', '/') + new string('=', (4 - text.Length % 4) % 4);
        try
        {
            return Convert.FromBase64String(padded);
        }
        catch (FormatException)
        {
            return null;
        }
    }
}
