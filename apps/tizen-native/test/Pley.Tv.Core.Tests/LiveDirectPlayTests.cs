using System;
using System.Collections.Generic;
using System.Linq;
using System.Text.Json;
using Pley.Tv.Core.Api;
using Pley.Tv.Core.Live;
using Pley.Tv.Core.Player;
using Pley.Tv.Core.Ui;
using Xunit;

namespace Pley.Tv.Core.Tests;

/// <summary>Channels a TV plays as the provider sends them, by its own report, while the server's Direct Play is on.</summary>
public class LiveDirectPlayRuleTests
{
    /// <summary>A 2025 4K set, and one with a Full HD panel.</summary>
    static readonly ClientCaps Uhd = ClientCaps.Of(TizenCaps.Of(new TizenFacts()));
    static readonly ClientCaps Hd = ClientCaps.Of(TizenCaps.Of(new TizenFacts { Uhd = false }));

    /// <summary>A 4K HEVC channel with E-AC-3, which the server would make H.264 and AAC for players that can't.</summary>
    internal static LivePlan Hevc(int? depth = 10) => new LivePlan
    {
        VideoCodec = "hevc", Width = 3840, Height = 2160, FrameRate = 50, BitDepth = depth,
        AudioCodec = "eac3", AudioChannels = 6,
        Video = "h264_nvenc", Audio = "aac", OutHeight = 1080, VideoKbps = 8_000,
        DirectPlay = true, Kbps = 12_300,
    };

    static ClientCaps Decoders(params VideoDecoderCaps[] video) => new ClientCaps { Video = video, AudioDecode = new[] { "aac", "eac3" } };

    [Fact]
    public void Hevc_the_tv_decodes_plays_as_sent_though_the_plan_would_transcode_it()
    {
        Assert.True(LiveRules.PlaysAsSent(Hevc(10), Uhd));
        Assert.True(LiveRules.PlaysAsSent(Hevc(8), Uhd));
        // A bit depth not said is taken as 8.
        Assert.True(LiveRules.PlaysAsSent(Hevc(null), Decoders(new VideoDecoderCaps { Codec = "hevc", MaxBitDepth = 8 })));
        // Its sound alone converted, or its picture alone.
        Assert.True(LiveRules.PlaysAsSent(Hevc() with { Video = "copy" }, Uhd));
        Assert.True(LiveRules.PlaysAsSent(Hevc() with { Audio = "copy" }, Uhd));
        Assert.True(LiveRules.PlaysAsSent(new LivePlan { VideoCodec = "mpeg2video", Width = 1280, Height = 720, FrameRate = 50, AudioCodec = "mp2", Video = "libx264", Audio = "aac", DirectPlay = true }, Uhd));
    }

    [Fact]
    public void Hevc_without_a_decoder_for_it_stays_with_the_transcode()
    {
        var h264 = new VideoDecoderCaps { Codec = "h264", MaxWidth = 3840, MaxHeight = 2160, MaxFps = 60, MaxBitDepth = 8 };
        Assert.Equal(NotAsSent.Video, LiveRules.WhyNotAsSent(Hevc(8), Decoders(h264)));
        // A decoder of 8 bits takes no 10-bit picture.
        var eight = Decoders(h264, new VideoDecoderCaps { Codec = "hevc", MaxWidth = 3840, MaxHeight = 2160, MaxFps = 60, MaxBitDepth = 8 });
        Assert.True(LiveRules.PlaysAsSent(Hevc(8), eight));
        Assert.Equal(NotAsSent.Video, LiveRules.WhyNotAsSent(Hevc(10), eight));
        Assert.False(LiveRules.PlaysAsSent(Hevc(12), Uhd));
        // Its size and rate count where the report gives a decoder's.
        Assert.Equal(NotAsSent.Video, LiveRules.WhyNotAsSent(Hevc(), Hd));
        Assert.True(LiveRules.PlaysAsSent(Hevc() with { Width = 1920, Height = 1080 }, Hd));
        Assert.False(LiveRules.PlaysAsSent(Hevc() with { FrameRate = 100 }, Uhd));
        // A report without decoders is asked by its codecs' names.
        Assert.True(LiveRules.PlaysAsSent(Hevc(), new ClientCaps { VideoCodecs = new[] { "h264", "hevc" }, AudioCodecs = new[] { "eac3" } }));
        Assert.False(LiveRules.PlaysAsSent(Hevc(), new ClientCaps { VideoCodecs = new[] { "h264" }, AudioCodecs = new[] { "eac3" } }));
    }

    [Fact]
    public void Interlaced_video_stays_with_the_server_unless_it_sends_it_as_it_is()
    {
        var interlaced = new LivePlan { VideoCodec = "h264", Width = 1920, Height = 1080, FrameRate = 25, Interlaced = true, AudioCodec = "aac", Video = "h264_nvenc", Audio = "copy", DirectPlay = true };
        Assert.Equal(NotAsSent.Interlaced, LiveRules.WhyNotAsSent(interlaced, Uhd));
        Assert.True(LiveRules.PlaysAsSent(interlaced with { Interlaced = false }, Uhd));
        // Deinterlacing off on the server: sent as it is, played as it is.
        Assert.True(LiveRules.PlaysAsSent(interlaced with { Video = "copy" }, Uhd));
    }

    [Fact]
    public void With_direct_play_off_only_what_the_server_sends_as_it_is_plays_as_sent()
    {
        Assert.Equal(NotAsSent.Off, LiveRules.WhyNotAsSent(Hevc() with { DirectPlay = false }, Uhd));
        Assert.True(LiveRules.PlaysAsSent(new LivePlan { Video = "copy", Audio = "copy" }, Uhd));
        // Sound every Samsung TV decodes, as before there was such a setting.
        Assert.True(LiveRules.PlaysAsSent(new LivePlan { Video = "copy", Audio = "aac", AudioCodec = "ac3" }, Uhd));
    }

    [Fact]
    public void An_older_servers_plan_reads_without_direct_play_and_plays_as_it_did()
    {
        var plan = JsonSerializer.Deserialize<LivePlan>(
            @"{""video_codec"":""hevc"",""width"":3840,""height"":2160,""frame_rate"":50.0,""interlaced"":false,""audio_codec"":""eac3"",""audio_channels"":6,""video"":""h264_nvenc"",""audio"":""aac""}",
            ApiJson.Options)!;
        Assert.Null(plan.DirectPlay);
        Assert.Equal(0, plan.Kbps);
        Assert.Null(plan.BitDepth);
        Assert.Null(plan.OutHeight);
        Assert.Null(plan.VideoKbps);
        Assert.False(LiveRules.PlaysAsSent(plan, Uhd));
        // No word of a setting that server lacks.
        Assert.Equal(NotAsSent.Unknown, LiveRules.WhyNotAsSent(plan, Uhd));
        Assert.True(LiveRules.PlaysAsSent(plan with { Video = "copy" }, Uhd));
    }

    [Fact]
    public void A_plan_reads_what_the_provider_sends_and_what_a_transcode_makes()
    {
        var plan = JsonSerializer.Deserialize<LivePlan>(
            @"{""video_codec"":""hevc"",""width"":3840,""height"":2160,""frame_rate"":50.0,""interlaced"":false,""bit_depth"":10,""audio_codec"":""eac3"",""audio_channels"":6,""video"":""h264_nvenc"",""audio"":""aac"",""out_height"":1080,""video_kbps"":8000,""direct_play"":true,""kbps"":12300}",
            ApiJson.Options)!;
        Assert.Equal(Hevc(), plan);
    }

    [Fact]
    public void Sound_the_tv_neither_decodes_nor_passes_on_stays_with_the_server()
    {
        var dts = Hevc() with { AudioCodec = "dts" };
        Assert.Equal(NotAsSent.Audio, LiveRules.WhyNotAsSent(dts, Uhd));
        Assert.Equal(NotAsSent.Audio, LiveRules.WhyNotAsSent(dts with { Video = "copy" }, Uhd));
        // Sent as it is, there is nothing to decide.
        Assert.True(LiveRules.PlaysAsSent(dts with { Audio = "copy" }, Uhd));
        // Sound the plan doesn't name can't be said to play.
        Assert.Equal(NotAsSent.Unknown, LiveRules.WhyNotAsSent(Hevc() with { AudioCodec = null }, Uhd));
    }

    [Fact]
    public void Without_the_tvs_report_only_what_is_sent_as_it_is_plays_as_sent()
    {
        Assert.Equal(NotAsSent.Unknown, LiveRules.WhyNotAsSent(Hevc(), null));
        Assert.True(LiveRules.PlaysAsSent(new LivePlan { Video = "copy", Audio = "copy", DirectPlay = true }, null));
    }

    [Fact]
    public void Codecs_the_tvs_player_doesnt_read_out_of_mpeg_ts_stay_with_the_hls()
    {
        // The TV decodes AV1 and Opus from files; a provider's stream of them isn't given to its player.
        Assert.Contains(Uhd.Video, v => v.Codec == "av1");
        Assert.Contains("opus", Uhd.AudioDecode);
        Assert.Equal(NotAsSent.Unknown, LiveRules.WhyNotAsSent(Hevc() with { VideoCodec = "av1" }, Uhd));
        Assert.Equal(NotAsSent.Unknown, LiveRules.WhyNotAsSent(Hevc() with { AudioCodec = "opus" }, Uhd));
    }
}

/// <summary>Watching a channel the TV decodes as the provider sends it, and asking how it plays while Playback details show.</summary>
public class LiveDirectPlayTests
{
    const string Relayed = "http://127.0.0.1:5000/r/stream.ts";

    readonly FakeLive live = new FakeLive { StreamAt = _ => Relayed };
    readonly FakeScheduler scheduler = new FakeScheduler();
    readonly List<FakeEngine> engines = new List<FakeEngine>();

    LivePlayback Playback(TizenFacts? tv = null) => new LivePlayback(live, () =>
    {
        var e = new FakeEngine();
        engines.Add(e);
        return e;
    }, scheduler, TizenCaps.Of(tv ?? new TizenFacts()));

    int PlanAsks => live.Sent.Count(s => s.StartsWith("plan ", StringComparison.Ordinal));

    [Fact]
    public void A_channel_the_tv_decodes_plays_from_its_stream_ts_and_its_hls_is_never_asked_for()
    {
        live.PlanAnswer = LiveDirectPlayRuleTests.Hevc();
        var p = Playback();
        p.Tune(1);
        Assert.Equal(Relayed, engines[0].Source?.Url);
        Assert.Equal("play", engines[0].Calls.Last());
        // The server makes the HLS only for a player that asks for its playlist: no transcode here.
        Assert.DoesNotContain(live.Sent, s => s.StartsWith("prefetch", StringComparison.Ordinal));
        Assert.False(p.Direct);
        engines[0].Start();
        Assert.True(p.Direct);
    }

    [Fact]
    public void With_direct_play_off_or_no_decoder_for_it_the_channel_plays_its_hls()
    {
        live.PlanAnswer = LiveDirectPlayRuleTests.Hevc() with { DirectPlay = false };
        var p = Playback();
        p.Tune(1);
        Assert.True(engines[0].Source?.Hls);
        engines[0].Start();
        Assert.False(p.Direct);
        // A Full HD set has no decoder for a 4K channel.
        live.PlanAnswer = LiveDirectPlayRuleTests.Hevc();
        var hd = Playback(new TizenFacts { Uhd = false });
        hd.Tune(1);
        Assert.True(engines[1].Source?.Hls);
    }

    [Fact]
    public void A_stream_ts_that_fails_plays_as_hls_and_is_direct_play_no_more()
    {
        live.PlanAnswer = LiveDirectPlayRuleTests.Hevc();
        var p = Playback();
        p.Tune(1);
        engines[0].Start();
        engines[0].Fail(new EngineError.Other("it broke off"));
        Assert.False(p.Direct);
        Assert.True(engines[0].Source?.Hls);
        engines[0].Start();
        Assert.False(p.Direct);
        // Its plan still says it plays as sent: the details give no reason.
        Assert.Null(LiveDetails.Why(p.Plan.Value!, p.Caps));
    }

    [Fact]
    public void The_plan_is_asked_again_every_few_seconds_while_the_details_show()
    {
        live.PlanAnswer = LiveDirectPlayRuleTests.Hevc() with { Kbps = 0 };
        var p = Playback();
        p.Tune(1);
        engines[0].Start();
        Assert.Equal(0, p.Plan.Value?.Kbps);
        var tuned = PlanAsks;
        scheduler.Advance(10 * LivePlayback.DetailsMs);
        Assert.Equal(tuned, PlanAsks);

        live.PlanAnswer = LiveDirectPlayRuleTests.Hevc();
        p.SetDetailsOpen(true);
        Assert.Equal(tuned + 1, PlanAsks);
        Assert.Equal(12_300, p.Plan.Value?.Kbps);
        scheduler.Advance(LivePlayback.DetailsMs - 1);
        Assert.Equal(tuned + 1, PlanAsks);
        live.PlanAnswer = LiveDirectPlayRuleTests.Hevc() with { Kbps = 11_000 };
        scheduler.Advance(1);
        Assert.Equal(tuned + 2, PlanAsks);
        Assert.Equal(11_000, p.Plan.Value?.Kbps);
        // Not known this time: what was known stays.
        live.PlanAnswer = null;
        scheduler.Advance(LivePlayback.DetailsMs);
        Assert.Equal(tuned + 3, PlanAsks);
        Assert.Equal(11_000, p.Plan.Value?.Kbps);

        p.SetDetailsOpen(false);
        scheduler.Advance(10 * LivePlayback.DetailsMs);
        Assert.Equal(tuned + 3, PlanAsks);
        // Nothing of it started the HLS.
        Assert.DoesNotContain(live.Sent, s => s.StartsWith("prefetch", StringComparison.Ordinal));
    }

    [Fact]
    public void The_details_follow_the_channel_and_end_with_the_player()
    {
        live.PlanAnswer = LiveDirectPlayRuleTests.Hevc();
        var p = Playback();
        p.Tune(1);
        engines[0].Start();
        p.SetDetailsOpen(true);
        // Another channel: the last one's plan says nothing of it.
        live.HoldPlan = true;
        p.Tune(2);
        Assert.Null(p.Plan.Value);
        live.HoldPlan = false;
        live.AnswerPlan(new LivePlan { Video = "copy", Audio = "copy", DirectPlay = true });
        Assert.Equal("copy", p.Plan.Value?.Video);
        live.Sent.Clear();
        scheduler.Advance(LivePlayback.DetailsMs);
        Assert.Contains("plan s2", live.Sent);
        Assert.DoesNotContain("plan s1", live.Sent);

        p.Close();
        live.Sent.Clear();
        scheduler.Advance(10 * LivePlayback.DetailsMs);
        Assert.Empty(live.Sent);
    }

    [Fact]
    public void The_live_player_gives_the_details_as_the_film_players_info_panel()
    {
        live.PlanAnswer = LiveDirectPlayRuleTests.Hevc();
        var model = new LivePlayerModel(live, Playback(), scheduler, 1, new LiveCategory.All().Key);
        engines[0].Start();
        var tuned = PlanAsks;
        model.SetDetailsOpen(true);
        Assert.Equal(tuned + 1, PlanAsks);
        var panel = model.Details();
        Assert.Equal(PlayerPanelKind.Info, panel.Kind);
        Assert.Equal("Playback details", panel.Title);
        Assert.Equal(PlayerRowKind.Heading, panel.Rows[0].Kind);
        Assert.Equal("How it plays", panel.Rows[0].Label);
        Assert.Equal(("Playback", "Direct play"), (panel.Rows[1].Label, panel.Rows[1].Value));
        Assert.All(panel.Rows.Skip(1), r => Assert.Equal(PlayerRowKind.Fact, r.Kind));
        model.SetDetailsOpen(false);
        scheduler.Advance(10 * LivePlayback.DetailsMs);
        Assert.Equal(tuned + 1, PlanAsks);
        model.Dispose();
    }
}

/// <summary>Playback details of a channel: the way, why, and what goes in and comes out.</summary>
public class LiveDetailsTests
{
    static readonly ClientCaps Uhd = ClientCaps.Of(TizenCaps.Of(new TizenFacts()));
    static readonly ClientCaps Hd = ClientCaps.Of(TizenCaps.Of(new TizenFacts { Uhd = false }));

    /// <summary>An interlaced MPEG-2 channel with MP2 sound, which the server deinterlaces into H.264.</summary>
    static readonly LivePlan Sd = new LivePlan
    {
        VideoCodec = "mpeg2video", Width = 720, Height = 576, FrameRate = 25, Interlaced = true, BitDepth = 8,
        AudioCodec = "mp2", AudioChannels = 2,
        Video = "h264_nvenc", Audio = "aac", OutHeight = 576, VideoKbps = 3_000,
        DirectPlay = true, Kbps = 4_500,
    };

    static (string, string)[] Rows(bool direct, LivePlan? plan, ClientCaps? caps = null) =>
        LiveDetails.Facts(direct, plan, caps ?? Uhd).Select(f => (f.Label, f.Value)).ToArray();

    [Fact]
    public void Direct_play_shows_what_the_provider_sends()
    {
        Assert.Equal(new[]
        {
            ("Playback", "Direct play"),
            ("Video", "HEVC 10-bit · 3840×2160 · 50 fps"),
            ("Audio", "EAC3 5.1"),
            ("Bitrate", "12 Mbps"),
        }, Rows(true, LiveDirectPlayRuleTests.Hevc()));
    }

    [Fact]
    public void A_transcode_shows_why_and_what_the_server_makes()
    {
        Assert.Equal(new[]
        {
            ("Playback", "Transcoding"),
            ("Why", "Interlaced video is made progressive"),
            ("Video", "MPEG-2 8-bit · 720×576 · 25 fps · interlaced → H.264 576p"),
            ("Audio", "MP2 Stereo → AAC Stereo"),
            ("Bitrate", "4.5 Mbps → 3.0 Mbps"),
            ("Encoder", "NVIDIA NVENC"),
        }, Rows(false, Sd));
        // A server that doesn't say a transcode's lines: the provider's.
        var rows = Rows(false, LiveDirectPlayRuleTests.Hevc() with { OutHeight = null, VideoKbps = null, DirectPlay = false });
        Assert.Contains(("Video", "HEVC 10-bit · 3840×2160 · 50 fps → H.264 2160p"), rows);
        Assert.Contains(("Bitrate", "12 Mbps"), rows);
        Assert.Contains(("Encoder", "NVIDIA NVENC"), rows);
    }

    [Fact]
    public void A_direct_stream_is_the_hls_of_a_picture_sent_as_it_is()
    {
        var dts = new LivePlan { VideoCodec = "h264", Width = 1920, Height = 1080, FrameRate = 50, AudioCodec = "dts", AudioChannels = 6, Video = "copy", Audio = "aac", DirectPlay = true, Kbps = 850 };
        Assert.Equal(new[]
        {
            ("Playback", "Direct stream"),
            ("Why", "This device doesn't play DTS sound"),
            ("Video", "H.264 · 1920×1080 · 50 fps"),
            ("Audio", "DTS 5.1 → AAC Stereo"),
            ("Bitrate", "850 kbps"),
        }, Rows(false, dts));
    }

    [Fact]
    public void Why_says_the_servers_setting_then_the_picture_then_the_sound()
    {
        var hevc = LiveDirectPlayRuleTests.Hevc();
        Assert.Equal("Direct Play is off on the server", LiveDetails.Why(hevc with { DirectPlay = false }, Uhd));
        Assert.Equal("Direct Play is off on the server", LiveDetails.Why(Sd with { DirectPlay = false }, Uhd));
        Assert.Equal("This device doesn't play HEVC", LiveDetails.Why(hevc, Hd));
        Assert.Equal("This device doesn't play DTS sound", LiveDetails.Why(hevc with { AudioCodec = "dts" }, Uhd));
        // Played as sent, or for a reason neither the plan nor the report gives: nothing.
        Assert.Null(LiveDetails.Why(hevc, Uhd));
        Assert.Null(LiveDetails.Why(hevc, null));
        Assert.Null(LiveDetails.Why(hevc with { VideoCodec = "av1" }, Uhd));
        Assert.DoesNotContain(Rows(false, hevc), r => r.Item1 == "Why");
        // Playing as the provider sends it needs no reason, whatever the plan would make.
        Assert.DoesNotContain(Rows(true, Sd), r => r.Item1 == "Why" || r.Item1 == "Encoder");
    }

    [Fact]
    public void A_line_whose_value_isnt_known_is_left_out()
    {
        Assert.Equal(new[] { ("Playback", "Direct play") }, Rows(true, new LivePlan { Video = "copy", Audio = "copy" }));
        // Radio: no picture.
        Assert.Equal(new[] { ("Playback", "Direct play"), ("Audio", "MP3 Stereo") }, Rows(true, new LivePlan { AudioCodec = "mp3", AudioChannels = 2, Video = "none", Audio = "copy" }));
        Assert.Equal(new[] { ("Playback", "Direct stream"), ("Video", "H.264"), ("Audio", "AAC 5ch") },
            Rows(false, new LivePlan { VideoCodec = "h264", AudioCodec = "aac", AudioChannels = 5, Video = "copy", Audio = "copy" }));
        // Without a plan: the way alone, which only a channel's stream.ts says by itself.
        Assert.Equal(new[] { ("Playback", "Direct play") }, Rows(true, null));
        Assert.Empty(Rows(false, null));
    }

    [Fact]
    public void The_panel_waits_for_the_server_or_lists_how_it_plays_then_this_tv()
    {
        var waiting = LiveDetails.Panel(false, null, Uhd, null);
        Assert.Equal(new[] { (PlayerRowKind.Note, "Asking the server…") }, waiting.Rows.Select(r => (r.Kind, r.Label)));

        var stats = new EngineStats { Width = 3840, Height = 2160, FrameRate = 50, Bandwidth = 40_000_000 };
        var sections = LiveDetails.Sections(true, LiveDirectPlayRuleTests.Hevc(), Uhd, stats);
        Assert.Equal(new[] { "How it plays", "This TV" }, sections.Select(s => s.Title));
        Assert.Equal(new MediaFact("TV network speed", "40 Mbps"), sections[0].Facts.Last());
        Assert.Equal(new[] { new MediaFact("Decoded", "3840×2160 · 50 fps") }, sections[1].Facts);
        // A player that says nothing of itself: how it plays alone.
        Assert.Equal(new[] { "How it plays" }, LiveDetails.Sections(true, LiveDirectPlayRuleTests.Hevc(), Uhd, new EngineStats()).Select(s => s.Title));
    }
}
