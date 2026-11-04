using System;
using System.Collections.Generic;
using System.Threading.Tasks;
using Pley.Tv.Core.Api;
using Pley.Tv.Core.Auth;
using Pley.Tv.Core.Ui;

namespace Pley.Tv.Core.Player;

/// <summary>What the player needs from the app around it: the app's state and its API, or a fake in tests.</summary>
public interface IPlayerSource
{
    /// <summary>A file on a drive (<see cref="Local.LocalPlayerSource"/>): it plays as it is, at its own path, with no server.</summary>
    bool Local { get; }

    /// <summary>The server's origin: addresses are made whole against it, and subtitle offsets kept under it.</summary>
    string Server { get; }

    /// <summary>This TV's client id: starting a session ends this profile's earlier ones on it.</summary>
    string ClientId { get; }

    /// <summary>The profile watching; null when not known (offsets aren't kept then).</summary>
    long? ProfileId { get; }

    /// <summary>Where this TV keeps things: subtitle offsets.</summary>
    IKeyValue Storage { get; }

    /// <summary>What a person set on this TV: the quality last picked, Up Next's autoplay, the subtitles' look.</summary>
    Settings Settings { get; }

    void UpdateSettings(Func<Settings, Settings> change);

    /// <summary>An absolute address on the server for <paramref name="path"/>.</summary>
    string Url(string path);

    /// <summary>The TV's capabilities report (<see cref="TizenCaps.Of"/>); null when it couldn't be made.</summary>
    IReadOnlyDictionary<string, object?>? Caps();

    /// <summary>The detail of <paramref name="id"/> the app has already, to start without waiting; null when none is kept.</summary>
    ItemDetail? Saved(long id);

    Task<ItemDetail> Item(long id);

    /// <summary>POST /api/playback with <paramref name="body"/>.</summary>
    Task<PlaybackInfo> StartPlayback(IReadOnlyDictionary<string, object?> body);

    /// <summary>DELETE /api/playback/{session}.</summary>
    Task StopPlayback(string session);

    /// <summary>POST /api/items/{id}/progress.</summary>
    Task Progress(long itemId, long positionMs, long? durationMs, string state, string session);

    /// <summary>GET /api/playback/{session}, waiting <paramref name="timeoutMs"/> at most.</summary>
    Task<SessionStatus> PlaybackStatus(string session, int timeoutMs);

    /// <summary>Whether the server answers at all, waiting <paramref name="timeoutMs"/> at most.</summary>
    Task Reachable(int timeoutMs);

    /// <summary>A text subtitle track's WebVTT at an absolute address.</summary>
    Task<SubtitleText> Subtitles(string url);

    /// <summary>The server's .mks of a picture subtitle track at an absolute address.</summary>
    Task<SubtitlePictures> PictureSubtitles(string url);

    /// <summary>Bytes from, to (inclusive) of the server's .mks of a picture subtitle track (<see cref="PictureRanges"/>).</summary>
    Task<SubtitlePart> PictureSubtitlesPart(string url, long from, long to);

    /// <summary>The file's seek previews; null while the server makes them.</summary>
    Task<TrickplayInfo?> Trickplay(long fileId);

    string TrickplaySheetUrl(long fileId, int sheet, string version);

    /// <summary>The episode after <paramref name="itemId"/>; null at the end.</summary>
    Task<ItemSummary?> Next(long itemId);

    /// <summary>A playlist's or collection's items, in their order.</summary>
    Task<IReadOnlyList<ItemSummary>> Queue(PlayQueue queue);

    /// <summary>
    /// Keeps the tracks picked for <paramref name="itemId"/> (PUT /api/items/{id}/streams): the
    /// audio left out (null) keeps the one chosen before, as subtitles alone were picked.
    /// </summary>
    Task SetStreams(long itemId, long? fileId, int? audio, int? subtitle);

    /// <summary>Tracks were chosen for <paramref name="detail"/>'s title: details kept elsewhere name the tracks of before.</summary>
    void TracksChosen(ItemDetail detail);

    /// <summary>Calls <paramref name="listener"/> with each of the server's events; disposing stops it.</summary>
    IDisposable OnEvent(Action<ServerEvent> listener);
}

/// <summary>The player's source in the app: <see cref="AppState"/>, its API and its settings.</summary>
public sealed class AppPlayerSource : IPlayerSource
{
    readonly AppState app;
    readonly Func<IReadOnlyDictionary<string, object?>?> caps;

    /// <param name="caps">The TV's report, made once by the Tizen side (<see cref="TizenCaps.Of"/>).</param>
    public AppPlayerSource(AppState app, Func<IReadOnlyDictionary<string, object?>?> caps)
    {
        this.app = app;
        this.caps = caps;
    }

    Api.Api Api => app.Api.Value ?? throw new SignedOutException();

    public bool Local => false;

    public string Server => app.Server.Value ?? "";

    public string ClientId => app.InstallId();

    public long? ProfileId => app.Me.Value?.Id;

    public IKeyValue Storage => app.Storage;

    public Settings Settings => app.Settings.Value;

    public void UpdateSettings(Func<Settings, Settings> change) => app.UpdateSettings(change);

    public string Url(string path) => app.Api.Value?.Url(path) ?? Artwork.Absolute(path, Server);

    public IReadOnlyDictionary<string, object?>? Caps() => caps();

    public ItemDetail? Saved(long id) => app.CachedItem(id);

    public Task<ItemDetail> Item(long id) => app.Api.Value == null ? Task.FromException<ItemDetail>(new SignedOutException()) : app.Item(id);

    public Task<PlaybackInfo> StartPlayback(IReadOnlyDictionary<string, object?> body) => Api.StartPlayback(body);

    public Task StopPlayback(string session) => Api.StopPlayback(session);

    public Task Progress(long itemId, long positionMs, long? durationMs, string state, string session) => Api.Progress(itemId, positionMs, durationMs, state, session);

    public Task<SessionStatus> PlaybackStatus(string session, int timeoutMs) => Api.PlaybackStatus(session, timeoutMs: timeoutMs);

    public Task Reachable(int timeoutMs) => Api.Reachable(timeoutMs);

    public Task<SubtitleText> Subtitles(string url) => Api.Subtitles(url);

    public Task<SubtitlePictures> PictureSubtitles(string url) => Api.PictureSubtitles(url);

    public Task<SubtitlePart> PictureSubtitlesPart(string url, long from, long to) => Api.PictureSubtitlesPart(url, from, to);

    public Task<TrickplayInfo?> Trickplay(long fileId) => Api.Trickplay(fileId);

    public string TrickplaySheetUrl(long fileId, int sheet, string version) => Api.TrickplaySheetUrl(fileId, sheet, version);

    public async Task<ItemSummary?> Next(long itemId) => (await Api.Next(itemId).ConfigureAwait(false)).Item;

    public async Task<IReadOnlyList<ItemSummary>> Queue(PlayQueue queue)
    {
        if (queue is PlayQueue.Collection c) return (await Api.Collection(c.CollectionId).ConfigureAwait(false)).Items;
        var playlist = await Api.Playlist(((PlayQueue.Playlist)queue).PlaylistId).ConfigureAwait(false);
        var items = new List<ItemSummary>();
        foreach (var entry in playlist.Entries) items.Add(entry.Item);
        return items;
    }

    public Task SetStreams(long itemId, long? fileId, int? audio, int? subtitle) =>
        AppItemSource.SendStreams(Api, itemId, fileId, audio, subtitle, automaticAudio: false);

    public void TracksChosen(ItemDetail detail) => app.TracksChanged(detail);

    public IDisposable OnEvent(Action<ServerEvent> listener) => app.OnEvent(listener);
}
