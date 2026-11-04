using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Net.Http;
using System.Net.Sockets;
using System.Text;
using System.Text.Json;
using System.Text.RegularExpressions;
using System.Threading;
using System.Threading.Tasks;
using Pley.Tv.Core.Auth;
using Pley.Tv.Core.Net;

namespace Pley.Tv.Core.Api;

/// <summary>
/// One of the server's events (docs/API.md, "Activity &amp; events"). Types: library_changed,
/// libraries_changed, sessions_changed, watchlist_changed, user_state_changed, session_stopped,
/// scan_started, scan_progress, scan_finished, live_channels_changed, live_tuners_changed,
/// live_taken_over, player_command.
/// </summary>
public sealed record ServerEvent
{
    public string Type { get; init; } = "";
    public long? LibraryId { get; init; }
    /// <summary>user_state_changed: the item, then its parent and grandparent.</summary>
    public IReadOnlyList<long>? ItemIds { get; init; }
    /// <summary>user_state_changed: everything under the first item changed too.</summary>
    public bool? Children { get; init; }
    public string? SessionId { get; init; }
    /// <summary>live_taken_over: the channel whose connection someone took over, every viewer of it ended.</summary>
    public long? ChannelId { get; init; }
    public string? Message { get; init; }
    public bool? ByAdmin { get; init; }
    /// <summary>
    /// player_command: what the profile's remote, on another device, asks of the player of
    /// <see cref="SessionId"/> ("play", "pause", "seek", "skip", "next", "previous", "audio",
    /// "subtitles"; a newer server may send others), with <see cref="PositionMs"/> for a seek,
    /// <see cref="OffsetMs"/> for a skip and <see cref="StreamIndex"/> for a track.
    /// </summary>
    public string? Command { get; init; }
    public long? PositionMs { get; init; }
    public long? OffsetMs { get; init; }
    /// <summary>player_command: the audio or subtitle track to play, by stream index; -1 turns the subtitles off.</summary>
    public int? StreamIndex { get; init; }
    /// <summary>scan_progress: "queued", "walking", "probing", "metadata" or "done".</summary>
    public string? Phase { get; init; }
    /// <summary>scan_progress: the files read so far, of <see cref="Total"/>.</summary>
    public long Processed { get; init; }
    public long Total { get; init; }
}

/// <summary>A library scan under way: how far, 0 to 1, when the server has said.</summary>
public sealed record ScanState(long LibraryId, float? Progress);

/// <summary>
/// What one connection to the event stream came to: the status (0 when nothing answered),
/// whether the server ended a stream it had opened itself (<paramref name="Clean"/>, not a
/// failure or a silence), and a refusal's WWW-Authenticate.
/// </summary>
public sealed record EventEnd(int Status, bool Clean, string? Challenge = null);

/// <summary>
/// Opens the event stream at <paramref name="url"/> with <paramref name="headers"/> (the
/// token among them): calls <paramref name="onOpen"/> once the server answers 200,
/// <paramref name="onText"/> with each piece of the body as it arrives, and answers how it ended.
/// <paramref name="cancel"/> closes it at once.
/// </summary>
public delegate Task<EventEnd> EventConnect(string url, IReadOnlyDictionary<string, string> headers, Action onOpen, Action<string> onText, CancellationToken cancel);

/// <summary>Reads Server-Sent Events from text as it arrives: each event's data lines, joined.</summary>
public sealed class SseReader
{
    static readonly Regex Blank = new Regex(@"\r?\n\r?\n", RegexOptions.CultureInvariant);
    static readonly Regex Line = new Regex(@"\r?\n", RegexOptions.CultureInvariant);
    string buffer = "";

    /// <summary>The events <paramref name="text"/> completes: those this app knows, with what each needs.</summary>
    public IReadOnlyList<ServerEvent> Feed(string text)
    {
        buffer += text;
        var blocks = Blank.Split(buffer);
        buffer = blocks[blocks.Length - 1];
        var events = new List<ServerEvent>();
        foreach (var block in blocks.Take(blocks.Length - 1))
        {
            var data = string.Join("\n", Line.Split(block)
                .Where(l => l.StartsWith("data:", StringComparison.Ordinal))
                .Select(l => l.Substring(5).StartsWith(" ") ? l.Substring(6) : l.Substring(5)));
            if (data.Length == 0) continue;
            try
            {
                var e = JsonSerializer.Deserialize<ServerEvent>(data, ApiJson.Options);
                if (e != null && Readable(e)) events.Add(e);
            }
            catch (JsonException)
            {
                // Not an event this app reads.
            }
        }
        return events;
    }

    /// <summary>
    /// Whether this app knows <paramref name="e"/>'s type and it has what that type needs: a type
    /// a newer server adds, or an event missing its library, is skipped, as on Android.
    /// </summary>
    static bool Readable(ServerEvent e) => e.Type switch
    {
        "library_changed" or "scan_started" or "scan_progress" or "scan_finished" => e.LibraryId != null,
        "session_stopped" or "player_command" => !string.IsNullOrEmpty(e.SessionId),
        "live_taken_over" => e.ChannelId != null,
        "libraries_changed" or "sessions_changed" or "watchlist_changed" or "user_state_changed" or "live_channels_changed" or "live_tuners_changed" => true,
        _ => false,
    };
}

/// <summary>
/// The server's events (<c>GET /api/events</c>), with the token in the Authorization header,
/// the only place Pley takes an app's token. The Android app's EventStream:
/// <list type="bullet">
/// <item><c>onReconnect</c> hears every connection, the first included: nothing is replayed.</item>
/// <item>A stream the server ended itself after 30 s or more (it ends streams when their token
/// expires) is opened again at once, with the refreshed token.</item>
/// <item>Any other end waits 2 s, doubling after each failure up to 30 s; a connection that
/// opened starts the count again.</item>
/// <item>A 401 whose challenge says <c>invalid_token</c> refreshes the token once and tries
/// again; a 401 after that, or any 403, stops the stream until the next <see cref="Start"/>:
/// trying again can't help.</item>
/// </list>
/// </summary>
public sealed class EventStream
{
    /// <summary>First wait after a stream fails, doubled each time up to the last.</summary>
    public const long FirstRetryMs = 2_000;
    public const long MaxRetryMs = 30_000;
    /// <summary>A stream that ran this long and then was ended by the server ended as planned (its token expired): it reconnects at once.</summary>
    public const long StableMs = 30_000;
    /// <summary>How long the stream may stay silent: the server's keep-alives come every 15 s.</summary>
    public const int ReadTimeoutMs = 45_000;

    readonly string server;
    readonly Tokens tokens;
    readonly Action<ServerEvent> onEvent;
    readonly Action onReconnect;
    readonly EventConnect connect;
    readonly IScheduler scheduler;
    readonly string? userAgent;
    CancellationTokenSource? running;
    IDisposable? timer;
    int failures;
    bool stopped = true;

    public EventStream(string server, Tokens tokens, Action<ServerEvent> onEvent, Action onReconnect, EventConnect connect, IScheduler scheduler,
        string? userAgent = null)
    {
        this.server = server;
        this.tokens = tokens;
        this.onEvent = onEvent;
        this.onReconnect = onReconnect;
        this.connect = connect;
        this.scheduler = scheduler;
        this.userAgent = userAgent;
    }

    /// <summary>Whether the stream is followed: started, and not stopped by the server's refusal since.</summary>
    public bool Running => !stopped;

    public void Start()
    {
        if (!stopped) return;
        stopped = false;
        failures = 0;
        _ = Connect(refreshed: false);
    }

    public void Stop()
    {
        stopped = true;
        timer?.Dispose();
        timer = null;
        running?.Cancel();
        running = null;
    }

    long RetryDelay()
    {
        var wait = FirstRetryMs;
        for (var i = 1; i < failures; i++) wait = Math.Min(wait * 2, MaxRetryMs);
        return Math.Min(wait, MaxRetryMs);
    }

    void Later(long ms)
    {
        if (stopped) return;
        timer = scheduler.After(ms, () =>
        {
            timer = null;
            _ = Connect(refreshed: false);
        });
    }

    void Failed()
    {
        failures++;
        Later(RetryDelay());
    }

    /// <param name="refreshed">The token was refreshed for this connection: a 401 now is the answer.</param>
    async Task Connect(bool refreshed)
    {
        if (stopped) return;
        string? token;
        try
        {
            token = await tokens.AccessToken();
        }
        catch (NetworkException)
        {
            // A refresh that couldn't reach the server: the server is away, not the sign-in.
            if (!stopped) Failed();
            return;
        }
        catch (Exception)
        {
            // Signed out, or the tokens can't be kept: nothing to follow until the next start.
            Stop();
            return;
        }
        if (stopped) return;
        if (token == null)
        {
            Stop();
            return;
        }
        var mine = new CancellationTokenSource();
        running = mine;
        long openedAt = -1;
        var reader = new SseReader();
        var headers = new Dictionary<string, string>
        {
            ["Accept"] = "text/event-stream",
            ["Authorization"] = "Bearer " + token,
        };
        if (userAgent != null) headers["User-Agent"] = userAgent;
        EventEnd end;
        try
        {
            // The stream is read off the UI thread; what it says is told there, in order.
            end = await connect($"{server}/api/events", headers, () => scheduler.Post(() =>
            {
                if (openedAt >= 0 || running != mine) return;
                openedAt = scheduler.Elapsed;
                failures = 0;
                onReconnect();
            }), text =>
            {
                var events = reader.Feed(text);
                if (events.Count > 0) scheduler.Post(() =>
                {
                    if (stopped || running != mine) return;
                    foreach (var e in events) onEvent(e);
                });
            }, mine.Token);
        }
        catch (Exception)
        {
            end = new EventEnd(0, false);
        }
        if (running != mine) return;
        running = null;
        if (stopped) return;
        // Nothing answered, or the stream broke off: the server proves itself again before the next token.
        if (end.Status == 0) tokens.DoubtServer();
        if (end.Status == 401 && !refreshed && Api.InvalidToken(end.Challenge))
        {
            string? fresh;
            try
            {
                // Refused moments after a refresh: the sign-in ends, and so does the stream.
                fresh = await tokens.RefusedAfterRefresh(token) ? null : await tokens.Refresh(token);
            }
            catch (Exception)
            {
                fresh = null;
            }
            if (stopped) return;
            if (fresh == null) Stop();
            else _ = Connect(refreshed: true);
            return;
        }
        if (end.Status is 401 or 403)
        {
            Stop();
            return;
        }
        if (end.Clean && openedAt >= 0 && scheduler.Elapsed - openedAt >= StableMs)
        {
            Later(0);
            return;
        }
        Failed();
    }

    /// <summary>
    /// The event stream over HttpClient with <paramref name="handlers"/>' handlers (Core's own
    /// without), read as it arrives: a stream silent for 45 s is let go, and cancelling closes it
    /// at once.
    /// </summary>
    public static EventConnect Over(HandlerFactory? handlers = null)
    {
        var clientFor = Transports.Clients(handlers);
        return async (url, headers, onOpen, onText, cancel) =>
        {
            var connectMs = Transports.ConnectTimeoutFor(url);
            using var idle = new CancellationTokenSource();
            using var both = CancellationTokenSource.CreateLinkedTokenSource(idle.Token, cancel);
            idle.CancelAfter(connectMs + ReadTimeoutMs);
            try
            {
                using var req = new HttpRequestMessage(HttpMethod.Get, url);
                foreach (var h in headers) req.Headers.TryAddWithoutValidation(h.Key, h.Value);
                using var res = await clientFor(connectMs).SendAsync(req, HttpCompletionOption.ResponseHeadersRead, both.Token).ConfigureAwait(false);
                var status = (int)res.StatusCode;
                if (status != 200)
                {
                    var challenge = res.Headers.WwwAuthenticate.Count > 0 ? string.Join(", ", res.Headers.WwwAuthenticate) : null;
                    return new EventEnd(status, false, challenge);
                }
                // Something else answering 200 isn't a stream to wait on.
                if (res.Content.Headers.ContentType?.MediaType != "text/event-stream") return new EventEnd(status, false);
                // A handler whose reads don't hear the token stops when the answer is let go.
                using var stop = both.Token.Register(res.Dispose);
                idle.CancelAfter(ReadTimeoutMs);
                onOpen();
                using var stream = await res.Content.ReadAsStreamAsync().ConfigureAwait(false);
                var decoder = Encoding.UTF8.GetDecoder();
                var bytes = new byte[8 * 1024];
                var chars = new char[Encoding.UTF8.GetMaxCharCount(bytes.Length)];
                int n;
                while ((n = await stream.ReadAsync(bytes, 0, bytes.Length, both.Token).ConfigureAwait(false)) > 0)
                {
                    idle.CancelAfter(ReadTimeoutMs);
                    var count = decoder.GetChars(bytes, 0, n, chars, 0);
                    if (count > 0) onText(new string(chars, 0, count));
                }
                return new EventEnd(status, true);
            }
            catch (Exception e) when (e is OperationCanceledException or HttpRequestException or IOException or ObjectDisposedException or SocketException)
            {
                return new EventEnd(0, false);
            }
        };
    }

    /// <summary>The event stream with Core's own handlers.</summary>
    public static readonly EventConnect Http = Over();
}
