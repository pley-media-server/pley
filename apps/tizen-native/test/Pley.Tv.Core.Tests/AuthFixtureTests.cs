using System;
using System.Collections.Generic;
using System.Linq;
using System.Text.Json;
using System.Text.Json.Nodes;
using Pley.Tv.Core.Auth;
using Xunit;

namespace Pley.Tv.Core.Tests;

/// <summary>
/// Sign-in answers are read as the web TV app reads them (fixtures/auth.json), where the Android
/// app, which this app follows, reads them the same; where it doesn't, the case says so.
/// </summary>
public class AuthFixtureTests
{
    static readonly JsonSerializerOptions Camel = new JsonSerializerOptions { PropertyNamingPolicy = JsonNamingPolicy.CamelCase };

    const string Unreadable = "the server's answer can't be read: try again";

    /// <summary>
    /// The token answers the Android app reads otherwise than the web app: it requires
    /// token_type (case 1 has none), and refuses an expires_in that is there but not above 0
    /// (case 2's -5) rather than reading it as an hour.
    /// </summary>
    static readonly Dictionary<int, string> TokenErrors = new Dictionary<int, string> { [1] = Unreadable, [2] = Unreadable };

    /// <summary>
    /// The codes the Android app reads otherwise: it refuses one without expires_in (case 1) or
    /// with one of 0 (case 2) rather than giving it 10 minutes.
    /// </summary>
    static readonly Dictionary<int, string> DeviceErrors = new Dictionary<int, string> { [1] = Unreadable, [2] = Unreadable };

    public static IEnumerable<object[]> Indexes(string rule) =>
        Enumerable.Range(0, Fixtures.Load("auth").GetProperty(rule).GetArrayLength()).Select(i => new object[] { i });

    public static IEnumerable<object[]> MetadataCases => Indexes("metadata");
    public static IEnumerable<object[]> TokenCases => Indexes("tokenResponse");
    public static IEnumerable<object[]> ErrorCases => Indexes("oauthError");
    public static IEnumerable<object[]> DeviceCases => Indexes("deviceCode");

    static JsonElement Case(string rule, int i) => Fixtures.Load("auth").GetProperty(rule)[i];

    /// <summary>What happened, written as the fixture writes it: { ok: answer } or { error: message }.</summary>
    static JsonNode? Outcome(Func<object?> f)
    {
        try
        {
            return new JsonObject { ["ok"] = JsonSerializer.SerializeToNode(f(), Camel) };
        }
        catch (AuthException e)
        {
            return new JsonObject { ["error"] = e.Message };
        }
    }

    static void Same(JsonElement expected, JsonNode? actual) =>
        Assert.True(JsonNode.DeepEquals(JsonNode.Parse(expected.GetRawText()), actual), $"expected {expected.GetRawText()}\n  actual {actual?.ToJsonString()}");

    /// <summary>
    /// The fixture's metadata as a server today writes it: the Android app requires PKCE's S256
    /// and an authorization_endpoint, which the web app doesn't read, so they are added where
    /// they are missing (a server without them is too old; SignInTests checks that).
    /// </summary>
    static string WithPkce(string text)
    {
        JsonNode? node;
        try
        {
            node = JsonNode.Parse(text);
        }
        catch (JsonException)
        {
            return text;
        }
        if (node is not JsonObject o) return text;
        var issuer = o["issuer"] is JsonValue v && v.TryGetValue<string>(out var s) ? s : "";
        if (!o.ContainsKey("code_challenge_methods_supported")) o["code_challenge_methods_supported"] = new JsonArray("S256");
        if (!o.ContainsKey("authorization_endpoint")) o["authorization_endpoint"] = issuer + "/api/oauth/authorize";
        return o.ToJsonString();
    }

    static ServerMetadata FirstMetadata()
    {
        var first = Case("metadata", 0).GetProperty("in");
        return Auth.Metadata.Parse(WithPkce(first.GetProperty("text").GetString()!), first.GetProperty("server").GetString()!);
    }

    [Theory, MemberData(nameof(MetadataCases))]
    public void Metadata(int i)
    {
        var c = Case("metadata", i);
        var input = c.GetProperty("in");
        var outcome = Outcome(() => Auth.Metadata.Parse(WithPkce(input.GetProperty("text").GetString()!), input.GetProperty("server").GetString()!));
        // The web app has no authorization endpoint to keep.
        (outcome?["ok"] as JsonObject)?.Remove("authorizationEndpoint");
        Same(c.GetProperty("out"), outcome);
    }

    [Theory, MemberData(nameof(TokenCases))]
    public void TokenResponse(int i)
    {
        var c = Case("tokenResponse", i);
        var input = c.GetProperty("in");
        var outcome = Outcome(() => TokenRequests.ParseTokenResponse(input.GetProperty("body"), input.GetProperty("asked").GetString()!));
        if (TokenErrors.TryGetValue(i, out var error)) Assert.Equal(error, outcome?["error"]?.GetValue<string>());
        else Same(c.GetProperty("out"), outcome);
    }

    [Theory, MemberData(nameof(ErrorCases))]
    public void OAuthError(int i)
    {
        var c = Case("oauthError", i);
        Same(c.GetProperty("out"), JsonSerializer.SerializeToNode(TokenRequests.ParseOAuthError(c.GetProperty("in")), Camel));
    }

    [Theory, MemberData(nameof(DeviceCases))]
    public void DeviceCode(int i)
    {
        var c = Case("deviceCode", i);
        var outcome = Outcome(() => DeviceSignIn.ParseDeviceCode(c.GetProperty("in"), "https://pley.example.com", FirstMetadata(), "library play", 1_700_000_000_000));
        if (DeviceErrors.TryGetValue(i, out var error)) Assert.Equal(error, outcome?["error"]?.GetValue<string>());
        else Same(c.GetProperty("out"), outcome);
    }
}
