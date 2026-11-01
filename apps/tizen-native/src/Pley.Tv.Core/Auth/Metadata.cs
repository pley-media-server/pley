using System;
using System.Collections.Generic;
using System.Linq;
using System.Text.Json;
using System.Text.Json.Serialization;
using System.Threading.Tasks;
using Pley.Tv.Core.Net;

namespace Pley.Tv.Core.Auth;

/// <summary>A sign-in problem, in a few words for people.</summary>
public class AuthException : Exception
{
    public AuthException(string message, Exception? inner = null) : base(message, inner)
    {
    }
}

/// <summary>The server didn't answer at all: worth trying again by itself.</summary>
public class UnreachableException : AuthException
{
    public UnreachableException(NetworkException? cause = null)
        : base("the server can't be reached: check its address and that it's on", cause)
    {
    }
}

/// <summary>
/// Plain http is for home networks: Pley's apps send codes and tokens over it only to
/// addresses at home, as browsers on the server's own network do with passwords. Anything
/// else needs https (the Android app's <c>Transport.checkAddress</c>).
/// </summary>
public static class PlainHttp
{
    /// <summary>
    /// Refuses <paramref name="url"/> when it is plain http to a host that isn't on the home
    /// network (<see cref="Addresses.IsLocalHost"/>). Every sign-in request is checked, not only
    /// the first. The Android app checks the address it connected to as well (a name that turns
    /// out to be on the internet); HttpClient doesn't tell which address that was.
    /// </summary>
    public static void Check(string url)
    {
        if (Addresses.Origin(url) == null) throw new AuthException("that isn't a server address");
        if (Addresses.IsUnencrypted(url)) throw new AuthException(Addresses.Unencrypted);
    }
}

/// <summary>
/// Where a server signs apps in (RFC 8414), read from
/// <c>&lt;server&gt;/.well-known/oauth-authorization-server</c>. Only kept when it is this
/// server's own: its issuer is the address the app used and every endpoint is on it, so an
/// answer can't send the app's codes or tokens anywhere else.
/// </summary>
public sealed record ServerMetadata(
    string Issuer,
    string AuthorizationEndpoint,
    string TokenEndpoint,
    string RevocationEndpoint,
    string? DeviceAuthorizationEndpoint,
    IReadOnlyList<string>? ScopesSupported,
    IReadOnlyList<string>? GrantTypesSupported)
{
    /// <summary>Whether a TV can sign in at this server with a code.</summary>
    [JsonIgnore]
    public bool SignsInWithCodes => DeviceAuthorizationEndpoint != null && (GrantTypesSupported ?? new string[0]).Contains(Metadata.DeviceCodeGrant);
}

/// <summary>The Android app's <c>ServerMetadata</c>.</summary>
public static class Metadata
{
    public const string TooOld = "this server is too old for this app";

    /// <summary>RFC 8628's grant: a TV collects its tokens with the code it was given.</summary>
    public const string DeviceCodeGrant = "urn:ietf:params:oauth:grant-type:device_code";

    /// <summary>Pley's own apps move between the household's profiles with this grant.</summary>
    public const string SwitchProfileGrant = "urn:pley:params:oauth:grant-type:switch-profile";

    /// <summary>What a server without sign-in with a code answers (the Android app's words).</summary>
    public const string NoCodes = "this server can't sign in with a code yet: update it, or sign in with a password";

    /// <summary>A server that redirects its metadata elsewhere.</summary>
    public const string Moved = "this server answers at another address: open it by that one";

    public const string Path = "/.well-known/oauth-authorization-server";

    /// <summary>How long the server may stay silent on a sign-in request, as the Android app allows.</summary>
    public const int ReadTimeoutMs = 20_000;

    /// <summary>
    /// Reads the metadata of the server at <paramref name="server"/>. No redirect is followed:
    /// the issuer must be the address the app used, so another address can't be the answer.
    /// </summary>
    public static async Task<ServerMetadata> Fetch(Transport http, string server)
    {
        var home = Addresses.Origin(server) ?? throw new AuthException("that isn't a server address");
        PlainHttp.Check(home);
        HttpResponse res;
        try
        {
            res = await http(new HttpRequest
            {
                Url = home + Path,
                Headers = new Dictionary<string, string> { ["Accept"] = "application/json" },
                TimeoutMs = ReadTimeoutMs,
            }).ConfigureAwait(false);
        }
        catch (NetworkException e)
        {
            throw new UnreachableException(e);
        }
        if (res.Status is 300 or 301 or 302 or 303 or 307 or 308) throw new AuthException(Moved);
        if (res.Status == 404) throw new AuthException(TooOld);
        if (res.Status < 200 || res.Status >= 300) throw new AuthException($"the server couldn't answer (HTTP {res.Status})");
        // Older servers answer their web app's page here.
        if (Subtype(res.Header("Content-Type")) != "json") throw new AuthException(TooOld);
        return Parse(res.Text ?? "", home);
    }

    /// <summary>The subtype of a Content-Type, in lower case: "json" for <c>application/json; charset=utf-8</c>.</summary>
    static string? Subtype(string? contentType)
    {
        var type = (contentType ?? "").Split(';')[0].Trim();
        var slash = type.IndexOf('/');
        return slash < 0 ? null : type.Substring(slash + 1).Trim().ToLowerInvariant();
    }

    /// <summary>The checks of <see cref="Fetch"/>, on the answer <paramref name="text"/> of the server at <paramref name="server"/>.</summary>
    public static ServerMetadata Parse(string text, string server)
    {
        var home = Addresses.Origin(server) ?? throw new AuthException("that isn't a server address");
        JsonElement o;
        try
        {
            using var doc = JsonDocument.Parse(text);
            o = doc.RootElement.Clone();
        }
        catch (JsonException)
        {
            throw new AuthException(TooOld);
        }
        if (o.ValueKind != JsonValueKind.Object) throw new AuthException(TooOld);
        var issuer = Json.String(o, "issuer") ?? "";
        if (issuer.Length == 0) throw new AuthException(TooOld);
        // RFC 8414 §3.3: the issuer is exactly the address asked at.
        if (issuer != home)
        {
            var named = Plain.Of(issuer);
            throw new AuthException(
                $"this server calls itself {(named.Length > 100 ? named.Substring(0, 100) : named)}: open it by that address, or check the proxy in front of it passes Host");
        }
        // A server without PKCE's S256 predates Pley's sign-in for apps.
        if (!(Json.Strings(o, "code_challenge_methods_supported") ?? new string[0]).Contains("S256")) throw new AuthException(TooOld);
        string Endpoint(string name)
        {
            var url = Json.String(o, name);
            if (string.IsNullOrEmpty(url)) throw new AuthException(TooOld);
            if (Addresses.Origin(url!) != issuer || !url!.StartsWith(issuer + "/", StringComparison.Ordinal))
                throw new AuthException("this server sends apps to another address to sign in: Pley doesn't");
            return url;
        }
        var authorization = Endpoint("authorization_endpoint");
        var token = Endpoint("token_endpoint");
        var revocation = Endpoint("revocation_endpoint");
        // Optional, but on this server like the others.
        var device = string.IsNullOrEmpty(Json.String(o, "device_authorization_endpoint")) ? null : Endpoint("device_authorization_endpoint");
        return new ServerMetadata(
            issuer,
            authorization,
            token,
            revocation,
            device,
            Json.Strings(o, "scopes_supported"),
            Json.Strings(o, "grant_types_supported"));
    }
}
