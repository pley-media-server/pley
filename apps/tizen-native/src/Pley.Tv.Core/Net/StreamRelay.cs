using System;
using System.IO;
using System.Net;
using System.Net.Sockets;
using System.Security.Cryptography;
using System.Text;
using System.Threading;
using System.Threading.Tasks;

namespace Pley.Tv.Core.Net;

/// <summary>
/// A stream the TV's own player can open though it needs the token: the player can't send the
/// Authorization header, the only place Pley takes an app's token (SECURITY.md), so this app reads
/// the stream with it and hands it on as it comes, on the TV's loopback address only, under a
/// random path that lasts until the next <see cref="Serve"/> or <see cref="Stop"/>. One stream at a
/// time: serving another, or stopping, ends the connections of the one before.
/// </summary>
public sealed class StreamRelay : IDisposable
{
    /// <summary>The most of a request's head read: the player's GET is a few lines.</summary>
    const int HeadMost = 8 * 1024;

    const int BufferBytes = 64 * 1024;

    /// <summary>How long a player has to send its request once connected.</summary>
    const int HeadWaitMs = 5_000;

    readonly object gate = new object();
    TcpListener? listener;
    int port;
    string? path;
    Func<CancellationToken, Task<StreamResponse>>? open;
    /// <summary>The stream opened as it was served, for the first player to ask: the server answers while the player starts.</summary>
    Task<StreamResponse>? early;
    CancellationTokenSource serving = new CancellationTokenSource();
    bool disposed;

    /// <summary>
    /// Hands on what <paramref name="open"/> reads, in place of what was handed on, to each player
    /// that asks: its address, <c>http://127.0.0.1:{port}/{random}/stream.ts</c>. It is opened at
    /// once for the first player; its data waits in the connection until that player reads it.
    /// </summary>
    public string Serve(Func<CancellationToken, Task<StreamResponse>> open)
    {
        lock (gate)
        {
            if (disposed) throw new ObjectDisposedException(nameof(StreamRelay));
            EndServing();
            if (listener == null)
            {
                var l = new TcpListener(IPAddress.Loopback, 0);
                l.Start();
                listener = l;
                port = ((IPEndPoint)l.LocalEndpoint).Port;
                _ = Accept(l);
            }
            path = $"/{RandomPath()}/stream.ts";
            this.open = open;
            early = Opening(open, serving.Token);
            return $"http://127.0.0.1:{port}{path}";
        }
    }

    /// <summary>Hands on nothing more: the address served goes, and its connections with it.</summary>
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

    /// <summary><paramref name="open"/> started now; a failure is the asking player's to hear, or nobody's.</summary>
    static Task<StreamResponse> Opening(Func<CancellationToken, Task<StreamResponse>> open, CancellationToken cancel)
    {
        Task<StreamResponse> task;
        try
        {
            task = open(cancel);
        }
        catch (Exception e)
        {
            task = Task.FromException<StreamResponse>(e);
        }
        _ = task.ContinueWith(t => t.Exception, TaskContinuationOptions.OnlyOnFaulted);
        return task;
    }

    void EndServing()
    {
        // Opened early and never asked for: let go once it's there.
        _ = early?.ContinueWith(t => t.Result.Dispose(), TaskContinuationOptions.OnlyOnRanToCompletion);
        early = null;
        // Off the caller's thread (the UI's): cancelling closes the connections and the requests to
        // the server, and a request still waiting on a provider that sends nothing held the UI for
        // half a second. Not disposed: a player connecting just now may still listen to it.
        var ending = serving;
        _ = Task.Run(ending.Cancel);
        serving = new CancellationTokenSource();
        path = null;
        open = null;
    }

    /// <summary>128 random bits, as hex: no other program on the TV guesses it.</summary>
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
                // Stopped.
                return;
            }
            _ = Handle(client);
        }
    }

    async Task Handle(TcpClient client)
    {
        CancellationToken cancel;
        Func<CancellationToken, Task<StreamResponse>>? opener;
        string? served;
        lock (gate)
        {
            cancel = serving.Token;
            opener = open;
            served = path;
        }
        StreamResponse? body = null;
        // Ending the serving closes both ends: reads and writes on sockets don't all hear a token.
        using var closing = cancel.Register(() =>
        {
            client.Close();
            body?.Dispose();
        });
        try
        {
            using (client)
            {
                var net = client.GetStream();
                var (method, target) = await ReadHead(net, cancel).ConfigureAwait(false);
                if (Marks.On) Marks.Note($"relay: the player asks {method} ({(target == served ? "this stream" : "another")})");
                if (opener == null || served == null || target != served)
                {
                    await Answer(net, "404 Not Found").ConfigureAwait(false);
                    return;
                }
                if (method == "HEAD")
                {
                    // As the stream's head, without a length: it has none.
                    await Answer(net, "200 OK", "Content-Type: video/mp2t\r\n", empty: false).ConfigureAwait(false);
                    return;
                }
                if (method != "GET")
                {
                    await Answer(net, "405 Method Not Allowed").ConfigureAwait(false);
                    return;
                }
                Task<StreamResponse>? opened = null;
                lock (gate)
                {
                    if (path == served)
                    {
                        opened = early;
                        early = null;
                    }
                }
                try
                {
                    body = await (opened ?? opener(cancel)).ConfigureAwait(false);
                }
                catch (Exception)
                {
                    // Refused, or out of reach: the player's error makes its owner fall back.
                    await Answer(net, "502 Bad Gateway").ConfigureAwait(false);
                    return;
                }
                if (cancel.IsCancellationRequested) return;
                // As it comes, to the end or until the player lets go: a live stream has no length.
                await Answer(net, "200 OK", "Content-Type: video/mp2t\r\nCache-Control: no-store\r\n", empty: false).ConfigureAwait(false);
                if (Marks.On) Marks.Note($"relay: the server answered {body.Status}");
                var buffer = new byte[BufferBytes];
                int n;
                long total = 0;
                while ((n = await body.Body.ReadAsync(buffer, 0, buffer.Length, cancel).ConfigureAwait(false)) > 0)
                {
                    if (total == 0 && Marks.On) Marks.Note($"relay: first {n} bytes handed on");
                    total += n;
                    await net.WriteAsync(buffer, 0, n, cancel).ConfigureAwait(false);
                }
                if (Marks.On) Marks.Note($"relay: the stream ended after {total / 1024} KB");
            }
        }
        catch (Exception)
        {
            // The player let go, the server's stream ended or broke, or serving ended: either way, done.
        }
        finally
        {
            body?.Dispose();
        }
    }

    /// <summary>
    /// The request line's method and target, from a head read up to its blank line, <see
    /// cref="HeadMost"/> at most, within <see cref="HeadWaitMs"/>; empty ones when it isn't HTTP.
    /// </summary>
    internal static async Task<(string Method, string Target)> ReadHead(Stream net, CancellationToken cancel)
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
        var text = Encoding.ASCII.GetString(head, 0, length);
        var end = text.IndexOf("\r\n", StringComparison.Ordinal);
        var parts = (end >= 0 ? text.Substring(0, end) : text).Split(' ');
        return parts.Length >= 3 && parts[2].StartsWith("HTTP/", StringComparison.Ordinal) ? (parts[0], parts[1]) : ("", "");
    }

    static Task Answer(Stream net, string status, string headers = "", bool empty = true)
    {
        var length = empty ? "Content-Length: 0\r\n" : "";
        var bytes = Encoding.ASCII.GetBytes($"HTTP/1.1 {status}\r\n{headers}{length}Connection: close\r\n\r\n");
        return net.WriteAsync(bytes, 0, bytes.Length);
    }
}
