using System;
using System.Threading.Tasks;
using Pley.Tv.Core.Auth;

namespace Pley.Tv.Core.Player;

/// <summary>Music's server in the app: <see cref="AppState"/>'s API, with what this TV plays.</summary>
public sealed class AppMusicServer : IMusicServer
{
    /// <summary>The longest a new track waits for the last one's reports to go out.</summary>
    const int FlushWaitMs = 3_000;

    readonly AppState app;
    readonly Func<TizenFacts> facts;

    /// <param name="facts">What this TV is, for the report of what it plays.</param>
    public AppMusicServer(AppState app, Func<TizenFacts> facts)
    {
        this.app = app;
        this.facts = facts;
    }

    Api.Api Api => app.Api.Value ?? throw new SignedOutException();

    public string Url(string path) => Api.Url(path);

    public async Task<IPlaybackSession> Start(long itemId, long startMs, bool transcode)
    {
        var api = Api;
        // The session before goes first (stopped, then DELETE): the server's limit on conversions. The
        // reports go with the player's, the app's, which a cast's switch waits for.
        await Task.WhenAny(app.Reporter.Flush(), Task.Delay(FlushWaitMs)).ConfigureAwait(false);
        var caps = TizenCaps.Of(facts(), app.Settings.Value.Passthrough);
        var body = PlayRequests.MusicBody(itemId, startMs, transcode, app.InstallId(), caps);
        return await PlaybackSession.Start(api, itemId, body, app.Reporter).ConfigureAwait(false);
    }

    public Task Status(string session) => Api.PlaybackStatus(session);

    /// <summary>
    /// The app's music player: it plays behind every page, ends with the profile, the sign-in, the
    /// server or the app (<see cref="AppState.OnLeave"/>), stops when the server stops its
    /// session (<c>session_stopped</c>), and obeys the profile's remote (<c>player_command</c>).
    /// </summary>
    public static MusicPlayer Player(AppState app, Func<IMediaEngine> engines, Func<TizenFacts> facts, IScheduler scheduler)
    {
        var player = new MusicPlayer(new AppMusicServer(app, facts), engines, scheduler);
        app.OnLeave(player.Stop);
        app.OnEvent(e =>
        {
            if (e.Type == "session_stopped" && e.SessionId != null) player.StoppedByServer(e.SessionId);
            else if (e.Type == "player_command") player.Obey(e);
        });
        return player;
    }
}
