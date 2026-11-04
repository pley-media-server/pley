using System;
using System.Collections.Generic;
using System.Linq;
using System.Text;
using System.Text.Json;
using System.Text.RegularExpressions;
using System.Threading;
using System.Threading.Tasks;
using Pley.Tv.Core.Auth;
using Pley.Tv.Core.Net;

namespace Pley.Tv.Core.Api;

/// <summary>An answer that isn't 2xx, with Pley's <c>{ code, message }</c>: the message is the server's sentence, written for people.</summary>
public class ApiException : Exception
{
    public ApiException(int status, string code, string message, JsonElement? body = null) : base(message)
    {
        Status = status;
        Code = code;
        Body = body;
    }

    public int Status { get; }
    public string Code { get; }
    /// <summary>The answer's JSON, when it had some (fields past <c>code</c> and <c>message</c>).</summary>
    public JsonElement? Body { get; }
}

/// <summary>A playback session stopped from elsewhere (<c>410 session_stopped</c>), with what the one who stopped it wrote.</summary>
public class SessionStoppedException : ApiException
{
    /// <summary>What the player says, the Android app's words: who stopped it, then their note on a line of its own.</summary>
    public const string ByAdmin = "Playback was stopped by the server's administrator.";

    public SessionStoppedException(string? note)
        : base(410, "session_stopped", string.IsNullOrWhiteSpace(note) ? ByAdmin : ByAdmin + "\n" + note!.Trim()) => Note = note;

    public string? Note { get; }
}

/// <summary>The server answered with something that isn't what Pley answers: another program, or a much older Pley.</summary>
public class UnreadableAnswerException : Exception
{
    public UnreadableAnswerException(int status, Exception? inner = null) : base("the server's answer can't be read: is it Pley, and up to date?", inner) =>
        Status = status;

    public int Status { get; }
}

/// <summary>How errors read to people, and whether the server answered at all: the Android app's <c>describe</c> and <c>sentenceOf</c>.</summary>
public static class Errors
{
    /// <summary>A failed page's title when the server answered nothing.</summary>
    public const string OfflineTitle = "Can't reach your server";

    /// <summary>Why <paramref name="e"/> happened, in a few words in lower case: "no answer in time".</summary>
    public static string Describe(Exception e) => e switch
    {
        JsonException => "that isn't a Pley server",
        _ => e.Message,
    };

    /// <summary><paramref name="text"/> as a sentence: a capital first, a full stop last (the app's own texts come in lower case, without one).</summary>
    public static string AsSentence(string text)
    {
        var t = text.Trim();
        if (t.Length == 0) return t;
        var capital = char.ToUpperInvariant(t[0]) + t.Substring(1);
        return ".…!?".IndexOf(capital[capital.Length - 1]) >= 0 ? capital : capital + ".";
    }

    /// <summary>What to show for an error, as a sentence: "No answer in time."</summary>
    public static string Message(Exception e) => AsSentence(Describe(e));

    /// <summary>The server answered nothing: no connection, or no answer in time. A refusal, a 502 among them, is an answer.</summary>
    public static bool Unreachable(Exception e) => e is NetworkException { Cancelled: false };

    /// <summary>Why the server answered nothing, as the Android app says it: "Nothing answers there." or "No answer in time."</summary>
    public static string UnreachableReason(Exception e) => Message(e);

    /// <summary>A failed page's text when the server answered nothing: why, and what to check.</summary>
    public static string OfflineBody(Exception e) => $"{Message(e)} Check that the server is on and that this device is on its network.";
}

// Answers whose shape is an envelope around the types.
public sealed record HubsAnswer(IReadOnlyList<Hub> Hubs);
public sealed record ItemsPage(long Total, IReadOnlyList<ItemSummary> Items);
public sealed record ItemsAnswer(IReadOnlyList<ItemSummary> Items);
public sealed record ItemAnswer(ItemSummary? Item);
/// <summary>
/// What a library's listing can be filtered by. <see cref="Resolutions"/> and <see cref="DynamicRanges"/>
/// hold the server's tokens, best first, and only when the titles have two or more; a server from
/// before them sends neither.
/// </summary>
public sealed record FiltersAnswer(IReadOnlyList<string> Genres, IReadOnlyList<int> Years)
{
    public IReadOnlyList<string> Resolutions { get; init; } = Array.Empty<string>();
    public IReadOnlyList<string> DynamicRanges { get; init; } = Array.Empty<string>();
}
public sealed record CollectionsAnswer(IReadOnlyList<Collection> Collections);
public sealed record CollectionDetail : Collection
{
    public IReadOnlyList<ItemSummary> Items { get; init; } = new ItemSummary[0];
}
public sealed record PlaylistsAnswer(IReadOnlyList<Playlist> Playlists);
public sealed record PlaylistDetail : Playlist
{
    public IReadOnlyList<PlaylistEntry> Entries { get; init; } = new PlaylistEntry[0];
}
/// <summary>Live TV's <see cref="Channels"/> by name, number or what's on, for those it's open to; none from a server without it.</summary>
public sealed record SearchAnswer(IReadOnlyList<ItemSummary> Items, IReadOnlyList<PersonSummary>? People, IReadOnlyList<LiveChannel>? Channels = null);
public sealed record CollectionRef(long Id, string Title);
public sealed record ItemCollectionsAnswer(IReadOnlyList<CollectionRef> Collections);
public sealed record AddedAnswer(long Added);
public sealed record HistoryPage(long Total, IReadOnlyList<HistoryEntry> Entries);

/// <summary>Query values of <see cref="Api.LibraryItems"/>; null ones are left out.</summary>
public sealed record LibraryQuery
{
    public string? Sort { get; init; }
    /// <summary>"asc" or "desc".</summary>
    public string? Order { get; init; }
    public bool? Watched { get; init; }
    public string? Genre { get; init; }
    public int? Year { get; init; }
    /// <summary>"4k", "1080", "720" or "sd": a title's best file's.</summary>
    public string? Resolution { get; init; }
    /// <summary>"hdr" (any but SDR), "dolby_vision", "hdr10plus", "hdr10", "hlg" or "sdr": a title's best file's.</summary>
    public string? DynamicRange { get; init; }
    /// <summary>Always sent, as the Android app sends it: the first page is at 0.</summary>
    public int Offset { get; init; }
    public int? Limit { get; init; }
}

/// <summary>
/// Pley's API for one server, as one signed-in app: every request carries the access token in
/// <c>Authorization: Bearer</c> (the only place Pley takes an app's token) to the server's own
/// address only, refreshed once on <c>invalid_token</c>. The Android app's <c>HttpPleyApi</c>.
/// </summary>
public sealed class Api
{
    /// <summary>The most of a refusal's body read: they are a sentence.</summary>
    const int MaxErrorBody = 16 * 1024;

    /// <summary>The longest message kept from a refusal.</summary>
    const int MaxErrorText = 300;

    static readonly Regex Absolute = new Regex(@"^https?://", RegexOptions.CultureInvariant);
    static readonly Regex InvalidTokenParam = new Regex(@"(^|[\s,])error\s*=\s*""?invalid_token""?(\s*,|\s*$)", RegexOptions.CultureInvariant | RegexOptions.IgnoreCase);

    /// <summary>Request bodies: written as given, nulls included (a null clears a field; a field left out keeps it).</summary>
    static readonly JsonSerializerOptions BodyOptions = new JsonSerializerOptions();

    readonly Tokens tokens;
    readonly Transport http;
    readonly StreamTransport streams;

    public Api(string server, Tokens tokens, Transport http, StreamTransport? streams = null)
    {
        Server = server;
        this.tokens = tokens;
        this.http = http;
        this.streams = streams ?? Transports.Streams;
    }

    public string Server { get; }

    /// <summary>An absolute address on this server for <paramref name="path"/> ("/api/…").</summary>
    public string Url(string path) => Absolute.IsMatch(path) ? path : Server + path;

    /// <summary>
    /// Whether <paramref name="path"/> is on this server: an address the server hands out (a
    /// picture's, a playlist's) that names another origin gets no token, as Android's (SECURITY.md).
    /// </summary>
    bool OwnServer(string path) => Addresses.Origin(Url(path)) == Addresses.Origin(Server);

    /// <summary>
    /// The raw answer of an authenticated request; errors as <see cref="ApiException"/>.
    /// <paramref name="timeoutMs"/> is how long the server may stay silent (10 s without).
    /// </summary>
    public async Task<HttpResponse> Send(string path, string method = "GET", object? body = null, IEnumerable<KeyValuePair<string, object?>>? query = null,
        CancellationToken cancel = default, int? timeoutMs = null, ResponseType responseType = ResponseType.Json, Action<long>? onProgress = null, string? accept = null)
    {
        HttpResponse res;
        try
        {
            var token = await tokens.AccessToken().ConfigureAwait(false) ?? throw new SignedOutException();
            res = await Once(path, method, body, query, cancel, timeoutMs, responseType, onProgress, token, accept).ConfigureAwait(false);
            if (res.Status == 401 && OwnServer(path) && InvalidToken(res.Header("WWW-Authenticate")))
            {
                var fresh = await Refreshed(token).ConfigureAwait(false);
                // Once: a 401 after a refresh is the answer.
                if (fresh != null) res = await Once(path, method, body, query, cancel, timeoutMs, responseType, onProgress, fresh, accept).ConfigureAwait(false);
            }
        }
        catch (NetworkException e) when (!e.Cancelled && OwnServer(path))
        {
            Unanswered();
            throw;
        }
        if (res.Status >= 200 && res.Status < 300) return res;
        throw ToError(res);
    }

    /// <summary>
    /// A request to the server got no answer: the TV may be on another network now, so what
    /// answers at the address next proves it is the server before a token goes.
    /// </summary>
    void Unanswered() => tokens.DoubtServer();

    /// <summary>
    /// GET <paramref name="path"/> with the token, its body read as it comes for as long as it lasts
    /// (a live channel's MPEG-TS); refused, an <see cref="ApiException"/>, as <see cref="Send"/>'s.
    /// The server may stay silent <paramref name="timeoutMs"/> before it answers (null: the API's).
    /// </summary>
    public async Task<StreamResponse> Open(string path, CancellationToken cancel = default, string accept = "*/*", int? timeoutMs = null, string? range = null)
    {
        StreamResponse res;
        try
        {
            var token = await tokens.AccessToken().ConfigureAwait(false) ?? throw new SignedOutException();
            res = await OpenOnce(path, token, cancel, accept, timeoutMs, range).ConfigureAwait(false);
            if (res.Status == 401 && OwnServer(path) && InvalidToken(res.Header("WWW-Authenticate")))
            {
                var fresh = await Refreshed(token).ConfigureAwait(false);
                if (fresh != null)
                {
                    res.Dispose();
                    res = await OpenOnce(path, fresh, cancel, accept, timeoutMs, range).ConfigureAwait(false);
                }
            }
        }
        catch (NetworkException e) when (!e.Cancelled && OwnServer(path))
        {
            Unanswered();
            throw;
        }
        if (res.Status >= 200 && res.Status < 300) return res;
        using (res)
        {
            // A refusal is a sentence: as much of it as an error keeps.
            var bytes = new byte[MaxErrorBody];
            var length = 0;
            int n;
            while (length < bytes.Length && (n = await res.Body.ReadAsync(bytes, length, bytes.Length - length, cancel).ConfigureAwait(false)) > 0) length += n;
            throw ToError(new HttpResponse(res.Status, res.Header, text: Encoding.UTF8.GetString(bytes, 0, length)));
        }
    }

    Task<StreamResponse> OpenOnce(string path, string token, CancellationToken cancel, string accept, int? timeoutMs, string? range = null)
    {
        var headers = new Dictionary<string, string> { ["Accept"] = accept };
        if (range != null) headers["Range"] = range;
        if (OwnServer(path)) headers["Authorization"] = "Bearer " + token;
        return streams(new HttpRequest { Url = Url(path), Headers = headers, Cancel = cancel, TimeoutMs = timeoutMs ?? Transports.ReadTimeoutMs });
    }

    /// <summary>
    /// A new token in place of <paramref name="sent"/>, which the server refused; null when the
    /// refusal is the answer: refused moments after a refresh (the sign-in ends), or a refresh
    /// that failed.
    /// </summary>
    async Task<string?> Refreshed(string sent)
    {
        try
        {
            if (await tokens.RefusedAfterRefresh(sent).ConfigureAwait(false)) return null;
            return await tokens.Refresh(sent).ConfigureAwait(false);
        }
        catch (Exception e) when (e is AuthException or NetworkException)
        {
            return null;
        }
    }

    /// <summary>Whether a WWW-Authenticate value is a Bearer challenge with error="invalid_token" (RFC 6750 §3).</summary>
    public static bool InvalidToken(string? header)
    {
        var h = (header ?? "").Trim();
        if (!h.StartsWith("Bearer", StringComparison.OrdinalIgnoreCase) || (h.Length > 6 && !char.IsWhiteSpace(h[6]))) return false;
        return InvalidTokenParam.IsMatch(h.Substring(6));
    }

    Task<HttpResponse> Once(string path, string method, object? body, IEnumerable<KeyValuePair<string, object?>>? query, CancellationToken cancel,
        int? timeoutMs, ResponseType responseType, Action<long>? onProgress, string token, string? accept)
    {
        var headers = new Dictionary<string, string> { ["Accept"] = accept ?? "application/json" };
        if (OwnServer(path)) headers["Authorization"] = "Bearer " + token;
        string? text = null;
        if (body != null)
        {
            headers["Content-Type"] = "application/json";
            text = JsonSerializer.Serialize(body, BodyOptions);
        }
        return http(new HttpRequest
        {
            Method = method,
            Url = Transports.WithQuery(Url(path), query),
            Headers = headers,
            Body = text,
            ResponseType = responseType,
            TimeoutMs = timeoutMs ?? Transports.ReadTimeoutMs,
            // The API's redirects are followed (a collection's picture is its item's); the token stays on this server.
            FollowRedirects = true,
            Cancel = cancel,
            OnProgress = onProgress,
        });
    }

    public async Task<T> Get<T>(string path, IEnumerable<KeyValuePair<string, object?>>? query = null, CancellationToken cancel = default, int? timeoutMs = null)
    {
        var res = await Send(path, query: query, cancel: cancel, timeoutMs: timeoutMs).ConfigureAwait(false);
        return Read<T>(res);
    }

    public async Task<JsonElement?> Call(string method, string path, object? body = null, CancellationToken cancel = default)
    {
        var res = await Send(path, method, body, cancel: cancel).ConfigureAwait(false);
        return res.Json();
    }

    async Task<T> Call<T>(string method, string path, object? body = null, CancellationToken cancel = default) =>
        Read<T>(await Send(path, method, body, cancel: cancel).ConfigureAwait(false));

    static T Read<T>(HttpResponse res)
    {
        try
        {
            return JsonSerializer.Deserialize<T>(res.Text ?? "null", ApiJson.Options) ?? throw new UnreadableAnswerException(res.Status);
        }
        catch (JsonException e)
        {
            throw new UnreadableAnswerException(res.Status, e);
        }
    }

    static KeyValuePair<string, object?> Q(string key, object? value) => new KeyValuePair<string, object?>(key, value);

    // ---- account

    public Task<User> Me(CancellationToken cancel = default) => Get<User>("/api/auth/me", cancel: cancel);

    public Task<List<HouseholdProfile>> Profiles(CancellationToken cancel = default) => Get<List<HouseholdProfile>>("/api/oauth/profiles", cancel: cancel);

    public Task<JsonElement?> SetTrackDefaults(string? audio, string? subtitle, string mode) =>
        Call("PUT", "/api/me/track-defaults", new Dictionary<string, object?> { ["audio_language"] = audio, ["subtitle_language"] = subtitle, ["subtitle_mode"] = mode });

    // ---- browsing

    public Task<HubsAnswer> Home(CancellationToken cancel = default) => Get<HubsAnswer>("/api/hubs/home", cancel: cancel);

    public Task<List<Library>> Libraries(CancellationToken cancel = default) => Get<List<Library>>("/api/libraries", cancel: cancel);

    /// <summary>The profile's own order of its libraries, <paramref name="ids"/> naming each once (scope play); them in it.</summary>
    public Task<List<Library>> SortLibraries(IReadOnlyList<long> ids) => Call<List<Library>>("PUT", "/api/me/libraries/order", new Dictionary<string, object?> { ["ids"] = ids });

    /// <summary>The order everyone sees, for this profile again (scope play); the libraries in it.</summary>
    public Task<List<Library>> ForgetLibraryOrder() => Call<List<Library>>("DELETE", "/api/me/libraries/order");

    public Task<ItemsPage> LibraryItems(long id, LibraryQuery query, CancellationToken cancel = default) =>
        Get<ItemsPage>($"/api/libraries/{id}/items", new[]
        {
            // In this order: the server's listing ETag hashes the query as written.
            Q("sort", query.Sort), Q("order", query.Order), Q("watched", query.Watched), Q("genre", query.Genre),
            Q("year", query.Year), Q("resolution", query.Resolution), Q("dynamic_range", query.DynamicRange),
            Q("offset", query.Offset), Q("limit", query.Limit),
        }, cancel);

    public Task<HubsAnswer> LibraryHubs(long id, CancellationToken cancel = default) => Get<HubsAnswer>($"/api/libraries/{id}/hubs", cancel: cancel);

    public Task<FiltersAnswer> LibraryFilters(long id, CancellationToken cancel = default) => Get<FiltersAnswer>($"/api/libraries/{id}/filters", cancel: cancel);

    public Task<CollectionsAnswer> Collections(long libraryId, CancellationToken cancel = default) => Get<CollectionsAnswer>($"/api/libraries/{libraryId}/collections", cancel: cancel);

    public Task<CollectionDetail> Collection(long id, CancellationToken cancel = default) => Get<CollectionDetail>($"/api/collections/{id}", cancel: cancel);

    public Task<ItemDetail> Item(long id, CancellationToken cancel = default) => Get<ItemDetail>($"/api/items/{id}", cancel: cancel);

    public Task<ItemsAnswer> Children(long id, CancellationToken cancel = default) => Get<ItemsAnswer>($"/api/items/{id}/children", cancel: cancel);

    public Task<ItemAnswer> OnDeck(long id, CancellationToken cancel = default) => Get<ItemAnswer>($"/api/items/{id}/on-deck", cancel: cancel);

    public Task<ItemAnswer> Next(long id, CancellationToken cancel = default) => Get<ItemAnswer>($"/api/items/{id}/next", cancel: cancel);

    public Task<ItemsAnswer> Related(long id, CancellationToken cancel = default) => Get<ItemsAnswer>($"/api/items/{id}/related", new[] { Q("limit", 20) }, cancel);

    public Task<PersonDetail> Person(long id, CancellationToken cancel = default) => Get<PersonDetail>($"/api/people/{id}", cancel: cancel);

    /// <summary>Up to 200 items and 200 people, the server's most, which search's rows show all of; as many Live TV channels come with Live TV.</summary>
    public Task<SearchAnswer> Search(string q, CancellationToken cancel = default) => Get<SearchAnswer>("/api/search", new[] { Q("q", q), Q("limit", 200) }, cancel);

    public Task<PlaylistsAnswer> Playlists(CancellationToken cancel = default) => Get<PlaylistsAnswer>("/api/playlists", cancel: cancel);

    public Task<PlaylistDetail> Playlist(long id, CancellationToken cancel = default) => Get<PlaylistDetail>($"/api/playlists/{id}", cancel: cancel);

    /// <summary>The collections an item is part of.</summary>
    public Task<ItemCollectionsAnswer> ItemCollections(long id, CancellationToken cancel = default) => Get<ItemCollectionsAnswer>($"/api/items/{id}/collections", cancel: cancel);

    public Task<AddedAnswer> AddToPlaylist(long playlistId, IReadOnlyList<long> itemIds) =>
        Call<AddedAnswer>("POST", $"/api/playlists/{playlistId}/items", new Dictionary<string, object?> { ["item_ids"] = itemIds });

    /// <param name="kind">"video", "audio", or null for the server to tell from the items.</param>
    public Task<Playlist> CreatePlaylist(string title, string? kind, IReadOnlyList<long> itemIds)
    {
        var body = new Dictionary<string, object?> { ["title"] = title };
        if (kind != null) body["kind"] = kind;
        body["item_ids"] = itemIds;
        return Call<Playlist>("POST", "/api/playlists", body);
    }

    /// <summary>The playlist renamed, with its entries.</summary>
    public Task<PlaylistDetail> RenamePlaylist(long id, string title) =>
        Call<PlaylistDetail>("PATCH", $"/api/playlists/{id}", new Dictionary<string, object?> { ["title"] = title });

    public Task<JsonElement?> DeletePlaylist(long id) => Call("DELETE", $"/api/playlists/{id}");

    /// <summary>Takes one entry out (the item stays in its library).</summary>
    public Task<JsonElement?> RemoveFromPlaylist(long id, long entryId) => Call("DELETE", $"/api/playlists/{id}/items/{entryId}");

    /// <summary>The entries in their new order (these first, the rest after); the playlist in it.</summary>
    public Task<PlaylistDetail> ReorderPlaylist(long id, IReadOnlyList<long> entryIds) =>
        Call<PlaylistDetail>("PUT", $"/api/playlists/{id}/order", new Dictionary<string, object?> { ["entry_ids"] = entryIds });

    /// <summary>A smart playlist: a library's listing by <paramref name="filter"/> (Plex's syntax), which keeps up with the library by itself.</summary>
    public Task<Playlist> CreateSmartPlaylist(string title, long libraryId, string filter) =>
        Call<Playlist>("POST", "/api/playlists", new Dictionary<string, object?> { ["title"] = title, ["smart_filter"] = filter, ["library_id"] = libraryId });

    // ---- watching

    public Task<HistoryPage> History(int offset, int limit, CancellationToken cancel = default) =>
        Get<HistoryPage>("/api/history", new[] { Q("offset", offset), Q("limit", limit) }, cancel);

    public Task<JsonElement?> RemoveHistory(long id) => Call("DELETE", $"/api/history/{id}");

    public Task<JsonElement?> AddToMyList(long id) => Call("PUT", $"/api/watchlist/{id}");

    public Task<JsonElement?> RemoveFromMyList(long id) => Call("DELETE", $"/api/watchlist/{id}");

    public Task<JsonElement?> HideFromContinueWatching(long id) => Call("DELETE", $"/api/continue-watching/{id}");

    /// <summary>The person's rating, 0–10 (halves), or none.</summary>
    public Task<JsonElement?> Rate(long id, double? rating) =>
        Call("PUT", $"/api/items/{id}/rating", rating == null ? new Dictionary<string, object?>() : new Dictionary<string, object?> { ["rating"] = rating });

    public Task<JsonElement?> MarkWatched(long id) => Call("POST", $"/api/items/{id}/watched");

    public Task<JsonElement?> MarkUnwatched(long id) => Call("POST", $"/api/items/{id}/unwatched");

    /// <summary>
    /// The tracks picked for an item (the whole show for an episode). <paramref name="keepAudio"/>
    /// leaves the audio picked before as it is.
    /// </summary>
    public Task<JsonElement?> SetStreams(long itemId, long? fileId, int? audio, int? subtitle, bool keepAudio = false)
    {
        var body = new Dictionary<string, object?>();
        if (fileId != null) body["file_id"] = fileId;
        if (!keepAudio) body["audio_index"] = audio;
        body["subtitle_index"] = subtitle;
        return Call("PUT", $"/api/items/{itemId}/streams", body);
    }

    // ---- playback

    /// <summary>
    /// POST /api/playback: a session for <paramref name="request"/> (its JSON body, snake_case).
    /// The server may start a conversion first, so it has a minute.
    /// </summary>
    public async Task<PlaybackInfo> StartPlayback(IReadOnlyDictionary<string, object?> request, CancellationToken cancel = default) =>
        Read<PlaybackInfo>(await Send("/api/playback", "POST", request, cancel: cancel, timeoutMs: 60_000).ConfigureAwait(false));

    /// <summary>DELETE /api/playback/{session}: the session and its conversion end.</summary>
    public Task<JsonElement?> StopPlayback(string session) => Call("DELETE", $"/api/playback/{Uri.EscapeDataString(session)}");

    /// <summary>POST /api/items/{id}/progress, with the session, so it and its conversion stay alive.</summary>
    public Task<JsonElement?> Progress(long itemId, long positionMs, long? durationMs, string state, string? session)
    {
        var body = new Dictionary<string, object?> { ["position_ms"] = positionMs, ["state"] = state };
        if (durationMs != null) body["duration_ms"] = durationMs;
        if (session != null) body["session_id"] = session;
        return Call("POST", $"/api/items/{itemId}/progress", body);
    }

    /// <summary>
    /// GET /api/playback/{session}: how it plays now, with its transcode's live numbers. Gone, it is
    /// a 404; stopped from elsewhere, a <see cref="SessionStoppedException"/>.
    /// </summary>
    public Task<SessionStatus> PlaybackStatus(string session, CancellationToken cancel = default, int? timeoutMs = null) =>
        Get<SessionStatus>($"/api/playback/{Uri.EscapeDataString(session)}", cancel: cancel, timeoutMs: timeoutMs);

    /// <summary>
    /// PUT /api/playback/{session}/bitrate: a converted stream (<see cref="PlaybackInfo.LiveBitrate"/>)
    /// goes on at <paramref name="kbps"/> from the segments not yet fetched; the answer is what it goes on at.
    /// </summary>
    public Task<BitrateAnswer> SetBitrate(string session, long kbps) =>
        Call<BitrateAnswer>("PUT", $"/api/playback/{Uri.EscapeDataString(session)}/bitrate", new Dictionary<string, object?> { ["max_bitrate"] = kbps });

    /// <summary>
    /// A subtitle track's WebVTT at <paramref name="url"/> (the session's <see cref="SubtitleTrack.Url"/>).
    /// The transcoder may still be reading it out of its file: it has two minutes, and an answer
    /// marked <c>X-Pley-Complete: 0</c> is a part, to ask for again.
    /// </summary>
    public async Task<SubtitleText> Subtitles(string url, CancellationToken cancel = default)
    {
        var res = await Send(url, cancel: cancel, timeoutMs: 120_000, responseType: ResponseType.Text, accept: "text/vtt").ConfigureAwait(false);
        return new SubtitleText(res.Text ?? "", (res.Header("X-Pley-Complete") ?? "").Trim() != "0");
    }

    /// <summary>
    /// The server's .mks of picture subtitle track <paramref name="url"/>
    /// (<c>/api/files/{id}/subtitles/{n}.mks</c>). The server reads the whole file to make it the
    /// first time, minutes for a big one on a network share, so it may stay silent before it
    /// answers: <see cref="PicturesTimeoutMs"/>. An answer marked <c>X-Pley-Complete: 0</c> is a
    /// part, to ask for again. An older server answers 404, an older transcoder 503.
    /// </summary>
    public async Task<SubtitlePictures> PictureSubtitles(string url, CancellationToken cancel = default)
    {
        // Read as it comes into one array of its length: gathered in a growing buffer and copied
        // out, a film's track took three times its size of .NET's heap, tens of MB on a 32-bit TV.
        using var res = await Open(url, cancel, "video/x-matroska", PicturesTimeoutMs).ConfigureAwait(false);
        var bytes = await ReadAll(res, cancel).ConfigureAwait(false);
        return new SubtitlePictures(bytes, (res.Header("X-Pley-Complete") ?? "").Trim() != "0");
    }

    /// <summary>
    /// How long the server may stay silent before it answers with a picture track's .mks: a minute,
    /// as Android's subtitles client allows. The API's 10 s left a track the server was still
    /// making "couldn't be loaded".
    /// </summary>
    public const int PicturesTimeoutMs = 60_000;

    /// <summary>
    /// Bytes <paramref name="from"/> to <paramref name="to"/> (inclusive) of a picture track's .mks:
    /// a whole one is served from the server's cache with ranges, so the pictures around where the
    /// film is can be read without the rest (<see cref="Player.PictureRanges"/>). A server that
    /// answers whole (200) gives the file from its start; one still making it says so
    /// (<see cref="SubtitlePart.Complete"/>).
    /// </summary>
    public async Task<SubtitlePart> PictureSubtitlesPart(string url, long from, long to, CancellationToken cancel = default)
    {
        using var res = await Open(url, cancel, "video/x-matroska", PicturesTimeoutMs, $"bytes={from}-{to}").ConfigureAwait(false);
        var bytes = await ReadAll(res, cancel).ConfigureAwait(false);
        var complete = (res.Header("X-Pley-Complete") ?? "").Trim() != "0";
        if (res.Status == 206 && ContentRange(res.Header("Content-Range")) is { } range) return new SubtitlePart(bytes, range.Start, range.Total, complete);
        return new SubtitlePart(bytes, 0, complete ? bytes.Length : null, complete);
    }

    /// <summary>"bytes 0-65535/20997000": where the part starts and the whole's length (null: "*", not known yet); null for a header that isn't one.</summary>
    internal static (long Start, long? Total)? ContentRange(string? header)
    {
        var m = ContentRangeForm.Match(header ?? "");
        if (!m.Success || !long.TryParse(m.Groups[1].Value, System.Globalization.NumberStyles.None, System.Globalization.CultureInfo.InvariantCulture, out var start)) return null;
        if (m.Groups[3].Value == "*") return (start, null);
        return long.TryParse(m.Groups[3].Value, System.Globalization.NumberStyles.None, System.Globalization.CultureInfo.InvariantCulture, out var total) ? (start, total) : null;
    }

    static readonly System.Text.RegularExpressions.Regex ContentRangeForm = new System.Text.RegularExpressions.Regex(@"^\s*bytes\s+(\d+)-(\d+)/(\d+|\*)\s*$", System.Text.RegularExpressions.RegexOptions.CultureInvariant);

    /// <summary>The whole body of <paramref name="res"/>: into one array when its length is said, else gathered.</summary>
    static async Task<byte[]> ReadAll(StreamResponse res, CancellationToken cancel)
    {
        if (long.TryParse(res.Header("Content-Length"), out var length) && length >= 0 && length <= int.MaxValue)
        {
            var bytes = new byte[length];
            var at = 0;
            int n;
            while (at < bytes.Length && (n = await res.Body.ReadAsync(bytes, at, bytes.Length - at, cancel).ConfigureAwait(false)) > 0) at += n;
            if (at == bytes.Length) return bytes;
            // Shorter than it said: what came.
            var came = new byte[at];
            Array.Copy(bytes, came, at);
            return came;
        }
        using var gathered = new System.IO.MemoryStream();
        await res.Body.CopyToAsync(gathered, 81_920, cancel).ConfigureAwait(false);
        return gathered.ToArray();
    }

    /// <summary>GET /api/files/{id}/trickplay: the file's seek previews; null while the server makes them (its 404).</summary>
    public async Task<TrickplayInfo?> Trickplay(long fileId, CancellationToken cancel = default)
    {
        try
        {
            return await Get<TrickplayInfo>($"/api/files/{fileId}/trickplay", cancel: cancel).ConfigureAwait(false);
        }
        catch (ApiException e) when (e.Status == 404)
        {
            return null;
        }
    }

    /// <summary>Where sheet <paramref name="sheet"/> of a file's previews is, for the set <paramref name="version"/> (<see cref="TrickplayInfo.Id"/>).</summary>
    public string TrickplaySheetUrl(long fileId, int sheet, string version) =>
        Url($"/api/files/{fileId}/trickplay/{sheet}.jpg?v={Uri.EscapeDataString(version)}");

    /// <summary>Whether the server answers at all: a player reconnecting without a session to ask about.</summary>
    public Task<JsonElement?> Reachable(int timeoutMs, CancellationToken cancel = default) =>
        Send("/api/server", cancel: cancel, timeoutMs: timeoutMs).ContinueWith(t => t.GetAwaiter().GetResult().Json(), cancel, TaskContinuationOptions.ExecuteSynchronously, TaskScheduler.Default);

    // ---- live TV (docs/API.md, "Live TV")

    /// <summary>The longest the server takes to tune a channel before it answers.</summary>
    const int WatchTimeoutMs = 45_000;

    public Task<LiveSummary> LiveSummary(CancellationToken cancel = default) => Get<LiveSummary>("/api/live", cancel: cancel);

    public Task<LiveGroups> LiveGroups(CancellationToken cancel = default) => Get<LiveGroups>("/api/live/groups", cancel: cancel);

    /// <summary>A page of <paramref name="category"/>'s channels matching <paramref name="q"/>; with <paramref name="around"/>, the page that holds that channel.</summary>
    public Task<LiveChannelPage> LiveBrowse(LiveCategory category, string? q = null, int offset = 0, int limit = 100, long? around = null, CancellationToken cancel = default)
    {
        var words = q?.Trim();
        return Get<LiveChannelPage>("/api/live/browse", new[]
        {
            Q("category", category.Wire),
            Q("group", (category as LiveCategory.Group)?.Name),
            Q("q", string.IsNullOrEmpty(words) ? null : words),
            Q("offset", around == null ? offset : (object?)null),
            Q("limit", limit),
            Q("around", around),
        }, cancel);
    }

    public Task<LiveChannel> LiveChannel(long id, CancellationToken cancel = default) => Get<LiveChannel>($"/api/live/channels/{id}", cancel: cancel);

    /// <summary>The programmes of channels <paramref name="ids"/> between <paramref name="from"/> and <paramref name="to"/> (unix seconds).</summary>
    public Task<LiveGuide> LiveGuide(long from, long to, IReadOnlyList<long> ids, CancellationToken cancel = default) =>
        Get<LiveGuide>("/api/live/guide", new[]
        {
            Q("from", from), Q("to", to), Q("ids", string.Join(",", ids.Select(i => i.ToString(System.Globalization.CultureInfo.InvariantCulture)))),
        }, cancel);

    public Task<JsonElement?> SetFavoriteChannel(long id, bool favorite) =>
        Call(favorite ? "PUT" : "DELETE", $"/api/live/channels/{id}/favorite");

    /// <summary>
    /// POST /api/live/channels/{id}/watch with keyed URLs (the TV's player can't send the
    /// Authorization header): <paramref name="clientId"/> ends what this TV watched before.
    /// <paramref name="takeOver"/>: every connection in use, the channel whose connection to take.
    /// </summary>
    public async Task<LiveWatching> WatchChannel(long id, string clientId, long? takeOver = null, CancellationToken cancel = default)
    {
        var body = new Dictionary<string, object?> { ["client_id"] = clientId, ["keyed_urls"] = true };
        if (takeOver != null) body["take_over"] = takeOver;
        return Read<LiveWatching>(await Send($"/api/live/channels/{id}/watch", "POST", body, cancel: cancel, timeoutMs: WatchTimeoutMs).ConfigureAwait(false));
    }

    /// <summary>
    /// Why a watching session ended (someone took its connection over, say), or null while it goes
    /// on. Asked of its plan with the token: a keyed address only says the session has gone.
    /// </summary>
    public async Task<string?> WhyLiveEnded(string session, CancellationToken cancel = default)
    {
        try
        {
            await Send($"/api/live/sessions/{Uri.EscapeDataString(session)}/plan", cancel: cancel, timeoutMs: 8_000).ConfigureAwait(false);
            return null;
        }
        catch (SessionStoppedException e)
        {
            // The server's words, or "" when it gave none: not a video's "stopped by the administrator".
            return e.Note?.Trim() ?? "";
        }
        catch (ApiException e) when (e.Status == 410)
        {
            return e.Message;
        }
    }

    /// <summary>
    /// GET /api/live/sessions/{session}/plan: what the session's channel is made into, the server
    /// waiting up to 3 s to know; null when it didn't know in time.
    /// </summary>
    public async Task<LivePlan?> LivePlan(string session, CancellationToken cancel = default)
    {
        var res = await Send($"/api/live/sessions/{Uri.EscapeDataString(session)}/plan", cancel: cancel, timeoutMs: 8_000).ConfigureAwait(false);
        try
        {
            return JsonSerializer.Deserialize<LivePlan?>(res.Text ?? "null", ApiJson.Options);
        }
        catch (JsonException e)
        {
            throw new UnreadableAnswerException(res.Status, e);
        }
    }

    /// <summary>Stops watching now; a session already gone (left by itself, stopped by an administrator) is left all the same.</summary>
    public async Task LeaveChannel(string session)
    {
        try
        {
            await Call("DELETE", $"/api/live/sessions/{Uri.EscapeDataString(session)}").ConfigureAwait(false);
        }
        catch (ApiException e) when (e.Status == 404)
        {
            // Gone already.
        }
    }

    /// <summary>
    /// A playlist or segment at <paramref name="url"/>, as the TV's player would ask for it: a keyed
    /// address goes without the token (its key is its own), anything else with it.
    /// </summary>
    public async Task<string> Media(string url, int timeoutMs, CancellationToken cancel = default)
    {
        var full = Url(url);
        if (!KeyedMedia.IsMatch(full))
            return (await Send(full, cancel: cancel, timeoutMs: timeoutMs, responseType: ResponseType.Text).ConfigureAwait(false)).Text ?? "";
        var res = await http(new HttpRequest
        {
            Url = full,
            Headers = new Dictionary<string, string> { ["Accept"] = "*/*" },
            ResponseType = ResponseType.Text,
            TimeoutMs = timeoutMs,
            Cancel = cancel,
        }).ConfigureAwait(false);
        if (res.Status >= 200 && res.Status < 300) return res.Text ?? "";
        throw ToError(res);
    }

    static readonly Regex KeyedMedia = new Regex(@"^(https?://[^/]+)?/api/(live/)?media/", RegexOptions.CultureInvariant);

    /// <summary>
    /// The refusal <paramref name="res"/> is: Pley's JSON, or the plain text a body or query the
    /// server couldn't read gets (400, 415, 422), as one short line; never a page's HTML.
    /// </summary>
    static ApiException ToError(HttpResponse res)
    {
        var text = res.Text ?? "";
        if (text.Length > MaxErrorBody) text = text.Substring(0, MaxErrorBody);
        var start = text.TrimStart();
        JsonElement? json = null;
        if (start.StartsWith("{", StringComparison.Ordinal))
        {
            try
            {
                using var doc = JsonDocument.Parse(text);
                json = doc.RootElement.Clone();
            }
            catch (JsonException)
            {
                json = null;
            }
        }
        var bodyMessage = json is { } o ? Plain.FirstLine(Json.String(o, "message"), MaxErrorText) : "";
        var code = (json is { } c ? Json.String(c, "code") : null) is { Length: > 0 } given ? given : StatusCode(res.Status);
        var message = bodyMessage.Length > 0 ? bodyMessage
            : json == null && !start.StartsWith("<", StringComparison.Ordinal) && Plain.FirstLine(text, MaxErrorText) is { Length: > 0 } line ? line
            : $"the server refused (HTTP {res.Status})";
        if (res.Status == 410 && code == "session_stopped") return new SessionStoppedException(bodyMessage.Length > 0 ? bodyMessage : null);
        return new ApiException(res.Status, code, message, json);
    }

    static string StatusCode(int status) => status switch
    {
        400 => "bad_request",
        401 => "unauthorized",
        403 => "forbidden",
        404 => "not_found",
        409 => "conflict",
        410 => "gone",
        429 => "too_many_requests",
        503 => "unavailable",
        _ => status >= 500 ? "internal" : "bad_request",
    };
}
