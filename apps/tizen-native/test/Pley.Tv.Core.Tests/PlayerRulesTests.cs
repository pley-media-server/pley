using System.Collections.Generic;
using System.Linq;
using Pley.Tv.Core.Api;
using Pley.Tv.Core.Player;
using Pley.Tv.Core.Ui;
using Xunit;

namespace Pley.Tv.Core.Tests;

/// <summary>The recovery ladder's transitions and budgets (the Android app's RecoveryTest).</summary>
public class RecoveryTests
{
    static readonly EngineError UnsupportedAudio = new EngineError.Unsupported("audio", Audio: true, Container: false);
    static readonly EngineError UnsupportedContainer = new EngineError.Unsupported("container", Audio: false, Container: true);
    static readonly EngineError UnsupportedVideo = new EngineError.Unsupported("video", Audio: false, Container: false);

    [Fact]
    public void Direct_play_falls_back_to_a_remux_for_h264_and_hevc_when_only_the_audio_or_container_failed_then_a_transcode()
    {
        var r = new Recovery();
        Assert.Equal(new RecoveryAction.NewSession("remux", FallbackReason.Unplayable), r.Decide(UnsupportedAudio, "direct", "hevc"));
        Assert.Equal("remux", r.FallbackMode);
        Assert.Equal(new RecoveryAction.NewSession("transcode", FallbackReason.Unplayable), r.Decide(UnsupportedAudio, "remux", "hevc"));
        Assert.Equal(new RecoveryAction.Fail(Failure.Unplayable, "audio"), r.Decide(UnsupportedAudio, "transcode", "hevc"));
    }

    [Fact]
    public void A_container_failure_of_h264_remuxes_of_other_video_transcodes_as_does_a_video_failure()
    {
        Assert.Equal("remux", ((RecoveryAction.NewSession)new Recovery().Decide(UnsupportedContainer, "direct", "h264")).Mode);
        Assert.Equal("transcode", ((RecoveryAction.NewSession)new Recovery().Decide(UnsupportedContainer, "direct", "vp9")).Mode);
        Assert.Equal("transcode", ((RecoveryAction.NewSession)new Recovery().Decide(UnsupportedVideo, "direct", "h264")).Mode);
    }

    [Fact]
    public void Each_fallback_happens_once()
    {
        var r = new Recovery();
        r.Decide(UnsupportedVideo, "direct", "h264");
        Assert.IsType<RecoveryAction.Fail>(r.Decide(UnsupportedVideo, "direct", "h264"));
        var s = new Recovery();
        s.Decide(UnsupportedAudio, "direct", "h264");
        s.Decide(UnsupportedAudio, "direct", "h264");
        Assert.IsType<RecoveryAction.Fail>(s.Decide(UnsupportedAudio, "direct", "h264"));
    }

    [Fact]
    public void Expired_sessions_and_network_errors_get_three_new_sessions_each()
    {
        var r = new Recovery();
        for (var i = 0; i < 3; i++) Assert.Equal(new RecoveryAction.NewSession(null, null), r.Decide(new EngineError.Http(404, "x"), "remux", "h264"));
        Assert.Equal(new RecoveryAction.Fail(Failure.Stopped, "x"), r.Decide(new EngineError.Http(410, "x"), "remux", "h264"));
        for (var i = 0; i < 3; i++) Assert.Equal(new RecoveryAction.NewSession(null, null), r.Decide(new EngineError.Network("n"), "remux", "h264"));
        Assert.Equal(new RecoveryAction.Fail(Failure.Network, "n"), r.Decide(new EngineError.Http(500, "n"), "remux", "h264"));
        Assert.IsType<RecoveryAction.Fail>(r.Decide(new EngineError.Other("o"), "remux", "h264"));
    }

    [Fact]
    public void Media_that_ends_short_is_a_transcode_for_direct_play_one_new_session_for_a_stream_then_a_failure()
    {
        var r = new Recovery();
        Assert.Equal(new RecoveryAction.NewSession("transcode", FallbackReason.Unplayable), r.OnEndedShort("direct"));
        Assert.Equal(new RecoveryAction.NewSession(null, null), r.OnEndedShort("transcode"));
        Assert.Equal(new RecoveryAction.Fail(Failure.Short, null), r.OnEndedShort("transcode"));
        // Retry gives the new session back, not the transcode: that was tried.
        r.Retry();
        Assert.Equal(new RecoveryAction.NewSession(null, null), r.OnEndedShort("direct"));
        Assert.Equal(new RecoveryAction.Fail(Failure.Short, null), r.OnEndedShort("direct"));
    }

    [Fact]
    public void Five_seconds_of_progress_and_retry_give_the_budgets_back()
    {
        var r = new Recovery();
        for (var i = 0; i < 3; i++) r.Decide(new EngineError.Playlist("p"), "remux", null);
        Assert.Equal(3, r.Recreate);
        r.OnPosition(10_000);
        r.OnPosition(14_999);
        Assert.Equal(3, r.Recreate);
        r.OnPosition(15_000);
        Assert.Equal(0, r.Recreate);
        for (var i = 0; i < 3; i++) r.Decide(new EngineError.Playlist("p"), "remux", null);
        // A seek: progress counts from where it lands.
        r.OnJump();
        r.OnPosition(100_000);
        r.OnPosition(104_000);
        Assert.Equal(3, r.Recreate);
        r.Retry();
        Assert.Equal(0, r.Recreate);
    }

    [Fact]
    public void Stalls_transcode_a_direct_play_with_data_ask_the_server_about_streams_and_count()
    {
        var r = new Recovery();
        Assert.Equal(new RecoveryAction.NewSession("transcode", FallbackReason.NotSmooth), r.OnStall("direct", starving: false));
        Assert.Equal(new RecoveryAction.CheckServer(), r.OnStall("transcode", starving: true));
        Assert.Equal(new RecoveryAction.SuggestLowerQuality(), r.AfterServerCheck(0.8, starving: true));
        Assert.Equal(new RecoveryAction.NewSession(null, null), r.AfterServerCheck(2.0, starving: false));
        Assert.Equal(new RecoveryAction.Fail(Failure.Stuck, null), r.AfterServerCheck(null, starving: false));
        for (var i = 0; i < 3; i++) Assert.Equal(new RecoveryAction.NewSession(null, null), r.AfterServerCheck(null, starving: true));
        Assert.Equal(new RecoveryAction.Fail(Failure.Stopped, null), r.AfterServerCheck(null, starving: true));
        // A starving direct play gets new sessions too.
        Assert.Equal(new RecoveryAction.NewSession(null, null), new Recovery().OnStall("direct", starving: true));
    }

    [Fact]
    public void Dropped_frames_switch_once_per_item_play_original_goes_back_a_new_item_starts_over()
    {
        var r = new Recovery();
        Assert.Null(r.OnStruggling("transcode"));
        Assert.Equal(new RecoveryAction.NewSession("transcode", FallbackReason.NotSmooth), r.OnStruggling("direct"));
        Assert.True(r.StruggleSwitched);
        r.PlayOriginal();
        Assert.Null(r.FallbackMode);
        Assert.Null(r.OnStruggling("direct"));
        r.NewItem();
        Assert.Equal(new RecoveryAction.NewSession("transcode", FallbackReason.NotSmooth), r.OnStruggling("remux"));
    }
}

/// <summary>The Android app's StruggleMeterTest.</summary>
public class StruggleMeterTests
{
    static FrameCounts Counts(long rendered, long dropped, int run = 0) => new FrameCounts(rendered, dropped, run);

    [Fact]
    public void Over_1_percent_dropped_in_10_s_struggles_1_percent_or_less_doesnt()
    {
        var m = new StruggleMeter();
        Assert.False(m.Sample(0, Counts(0, 0)));
        Assert.False(m.Sample(5_000, Counts(118, 2)));
        Assert.False(m.Sample(10_000, Counts(238, 2)));
        Assert.False(m.Sample(15_000, Counts(356, 4)));
        Assert.True(m.Sample(20_000, Counts(473, 7)));
    }

    [Fact]
    public void Needs_10_s_of_samples()
    {
        var m = new StruggleMeter();
        m.Sample(0, Counts(0, 0));
        Assert.False(m.Sample(9_000, Counts(100, 50)));
        Assert.True(m.Sample(10_000, Counts(110, 50)));
    }

    [Fact]
    public void Ten_frames_dropped_in_a_row_struggle_at_once()
    {
        var m = new StruggleMeter();
        m.Sample(0, Counts(0, 0, run: 3));
        Assert.True(m.Sample(2_000, Counts(48, 12, run: 10)));
    }

    [Fact]
    public void Counts_that_start_over_with_a_new_decoder_start_the_window_over()
    {
        var m = new StruggleMeter();
        m.Sample(0, Counts(1_000, 100));
        Assert.False(m.Sample(10_000, Counts(10, 0)));
        Assert.False(m.Sample(15_000, Counts(120, 0)));
        m.Reset();
        Assert.False(m.Sample(20_000, Counts(240, 50)));
    }
}

/// <summary>Skip Intro, Skip Credits and what comes next (the Android app's MarkersTest and NextUp's rules).</summary>
public class MarkersTests
{
    static Marker MarkerOf(string kind, long start, long end, long file = 1) => new Marker { FileId = file, Kind = kind, StartMs = start, EndMs = end, Source = "chapter" };

    static readonly Marker Intro = MarkerOf("intro", 10_000, 70_000);
    static readonly Marker Credits = MarkerOf("credits", 1_200_000, 1_300_000);

    [Fact]
    public void Takes_the_markers_of_the_file_being_played_else_the_first_file()
    {
        var other = MarkerOf("intro", 0, 5_000, file: 2);
        var odd = MarkerOf("recap", 0, 5_000);
        var item = new ItemDetail { Id = 1, Kind = "movie", Files = new[] { new MediaFile { Id = 1 }, new MediaFile { Id = 2 } }, Markers = new[] { Intro, other, odd } };
        Assert.Equal(new[] { other }, Markers.ForFile(item, 2));
        Assert.Equal(new[] { Intro }, Markers.ForFile(item, null));
        Assert.Empty(Markers.ForFile(item with { Markers = null }, 1));
    }

    [Fact]
    public void Finds_the_segment_the_playhead_is_in_until_a_second_before_it_ends()
    {
        var markers = new[] { Intro, Credits };
        Assert.Null(Markers.Active(markers, 9_900));
        Assert.Same(Intro, Markers.Active(markers, 10_000));
        Assert.Same(Intro, Markers.Active(markers, 68_900));
        Assert.Null(Markers.Active(markers, 69_000));
        Assert.Same(Credits, Markers.Active(markers, 1_250_000));
        Assert.Equal("Skip Intro", Markers.SkipLabel(Intro));
        Assert.Equal("Skip Credits", Markers.SkipLabel(Credits));
        Assert.Equal("Credits", Markers.Name(Credits));
    }

    [Fact]
    public void Lands_just_before_an_intro_ends_so_its_last_moment_plays_and_at_the_end_of_credits()
    {
        Assert.Equal(69_250, Markers.SkipTarget(Intro));
        Assert.Equal(1_300_000, Markers.SkipTarget(Credits));
        // Inside a very short intro.
        Assert.Equal(500, Markers.SkipTarget(MarkerOf("intro", 0, 1_000)));
    }

    [Fact]
    public void Announces_the_credits_once_even_when_seeking_back_into_them()
    {
        var gate = new CreditsGate();
        Assert.Null(gate.Entered(null));
        Assert.Null(gate.Entered(Intro));
        Assert.Same(Credits, gate.Entered(Credits));
        Assert.Null(gate.Entered(Credits));
        var other = MarkerOf("credits", 1_200_000, 1_300_000, file: 2);
        Assert.Same(other, gate.Entered(other));
        gate.Reset();
        Assert.Same(Credits, gate.Entered(Credits));
    }

    [Fact]
    public void What_comes_next_in_a_queue_and_where_it_starts()
    {
        ItemSummary Of(long id, string kind) => new ItemSummary { Id = id, Kind = kind, Title = $"#{id}" };
        var items = new[] { Of(1, "movie"), Of(2, "track"), Of(3, "episode"), Of(4, "extra") };
        Assert.Equal(3, NextUp.NextIn(items, 1)!.Id);
        Assert.Equal(4, NextUp.NextIn(items, 3)!.Id);
        Assert.Null(NextUp.NextIn(items, 4));
        Assert.Null(NextUp.NextIn(items, 99));
        Assert.True(NextUp.HasNext("episode", null));
        Assert.False(NextUp.HasNext("movie", null));
        Assert.True(NextUp.HasNext("movie", new PlayQueue.Playlist(1)));
        Assert.Equal(0, NextUp.StartOf(Of(1, "episode") with { UserState = new UserState { PositionMs = 50_000, Watched = true } }));
        Assert.Equal(50_000, NextUp.StartOf(Of(1, "episode") with { UserState = new UserState { PositionMs = 50_000 } }));
        Assert.Equal("S1 · E2", NextUp.LabelOf(Of(1, "episode") with { ParentIndex = 1, Index = 2 }));
        Assert.Null(NextUp.LabelOf(Of(1, "movie")));
    }
}

/// <summary>Speeds and the audio delay's steps (the Android app's PlaybackSpeeds and AudioDelays).</summary>
public class SpeedTests
{
    [Fact]
    public void Speeds_read_as_android_writes_them()
    {
        Assert.Equal(new[] { "0.5×", "0.75×", "1×", "1.25×", "1.5×", "2×" }, PlaybackSpeeds.All.Select(PlaybackSpeeds.Label));
    }

    [Fact]
    public void The_audio_delay_goes_in_50_ms_steps_within_5_s()
    {
        Assert.Equal(5_000, AudioDelays.Clamp(7_000));
        Assert.Equal(-5_000, AudioDelays.Clamp(-7_000));
        Assert.Equal(50, AudioDelays.Clamp(30));
        Assert.Equal(0, AudioDelays.Clamp(-25));
        Assert.Equal("+0.25 s", AudioDelays.Label(250));
        Assert.Equal("−1.5 s", AudioDelays.Label(-1_500));
        Assert.Equal("0 s", AudioDelays.Label(0));
    }
}

/// <summary>The control row's chapter rules and words (the Android app's TvPlayerScreen and TvPlayerText).</summary>
public class PlayerScreenRulesTests
{
    static readonly IReadOnlyList<ChapterMark> Chapters = new[]
    {
        new ChapterMark("Opening", 0, 60_000, null),
        new ChapterMark(null, 60_000, 120_000, null),
        new ChapterMark("Chapter 3", 120_000, 180_000, null),
    };

    [Fact]
    public void The_chapter_playing_and_its_names()
    {
        Assert.Equal(0, PlayerText.ChapterAt(Chapters, 0));
        Assert.Equal(1, PlayerText.ChapterAt(Chapters, 61_000));
        Assert.Null(PlayerText.ChapterAt(Chapters, 200_000));
        Assert.Null(PlayerText.ChapterAt(new ChapterMark[0], 0));
        Assert.Equal("Opening", PlayerText.ChapterTitle(Chapters[0], 0));
        Assert.Equal("Chapter 2", PlayerText.ChapterTitle(Chapters[1], 1));
        Assert.Equal("Chapter 1 · Opening", PlayerText.ChapterLine(Chapters[0], 0));
        Assert.Equal("Chapter 2", PlayerText.ChapterLine(Chapters[1], 1));
        Assert.Equal("Chapter 3", PlayerText.ChapterLine(Chapters[2], 2));
    }

    [Theory]
    [InlineData("00:06:19.212")]
    [InlineData("00:06:19")]
    [InlineData("6:19")]
    [InlineData(" 01:02:03,500 ")]
    public void A_chapter_named_only_for_where_it_starts_is_numbered(string title)
    {
        var chapter = new ChapterMark(title, 379_212, 500_000, null);
        Assert.Equal("Chapter 2", PlayerText.ChapterTitle(chapter, 1));
        Assert.Equal("Chapter 2", PlayerText.ChapterLine(chapter, 1));
    }

    [Fact]
    public void A_chapter_with_a_time_in_a_real_name_keeps_it()
    {
        var chapter = new ChapterMark("Act 2: 12:00 at Noon", 0, 1, null);
        Assert.Equal("Act 2: 12:00 at Noon", PlayerText.ChapterTitle(chapter, 0));
    }

    [Fact]
    public void Next_goes_to_a_chapter_more_than_a_second_ahead_previous_restarts_one_3_s_in()
    {
        Assert.Equal(60_000, PlayerText.NextChapterStart(Chapters, 30_000));
        Assert.Equal(120_000, PlayerText.NextChapterStart(Chapters, 59_500));
        Assert.Null(PlayerText.NextChapterStart(Chapters, 150_000));
        Assert.Equal(60_000, PlayerText.PreviousTarget(Chapters, 70_000));
        Assert.Equal(0, PlayerText.PreviousTarget(Chapters, 62_000));
        Assert.Equal(0, PlayerText.PreviousTarget(Chapters, 1_000));
        Assert.Equal(0, PlayerText.PreviousTarget(Chapters, 500_000));
    }

    [Fact]
    public void The_screens_words()
    {
        Assert.Equal("Ends at 22:41", PlayerText.EndsAt("22:41"));
        Assert.Equal("Next in 7", PlayerText.NextIn(7));
        Assert.Equal("Subtitles: English (SRT)", PlayerText.SubtitlesToast("English (SRT)"));
        Assert.Equal("The original didn't play smoothly on this device. Play original is in Quality.", PlayerText.TranscodeToast(SessionText.FallbackNotSmooth));
        Assert.Equal("The server can't keep up with this video. Try a lower quality. Choose one in Quality.", PlayerText.SlowServerToast(SessionText.SlowServer));
        Assert.Equal("32 minutes 40 seconds of 1 hour 2 minutes", PlayerText.Position(1_960_000, 3_720_000));
        Assert.Equal("0 seconds", PlayerText.Spoken(0));
        Assert.Equal("1 hour 1 second", PlayerText.Spoken(3_601_000));
    }
}
