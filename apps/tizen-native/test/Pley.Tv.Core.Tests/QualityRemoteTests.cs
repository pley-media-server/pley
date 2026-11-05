using System.Collections.Generic;
using System.Linq;
using Pley.Tv.Core.Api;
using Pley.Tv.Core.Player;
using Pley.Tv.Core.Ui;
using Xunit;

namespace Pley.Tv.Core.Tests;

/// <summary>The remote's quality rule (docs/API.md, "Remote control"), as the Android app's player obeys it.</summary>
public class QualityRuleTests
{
    static readonly QualityPlayer Player = new QualityPlayer("s1", new[] { "original", "auto", "8m", "4m", "2m" }, "auto");

    static ServerEvent Command(string? quality, string session = "s1", string command = "quality") =>
        new ServerEvent { Type = "player_command", SessionId = session, Command = command, Quality = quality };

    [Fact]
    public void A_quality_the_menu_offers_switches_the_one_picked_is_said_again_and_anything_else_does_nothing()
    {
        Assert.Equal(new QualityAction("8m", true), Remote.Quality(Player, Command("8m")));
        Assert.Equal(new QualityAction("original", true), Remote.Quality(Player, Command("original")));
        Assert.Equal(QualityAction.Same, Remote.Quality(Player, Command("auto")));
        Assert.Equal(QualityAction.None, Remote.Quality(Player, Command("30m")));
        Assert.Equal(QualityAction.None, Remote.Quality(Player, Command(null)));
        Assert.Equal(QualityAction.None, Remote.Quality(Player, Command("8m", session: "s2")));
        Assert.Equal(QualityAction.None, Remote.Quality(Player with { Session = null }, Command("8m")));
        Assert.Equal(QualityAction.None, Remote.Quality(Player, Command("8m", command: "audio")));
    }

    [Fact]
    public void A_video_with_a_quality_menu_declares_quality_last()
    {
        Assert.Equal(new[] { "play", "pause", "seek", "skip", "next", "audio", "subtitles", "quality" }, Remote.VideoControls(true, 2, 1, quality: true));
        Assert.Equal(new[] { "play", "pause", "seek", "skip", "quality" }, Remote.VideoControls(false, quality: true));
        Assert.DoesNotContain("quality", Remote.MusicControls);
        Assert.DoesNotContain("quality", Remote.LiveControls(true));
    }

    [Fact]
    public void The_quality_command_is_read_off_the_event_stream()
    {
        var e = new SseReader().Feed("data: {\"type\":\"player_command\",\"session_id\":\"s1\",\"command\":\"quality\",\"position_ms\":null,\"offset_ms\":null,\"stream_index\":null,\"quality\":\"8m\"}\n\n").Single();
        Assert.Equal(("quality", "8m"), (e.Command, e.Quality));
    }
}

/// <summary>A video's quality changed by the remote, and the quality every report says.</summary>
public class VideoQualityTests
{
    readonly PlayerHarness h = new PlayerHarness();

    static ServerEvent Command(string? quality, string session = "s1") =>
        new ServerEvent { Type = "player_command", SessionId = session, Command = "quality", Quality = quality };

    static IEnumerable<string?> Controls(IReadOnlyDictionary<string, object?> body) => ((IEnumerable<string>)body["controls"]!).Select(c => (string?)c);

    [Fact]
    public void A_file_on_a_drive_has_no_quality_menu_to_obey_or_report()
    {
        h.Source.Local = true;
        h.Add(PlayerHarness.Movie());
        h.Start(new PlayRequest(1, 0, true));
        Assert.DoesNotContain("quality", Controls(h.LastBody));
        h.Play(4_000);
        h.Source.Emit(Command("8m"));
        h.Model.Close();
        Assert.Single(h.Source.Bodies);
        Assert.All(h.Source.ReportQualities, Assert.Null);
    }

    [Fact]
    public void Every_report_says_the_quality_picked_the_end_too()
    {
        h.Add(PlayerHarness.Movie());
        h.Start(new PlayRequest(1, 0, true));
        h.Play(PlayerModel.ProgressMs);
        h.Model.Pause();
        h.Model.Close();
        Assert.Equal(new[] { "playing", "paused", "stopped" }, h.Source.Reports.Select(r => r.State));
        Assert.All(h.Source.ReportQualities, q => Assert.Equal("auto", q));
    }

    [Fact]
    public void A_lower_quality_starts_a_session_at_it_kept_as_the_panels_pick_that_says_so_as_it_begins()
    {
        h.Add(PlayerHarness.Movie());
        h.Start(new PlayRequest(1, 0, true));
        h.Play(5_000);
        h.Source.Emit(Command("8m"));
        Assert.Equal(2, h.Source.Bodies.Count);
        Assert.Equal((8_000, 1_080, "transcode"), ((int)h.LastBody["max_bitrate"]!, (int)h.LastBody["max_height"]!, (string)h.LastBody["mode"]!));
        Assert.Equal("8m", h.Source.Settings.Quality);
        Assert.True(h.State.Qualities.Single(q => q.Id == "8m").Selected);
        // The session it leaves ends; the new one says at once what it plays.
        Assert.Equal(new[] { ("stopped", 5_000L, "s1"), ("playing", 5_000L, "s2") }, h.Source.Reports.Select(r => (r.State, r.Position, r.Session)));
        Assert.Equal("8m", h.Source.ReportQualities.Last());
    }

    [Fact]
    public void Original_from_Automatic_plays_the_same_and_reports_at_once()
    {
        h.Add(PlayerHarness.Movie());
        h.Start(new PlayRequest(1, 0, true));
        h.Play(4_000);
        h.Source.Emit(Command("original"));
        Assert.Single(h.Source.Bodies);
        Assert.Equal("Original", h.Source.Settings.Quality);
        Assert.Equal((4_000L, "playing", "s1"), (h.Source.Reports.Single().Position, h.Source.Reports.Single().State, h.Source.Reports.Single().Session));
        Assert.Equal("original", h.Source.ReportQualities.Single());
    }

    [Fact]
    public void The_pick_it_has_is_said_again_and_another_sessions_or_one_not_offered_does_nothing()
    {
        h.Add(PlayerHarness.Movie());
        h.Start(new PlayRequest(1, 0, true));
        h.Play(4_000);
        h.Source.Emit(Command("auto"));
        h.Source.Emit(Command("8m", session: "s2"));
        h.Source.Emit(Command("30m"));
        h.Source.Emit(Command(null));
        Assert.Single(h.Source.Bodies);
        Assert.Equal("auto", h.Source.ReportQualities.Single());
        Assert.Equal("auto", h.Source.Settings.Quality);
    }

    [Fact]
    public void A_session_the_server_stopped_changes_no_quality()
    {
        h.Add(PlayerHarness.Movie());
        h.Start(new PlayRequest(1, 0, true));
        h.Source.Emit(new ServerEvent { Type = "session_stopped", SessionId = "s1", ByAdmin = false });
        var reports = h.Source.Reports.Count;
        h.Source.Emit(Command("8m"));
        Assert.Equal(reports, h.Source.Reports.Count);
        Assert.Single(h.Source.Bodies);
    }
}
