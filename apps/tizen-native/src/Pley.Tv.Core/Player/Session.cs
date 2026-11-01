using System;
using System.Collections.Generic;
using System.Threading.Tasks;
using Pley.Tv.Core.Api;
using Pley.Tv.Core.Ui;

namespace Pley.Tv.Core.Player;

/// <summary>Sends reports to the server one after the other, in the order they were made; best effort.</summary>
public interface IReporter
{
    void Send(Func<Task> report);

    /// <summary>Completes once everything queued so far went out.</summary>
    Task Flush();
}

/// <summary>
/// Sends playbacks' reports one after the other (the Android app's SessionReporter): a player
/// that closes still gets its last position and its session's end to the server, in the order
/// they were made (progress, then "stopped", then DELETE). Best effort: a failure is dropped, as on
/// the web and Android.
/// </summary>
public sealed class SessionReporter : IReporter
{
    readonly object gate = new object();
    Task tail = Task.CompletedTask;

    public void Send(Func<Task> report)
    {
        lock (gate)
        {
            tail = tail.ContinueWith(async _ =>
            {
                try
                {
                    await report().ConfigureAwait(false);
                }
                catch (Exception)
                {
                    // Best effort: the next report says where the player is anyway.
                }
            }, TaskScheduler.Default).Unwrap();
        }
    }

    /// <summary>Completes once everything queued so far went out.</summary>
    public Task Flush()
    {
        lock (gate) return tail;
    }
}

/// <summary>A session on the server as a player reports to it: <see cref="PlaybackSession"/>, or a fake in tests.</summary>
public interface IPlaybackSession
{
    PlaybackInfo Info { get; }

    string Id { get; }

    bool Ended { get; }

    /// <summary>"playing" or "paused" at <paramref name="positionMs"/>, so the session and its conversion stay alive.</summary>
    void Progress(long positionMs, bool playing);

    /// <summary>Reports "stopped" at <paramref name="positionMs"/>, then deletes the session.</summary>
    void End(long positionMs);

    /// <summary>The server stopped the session itself: "stopped" keeps the resume point, and nothing is deleted.</summary>
    void StoppedByServer(long positionMs);
}

/// <summary>
/// One session on the server (POST /api/playback; the Android app's ServerPlaybackSession): its
/// progress reports and its end. Ending it reports "stopped" at the last position, then deletes it
/// (and any conversion); it ends once.
/// </summary>
public sealed class PlaybackSession : IPlaybackSession
{
    readonly Api.Api api;
    readonly SessionReporter reporter;

    public PlaybackSession(Api.Api api, long itemId, PlaybackInfo info, SessionReporter reporter)
    {
        this.api = api;
        this.reporter = reporter;
        ItemId = itemId;
        Info = info;
    }

    public long ItemId { get; }
    public PlaybackInfo Info { get; }
    public string Id => Info.SessionId;
    public bool Ended { get; private set; }

    /// <summary>"playing" or "paused" at <paramref name="positionMs"/>, so the session and its conversion stay alive.</summary>
    public void Progress(long positionMs, bool playing)
    {
        if (Ended) return;
        Report(positionMs, playing ? "playing" : "paused");
    }

    /// <summary>Reports "stopped" at <paramref name="positionMs"/>, then DELETE /api/playback/{session}.</summary>
    public void End(long positionMs)
    {
        if (Ended) return;
        Ended = true;
        Report(positionMs, "stopped");
        reporter.Send(() => api.StopPlayback(Id));
    }

    /// <summary>The server stopped the session itself: "stopped" keeps the resume point, and nothing is deleted.</summary>
    public void StoppedByServer(long positionMs)
    {
        if (Ended) return;
        Ended = true;
        Report(positionMs, "stopped");
    }

    void Report(long positionMs, string state)
    {
        var duration = Info.DurationMs > 0 ? Info.DurationMs : (long?)null;
        reporter.Send(() => api.Progress(ItemId, Math.Max(0, positionMs), duration, state, Id));
    }

    /// <summary>
    /// Starts a session for <paramref name="request"/>. The request isn't cancelled halfway: the
    /// server may have made the session (and its conversion) already.
    /// </summary>
    public static async Task<PlaybackSession> Start(Api.Api api, long itemId, IReadOnlyDictionary<string, object?> request, SessionReporter reporter)
    {
        var info = await api.StartPlayback(request).ConfigureAwait(false);
        return new PlaybackSession(api, itemId, info, reporter);
    }
}

/// <summary>What goes in the body of POST /api/playback (the Android app's PlaybackPlan, packages/tv's playbackRequest).</summary>
public static class PlayRequests
{
    /// <summary>
    /// The body for <paramref name="request"/>: the file and audio only when picked (else the
    /// server chooses the one this TV plays best), Automatic quality, and keyed addresses, since the
    /// TV's player can't send the Authorization header.
    /// </summary>
    public static Dictionary<string, object?> Body(PlayRequest request, string clientId, IReadOnlyDictionary<string, object?> caps)
    {
        var body = Base(request.ItemId, request.StartMs, "auto", clientId, caps);
        if (request.FileId != null) body["file_id"] = request.FileId;
        if (request.AudioIndex != null) body["audio_index"] = request.AudioIndex;
        return body;
    }

    /// <summary>
    /// The body for <paramref name="choices"/> (the Android app's PlaybackPlan.request): the file
    /// always; the audio and subtitle indexes only when the server has to act on them
    /// (<see cref="PlaybackPlan.AudioIndexFor"/>, <see cref="PlaybackPlan.SubtitleIndexFor"/>); a
    /// quality's caps; <paramref name="keepPrevious"/> when it is made ready while the session
    /// before plays on. <paramref name="read"/> is <paramref name="caps"/> as the plan reads it.
    /// </summary>
    public static Dictionary<string, object?> Body(long itemId, PlaybackPlan.Choices choices, string clientId, IReadOnlyDictionary<string, object?> caps, ClientCaps? read, bool keepPrevious = false)
    {
        var body = Base(itemId, choices.StartMs, choices.Mode, clientId, caps);
        body["file_id"] = choices.File.Id;
        if (PlaybackPlan.AudioIndexFor(choices, read) is int audio) body["audio_index"] = audio;
        if (PlaybackPlan.SubtitleIndexFor(choices) is int subtitle) body["subtitle_index"] = subtitle;
        if (choices.Quality.MaxBitrateKbps is int kbps) body["max_bitrate"] = kbps;
        if (choices.Quality.MaxHeight is int lines) body["max_height"] = lines;
        if (keepPrevious) body["keep_previous"] = true;
        return body;
    }

    static Dictionary<string, object?> Base(long itemId, long startMs, string mode, string clientId, IReadOnlyDictionary<string, object?> caps) =>
        new Dictionary<string, object?>
        {
            ["item_id"] = itemId,
            ["start_ms"] = Math.Max(0, startMs),
            ["mode"] = mode,
            ["client_id"] = clientId,
            ["keyed_urls"] = true,
            ["caps"] = caps,
        };

    /// <summary>
    /// The body for a music track: from <paramref name="startMs"/> when past its start, as the
    /// server chooses unless <paramref name="transcode"/> (the TV couldn't decode it as it is),
    /// under music's own client id (<c>{install id}-music</c>), so a video starting doesn't end the
    /// music's session nor the other way round.
    /// </summary>
    public static Dictionary<string, object?> MusicBody(long itemId, long startMs, bool transcode, string clientId, IReadOnlyDictionary<string, object?> caps)
    {
        var body = new Dictionary<string, object?> { ["item_id"] = itemId };
        if (startMs > 0) body["start_ms"] = startMs;
        body["mode"] = transcode ? "transcode" : "auto";
        body["client_id"] = clientId + "-music";
        body["keyed_urls"] = true;
        body["caps"] = caps;
        return body;
    }
}
