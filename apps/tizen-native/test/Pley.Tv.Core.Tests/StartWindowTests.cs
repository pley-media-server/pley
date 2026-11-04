using System;
using System.Collections.Concurrent;
using System.Linq;
using System.Net;
using System.Net.Http;
using System.Text;
using System.Threading;
using System.Threading.Tasks;
using Pley.Tv.Core.Player;
using Xunit;

namespace Pley.Tv.Core.Tests;

public class StartWindowTests
{
    /// <summary>Segments of 2.0, 2.5, 3.0 and 2.0 s: they begin at 0, 2, 4.5 and 7.5 s.</summary>
    const string Media = "#EXTM3U\n#EXT-X-VERSION:7\n#EXT-X-TARGETDURATION:3\n#EXT-X-MEDIA-SEQUENCE:0\n#EXT-X-PLAYLIST-TYPE:VOD\n#EXT-X-MAP:URI=\"init.mp4\"\n"
        + "#EXTINF:2.0,\n0.m4s\n#EXTINF:2.5,\n1.m4s\n#EXTINF:3.0,\n2.m4s\n#EXTINF:2.0,\n3.m4s\n#EXT-X-ENDLIST\n";
    const string Master = "#EXTM3U\n#EXT-X-STREAM-INF:BANDWIDTH=95000000,CODECS=\"hvc1.2.4.L153.B0\"\nindex.m3u8\n";
    static readonly Uri At = new Uri("https://pley.example.com:8443/api/media/k3y/index.m3u8");

    static string[] Uris(string text) => text.Split('\n').Where(l => l.Length > 0 && !l.StartsWith("#", StringComparison.Ordinal)).ToArray();

    [Fact]
    public void A_media_playlist_begins_at_the_segment_the_film_resumes_in()
    {
        var (text, offset) = StartWindow.Trim(Media, At, 5_000);
        Assert.Equal(4_500, offset);
        Assert.Equal(new[] { "https://pley.example.com:8443/api/media/k3y/2.m4s", "https://pley.example.com:8443/api/media/k3y/3.m4s" }, Uris(text));
        Assert.Contains("#EXT-X-MEDIA-SEQUENCE:2\n", text);
        Assert.Single(text.Split('\n'), l => l.StartsWith("#EXT-X-MEDIA-SEQUENCE", StringComparison.Ordinal));
        Assert.Contains("#EXT-X-MAP:URI=\"https://pley.example.com:8443/api/media/k3y/init.mp4\"", text);
        Assert.Contains("#EXT-X-PLAYLIST-TYPE:VOD\n", text);
        Assert.Contains("#EXTINF:3.0,\nhttps://pley.example.com:8443/api/media/k3y/2.m4s\n", text);
        Assert.EndsWith("#EXT-X-ENDLIST\n", text);
    }

    [Theory]
    [InlineData(0, 0, 4)]
    [InlineData(1_999, 0, 4)]
    [InlineData(2_000, 2_000, 3)]
    [InlineData(7_499, 4_500, 2)]
    [InlineData(60_000, 7_500, 1)]
    public void The_start_falls_in_one_segment_and_one_is_always_left(long start, long offset, int left)
    {
        var (text, from) = StartWindow.Trim(Media, At, start);
        Assert.Equal(offset, from);
        Assert.Equal(left, Uris(text).Length);
    }

    [Fact]
    public void A_multivariant_playlist_is_left_as_it_is_so_its_playlists_come_here_too()
    {
        Assert.Equal((Master, -1L), StartWindow.Trim(Master, At, 60_000));
    }

    sealed class Server : HttpMessageHandler
    {
        public ConcurrentQueue<string> Asked { get; } = new ConcurrentQueue<string>();

        protected override Task<HttpResponseMessage> SendAsync(HttpRequestMessage request, CancellationToken cancellationToken)
        {
            var path = request.RequestUri!.PathAndQuery;
            Asked.Enqueue(path);
            var text = path.EndsWith("master.m3u8", StringComparison.Ordinal) ? Master : path.EndsWith("index.m3u8", StringComparison.Ordinal) ? Media : null;
            return Task.FromResult(text == null
                ? new HttpResponseMessage(HttpStatusCode.Gone) { Content = new StringContent("{\"code\":\"gone\"}") }
                : new HttpResponseMessage(HttpStatusCode.OK) { Content = new StringContent(text, Encoding.UTF8, "application/vnd.apple.mpegurl") });
        }
    }

    [Fact]
    public async Task The_player_reads_the_playlists_from_the_tv_and_learns_where_they_begin()
    {
        var server = new Server();
        using var window = new StartWindow(new Uri("http://pley.example.com/api/media/k3y/master.m3u8"), 3_000, server);
        Assert.Equal("127.0.0.1", window.Url.Host);
        Assert.Equal(-1, window.OffsetMs);
        using var player = new HttpClient();
        Assert.Equal(Master, await player.GetStringAsync(window.Url));
        var media = await player.GetStringAsync(new Uri(window.Url, "index.m3u8"));
        Assert.Equal(new[] { "http://pley.example.com/api/media/k3y/1.m4s", "http://pley.example.com/api/media/k3y/2.m4s", "http://pley.example.com/api/media/k3y/3.m4s" }, Uris(media));
        Assert.Equal(2_000, window.OffsetMs);
        Assert.Equal(new[] { "/api/media/k3y/master.m3u8", "/api/media/k3y/index.m3u8" }, server.Asked);
    }

    [Fact]
    public async Task What_the_server_refuses_the_player_hears()
    {
        using var window = new StartWindow(new Uri("http://pley.example.com/api/media/k3y/master.m3u8"), 3_000, new Server());
        using var player = new HttpClient();
        using var response = await player.GetAsync(new Uri(window.Url, "other.m3u8"));
        Assert.Equal(HttpStatusCode.Gone, response.StatusCode);
    }
}
