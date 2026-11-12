using System.Collections.Generic;
using System.Linq;
using System.Text.Json;
using Pley.Tv.Core.Api;
using Pley.Tv.Core.Player;
using Pley.Tv.Core.Ui;
using Xunit;

namespace Pley.Tv.Core.Tests;

/// <summary>Capability reports written by hand (the Android app's TestCaps).</summary>
static class TestCaps
{
    static readonly VideoDecoderCaps Hevc10 = new VideoDecoderCaps { Codec = "hevc", Hardware = true, Profiles = new[] { "Main", "Main 10" }, MaxWidth = 3840, MaxHeight = 2160, MaxFps = 60, MaxBitDepth = 10 };
    static readonly VideoDecoderCaps Avc = new VideoDecoderCaps { Codec = "h264", Hardware = true, Profiles = new[] { "High" }, MaxWidth = 3840, MaxHeight = 2160, MaxFps = 60, MaxBitDepth = 8 };

    /// <summary>
    /// A TV on an HDR10 screen and a receiver that takes AC-3 and E-AC-3: the HDR10 movies play as
    /// they are, with any of their audio tracks. <paramref name="dv"/>: Dolby Vision profiles decoded.
    /// </summary>
    public static ClientCaps HdrTv(int[]? dv = null, string[]? passthrough = null)
    {
        dv ??= new int[0];
        passthrough ??= new[] { "ac3", "eac3" };
        return new ClientCaps
        {
            Containers = new[] { "mp4", "m4v", "mov", "mkv", "matroska", "webm", "ts" },
            VideoCodecs = new[] { "h264", "hevc" },
            AudioCodecs = new[] { "aac", "mp3", "opus", "flac" }.Concat(passthrough).ToList(),
            Hdr = true,
            DolbyVision = dv.Length > 0,
            CapsVersion = 2,
            Video = new[] { Avc, Hevc10 },
            HdrTypes = new[] { "hdr10", "hlg" }.Concat(dv.Length > 0 ? new[] { "dolby_vision" } : new string[0]).ToList(),
            DolbyVisionProfiles = dv.Select(p => new DolbyVisionCaps { Profile = p, MaxLevel = 9 }).ToList(),
            AudioDecode = new[] { "aac", "mp3", "opus", "flac" },
            AudioPassthrough = passthrough,
        };
    }
}

/// <summary>The playback plan (the Android app's PlaybackPlanTest).</summary>
public class PlaybackPlanTests
{
    static readonly MediaStream Video = new MediaStream { Index = 0, Kind = "video", Codec = "hevc", Width = 3840, Height = 2160, BitDepth = 10, Profile = "Main 10", DynamicRange = "hdr10", Default = true };
    static readonly MediaStream Ac3 = new MediaStream { Index = 1, Kind = "audio", Codec = "ac3", Language = "eng", Default = true, Channels = 6 };
    static readonly MediaStream Aac = new MediaStream { Index = 2, Kind = "audio", Codec = "aac", Language = "fre", Channels = 2 };
    static readonly MediaStream Srt = new MediaStream { Index = 3, Kind = "subtitle", Codec = "subrip", Language = "eng", TextBased = true, Default = true };
    static readonly MediaStream Pgs = new MediaStream { Index = 4, Kind = "subtitle", Codec = "hdmv_pgs_subtitle", Language = "fre", TextBased = false };
    static readonly MediaFile File = new MediaFile { Id = 7, Path = "a.mkv", Container = "mkv", Bitrate = 40_000_000, Width = 3840, Height = 2160, Streams = new[] { Video, Ac3, Aac, Srt, Pgs } };
    static readonly ItemDetail Item = new ItemDetail { Id = 70, LibraryId = 1, Kind = "movie", Title = "A", Files = new[] { File } };
    static readonly ClientCaps Tv = TestCaps.HdrTv();
    static readonly Dictionary<string, object?> Report = new Dictionary<string, object?> { ["caps_version"] = 2 };

    static PlaybackPlan.Choices Choices(MediaStream? audio = null, MediaStream? subtitle = null, string mode = "auto", QualityPreset? quality = null, bool streaming = false, MediaFile? file = null, bool noAudio = false) =>
        new PlaybackPlan.Choices(file ?? File, 1_000, noAudio ? null : audio ?? Ac3, subtitle, quality ?? Qualities.Original, mode, streaming);

    static Dictionary<string, object?> Request(PlaybackPlan.Choices c, ClientCaps? caps = null) => PlayRequests.Body(70, c, "c", Report, caps ?? Tv);

    static int? Int(Dictionary<string, object?> body, string key) => body.TryGetValue(key, out var v) ? (int?)v : null;

    [Fact]
    public void The_file_asked_for_else_the_one_that_plays_best_here_else_the_first()
    {
        var sdr = File with { Id = 8, Streams = new[] { Video with { Codec = "h264", DynamicRange = "sdr", BitDepth = 8, Profile = "High", Width = 1920, Height = 1080 }, Aac } };
        var two = Item with { Files = new[] { File, sdr } };
        Assert.Equal(8, PlaybackPlan.File(two, 8, Tv)?.Id);
        Assert.Equal(7, PlaybackPlan.File(two, null, Tv)?.Id);
        Assert.Equal(7, PlaybackPlan.File(two, 99, null)?.Id);
        Assert.Null(PlaybackPlan.File(Item with { Files = new MediaFile[0] }, null, Tv));
    }

    [Fact]
    public void The_start_is_as_asked_from_the_start_or_the_resume_point_unless_watched()
    {
        var resumed = Item with { UserState = new UserState { PositionMs = 60_000 } };
        Assert.Equal(60_000, PlaybackPlan.StartMs(resumed, null, false));
        Assert.Equal(0, PlaybackPlan.StartMs(resumed, null, true));
        Assert.Equal(5_000, PlaybackPlan.StartMs(resumed, 5_000, false));
        Assert.Equal(0, PlaybackPlan.StartMs(resumed with { UserState = new UserState { PositionMs = 60_000, Watched = true } }, null, false));
    }

    [Fact]
    public void Tracks_start_as_asked_the_audio_else_as_the_server_picks_it_subtitles_as_the_profile_chose_and_off_is_off()
    {
        Assert.Equal(Aac, PlaybackPlan.InitialAudio(File, 2));
        // Nothing asked: automatic, whatever the details say.
        Assert.Null(PlaybackPlan.InitialAudio(File with { SelectedAudio = 2 }, null));
        Assert.Null(PlaybackPlan.InitialAudio(File, null));
        // Its answer names the track that plays: that one, else the file's default.
        Assert.Equal(Aac, PlaybackPlan.PlayedAudio(File, 2));
        Assert.Equal(Ac3, PlaybackPlan.PlayedAudio(File, null));
        Assert.Equal(Ac3, PlaybackPlan.PlayedAudio(File, 99));
        Assert.Equal(Pgs, PlaybackPlan.InitialSubtitle(File, 4));
        Assert.Equal(Srt, PlaybackPlan.InitialSubtitle(File with { SelectedSubtitle = 3 }, null));
        // The track the file flags as default doesn't show by itself.
        Assert.Null(PlaybackPlan.InitialSubtitle(File, null));
    }

    [Fact]
    public void Direct_play_names_the_audio_only_when_it_isnt_the_servers_own_a_stream_always_pictures_burn_only_into_streams()
    {
        var direct = Request(Choices(Ac3, Pgs));
        Assert.Null(Int(direct, "audio_index"));
        Assert.Null(Int(direct, "subtitle_index"));
        Assert.Equal(7L, direct["file_id"]);
        Assert.Equal(1_000L, direct["start_ms"]);
        Assert.Equal("c", direct["client_id"]);
        Assert.Same(Report, direct["caps"]);
        Assert.Equal(true, direct["keyed_urls"]);
        // Another track than the server would play: named, so it judges that one.
        Assert.Equal(2, Int(Request(Choices(Aac, Pgs)), "audio_index"));

        var stream = Request(Choices(Aac, Pgs, streaming: true));
        Assert.Equal(2, Int(stream, "audio_index"));
        Assert.Equal(4, Int(stream, "subtitle_index"));
        // Text subtitles are never burned in.
        Assert.Null(Int(Request(Choices(subtitle: Srt, streaming: true)), "subtitle_index"));
        // Pictures the player draws aren't either.
        Assert.Null(Int(Request(Choices(subtitle: Pgs, streaming: true) with { Sideload = true }), "subtitle_index"));
    }

    [Fact]
    public void Audio_the_tv_neither_decodes_nor_passes_through_goes_to_the_server()
    {
        var noAc3 = TestCaps.HdrTv(passthrough: new string[0]);
        Assert.False(PlaybackPlan.AudioPlaysHere(Ac3, noAc3));
        Assert.True(PlaybackPlan.AudioPlaysHere(Ac3, Tv));
        Assert.Equal(1, Int(Request(Choices(Ac3), noAc3), "audio_index"));
    }

    [Fact]
    public void Automatic_audio_is_named_to_no_server_even_for_a_stream()
    {
        var trueHd = new MediaStream { Index = 1, Kind = "audio", Codec = "truehd", Language = "eng", Default = true, Channels = 8 };
        var eac3 = new MediaStream { Index = 2, Kind = "audio", Codec = "eac3", Language = "eng", Channels = 6 };
        var remux = File with { Streams = new[] { Video, trueHd, eac3 } };
        Assert.Null(Int(Request(Choices(file: remux, noAudio: true)), "audio_index"));
        Assert.Null(Int(Request(Choices(file: remux, noAudio: true, streaming: true)), "audio_index"));
        // The E-AC-3 picked: named, as the server would judge the TrueHD.
        Assert.Equal(2, Int(Request(Choices(eac3, file: remux)), "audio_index"));
        // A receiver that takes TrueHD keeps it, unnamed.
        Assert.Null(Int(Request(Choices(trueHd, file: remux), TestCaps.HdrTv(passthrough: new[] { "ac3", "eac3", "truehd" })), "audio_index"));
    }

    [Fact]
    public void A_quality_caps_the_bitrate_and_height()
    {
        var asked = Request(Choices(quality: Qualities.ById("8m"), mode: "transcode", streaming: true));
        Assert.Equal(8_000, asked["max_bitrate"]);
        Assert.Equal(1_080, asked["max_height"]);
        Assert.Equal("transcode", asked["mode"]);
        Assert.False(Request(Choices()).ContainsKey("max_bitrate"));
        Assert.True(PlayRequests.Body(70, Choices(), "c", Report, Tv, keepPrevious: true).ContainsKey("keep_previous"));
    }

    [Fact]
    public void A_stream_is_certain_for_other_modes_videos_that_dont_play_here_containers_and_audio()
    {
        Assert.False(PlaybackPlan.Streams("auto", CapsDecision.Direct, File, Ac3, Tv));
        Assert.True(PlaybackPlan.Streams("remux", CapsDecision.Direct, File, Ac3, Tv));
        Assert.True(PlaybackPlan.Streams("auto", CapsDecision.ServerToneMaps, File, Ac3, Tv));
        Assert.True(PlaybackPlan.Streams("auto", CapsDecision.Direct, File with { Container = "avi" }, Ac3, Tv));
        Assert.True(PlaybackPlan.Streams("auto", CapsDecision.Direct, File, Ac3, TestCaps.HdrTv(passthrough: new string[0])));
    }

    [Fact]
    public void An_unexpected_stream_is_asked_again_with_the_indexes_it_needs()
    {
        var info = new PlaybackInfo { SessionId = "s", Method = "remux", Url = "/x", Hls = true, FileId = 7, AudioIndex = 1 };
        // The server's pick was thought to be the French track, so none was named.
        var stale = Choices(Aac, file: File with { SelectedAudio = 2 });
        var sent = Request(stale);
        Assert.Null(Int(sent, "audio_index"));
        Assert.True(PlaybackPlan.NeedsIndexes(info, sent, stale));
        Assert.False(PlaybackPlan.NeedsIndexes(info, sent, Choices(Ac3)));
        Assert.True(PlaybackPlan.NeedsIndexes(info, Request(Choices(subtitle: Pgs)), Choices(subtitle: Pgs)));
        Assert.False(PlaybackPlan.NeedsIndexes(info with { Hls = false, Method = "direct" }, sent, stale));
        var withIndex = Request(Choices(Aac, streaming: true));
        Assert.False(PlaybackPlan.NeedsIndexes(info, withIndex, Choices(Aac, streaming: true)));
        // Automatic audio is whatever the server chose: never asked again for it.
        var automatic = Choices(noAudio: true);
        Assert.False(PlaybackPlan.NeedsIndexes(info, Request(automatic), automatic));
    }

    [Fact]
    public void The_dolby_vision_guards()
    {
        var p5 = File with { Streams = new[] { Video with { DynamicRange = "dolby_vision", DvProfile = 5 }, Ac3 } };
        var p7 = File with { Streams = new[] { Video with { DynamicRange = "dolby_vision", DvProfile = 7, DvBlCompat = 6 }, Ac3 } };
        var p8 = File with { Streams = new[] { Video with { DynamicRange = "dolby_vision", DvProfile = 8, DvBlCompat = 1 }, Ac3 } };
        PlaybackInfo InfoOf(string method) => new PlaybackInfo { SessionId = "s", Method = method, Url = "/x", Hls = method != "direct", FileId = 7 };
        Assert.Equal("transcode", DvGuards.RetryMode(InfoOf("remux"), p5, Tv));
        // A remux that names its Dolby Vision keeps it: never tone-mapped instead.
        Assert.Null(DvGuards.RetryMode(InfoOf("remux") with { VideoRange = "Dolby Vision P5" }, p5, TestCaps.HdrTv(dv: new[] { 5 })));
        Assert.Equal("transcode", DvGuards.RetryMode(InfoOf("remux") with { VideoRange = "Dolby Vision P5 not decoded (wrong colours)" }, p5, Tv));
        Assert.Null(DvGuards.RetryMode(InfoOf("direct"), p5, TestCaps.HdrTv(dv: new[] { 5 })));
        Assert.Null(DvGuards.RetryMode(InfoOf("transcode"), p5, Tv));
        Assert.Equal("remux", DvGuards.RetryMode(InfoOf("direct"), p7, Tv));
        Assert.Null(DvGuards.RetryMode(InfoOf("direct"), p7, TestCaps.HdrTv(dv: new[] { 7 })));
        Assert.Null(DvGuards.RetryMode(InfoOf("remux"), p7, Tv));
        Assert.Null(DvGuards.RetryMode(InfoOf("direct"), p8, Tv));
        Assert.Null(DvGuards.RetryMode(InfoOf("remux"), File, Tv));
    }

    [Fact]
    public void Container_types_for_the_engine()
    {
        Assert.Equal("video/x-matroska", PlaybackPlan.MimeTypeOf("mkv"));
        Assert.Equal("video/mp4", PlaybackPlan.MimeTypeOf("MP4"));
        Assert.Null(PlaybackPlan.MimeTypeOf(null));
    }
}

/// <summary>What the server will do with a file for a report, and which version plays best (the Android app's DefaultPlaybackCapsPolicy).</summary>
public class CapsPolicyTests
{
    static MediaFile FileOf(long id, MediaStream video, long? bitrate = null) => new MediaFile { Id = id, Path = $"{id}.mkv", Container = "mkv", Bitrate = bitrate, Streams = new[] { video } };

    static readonly MediaStream Hdr10 = new MediaStream { Index = 0, Kind = "video", Codec = "hevc", Width = 3840, Height = 2160, BitDepth = 10, Profile = "Main 10", DynamicRange = "hdr10" };

    [Fact]
    public void Hdr_plays_where_the_screen_shows_it_and_is_tone_mapped_elsewhere()
    {
        Assert.Equal(CapsDecisionKind.Direct, CapsPolicy.Decide(FileOf(1, Hdr10), TestCaps.HdrTv()).Kind);
        Assert.Equal(CapsDecisionKind.ServerToneMaps, CapsPolicy.Decide(FileOf(1, Hdr10), TestCaps.HdrTv() with { HdrTypes = new string[0] }).Kind);
        Assert.Equal(CapsDecisionKind.ServerToneMaps, CapsPolicy.Decide(FileOf(1, Hdr10 with { DynamicRange = "hlg" }), TestCaps.HdrTv() with { HdrTypes = new[] { "hdr10" } }).Kind);
        // HDR10+ shows as HDR10 where HDR10+ doesn't.
        Assert.Equal(CapsDecisionKind.Direct, CapsPolicy.Decide(FileOf(1, Hdr10 with { DynamicRange = "hdr10plus" }), TestCaps.HdrTv()).Kind);
    }

    [Fact]
    public void Dolby_vision_goes_by_its_profile_and_base_layer()
    {
        var p7 = Hdr10 with { DynamicRange = "dolby_vision", DvProfile = 7, DvBlCompat = 6 };
        var p8 = Hdr10 with { DynamicRange = "dolby_vision", DvProfile = 8, DvBlCompat = 1 };
        var p5 = Hdr10 with { DynamicRange = "dolby_vision", DvProfile = 5, DvBlCompat = 0 };
        Assert.Equal(CapsDecisionKind.RemuxBaseLayer, CapsPolicy.Decide(FileOf(1, p7), TestCaps.HdrTv()).Kind);
        Assert.Equal(CapsDecisionKind.Direct, CapsPolicy.Decide(FileOf(1, p8), TestCaps.HdrTv()).Kind);
        Assert.Equal(CapsDecisionKind.ServerToneMaps, CapsPolicy.Decide(FileOf(1, p5), TestCaps.HdrTv()).Kind);
        Assert.Equal(CapsDecisionKind.Direct, CapsPolicy.Decide(FileOf(1, p5), TestCaps.HdrTv(dv: new[] { 5 })).Kind);
        // The Blu-ray's id in profile 8 is HDR10.
        Assert.Equal(CapsDecisionKind.Direct, CapsPolicy.Decide(FileOf(1, p8 with { DvBlCompat = 6 }), TestCaps.HdrTv()).Kind);
        Assert.Equal(CapsDecisionKind.Unsupported, CapsPolicy.Decide(FileOf(1, p5 with { DvLevel = 12 }), TestCaps.HdrTv(dv: new[] { 5 })).Kind);
    }

    [Fact]
    public void Decoders_refuse_what_is_beyond_them()
    {
        var tv = TestCaps.HdrTv();
        Assert.Equal("No decoder for av1 here", CapsPolicy.Decide(FileOf(1, Hdr10 with { Codec = "av1", DynamicRange = "sdr" }), tv).Reason);
        Assert.Equal(CapsDecisionKind.Unsupported, CapsPolicy.Decide(FileOf(1, Hdr10 with { Width = 7680, Height = 4320 }), tv).Kind);
        Assert.Equal(CapsDecisionKind.Unsupported, CapsPolicy.Decide(FileOf(1, Hdr10 with { FrameRate = 120 }), tv).Kind);
        // A profile the decoder doesn't list, known to both: refused; one only ffprobe names: let through.
        var h264 = new MediaStream { Index = 0, Kind = "video", Codec = "h264", Width = 1920, Height = 1080, Profile = "High 10", BitDepth = 8 };
        Assert.Equal(CapsDecisionKind.Unsupported, CapsPolicy.Decide(FileOf(1, h264), tv).Kind);
        Assert.Equal(CapsDecisionKind.Direct, CapsPolicy.Decide(FileOf(1, h264 with { Profile = "Progressive High" }), tv).Kind);
        // A portrait video fits turned.
        Assert.Equal(CapsDecisionKind.Direct, CapsPolicy.Decide(FileOf(1, h264 with { Profile = "High", Width = 1080, Height = 1920 }), tv).Kind);
        // The depth from the pixel format.
        Assert.Equal(10, CapsPolicy.BitDepth(new MediaStream { PixFmt = "yuv420p10le" }));
        Assert.Equal(10, CapsPolicy.BitDepth(new MediaStream { Profile = "Main 10" }));
    }

    [Fact]
    public void The_best_version_plays_as_it_is_fits_the_screen_and_is_the_richest()
    {
        var sdr1080 = FileOf(1, new MediaStream { Kind = "video", Codec = "h264", Width = 1920, Height = 1080, Profile = "High" }, 8_000_000);
        var hdr4k = FileOf(2, Hdr10, 40_000_000);
        var av1 = FileOf(3, Hdr10 with { Codec = "av1" }, 20_000_000);
        var item = new ItemDetail { Id = 1, Files = new[] { sdr1080, av1, hdr4k } };
        Assert.Equal(2, CapsPolicy.BestFile(item, TestCaps.HdrTv())!.Id);
        // On a 1080p screen, the 1080p one.
        Assert.Equal(1, CapsPolicy.BestFile(item, TestCaps.HdrTv() with { Display = new DisplayCaps { Width = 1920, Height = 1080 } })!.Id);
    }

    [Fact]
    public void The_tvs_own_report_reads_back()
    {
        var caps = ClientCaps.Of(TizenCaps.Of(new TizenFacts { Year = 2025, OutputMode = "DOLBY_DIGITAL_PLUS" }));
        Assert.Equal(2, caps.CapsVersion);
        Assert.Contains("hdr10", caps.HdrTypes);
        Assert.Contains("eac3", caps.AudioPassthrough);
        Assert.Contains(caps.Video, v => v.Codec == "hevc" && v.MaxLevel == 156);
        Assert.Equal(3840, caps.Display!.Width);
        // Picture subtitles are drawn from the server's .mks, as the web app's report says.
        Assert.True(caps.PictureSubtitlesSideload);
        Assert.Contains("hdmv_pgs_subtitle", caps.SubtitleCodecs);
        Assert.Contains("dvd_subtitle", caps.SubtitleCodecs);
        Assert.Contains("dvb_subtitle", caps.SubtitleCodecs);
        // Not claimed when not drawn: the server burns them in.
        var burned = ClientCaps.Of(TizenCaps.Of(new TizenFacts { PictureSubtitles = false }));
        Assert.False(burned.PictureSubtitlesSideload);
        Assert.DoesNotContain("hdmv_pgs_subtitle", burned.SubtitleCodecs);
    }
}

/// <summary>The qualities offered (the Android app's QualityTest).</summary>
public class QualityTests
{
    static IReadOnlyList<string> Ids(long? bitrate, int? height) =>
        Qualities.ForFile(new MediaFile { Id = 1, Path = "a", Bitrate = bitrate, Height = height }).Select(q => q.Id).ToList();

    [Fact]
    public void Finds_them_by_id_falling_back_to_the_original()
    {
        Assert.Equal(4_000, Qualities.ById("4m").MaxBitrateKbps);
        Assert.Equal(720, Qualities.ById("4m").MaxHeight);
        Assert.Same(Qualities.Original, Qualities.ById("nope"));
        Assert.Equal("original", Qualities.ById(null).Id);
        Assert.Same(Qualities.Automatic, Qualities.ById("auto"));
        // The settings say "Original" for the original.
        Assert.Same(Qualities.Original, Qualities.ByIdOrLabel("Original"));
        Assert.Same(Qualities.Automatic, Qualities.ByIdOrLabel("auto"));
    }

    [Fact]
    public void The_menus_order_is_the_same_on_every_app()
    {
        Assert.Equal(new[] { "original", "auto", "30m", "15m", "20m", "12m", "8m", "4m", "2m" }, Qualities.All.Select(q => q.Id));
        Assert.Equal(new int?[] { 2160, 2160, 1080, 1080, 1080, 720, 480 }, Qualities.Transcodes.Select(q => q.MaxHeight));
        Assert.Same(Qualities.ById("2m"), Qualities.Lowest);
    }

    [Fact]
    public void Automatic_is_a_choice_that_never_plays_as_itself_and_transcodes_are_the_rest()
    {
        Assert.True(Qualities.Automatic.Automatic);
        Assert.False(Qualities.Automatic.Original);
        Assert.Null(Qualities.Automatic.MaxBitrateKbps);
        Assert.Equal("auto", Qualities.ById("original").Mode);
        Assert.Equal("auto", Qualities.ById("auto").Mode);
        Assert.Equal("transcode", Qualities.ById("8m").Mode);
    }

    [Fact]
    public void Offers_only_the_qualities_with_fewer_lines_or_at_most_80_percent_of_the_bitrate()
    {
        // 1080p at 9 Mbps: 8 Mbps is barely lighter, and plays no better.
        Assert.Equal(new[] { "original", "auto", "4m", "2m" }, Ids(9_000_000, 1080));
        Assert.Equal(new[] { "original", "auto", "8m", "4m", "2m" }, Ids(10_000_000, 1080));
        Assert.Equal(new[] { "original", "auto", "2m" }, Ids(3_000_000, 720));
        Assert.Equal(new[] { "original", "auto" }, Ids(1_500_000, 480));
        // A 4K file at a low bitrate still gets smaller at 1080p.
        Assert.Equal(new[] { "original", "auto", "20m", "12m", "8m", "4m", "2m" }, Ids(6_000_000, 2160));
        Assert.Equal(new[] { "original", "auto", "30m", "15m", "20m", "12m", "8m", "4m", "2m" }, Ids(60_000_000, 2160));
        Assert.Equal(new[] { "original", "auto", "15m", "20m", "12m", "8m", "4m", "2m" }, Ids(20_000_000, 2160));
        Assert.Equal(new[] { "original", "auto", "8m", "4m", "2m" }, Ids(12_000_000, 720));
        Assert.Equal(Qualities.All.Count, Ids(null, null).Count);
        Assert.Equal(Qualities.All.Count, Qualities.ForFile(null).Count);
    }

    [Fact]
    public void A_server_that_converts_no_video_offers_the_original_alone()
    {
        var film = new MediaFile { Bitrate = 60_000_000, Width = 3840, Height = 2160 };
        Assert.Equal(new[] { Qualities.Original }, Qualities.Offered(film, originalOnly: true));
        Assert.Equal(new[] { Qualities.Original }, Qualities.Offered(null, originalOnly: true));
        Assert.Equal(Qualities.ForFile(film), Qualities.Offered(film, originalOnly: false));
    }

    [Fact]
    public void A_files_lines_are_the_servers_so_a_scope_film_is_1080p()
    {
        Assert.Equal(1080, Qualities.LinesClassOf(new MediaFile { Width = 1920, Height = 800 }));
        Assert.Equal(2160, Qualities.LinesClassOf(new MediaFile { Width = 3840, Height = 1600 }));
        Assert.Equal(720, Qualities.LinesClassOf(new MediaFile { Height = 720 }));
        Assert.Null(Qualities.LinesClassOf(new MediaFile()));
        var scope = new MediaFile { Bitrate = 30_000_000, Width = 1920, Height = 800 };
        Assert.Equal(new[] { "original", "auto", "20m", "15m", "12m", "8m", "4m", "2m" }, Qualities.ForFile(scope).Select(q => q.Id));
    }

    [Fact]
    public void Qualities_that_come_to_the_same_bitrate_and_lines_show_once()
    {
        Assert.Equal(1080, Qualities.LinesFor(Qualities.ById("15m"), 1080));
        Assert.Equal(720, Qualities.LinesFor(Qualities.ById("4m"), 1080));
        Assert.Equal(1080, Qualities.LinesFor(Qualities.Original, 1080));
        var offered = Qualities.ForFile(new MediaFile { Bitrate = 80_000_000, Height = 1080 });
        var keys = offered.Where(q => q.MaxBitrateKbps != null).Select(q => (q.MaxBitrateKbps, Qualities.LinesFor(q, 1080))).ToList();
        Assert.Equal(keys.Distinct(), keys);
    }

    [Fact]
    public void Nothing_chosen_is_automatic_a_remembered_quality_the_file_isnt_offered_plays_the_original()
    {
        var file = new MediaFile { Bitrate = 5_000_000, Height = 1080 };
        Assert.Equal("4m", Qualities.Choose(file, "4m").Id);
        Assert.Same(Qualities.Original, Qualities.Choose(file, "8m"));
        Assert.Same(Qualities.Automatic, Qualities.Choose(file, null));
        Assert.Same(Qualities.Automatic, Qualities.Choose(file, "auto"));
        Assert.Same(Qualities.Original, Qualities.Choose(file, "original"));
        Assert.Same(Qualities.Original, Qualities.Choose(file, "Original"));
    }

    [Fact]
    public void The_server_converts_the_original_at_its_sizes_closest()
    {
        Assert.Equal("20m", Qualities.Closest(new MediaFile { Bitrate = 40_000_000, Height = 1080 }).Id);
        Assert.Equal("8m", Qualities.Closest(new MediaFile { Bitrate = 6_000_000, Height = 1080 }).Id);
        Assert.Equal("15m", Qualities.Closest(new MediaFile { Bitrate = 12_000_000, Height = 2160 }).Id);
        Assert.Equal("8m", Qualities.Closest(new MediaFile { Bitrate = 1_000_000, Height = 1080 }).Id);
    }

    [Fact]
    public void The_texts_are_the_webs_words()
    {
        Assert.Equal("30 Mbps 4K", SessionText.Quality(Qualities.ById("30m")));
        Assert.Equal("Original", SessionText.Quality(Qualities.Original));
        Assert.Equal("Automatic", SessionText.Quality(Qualities.Automatic));
        // Taller than the file: named by its lines.
        Assert.Equal("8 Mbps 720p", SessionText.Quality(Qualities.ById("8m"), 720));
        Assert.Equal("8 Mbps 1080p", SessionText.Quality(Qualities.ById("8m"), 2160));
        Assert.Equal("15 Mbps 1080p", SessionText.Quality(Qualities.ById("15m"), 1080));
        Assert.Equal("Up next · S1 · E2", SessionText.UpNext("S1 · E2"));
        Assert.Equal("Up next", SessionText.UpNext(null));
        Assert.Equal("Playback failed (ERROR_CODE_DECODING_FAILED).", SessionText.Failure(Failure.Engine, "ERROR_CODE_DECODING_FAILED: boom"));
        Assert.Equal("Playback failed (?).", SessionText.Failure(Failure.Engine, null));
        Assert.Equal("Playback was stopped by the server's administrator.\nBedtime", SessionText.Stopped(" Bedtime ", byAdmin: true));
        Assert.Equal("Playback was stopped from another device.", SessionText.Stopped(null, byAdmin: false));
        Assert.Equal("No answer in time. Check that the server is on and that this device is on its network.", SessionText.Unreachable("no answer in time"));
        Assert.Equal("1080p · 8.2 Mbps", Qualities.FileHint(new MediaFile { Width = 1920, Height = 1080, Bitrate = 8_200_000 }));
    }
}
