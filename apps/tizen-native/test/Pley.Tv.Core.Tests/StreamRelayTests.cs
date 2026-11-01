using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Net;
using System.Net.Http;
using System.Text;
using System.Threading.Tasks;
using Pley.Tv.Core.Api;
using Pley.Tv.Core.Auth;
using Pley.Tv.Core.Net;
using Xunit;

namespace Pley.Tv.Core.Tests;

/// <summary>A channel's stream.ts handed on to the TV's player, which can't send the token.</summary>
public class StreamRelayTests
{
    const string Home = "https://pley.example.com";

    static StreamResponse Answer(byte[] bytes) => new StreamResponse(200, _ => null, new MemoryStream(bytes));

    static readonly byte[] Bytes = Enumerable.Range(0, 300_000).Select(i => (byte)i).ToArray();

    [Fact]
    public async Task A_player_gets_the_stream_on_loopback_as_it_comes_under_its_random_path_only()
    {
        using var relay = new StreamRelay();
        var url = relay.Serve(_ => Task.FromResult(Answer(Bytes)));
        Assert.Matches(@"^http://127\.0\.0\.1:\d+/[0-9a-f]{32}/stream\.ts$", url);
        using var http = new HttpClient();
        var res = await http.GetAsync(url);
        Assert.Equal(HttpStatusCode.OK, res.StatusCode);
        Assert.Equal("video/mp2t", res.Content.Headers.ContentType?.MediaType);
        Assert.Equal(Bytes, await res.Content.ReadAsByteArrayAsync());
        Assert.Equal(HttpStatusCode.NotFound, (await http.GetAsync(new Uri(new Uri(url), "/00000000000000000000000000000000/stream.ts"))).StatusCode);
    }

    [Fact]
    public async Task The_stream_is_opened_as_it_is_served_for_the_first_player_and_anew_for_the_next()
    {
        using var relay = new StreamRelay();
        var opens = 0;
        var url = relay.Serve(_ =>
        {
            opens++;
            return Task.FromResult(Answer(new byte[] { 7 }));
        });
        // Before any player asks: the server answers while the player starts.
        Assert.Equal(1, opens);
        using var http = new HttpClient();
        Assert.Equal(new byte[] { 7 }, await http.GetByteArrayAsync(url));
        Assert.Equal(1, opens);
        Assert.Equal(new byte[] { 7 }, await http.GetByteArrayAsync(url));
        Assert.Equal(2, opens);
    }

    [Fact]
    public async Task Serving_another_or_stopping_ends_the_address_before()
    {
        using var relay = new StreamRelay();
        using var http = new HttpClient();
        var first = relay.Serve(_ => Task.FromResult(Answer(Bytes)));
        var second = relay.Serve(_ => Task.FromResult(Answer(new byte[] { 1, 2, 3 })));
        Assert.NotEqual(first, second);
        Assert.Equal(HttpStatusCode.NotFound, (await http.GetAsync(first)).StatusCode);
        Assert.Equal(new byte[] { 1, 2, 3 }, await http.GetByteArrayAsync(second));
        relay.Stop();
        Assert.Equal(HttpStatusCode.NotFound, (await http.GetAsync(second)).StatusCode);
    }

    [Fact]
    public async Task A_stream_the_server_refuses_is_a_bad_gateway_to_the_player()
    {
        using var relay = new StreamRelay();
        var url = relay.Serve(_ => Task.FromException<StreamResponse>(new ApiException(410, "gone", "this channel isn't being watched any more")));
        using var http = new HttpClient();
        Assert.Equal(HttpStatusCode.BadGateway, (await http.GetAsync(url)).StatusCode);
    }

    static (Api.Api Api, List<HttpRequest> Opened) SignedIn(Func<HttpRequest, StreamResponse> answer)
    {
        var clock = new FakeClock();
        var fake = new FakeServer();
        var tokens = new Tokens(Home, "client", new MemoryKeyValue(), fake.Transport, null, clock.Read);
        tokens.Install(new StoredTokens(Home, "pley_at_0", "pley_rt_0", clock.Now + 3_600_000, "library play", Home + "/api/oauth/token", Home + "/api/oauth/revoke"), "library play").Wait();
        var opened = new List<HttpRequest>();
        StreamTransport streams = req =>
        {
            opened.Add(req);
            return Task.FromResult(answer(req));
        };
        return (new Api.Api(Home, tokens, fake.Transport, streams), opened);
    }

    [Fact]
    public async Task The_app_reads_the_stream_with_the_token_in_its_header()
    {
        var (api, opened) = SignedIn(_ => Answer(Bytes));
        using var res = await api.Open("/api/live/channels/7/stream.ts?client_id=tizen-1-stream");
        var req = Assert.Single(opened);
        Assert.Equal(Home + "/api/live/channels/7/stream.ts?client_id=tizen-1-stream", req.Url);
        Assert.Equal("Bearer pley_at_0", req.Headers!["Authorization"]);
        Assert.Equal(200, res.Status);
    }

    [Fact]
    public async Task A_refused_stream_is_the_servers_sentence()
    {
        var body = Encoding.UTF8.GetBytes(@"{""code"":""conflict"",""message"":""every connection is in use""}");
        var (api, _) = SignedIn(_ => new StreamResponse(409, h => h == "Content-Type" ? "application/json" : null, new MemoryStream(body)));
        var e = await Assert.ThrowsAsync<ApiException>(() => api.Open("/api/live/channels/7/stream.ts"));
        Assert.Equal(409, e.Status);
        Assert.Equal("every connection is in use", e.Message);
    }

    [Theory]
    [InlineData("copy", "copy", true)]
    [InlineData("copy", "none", true)]
    [InlineData("none", "copy", true)]
    [InlineData("copy", "aac", false)]
    [InlineData("h264_vaapi", "copy", false)]
    public void A_plan_sent_as_it_is_plays_from_stream_ts(string video, string audio, bool direct) =>
        Assert.Equal(direct, new LivePlan { Video = video, Audio = audio }.Direct);
}
