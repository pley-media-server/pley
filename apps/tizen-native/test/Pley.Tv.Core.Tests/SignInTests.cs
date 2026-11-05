using System;
using System.Collections.Generic;
using System.Linq;
using System.Text;
using System.Text.Json;
using System.Threading;
using System.Threading.Tasks;
using Pley.Tv.Core.Auth;
using Xunit;

namespace Pley.Tv.Core.Tests;

/// <summary>Signing a TV in with a code, and keeping its tokens.</summary>
public class SignInTests
{
    const string Home = "https://pley.example.com";
    const string Token = Home + "/api/oauth/token";
    const string Revoke = Home + "/api/oauth/revoke";
    const string Device = Home + "/api/oauth/device";
    const string ClientId = "https://pley-media-server.github.io/pley/oauth/tizen.json";

    static string Metadata(string home = Home, string extra = @",""code_challenge_methods_supported"":[""S256""]") =>
        $@"{{""issuer"":""{home}"",""authorization_endpoint"":""{home}/api/oauth/authorize"",""token_endpoint"":""{home}/api/oauth/token"",
        ""revocation_endpoint"":""{home}/api/oauth/revoke"",""device_authorization_endpoint"":""{home}/api/oauth/device"",
        ""scopes_supported"":[""library"",""play"",""playlists"",""profiles"",""admin""],
        ""grant_types_supported"":[""refresh_token"",""urn:ietf:params:oauth:grant-type:device_code""]{extra}}}";

    static readonly string MetadataJson = Metadata();

    static string Tokens(string access, string refresh, string scope = "library play playlists profiles") =>
        $@"{{""access_token"":""{access}"",""token_type"":""Bearer"",""expires_in"":3600,""scope"":""{scope}"",""refresh_token"":""{refresh}""}}";

    static DeviceCode Code(FakeClock clock, int interval = 5) =>
        new DeviceCode(Home, Home, "dc", "BCDF-GHJK", Home + "/link", null, clock.Now + 600_000, interval, Token, Revoke, "library play");

    /// <summary>A sign-in for <see cref="Home"/> whose tokens a test can look at.</summary>
    static (DeviceSignIn SignIn, Tokens Tokens, MemoryKeyValue Store) Device_(FakeServer server, FakeClock clock)
    {
        var store = new MemoryKeyValue();
        var tokens = new Tokens(Home, ClientId, store, server.Transport, null, clock.Read);
        return (new DeviceSignIn(server.Transport, ClientId, _ => tokens, clock.Read, clock.Sleep), tokens, store);
    }

    [Fact]
    public async Task Start_asks_for_pleys_own_scopes_that_the_server_offers()
    {
        var server = new FakeServer()
            .On("GET", Home + Auth.Metadata.Path, 200, MetadataJson)
            .On("POST", Device, 200, $@"{{""device_code"":""dc"",""user_code"":""bcdfghjk"",""verification_uri"":""{Home}/link"",""expires_in"":600,""interval"":5}}");
        var code = await Device_(server, new FakeClock()).SignIn.Start(Home + "/");
        Assert.Equal("BCDF-GHJK", code.UserCode);
        Assert.Equal("library play playlists profiles", code.Scope);
        var sent = server.Requests.Last();
        Assert.Contains("scope=library%20play%20playlists%20profiles", sent.Body);
        Assert.Contains("client_id=" + Uri.EscapeDataString(ClientId), sent.Body);
        // Sign-in requests go where they were sent, or nowhere, and wait 20 s for the server.
        Assert.All(server.Requests, r => Assert.Equal((false, 20_000), (r.FollowRedirects, r.TimeoutMs)));
    }

    [Fact]
    public void Asks_for_library_when_the_server_offers_none_of_pleys_scopes()
    {
        var metadata = Auth.Metadata.Parse(Metadata(extra: @",""code_challenge_methods_supported"":[""S256""],""scopes_supported"":[""admin""]").Replace(
            @"""scopes_supported"":[""library"",""play"",""playlists"",""profiles"",""admin""],", ""), Home);
        Assert.Equal("library", DeviceSignIn.ScopeFor(metadata));
    }

    [Fact]
    public void Asks_for_requests_and_approvals_only_where_the_server_lists_them()
    {
        var listed = Auth.Metadata.Parse(Metadata().Replace(@"""profiles"",""admin""", @"""profiles"",""requests"",""approvals"",""admin"""), Home);
        Assert.Equal("library play playlists profiles requests approvals", DeviceSignIn.ScopeFor(listed));
        // An older server refuses a sign-in that asks for a scope it doesn't know.
        Assert.Equal("library play playlists profiles", DeviceSignIn.ScopeFor(Auth.Metadata.Parse(MetadataJson, Home)));
    }

    [Fact]
    public async Task Start_refuses_plain_http_beyond_the_home_network()
    {
        var e = await Assert.ThrowsAsync<AuthException>(() => Device_(new FakeServer(), new FakeClock()).SignIn.Start("http://pley.example.com"));
        Assert.Equal(Addresses.Unencrypted, e.Message);
    }

    [Fact]
    public async Task Start_says_why_the_server_refused_in_its_own_words()
    {
        var server = new FakeServer()
            .On("GET", Home + Auth.Metadata.Path, 200, MetadataJson)
            .On("POST", Device, 401, @"{""error"":""invalid_client""}");
        var e = await Assert.ThrowsAsync<TokenException>(() => Device_(server, new FakeClock()).SignIn.Start(Home));
        Assert.Equal("invalid_client", e.Message);
        server.On("POST", Device, 429, "");
        Assert.Equal("the server refused (HTTP 429)", (await Assert.ThrowsAsync<TokenException>(() => Device_(server, new FakeClock()).SignIn.Start(Home))).Message);
        server.On("POST", Device, 400, @"{""error"":""invalid_scope"",""error_description"":""‮no\u0007 thanks\n""}");
        Assert.Equal("no thanks", (await Assert.ThrowsAsync<TokenException>(() => Device_(server, new FakeClock()).SignIn.Start(Home))).Message);
    }

    [Fact]
    public async Task Reads_only_a_servers_own_up_to_date_metadata()
    {
        async Task<string> Refusal(int status, string? body, Dictionary<string, string>? headers = null) =>
            (await Assert.ThrowsAsync<AuthException>(() => Auth.Metadata.Fetch(new FakeServer().On("GET", Home + Auth.Metadata.Path, status, body, headers).Transport, Home))).Message;
        Assert.Equal("this server answers at another address: open it by that one", await Refusal(301, null, new Dictionary<string, string> { ["Location"] = "https://other.example.com/" }));
        Assert.Equal(Auth.Metadata.TooOld, await Refusal(404, null));
        Assert.Equal(Auth.Metadata.TooOld, await Refusal(200, MetadataJson, new Dictionary<string, string> { ["Content-Type"] = "text/html" }));
        Assert.Equal(Auth.Metadata.TooOld, await Refusal(200, MetadataJson, new Dictionary<string, string> { ["Content-Type"] = "application/problem+json" }));
        Assert.Equal(Auth.Metadata.TooOld, await Refusal(200, Metadata(extra: "")));
        Assert.Equal(Auth.Metadata.TooOld, await Refusal(200, MetadataJson.Replace(@"""authorization_endpoint"":""https://pley.example.com/api/oauth/authorize"",", "")));
        Assert.Equal("the server couldn't answer (HTTP 500)", await Refusal(500, null));
        var issuer = Assert.Throws<AuthException>(() => Auth.Metadata.Parse(Metadata("https://evil.example.com‮"), Home));
        Assert.StartsWith("this server calls itself https://evil.example.com:", issuer.Message);
        var ok = await Auth.Metadata.Fetch(new FakeServer().On("GET", Home + Auth.Metadata.Path, 200, MetadataJson,
            new Dictionary<string, string> { ["Content-Type"] = "application/json; charset=utf-8" }).Transport, Home);
        Assert.Equal(Home + "/api/oauth/authorize", ok.AuthorizationEndpoint);
        Assert.Equal("this server can't sign in with a code yet: update it, or sign in with a password", Auth.Metadata.NoCodes);
    }

    [Fact]
    public void Reads_a_code_as_people_type_it()
    {
        Assert.Equal("BCDF-GHJK", DeviceSignIn.Shown("bcdf ghjk"));
        Assert.Equal("ABC", DeviceSignIn.Shown("ABC"));
        var metadata = Auth.Metadata.Parse(MetadataJson, Home);
        JsonElement Answer(string uri) => JsonDocument.Parse($@"{{""device_code"":""d"",""user_code"":""BCDFGHJK"",""verification_uri"":{JsonSerializer.Serialize(uri)},""expires_in"":600}}").RootElement;
        Assert.Throws<AuthException>(() => DeviceSignIn.ParseDeviceCode(Answer(Home + "/link​"), Home, metadata, "library", 0));
        Assert.Throws<AuthException>(() => DeviceSignIn.ParseDeviceCode(Answer("javascript:alert(1)"), Home, metadata, "library", 0));
        Assert.Equal("BCDF-GHJK", DeviceSignIn.ParseDeviceCode(Answer(Home + "/link"), Home, metadata, "library", 0).UserCode);
    }

    [Fact]
    public async Task Poll_waits_slows_down_when_told_and_keeps_the_tokens_before_saying_signed_in()
    {
        var clock = new FakeClock();
        var answers = new Queue<(int, string?)>(new (int, string?)[]
        {
            (400, @"{""error"":""authorization_pending""}"),
            (400, @"{""error"":""slow_down""}"),
            (200, Tokens("pley_at_1", "pley_rt_1", "library play")),
        });
        var server = new FakeServer().On("POST", Token, _ => answers.Dequeue()).On("POST", Revoke, 200, "{}");
        var (signIn, tokens, _) = Device_(server, clock);
        await tokens.Install(new StoredTokens(Home, "pley_at_0", "pley_rt_0", clock.Now + 3_600_000, "library", Token, Revoke), "library");
        var updates = new List<DevicePoll>();
        var result = await signIn.Poll(Code(clock), updates.Add, CancellationToken.None);
        Assert.Equal(new DevicePoll.SignedIn(Home), result);
        Assert.Equal("pley_at_1", tokens.Load()!.AccessToken);
        // The sign-in it replaces is ended.
        Assert.Contains(server.Requests, r => r.Url == Revoke && r.Body!.Contains("token=pley_rt_0"));
        // Every wait 5 s, and 5 s more after slow_down, for good.
        Assert.Equal(new long[] { 5000, 5000, 10000 }, clock.Sleeps);
        Assert.IsType<DevicePoll.Waiting>(updates.First());
        Assert.IsType<DevicePoll.SignedIn>(updates.Last());
    }

    [Fact]
    public async Task Poll_fails_when_the_tokens_hold_more_than_asked_and_ends_their_grant()
    {
        var clock = new FakeClock();
        var server = new FakeServer()
            .On("POST", Token, 200, Tokens("pley_at_1", "pley_rt_1", "library play admin"))
            .On("POST", Revoke, 200, "{}");
        var (signIn, tokens, _) = Device_(server, clock);
        var result = await signIn.Poll(Code(clock), _ => { }, CancellationToken.None);
        Assert.Equal(new DevicePoll.Failed(TokenRequests.WiderScopeMessage), result);
        Assert.Null(tokens.Load());
        Assert.Contains(server.Requests, r => r.Url == Revoke && r.Body!.Contains("token=pley_rt_1"));

        var unreadable = new FakeServer().On("POST", Token, 200, @"{""access_token"":""a""}");
        Assert.Equal(new DevicePoll.Failed("the server's answer can't be read: try again"),
            await Device_(unreadable, clock).SignIn.Poll(Code(clock), _ => { }, CancellationToken.None));
    }

    [Fact]
    public async Task Poll_backs_off_while_the_server_is_unreachable_and_says_so()
    {
        var clock = new FakeClock();
        var answers = new Queue<(int, string?)>(new (int, string?)[]
        {
            (0, null),
            (0, null),
            (0, null),
            (400, @"{""error"":""authorization_pending""}"),
            (400, @"{""error"":""access_denied""}"),
        });
        var server = new FakeServer().On("POST", Token, _ => answers.Dequeue());
        var updates = new List<DevicePoll>();
        var result = await Device_(server, clock).SignIn.Poll(Code(clock, interval: 1), updates.Add, CancellationToken.None);
        Assert.IsType<DevicePoll.Denied>(result);
        // 1 s, then 2, 4 and 8 s after one, two and three failures, then back to 1 s.
        Assert.Equal(new long[] { 1000, 2000, 4000, 8000, 1000 }, clock.Sleeps);
        Assert.Equal(new long[] { 2000, 4000, 8000 }, updates.OfType<DevicePoll.Unreachable>().Select(u => u.RetryInMs));
        // Back to waiting once the server answers again.
        Assert.IsType<DevicePoll.Waiting>(updates[updates.Count - 2]);
    }

    [Fact]
    public async Task Poll_ends_when_the_code_expires_and_when_stopped()
    {
        var clock = new FakeClock();
        var server = new FakeServer().On("POST", Token, 400, @"{""error"":""authorization_pending""}");
        var code = Code(clock) with { ExpiresAt = clock.Now + 12_000 };
        var result = await Device_(server, clock).SignIn.Poll(code, _ => { }, CancellationToken.None);
        Assert.IsType<DevicePoll.Expired>(result);
        Assert.Equal(new long[] { 5000, 5000, 2000 }, clock.Sleeps);

        using var stop = new CancellationTokenSource();
        stop.Cancel();
        var stopped = await Device_(server, clock).SignIn.Poll(Code(clock), _ => { }, stop.Token);
        Assert.IsType<DevicePoll.Waiting>(stopped);
    }

    [Fact]
    public void Never_prints_a_secret()
    {
        var clock = new FakeClock();
        var printed = string.Join(" ", new object[]
        {
            Code(clock),
            new StoredTokens(Home, "pley_at_secret", "pley_rt_secret", 0, "library", Token, Revoke),
            new TokenResponse("pley_at_secret", "pley_rt_secret", 3600, "library"),
        });
        Assert.DoesNotContain("secret", printed);
        Assert.DoesNotContain("\"dc\"", printed);
        Assert.Contains("BCDF-GHJK", printed);
    }

    static (Tokens Tokens, FakeServer Server, MemoryKeyValue Store, FakeClock Clock, List<SignOutReason> SignedOut) Kept(StoredTokens? saved = null, ITokenVault? vault = null)
    {
        var clock = new FakeClock();
        var server = new FakeServer().On("POST", Revoke, 200, "{}");
        var store = new MemoryKeyValue();
        var signedOut = new List<SignOutReason>();
        var tokens = new Tokens(Home, ClientId, store, server.Transport, signedOut.Add, clock.Read, vault);
        saved ??= new StoredTokens(Home, "pley_at_0", "pley_rt_0", clock.Now + 3_600_000, "library play", Token, Revoke);
        tokens.Install(saved, "library play").Wait();
        return (tokens, server, store, clock, signedOut);
    }

    [Fact]
    public async Task An_access_token_is_used_until_a_minute_before_it_ends_then_refreshed_once()
    {
        var (tokens, server, _, clock, _) = Kept();
        var refreshes = 0;
        server.On("POST", Token, _ =>
        {
            refreshes++;
            return (200, Tokens($"pley_at_{refreshes}", $"pley_rt_{refreshes}", "library play"));
        });
        Assert.Equal("pley_at_0", await tokens.AccessToken());
        clock.Now += 3_600_000 - 30_000;
        // Two callers at once: one refresh, and both get its token.
        var both = await Task.WhenAll(tokens.AccessToken(), tokens.AccessToken());
        Assert.Equal(new[] { "pley_at_1", "pley_at_1" }, both);
        Assert.Equal(1, refreshes);
        Assert.Equal("pley_rt_1", tokens.Load()!.RefreshToken);
    }

    [Fact]
    public async Task A_spent_refresh_token_signs_the_TV_out()
    {
        var (tokens, server, store, _, signedOut) = Kept();
        server.On("POST", Token, 400, @"{""error"":""invalid_grant""}");
        await Assert.ThrowsAsync<SignedOutException>(() => tokens.Refresh("pley_at_0"));
        Assert.Equal(new[] { SignOutReason.Ended }, signedOut);
        Assert.Empty(store.Values);
    }

    [Fact]
    public async Task A_refresh_that_widens_the_scope_ends_the_grant()
    {
        var (tokens, server, store, _, signedOut) = Kept();
        server.On("POST", Token, 200, Tokens("pley_at_1", "pley_rt_1", "library play admin"));
        var e = await Assert.ThrowsAsync<AuthException>(() => tokens.Refresh("pley_at_0"));
        Assert.Equal(TokenRequests.WiderScopeMessage, e.Message);
        Assert.Equal(new[] { SignOutReason.WiderScope }, signedOut);
        Assert.Empty(store.Values);
        Assert.Contains(server.Requests, r => r.Url == Revoke && r.Body!.Contains("token=pley_rt_1"));
    }

    [Fact]
    public async Task A_new_sign_in_ends_the_one_it_replaces()
    {
        var (tokens, server, _, clock, _) = Kept();
        await tokens.Install(new StoredTokens(Home, "pley_at_9", "pley_rt_9", clock.Now + 3_600_000, "library", Token, Revoke), "library play");
        Assert.Contains(server.Requests, r => r.Url == Revoke && r.Body!.Contains("token=pley_rt_0"));
        Assert.Equal("pley_at_9", tokens.Load()!.AccessToken);
    }

    [Fact]
    public async Task A_token_refused_right_after_its_refresh_ends_the_sign_in()
    {
        var (tokens, server, _, clock, signedOut) = Kept();
        server.On("POST", Token, 200, Tokens("pley_at_1", "pley_rt_1", "library play"));
        Assert.False(await tokens.RefusedAfterRefresh("pley_at_0"));
        var fresh = await tokens.Refresh("pley_at_0");
        clock.Now += 10_000;
        Assert.True(await tokens.RefusedAfterRefresh(fresh));
        Assert.Equal(new[] { SignOutReason.Ended }, signedOut);
    }

    [Fact]
    public async Task A_wrong_PIN_changes_nothing()
    {
        var (tokens, server, _, _, signedOut) = Kept();
        server.On("POST", Token, 400, @"{""error"":""invalid_grant"",""error_description"":""That PIN isn't right"",""reason"":""wrong_pin""}");
        var e = await Assert.ThrowsAsync<TokenException>(() => tokens.SwitchProfile(2, "0000"));
        Assert.Equal("That PIN isn't right", e.Message);
        Assert.Empty(signedOut);
        Assert.Equal("pley_at_0", tokens.Load()!.AccessToken);
    }

    [Fact]
    public async Task Every_token_request_refuses_plain_http_beyond_the_home_network()
    {
        // Tokens kept for a server whose endpoints have moved off the home network since.
        const string Far = "http://pley.example.com";
        var (tokens, server, _, _, _) = Kept(new StoredTokens(Home, "pley_at_0", "pley_rt_0", 0, "library play", Far + "/t", Far + "/r"));
        var e = await Assert.ThrowsAsync<AuthException>(() => tokens.Refresh("pley_at_0"));
        Assert.Equal(Addresses.Unencrypted, e.Message);
        await Assert.ThrowsAsync<AuthException>(() => tokens.SwitchProfile(2, null));
        await tokens.SignOut();
        Assert.DoesNotContain(server.Requests, r => r.Url.StartsWith(Far));
    }

    /// <summary>A vault that reverses the bytes and binds them to their server, as sealing would.</summary>
    sealed class FakeVault : ITokenVault
    {
        public bool Broken { get; set; }

        public byte[] Seal(byte[] plain, string aad) => Encoding.UTF8.GetBytes(aad + "|").Concat(plain.Reverse()).ToArray();

        public byte[]? Open(byte[] sealedValue, string aad)
        {
            if (Broken) throw new InvalidOperationException("the key went");
            var prefix = Encoding.UTF8.GetBytes(aad + "|");
            return sealedValue.Take(prefix.Length).SequenceEqual(prefix) ? sealedValue.Skip(prefix.Length).Reverse().ToArray() : null;
        }
    }

    [Fact]
    public void Keeps_the_tokens_sealed_by_the_vault()
    {
        var vault = new FakeVault();
        var (tokens, _, store, _, signedOut) = Kept(vault: vault);
        Assert.DoesNotContain("pley_rt_0", Encoding.UTF8.GetString(Convert.FromBase64String(store.Values.Values.Single())));
        Assert.Equal("pley_rt_0", tokens.Load()!.RefreshToken);
        // Another server's tokens can't be passed off as these.
        var other = new Tokens("https://other.example.com", ClientId, store, new FakeServer().Transport, null, null, vault);
        store.Set("pley.tokens.https://other.example.com", store.Values["pley.tokens." + Home]);
        Assert.Null(other.Load());
        Assert.Empty(signedOut);
    }

    [Theory]
    [InlineData("not base64!")]
    [InlineData("e30=")]
    [InlineData("eyJpc3N1ZXIiOiJodHRwczovL3BsZXkuZXhhbXBsZS5jb20iLCJhY2Nlc3NUb2tlbiI6ImEifQ==")]
    public void Unreadable_tokens_are_removed_and_sign_the_TV_out(string kept)
    {
        var (tokens, _, store, _, signedOut) = Kept();
        store.Set("pley.tokens." + Home, kept);
        Assert.True(tokens.Kept);
        Assert.Null(tokens.Load());
        Assert.Equal(new[] { SignOutReason.Unreadable }, signedOut);
        Assert.Empty(store.Values);
        Assert.False(tokens.Kept);
    }

    [Fact]
    public void Tokens_a_vault_cant_open_anymore_sign_the_TV_out()
    {
        var vault = new FakeVault();
        var (tokens, _, store, _, signedOut) = Kept(vault: vault);
        vault.Broken = true;
        // Read again, as at the next launch.
        var again = new Tokens(Home, ClientId, store, new FakeServer().Transport, signedOut.Add, null, vault);
        Assert.Null(again.Load());
        Assert.Equal(new[] { SignOutReason.Unreadable }, signedOut);
    }
}
