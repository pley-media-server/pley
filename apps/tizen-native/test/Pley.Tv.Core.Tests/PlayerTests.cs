using System.Collections.Generic;
using System.Linq;
using System.Text.Json;
using Pley.Tv.Core.Player;
using Pley.Tv.Core.Ui;
using Xunit;

namespace Pley.Tv.Core.Tests;

/// <summary>The player's seeking: the Android app's TvSeekerTest.</summary>
public class SeekerTests
{
    long position = 60_000;
    readonly List<long> seeks = new List<long>();

    Seeker Make(long duration = 3_600_000) => new Seeker(() => position, () => duration, seeks.Add);

    [Fact]
    public void Presses_in_a_row_add_up_and_seek_once_when_they_settle()
    {
        var s = Make();
        s.Press(1);
        s.Press(1);
        s.Press(1);
        Assert.Equal(90_000, s.Target);
        Assert.Equal(30_000, s.BadgeAt);
        s.Settle(2);
        Assert.Empty(seeks);
        s.Settle(s.Presses);
        Assert.Equal(new long[] { 90_000 }, seeks);
        Assert.Null(s.Target);
    }

    [Fact]
    public void A_seek_stops_at_the_start_and_short_of_the_end()
    {
        var s = Make(duration: 65_000);
        s.Press(1);
        Assert.Equal(64_000, s.Target);
        s.Cancel();
        position = 5_000;
        s.Press(-1);
        Assert.Equal(0, s.Target);
        // The badge says what was asked for.
        Assert.Equal(-10_000, s.BadgeAt);
    }

    /// <summary>How far a key held from 0 moves the target between <paramref name="from"/> and <paramref name="to"/>, repeating every 50 ms.</summary>
    static long Scrubbed(Seeker s, long from, long to)
    {
        var before = s.Target ?? 0;
        for (var t = from + 50; t <= to; t += 50) s.Repeat(1, t);
        return (s.Target ?? 0) - before;
    }

    [Fact]
    public void A_held_key_scrubs_twice_as_fast_every_1_5_s_and_waits_when_let_go()
    {
        var s = Make(duration: 36_000_000);
        s.Repeat(1, 0);
        Assert.True(s.Holding);
        s.Repeat(1, 50);
        Assert.Equal(60_000 + (long)(Seeker.HoldStartRate * System.Math.Pow(2, 50 / 1_500.0) * 50), s.Target);
        // A second held goes about twice as far as the second 1.5 s before it.
        Scrubbed(s, 50, 500);
        var early = Scrubbed(s, 500, 1_500);
        Scrubbed(s, 1_500, 2_000);
        var later = Scrubbed(s, 2_000, 3_000);
        Assert.InRange(later / (double)early, 1.9, 2.1);
        Assert.Empty(seeks);
        // Let go, it waits for OK as presses do: nothing yet, then once it settles.
        Assert.True(s.Release());
        Assert.False(s.Holding);
        Assert.Empty(seeks);
        s.Settle(s.Presses);
        Assert.Single(seeks);
    }

    [Fact]
    public void A_held_key_tops_out_at_a_whole_video_in_10_s_or_300_ms_a_ms()
    {
        Assert.Equal(20, Make().RateAt(0));
        Assert.Equal(40, Make().RateAt(1_500), 6);
        Assert.Equal(160, Make().RateAt(4_500), 6);
        // An hour's video: an hour in 10 s, 360 ms of video a ms.
        Assert.Equal(360, Make(duration: 3_600_000).RateAt(20_000));
        // A short one: 300 ms a ms at most.
        Assert.Equal(300, Make(duration: 600_000).RateAt(20_000));
    }

    [Fact]
    public void A_gap_between_repeats_counts_as_200_ms_at_most()
    {
        var s = Make();
        s.Repeat(1, 0);
        s.Repeat(1, 1_000);
        Assert.Equal(60_000 + (long)(Seeker.HoldStartRate * System.Math.Pow(2, 1_000 / 1_500.0) * 200), s.Target);
    }

    [Fact]
    public void Ok_confirms_a_coming_seek_and_one_just_made()
    {
        var s = Make();
        s.Press(1);
        Assert.True(s.Confirm());
        Assert.Equal(new long[] { 70_000 }, seeks);
        // Confirmed: the next OK pauses.
        Assert.False(s.Confirm());
        s.Press(1);
        s.Settle(s.Presses);
        Assert.True(s.Confirm());
        s.Press(1);
        s.Settle(s.Presses);
        s.Settled(s.Seeks);
        Assert.False(s.Confirm());
    }
}

public class PlayerTextTests
{
    [Fact]
    public void Times()
    {
        Assert.Equal("0:00", PlayerText.Time(-5));
        Assert.Equal("4:05", PlayerText.Time(245_000));
        Assert.Equal("32:40", PlayerText.Time(1_960_999));
        Assert.Equal("1:02:03", PlayerText.Time(3_723_000));
        Assert.Equal("−25:20", PlayerText.Remaining(0, 1_520_000));
        Assert.Equal("−0:00", PlayerText.Remaining(10_000, 5_000));
    }

    [Fact]
    public void Episode_lines_and_badges()
    {
        Assert.Equal("S1 · E2 · Pilot", PlayerText.EpisodeLine("S1 E2 — Pilot"));
        Assert.Null(PlayerText.EpisodeLine(" "));
        Assert.Equal(10, PlayerText.Seconds(-10_000));
        Assert.Equal("+30 s", PlayerText.Badge(30_000));
        Assert.Equal("−10 s", PlayerText.Badge(-10_000));
    }

    [Fact]
    public void Titles_as_the_android_player_writes_them()
    {
        var episode = new Api.ItemSummary { Kind = "episode", Title = "The Long Night", GrandparentTitle = "Ashfall", ParentIndex = 2, Index = 5 };
        Assert.Equal("Ashfall", PlayerText.Title(episode));
        Assert.Equal("S2 · E5 · The Long Night", PlayerText.Line(episode));
        var film = new Api.ItemSummary { Kind = "movie", Title = "Heat", Year = 1995 };
        Assert.Equal("Heat (1995)", PlayerText.Title(film));
        Assert.Null(PlayerText.Line(film));
    }
}

/// <summary>The play request and the TV's report: apps/tizen's caps tests, for the native app.</summary>
public class PlayRequestTests
{
    static JsonElement Json(object o) => JsonDocument.Parse(JsonSerializer.Serialize(o)).RootElement;

    [Fact]
    public void A_2025_set_reports_as_the_web_app_does()
    {
        var caps = Json(TizenCaps.Of(new TizenFacts { Tizen = 9, Year = 2025, Model = "QN65QN90FAFXZA", OutputMode = "PCM" }));
        Assert.Equal(2, caps.GetProperty("caps_version").GetInt32());
        Assert.False(caps.GetProperty("dolby_vision").GetBoolean());
        Assert.Equal(new[] { "hdr10", "hdr10plus", "hlg" }, caps.GetProperty("hdr_types").EnumerateArray().Select(e => e.GetString()));
        Assert.Equal(new[] { "h264", "hevc", "vp9", "av1", "mpeg2video", "mpeg4" }, caps.GetProperty("video_codecs").EnumerateArray().Select(e => e.GetString()));
        var hevc = caps.GetProperty("video").EnumerateArray().First(v => v.GetProperty("codec").GetString() == "hevc");
        Assert.Equal(156, hevc.GetProperty("max_level").GetInt32());
        Assert.Equal(new[] { "ts", "fmp4" }, caps.GetProperty("hls").GetProperty("segments").EnumerateArray().Select(e => e.GetString()));
        Assert.Empty(caps.GetProperty("audio_passthrough").EnumerateArray());
        Assert.Equal(6, caps.GetProperty("audio_channels").GetInt32());
        // Samsung's media specifications: 8 to 48 kHz, for every codec.
        Assert.Equal(48_000, caps.GetProperty("audio_max_sample_rate").GetInt32());
        Assert.Contains("ac4", caps.GetProperty("audio_decode").EnumerateArray().Select(e => e.GetString()));
        Assert.DoesNotContain("dts", caps.GetProperty("audio_codecs").EnumerateArray().Select(e => e.GetString()));
        Assert.Equal("high", caps.GetProperty("device").GetProperty("tier").GetString());
    }

    [Fact]
    public void Dolby_to_a_receiver_is_passed_through()
    {
        var caps = Json(TizenCaps.Of(new TizenFacts { OutputMode = "DOLBY_DIGITAL_PLUS" }));
        Assert.Equal(new[] { "ac3", "eac3" }, caps.GetProperty("audio_passthrough").EnumerateArray().Select(e => e.GetString()));
        Assert.Equal(8, caps.GetProperty("audio_channels").GetInt32());
    }

    [Fact]
    public void Model_years_follow_the_tizen_version()
    {
        Assert.Equal(2025, TizenFacts.YearOf(9));
        Assert.Equal(2020, TizenFacts.YearOf(5.5));
        Assert.Equal(2015, TizenFacts.YearOf(1));
    }

    [Fact]
    public void The_body_asks_for_keyed_urls_and_names_only_what_was_picked()
    {
        var caps = TizenCaps.Of(new TizenFacts());
        var plain = Json(PlayRequests.Body(new PlayRequest(7, 120_000, false), "tizen-ab", caps));
        Assert.Equal(7, plain.GetProperty("item_id").GetInt64());
        Assert.Equal(120_000, plain.GetProperty("start_ms").GetInt64());
        Assert.True(plain.GetProperty("keyed_urls").GetBoolean());
        Assert.Equal("tizen-ab", plain.GetProperty("client_id").GetString());
        Assert.False(plain.TryGetProperty("file_id", out _));
        Assert.False(plain.TryGetProperty("audio_index", out _));
        var picked = Json(PlayRequests.Body(new PlayRequest(7, 0, true, FileId: 3, AudioIndex: 2), "tizen-ab", caps));
        Assert.Equal(3, picked.GetProperty("file_id").GetInt64());
        Assert.Equal(2, picked.GetProperty("audio_index").GetInt32());
    }
}
