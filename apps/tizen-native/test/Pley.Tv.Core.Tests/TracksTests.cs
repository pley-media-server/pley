using System.Collections.Generic;
using System.Linq;
using System.Text.Json;
using Pley.Tv.Core.Api;
using Pley.Tv.Core.Player;
using Pley.Tv.Core.Ui;
using Xunit;

namespace Pley.Tv.Core.Tests;

/// <summary>
/// The remote's audio and subtitles, as the fixture packages/tv's tracks.test.ts reads too has
/// them: what a video declares, and what it does with a track command.
/// </summary>
public class TrackRuleTests
{
    public static IEnumerable<object[]> ControlsCases => Indexes("controls");
    public static IEnumerable<object[]> CommandCases => Indexes("commands");

    static IEnumerable<object[]> Indexes(string rule) =>
        Enumerable.Range(0, Fixtures.Load("tracks").GetProperty(rule).GetArrayLength()).Select(i => new object[] { i });

    static JsonElement Case(string rule, int i) => Fixtures.Load("tracks").GetProperty(rule)[i];

    static int? Int(JsonElement e) => e.ValueKind == JsonValueKind.Null ? null : e.GetInt32();

    [Theory]
    [MemberData(nameof(ControlsCases))]
    public void A_video_declares_audio_and_subtitles_as_the_fixture_has_it(int i)
    {
        var c = Case("controls", i);
        var input = c.GetProperty("in");
        var declared = Remote.VideoControls(input.GetProperty("has_next").GetBoolean(), input.GetProperty("audio").GetInt32(), input.GetProperty("subtitles").GetInt32());
        Assert.Equal(c.GetProperty("out").EnumerateArray().Select(e => e.GetString()), declared);
    }

    [Theory]
    [MemberData(nameof(CommandCases))]
    public void A_track_command_does_what_the_fixture_says(int i)
    {
        var c = Case("commands", i);
        var input = c.GetProperty("in");
        var p = input.GetProperty("player");
        var player = new TrackPlayer(p.GetProperty("session_id").GetString(),
            p.GetProperty("audio").EnumerateArray().Select(e => e.GetInt32()).ToList(),
            p.GetProperty("subtitles").EnumerateArray().Select(e => e.GetInt32()).ToList(),
            Int(p.GetProperty("audio_index")), p.GetProperty("subtitle_index").GetInt32());
        var command = input.GetProperty("command");
        var action = Remote.Track(player, new ServerEvent
        {
            Type = "player_command",
            SessionId = command.GetProperty("session_id").GetString(),
            Command = command.GetProperty("command").GetString(),
            StreamIndex = Int(command.GetProperty("stream_index")),
        });
        var written = action.Act == TrackAct.None
            ? new Dictionary<string, object?> { ["act"] = "none", ["report"] = action.Report }
            : new Dictionary<string, object?> { ["act"] = action.Act == TrackAct.Audio ? "audio" : "subtitles", ["stream_index"] = action.Index, ["report"] = action.Report };
        Fixtures.SameFields(input, written, c.GetProperty("out"));
    }
}

/// <summary>A video's audio and subtitles changed by the remote, and the tracks every report says.</summary>
public class VideoTrackTests
{
    readonly PlayerHarness h = new PlayerHarness();

    /// <summary>A film with English and French audio, and English text and French picture subtitles.</summary>
    static ItemDetail Film(MediaFile? file = null) => new ItemDetail
    {
        Id = 1, LibraryId = 1, Kind = "movie", Title = "Film", Year = 2024, DurationMs = 2_700_000, Files = new[] { file ?? PlayerHarness.FileOf(10) },
    };

    static ServerEvent Command(string command, int? index, string session = "s1") =>
        new ServerEvent { Type = "player_command", SessionId = session, Command = command, StreamIndex = index };

    static IEnumerable<string?> Controls(IReadOnlyDictionary<string, object?> body) => ((IEnumerable<string>)body["controls"]!).Select(c => (string?)c);

    [Fact]
    public void A_video_declares_audio_and_subtitles_when_its_file_has_them()
    {
        h.Add(Film());
        h.Start(new PlayRequest(1, 0, true));
        Assert.Equal(new[] { "play", "pause", "seek", "skip", "audio", "subtitles" }, Controls(h.LastBody));

        var bare = new PlayerHarness().Add(Film(PlayerHarness.FileOf(10) with { Streams = new[] { PlayerHarness.Video, PlayerHarness.English } }));
        bare.Start(new PlayRequest(1, 0, true));
        Assert.Equal(new[] { "play", "pause", "seek", "skip", "audio" }, Controls(bare.LastBody));
    }

    [Fact]
    public void The_audio_switches_in_place_as_the_panel_does_it_saved_as_a_pick_and_reported_at_once()
    {
        h.Add(Film());
        h.Start(new PlayRequest(1, 0, true));
        h.Play(4_000);
        h.Source.Emit(Command("audio", 2));
        Assert.Equal((1, "fre"), h.Engine.AudioSwitches.Last());
        Assert.Single(h.Source.Bodies);
        Assert.Equal((1L, (long?)10, (int?)2, (int?)null), h.Source.Streams.Single());
        Assert.Equal((4_000L, "playing"), (h.Source.Reports.Single().Position, h.Source.Reports.Single().State));
        Assert.Equal(new PlayingTracks(2, -1), h.Source.ReportTracks.Single());
        Assert.True(h.State.Audio.Single(a => a.Index == 2).Selected);
    }

    [Fact]
    public void Audio_a_stream_plays_starts_a_new_session_that_says_what_it_plays_as_it_begins()
    {
        h.Add(Film());
        h.Source.Answer = body => FakePlayerSource.Stream((long)body["file_id"]!) with { AudioIndex = body.TryGetValue("audio_index", out var a) ? (int?)a : 1 };
        h.Start(new PlayRequest(1, 0, true));
        h.Play(5_000);
        h.Source.Emit(Command("audio", 2));
        Assert.Equal(2, h.Source.Bodies.Count);
        Assert.Equal(2, h.LastBody["audio_index"]);
        // The session it leaves says what it played; the new one, what it plays, at once.
        Assert.Equal(new[] { ("stopped", 5_000L, "s1"), ("playing", 5_000L, "s2") }, h.Source.Reports.Select(r => (r.State, r.Position, r.Session)));
        Assert.Equal(new PlayingTracks?[] { new PlayingTracks(1, -1), new PlayingTracks(2, -1) }, h.Source.ReportTracks);
    }

    [Fact]
    public void Subtitles_go_on_and_off_as_the_panel_has_them_and_the_tracks_are_reported_at_once()
    {
        h.Add(Film());
        h.Start(new PlayRequest(1, 0, true));
        h.Play(4_000);
        h.Source.Emit(Command("subtitles", 3));
        h.Source.Emit(Command("subtitles", -1));
        Assert.Single(h.Source.Bodies);
        Assert.Equal(new PlayingTracks?[] { new PlayingTracks(1, 3), new PlayingTracks(1, -1) }, h.Source.ReportTracks);
        Assert.Equal(new[] { (1L, (long?)10, (int?)null, (int?)3), (1L, (long?)10, (int?)null, (int?)null) }, h.Source.Streams);
    }

    [Fact]
    public void A_pick_of_what_plays_is_said_again_and_another_sessions_or_a_track_the_file_lacks_does_nothing()
    {
        h.Add(Film());
        h.Start(new PlayRequest(1, 0, true));
        h.Play(4_000);
        h.Source.Emit(Command("audio", 1));
        h.Source.Emit(Command("subtitles", -1));
        h.Source.Emit(Command("audio", 2, session: "s2"));
        h.Source.Emit(Command("audio", 9));
        h.Source.Emit(Command("subtitles", 1));
        h.Source.Emit(Command("audio", null));
        Assert.Equal(new PlayingTracks?[] { new PlayingTracks(1, -1), new PlayingTracks(1, -1) }, h.Source.ReportTracks);
        Assert.Single(h.Source.Bodies);
        Assert.Empty(h.Source.Streams);
    }

    [Fact]
    public void Every_report_says_which_tracks_play_the_end_too()
    {
        h.Add(Film(PlayerHarness.FileOf(10) with { SelectedSubtitle = 3 }));
        h.Start(new PlayRequest(1, 0, true));
        h.Play(PlayerModel.ProgressMs);
        h.Model.Pause();
        h.Model.Close();
        Assert.Equal(new[] { "playing", "paused", "stopped" }, h.Source.Reports.Select(r => r.State));
        Assert.All(h.Source.ReportTracks, t => Assert.Equal(new PlayingTracks(1, 3), t));
    }

    [Fact]
    public void A_session_the_server_stopped_changes_no_track()
    {
        h.Add(Film());
        h.Start(new PlayRequest(1, 0, true));
        h.Source.Emit(new ServerEvent { Type = "session_stopped", SessionId = "s1", ByAdmin = false });
        var reports = h.Source.Reports.Count;
        h.Source.Emit(Command("audio", 2));
        h.Source.Emit(Command("subtitles", 3));
        Assert.Equal(reports, h.Source.Reports.Count);
        Assert.Empty(h.Source.Streams);
    }
}

/// <summary>Music declares no tracks, and the remote's audio and subtitles do nothing to it.</summary>
public class MusicTrackTests
{
    [Fact]
    public void Music_has_no_tracks_to_change()
    {
        var server = new FakeMusicServer();
        var music = new MusicPlayer(server, () => new FakeEngine(), new FakeScheduler());
        music.Play(new[] { new ItemSummary { Id = 11, Kind = "track", Title = "Track 1", DurationMs = 200_000 } });
        music.Obey(new ServerEvent { Type = "player_command", SessionId = "m1", Command = "audio", StreamIndex = 1 });
        music.Obey(new ServerEvent { Type = "player_command", SessionId = "m1", Command = "subtitles", StreamIndex = -1 });
        Assert.Empty(server.Last.Reports);
        Assert.Single(server.Asked);
        Assert.DoesNotContain("audio", Remote.MusicControls);
    }
}
