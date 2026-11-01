using System;
using System.Collections.Concurrent;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Net;
using System.Net.Http;
using System.Threading;
using System.Threading.Tasks;
using Pley.Tv.Core.Auth;
using Pley.Tv.Core.Net;

namespace Pley.Tv.Core.Tests;

/// <summary>A server in a test: answers by method and URL, and remembers what it was asked.</summary>
sealed class FakeServer
{
    readonly Dictionary<string, Func<HttpRequest, (int Status, string? Body)>> routes = new Dictionary<string, Func<HttpRequest, (int, string?)>>();
    readonly Dictionary<string, IReadOnlyDictionary<string, string>> headers = new Dictionary<string, IReadOnlyDictionary<string, string>>();

    public ConcurrentQueue<HttpRequest> Requests { get; } = new ConcurrentQueue<HttpRequest>();

    /// <summary>
    /// Answers <paramref name="method"/> <paramref name="url"/>, with <paramref name="with"/>'s
    /// headers (JSON's Content-Type without); a status of 0 is no answer in time.
    /// </summary>
    public FakeServer On(string method, string url, Func<HttpRequest, (int Status, string? Body)> answer, IReadOnlyDictionary<string, string>? with = null)
    {
        routes[method + " " + url] = answer;
        if (with != null) headers[method + " " + url] = with;
        else headers.Remove(method + " " + url);
        return this;
    }

    public FakeServer On(string method, string url, int status, string? body = null, IReadOnlyDictionary<string, string>? with = null) =>
        On(method, url, _ => (status, body), with);

    public Transport Transport => req =>
    {
        Requests.Enqueue(req);
        var key = req.Method + " " + req.Url;
        if (!routes.TryGetValue(key, out var answer)) throw new NetworkException(NetworkFailure.NothingAnswers);
        var (status, body) = answer(req);
        if (status == 0) throw new NetworkException(NetworkFailure.NoAnswerInTime);
        var given = headers.TryGetValue(key, out var h) ? h : new Dictionary<string, string> { ["Content-Type"] = "application/json" };
        return Task.FromResult(new HttpResponse(status, name => given.FirstOrDefault(g => g.Key.Equals(name, StringComparison.OrdinalIgnoreCase)).Value, body));
    };
}

sealed class MemoryKeyValue : IKeyValue
{
    public Dictionary<string, string> Values { get; } = new Dictionary<string, string>();

    public string? Get(string key) => Values.TryGetValue(key, out var v) ? v : null;

    public void Set(string key, string value)
    {
        Values[key] = value;
        Written.Add(key);
    }

    /// <summary>The keys set, in order, each time.</summary>
    public List<string> Written { get; } = new List<string>();

    public void Remove(string key) => Values.Remove(key);
}

/// <summary>Timers that run when a test moves the clock past them; posts run at once.</summary>
sealed class FakeScheduler : IScheduler
{
    const long Start = 1_700_000_000_000;
    readonly List<(long At, Action Action, Cancelled Handle)> timers = new List<(long, Action, Cancelled)>();

    public long Now { get; private set; } = Start;

    public long Elapsed => Now - Start;

    public IDisposable After(long ms, Action action)
    {
        var handle = new Cancelled();
        timers.Add((Now + ms, action, handle));
        return handle;
    }

    public void Post(Action action) => action();

    /// <summary>Moves the clock by <paramref name="ms"/>, running each timer due on the way, in order.</summary>
    public void Advance(long ms)
    {
        var until = Now + ms;
        while (true)
        {
            var due = timers.Where(t => !t.Handle.Disposed && t.At <= until).OrderBy(t => t.At).FirstOrDefault();
            if (due.Action == null) break;
            timers.Remove(due);
            Now = due.At;
            due.Action();
        }
        Now = until;
    }

    /// <summary>The timers waiting, as ms from now.</summary>
    public IReadOnlyList<long> Pending => timers.Where(t => !t.Handle.Disposed).Select(t => t.At - Now).OrderBy(t => t).ToList();

    public sealed class Cancelled : IDisposable
    {
        public bool Disposed { get; private set; }

        public void Dispose() => Disposed = true;
    }
}

/// <summary>A clock tests move by hand; sleeping on it moves it at once.</summary>
sealed class FakeClock
{
    public long Now { get; set; } = 1_700_000_000_000;

    public List<long> Sleeps { get; } = new List<long>();

    public Func<long> Read => () => Now;

    public Task Sleep(long ms, System.Threading.CancellationToken cancel)
    {
        Sleeps.Add(ms);
        Now += ms;
        cancel.ThrowIfCancellationRequested();
        return Task.CompletedTask;
    }
}

/// <summary>An HttpMessageHandler that answers as a test says, for Core's own transport.</summary>
sealed class FakeHandler : HttpMessageHandler
{
    readonly Func<HttpRequestMessage, CancellationToken, Task<HttpResponseMessage>> answer;

    public FakeHandler(Func<HttpRequestMessage, CancellationToken, Task<HttpResponseMessage>> answer) => this.answer = answer;

    public FakeHandler(Func<HttpRequestMessage, HttpResponseMessage> answer) : this((req, _) => Task.FromResult(answer(req)))
    {
    }

    public ConcurrentQueue<(string Method, string Url, Dictionary<string, string> Headers)> Seen { get; } = new ConcurrentQueue<(string, string, Dictionary<string, string>)>();

    protected override Task<HttpResponseMessage> SendAsync(HttpRequestMessage request, CancellationToken cancellationToken)
    {
        // As sent: a User-Agent's parts are one value on the wire.
        Seen.Enqueue((request.Method.Method, request.RequestUri!.AbsoluteUri, request.Headers.NonValidated.ToDictionary(h => h.Key, h => h.Value.ToString())));
        return answer(request, cancellationToken);
    }

    public static HttpResponseMessage Answer(HttpStatusCode status, string body = "", string? location = null)
    {
        var res = new HttpResponseMessage(status) { Content = new StringContent(body) };
        if (location != null) res.Headers.Location = new Uri(location, UriKind.RelativeOrAbsolute);
        return res;
    }
}

/// <summary>A body that sends <paramref name="first"/> and then stays silent until cancelled.</summary>
sealed class StallingStream : Stream
{
    readonly byte[] first;
    bool sent;

    public StallingStream(byte[] first) => this.first = first;

    public override async Task<int> ReadAsync(byte[] buffer, int offset, int count, CancellationToken cancellationToken)
    {
        if (!sent)
        {
            sent = true;
            Array.Copy(first, 0, buffer, offset, first.Length);
            return first.Length;
        }
        await Task.Delay(Timeout.Infinite, cancellationToken);
        return 0;
    }

    public override int Read(byte[] buffer, int offset, int count) => throw new NotSupportedException();
    public override bool CanRead => true;
    public override bool CanSeek => false;
    public override bool CanWrite => false;
    public override long Length => throw new NotSupportedException();
    public override long Position { get => throw new NotSupportedException(); set => throw new NotSupportedException(); }
    public override void Flush() { }
    public override long Seek(long offset, SeekOrigin origin) => throw new NotSupportedException();
    public override void SetLength(long value) => throw new NotSupportedException();
    public override void Write(byte[] buffer, int offset, int count) => throw new NotSupportedException();
}
