using System;
using System.Collections.Generic;
using System.Linq;
using System.Text.Json;
using System.Threading.Tasks;
using Pley.Tv.Core.Api;
using Pley.Tv.Core.Auth;
using Pley.Tv.Core.Live;
using Pley.Tv.Core.Player;
using Pley.Tv.Core.Ui;
using Xunit;

namespace Pley.Tv.Core.Tests;

/// <summary>
/// The rule every receiver of the remote follows, as the fixture packages/tv's remote.test.ts
/// reads too has it (the Android app's RemoteTest, MusicPlaybackTest and LivePlaybackTest pin the
/// same rule).
/// </summary>
public class RemoteRuleTests
{
    public static IEnumerable<object[]> ControlsCases => Indexes("controls");
    public static IEnumerable<object[]> VideoCases => Indexes("video");
    public static IEnumerable<object[]> MusicCases => Indexes("music");
    public static IEnumerable<object[]> LiveCases => Indexes("live");

    static IEnumerable<object[]> Indexes(string rule) =>
        Enumerable.Range(0, Fixtures.Load("remote").GetProperty(rule).GetArrayLength()).Select(i => new object[] { i });

    static JsonElement Case(string rule, int i) => Fixtures.Load("remote").GetProperty(rule)[i];

    static ServerEvent Command(JsonElement c) => new ServerEvent
    {
        Type = "player_command",
        SessionId = c.GetProperty("session_id").GetString(),
        Command = c.GetProperty("command").GetString(),
        PositionMs = Long(c.GetProperty("position_ms")),
        OffsetMs = Long(c.GetProperty("offset_ms")),
    };

    static long? Long(JsonElement e) => e.ValueKind == JsonValueKind.Null ? null : e.GetInt64();

    /// <summary>
    /// An action as the fixture writes it: what the player does, and whether its session reports at
    /// once, as the rule says.
    /// </summary>
    static string Written(RemoteAction a) => a.Act == RemoteAct.Seek
        ? Fixtures.Json(new Dictionary<string, object> { ["act"] = "seek", ["position_ms"] = a.PositionMs, ["report"] = a.Report })
        : Fixtures.Json(new Dictionary<string, object> { ["act"] = a.Act.ToString().ToLowerInvariant(), ["report"] = a.Report });

    static string Played(RemoteKind kind, JsonElement input)
    {
        var p = input.GetProperty("player");
        var player = new RemotePlayer(kind, p.GetProperty("session_id").GetString(), p.GetProperty("state").GetString() == "playing",
            p.GetProperty("position_ms").GetInt64(), Long(p.GetProperty("duration_ms")), p.GetProperty("has_next").GetBoolean(),
            p.TryGetProperty("index", out var index) ? index.GetInt32() : 0);
        return Written(Remote.Action(player, Command(input.GetProperty("command"))));
    }

    [Theory]
    [MemberData(nameof(ControlsCases))]
    public void Each_player_declares_what_the_fixture_says_it_obeys(int i)
    {
        var c = Case("controls", i);
        var input = c.GetProperty("in");
        var declared = input.GetProperty("kind").GetString() switch
        {
            "video" => Remote.VideoControls(input.GetProperty("has_next").GetBoolean()),
            "music" => Remote.MusicControls,
            _ => Remote.LiveControls(input.GetProperty("has_order").GetBoolean()),
        };
        Assert.Equal(c.GetProperty("out").EnumerateArray().Select(e => e.GetString()), declared);
    }

    [Theory]
    [MemberData(nameof(VideoCases))]
    public void A_video_obeys_as_the_fixture_has_it(int i)
    {
        var c = Case("video", i);
        Assert.Equal(Fixtures.Normalize(c.GetProperty("out").GetRawText()), Played(RemoteKind.Video, c.GetProperty("in")));
    }

    [Theory]
    [MemberData(nameof(MusicCases))]
    public void Music_obeys_as_the_fixture_has_it(int i)
    {
        var c = Case("music", i);
        Assert.Equal(Fixtures.Normalize(c.GetProperty("out").GetRawText()), Played(RemoteKind.Music, c.GetProperty("in")));
    }

    /// <summary>
    /// Through the player as the remote drives it: its channels a group of the lineup's (the channel
    /// playing, when it isn't among them, in another), channel up and down going through that group
    /// (<see cref="LivePlayerModel.Step"/>), and the channel it tunes read off the server's watches.
    /// Without an order to go through it is a playback alone, as a player without the model is.
    /// </summary>
    [Theory]
    [MemberData(nameof(LiveCases))]
    public void A_live_channel_obeys_as_the_fixture_has_it(int i)
    {
        var c = Case("live", i);
        var input = c.GetProperty("in");
        var p = input.GetProperty("player");
        var channels = p.GetProperty("channels");
        var order = channels.ValueKind == JsonValueKind.Null ? null : channels.EnumerateArray().Select(e => e.GetInt64()).ToList();
        var current = p.GetProperty("channel").GetInt64();
        var live = new FakeLive();
        live.Lineup.Clear();
        live.Lineup.AddRange((order ?? new List<long>()).Select(id => new LiveChannel { Id = id, Name = $"Ch {id}", Number = $"{id}", Group = "Order" }));
        if (order == null || !order.Contains(current))
            live.Lineup.Add(new LiveChannel { Id = current, Name = $"Ch {current}", Number = $"{current}", Group = "Elsewhere" });
        var scheduler = new FakeScheduler();
        var playback = new LivePlayback(live, () => new FakeEngine(), scheduler);
        using var vm = order == null ? null : new LivePlayerModel(live, playback, scheduler, current, new LiveCategory.Group("Order").Key);
        if (vm == null) playback.Tune(current);
        // The fixture's session is the one watched here (the first); another is any other.
        var command = Command(input.GetProperty("command"));
        command = command with { SessionId = command.SessionId == p.GetProperty("session_id").GetString() ? "s1" : "s9" };
        var watches = live.Controls.Count;
        live.Emit(command);
        // Whether its session reports at once, as the rule says: a new channel tells the server with its own.
        var report = Remote.Action(new RemotePlayer(RemoteKind.Live, "s1", HasOrder: order != null), command).Report;
        var tuned = live.Controls.Count > watches
            ? new Dictionary<string, object> { ["act"] = "channel", ["channel"] = long.Parse(live.LastWatch.Split(' ')[1]), ["report"] = report }
            : new Dictionary<string, object> { ["act"] = "none", ["report"] = report };
        Assert.Equal(Fixtures.Normalize(c.GetProperty("out").GetRawText()), Fixtures.Json(tuned));
    }

    [Fact]
    public void In_recently_watched_a_channel_not_in_it_counts_from_its_first_as_on_android()
    {
        // Deviation from packages/tv (and the fixture's channel not in the order, which goes to the
        // first): Android's neighbour counts from Recently watched's first channel when the one
        // playing isn't in it, so next goes to its second and previous to its last.
        var live = new FakeLive { Recent = new List<long> { 2, 1, 1001 } };
        var scheduler = new FakeScheduler();
        using var vm = new LivePlayerModel(live, new LivePlayback(live, () => new FakeEngine(), scheduler), scheduler, 5, new LiveCategory.Recent().Key);
        live.Emit(new ServerEvent { Type = "player_command", SessionId = "s1", Command = "next" });
        Assert.Equal("watch 1 tizen-1", live.LastWatch);
        vm.Tune(5);
        live.Emit(new ServerEvent { Type = "player_command", SessionId = "s3", Command = "previous" });
        Assert.Equal("watch 1001 tizen-1", live.LastWatch);
    }
}

public class RemoteApiTests
{
    const string Home = "https://pley.example.com";

    [Fact]
    public void A_command_is_read_with_its_place_or_offset_and_needs_its_session()
    {
        var events = new SseReader().Feed(
            "data: {\"type\":\"player_command\",\"session_id\":\"s1\",\"command\":\"seek\",\"position_ms\":120000,\"offset_ms\":null}\n\n" +
            "data: {\"type\":\"player_command\",\"session_id\":\"s1\",\"command\":\"skip\",\"position_ms\":null,\"offset_ms\":-10000}\n\n" +
            "data: {\"type\":\"player_command\",\"command\":\"pause\"}\n\n" +
            "data: {\"type\":\"player_command\",\"session_id\":\"s2\",\"command\":\"rewind\"}\n\n");
        Assert.Equal(new[] { ((string?)"s1", (string?)"seek", (long?)120_000, (long?)null), ("s1", "skip", null, -10_000), ("s2", "rewind", null, null) },
            events.Select(e => (e.SessionId, e.Command, e.PositionMs, e.OffsetMs)));
    }

    [Fact]
    public void Every_playback_says_it_plays_on_a_tv_and_music_obeys_every_command()
    {
        var caps = TizenCaps.Of(new TizenFacts());
        var video = PlayRequests.Body(new PlayRequest(7, 0, true), "tizen-ab", caps);
        Assert.Equal("tv", video["device_kind"]);
        // Asked without a player's choices: it obeys nothing but Stop.
        Assert.False(video.ContainsKey("controls"));
        var music = JsonDocument.Parse(JsonSerializer.Serialize(PlayRequests.MusicBody(11, 0, false, "tizen-ab", caps))).RootElement;
        Assert.Equal("tv", music.GetProperty("device_kind").GetString());
        Assert.Equal(new[] { "play", "pause", "seek", "skip", "next", "previous" }, music.GetProperty("controls").EnumerateArray().Select(e => e.GetString()));
    }

    [Fact]
    public async Task Watching_a_channel_says_what_its_player_obeys_and_that_it_plays_on_a_tv()
    {
        var fake = new FakeServer();
        var clock = new FakeClock();
        var tokens = new Tokens(Home, "client", new MemoryKeyValue(), fake.Transport, null, clock.Read);
        await tokens.Install(new StoredTokens(Home, "pley_at_0", "pley_rt_0", clock.Now + 3_600_000, "library play", Home + "/api/oauth/token", Home + "/api/oauth/revoke"), "library play");
        var api = new Api.Api(Home, tokens, fake.Transport);
        fake.On("POST", Home + "/api/live/channels/7/watch", 200, @"{""session_id"":""l1"",""url"":""/api/live/media/pley_mk_7/index.m3u8"",""kind"":""hls""}");
        await api.WatchChannel(7, "tizen-1", controls: Remote.LiveControls(true));
        await api.WatchChannel(7, "tizen-1");
        var bodies = fake.Requests.Where(r => r.Method == "POST").Select(r => JsonDocument.Parse(r.Body!).RootElement).ToList();
        Assert.Equal(new[] { "next", "previous" }, bodies[0].GetProperty("controls").EnumerateArray().Select(e => e.GetString()));
        Assert.Equal("tv", bodies[0].GetProperty("device_kind").GetString());
        Assert.Equal(0, bodies[1].GetProperty("controls").GetArrayLength());
        // The channel's MPEG-TS, read by this app and handed on, says so too.
        Assert.Equal("/api/live/channels/7/stream.ts?client_id=tizen%201-stream&device_kind=tv", Api.Api.LiveStreamPath(7, "tizen 1-stream"));
    }
}

/// <summary>A video obeying the remote (the Android app's RemoteTest).</summary>
public class VideoRemoteTests
{
    readonly PlayerHarness h = new PlayerHarness();

    static ServerEvent Command(string command, string session = "s1", long? positionMs = null, long? offsetMs = null) =>
        new ServerEvent { Type = "player_command", SessionId = session, Command = command, PositionMs = positionMs, OffsetMs = offsetMs };

    static IEnumerable<string?> Controls(IReadOnlyDictionary<string, object?> body) => ((IEnumerable<string>)body["controls"]!).Select(c => (string?)c);

    [Fact]
    public void A_video_declares_play_pause_seek_and_skip_and_next_where_something_can_come_next()
    {
        h.Add(PlayerHarness.Movie());
        h.Start(new PlayRequest(1, 0, true));
        // Its file has audio and subtitles, which the remote picks from (TracksTests), and a Quality menu (QualityTests).
        Assert.Equal(new[] { "play", "pause", "seek", "skip", "audio", "subtitles", "quality" }, Controls(h.LastBody));
        Assert.Equal("tv", h.LastBody["device_kind"]);

        var e = new PlayerHarness().Add(PlayerHarness.Episode(1, 1));
        e.Start(new PlayRequest(1, 0, true));
        Assert.Equal(new[] { "play", "pause", "seek", "skip", "next", "audio", "subtitles", "quality" }, Controls(e.LastBody));
    }

    [Fact]
    public void Play_and_pause_act_on_its_own_session_only_and_report_at_once()
    {
        h.Add(PlayerHarness.Movie());
        h.Start(new PlayRequest(1, 0, true));
        h.Play(4_000);
        h.Source.Emit(Command("pause", session: "s2"));
        Assert.True(h.Engine.Playing);
        Assert.Empty(h.Source.Reports);
        h.Source.Emit(Command("pause"));
        Assert.False(h.Engine.Playing);
        Assert.Equal((4_000L, "paused"), (h.Source.Reports.Single().Position, h.Source.Reports.Single().State));
        h.Source.Emit(Command("play"));
        Assert.True(h.Engine.Playing);
        Assert.Equal("playing", h.Source.Reports.Last().State);
        Assert.Equal(2, h.Source.Reports.Count);
    }

    [Fact]
    public void Play_while_playing_and_pause_while_paused_say_so_again_so_a_remote_a_little_behind_settles()
    {
        h.Add(PlayerHarness.Movie());
        h.Start(new PlayRequest(1, 0, true));
        h.Play(4_000);
        h.Source.Emit(Command("play"));
        Assert.True(h.Engine.Playing);
        Assert.Equal((4_000L, "playing"), (h.Source.Reports.Single().Position, h.Source.Reports.Single().State));
        h.Source.Emit(Command("pause"));
        h.Source.Emit(Command("pause"));
        Assert.False(h.Engine.Playing);
        Assert.Equal(new[] { "playing", "paused", "paused" }, h.Source.Reports.Select(r => r.State));
    }

    [Fact]
    public void A_seek_stays_within_the_title_a_skip_counts_from_where_it_is_and_both_report_at_once()
    {
        h.Add(PlayerHarness.Movie());
        h.Start(new PlayRequest(1, 0, true));
        h.Play(600_000);
        h.Source.Emit(Command("skip", offsetMs: 30_000));
        Assert.Equal(630_000, h.Engine.Seeks.Last());
        // Not a second later, as after the screen's seeks.
        Assert.Equal((630_000L, "playing"), (h.Source.Reports.Last().Position, h.Source.Reports.Last().State));
        h.Source.Emit(Command("seek", positionMs: 9_000_000));
        // The end, short of it as every seek stops.
        Assert.Equal(2_700_000 - PlayerModel.SeekEndMarginMs, h.Engine.Seeks.Last());
        Assert.Equal(2_700_000 - PlayerModel.SeekEndMarginMs, h.Source.Reports.Last().Position);
        h.Model.Pause();
        h.Source.Emit(Command("skip", offsetMs: -600_000));
        Assert.Equal(("paused", 2_700_000 - PlayerModel.SeekEndMarginMs - 600_000), (h.Source.Reports.Last().State, h.Source.Reports.Last().Position));
        // The report a second after a seek isn't sent again.
        var reports = h.Source.Reports.Count;
        h.Clock.Advance(PlayerModel.SeekReportMs);
        Assert.Equal(reports, h.Source.Reports.Count);
        // Without a place or an offset, nothing.
        h.Source.Emit(Command("seek"));
        h.Source.Emit(Command("skip"));
        Assert.Equal(3, h.Engine.Seeks.Count);
    }

    [Fact]
    public void Next_plays_what_comes_next_and_at_the_end_does_nothing_and_says_nothing_and_a_video_has_no_previous()
    {
        var next = PlayerHarness.Episode(2, 2);
        h.Add(PlayerHarness.Episode(1, 1)).Add(next);
        h.Source.NextOf[1] = next;
        h.Start(new PlayRequest(1, 0, true));
        h.Source.Emit(Command("previous"));
        Assert.Single(h.Source.Bodies);
        Assert.Empty(h.Engine.Seeks);
        h.Source.Emit(Command("next"));
        Assert.Equal(2L, h.LastBody["item_id"]);
        Assert.Equal(2, h.State.ItemId);
        // The screen's banner shows the next title as it begins: nothing is toasted for it.
        Assert.Empty(h.Toasts);

        // The last episode: the screen's Next says so, the remote's says nothing here (Android's).
        var last = new PlayerHarness().Add(PlayerHarness.Episode(1, 1));
        last.Start(new PlayRequest(1, 0, true));
        last.Source.Emit(Command("next"));
        Assert.Empty(last.Toasts);
        Assert.Single(last.Source.Bodies);
        // A movie declares no next, and obeys none.
        var movie = new PlayerHarness().Add(PlayerHarness.Movie());
        movie.Start(new PlayRequest(1, 0, true));
        movie.Source.Emit(Command("next"));
        Assert.Single(movie.Source.Bodies);
    }

    [Fact]
    public void What_it_does_is_toasted_as_briefly_as_the_captions_key_and_a_command_that_changes_nothing_says_nothing()
    {
        h.Add(PlayerHarness.Movie());
        h.Start(new PlayRequest(1, 0, true));
        h.Play(600_000);
        h.Source.Emit(Command("pause", session: "s2"));
        h.Source.Emit(Command("play"));
        Assert.Empty(h.Toasts);
        h.Source.Emit(Command("pause"));
        h.Source.Emit(Command("pause"));
        h.Source.Emit(Command("play"));
        // A skip says the offset asked for, as the seek badge does; a seek where it went, the end short of it.
        h.Source.Emit(Command("skip", offsetMs: 30_000));
        h.Source.Emit(Command("skip", offsetMs: -10_000));
        h.Source.Emit(Command("seek", positionMs: 754_000));
        h.Source.Emit(Command("seek", positionMs: 9_000_000));
        Assert.Equal(new[] { "Paused", "Playing", "+30 s", "−10 s", "Jumped to 12:34", "Jumped to 44:59" }, h.Toasts.Select(t => t.Text));
        Assert.All(h.Toasts, t => Assert.Equal((PlayerText.CaptionsToastMs, false), (t.Ms, t.Error)));
    }

    [Fact]
    public void The_remotes_pause_isnt_the_persons_until_it_plays_again_whoever_plays_it()
    {
        h.Add(PlayerHarness.Movie());
        h.Start(new PlayRequest(1, 0, true));
        h.Play(4_000);
        h.Model.Pause();
        Assert.False(h.State.PausedByRemote);
        h.Model.Play();
        h.Source.Emit(Command("pause"));
        Assert.True(h.State.PausedByRemote);
        Assert.Equal(PhaseKind.Paused, h.State.Phase.Kind);
        h.Model.Play();
        Assert.False(h.State.PausedByRemote);
        h.Source.Emit(Command("pause"));
        h.Source.Emit(Command("play"));
        Assert.False(h.State.PausedByRemote);
    }

    [Fact]
    public void A_session_the_server_stopped_obeys_nothing_more()
    {
        h.Add(PlayerHarness.Movie());
        h.Start(new PlayRequest(1, 0, true));
        h.Model.Pause();
        h.Source.Emit(new ServerEvent { Type = "session_stopped", SessionId = "s1", ByAdmin = false });
        var reports = h.Source.Reports.Count;
        h.Source.Emit(Command("play"));
        h.Source.Emit(Command("seek", positionMs: 0));
        Assert.False(h.Engine.Playing);
        Assert.Empty(h.Engine.Seeks);
        Assert.Equal(reports, h.Source.Reports.Count);
    }
}

/// <summary>Music obeying the remote (the Android app's MusicPlaybackTest).</summary>
public class MusicRemoteTests
{
    readonly FakeMusicServer server = new FakeMusicServer();
    readonly FakeScheduler scheduler = new FakeScheduler();
    readonly List<FakeEngine> engines = new List<FakeEngine>();

    static readonly IReadOnlyList<ItemSummary> Album = Enumerable.Range(1, 3)
        .Select(i => new ItemSummary { Id = 10 + i, Kind = "track", Title = $"Track {i}", ParentId = 9, DurationMs = 200_000 })
        .ToList();

    static ServerEvent Command(string command, string session = "m1", long? positionMs = null, long? offsetMs = null) =>
        new ServerEvent { Type = "player_command", SessionId = session, Command = command, PositionMs = positionMs, OffsetMs = offsetMs };

    MusicPlayer Player() => new MusicPlayer(server, () =>
    {
        var e = new FakeEngine();
        engines.Add(e);
        return e;
    }, scheduler, new Random(3));

    FakeEngine Engine => engines.Single();

    /// <summary>Track <paramref name="index"/> of the album playing, <paramref name="atMs"/> into it.</summary>
    MusicPlayer Playing(int index = 1, long atMs = 60_000)
    {
        var p = Player();
        p.Play(Album, index);
        Engine.SetPlaying(true);
        Engine.PositionMs = atMs;
        return p;
    }

    [Fact]
    public void Play_and_pause_act_on_the_tracks_session_only_and_report_at_once()
    {
        var p = Playing();
        p.Obey(Command("pause", session: "m2"));
        Assert.True(Engine.Playing);
        p.Obey(Command("pause"));
        Assert.False(Engine.Playing);
        Assert.Equal("paused 60000", server.Last.Reports.Last());
        p.Obey(Command("play"));
        Assert.True(Engine.Playing);
        Assert.Equal("playing 60000", server.Last.Reports.Last());
        Assert.Equal(2, server.Last.Reports.Count);
    }

    [Fact]
    public void Play_while_playing_and_pause_while_paused_say_so_again_so_a_remote_a_little_behind_settles()
    {
        var p = Playing();
        var before = server.Last.Reports.Count;
        p.Obey(Command("play"));
        Assert.True(Engine.Playing);
        p.Obey(Command("pause"));
        p.Obey(Command("pause"));
        Assert.False(Engine.Playing);
        Assert.Equal(new[] { "playing 60000", "paused 60000", "paused 60000" }, server.Last.Reports.Skip(before));
    }

    [Fact]
    public void A_seek_stays_within_the_track_and_a_skip_counts_from_where_it_is()
    {
        var p = Playing();
        p.Obey(Command("skip", offsetMs: 15_000));
        Assert.Equal(("seek 75000", "playing 75000"), (Engine.Calls.Last(), server.Last.Reports.Last()));
        // The engine doesn't know the length yet: the session's.
        p.Obey(Command("seek", positionMs: 300_000));
        Assert.Equal(("seek 200000", "playing 200000"), (Engine.Calls.Last(), server.Last.Reports.Last()));
        p.Obey(Command("skip", offsetMs: -600_000));
        Assert.Equal(("seek 0", "playing 0"), (Engine.Calls.Last(), server.Last.Reports.Last()));
    }

    [Fact]
    public void Next_and_previous_go_as_the_buttons_do()
    {
        var p = Playing(index: 1, atMs: 1_000);
        // Under 3 s into the second track: the first.
        p.Obey(Command("previous"));
        Assert.Equal(new[] { "12 at 0", "11 at 0" }, server.Asked);
        Engine.SetPlaying(true);
        // The first track: back to its start, the same session, reported.
        p.Obey(Command("previous", session: "m2"));
        Assert.Equal("seek 0", Engine.Calls.Last());
        Assert.Equal("playing 0", server.Last.Reports.Last());
        p.Obey(Command("next", session: "m2"));
        Engine.SetPlaying(true);
        p.Obey(Command("next", session: "m3"));
        Assert.Equal(new[] { "12 at 0", "11 at 0", "12 at 0", "13 at 0" }, server.Asked);
        Engine.PositionMs = 120_000;
        // The last track: next does nothing; previous past 3 s starts it again.
        p.Obey(Command("next", session: "m4"));
        Assert.Equal(4, server.Asked.Count);
        p.Obey(Command("previous", session: "m4"));
        Assert.Equal(("seek 0", "playing 0"), (Engine.Calls.Last(), server.Last.Reports.Last()));
        Assert.Equal(4, server.Asked.Count);
    }

    [Fact]
    public void Music_stopped_or_failed_obeys_nothing()
    {
        var p = Playing();
        p.StoppedByServer("m1");
        p.Obey(Command("play"));
        Assert.Single(server.Asked);
        Assert.False(Engine.Playing);
    }
}

/// <summary>Live TV obeying the remote and stopped from elsewhere (the Android app's LivePlaybackTest).</summary>
public class LiveRemoteTests
{
    readonly FakeLive live = new FakeLive();
    readonly FakeScheduler scheduler = new FakeScheduler();
    readonly List<FakeEngine> engines = new List<FakeEngine>();

    LivePlayback Playback() => new LivePlayback(live, () =>
    {
        var e = new FakeEngine();
        engines.Add(e);
        return e;
    }, scheduler);

    static ServerEvent Command(string command, string session = "s1") =>
        new ServerEvent { Type = "player_command", SessionId = session, Command = command };

    [Fact]
    public void The_player_declares_channel_up_and_down_and_the_remotes_next_and_previous_are_those()
    {
        var vm = new LivePlayerModel(live, Playback(), scheduler, 1, new LiveCategory.All().Key);
        Assert.Equal(new[] { "next", "previous" }, live.Controls.Single());
        live.Emit(Command("next", session: "s9"));
        Assert.Equal("watch 1 tizen-1", live.LastWatch);
        live.Emit(Command("next"));
        Assert.Equal("watch 2 tizen-1", live.LastWatch);
        // As channel down there: round the ends.
        live.Emit(Command("previous", session: "s2"));
        live.Emit(Command("previous", session: "s3"));
        Assert.Equal("watch 1200 tizen-1", live.LastWatch);
        // Live is watched as it airs: nothing else.
        foreach (var c in new[] { "pause", "play", "seek", "skip" }) live.Emit(Command(c, session: "s4"));
        Assert.Equal(4, live.Controls.Count);
        vm.Dispose();
        live.Emit(Command("next", session: "s4"));
        Assert.Equal(4, live.Controls.Count);
    }

    [Fact]
    public void A_channel_picked_while_the_remotes_step_looks_for_its_neighbour_stays()
    {
        using var vm = new LivePlayerModel(live, Playback(), scheduler, 1, new LiveCategory.All().Key);
        live.HoldAround = true;
        live.Emit(Command("next"));
        vm.Tune(5);
        live.AnswerAround();
        Assert.Equal(new[] { "watch 1 tizen-1", "watch 5 tizen-1" }, live.Sent.Where(s => s.StartsWith("watch")));
        Assert.Equal(5, vm.State.Value.ChannelId);
    }

    [Fact]
    public void A_step_whose_neighbour_comes_in_the_background_or_once_stopped_watches_nothing()
    {
        using var vm = new LivePlayerModel(live, Playback(), scheduler, 1, new LiveCategory.All().Key);
        live.HoldAround = true;
        live.Emit(Command("next"));
        vm.Stop();
        live.AnswerAround();
        Assert.Single(live.Controls);
        // Back in front: the channel it left, and nothing else.
        vm.Resume();
        Assert.Equal(new[] { "watch 1 tizen-1", "watch 1 tizen-1" }, live.Sent.Where(s => s.StartsWith("watch")));

        live.HoldAround = true;
        live.Emit(Command("next", session: "s2"));
        live.Emit(new ServerEvent { Type = "session_stopped", SessionId = "s2", ByAdmin = false });
        live.AnswerAround();
        Assert.Equal(2, live.Controls.Count);
        Assert.Equal(LiveTroubleKind.Stopped, vm.State.Value.Trouble?.Kind);
    }

    [Fact]
    public void Channel_up_on_the_tv_pressed_as_it_stopped_still_changes_channel()
    {
        using var vm = new LivePlayerModel(live, Playback(), scheduler, 1, new LiveCategory.All().Key);
        live.HoldAround = true;
        vm.Step(1);
        live.Emit(new ServerEvent { Type = "session_stopped", SessionId = "s1", ByAdmin = false });
        live.AnswerAround();
        Assert.Equal("watch 2 tizen-1", live.LastWatch);
    }

    [Fact]
    public void Without_an_order_to_go_through_it_obeys_nothing_but_stop()
    {
        var p = Playback();
        p.Tune(1);
        Assert.Empty(live.Controls.Single());
        live.Emit(Command("next"));
        Assert.Single(live.Controls);
    }

    [Fact]
    public void Stopped_from_elsewhere_it_stops_at_once_and_says_who_and_why()
    {
        var p = Playback();
        p.Tune(1);
        engines[0].Start();
        live.Emit(new ServerEvent { Type = "session_stopped", SessionId = "s9", ByAdmin = true });
        Assert.True(p.State.Value.Playing);
        live.Emit(new ServerEvent { Type = "session_stopped", SessionId = "s1", Message = " Bedtime ", ByAdmin = false });
        Assert.Equal(new LiveTrouble(LiveTroubleKind.Stopped, "Live TV was stopped from another device.\nBedtime"), p.State.Value.Trouble);
        Assert.Equal(LiveText.Stopped, LiveText.TroubleTitle(p.State.Value.Trouble!, "Pley News HD"));
        Assert.Equal("stop", engines[0].Calls.Last());
        // Its session is gone on the server: nothing to leave, and nothing tuned again by itself.
        Assert.Empty(live.Left);
        engines[0].Fail(new EngineError.Network("the stream ended"));
        scheduler.Advance(60_000);
        Assert.Single(live.Controls);
        Assert.Equal("Live TV was stopped from another device.\nBedtime", p.State.Value.Trouble?.Message);

        var admin = Playback();
        admin.Tune(2);
        live.Emit(new ServerEvent { Type = "session_stopped", SessionId = "s2", ByAdmin = true });
        Assert.Equal("Live TV was stopped by the server's administrator.", admin.State.Value.Trouble?.Message);
    }

    [Fact]
    public void Stopped_from_elsewhere_after_its_410_said_so_keeps_the_card_it_has_as_on_android()
    {
        // Deviation from packages/tv, which names who stopped it on a card without words: Android's
        // LivePlayback heeds session_stopped only while the session is its own, and a 410 the server
        // says ended it has let the session go.
        var p = Playback();
        p.Tune(1);
        engines[0].Start();
        live.Ended = "";
        engines[0].Fail(new EngineError.Http(410, "gone"));
        Assert.Equal(LiveTrouble.Stopped(null), p.State.Value.Trouble);
        live.Emit(new ServerEvent { Type = "session_stopped", SessionId = "s1", Message = "Bedtime", ByAdmin = false });
        Assert.Equal(LiveText.StoppedBody, p.State.Value.Trouble?.Message);
    }
}
