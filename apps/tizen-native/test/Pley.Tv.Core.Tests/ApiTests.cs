using System;
using System.Collections.Generic;
using System.Linq;
using System.Net;
using System.Net.Http;
using System.Net.Sockets;
using System.Security.Authentication;
using System.Text;
using System.Threading;
using System.Threading.Tasks;
using Pley.Tv.Core.Api;
using Pley.Tv.Core.Auth;
using Pley.Tv.Core.Net;
using Xunit;

namespace Pley.Tv.Core.Tests;

/// <summary>Pley's API as one signed-in app, and the transport under it.</summary>
public class ApiTests
{
    const string Home = "https://pley.example.com";
    const string Token = Home + "/api/oauth/token";
    const string Refreshed = @"{""access_token"":""pley_at_1"",""token_type"":""Bearer"",""expires_in"":3600,""scope"":""library play"",""refresh_token"":""pley_rt_1""}";
    static readonly Dictionary<string, string> InvalidTokenChallenge = new Dictionary<string, string>
    {
        ["Content-Type"] = "application/json",
        ["WWW-Authenticate"] = @"Bearer realm=""pley"", error=""invalid_token""",
    };

    static (Api.Api Api, FakeServer Server, FakeClock Clock) SignedIn(string server = Home)
    {
        var clock = new FakeClock();
        var fake = new FakeServer();
        var tokens = new Tokens(server, "client", new MemoryKeyValue(), fake.Transport, null, clock.Read);
        tokens.Install(new StoredTokens(server, "pley_at_0", "pley_rt_0", clock.Now + 3_600_000, "library play", server + "/api/oauth/token", server + "/api/oauth/revoke"), "library play").Wait();
        return (new Api.Api(server, tokens, fake.Transport), fake, clock);
    }

    [Fact]
    public async Task Reads_the_servers_snake_case_and_leaves_missing_fields_null()
    {
        var (api, server, _) = SignedIn();
        server.On("GET", Home + "/api/hubs/home", 200, @"{""hubs"":[{""key"":""continue_watching"",""title"":""Continue Watching"",""library_id"":null,
            ""items"":[{""id"":12,""library_id"":1,""kind"":""episode"",""title"":""Pilot"",""sort_title"":""pilot"",""index"":1,""parent_index"":1,
            ""grandparent_title"":""The Show"",""added_at"":1729600000,""poster_url"":""/api/items/12/images/poster?v=1"",
            ""user_state"":{""position_ms"":61000,""watched"":false,""play_count"":0},""media"":{""bitrate"":8000,""size"":null,""width"":1920,
            ""height"":1080,""video_codec"":""hevc"",""dynamic_range"":""hdr10""},""an_unknown_field"":true}]}]}");
        var hub = Assert.Single((await api.Home()).Hubs);
        Assert.Equal("continue_watching", hub.Key);
        Assert.Null(hub.LibraryId);
        var item = Assert.Single(hub.Items);
        Assert.Equal((12L, "episode", "The Show", 61000L), (item.Id, item.Kind, item.GrandparentTitle, item.UserState.PositionMs));
        Assert.Null(item.Year);
        Assert.Equal("hdr10", item.Media!.DynamicRange);
        var sent = server.Requests.Single();
        Assert.Equal("Bearer pley_at_0", sent.Headers!["Authorization"]);
        // The API's redirects are followed; the server may stay silent 10 s.
        Assert.Equal((true, 10_000), (sent.FollowRedirects, sent.TimeoutMs));
    }

    [Fact]
    public async Task Reads_a_null_where_a_value_belongs_as_its_default()
    {
        var (api, server, _) = SignedIn();
        server.On("GET", Home + "/api/items/12", 200, @"{""id"":12,""library_id"":null,""kind"":null,""title"":""Pilot"",""added_at"":null,
            ""user_state"":null,""genres"":null,""files"":[{""id"":1,""size"":null,""streams"":null,""chapters"":null}]}");
        var item = await api.Item(12);
        Assert.Equal((0L, "", 0L, 0L), (item.LibraryId, item.Kind, item.AddedAt, item.UserState.PositionMs));
        Assert.Empty(item.Genres);
        Assert.Empty(item.Files[0].Streams);
        server.On("GET", Home + "/api/auth/me", 200, @"{""id"":1,""username"":""a"",""name"":""A"",""is_admin"":null}");
        Assert.Equal("forced", (await api.Me()).SubtitleMode);
        server.On("GET", Home + "/api/oauth/profiles", 200, @"[{""id"":1,""name"":""A""},{""id"":2,""name"":""B"",""needs"":""retina_scan""},{""id"":3,""name"":""C"",""needs"":""pin""}]");
        Assert.Equal(new[] { "locked", "locked", "pin" }, (await api.Profiles()).Select(p => p.Takes));
    }

    [Fact]
    public async Task Refreshes_a_token_the_server_calls_invalid_once_and_asks_again()
    {
        var (api, server, _) = SignedIn();
        server.On("POST", Token, 200, Refreshed);
        server.On("GET", Home + "/api/auth/me", req => req.Headers!["Authorization"] == "Bearer pley_at_1"
            ? (200, @"{""id"":1,""username"":""a"",""name"":""A"",""subtitle_mode"":""off""}")
            : (401, @"{""code"":""unauthorized"",""message"":""expired""}"), InvalidTokenChallenge);
        var me = await api.Me();
        Assert.Equal("off", me.SubtitleMode);
        Assert.Equal(new[] { "GET", "POST", "GET" }, server.Requests.Select(r => r.Method));
    }

    [Fact]
    public async Task A_401_without_an_invalid_token_challenge_or_after_a_refresh_is_the_answer()
    {
        var (api, server, _) = SignedIn();
        server.On("POST", Token, 200, Refreshed);
        server.On("GET", Home + "/api/auth/me", 401, @"{""code"":""unauthorized"",""message"":""no""}");
        var bare = await Assert.ThrowsAsync<ApiException>(() => api.Me());
        Assert.Equal(401, bare.Status);
        Assert.DoesNotContain(server.Requests, r => r.Method == "POST");

        server.On("GET", Home + "/api/auth/me", 401, @"{""code"":""unauthorized"",""message"":""no""}", InvalidTokenChallenge);
        await Assert.ThrowsAsync<ApiException>(() => api.Me());
        Assert.Equal(1, server.Requests.Count(r => r.Method == "POST"));
        Assert.True(Api.Api.InvalidToken(@"Bearer error=invalid_token"));
        Assert.False(Api.Api.InvalidToken(@"Basic error=""invalid_token"""));
        Assert.False(Api.Api.InvalidToken(@"Bearer error=""insufficient_scope"""));
    }

    [Fact]
    public async Task Sends_no_token_to_another_origin()
    {
        var (api, server, _) = SignedIn();
        server.On("GET", "https://images.example.org/poster.jpg", 200, "{}");
        await api.Send("https://images.example.org/poster.jpg");
        Assert.False(server.Requests.Single().Headers!.ContainsKey("Authorization"));
    }

    [Fact]
    public async Task Reads_pleys_errors_and_a_stopped_session()
    {
        var (api, server, _) = SignedIn();
        server.On("GET", Home + "/api/items/9", 404, @"{""code"":""not_found"",""message"":""no such item""}");
        server.On("GET", Home + "/api/items/10", 502, "<html>Bad gateway</html>");
        server.On("GET", Home + "/api/items/11", 410, @"{""code"":""session_stopped"",""message"":""Maintenance tonight""}");
        server.On("GET", Home + "/api/items/12", 422, "\n\nFailed to deserialize the JSON body\u0007 into the target type\nmore");
        server.On("GET", Home + "/api/items/13", 400, @"{""code"":""bad_request"",""message"":""""}");
        server.On("GET", Home + "/api/items/14", 200, "<html>");
        var missing = await Assert.ThrowsAsync<ApiException>(() => api.Item(9));
        Assert.Equal(("not_found", "No such item."), (missing.Code, Errors.Message(missing)));
        // A gateway's refusal is an answer: the server is reached, not away.
        var gateway = await Assert.ThrowsAsync<ApiException>(() => api.Item(10));
        Assert.Equal(("internal", "the server refused (HTTP 502)"), (gateway.Code, gateway.Message));
        Assert.False(Errors.Unreachable(gateway));
        var stopped = await Assert.ThrowsAsync<SessionStoppedException>(() => api.Item(11));
        Assert.Equal("Maintenance tonight", stopped.Note);
        Assert.Equal("Playback was stopped by the server's administrator.\nMaintenance tonight", stopped.Message);
        Assert.Equal("Failed to deserialize the JSON body into the target type", (await Assert.ThrowsAsync<ApiException>(() => api.Item(12))).Message);
        Assert.Equal("the server refused (HTTP 400)", (await Assert.ThrowsAsync<ApiException>(() => api.Item(13))).Message);
        var unreadable = await Assert.ThrowsAsync<UnreadableAnswerException>(() => api.Item(14));
        Assert.Equal("The server's answer can't be read: is it Pley, and up to date?", Errors.Message(unreadable));
    }

    [Fact]
    public async Task Writes_nulls_it_means_and_leaves_out_what_it_keeps()
    {
        var (api, server, _) = SignedIn();
        server.On("PUT", Home + "/api/me/track-defaults", 204);
        server.On("PUT", Home + "/api/items/5/streams", 204);
        server.On("GET", Home + "/api/libraries/3/items?sort=title&watched=false&offset=0&limit=50", 200, @"{""total"":0,""items"":[]}");
        await api.SetTrackDefaults("en", null, "forced");
        await api.SetStreams(5, null, null, 3, keepAudio: true);
        await api.LibraryItems(3, new LibraryQuery { Sort = LibrarySort.Title, Watched = false, Limit = 50 });
        var bodies = server.Requests.Where(r => r.Body != null).Select(r => r.Body).ToList();
        Assert.Equal(@"{""audio_language"":""en"",""subtitle_language"":null,""subtitle_mode"":""forced""}", bodies[0]);
        Assert.Equal(@"{""subtitle_index"":3}", bodies[1]);
    }

    [Fact]
    public async Task A_librarys_listing_asks_for_its_quality_after_the_year_and_reads_what_it_can_be_filtered_by()
    {
        var (api, server, _) = SignedIn();
        // In one order: the listing's ETag hashes the query as written.
        server.On("GET", Home + "/api/libraries/3/items?sort=year&order=desc&genre=Drama&year=1999&resolution=4k&dynamic_range=hdr&offset=0&limit=200", 200,
            @"{""total"":0,""items"":[]}");
        await api.LibraryItems(3, new Pley.Tv.Core.Ui.LibraryParams(Sort: LibrarySort.Year, Genre: "Drama", Year: 1999, Resolution: "4k", DynamicRange: "hdr").Query() with { Limit = 200 });
        server.On("GET", Home + "/api/libraries/3/filters", 200,
            @"{""genres"":[""Drama""],""years"":[1999],""resolutions"":[""4k"",""1080""],""dynamic_ranges"":[""hdr"",""dolby_vision"",""sdr""]}");
        var filters = await api.LibraryFilters(3);
        Assert.Equal(new[] { "4k", "1080" }, filters.Resolutions);
        Assert.Equal(new[] { "hdr", "dolby_vision", "sdr" }, filters.DynamicRanges);
        // A server from before them sends neither.
        server.On("GET", Home + "/api/libraries/4/filters", 200, @"{""genres"":[],""years"":[2001]}");
        var older = await api.LibraryFilters(4);
        Assert.Equal(new[] { 2001 }, older.Years);
        Assert.Empty(older.Resolutions);
        Assert.Empty(older.DynamicRanges);
    }

    [Fact]
    public async Task Search_asks_for_60_and_reads_the_live_tv_channels_found()
    {
        var (api, server, _) = SignedIn();
        server.On("GET", Home + "/api/search?q=dune&limit=60", 200, @"{""items"":[],""people"":[{""id"":714,""name"":""Dune Hartley""}],
            ""channels"":[{""id"":9,""source_id"":1,""name"":""Dune Classics"",""number"":""212"",""group"":""Films"",
            ""logo"":""/api/live/channels/9/logo"",""favorite"":false,""has_guide"":false,""now"":null,""next"":null}]}");
        var found = await api.Search("dune");
        var channel = Assert.Single(found.Channels!);
        Assert.Equal((9L, "Dune Classics", "212", "/api/live/channels/9/logo"), (channel.Id, channel.Name, channel.Number, channel.Logo));
        Assert.Null(channel.Now);
        // A server from before search found channels sends none.
        server.On("GET", Home + "/api/search?q=north&limit=60", 200, @"{""items"":[],""people"":[]}");
        Assert.Null((await api.Search("north")).Channels);
    }

    [Fact]
    public void Says_errors_as_the_android_app_does()
    {
        Assert.Equal("No answer in time.", Errors.Message(new NetworkException(NetworkFailure.NoAnswerInTime)));
        Assert.Equal("Nothing answers there.", Errors.UnreachableReason(new NetworkException(NetworkFailure.NothingAnswers)));
        Assert.Equal("Done!", Errors.AsSentence("done!"));
        Assert.Equal("That isn't a Pley server.", Errors.Message(new System.Text.Json.JsonException()));
        Assert.Equal("That address isn't known on this network. Check that the server is on and that this device is on its network.",
            Errors.OfflineBody(new NetworkException(NetworkFailure.UnknownAddress)));
        Assert.True(Errors.Unreachable(new NetworkException(NetworkFailure.SecureConnection)));
        Assert.False(Errors.Unreachable(new NetworkException(NetworkFailure.Cancelled)));
    }

    // ---- the transport

    static Transport Over(FakeHandler handler) => Transports.Create(_ => handler);

    [Fact]
    public async Task Follows_the_apis_redirects_within_their_scheme_with_the_token_on_the_server_only()
    {
        var handler = new FakeHandler(req => req.RequestUri!.AbsoluteUri switch
        {
            Home + "/api/collections/1/images/poster" => FakeHandler.Answer(HttpStatusCode.TemporaryRedirect, location: "/api/items/7/images/poster"),
            Home + "/api/items/7/images/poster" => FakeHandler.Answer(HttpStatusCode.Found, location: "https://cdn.example.org/p.jpg"),
            "https://cdn.example.org/p.jpg" => FakeHandler.Answer(HttpStatusCode.OK, "jpeg"),
            Home + "/down" => FakeHandler.Answer(HttpStatusCode.Found, location: "http://pley.example.com/x"),
            _ => FakeHandler.Answer(HttpStatusCode.NotFound),
        });
        var http = Over(handler);
        var auth = new Dictionary<string, string> { ["Authorization"] = "Bearer t" };
        var res = await http(new HttpRequest { Url = Home + "/api/collections/1/images/poster", Headers = auth, FollowRedirects = true });
        Assert.Equal((200, "jpeg"), (res.Status, res.Text));
        Assert.Equal(new[] { true, true, false }, handler.Seen.Select(s => s.Headers.ContainsKey("Authorization")));
        // Never from https down to http; and without being asked, not at all.
        Assert.Equal(302, (await http(new HttpRequest { Url = Home + "/down", FollowRedirects = true })).Status);
        var unfollowed = await http(new HttpRequest { Url = Home + "/api/collections/1/images/poster" });
        Assert.Equal((307, "/api/items/7/images/poster"), (unfollowed.Status, unfollowed.Location));
    }

    [Fact]
    public async Task Introduces_the_app_on_every_request()
    {
        var handler = new FakeHandler(_ => FakeHandler.Answer(HttpStatusCode.OK));
        var http = Transports.WithUserAgent(Over(handler), UserAgents.Of("0.1.0", "9.0", "QE55Q80D (Living room); TV"));
        await http(new HttpRequest { Url = Home + "/api/server" });
        Assert.Equal("pleyTizen/0.1.0 (SMART-TV; Tizen 9.0; QE55Q80D Living room TV)", handler.Seen.Single().Headers["User-Agent"]);
        Assert.Equal("pleyTizen/? (SMART-TV; Tizen ?)", UserAgents.Of("?", null, "★"));
        Assert.Equal("pleyTizen/1 (SMART-TV; Tizen 8.0; Tele - salon)", UserAgents.Of("1", "8.0", "Télé – salon"));
    }

    [Theory]
    [InlineData(SocketError.ConnectionRefused, "nothing answers there")]
    [InlineData(SocketError.HostUnreachable, "nothing answers there")]
    [InlineData(SocketError.TimedOut, "no answer in time")]
    [InlineData(SocketError.HostNotFound, "that address isn't known on this network")]
    [InlineData(SocketError.ConnectionReset, "the connection to the server failed")]
    public async Task Says_why_nothing_answered(SocketError error, string reason)
    {
        var http = Over(new FakeHandler((_, _) => Task.FromException<HttpResponseMessage>(new HttpRequestException("failed", new SocketException((int)error)))));
        var e = await Assert.ThrowsAsync<NetworkException>(() => http(new HttpRequest { Url = Home + "/api/server" }));
        Assert.Equal(reason, e.Message);
    }

    [Fact]
    public async Task Says_a_secure_connection_failed_and_a_request_was_cancelled()
    {
        var tls = Over(new FakeHandler((_, _) => Task.FromException<HttpResponseMessage>(new HttpRequestException("ssl", new AuthenticationException("bad certificate")))));
        Assert.Equal(NetworkFailure.SecureConnection, (await Assert.ThrowsAsync<NetworkException>(() => tls(new HttpRequest { Url = Home }))).Failure);
        using var cancel = new CancellationTokenSource();
        var waiting = Over(new FakeHandler(async (_, token) =>
        {
            await Task.Delay(Timeout.Infinite, token);
            return FakeHandler.Answer(HttpStatusCode.OK);
        }));
        var asked = waiting(new HttpRequest { Url = Home, Cancel = cancel.Token });
        cancel.Cancel();
        Assert.True((await Assert.ThrowsAsync<NetworkException>(() => asked)).Cancelled);
    }

    [Fact]
    public async Task Gives_up_on_a_server_silent_longer_than_the_read_timeout_however_long_the_whole_takes()
    {
        var http = Over(new FakeHandler(_ => new HttpResponseMessage(HttpStatusCode.OK) { Content = new StreamContent(new StallingStream(Encoding.UTF8.GetBytes("{"))) }));
        var started = DateTime.UtcNow;
        var e = await Assert.ThrowsAsync<NetworkException>(() => http(new HttpRequest { Url = Home, TimeoutMs = 300 }));
        Assert.Equal(NetworkFailure.NoAnswerInTime, e.Failure);
        Assert.True(DateTime.UtcNow - started < TimeSpan.FromSeconds(5));
    }

    [Fact]
    public void Makes_handlers_that_follow_no_redirects_and_time_out_connecting()
    {
        using var handler = Transports.DefaultHandler(TimeSpan.FromSeconds(3));
        Assert.Equal("SocketsHttpHandler", handler.GetType().Name);
        Assert.False((bool)handler.GetType().GetProperty("AllowAutoRedirect")!.GetValue(handler)!);
        Assert.Equal(TimeSpan.FromSeconds(3), handler.GetType().GetProperty("ConnectTimeout")!.GetValue(handler));
        Assert.Equal(Transports.HomeConnectTimeoutMs, Transports.ConnectTimeoutFor("http://192.168.1.20:7539/api/server"));
        Assert.Equal(Transports.ConnectTimeoutMs, Transports.ConnectTimeoutFor("http://100.64.0.1:7539/api/server"));
    }
}
