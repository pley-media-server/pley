using System;
using System.Collections.Generic;
using System.IO;
using System.Net;
using System.Net.Sockets;
using System.Security.Cryptography;
using System.Text;
using System.Threading;
using System.Threading.Tasks;
using Pley.Tv.Core.Local;

namespace Pley.Tv.Core.Net;

/// <summary>
/// Stretches of a file served as one file to the TV's own player, which opens addresses, not
/// parts of files: a DVD title's cells, read out of its disc image. On the TV's loopback address
/// only, under a random path that lasts until the next <see cref="Serve"/> or <see cref="Stop"/>,
/// with ranges, so the player seeks in it. One at a time, as <see cref="StreamRelay"/>.
/// </summary>
public sealed class SpanServer : IDisposable
{
    const int HeadMost = 8 * 1024;
    const int HeadWaitMs = 5_000;
    const int BufferBytes = 64 * 1024;

    readonly object gate = new object();
    TcpListener? listener;
    int port;
    string? path;
    Served? served;
    CancellationTokenSource serving = new CancellationTokenSource();
    bool disposed;

    sealed record Served(string File, IReadOnlyList<DiscSpan> Spans, long Length, string Type);

    /// <summary>Serves <paramref name="spans"/> of <paramref name="file"/>, one after another, as <paramref name="type"/>: its address.</summary>
    public string Serve(string file, IReadOnlyList<DiscSpan> spans, string type = "video/mpeg", string name = "title.mpg")
    {
        lock (gate)
        {
            if (disposed) throw new ObjectDisposedException(nameof(SpanServer));
            EndServing();
            if (listener == null)
            {
                var l = new TcpListener(IPAddress.Loopback, 0);
                l.Start();
                listener = l;
                port = ((IPEndPoint)l.LocalEndpoint).Port;
                _ = Accept(l);
            }
            long length = 0;
            foreach (var s in spans) length += s.Length;
            served = new Served(file, spans, length, type);
            path = $"/{RandomPath()}/{name}";
            return $"http://127.0.0.1:{port}{path}";
        }
    }

    public void Stop()
    {
        lock (gate) EndServing();
    }

    public void Dispose()
    {
        lock (gate)
        {
            if (disposed) return;
            disposed = true;
            EndServing();
            listener?.Stop();
            listener = null;
        }
    }

    void EndServing()
    {
        var ending = serving;
        _ = Task.Run(ending.Cancel);
        serving = new CancellationTokenSource();
        path = null;
        served = null;
    }

    static string RandomPath()
    {
        var bytes = new byte[16];
        using (var random = RandomNumberGenerator.Create()) random.GetBytes(bytes);
        var hex = new StringBuilder(32);
        foreach (var b in bytes) hex.Append(b.ToString("x2"));
        return hex.ToString();
    }

    async Task Accept(TcpListener l)
    {
        while (true)
        {
            TcpClient client;
            try
            {
                client = await l.AcceptTcpClientAsync().ConfigureAwait(false);
            }
            catch (Exception)
            {
                return;
            }
            _ = Handle(client);
        }
    }

    async Task Handle(TcpClient client)
    {
        CancellationToken cancel;
        Served? what;
        string? expected;
        lock (gate)
        {
            cancel = serving.Token;
            what = served;
            expected = path;
        }
        using var closing = cancel.Register(client.Close);
        try
        {
            using (client)
            {
                var net = client.GetStream();
                var (method, target, range) = await ReadHead(net, cancel).ConfigureAwait(false);
                if (Marks.On) Marks.Note($"disc: the player asks {method} {range ?? "all"}");
                if (what == null || expected == null || target != expected)
                {
                    await Answer(net, "404 Not Found", "Content-Length: 0\r\n").ConfigureAwait(false);
                    return;
                }
                var (from, to) = Range(range, what.Length);
                if (from > to || from >= what.Length)
                {
                    await Answer(net, "416 Range Not Satisfiable", $"Content-Range: bytes */{what.Length}\r\nContent-Length: 0\r\n").ConfigureAwait(false);
                    return;
                }
                var partial = range != null;
                var headers = $"Content-Type: {what.Type}\r\nAccept-Ranges: bytes\r\nContent-Length: {to - from + 1}\r\n"
                    + (partial ? $"Content-Range: bytes {from}-{to}/{what.Length}\r\n" : "");
                await Answer(net, partial ? "206 Partial Content" : "200 OK", headers).ConfigureAwait(false);
                if (method == "HEAD") return;
                using var file = new FileStream(what.File, FileMode.Open, FileAccess.Read, FileShare.ReadWrite, 4096, useAsync: true);
                var buffer = new byte[BufferBytes];
                var position = from;
                // The spans in order: where the asked range falls in each.
                long start = 0;
                foreach (var span in what.Spans)
                {
                    var end = start + span.Length;
                    if (position < end && position <= to)
                    {
                        file.Seek(span.Offset + (position - start), SeekOrigin.Begin);
                        var left = Math.Min(end, to + 1) - position;
                        while (left > 0)
                        {
                            var n = await file.ReadAsync(buffer, 0, (int)Math.Min(buffer.Length, left), cancel).ConfigureAwait(false);
                            if (n <= 0) return;
                            await net.WriteAsync(buffer, 0, n, cancel).ConfigureAwait(false);
                            left -= n;
                            position += n;
                        }
                    }
                    start = end;
                    if (position > to) break;
                }
            }
        }
        catch (Exception)
        {
            // The player let go (it does at every seek), or serving ended.
        }
    }

    /// <summary>The first and last byte of a "bytes=a-b" range (either end left out), else the whole.</summary>
    static (long From, long To) Range(string? range, long length)
    {
        if (range == null || !range.StartsWith("bytes=", StringComparison.OrdinalIgnoreCase)) return (0, length - 1);
        var spec = range.Substring(6).Split(',')[0].Trim();
        var dash = spec.IndexOf('-');
        if (dash < 0) return (0, length - 1);
        var a = spec.Substring(0, dash).Trim();
        var b = spec.Substring(dash + 1).Trim();
        if (a.Length == 0)
        {
            // The last n bytes.
            var n = long.TryParse(b, out var last) ? last : 0;
            return (Math.Max(0, length - n), length - 1);
        }
        var from = long.TryParse(a, out var f) ? f : 0;
        var to = b.Length > 0 && long.TryParse(b, out var t) ? Math.Min(t, length - 1) : length - 1;
        return (from, to);
    }

    static async Task<(string Method, string Target, string? Range)> ReadHead(Stream net, CancellationToken cancel)
    {
        using var wait = CancellationTokenSource.CreateLinkedTokenSource(cancel);
        wait.CancelAfter(HeadWaitMs);
        var head = new byte[HeadMost];
        var length = 0;
        while (length < head.Length)
        {
            var n = await net.ReadAsync(head, length, head.Length - length, wait.Token).ConfigureAwait(false);
            if (n == 0) break;
            length += n;
            if (Encoding.ASCII.GetString(head, 0, length).Contains("\r\n\r\n")) break;
        }
        var lines = Encoding.ASCII.GetString(head, 0, length).Split(new[] { "\r\n" }, StringSplitOptions.None);
        var parts = lines[0].Split(' ');
        string? range = null;
        foreach (var line in lines)
            if (line.StartsWith("Range:", StringComparison.OrdinalIgnoreCase)) range = line.Substring(6).Trim();
        return parts.Length >= 3 ? (parts[0], parts[1], range) : ("", "", null);
    }

    static Task Answer(Stream net, string status, string headers)
    {
        var bytes = Encoding.ASCII.GetBytes($"HTTP/1.1 {status}\r\n{headers}Connection: close\r\n\r\n");
        return net.WriteAsync(bytes, 0, bytes.Length);
    }
}
