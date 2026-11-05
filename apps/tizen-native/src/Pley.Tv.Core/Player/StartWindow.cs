using System;
using System.Collections.Generic;
using System.Globalization;
using System.IO;
using System.Net;
using System.Net.Http;
using System.Net.Sockets;
using System.Text;
using System.Text.RegularExpressions;
using System.Threading;
using System.Threading.Tasks;
using Pley.Tv.Core.Net;

namespace Pley.Tv.Core.Player;

/// <summary>
/// Tizen's player prepares an HLS stream from its first segment and ignores <c>EXT-X-START</c>: a
/// film resumed part-way had it fetch the film's start, which had the server start making the
/// stream again there, then again where it resumed (3 to 6 s more). This serves the stream's
/// playlists to the TV's player from the TV itself, the media playlist beginning at the segment the
/// film resumes in, every segment named by the server's own address, so the video still comes
/// straight from the server and only playlists pass through here. <see cref="OffsetMs"/> is where
/// that segment begins on the film's timeline: a player that counts from the playlist's first
/// segment is that far behind the film. The playlists are asked for as soon as it's made, while
/// the TV's player is still being set up: the server takes a third of a second for the first.
/// </summary>
public sealed class StartWindow : IDisposable
{
    readonly Uri upstream;
    readonly long startMs;
    readonly HttpClient http;
    readonly bool ownsClient;

    /// <summary>
    /// One client for every film: its connection to the server stays open from one film to the next.
    /// A client of each window's own had every film open a new connection (TLS and all) for two playlists.
    /// </summary>
    static readonly HttpClient Shared = new HttpClient { Timeout = TimeSpan.FromSeconds(30) };
    readonly Action<string> mark;
    readonly TcpListener listener;
    long offsetMs = -1;
    volatile bool gone;
    /// <summary>The playlists asked for ahead of the player, by address; each is given once.</summary>
    readonly Dictionary<string, Task<Answer>> ahead = new Dictionary<string, Task<Answer>>();
    readonly Task askedAhead;
    /// <summary>Cancelled as the film goes: its requests to the server, and connections still open, end with it.</summary>
    readonly CancellationTokenSource ending = new CancellationTokenSource();

    /// <summary>The most of a playlist read: a film's media playlist is a few hundred KB.</summary>
    const int MostPlaylistBytes = 4 << 20;

    sealed record Answer(HttpStatusCode Status, bool Ok, string? ContentType, string Body);

    public StartWindow(Uri source, long startMs, HttpMessageHandler? handler = null, Action<string>? mark = null)
    {
        upstream = new Uri(source.GetLeftPart(UriPartial.Authority));
        this.startMs = startMs;
        ownsClient = handler != null;
        http = handler == null ? Shared : new HttpClient(handler, disposeHandler: false) { Timeout = Shared.Timeout };
        this.mark = mark ?? (_ => { });
        listener = new TcpListener(IPAddress.Loopback, 0);
        listener.Start();
        var port = ((IPEndPoint)listener.LocalEndpoint).Port;
        Url = new Uri($"http://127.0.0.1:{port.ToString(CultureInfo.InvariantCulture)}{source.PathAndQuery}");
        var first = Get(source);
        lock (ahead) ahead[source.AbsoluteUri] = first;
        askedAhead = AskAhead(first, source);
        _ = Accept();
    }

    /// <summary>A multivariant playlist's first playlist, asked for once it came: the one the TV's player takes next.</summary>
    async Task AskAhead(Task<Answer> master, Uri at)
    {
        try
        {
            var answer = await master.ConfigureAwait(false);
            if (!answer.Ok || answer.Body.Contains("#EXTINF") || gone) return;
            foreach (var raw in answer.Body.Split('\n'))
            {
                var line = raw.Trim();
                if (line.Length == 0 || line.StartsWith("#", StringComparison.Ordinal)) continue;
                var next = new Uri(at, line);
                // One on another host is the player's to ask for: it doesn't pass through here.
                if (next.GetLeftPart(UriPartial.Authority) != upstream.GetLeftPart(UriPartial.Authority)) return;
                lock (ahead) ahead[next.AbsoluteUri] = Get(next);
                return;
            }
        }
        catch (Exception)
        {
            // The player asks for it itself.
        }
    }

    async Task<Answer> Get(Uri at)
    {
        var asked = System.Diagnostics.Stopwatch.StartNew();
        using var response = await http.GetAsync(at, HttpCompletionOption.ResponseHeadersRead, ending.Token).ConfigureAwait(false);
        var body = await ReadText(response.Content, ending.Token).ConfigureAwait(false);
        // Its name only: the query carries the stream's key.
        mark($"{Path.GetFileName(at.AbsolutePath)} from the server in {asked.ElapsedMilliseconds} ms");
        return new Answer(response.StatusCode, response.IsSuccessStatusCode, response.Content.Headers.ContentType?.ToString(), body);
    }

    /// <summary>A playlist's text, <see cref="MostPlaylistBytes"/> at most.</summary>
    static async Task<string> ReadText(HttpContent content, CancellationToken cancel)
    {
        using var from = await content.ReadAsStreamAsync().ConfigureAwait(false);
        using var text = new MemoryStream();
        var buffer = new byte[16 * 1024];
        int n;
        while ((n = await from.ReadAsync(buffer, 0, buffer.Length, cancel).ConfigureAwait(false)) > 0)
        {
            if (text.Length + n > MostPlaylistBytes) throw new HttpRequestException("the playlist is larger than a playlist can be");
            text.Write(buffer, 0, n);
        }
        return Encoding.UTF8.GetString(text.GetBuffer(), 0, (int)text.Length);
    }

    /// <summary>What the player is given in place of the source.</summary>
    public Uri Url { get; }

    /// <summary>Where the media playlist given begins on the film's timeline, in ms; -1 until one was given.</summary>
    public long OffsetMs => System.Threading.Interlocked.Read(ref offsetMs);

    public void Dispose()
    {
        gone = true;
        ending.Cancel();
        try
        {
            listener.Stop();
        }
        catch (SocketException)
        {
            // Stopped already.
        }
        if (ownsClient) http.Dispose();
    }

    static readonly Regex MapUri = new Regex("URI=\"([^\"]*)\"", RegexOptions.CultureInvariant);

    /// <summary>
    /// <paramref name="text"/>, a playlist read from <paramref name="at"/>, for the TV's player, and
    /// where it now begins on the film's timeline. A media playlist (one with segments) loses the
    /// segments that end before <paramref name="startMs"/> and names the rest, and its initialization
    /// segment, by the server's full address; its media sequence counts on from where it was. Any
    /// other playlist is given as it is (offset -1), its playlists' relative addresses coming here too.
    /// </summary>
    public static (string Text, long OffsetMs) Trim(string text, Uri at, long startMs)
    {
        if (!text.Contains("#EXTINF")) return (text, -1);
        var head = new List<string>();
        var tail = new List<string>();
        var segments = new List<(List<string> Lines, double Seconds)>();
        var pending = new List<string>();
        double seconds = 0;
        var inSegments = false;
        foreach (var raw in text.Split('\n'))
        {
            var line = raw.TrimEnd('\r');
            if (line.StartsWith("#EXT-X-START", StringComparison.Ordinal)) continue;
            if (line.StartsWith("#EXT-X-MAP:", StringComparison.Ordinal))
                line = MapUri.Replace(line, m => $"URI=\"{new Uri(at, m.Groups[1].Value).AbsoluteUri}\"");
            if (line.StartsWith("#EXTINF:", StringComparison.Ordinal))
            {
                inSegments = true;
                double.TryParse(line.Substring(8).Split(',')[0], NumberStyles.Float, CultureInfo.InvariantCulture, out seconds);
            }
            if (!inSegments)
            {
                head.Add(line);
                continue;
            }
            if (line.Length > 0 && !line.StartsWith("#", StringComparison.Ordinal))
            {
                pending.Add(new Uri(at, line).AbsoluteUri);
                segments.Add((pending, seconds));
                pending = new List<string>();
                seconds = 0;
            }
            else pending.Add(line);
        }
        // What came after the last segment (#EXT-X-ENDLIST): the end.
        tail.AddRange(pending);
        // The segment the start is in: the first whose end is past it.
        var first = 0;
        double begins = 0;
        while (first < segments.Count - 1 && (begins + segments[first].Seconds) * 1000 <= startMs)
        {
            begins += segments[first].Seconds;
            first++;
        }
        var lines = new List<string>();
        foreach (var line in head)
        {
            if (line.StartsWith("#EXT-X-MEDIA-SEQUENCE:", StringComparison.Ordinal)) continue;
            lines.Add(line);
            if (line == "#EXTM3U") lines.Add("#EXT-X-MEDIA-SEQUENCE:" + (Sequence(head) + first).ToString(CultureInfo.InvariantCulture));
        }
        for (var i = first; i < segments.Count; i++) lines.AddRange(segments[i].Lines);
        lines.AddRange(tail);
        return (string.Join("\n", lines), (long)Math.Round(begins * 1000));
    }

    static long Sequence(List<string> head)
    {
        foreach (var line in head)
            if (line.StartsWith("#EXT-X-MEDIA-SEQUENCE:", StringComparison.Ordinal) && long.TryParse(line.Substring(22), NumberStyles.Integer, CultureInfo.InvariantCulture, out var n))
                return n;
        return 0;
    }

    async Task Accept()
    {
        while (!gone)
        {
            TcpClient client;
            try
            {
                client = await listener.AcceptTcpClientAsync().ConfigureAwait(false);
            }
            catch (Exception)
            {
                return;
            }
            _ = Serve(client);
        }
    }

    /// <summary>
    /// One request, then the connection closes: a playlist or two, at the start. Only the server's
    /// own addresses are asked for: another program on the TV can't use this to reach another host.
    /// </summary>
    async Task Serve(TcpClient client)
    {
        using (client)
        using (ending.Token.Register(client.Close))
        {
            try
            {
                var stream = client.GetStream();
                var (method, target) = await StreamRelay.ReadHead(stream, ending.Token).ConfigureAwait(false);
                if (method != "GET" || !target.StartsWith("/", StringComparison.Ordinal) || target.StartsWith("//", StringComparison.Ordinal)) return;
                var at = new Uri(upstream, target);
                if (at.GetLeftPart(UriPartial.Authority) != upstream.GetLeftPart(UriPartial.Authority)) return;
                Task<Answer>? early;
                lock (ahead)
                {
                    if (ahead.TryGetValue(at.AbsoluteUri, out early)) ahead.Remove(at.AbsoluteUri);
                }
                var response = await (early ?? Get(at)).ConfigureAwait(false);
                // The playlist after it asked for first: the player, told of it, finds it on its way.
                if (early != null) await askedAhead.ConfigureAwait(false);
                var body = response.Body;
                if (response.Ok && target.Split('?')[0].EndsWith(".m3u8", StringComparison.OrdinalIgnoreCase))
                {
                    var (trimmed, offset) = Trim(body, at, startMs);
                    if (offset >= 0)
                    {
                        System.Threading.Interlocked.CompareExchange(ref offsetMs, offset, -1);
                        mark($"a playlist from {offset / 1000.0:0.0} s, to start at {startMs / 1000.0:0.0} s");
                    }
                    body = trimmed;
                }
                var bytes = Encoding.UTF8.GetBytes(body);
                var head = Encoding.ASCII.GetBytes(
                    $"HTTP/1.1 {((int)response.Status).ToString(CultureInfo.InvariantCulture)} {(response.Ok ? "OK" : "Error")}\r\n" +
                    $"Content-Type: {response.ContentType ?? "application/vnd.apple.mpegurl"}\r\n" +
                    $"Content-Length: {bytes.Length.ToString(CultureInfo.InvariantCulture)}\r\nConnection: close\r\n\r\n");
                await stream.WriteAsync(head, 0, head.Length).ConfigureAwait(false);
                await stream.WriteAsync(bytes, 0, bytes.Length).ConfigureAwait(false);
            }
            catch (Exception e) when (e is IOException or SocketException or HttpRequestException or OperationCanceledException or ObjectDisposedException)
            {
                // The player hung up, or the server didn't answer: the player says so itself.
            }
        }
    }
}
