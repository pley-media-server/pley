using System;
using System.Collections.Generic;
using System.Globalization;
using System.Threading;
using System.Threading.Tasks;
using Pley.Tv.Core.Api;
using Pley.Tv.Core.Auth;
using Pley.Tv.Core.Net;

namespace Pley.Tv.Core.Live;

/// <summary>
/// How Live TV was last left on this TV, by the profile shown: the category ("all",
/// "favorites", "recent", "group:UK| Sport"), the view ("Guide" or "Channels") and the channel
/// last watched, which the guide opens back at after a restart.
/// </summary>
public sealed record LiveTvView(string? Category = null, string? View = null, long? Channel = null);

/// <summary>What Live TV's pages read and change: the app's state, or a fake in tests.</summary>
public interface ILiveSource
{
    /// <summary>The server's origin: logos and programme pictures are on it.</summary>
    string Origin { get; }

    Task<LiveSummary> Summary();

    Task<LiveGroups> Groups();

    /// <summary>Page <paramref name="page"/> (<see cref="LiveRules.Page"/> channels) of <paramref name="category"/>'s channels matching <paramref name="query"/>.</summary>
    Task<LiveChannelPage> Page(LiveCategory category, string query, int page);

    /// <summary>The page of <paramref name="category"/> that holds <paramref name="channelId"/>.</summary>
    Task<LiveChannelPage> Around(LiveCategory category, long channelId);

    Task<LiveChannel> Channel(long channelId);

    /// <summary>The guide of channels <paramref name="ids"/> from <paramref name="from"/> for <see cref="LiveRules.GuideChunkS"/>.</summary>
    Task<LiveGuide> Guide(long from, IReadOnlyList<long> ids);

    /// <summary>The channel numbered <paramref name="number"/> exactly, of every channel; null when none is.</summary>
    Task<LiveChannel?> Numbered(string number);

    Task SetFavorite(long channelId, bool favorite);

    /// <summary>What was kept for the profile: all null when nothing was.</summary>
    LiveTvView Kept();

    /// <summary>Keeps what isn't null, the rest as it was; without a profile nothing is kept.</summary>
    void Keep(string? category = null, string? view = null, long? channel = null);

    /// <summary>Calls <paramref name="changed"/> when the lineup may have changed (the server's events, a reconnection).</summary>
    IDisposable OnChange(Action changed);
}

/// <summary>What watching a channel needs of the server: the app's API, or a fake in tests.</summary>
public interface ILiveServer
{
    /// <summary>This TV's own id: watching on it ends what it watched before, so changing channels holds one connection.</summary>
    string ClientId { get; }

    /// <summary>An absolute address on the server for <paramref name="path"/>.</summary>
    string Url(string path);

    /// <summary>
    /// Starts watching <paramref name="channelId"/> with a keyed playlist address;
    /// <paramref name="takeOver"/>, the connection to take; <paramref name="controls"/>, what the
    /// player obeys from the remote.
    /// </summary>
    Task<LiveWatching> Watch(long channelId, long? takeOver, IReadOnlyList<string> controls, CancellationToken cancel);

    /// <summary>Asks for the playlist at <paramref name="url"/> as the player will, waiting for its first segments.</summary>
    Task Prefetch(string url, int timeoutMs, CancellationToken cancel);

    /// <summary>Why a session ended (its connection taken over, say), or null while it goes on.</summary>
    Task<string?> WhyEnded(string session);

    /// <summary>
    /// The channel's MPEG-TS as the provider sends it (<c>stream.ts</c>, read under
    /// <paramref name="clientId"/>), on an address the TV's player can open, in place of the one
    /// before; null where there is none.
    /// </summary>
    string? Stream(long channelId, string clientId);

    /// <summary>Stops handing on the channel's MPEG-TS: its address goes, and its reading.</summary>
    void EndStream();

    /// <summary>What the session's channel is made into; null when that isn't known in time, or asking failed.</summary>
    Task<LivePlan?> Plan(string session);

    /// <summary>Stops watching <paramref name="session"/>, in the background.</summary>
    void Leave(string session);

    /// <summary>
    /// Calls <paramref name="listener"/> with the server's events: someone taking over a channel's
    /// connection from this profile (<c>live_taken_over</c>), a session stopped from elsewhere
    /// (<c>session_stopped</c>), and the profile's remote (<c>player_command</c>).
    /// </summary>
    IDisposable OnEvent(Action<ServerEvent> listener);
}

/// <summary>Live TV's source in the app: <see cref="AppState"/> and its API.</summary>
public sealed class AppLiveSource : ILiveSource, ILiveServer
{
    /// <summary>How Live TV was left, per server and profile.</summary>
    const string ViewKey = "pley.liveTv";

    readonly AppState app;

    public AppLiveSource(AppState app) => this.app = app;

    Api.Api Api => app.Api.Value ?? throw new SignedOutException();

    public string Origin => app.Server.Value ?? "";

    public string ClientId => app.InstallId();

    public string Url(string path) => Api.Url(path);

    public Task<LiveSummary> Summary() => Answered(() => Api.LiveSummary());

    public Task<LiveGroups> Groups() => Answered(() => Api.LiveGroups());

    public Task<LiveChannelPage> Page(LiveCategory category, string query, int page) =>
        Answered(() => Api.LiveBrowse(category, query, page * LiveRules.Page, LiveRules.Page));

    public Task<LiveChannelPage> Around(LiveCategory category, long channelId) =>
        Answered(() => Api.LiveBrowse(category, limit: LiveRules.Page, around: channelId));

    public Task<LiveChannel> Channel(long channelId) => Answered(() => Api.LiveChannel(channelId));

    public Task<LiveGuide> Guide(long from, IReadOnlyList<long> ids) => Answered(() => Api.LiveGuide(from, from + LiveRules.GuideChunkS, ids));

    public async Task<LiveChannel?> Numbered(string number)
    {
        var page = await Answered(() => Api.LiveBrowse(new LiveCategory.All(), number, limit: LiveRules.Page));
        foreach (var c in page.Channels)
            if (c.Number == number) return c;
        return null;
    }

    public Task SetFavorite(long channelId, bool favorite) => Api.SetFavoriteChannel(channelId, favorite);

    public Task<LiveWatching> Watch(long channelId, long? takeOver, IReadOnlyList<string> controls, CancellationToken cancel) =>
        Api.WatchChannel(channelId, ClientId, takeOver, controls, cancel);

    public Task Prefetch(string url, int timeoutMs, CancellationToken cancel) => Api.Media(url, timeoutMs, cancel);

    public Task<string?> WhyEnded(string session) => Api.WhyLiveEnded(session);

    /// <summary>Hands the channel's MPEG-TS on to the TV's player, which can't send the token.</summary>
    readonly StreamRelay relay = new StreamRelay();

    public string? Stream(long channelId, string clientId)
    {
        var api = app.Api.Value;
        if (api == null) return null;
        var path = Core.Api.Api.LiveStreamPath(channelId, clientId);
        try
        {
            return relay.Serve(cancel => api.Open(path, cancel));
        }
        catch (Exception)
        {
            // No loopback to listen on: the channel's HLS.
            return null;
        }
    }

    public void EndStream() => relay.Stop();

    public async Task<LivePlan?> Plan(string session)
    {
        try
        {
            return await Api.LivePlan(session).ConfigureAwait(false);
        }
        catch (Exception)
        {
            // Not had: the channel's HLS, as before there were plans.
            return null;
        }
    }

    public void Leave(string session)
    {
        var api = app.Api.Value;
        if (api == null) return;
        _ = Quietly(api.LeaveChannel(session));
    }

    public IDisposable OnEvent(Action<ServerEvent> listener) => app.OnEvent(listener);

    static async Task Quietly(Task task)
    {
        try
        {
            await task.ConfigureAwait(false);
        }
        catch (Exception)
        {
            // Out of reach: the server lets the session go by itself after 45 s.
        }
    }

    public LiveTvView Kept()
    {
        if (KeyOf() is not { } key) return new LiveTvView();
        var store = app.Platform.Storage;
        return new LiveTvView(
            store.Get(key + ".category"),
            store.Get(key + ".view"),
            long.TryParse(store.Get(key + ".channel"), NumberStyles.Integer, CultureInfo.InvariantCulture, out var id) ? id : null);
    }

    public void Keep(string? category = null, string? view = null, long? channel = null)
    {
        if (KeyOf() is not { } key) return;
        var store = app.Platform.Storage;
        if (category != null) store.Set(key + ".category", category);
        if (view != null) store.Set(key + ".view", view);
        if (channel != null) store.Set(key + ".channel", channel.Value.ToString(CultureInfo.InvariantCulture));
    }

    string? KeyOf()
    {
        var server = app.Server.Value;
        var me = app.Me.Value?.Id;
        return server == null || me == null ? null : string.Join(".", ViewKey, server, me.Value.ToString(CultureInfo.InvariantCulture));
    }

    public IDisposable OnChange(Action changed)
    {
        // The lineup's own events, and every refresh the app makes (a reconnection, a playback that ended).
        var events = app.OnEvent(e =>
        {
            if (e.Type is "live_channels_changed") changed();
        });
        var first = true;
        var changes = app.Changes.Watch(_ =>
        {
            if (first) first = false;
            else changed();
        });
        return new Both(events, changes);
    }

    sealed class Both : IDisposable
    {
        readonly IDisposable a, b;

        public Both(IDisposable a, IDisposable b)
        {
            this.a = a;
            this.b = b;
        }

        public void Dispose()
        {
            a.Dispose();
            b.Dispose();
        }
    }

    /// <summary><paramref name="call"/>'s answer; the server out of reach shows as such, and an answer after it brings the pages back.</summary>
    async Task<T> Answered<T>(Func<Task<T>> call)
    {
        try
        {
            var value = await call();
            app.ServerAnswered();
            return value;
        }
        catch (Exception e) when (Errors.Unreachable(e))
        {
            app.ServerAway();
            throw;
        }
    }
}
