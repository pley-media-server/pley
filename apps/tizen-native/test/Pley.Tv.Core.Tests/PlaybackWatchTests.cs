using System.Collections.Generic;
using Pley.Tv.Core.Player;
using Xunit;

namespace Pley.Tv.Core.Tests;

public class PlaybackWatchTests
{
    long clock = 1_000;
    readonly List<string> said = new List<string>();

    PlaybackWatch Watch(long startMs = 0, bool play = true)
    {
        var w = new PlaybackWatch(startMs, play, () => clock);
        w.StatusChanged += s => said.Add(s.ToString());
        w.PlayingChanged += p => said.Add(p ? "playing" : "not playing");
        w.FirstMove += () => said.Add("first move");
        w.Settled += at => said.Add($"settled at {at}");
        return w;
    }

    /// <summary>A read of a playing player at <paramref name="raw"/>, <paramref name="ms"/> later.</summary>
    void Plays(PlaybackWatch w, long raw, long ms = 40)
    {
        clock += ms;
        w.Read(w.Epoch, raw, playing: true);
    }

    /// <summary>
    /// Prepared, it waits until its first picture moves: ready in between, the screen's spinner went
    /// and came back, and the model took the picture to be out.
    /// </summary>
    [Fact]
    public void A_source_to_play_waits_from_its_prepare_until_its_picture_moves()
    {
        var w = Watch();
        Assert.Equal(EngineStatus.Buffering, w.Status);
        w.Opened();
        Assert.Equal(EngineStatus.Buffering, w.Status);
        Assert.Empty(said);
        Assert.Equal(PlaybackWatch.StartEveryMs, w.ReadEveryMs);
        // Where it landed, then standing there: not yet.
        Plays(w, 0);
        Plays(w, 0);
        Plays(w, 50);
        Assert.Empty(said);
        Plays(w, 120);
        Assert.Equal(new[] { "first move", "Ready", "playing" }, said);
        Assert.True(w.Moved);
        Assert.Equal(PlaybackWatch.EveryMs, w.ReadEveryMs);
    }

    [Fact]
    public void A_source_prepared_paused_is_ready_and_waits_once_played()
    {
        var w = Watch(play: false);
        w.Opened();
        Assert.Equal(new[] { "Ready" }, said);
        Assert.False(w.Playing);
        w.Play();
        Assert.Equal(new[] { "Ready", "Buffering" }, said);
        Plays(w, 0);
        Plays(w, 100);
        Assert.Equal(new[] { "Ready", "Buffering", "first move", "Ready", "playing" }, said);
    }

    /// <summary>A position standing still while it should play: waiting after <see cref="PlaybackWatch.StillMs"/>, playing again once it moves.</summary>
    [Fact]
    public void Standing_still_while_it_should_play_is_waiting_for_data()
    {
        var w = Watch();
        w.Opened();
        Plays(w, 0);
        Plays(w, 100);
        said.Clear();
        Plays(w, 600, 500);
        Plays(w, 600, 500);
        Plays(w, 600, 500);
        Plays(w, 600, 400);
        Assert.Empty(said);
        Plays(w, 600, 100);
        Assert.Equal(new[] { "Buffering", "not playing" }, said);
        Plays(w, 700, 500);
        Assert.Equal(new[] { "Buffering", "not playing", "Ready", "playing" }, said);
    }

    [Fact]
    public void Paused_it_doesnt_wait()
    {
        var w = Watch();
        w.Opened();
        Plays(w, 0);
        Plays(w, 100);
        w.Pause();
        said.Clear();
        for (var i = 0; i < 6; i++)
        {
            clock += 500;
            w.Read(w.Epoch, 100, playing: false);
        }
        Assert.Empty(said);
        Assert.Equal(EngineStatus.Ready, w.Status);
    }

    /// <summary>
    /// After a seek it waits until it plays on from where the player says it landed, a little off
    /// where it was sent: taken as moving at that first read, the spinner went, came back as it
    /// stood still, then went.
    /// </summary>
    [Fact]
    public void A_seek_waits_until_it_plays_on_from_where_it_landed()
    {
        var w = Watch();
        w.Opened();
        Plays(w, 0);
        Plays(w, 100);
        said.Clear();
        w.Seeking(60_000);
        Assert.Equal(60_000, w.PositionMs);
        // A read asked before the seek is dropped.
        var before = w.Epoch - 1;
        w.Read(before, 5_000, playing: true);
        Assert.Equal(60_000, w.PositionMs);
        Assert.True(w.Sought(60_000));
        Assert.Equal(new[] { "Buffering", "not playing" }, said);
        Assert.Equal(PlaybackWatch.SettleEveryMs, w.ReadEveryMs);
        Plays(w, 61_000, 100);
        Plays(w, 61_000, 2_000);
        Plays(w, 61_050, 100);
        Assert.Equal(new[] { "Buffering", "not playing" }, said);
        Plays(w, 61_100, 100);
        Assert.Equal(new[] { "Buffering", "not playing", "settled at 61000", "Ready", "playing" }, said);
        Assert.Equal(PlaybackWatch.EveryMs, w.ReadEveryMs);
    }

    [Fact]
    public void A_seek_overtaken_by_another_is_done_by_the_last()
    {
        var w = Watch();
        w.Opened();
        Plays(w, 0);
        Plays(w, 100);
        w.Seeking(10_000);
        w.Seeking(20_000);
        Assert.False(w.Sought(10_000));
        Assert.Equal(20_000, w.PositionMs);
        Assert.True(w.Sought(20_000));
        Assert.Equal(20_000, w.PositionMs);
    }

    [Fact]
    public void A_seek_while_paused_settles_at_once()
    {
        var w = Watch();
        w.Opened();
        Plays(w, 0);
        Plays(w, 100);
        w.Pause();
        said.Clear();
        w.Seeking(30_000);
        w.Sought(30_000);
        Assert.Equal(EngineStatus.Ready, w.Status);
        clock += 100;
        w.Read(w.Epoch, 30_000, playing: false);
        Assert.Equal(new[] { "settled at 30000" }, said);
        Assert.Equal(PlaybackWatch.EveryMs, w.ReadEveryMs);
    }

    [Fact]
    public void A_seek_from_the_end_plays_again()
    {
        var w = Watch();
        w.Opened();
        Plays(w, 0);
        Plays(w, 100);
        w.Ended();
        Assert.Equal(EngineStatus.Ended, w.Status);
        w.Seeking(5_000);
        w.Sought(5_000);
        Assert.Equal(EngineStatus.Buffering, w.Status);
        Plays(w, 5_000, 100);
        Plays(w, 5_100, 100);
        Assert.Equal(EngineStatus.Ready, w.Status);
    }

    /// <summary>
    /// A trimmed playlist begins at 1 288 871 ms: the player counts from it (it says 0) or from the
    /// film (it says the film's time), learnt from its first read while it plays; a seek goes to the
    /// player's own time, and one before the window is told apart.
    /// </summary>
    [Theory]
    [InlineData(0, true)]
    [InlineData(1_288_871, false)]
    public void A_trimmed_playlists_player_is_put_on_the_films_timeline(long first, bool fromPlaylist)
    {
        var w = Watch(startMs: 1_289_100);
        w.Trimmed(1_288_871);
        w.Opened();
        Assert.Equal(1_288_871, w.PositionMs);
        Assert.Null(w.CountsFromPlaylist);
        Plays(w, first);
        Assert.Equal(fromPlaylist, w.CountsFromPlaylist);
        Plays(w, first + 120);
        Assert.Equal(1_288_871 + 120, w.PositionMs);
        Assert.Equal(fromPlaylist ? 60_000 : 1_348_871, w.ToPlayer(1_348_871));
        Assert.True(w.Before(1_200_000));
        Assert.False(w.Before(1_300_000));
    }

    [Fact]
    public void Not_trimmed_the_players_time_is_the_films()
    {
        var w = Watch(startMs: 30_000);
        Assert.Equal(30_000, w.PositionMs);
        w.Jumped(30_000);
        w.Opened();
        Plays(w, 30_000);
        Plays(w, 30_120);
        Assert.Equal(30_120, w.PositionMs);
        Assert.Equal(45_000, w.ToPlayer(45_000));
        Assert.False(w.Before(0));
        Assert.Null(w.CountsFromPlaylist);
    }

    [Fact]
    public void Between_reads_the_position_runs_on_while_it_plays_and_no_further_than_half_a_second()
    {
        var w = Watch();
        w.Opened();
        Plays(w, 0);
        Plays(w, 100);
        clock += 200;
        Assert.Equal(300, w.PositionMs);
        clock += 2_000;
        Assert.Equal(100 + PlaybackWatch.ProjectMs, w.PositionMs);
        w.Pause();
        Assert.Equal(100, w.PositionMs);
    }

    /// <summary>A read for the clock alone (paused, or between the watch's reads) moves the position on, and decides nothing.</summary>
    [Fact]
    public void A_read_for_the_clock_moves_the_position_and_decides_nothing()
    {
        var w = Watch();
        w.Opened();
        w.Read(w.Epoch, 400, playing: true, weigh: false);
        Assert.Equal(400, w.PositionMs);
        w.Read(w.Epoch, 900, playing: true, weigh: false);
        Assert.False(w.Moved);
        Assert.Equal(EngineStatus.Buffering, w.Status);
    }
}
