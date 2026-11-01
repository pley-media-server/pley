using System;
using System.Collections.Generic;
using System.Linq;
using System.Threading.Tasks;
using Pley.Tv.Core.Api;
using Pley.Tv.Core.Net;
using Pley.Tv.Core.Player;
using Pley.Tv.Core.Ui;
using Xunit;

namespace Pley.Tv.Core.Tests;

/// <summary>Music's server in a test: the sessions it starts, and what each was told.</summary>
sealed class FakeMusicServer : IMusicServer
{
    public const string Server = "http://192.168.1.20:7539";

    public List<FakeSession> Sessions { get; } = new List<FakeSession>();
    public List<string> Asked { get; } = new List<string>();
    /// <summary>What starting a session throws, if anything.</summary>
    public Exception? StartFailure { get; set; }
    /// <summary>The method sessions play by.</summary>
    public string Method { get; set; } = "direct";
    /// <summary>What asking after a session answers: null, it goes on.</summary>
    public Exception? StatusFailure { get; set; }

    public string Url(string path) => Server + path;

    public Task<IPlaybackSession> Start(long itemId, long startMs, bool transcode)
    {
        Asked.Add($"{itemId} at {startMs}{(transcode ? " transcode" : "")}");
        if (StartFailure != null) return Task.FromException<IPlaybackSession>(StartFailure);
        var s = new FakeSession(new PlaybackInfo { SessionId = $"m{Sessions.Count + 1}", Method = Method, Url = $"/api/media/pley_mk_{itemId}/file.flac", DurationMs = 200_000 });
        Sessions.Add(s);
        return Task.FromResult<IPlaybackSession>(s);
    }

    public Task Status(string session) => StatusFailure != null ? Task.FromException(StatusFailure) : Task.CompletedTask;

    public FakeSession Last => Sessions.Last();
}

sealed class FakeSession : IPlaybackSession
{
    public FakeSession(PlaybackInfo info) => Info = info;

    public PlaybackInfo Info { get; }
    public string Id => Info.SessionId;
    public bool Ended { get; private set; }
    public List<string> Reports { get; } = new List<string>();

    public void Progress(long positionMs, bool playing)
    {
        if (!Ended) Reports.Add($"{(playing ? "playing" : "paused")} {positionMs}");
    }

    public void End(long positionMs)
    {
        if (Ended) return;
        Ended = true;
        Reports.Add($"stopped {positionMs}");
        Reports.Add("deleted");
    }

    public void StoppedByServer(long positionMs)
    {
        if (Ended) return;
        Ended = true;
        Reports.Add($"stopped {positionMs}");
    }
}

/// <summary>The queue: the Android app's MusicQueueTest.</summary>
public class MusicQueueTests
{
    static IReadOnlyList<ItemSummary> Tracks(int n) => Enumerable.Range(1, n).Select(i => new ItemSummary { Id = i, Kind = "track", Title = $"T{i}" }).ToList();

    [Fact]
    public void A_queue_plays_in_the_order_given_from_the_track_chosen()
    {
        var q = new MusicQueue(new Random(1));
        q.Start(Tracks(4), 2);
        Assert.Equal(3L, q.Current?.Id);
        Assert.True(q.Next());
        Assert.False(q.Next());
        Assert.Equal(4L, q.Current?.Id);
    }

    [Fact]
    public void Shuffling_is_a_permutation_that_starts_with_the_chosen_track_and_unshuffling_finds_its_place()
    {
        var q = new MusicQueue(new Random(7));
        q.Start(Tracks(10), 4, shuffle: true);
        Assert.Equal(5L, q.Tracks[0].Id);
        Assert.Equal(0, q.Index);
        Assert.Equal(Enumerable.Range(1, 10).Select(i => (long)i), q.Tracks.Select(t => t.Id).OrderBy(i => i));
        q.Next();
        var playing = q.Current!.Id;
        q.SetShuffle(false);
        Assert.Equal(playing, q.Current!.Id);
        Assert.Equal(playing - 1, q.Index);
        Assert.False(q.Shuffle);
    }

    [Fact]
    public void Previous_restarts_after_three_seconds_or_on_the_first_track()
    {
        var q = new MusicQueue();
        q.Start(Tracks(3), 1);
        Assert.Equal(PreviousMove.Restart, q.Previous(3_001));
        Assert.Equal(PreviousMove.Moved, q.Previous(3_000));
        Assert.Equal(PreviousMove.Restart, q.Previous(0));
        Assert.False(q.JumpTo(3));
        Assert.True(q.JumpTo(2));
        new MusicQueue().SetShuffle(true);
        Assert.Null(new MusicQueue().Current);
    }

    [Fact]
    public void The_same_track_twice_is_two_places()
    {
        var t = new ItemSummary { Id = 9 };
        var q = new MusicQueue();
        q.Start(new[] { t, t }, 1);
        Assert.Equal(1, q.Index);
        Assert.False(q.HasNext);
    }
}

/// <summary>The sleep timer: the Android app's SleepTimerTest.</summary>
public class SleepTimerTests
{
    const long Minute = 60_000;
    readonly FakeScheduler scheduler = new FakeScheduler();
    long? left;
    object? key = 1L;
    int fired;

    SleepTimer Timer() => new SleepTimer(scheduler, () => left, () => key, () => fired++);

    [Fact]
    public void Minutes_count_down_the_notice_shows_the_last_30_s_once_a_second_then_it_pauses()
    {
        var t = Timer();
        t.Set(new SleepChoice.Minutes(15));
        Assert.Equal(15, t.State.Value.MinutesLeft);
        Assert.Null(t.State.Value.SecondsLeft);
        scheduler.Advance(14 * Minute + 29_000);
        Assert.Null(t.State.Value.SecondsLeft);
        scheduler.Advance(1_000);
        Assert.Equal(30, t.State.Value.SecondsLeft);
        scheduler.Advance(15_000);
        Assert.Equal(15, t.State.Value.SecondsLeft);
        Assert.Equal(0, fired);
        scheduler.Advance(15_000);
        Assert.Equal(1, fired);
        Assert.Equal(new SleepState(Fired: true), t.State.Value);
        t.Resumed();
        Assert.False(t.State.Value.Fired);
    }

    [Fact]
    public void Keep_listening_adds_the_same_time_again_and_off_stops_it()
    {
        var t = Timer();
        t.Set(new SleepChoice.Minutes(30));
        scheduler.Advance(30 * Minute - 10_000);
        Assert.Equal(10, t.State.Value.SecondsLeft);
        t.Extend();
        Assert.Null(t.State.Value.SecondsLeft);
        Assert.Equal(31, t.State.Value.MinutesLeft);
        scheduler.Advance(20_000);
        Assert.Equal(0, fired);
        t.Set(new SleepChoice.Off());
        scheduler.Advance(60 * Minute);
        Assert.Equal(0, fired);
        Assert.Equal(new SleepState(new SleepChoice.Off()), t.State.Value);
    }

    [Fact]
    public void The_end_of_the_item_ends_it_and_keep_listening_waits_for_the_next_ones_end()
    {
        var t = Timer();
        left = 10 * Minute;
        t.Set(new SleepChoice.EndOfItem());
        Assert.True(t.EndsWith(1L));
        left = 25_000;
        scheduler.Advance(1_000);
        Assert.Equal(25, t.State.Value.SecondsLeft);
        t.Extend();
        scheduler.Advance(1_000);
        Assert.Null(t.State.Value.SecondsLeft);
        Assert.False(t.ItemEnded(1L));
        key = 2L;
        left = 15_000;
        scheduler.Advance(1_000);
        Assert.Equal(15, t.State.Value.SecondsLeft);
        Assert.True(t.ItemEnded(2L));
        Assert.Equal(1, fired);
        Assert.Equal("End of this track", SleepText.Choice(new SleepChoice.EndOfItem(), music: true));
        Assert.Equal("42 min left", SleepText.Summary(new SleepState(new SleepChoice.Minutes(45), MinutesLeft: 42)));
    }
}

/// <summary>Music's playback: the Android app's MusicPlaybackTest, on the TV's player through keyed sessions.</summary>
public class MusicPlayerTests
{
    readonly FakeMusicServer server = new FakeMusicServer();
    readonly FakeScheduler scheduler = new FakeScheduler();
    readonly List<FakeEngine> engines = new List<FakeEngine>();

    static readonly IReadOnlyList<ItemSummary> Album = Enumerable.Range(1, 3)
        .Select(i => new ItemSummary { Id = 10 + i, Kind = "track", Title = $"Track {i}", ParentId = 9, ParentTitle = "Harbour", GrandparentTitle = "Lights", DurationMs = 200_000 })
        .ToList();

    MusicPlayer Player() => new MusicPlayer(server, () =>
    {
        var e = new FakeEngine();
        engines.Add(e);
        return e;
    }, scheduler, new Random(3));

    FakeEngine Engine => engines.Single();

    [Fact]
    public void An_album_plays_from_the_track_chosen_with_its_own_keyed_session()
    {
        var p = Player();
        p.Play(Album, 1);
        Assert.Equal(new[] { "12 at 0" }, server.Asked);
        Assert.Equal(new EngineSource($"{FakeMusicServer.Server}/api/media/pley_mk_12/file.flac", false, DurationHintMs: 200_000), Engine.Source);
        Assert.Contains($"prepare {FakeMusicServer.Server}/api/media/pley_mk_12/file.flac play", Engine.Calls);
        var now = p.Now.Value!;
        Assert.Equal("Track 2", now.Title);
        Assert.Equal("Lights · Harbour", now.Byline);
        Assert.Equal($"{FakeMusicServer.Server}/api/items/9/images/poster", now.ArtUrl);
        Assert.True(now.Loading);
        Engine.SetPlaying(true);
        Assert.True(p.Now.Value!.Playing);
        Assert.False(p.Now.Value.Loading);
        Assert.Equal(1, p.Now.Value.Index);
        Assert.Equal(3, p.Now.Value.Count);
    }

    [Fact]
    public void Progress_goes_out_every_ten_seconds_while_playing_on_pause_and_every_thirty_paused()
    {
        var p = Player();
        p.Play(Album);
        Engine.SetPlaying(true);
        Engine.PositionMs = 10_000;
        scheduler.Advance(MusicPlayer.ProgressEveryMs);
        Assert.Equal(new[] { "playing 10000" }, server.Last.Reports);
        p.Toggle();
        Assert.Equal("paused 10000", server.Last.Reports.Last());
        scheduler.Advance(20_000);
        Assert.Equal(2, server.Last.Reports.Count);
        scheduler.Advance(10_000);
        Assert.Equal("paused 10000", server.Last.Reports.Last());
        Assert.Equal(3, server.Last.Reports.Count);
    }

    [Fact]
    public void A_track_ends_with_stopped_then_the_next_one_starts_its_own_session_and_the_last_one_stops()
    {
        var p = Player();
        p.Play(Album, 1);
        Engine.SetPlaying(true);
        Engine.Status(EngineStatus.Ended);
        Assert.Equal(new[] { "stopped 200000", "deleted" }, server.Sessions[0].Reports);
        Assert.Equal("13 at 0", server.Asked.Last());
        Engine.Status(EngineStatus.Ended);
        Assert.True(p.Now.Value!.Ended);
        Assert.Equal(200_000, p.Position().PositionMs);
        // Play after the end starts the last track again.
        p.Toggle();
        Assert.Equal(new[] { "12 at 0", "13 at 0", "13 at 0" }, server.Asked);
    }

    [Fact]
    public void Previous_restarts_after_three_seconds_else_goes_back()
    {
        var p = Player();
        p.Play(Album, 1);
        Engine.PositionMs = 5_000;
        p.Previous();
        Assert.Equal("seek 0", Engine.Calls.Last());
        Engine.PositionMs = 1_000;
        p.Previous();
        Assert.Equal("11 at 0", server.Asked.Last());
        Assert.Equal(new[] { "stopped 1000", "deleted" }, server.Sessions[0].Reports);
    }

    [Fact]
    public void A_track_the_tv_cant_decode_is_asked_for_again_as_a_conversion()
    {
        var p = Player();
        p.Play(Album);
        Engine.PositionMs = 4_000;
        Engine.Fail(new EngineError.Unsupported("FLAC 24-bit"));
        Assert.Equal("11 at 4000 transcode", server.Asked.Last());
        // Converted and still failing: a new session where it was, three times, then it can't be played.
        server.Method = "transcode";
        for (var i = 0; i < MusicPlayer.MaxRecreates; i++) Engine.Fail(new EngineError.Http(404, "playback session expired"));
        Assert.Equal(5, server.Asked.Count);
        Engine.Fail(new EngineError.Http(404, "playback session expired"));
        Assert.Equal(MusicError.Unplayable, p.Now.Value!.Error);
        Assert.Equal("This track can't be played.", MusicText.Error(MusicError.Unplayable));
    }

    [Fact]
    public void A_failure_the_player_cant_explain_asks_the_server()
    {
        var p = Player();
        p.Play(Album);
        server.StatusFailure = new SessionStoppedException("Stopped by Ann.");
        Engine.Fail(new EngineError.Other("it stopped"));
        Assert.Equal(MusicError.Stopped, p.Now.Value!.Error);
        Assert.Equal(new[] { "stopped 0" }, server.Sessions[0].Reports);
        // Play starts it again, with a session of its own.
        server.StatusFailure = null;
        p.Toggle();
        Assert.Equal(2, server.Sessions.Count);
        server.StatusFailure = new NetworkException(NetworkFailure.NothingAnswers);
        for (var i = 0; i <= MusicPlayer.MaxRecreates; i++) Engine.Fail(new EngineError.Other("it stopped"));
        Assert.Equal(MusicError.Offline, p.Now.Value!.Error);
    }

    [Fact]
    public void A_session_stopped_by_the_server_stays_stopped_and_the_server_out_of_reach_shows_as_offline()
    {
        var p = Player();
        p.Play(Album);
        p.StoppedByServer("m1");
        Assert.Equal(MusicError.Stopped, p.Now.Value!.Error);
        Assert.Single(server.Sessions);
        server.StartFailure = new NetworkException(NetworkFailure.NoAnswerInTime);
        p.Next();
        Assert.Equal(MusicError.Offline, p.Now.Value!.Error);
        Assert.Equal(MusicError.SignedOut, MusicPlayer.ErrorOf(new Pley.Tv.Core.Auth.SignedOutException()));
    }

    [Fact]
    public void A_video_takes_the_tvs_player_and_play_starts_a_new_session_where_it_was()
    {
        // Android only pauses the music for a video; a Samsung TV has one media pipeline, so its session goes (packages/tv's yieldToVideo).
        var p = Player();
        p.Play(Album);
        Engine.SetPlaying(true);
        Engine.PositionMs = 30_000;
        p.YieldToVideo();
        Assert.Equal(new[] { "stopped 30000", "deleted" }, server.Sessions[0].Reports);
        Assert.NotNull(p.Now.Value);
        p.Toggle();
        Assert.Equal("11 at 30000", server.Asked.Last());
    }

    [Fact]
    public void Stopping_ends_the_session_and_forgets_the_queue()
    {
        var p = Player();
        p.Play(Album);
        p.Stop();
        Assert.Null(p.Now.Value);
        Assert.Empty(p.Queue.Value.Tracks);
        Assert.Equal(new[] { "stopped 0", "deleted" }, server.Sessions[0].Reports);
        Assert.True(Engine.Released);
        p.SetSleep(new SleepChoice.Minutes(15));
        Assert.IsType<SleepChoice.Off>(p.Sleep.Value.Set);
    }

    [Fact]
    public void Shuffling_keeps_the_track_playing()
    {
        var p = Player();
        p.Play(Album, 1);
        p.SetShuffle(true);
        Assert.Equal(12L, p.Now.Value!.ItemId);
        Assert.Equal(0, p.Now.Value.Index);
        Assert.True(p.Now.Value.Shuffle);
        Assert.Single(server.Asked);
    }

    [Fact]
    public void The_sleep_timer_at_the_end_of_a_track_holds_the_next_one_paused()
    {
        var p = Player();
        p.Play(Album);
        Engine.SetPlaying(true);
        p.StepSleep();
        Assert.Equal(new SleepChoice.Minutes(15), p.Sleep.Value.Set);
        for (var i = 0; i < 5; i++) p.StepSleep();
        Assert.IsType<SleepChoice.EndOfItem>(p.Sleep.Value.Set);
        Engine.Status(EngineStatus.Ended);
        Assert.Contains($"prepare {FakeMusicServer.Server}/api/media/pley_mk_12/file.flac paused", Engine.Calls);
        Assert.True(p.Sleep.Value.Fired);
    }
}

/// <summary>Now Playing and the music pages: the Android app's MusicViewModelsTest.</summary>
public class MusicPagesTests
{
    readonly FakeMusicServer server = new FakeMusicServer();
    readonly FakeScheduler scheduler = new FakeScheduler();
    readonly FakeItems items = new FakeItems();
    readonly MusicPlayer player;

    public MusicPagesTests() => player = new MusicPlayer(server, () => new FakeEngine(), scheduler, new Random(5));

    static ItemSummary Track(long id, int index, int disc = 1, string? artist = "Lights") =>
        new ItemSummary { Id = id, Kind = "track", Title = $"Track {id}", Index = index, ParentIndex = disc, ParentId = 9, GrandparentTitle = artist, DurationMs = 192_000 };

    [Fact]
    public void An_album_shows_its_tracks_with_disc_headers_only_when_there_are_several_discs()
    {
        var one = MusicPages.Rows(new[] { Track(1, 1), Track(2, 2, artist: "Guest") }, "Lights");
        Assert.Equal(new[] { "track-1", "track-2" }, one.Select(r => r.Key));
        var two = (AlbumRow.Track)one[1];
        Assert.Equal(("2", "Guest", "3:12"), (two.Number, two.Artist, two.Duration));
        Assert.Null(((AlbumRow.Track)one[0]).Artist);
        var discs = MusicPages.Rows(new[] { Track(1, 1), Track(2, 1, disc: 2) }, "Lights");
        Assert.Equal(new[] { "disc-1", "track-1", "disc-2", "track-2" }, discs.Select(r => r.Key));
        Assert.Equal("Disc 2", ((AlbumRow.Disc)discs[2]).Label);
    }

    [Fact]
    public void The_album_page_says_year_tracks_and_length_and_plays()
    {
        items.Set(new ItemDetail { Id = 9, Kind = "album", Title = "Harbour", Year = 2019, ParentTitle = "Lights", ParentId = 3, UserState = new UserState { Rating = 7 } });
        items.SetChildren(9, Track(1, 1), Track(2, 2), Track(3, 3));
        var album = new AlbumModel(items, player, scheduler, 9, highlightId: 2, random: new Random(1));
        var ready = Assert.IsType<MusicPageState<AlbumView>.Ready>(album.State.Value);
        Assert.Equal("2019 · 3 tracks · 10m", ready.Page.Line);
        Assert.Equal(7, ready.Page.UserRating);
        // A half star shows as the star it is in, as the Android app draws it.
        Assert.Equal(new[] { true, true, true, true, false }, MusicPages.Stars(7));
        var told = new List<MusicEvent>();
        album.Told += told.Add;
        album.PlayOrToggle(1);
        Assert.IsType<MusicEvent.Played>(told.Single());
        ready = Assert.IsType<MusicPageState<AlbumView>.Ready>(album.State.Value);
        Assert.Equal(2L, ready.Current);
        Assert.True(ready.Playing);
        // The track playing toggles: it pauses once it plays.
        album.PlayOrToggle(1);
        Assert.Single(server.Sessions);
        album.Rate(8);
        Assert.Equal((9L, (double?)8), items.Ratings.Single());
    }

    [Fact]
    public void An_album_without_tracks_has_nothing_to_play_and_a_missing_one_is_not_found()
    {
        items.Set(new ItemDetail { Id = 9, Kind = "album", Title = "Empty" });
        items.SetChildren(9);
        var album = new AlbumModel(items, player, scheduler, 9);
        var told = new List<MusicEvent>();
        album.Told += told.Add;
        album.Play();
        Assert.IsType<MusicEvent.NothingToPlay>(told.Single());
        items.Fail(10, new ApiException(404, "not_found", "no such item"));
        Assert.IsType<MusicPageState<AlbumView>.NotFound>(new AlbumModel(items, player, scheduler, 10).State.Value);
        items.Fail(11, new ApiException(500, "internal", "broken"));
        Assert.IsType<MusicPageState<AlbumView>.Failed>(new AlbumModel(items, player, scheduler, 11).State.Value);
    }

    [Fact]
    public void An_artists_albums_are_newest_first_and_play_gathers_every_track()
    {
        items.Set(new ItemDetail { Id = 3, Kind = "artist", Title = "Lights", ChildCount = 2, LeafCount = 3 });
        items.SetChildren(3,
            new ItemSummary { Id = 8, Kind = "album", Title = "Old", Year = 2001 },
            new ItemSummary { Id = 9, Kind = "album", Title = "New", Year = 2019 });
        items.SetChildren(8, Track(81, 1));
        items.SetChildren(9, Track(91, 1), Track(92, 2));
        var artist = new ArtistModel(items, player, scheduler, 3);
        var page = Assert.IsType<MusicPageState<ArtistView>.Ready>(artist.State.Value).Page;
        Assert.Equal("2 albums · 3 tracks", page.Counts);
        Assert.Equal(new[] { 9L, 8L }, page.Albums.Select(a => a.Id));
        artist.Play();
        // In the server's order of the albums, as the web's play does.
        Assert.Equal(new[] { 81L, 91L, 92L }, player.Queue.Value.Tracks.Select(t => t.Id));
    }

    [Fact]
    public void Now_playing_lists_the_queue_and_says_what_the_buttons_do()
    {
        var model = new NowPlayingModel(player, FakeMusicServer.Server);
        Assert.Null(model.State.Value.Now);
        player.Play(new[] { Track(1, 1), Track(2, 2) });
        var state = model.State.Value;
        Assert.Equal(new[] { true, false }, state.Queue.Select(r => r.Current));
        Assert.Equal(("Lights", "3:12", "1"), (state.Queue[0].Subtitle, state.Queue[0].Duration, state.Queue[0].Number));
        Assert.Equal("Loading…", model.Status);
        Assert.True(model.ShowsPause);
        Assert.Equal("Shuffle", model.ShuffleLabel);
        Assert.Equal("Sleep timer", model.SleepLabel);
        model.StepSleep();
        Assert.Equal("Sleep timer: 15 min left", model.SleepLabel);
        model.SeekBy(1);
        Assert.Equal(("0:10", "−3:02"), model.Times(new MusicPosition(10_000, 192_000)));
        model.JumpTo(1);
        Assert.Equal(new[] { false, true }, model.State.Value.Queue.Select(r => r.Current));
    }
}
