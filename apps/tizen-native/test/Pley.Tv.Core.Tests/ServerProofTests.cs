using System;
using System.Collections.Generic;
using System.Linq;
using System.Text;
using System.Text.Json;
using System.Threading.Tasks;
using Pley.Tv.Core.Api;
using Pley.Tv.Core.Auth;
using Pley.Tv.Core.Net;
using Xunit;

namespace Pley.Tv.Core.Tests;

/// <summary>
/// No token leaves before the server at the address proves it is the one signed in to
/// (docs/API.md, "Telling servers apart"), as in the web TV app's server-proof tests and
/// Android's ServerProofTest.
/// </summary>
public class ServerProofTests
{
    const string Server = "http://192.168.1.20:7539";
    const string Id = "0123456789abcdef0123456789abcdef01234567";
    const string Key = "AQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQE";
    const string OtherKey = "CQkJCQkJCQkJCQkJCQkJCQkJCQkJCQkJCQkJCQkJCQk";
    static readonly ServerProof Proof = new ServerProof(Id, "17", Key);
    static readonly byte[] Challenge = Enumerable.Range(0, 16).Select(i => (byte)i).ToArray();

    static string ProofOf(string challenge) => ServerProofs.Proof(ServerProofs.KeyOf(Key)!, Id, Server, challenge);

    static HttpResponse Json(int status, object body) =>
        new HttpResponse(status, name => name.Equals("Content-Type", StringComparison.OrdinalIgnoreCase) ? "application/json" : null, JsonSerializer.Serialize(body));

    static HttpResponse Text(int status, string body) => new HttpResponse(status, _ => null, body);

    /// <summary>A TV signed in to <see cref="Server"/>, whose server answers proofs as <see cref="Answer"/> says and counts what it was asked.</summary>
    sealed class Rig
    {
        readonly List<HttpRequest> asked = new List<HttpRequest>();
        public readonly MemoryKeyValue Store = new MemoryKeyValue();
        public long Now = 1_000_000;
        /// <summary>The answer to a proof's challenge; null: nothing answers.</summary>
        public Func<string, HttpResponse?> Answer = c => Json(200, new { id = Id, name = "Den", proof = ProofOf(c) });
        public Func<HttpResponse> TokenAnswer = () => Json(200, new { access_token = "pley_at_new", token_type = "Bearer", expires_in = 3600, refresh_token = "pley_rt_new" });
        /// <summary>The API's other requests get no answer.</summary>
        public bool ApiDown;
        public readonly Tokens Tokens;

        public Rig(bool withProof = true, long expiresIn = 3_600_000)
        {
            Keep(new StoredTokens(Server, "pley_at_a", "pley_rt_b", Now + expiresIn, "library play", Server + "/api/oauth/token", Server + "/api/oauth/revoke",
                withProof ? Proof : null));
            Tokens = NewTokens();
        }

        public Tokens NewTokens() => new Tokens(Server, "app", Store, Transport, null, () => Now, null, n => Challenge.Take(n).ToArray());

        /// <summary>Keeps <paramref name="t"/> as the tokens keep them (the plain vault's base64 of their JSON).</summary>
        void Keep(StoredTokens t)
        {
            var fields = new Dictionary<string, object?>
            {
                ["issuer"] = t.Issuer,
                ["accessToken"] = t.AccessToken,
                ["refreshToken"] = t.RefreshToken,
                ["expiresAt"] = t.ExpiresAt,
                ["scope"] = t.Scope,
                ["tokenEndpoint"] = t.TokenEndpoint,
                ["revocationEndpoint"] = t.RevocationEndpoint,
            };
            if (t.Proof != null)
            {
                fields["serverId"] = t.Proof.ServerId;
                fields["proofId"] = t.Proof.ProofId;
                fields["proofKey"] = t.Proof.ProofKey;
            }
            Store.Set($"pley.tokens.{Server}", Convert.ToBase64String(JsonSerializer.SerializeToUtf8Bytes(fields)));
        }

        public IReadOnlyList<HttpRequest> Asked
        {
            get
            {
                lock (asked) return asked.ToList();
            }
        }

        public int Proofs => Asked.Count(r => r.Url.StartsWith(Server + "/api/server?", StringComparison.Ordinal));

        public IReadOnlyList<string> Paths => Asked.Select(r => new Uri(r.Url).AbsolutePath).ToList();

        public Task<HttpResponse> Transport(HttpRequest req)
        {
            lock (asked) asked.Add(req);
            var url = new Uri(req.Url);
            switch (url.AbsolutePath)
            {
                case "/api/server":
                    var challenge = url.Query.TrimStart('?').Split('&').Select(p => p.Split('=')).First(p => p[0] == "challenge")[1];
                    return Task.FromResult(Answer(challenge) ?? throw new NetworkException(NetworkFailure.NothingAnswers));
                case "/api/oauth/token":
                    return Task.FromResult(TokenAnswer());
                case "/api/oauth/revoke":
                    return Task.FromResult(Text(200, ""));
                default:
                    if (ApiDown) throw new NetworkException(NetworkFailure.NothingAnswers);
                    return Task.FromResult(Json(200, new { }));
            }
        }
    }

    [Fact]
    public async Task The_server_proves_itself_once_before_the_token_goes()
    {
        var rig = new Rig();
        var both = await Task.WhenAll(rig.Tokens.AccessToken(), rig.Tokens.AccessToken());
        Assert.Equal(new[] { "pley_at_a", "pley_at_a" }, both);
        var asked = Assert.Single(rig.Asked);
        Assert.Equal(Server + "/api/server?challenge=AAECAwQFBgcICQoLDA0ODw&proof_id=17", asked.Url);
        // Taken from the address itself, and without the token.
        Assert.False(asked.FollowRedirects);
        Assert.False(asked.Headers?.ContainsKey("Authorization") ?? false);
    }

    [Fact]
    public async Task It_proves_itself_again_after_half_an_hour_or_a_request_without_an_answer()
    {
        var rig = new Rig();
        await rig.Tokens.AccessToken();
        rig.Now += ServerProofs.TtlMs - 1;
        await rig.Tokens.AccessToken();
        Assert.Equal(1, rig.Proofs);
        rig.Now += 1;
        await rig.Tokens.AccessToken();
        Assert.Equal(2, rig.Proofs);
        rig.Tokens.DoubtServer();
        await rig.Tokens.AccessToken();
        Assert.Equal(3, rig.Proofs);
    }

    [Fact]
    public async Task It_proves_itself_again_on_another_network_or_after_the_clock_went_back()
    {
        var rig = new Rig();
        await rig.Tokens.AccessToken();
        ServerProofs.NetworkMayHaveChanged();
        await rig.Tokens.AccessToken();
        Assert.Equal(2, rig.Proofs);
        rig.Now -= 60_000;
        await rig.Tokens.AccessToken();
        Assert.Equal(3, rig.Proofs);
    }

    [Fact]
    public async Task It_proves_itself_again_when_the_network_changed_while_a_proof_was_under_way()
    {
        var rig = new Rig();
        var changed = false;
        rig.Answer = c =>
        {
            // The TV joins another network as the first proof is answered.
            if (!changed) ServerProofs.NetworkMayHaveChanged();
            changed = true;
            return Json(200, new { id = Id, name = "Den", proof = ProofOf(c) });
        };
        Assert.Equal("pley_at_a", await rig.Tokens.AccessToken());
        Assert.Equal(2, rig.Proofs);
    }

    public static IEnumerable<object[]> OtherDevices => new[]
    {
        new object[] { 200, $@"{{""id"":""{Id}"",""proof"":""AAAA""}}" },
        new object[] { 200, $@"{{""id"":""another"",""proof"":""{ProofOf("AAECAwQFBgcICQoLDA0ODw")}""}}" },
        new object[] { 200, $@"{{""id"":""{Id}"",""proof"":null}}" },
        new object[] { 200, $@"{{""id"":""{Id}""}}" },
        // A redirect, which could lead to the real server from anywhere, proves nothing.
        new object[] { 302, "" },
        new object[] { 403, @"{""message"":""a proof for a plain-http address is made for the home network only""}" },
        new object[] { 404, "Not found" },
        new object[] { 200, "<html>a router</html>" },
    };

    [Theory, MemberData(nameof(OtherDevices))]
    public async Task Another_device_at_the_address_gets_no_token(int status, string body)
    {
        var rig = new Rig { Answer = _ => Text(status, body) };
        var e = await Assert.ThrowsAsync<NetworkException>(() => rig.Tokens.AccessToken());
        Assert.Equal(NetworkFailure.AnotherDevice, e.Failure);
        // Out of reach, as the TV shows it.
        Assert.True(Errors.Unreachable(e));
        Assert.Equal("Another device answers there.", Errors.UnreachableReason(e));
        e = await Assert.ThrowsAsync<NetworkException>(() => rig.Tokens.Refresh("pley_at_a"));
        Assert.Equal(NetworkFailure.AnotherDevice, e.Failure);
        Assert.All(rig.Paths, p => Assert.Equal("/api/server", p));
    }

    [Fact]
    public async Task A_server_error_is_out_of_reach_not_another_device()
    {
        var rig = new Rig { Answer = _ => Text(502, "Bad Gateway") };
        var e = await Assert.ThrowsAsync<NetworkException>(() => rig.Tokens.AccessToken());
        Assert.Equal(NetworkFailure.ServerFailed, e.Failure);
    }

    [Fact]
    public async Task Nothing_answering_is_out_of_reach_and_tried_again()
    {
        var up = false;
        var rig = new Rig();
        var server = rig.Answer;
        rig.Answer = c => up ? server(c) : null;
        var e = await Assert.ThrowsAsync<NetworkException>(() => rig.Tokens.AccessToken());
        Assert.Equal(NetworkFailure.NothingAnswers, e.Failure);
        up = true;
        Assert.Equal("pley_at_a", await rig.Tokens.AccessToken());
    }

    [Fact]
    public async Task A_refresh_waits_for_a_proof_made_then_and_keeps_the_key()
    {
        var rig = new Rig();
        await rig.Tokens.AccessToken();
        // A refresh's answer bringing another key doesn't replace the sign-in's.
        rig.TokenAnswer = () => Json(200, new
        {
            access_token = "pley_at_new",
            token_type = "Bearer",
            expires_in = 3600,
            refresh_token = "pley_rt_new",
            server_id = Id,
            proof_id = "18",
            proof_key = OtherKey,
        });
        Assert.Equal("pley_at_new", await rig.Tokens.Refresh("pley_at_a"));
        Assert.Equal(new[] { "/api/server", "/api/server", "/api/oauth/token" }, rig.Paths);
        Assert.Equal(Proof, rig.NewTokens().Load()!.Proof);
    }

    [Fact]
    public async Task A_token_about_to_end_waits_for_one_proof_its_refreshs()
    {
        var rig = new Rig(expiresIn: 30_000);
        Assert.Equal("pley_at_new", await rig.Tokens.AccessToken());
        Assert.Equal(new[] { "/api/server", "/api/oauth/token" }, rig.Paths);
    }

    [Fact]
    public async Task A_token_another_refresh_gave_goes_out_only_after_a_proof()
    {
        var rig = new Rig();
        Assert.Equal("pley_at_a", await rig.Tokens.Refresh("pley_at_old"));
        Assert.Equal(new[] { "/api/server" }, rig.Paths);
    }

    [Fact]
    public async Task A_profile_switch_and_its_pin_wait_for_a_proof_made_then()
    {
        var rig = new Rig();
        await rig.Tokens.AccessToken();
        await rig.Tokens.SwitchProfile(2, "1234");
        Assert.Equal(new[] { "/api/server", "/api/server", "/api/oauth/token" }, rig.Paths);
        var away = new Rig { Answer = _ => Text(200, "{}") };
        await Assert.ThrowsAsync<NetworkException>(() => away.Tokens.SwitchProfile(2, "1234"));
        Assert.All(away.Asked, r => Assert.Null(r.Body));
    }

    [Fact]
    public async Task Signing_out_sends_another_device_nothing()
    {
        var away = new Rig { Answer = _ => Json(200, new { id = Id, proof = "AAAA" }) };
        await away.Tokens.SignOut();
        Assert.Equal(new[] { "/api/server" }, away.Paths);
        Assert.False(away.Tokens.SignedIn);
        // At its own server, the sign-in ends there.
        var home = new Rig();
        await home.Tokens.SignOut();
        Assert.Equal(new[] { "/api/server", "/api/oauth/revoke" }, home.Paths);
        Assert.Contains("pley_rt_b", home.Asked.Last().Body);
    }

    [Fact]
    public async Task A_new_sign_in_ends_the_one_before_only_where_the_server_proves_its_key()
    {
        var fresh = new StoredTokens(Server, "pley_at_c", "pley_rt_d", 9_000_000, "library play", Server + "/api/oauth/token", Server + "/api/oauth/revoke",
            new ServerProof(Id, "18", OtherKey));
        // Another device that got the TV to sign in again doesn't receive the old refresh token.
        var away = new Rig { Answer = _ => Json(200, new { id = Id, proof = "AAAA" }) };
        await away.Tokens.Install(fresh, "library play");
        Assert.Equal(new[] { "/api/server" }, away.Paths);
        Assert.Equal("pley_at_c", away.Tokens.Load()!.AccessToken);
        var home = new Rig();
        await home.Tokens.Install(fresh, "library play");
        Assert.Equal(new[] { "/api/server", "/api/oauth/revoke" }, home.Paths);
        Assert.Contains("pley_rt_b", home.Asked.Last().Body);
    }

    [Fact]
    public async Task A_sign_in_from_a_server_without_proofs_checks_nothing()
    {
        var rig = new Rig(withProof: false);
        Assert.Equal("pley_at_a", await rig.Tokens.AccessToken());
        await rig.Tokens.SignOut();
        Assert.Equal(new[] { "/api/oauth/revoke" }, rig.Paths);
    }

    [Fact]
    public async Task A_request_without_an_answer_has_the_server_prove_itself_again()
    {
        var rig = new Rig { ApiDown = true };
        var api = new Api.Api(Server, rig.Tokens, rig.Transport);
        await Assert.ThrowsAsync<NetworkException>(() => api.Send("/api/libraries"));
        Assert.Equal(1, rig.Proofs);
        rig.ApiDown = false;
        await api.Send("/api/libraries");
        Assert.Equal(2, rig.Proofs);
    }

    [Fact]
    public void The_key_is_kept_with_the_tokens_and_never_printed()
    {
        Assert.Equal(Proof, new Rig().Tokens.Load()!.Proof);
        Assert.DoesNotContain(Key, Proof.ToString());
        Assert.DoesNotContain(Key, new TokenResponse("a", "r", 60, "library", Proof).ToString());
    }
}
