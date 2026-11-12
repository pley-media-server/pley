using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Text.Json;
using System.Threading.Tasks;
using Pley.Tv.Core.Api;
using Pley.Tv.Core.Auth;
using Pley.Tv.Core.Net;
using Xunit;

namespace Pley.Tv.Core.Tests;

/// <summary>The player's calls to the API: sessions, subtitles, previews.</summary>
public class PlayerApiTests
{
    const string Home = "https://pley.example.com";

    static (Api.Api Api, FakeServer Server) SignedIn()
    {
        var clock = new FakeClock();
        var fake = new FakeServer();
        var tokens = new Tokens(Home, "client", new MemoryKeyValue(), fake.Transport, null, clock.Read);
        tokens.Install(new StoredTokens(Home, "pley_at_0", "pley_rt_0", clock.Now + 3_600_000, "library play", Home + "/api/oauth/token", Home + "/api/oauth/revoke"), "library play").Wait();
        return (new Api.Api(Home, tokens, fake.Transport), fake);
    }

    [Fact]
    public async Task A_sessions_answer_reads_its_tracks_quality_and_live_bitrate()
    {
        var (api, server) = SignedIn();
        server.On("POST", Home + "/api/playback", 200, @"{""session_id"":""s1"",""method"":""transcode"",""url"":""/api/media/k/playback/s1/master.m3u8"",""hls"":true,
            ""file_id"":7,""duration_ms"":5000,""audio_index"":1,""subtitle_index"":null,""burn_subtitles"":false,""audio_converted"":true,
            ""subtitles"":[{""index"":3,""codec"":""subrip"",""language"":""eng"",""title"":null,""default"":false,""forced"":false,""external"":false,""text_based"":true,""url"":""/api/files/7/subtitles/3.vtt""}],
            ""reasons"":[""HEVC 10-bit isn't supported.""],""reason"":""HEVC 10-bit isn't supported."",""auto_quality"":""8m"",""video_range"":null,""adaptive"":false,""live_bitrate"":true,""original_only"":true}");
        var info = await api.StartPlayback(new Dictionary<string, object?> { ["item_id"] = 1 });
        Assert.Equal(("s1", true, "8m", true), (info.SessionId, info.AudioConverted, info.AutoQuality, info.LiveBitrate));
        Assert.True(info.OriginalOnly);
        Assert.Equal("/api/files/7/subtitles/3.vtt", info.Subtitles.Single().Url);
        Assert.True(info.Subtitles.Single().TextBased);
    }

    [Fact]
    public async Task Subtitles_say_whether_they_are_whole()
    {
        var (api, server) = SignedIn();
        server.On("GET", Home + "/api/files/7/subtitles/3.vtt", 200, "WEBVTT\n", new Dictionary<string, string> { ["Content-Type"] = "text/vtt", ["X-Pley-Complete"] = "0" });
        var part = await api.Subtitles(Home + "/api/files/7/subtitles/3.vtt");
        Assert.Equal(("WEBVTT\n", false), (part.Text, part.Complete));
        var sent = server.Requests.Last();
        Assert.Equal(("text/vtt", 120_000), (sent.Headers!["Accept"], sent.TimeoutMs));
        server.On("GET", Home + "/api/files/7/subtitles/3.vtt", 200, "WEBVTT\n", new Dictionary<string, string> { ["Content-Type"] = "text/vtt" });
        Assert.True((await api.Subtitles(Home + "/api/files/7/subtitles/3.vtt")).Complete);
    }

    /// <summary>
    /// The server may stay silent a while making a picture track's .mks the first time: a minute,
    /// as Android's subtitles client allows. The API's 10 s said a German PGS track the server was
    /// still making couldn't be loaded.
    /// </summary>
    [Fact]
    public async Task Picture_subtitles_wait_a_minute_for_the_server()
    {
        var clock = new FakeClock();
        var fake = new FakeServer();
        var tokens = new Tokens(Home, "client", new MemoryKeyValue(), fake.Transport, null, clock.Read);
        await tokens.Install(new StoredTokens(Home, "pley_at_0", "pley_rt_0", clock.Now + 3_600_000, "library play", Home + "/api/oauth/token", Home + "/api/oauth/revoke"), "library play");
        HttpRequest? sent = null;
        var api = new Api.Api(Home, tokens, fake.Transport, req =>
        {
            sent = req;
            return Task.FromResult(new StreamResponse(200, h => h == "X-Pley-Complete" ? "0" : null, new MemoryStream(new byte[] { 1, 2, 3 })));
        });
        var part = await api.PictureSubtitles(Home + "/api/files/7/subtitles/4.mks");
        Assert.Equal((3, false), (part.Bytes.Length, part.Complete));
        Assert.Equal(("video/x-matroska", 60_000), (sent!.Headers!["Accept"], sent.TimeoutMs));
    }

    [Fact]
    public void A_parts_range_is_read_from_its_content_range()
    {
        Assert.Equal((0L, (long?)20_997_000), Api.Api.ContentRange("bytes 0-65535/20997000"));
        Assert.Equal((1_000L, (long?)null), Api.Api.ContentRange("bytes 1000-1999/*"));
        Assert.Null(Api.Api.ContentRange("bytes */20997000"));
        Assert.Null(Api.Api.ContentRange(null));
        Assert.Null(Api.Api.ContentRange("bytes 0-1/99999999999999999999999"));
    }

    [Fact]
    public async Task Previews_are_null_while_the_server_makes_them()
    {
        var (api, server) = SignedIn();
        server.On("GET", Home + "/api/files/7/trickplay", 404, @"{""code"":""not_found"",""message"":""being made""}");
        Assert.Null(await api.Trickplay(7));
        server.On("GET", Home + "/api/files/7/trickplay", 200, @"{""id"":""v1"",""interval_ms"":10000,""width"":320,""height"":180,""columns"":10,""rows"":10,""count"":250,""sheets"":3}");
        var info = await api.Trickplay(7);
        Assert.Equal(("v1", 10_000L, 250), (info!.Id, info.IntervalMs, info.Count));
        Assert.Equal(Home + "/api/files/7/trickplay/2.jpg?v=v1", api.TrickplaySheetUrl(7, 2, "v1"));
    }

    [Fact]
    public async Task A_sessions_status_and_its_bitrate()
    {
        var (api, server) = SignedIn();
        server.On("GET", Home + "/api/playback/s1", 200, @"{""session_id"":""s1"",""method"":""transcode"",""transcode"":{""job_id"":""j"",""state"":""running"",""video"":""transcode"",""audio"":""copy"",
            ""encoder"":""h264_nvenc"",""pipeline"":""gpu"",""speed"":2.5,""transcoded_ms"":60000,""bitrate_out"":8000000,""subtitle_burn"":false}}");
        var status = await api.PlaybackStatus("s1", timeoutMs: 5_000);
        Assert.Equal(("h264_nvenc", 2.5, 60_000L), (status.Transcode!.Encoder, status.Transcode.Speed, status.Transcode.TranscodedMs));
        Assert.Equal(5_000, server.Requests.Last().TimeoutMs);
        server.On("PUT", Home + "/api/playback/s1/bitrate", 200, @"{""bitrate"":4000}");
        Assert.Equal(4_000, (await api.SetBitrate("s1", 4_000)).Bitrate);
        Assert.Equal(4_000, JsonDocument.Parse(server.Requests.Last().Body!).RootElement.GetProperty("max_bitrate").GetInt64());
    }
}
