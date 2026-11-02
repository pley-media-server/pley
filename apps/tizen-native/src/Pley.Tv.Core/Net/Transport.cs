using System;
using System.Collections.Concurrent;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Net.Http;
using System.Net.Sockets;
using System.Security.Authentication;
using System.Text;
using System.Text.Json;
using System.Threading;
using System.Threading.Tasks;

namespace Pley.Tv.Core.Net;

/// <summary>How a response's body is read.</summary>
public enum ResponseType
{
    Text,
    /// <summary>Text, parsed as JSON on demand (<see cref="HttpResponse.Json"/>).</summary>
    Json,
    Bytes,
}

/// <summary>One HTTP request (the web TV app's <c>net/transport.ts</c>).</summary>
public sealed record HttpRequest
{
    public string Method { get; init; } = "GET";
    public string Url { get; init; } = "";
    public IReadOnlyDictionary<string, string>? Headers { get; init; }
    public string? Body { get; init; }
    public ResponseType ResponseType { get; init; } = ResponseType.Text;
    /// <summary>
    /// How long the server may stay silent: before it answers, and between pieces of the body. The
    /// whole has no limit, as the Android app's reads have none: a big answer over a slow network
    /// takes as long as it takes.
    /// </summary>
    public int TimeoutMs { get; init; } = Transports.ReadTimeoutMs;
    /// <summary>How long the connection may take to be accepted; null: 3 s for a server at home, 10 s elsewhere.</summary>
    public int? ConnectTimeoutMs { get; init; }
    /// <summary>
    /// Follows redirects (the API's), never from http to https or back, and sends Authorization to
    /// the address asked only. Off, a redirect is the answer: sign-in requests and server checks
    /// go to the address they were given, or nowhere.
    /// </summary>
    public bool FollowRedirects { get; init; }
    /// <summary>Stops the request: a screen that went away, a newer search.</summary>
    public CancellationToken Cancel { get; init; }
    /// <summary>Hears how much of the body has arrived so far, as it arrives (a speed test).</summary>
    public Action<long>? OnProgress { get; init; }
}

public sealed class HttpResponse
{
    readonly Func<string, string?> header;
    bool parsed;
    JsonElement? json;

    public HttpResponse(int status, Func<string, string?> header, string? text = null, byte[]? bytes = null)
    {
        Status = status;
        this.header = header;
        Text = text;
        Bytes = bytes;
    }

    public int Status { get; }
    public string? Text { get; }
    public byte[]? Bytes { get; }

    /// <summary>A response header, case-insensitively; null when absent.</summary>
    public string? Header(string name) => header(name);

    /// <summary>Where a redirect (3xx) sends the request, as the server wrote it.</summary>
    public string? Location => header("Location");

    /// <summary>Whether the answer is a redirect with somewhere to go.</summary>
    public bool IsRedirect => Status is 300 or 301 or 302 or 303 or 307 or 308 && !string.IsNullOrEmpty(Location);

    /// <summary>The body as JSON; null when it is empty or isn't JSON.</summary>
    public JsonElement? Json()
    {
        if (parsed) return json;
        parsed = true;
        if (string.IsNullOrEmpty(Text)) return null;
        try
        {
            using var doc = JsonDocument.Parse(Text!);
            json = doc.RootElement.Clone();
        }
        catch (JsonException)
        {
            json = null;
        }
        return json;
    }
}

/// <summary>Sends requests: HttpClient on the TV, a fake in tests.</summary>
public delegate Task<HttpResponse> Transport(HttpRequest request);

/// <summary>
/// An answer whose body is read as it comes, for as long as it lasts (a live channel's MPEG-TS):
/// disposed, its connection goes.
/// </summary>
public sealed class StreamResponse : IDisposable
{
    readonly Func<string, string?> header;
    readonly IDisposable? owner;

    public StreamResponse(int status, Func<string, string?> header, Stream body, IDisposable? owner = null)
    {
        Status = status;
        this.header = header;
        Body = body;
        this.owner = owner;
    }

    public int Status { get; }
    public Stream Body { get; }

    /// <summary>A response header, case-insensitively; null when absent.</summary>
    public string? Header(string name) => header(name);

    public void Dispose()
    {
        Body.Dispose();
        owner?.Dispose();
    }
}

/// <summary>Opens GETs whose bodies are read as they come: HttpClient on the TV, a fake in tests.</summary>
public delegate Task<StreamResponse> StreamTransport(HttpRequest request);

/// <summary>Why a request got no answer, in the Android app's words (<see cref="Transports.Reason"/>).</summary>
public enum NetworkFailure
{
    /// <summary>Nothing accepted the connection: the server is off, or not at that address.</summary>
    NothingAnswers,
    NoAnswerInTime,
    /// <summary>The name can't be looked up on this network.</summary>
    UnknownAddress,
    /// <summary>TLS failed: a certificate the TV doesn't trust, or another program on the port.</summary>
    SecureConnection,
    /// <summary>Any other failure of the connection.</summary>
    ConnectionFailed,
    /// <summary>The request was stopped on purpose (<see cref="HttpRequest.Cancel"/>).</summary>
    Cancelled,
    /// <summary>
    /// What answers at the server's address can't prove it is the server signed in to
    /// (<see cref="Auth.ServerProofs"/>): the TV treats it as the server being out of reach.
    /// </summary>
    AnotherDevice,
    /// <summary>The server, or a proxy before it, answered a proof with a server error: out of reach for now.</summary>
    ServerFailed,
}

/// <summary>No answer: the network, a timeout, or a request cancelled. Its message is the reason, in a few words.</summary>
public class NetworkException : Exception
{
    public NetworkException(NetworkFailure failure, Exception? inner = null) : base(Transports.Reason(failure), inner) => Failure = failure;

    public NetworkFailure Failure { get; }

    public bool Cancelled => Failure == NetworkFailure.Cancelled;
}

/// <summary>Makes the handler of the connections that get <paramref name="connectTimeout"/> to be accepted.</summary>
public delegate HttpMessageHandler HandlerFactory(TimeSpan connectTimeout);

public static class Transports
{
    /// <summary>How long the server may stay silent, as the Android app's API client allows.</summary>
    public const int ReadTimeoutMs = 10_000;

    /// <summary>How long a server on the internet has to accept a connection.</summary>
    public const int ConnectTimeoutMs = 10_000;

    /// <summary>
    /// How long a server at a home address or name (192.168.…, <c>nas.local</c>) has to accept a
    /// connection: at home one accepts at once or not at all, and away from home its address
    /// leads nowhere, which 10 s would only say later.
    /// </summary>
    public const int HomeConnectTimeoutMs = 3_000;

    /// <summary>
    /// How long an idle connection is kept: under the 30 s after which the server closes one
    /// (apps/server/src/http.rs), so a request never goes out on a connection being closed.
    /// </summary>
    public const int KeepAliveMs = 25_000;

    /// <summary>Redirects followed at most, as OkHttp does.</summary>
    const int MaxRedirects = 20;

    /// <summary>The words for each failure, as the Android app says them (its <c>describe</c>).</summary>
    public static string Reason(NetworkFailure failure) => failure switch
    {
        NetworkFailure.NothingAnswers => "nothing answers there",
        NetworkFailure.NoAnswerInTime => "no answer in time",
        NetworkFailure.UnknownAddress => "that address isn't known on this network",
        NetworkFailure.SecureConnection => "its secure connection failed",
        NetworkFailure.Cancelled => "cancelled",
        NetworkFailure.AnotherDevice => "another device answers there",
        NetworkFailure.ServerFailed => "the server couldn't answer",
        _ => ConnectionFailed,
    };

    /// <summary>What a network failure without words of its own says.</summary>
    public const string ConnectionFailed = "the connection to the server failed";

    static readonly Type? SocketsHandler = Type.GetType("System.Net.Http.SocketsHttpHandler, System.Net.Http", throwOnError: false);

    /// <summary>
    /// The handler Core makes by itself: .NET's SocketsHttpHandler, which the TV's .NET 6 has but
    /// .NET Standard can't name, given the connect timeout and idle connections' lifetime. Where
    /// it doesn't exist, HttpClientHandler, without a connect timeout of its own: the request's
    /// first wait then covers connecting too. Neither follows redirects or keeps cookies.
    /// </summary>
    public static HttpMessageHandler DefaultHandler(TimeSpan connectTimeout)
    {
        if (SocketsHandler != null)
        {
            try
            {
                var handler = (HttpMessageHandler)Activator.CreateInstance(SocketsHandler)!;
                SocketsHandler.GetProperty("AllowAutoRedirect")!.SetValue(handler, false);
                SocketsHandler.GetProperty("UseCookies")!.SetValue(handler, false);
                SocketsHandler.GetProperty("ConnectTimeout")!.SetValue(handler, connectTimeout);
                SocketsHandler.GetProperty("PooledConnectionIdleTimeout")!.SetValue(handler, TimeSpan.FromMilliseconds(KeepAliveMs));
                return handler;
            }
            catch (Exception)
            {
                // A runtime whose handler doesn't take these: the plain one below.
            }
        }
        return new HttpClientHandler { AllowAutoRedirect = false, UseCookies = false };
    }

    /// <summary>
    /// HttpClient, one per connect timeout, with the handlers <paramref name="handlers"/> makes
    /// (<see cref="DefaultHandler"/> without): each request with its own read timeout and
    /// cancellation, redirects as it asks.
    /// </summary>
    public static Transport Create(HandlerFactory? handlers = null)
    {
        var clientFor = Clients(handlers);
        return req => Send(clientFor, req);
    }

    /// <summary>HttpClient by connect timeout, made once each with <paramref name="handlers"/>' handler; each request has its own other timeouts.</summary>
    internal static Func<int, HttpClient> Clients(HandlerFactory? handlers)
    {
        var make = handlers ?? DefaultHandler;
        // Lazy, so two requests at once never make two clients (and two pools of connections).
        var clients = new ConcurrentDictionary<int, Lazy<HttpClient>>();
        return connectMs => clients.GetOrAdd(connectMs, ms => new Lazy<HttpClient>(() =>
            new HttpClient(make(TimeSpan.FromMilliseconds(ms))) { Timeout = System.Threading.Timeout.InfiniteTimeSpan })).Value;
    }

    /// <summary>The app's transport, made by <see cref="Create"/> with Core's own handlers.</summary>
    public static readonly Transport Http = Create();

    /// <summary>
    /// The app's GETs read as they come, with Core's own handlers: the connection and the first
    /// answer are timed as a request's, the body not at all (it lasts as long as it's read).
    /// Redirects are the answer.
    /// </summary>
    public static readonly StreamTransport Streams = CreateStreams();

    static StreamTransport CreateStreams()
    {
        var clientFor = Clients(null);
        return req => Open(clientFor, req);
    }

    static async Task<StreamResponse> Open(Func<int, HttpClient> clientFor, HttpRequest req)
    {
        var connectMs = req.ConnectTimeoutMs ?? ConnectTimeoutFor(req.Url);
        using var idle = new CancellationTokenSource();
        using var both = CancellationTokenSource.CreateLinkedTokenSource(idle.Token, req.Cancel);
        idle.CancelAfter(connectMs + req.TimeoutMs);
        try
        {
            using var message = new HttpRequestMessage(HttpMethod.Get, req.Url);
            foreach (var h in req.Headers ?? new Dictionary<string, string>()) message.Headers.TryAddWithoutValidation(h.Key, h.Value);
            var res = await clientFor(connectMs).SendAsync(message, HttpCompletionOption.ResponseHeadersRead, both.Token).ConfigureAwait(false);
            try
            {
                var body = await res.Content.ReadAsStreamAsync().ConfigureAwait(false);
                var headers = res.Headers.Concat(res.Content.Headers).ToList();
                string? Header(string name) => headers.Where(h => h.Key.Equals(name, StringComparison.OrdinalIgnoreCase))
                    .Select(h => string.Join(", ", h.Value)).FirstOrDefault();
                return new StreamResponse((int)res.StatusCode, Header, body, res);
            }
            catch (Exception)
            {
                res.Dispose();
                throw;
            }
        }
        catch (Exception e) when (e is not NetworkException)
        {
            if (req.Cancel.IsCancellationRequested) throw new NetworkException(NetworkFailure.Cancelled, e);
            if (idle.IsCancellationRequested) throw new NetworkException(NetworkFailure.NoAnswerInTime, e);
            var failure = Failure(e);
            if (failure == null) throw;
            throw failure;
        }
    }

    /// <summary><paramref name="inner"/>, every request introducing the app as <paramref name="userAgent"/>.</summary>
    public static Transport WithUserAgent(Transport inner, string userAgent) => req =>
    {
        var headers = new Dictionary<string, string>(StringComparer.OrdinalIgnoreCase);
        foreach (var h in req.Headers ?? new Dictionary<string, string>()) headers[h.Key] = h.Value;
        headers["User-Agent"] = userAgent;
        return inner(req with { Headers = headers });
    };

    /// <summary>The connect timeout of a request to <paramref name="url"/> that doesn't set its own.</summary>
    public static int ConnectTimeoutFor(string url) => Addresses.AtHome(url) ? HomeConnectTimeoutMs : ConnectTimeoutMs;

    static async Task<HttpResponse> Send(Func<int, HttpClient> clientFor, HttpRequest req)
    {
        if (req.Cancel.IsCancellationRequested) throw new NetworkException(NetworkFailure.Cancelled);
        var origin = Addresses.Origin(req.Url);
        var hop = req;
        for (var hops = 0; ; hops++)
        {
            var res = await Once(clientFor, hop).ConfigureAwait(false);
            if (!req.FollowRedirects || !res.IsRedirect || hops >= MaxRedirects) return res;
            var next = Redirected(hop.Url, res.Location!);
            if (next == null) return res;
            // 303, and 301 or 302 after anything but GET, go on as GET without a body, as browsers and OkHttp do.
            var asGet = res.Status == 303 || ((res.Status == 301 || res.Status == 302) && hop.Method != "GET" && hop.Method != "HEAD");
            var headers = (hop.Headers ?? new Dictionary<string, string>())
                // The token goes to the server's own address only, whatever the redirect says.
                .Where(h => Addresses.Origin(next) == origin || !h.Key.Equals("Authorization", StringComparison.OrdinalIgnoreCase))
                .Where(h => !asGet || !h.Key.Equals("Content-Type", StringComparison.OrdinalIgnoreCase))
                .ToDictionary(h => h.Key, h => h.Value);
            hop = hop with
            {
                Url = next,
                Headers = headers,
                Method = asGet ? "GET" : hop.Method,
                Body = asGet ? null : hop.Body,
            };
        }
    }

    /// <summary>Where <paramref name="location"/> sends a request to <paramref name="url"/>: null when it changes the scheme.</summary>
    static string? Redirected(string url, string location)
    {
        if (!Uri.TryCreate(url, UriKind.Absolute, out var from) || !Uri.TryCreate(from, location, out var to)) return null;
        return to.Scheme == from.Scheme && (to.Scheme == "http" || to.Scheme == "https") ? to.AbsoluteUri : null;
    }

    static async Task<HttpResponse> Once(Func<int, HttpClient> clientFor, HttpRequest req)
    {
        var connectMs = req.ConnectTimeoutMs ?? ConnectTimeoutFor(req.Url);
        using var idle = new CancellationTokenSource();
        using var both = CancellationTokenSource.CreateLinkedTokenSource(idle.Token, req.Cancel);
        // Until the answer comes, connecting and the server's silence both count.
        idle.CancelAfter(connectMs + req.TimeoutMs);
        try
        {
            using var message = new HttpRequestMessage(new HttpMethod(req.Method), req.Url);
            string? contentType = null;
            foreach (var h in req.Headers ?? new Dictionary<string, string>())
            {
                if (h.Key.Equals("Content-Type", StringComparison.OrdinalIgnoreCase)) contentType = h.Value;
                else message.Headers.TryAddWithoutValidation(h.Key, h.Value);
            }
            if (req.Body != null)
            {
                message.Content = new StringContent(req.Body, Encoding.UTF8);
                if (contentType != null) message.Content.Headers.ContentType = System.Net.Http.Headers.MediaTypeHeaderValue.Parse(contentType);
            }
            using var res = await clientFor(connectMs).SendAsync(message, HttpCompletionOption.ResponseHeadersRead, both.Token).ConfigureAwait(false);
            // A handler whose reads don't hear the token stops when the answer is let go.
            using var stop = both.Token.Register(res.Dispose);
            idle.CancelAfter(req.TimeoutMs);
            var bytes = await ReadBody(res, req.OnProgress, idle, req.TimeoutMs, both.Token).ConfigureAwait(false);
            var headers = res.Headers.Concat(res.Content.Headers).ToList();
            string? Header(string name) => headers.Where(h => h.Key.Equals(name, StringComparison.OrdinalIgnoreCase))
                .Select(h => string.Join(", ", h.Value)).FirstOrDefault();
            return req.ResponseType == ResponseType.Bytes
                ? new HttpResponse((int)res.StatusCode, Header, bytes: bytes)
                : new HttpResponse((int)res.StatusCode, Header, text: Encoding.UTF8.GetString(bytes));
        }
        catch (Exception e) when (e is not NetworkException)
        {
            if (req.Cancel.IsCancellationRequested) throw new NetworkException(NetworkFailure.Cancelled, e);
            if (idle.IsCancellationRequested) throw new NetworkException(NetworkFailure.NoAnswerInTime, e);
            var failure = Failure(e);
            if (failure == null) throw;
            throw failure;
        }
    }

    /// <summary>Reads the body, giving up when the server stays silent <paramref name="timeoutMs"/>.</summary>
    static async Task<byte[]> ReadBody(HttpResponseMessage res, Action<long>? onProgress, CancellationTokenSource idle, int timeoutMs, CancellationToken cancel)
    {
        using var stream = await res.Content.ReadAsStreamAsync().ConfigureAwait(false);
        using var memory = new MemoryStream();
        var buffer = new byte[64 * 1024];
        int n;
        while ((n = await stream.ReadAsync(buffer, 0, buffer.Length, cancel).ConfigureAwait(false)) > 0)
        {
            idle.CancelAfter(timeoutMs);
            memory.Write(buffer, 0, n);
            onProgress?.Invoke(memory.Length);
        }
        return memory.ToArray();
    }

    /// <summary>
    /// <paramref name="error"/>, thrown by HttpClient or a stream, as the network failure it is;
    /// null when it isn't one (a bug, which should show as such).
    /// </summary>
    public static NetworkException? Failure(Exception error)
    {
        for (Exception? e = error; e != null; e = e.InnerException)
        {
            switch (e)
            {
                case NetworkException n:
                    return n;
                case SocketException s:
                    return new NetworkException(s.SocketErrorCode switch
                    {
                        SocketError.ConnectionRefused or SocketError.HostUnreachable or SocketError.NetworkUnreachable or SocketError.HostDown
                            or SocketError.AddressNotAvailable => NetworkFailure.NothingAnswers,
                        SocketError.TimedOut => NetworkFailure.NoAnswerInTime,
                        SocketError.HostNotFound or SocketError.NoData or SocketError.TryAgain => NetworkFailure.UnknownAddress,
                        _ => NetworkFailure.ConnectionFailed,
                    }, error);
                case AuthenticationException:
                    return new NetworkException(NetworkFailure.SecureConnection, error);
                case TimeoutException:
                    return new NetworkException(NetworkFailure.NoAnswerInTime, error);
            }
        }
        return error switch
        {
            // A connect timeout of SocketsHttpHandler's, when nothing inside says so.
            OperationCanceledException => new NetworkException(NetworkFailure.NoAnswerInTime, error),
            // An address HttpClient can't send to fails as a connection does, as on Android.
            HttpRequestException or IOException or ObjectDisposedException or UriFormatException => new NetworkException(NetworkFailure.ConnectionFailed, error),
            _ => null,
        };
    }

    /// <summary><c>application/x-www-form-urlencoded</c> of <paramref name="fields"/>, in order.</summary>
    public static string Form(IEnumerable<KeyValuePair<string, string>> fields) =>
        string.Join("&", fields.Select(f => Uri.EscapeDataString(f.Key) + "=" + Uri.EscapeDataString(f.Value)));

    /// <summary><paramref name="path"/> with <paramref name="query"/> added, values left out when null.</summary>
    public static string WithQuery(string path, IEnumerable<KeyValuePair<string, object?>>? query)
    {
        if (query == null) return path;
        var parts = query.Where(q => q.Value != null).Select(q => Uri.EscapeDataString(q.Key) + "=" + Uri.EscapeDataString(QueryValue(q.Value!))).ToList();
        if (parts.Count == 0) return path;
        return path + (path.IndexOf('?') >= 0 ? "&" : "?") + string.Join("&", parts);
    }

    // As JavaScript's String() writes them: true and false in lower case, numbers without a culture.
    static string QueryValue(object value) => value switch
    {
        bool b => b ? "true" : "false",
        IFormattable f => f.ToString(null, System.Globalization.CultureInfo.InvariantCulture),
        _ => value.ToString() ?? "",
    };
}
