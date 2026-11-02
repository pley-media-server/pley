using System;
using System.Collections.Generic;
using System.Linq;
using System.Text.Json;
using System.Threading.Tasks;
using Pley.Tv.Core.Api;
using Pley.Tv.Core.Player;
using Xunit;

namespace Pley.Tv.Core.Tests;

/// <summary>WebVTT, the cue index, sync and offsets (the Android app's SubtitlesTest and SubtitleSyncTest, the web TV app's vtt.test.ts).</summary>
public class WebVttTests
{
    const string Vtt = "WEBVTT\n\n1\n00:00:01.000 --> 00:00:03.500\nHello <i>there</i>\n\n2\n00:00:02.000 --> 00:00:04.000 line:0 position:20% align:start size:50%\nSign & <b>top</b>\n\n00:01:00,250 --> 00:01:02,000\nComma times\nsecond line\n";

    const string Android = "WEBVTT\n\n00:00:01.000 --> 00:00:04.000\nThe fixture server plays this.\n\n00:00:03.000 --> 00:00:05.500 line:0\n<i>A sign</i> at the top\n\n00:01:06.000 --> 00:01:09.000\nTwo\nlines\n";

    static IEnumerable<string> Texts(IEnumerable<Cue> cues) => cues.Select(c => c.Text);

    [Fact]
    public void Reads_times_text_and_settings()
    {
        var cues = WebVtt.Parse(Vtt);
        Assert.Equal(new[] { (1000L, 3500L), (2000L, 4000L), (60250L, 62000L) }, cues.Select(c => (c.StartMs, c.EndMs)));
        Assert.Equal(new[] { new CueSpan("Hello "), new CueSpan("there", Italic: true) }, cues[0].Lines[0]);
        Assert.Equal((0.0, true, 20.0, "start", 50.0), (cues[1].Line!.Value, cues[1].SnapToLines, cues[1].Position!.Value, cues[1].Align, cues[1].Size!.Value));
        Assert.Equal("Sign & top", cues[1].Text);
        Assert.Equal("Comma times\nsecond line", cues[2].Text);
        Assert.Null(cues[0].Line);
    }

    [Fact]
    public void Keeps_only_safe_markup()
    {
        Assert.Equal("alert(1)Hi x <b>", string.Concat(WebVtt.Spans("<script>alert(1)</script><v Joe>Hi</v> <c.yellow>x</c> &lt;b&gt;")[0].Select(s => s.Text)));
        var open = WebVtt.Spans("<i>open <b>both")[0];
        Assert.Equal(new[] { new CueSpan("open ", Italic: true), new CueSpan("both", Italic: true, Bold: true) }, open);
        Assert.Equal("♪ &", string.Concat(WebVtt.Spans("&#9834; &amp;")[0].Select(s => s.Text)));
        // Markup for the TV's labels: bold and italic as font tags, the text escaped.
        var cue = new Cue(0, 1, WebVtt.Spans("<i>a</i> <b>b</b> 1 < 2 & 3\nnext"));
        Assert.Equal("<font slant='italic'>a</font> <font weight='bold'>b</font> 1 &lt; 2 &amp; 3\nnext", cue.Markup());
    }

    [Fact]
    public void Parses_times_with_and_without_hours_and_skips_what_isnt_a_cue()
    {
        Assert.Equal(3723456, WebVtt.TimeMs("01:02:03.456"));
        Assert.Equal(123500, WebVtt.TimeMs("02:03,5"));
        Assert.Empty(WebVtt.Parse("WEBVTT\n\nNOTE a note\n\n00:00:05.000 --> 00:00:04.000\nbackwards\n\n00:00:01.000 --> 00:00:02.000\n\n"));
        Assert.Single(WebVtt.Parse("﻿WEBVTT\r\n\r\n00:00:01.000 --> 00:00:02.000\r\nCRLF\r\n"));
    }

    [Fact]
    public void Reads_times_text_styling_and_lines_as_android_does()
    {
        var index = new SubtitleIndex(WebVtt.Parse(Android));
        Assert.Equal(69_000, index.EndMs);
        Assert.Equal(new[] { "The fixture server plays this." }, Texts(index.At(1_000)));
        Assert.Equal(new[] { "A sign at the top", "The fixture server plays this." }, Texts(index.At(3_500)).OrderBy(t => t, StringComparer.Ordinal));
        Assert.Equal(new[] { "A sign at the top" }, Texts(index.At(4_000)));
        Assert.Equal(new[] { "Two\nlines" }, Texts(index.At(67_000)));
        Assert.Empty(index.At(10_000));
        // Placed at the top by its line setting.
        Assert.Equal(0, index.At(4_500).Single().Line);
    }

    [Fact]
    public void The_next_change_is_the_next_start_or_end()
    {
        var index = new SubtitleIndex(WebVtt.Parse(Android));
        Assert.Equal(1_000, index.NextChangeAfter(0));
        Assert.Equal(3_000, index.NextChangeAfter(1_000));
        Assert.Equal(4_000, index.NextChangeAfter(3_000));
        Assert.Equal(66_000, index.NextChangeAfter(5_500));
        Assert.Null(index.NextChangeAfter(69_000));
    }

    [Fact]
    public void Merging_keeps_the_answer_that_reaches_further()
    {
        var first = new SubtitleIndex(WebVtt.Parse(Android.Substring(0, Android.IndexOf("00:01:06", StringComparison.Ordinal))));
        var whole = new SubtitleIndex(WebVtt.Parse(Android));
        var merged = first.Merge(whole);
        Assert.Same(whole, merged);
        Assert.Same(merged, merged.Merge(first));
        Assert.Equal(new[] { "Two\nlines" }, Texts(merged.At(67_000)));
    }
}

public class SubtitleSyncTests
{
    static readonly SubtitleIndex Index = new SubtitleIndex(new[]
    {
        new Cue(1_000, 4_000, WebVtt.Spans("A")),
        new Cue(6_000, 8_000, WebVtt.Spans("B")),
    });

    static IEnumerable<string> Showing(long positionMs, long offsetMs) => Index.At(SubtitleSync.TrackTime(positionMs, offsetMs)).Select(c => c.Text);

    [Fact]
    public void A_positive_offset_shows_the_subtitles_later()
    {
        Assert.Empty(Showing(1_400, 500));
        Assert.Equal(new[] { "A" }, Showing(1_500, 500));
        Assert.Equal(new[] { "A" }, Showing(4_499, 500));
        Assert.Empty(Showing(4_500, 500));
        Assert.Equal(new[] { "B" }, Showing(6_500, 500));
        // The next change is counted from the moved time.
        Assert.Equal(500, SubtitleSync.UntilChange(Index, 1_000, 500));
    }

    [Fact]
    public void A_negative_offset_shows_them_earlier_even_before_the_playhead_reached_them()
    {
        Assert.Equal(new[] { "A" }, Showing(0, -1_000));
        Assert.Equal(new[] { "A" }, Showing(2_999, -1_000));
        Assert.Empty(Showing(3_000, -1_000));
        Assert.Equal(new[] { "B" }, Showing(5_000, -1_000));
        Assert.Equal(1_000, SubtitleSync.UntilChange(Index, 2_000, -1_000));
        Assert.Equal(new[] { "A" }, Showing(1_000, 0));
        Assert.Empty(Showing(999, 0));
    }

    [Fact]
    public void Offsets_go_by_0_1_s_within_10_s_either_way_and_read_as_on_the_web()
    {
        Assert.Equal(10_000, SubtitleSync.Clamp(12_345));
        Assert.Equal(-10_000, SubtitleSync.Clamp(-99_999));
        Assert.Equal(0, SubtitleSync.Clamp(-40));
        Assert.Equal(100, SubtitleSync.Clamp(149));
        Assert.Equal(200, SubtitleSync.Clamp(150));
        // Halves round up, as Kotlin's and JavaScript's round do.
        Assert.Equal(-100, SubtitleSync.Clamp(-150));
        Assert.Equal("+0.5 s", SubtitleSync.Label(500));
        Assert.Equal("−1.2 s", SubtitleSync.Label(-1_200));
        Assert.Equal("0.0 s", SubtitleSync.Label(0));
        Assert.Equal("+10.0 s", SubtitleSync.Label(10_000));
    }

    [Fact]
    public void Offsets_are_remembered_per_file_and_profile_and_0_forgets()
    {
        var kv = new MemoryKeyValue();
        long? profile = 1;
        var store = new SubtitleOffsets(kv, () => profile);
        store.Set("http://nas:7539", 7, 1_500);
        store.Set("http://nas:7539", 8, -300);
        Assert.Equal(1_500, store.Get("http://nas:7539", 7));
        Assert.Equal(0, store.Get("http://other:7539", 7));
        profile = 2;
        Assert.Equal(0, store.Get("http://nas:7539", 7));
        store.Set("http://nas:7539", 7, -500);
        Assert.Equal(-500, store.Get("http://nas:7539", 7));
        profile = 1;
        Assert.Equal(1_500, store.Get("http://nas:7539", 7));
        store.Set("http://nas:7539", 7, 0);
        Assert.Equal(0, store.Get("http://nas:7539", 7));
        Assert.Equal(-300, store.Get("http://nas:7539", 8));
        // Without a profile nothing is read or kept.
        profile = null;
        Assert.Equal(0, store.Get("http://nas:7539", 8));
        store.Set("http://nas:7539", 9, 100);
        Assert.Equal(2, JsonDocument.Parse(kv.Values["pley.subtitleOffsets"]).RootElement.GetArrayLength());
        // Something unreadable kept: none.
        kv.Values["pley.subtitleOffsets"] = "{";
        profile = 1;
        Assert.Equal(0, store.Get("http://nas:7539", 8));
    }
}

/// <summary>Partial tracks asked for again and merged, and what shows when (the Android app's ServerSubtitles test).</summary>
public class SubtitlePresenterTests
{
    const string Vtt = "WEBVTT\n\n00:00:01.000 --> 00:00:04.000\nOne\n\n00:00:03.000 --> 00:00:05.500\nTwo\n\n00:01:06.000 --> 00:01:09.000\nThree\n";

    readonly FakeScheduler clock = new FakeScheduler();
    readonly List<string> asked = new List<string>();
    readonly Queue<Func<SubtitleText>> answers = new Queue<Func<SubtitleText>>();
    long position;
    long offset;
    int failures;

    SubtitlePresenter Make() => new SubtitlePresenter(clock, url =>
    {
        asked.Add(url);
        if (answers.Count == 0) return Task.FromException<SubtitleText>(new InvalidOperationException("gone"));
        return Task.FromResult(answers.Dequeue()());
    }, () => position, () => offset, () => 1, () => failures++);

    [Fact]
    public void A_partial_track_is_asked_for_every_3_s_until_whole_and_a_failure_is_said_once()
    {
        answers.Enqueue(() => new SubtitleText(Vtt.Substring(0, Vtt.IndexOf("00:00:03", StringComparison.Ordinal)), Complete: false));
        answers.Enqueue(() => new SubtitleText(Vtt.Substring(0, Vtt.IndexOf("00:01:06", StringComparison.Ordinal)), Complete: false));
        answers.Enqueue(() => new SubtitleText(Vtt, Complete: true));
        var p = Make();
        position = 3_500;
        p.Show("http://pley/api/files/7/subtitles/2.vtt");
        Assert.Equal(new[] { "One" }, p.Shown.Value.Select(c => c.Text));
        clock.Advance(3_000);
        Assert.Equal(new[] { "One", "Two" }, p.Shown.Value.Select(c => c.Text));
        clock.Advance(3_000);
        clock.Advance(30_000);
        Assert.Equal(3, asked.Count);

        // The same track again changes nothing; another one starts over, and its failure is said.
        p.Show("http://pley/api/files/7/subtitles/2.vtt");
        Assert.Equal(3, asked.Count);
        p.Show("http://pley/api/files/7/subtitles/1000.vtt");
        Assert.Empty(p.Shown.Value);
        Assert.Equal(1, failures);
        Assert.Null(p.Showing);
    }

    [Fact]
    public void What_shows_follows_the_playhead_and_the_offset()
    {
        answers.Enqueue(() => new SubtitleText(Vtt, Complete: true));
        var p = Make();
        position = 500;
        p.Show("http://pley/x.vtt");
        Assert.Empty(p.Shown.Value);
        // The next look is when the first cue starts, 500 ms on, but at most 250 ms away.
        Assert.Equal(250, clock.Pending.Single());
        position = 1_000;
        clock.Advance(250);
        Assert.Equal(new[] { "One" }, p.Shown.Value.Select(c => c.Text));
        // Moved 2 s later: none yet at 1 s.
        offset = 2_000;
        p.Refresh();
        Assert.Empty(p.Shown.Value);
        p.Hide();
        Assert.Null(p.Showing);
        Assert.Empty(clock.Pending);
    }

    static readonly byte[] Shown = PictureKit.PgsDisplaySet(0x80, true, rle: new[] { 1, 1, 1, 1, 0, 0, 1, 1, 1, 1, 0, 0 });
    static readonly byte[] Cleared = PictureKit.PgsDisplaySet(0x00, false);

    SubtitlePresenter WithPictures(Queue<Func<SubtitlePictures>> mks, List<string> unavailable) => new SubtitlePresenter(clock, url =>
    {
        asked.Add(url);
        return Task.FromResult(answers.Dequeue()());
    }, () => position, () => offset, () => 1, () => failures++, url =>
    {
        asked.Add(url);
        return Task.FromResult(mks.Dequeue()());
    }, () => unavailable.Add("unavailable"));

    [Fact]
    public void A_partial_picture_track_is_asked_for_every_3_s_until_whole()
    {
        var first = PictureKit.Mks("S_HDMV/PGS", new[] { new PictureKit.MksBlock(1000, Shown) });
        var whole = PictureKit.Mks("S_HDMV/PGS", new[] { new PictureKit.MksBlock(1000, Shown), new PictureKit.MksBlock(3000, Cleared), new PictureKit.MksBlock(5000, Shown) });
        var mks = new Queue<Func<SubtitlePictures>>();
        mks.Enqueue(() => new SubtitlePictures(first, Complete: false));
        mks.Enqueue(() => new SubtitlePictures(whole, Complete: true));
        var unavailable = new List<string>();
        var p = WithPictures(mks, unavailable);
        position = 4_000;
        p.ShowPictures("http://pley/api/files/7/subtitles/4.mks");
        Assert.True(p.ShowingPictures);
        // Only the first picture so far: it shows until another comes.
        Assert.Equal(100, p.Picture.Value!.Bitmaps.Single().X);
        Assert.Empty(p.Shown.Value);
        clock.Advance(3_000);
        // The whole track: cleared at 3 s, shown again at 5 s.
        Assert.Null(p.Picture.Value);
        position = 5_000;
        p.Refresh();
        Assert.NotNull(p.Picture.Value);
        clock.Advance(30_000);
        Assert.Equal(2, asked.Count);
        // Asked for from where the player was as it was picked: the server copies the track out of its file from
        // there first.
        Assert.All(asked, url => Assert.Equal("http://pley/api/files/7/subtitles/4.mks?at=4000", url));

        // The same track again changes nothing; text in its place clears the picture.
        p.ShowPictures("http://pley/api/files/7/subtitles/4.mks");
        Assert.Equal(2, asked.Count);
        answers.Enqueue(() => new SubtitleText(Vtt, Complete: true));
        p.Show("http://pley/api/files/7/subtitles/2.vtt");
        Assert.Null(p.Picture.Value);
        Assert.False(p.ShowingPictures);
        Assert.Empty(unavailable);
        Assert.Equal(0, failures);
    }

    [Fact]
    public void A_partial_picture_track_is_asked_for_less_often_each_time_up_to_a_minute()
    {
        // Deviation from Android, which asks every 3 s: each answer is the whole .mks so far, tens of MB.
        var part = PictureKit.Mks("S_HDMV/PGS", new[] { new PictureKit.MksBlock(1000, Shown) });
        var mks = new Queue<Func<SubtitlePictures>>();
        for (var i = 0; i < 8; i++) mks.Enqueue(() => new SubtitlePictures(part, Complete: false));
        var p = WithPictures(mks, new List<string>());
        p.ShowPictures("http://pley/api/files/7/subtitles/4.mks");
        var times = new List<long>();
        for (var t = 0; t < 200_000; t += 1_000)
        {
            clock.Advance(1_000);
            if (asked.Count > times.Count + 1) times.Add(t + 1_000);
        }
        // 3, 6, 12, 24, 48 s apart, then a minute.
        Assert.Equal(new long[] { 3_000, 9_000, 21_000, 45_000, 93_000, 153_000 }, times.Take(6));
    }

    [Fact]
    public void Pictures_the_server_cant_make_or_this_app_cant_decode_are_unavailable()
    {
        var mks = new Queue<Func<SubtitlePictures>>();
        mks.Enqueue(() => throw new ApiException(503, "transcoder_unavailable", "no transcoder"));
        mks.Enqueue(() => new SubtitlePictures(PictureKit.Mks("S_TEXT/UTF8", new[] { new PictureKit.MksBlock(1000, Shown) }), Complete: true));
        mks.Enqueue(() => throw new ApiException(500, "internal", "broken"));
        var unavailable = new List<string>();
        var p = WithPictures(mks, unavailable);
        p.ShowPictures("http://pley/a.mks");
        Assert.Single(unavailable);
        Assert.Null(p.Showing);
        p.ShowPictures("http://pley/b.mks");
        Assert.Equal(2, unavailable.Count);
        // Anything else is a failure to load, as for text.
        p.ShowPictures("http://pley/c.mks");
        Assert.Equal(2, unavailable.Count);
        Assert.Equal(1, failures);
    }
}

/// <summary>How it plays, in words (the Android app's PiecesTest's details and TvReadoutTest).</summary>
public class PlaybackDetailsTests
{
    static readonly MediaStream Dv = new MediaStream { Index = 0, Kind = "video", Codec = "hevc", Width = 3840, Height = 2160, DynamicRange = "dolby_vision", DvProfile = 8, DvBlCompat = 1 };

    [Fact]
    public void The_details_read_as_the_webs()
    {
        Assert.Equal("HEVC 4K Dolby Vision P8.1", Details.VideoLabel(Dv));
        Assert.Equal("HEVC 4K Dolby Vision P5", Details.VideoLabel(Dv with { DvProfile = 5, DvBlCompat = 0 }));
        Assert.Equal("TrueHD 7.1", Details.AudioLabel(new MediaStream { Index = 1, Kind = "audio", Codec = "truehd", Channels = 8 }));
        Assert.Equal("H.264 1080p", Details.Simplified("H.264 1920×1080"));
        Assert.Equal("NVIDIA NVENC (GPU)", Details.EncoderLabel(new TranscodeInfo { JobId = "j", Encoder = "h264_nvenc", Pipeline = "gpu" }));

        var audio = new MediaStream { Index = 1, Kind = "audio", Codec = "eac3", Channels = 6, Default = true };
        var file = new MediaFile { Id = 1, Path = "a.mkv", Bitrate = 62_000_000, Streams = new[] { Dv, audio } };
        var info = new PlaybackInfo { SessionId = "s", Method = "remux", Url = "/x", Hls = true, FileId = 1, AudioConverted = true, Reasons = new[] { "EAC3 audio isn't supported by this player." } };
        var remux = Details.Build(info, file, null, null, 10_000,
            new TranscodeInfo { JobId = "j", Video = "copy", Audio = "transcode", AudioIn = "EAC3 5.1", AudioOut = "AAC 2.0", BitrateOut = 40_000_000, Speed = 12.0, TranscodedMs = 70_000 });
        Assert.Equal("Direct stream · Audio transcoding", remux.Method);
        Assert.Equal("EAC3 audio isn't supported by this player.", remux.Why);
        Assert.Null(remux.Pipeline);
        Assert.Equal(new[]
        {
            ("Video", "HEVC 4K Dolby Vision P8.1"),
            ("Audio", "EAC3 5.1 → AAC 2.0"),
            ("Bitrate", "62 Mbps → 40 Mbps"),
            ("Speed", "12.0× real time · 60 s ready"),
        }, remux.Rows.Select(r => (r.Label, r.Value)));
        var direct = Details.Build(info with { Method = "direct", Hls = false }, file, null, null, 0, null);
        Assert.Equal("Direct play", direct.Method);
        // The file as it is needs no reason; a fallback's still shows.
        Assert.Null(direct.Why);
        Assert.Equal(SessionText.FallbackUnplayable, Details.Build(info with { Method = "direct", Hls = false }, file, null, SessionText.FallbackUnplayable, 0, null).Why);
        Assert.Equal(new[] { "Video", "Audio", "Bitrate" }, direct.Rows.Select(r => r.Label));
        var fallback = Details.Build(info with { Method = "transcode", AudioConverted = false }, file, null, SessionText.FallbackNotSmooth, 0, null);
        Assert.Equal(SessionText.FallbackNotSmooth, fallback.Why);
        Assert.Equal("HEVC 4K Dolby Vision P8.1 → …", fallback.Rows.First().Value);
    }

    [Fact]
    public void The_details_name_dolby_vision_as_the_server_does_and_say_what_the_screen_gets()
    {
        var p7 = Dv with { DvProfile = 7, DvBlCompat = 6 };
        Assert.Equal("HEVC 4K Dolby Vision P7", Details.VideoLabel(p7));
        Assert.Equal("HEVC 4K Dolby Vision P7 FEL", Details.VideoLabel(p7 with { DvEl = "fel" }));
        Assert.Equal("HEVC 4K Dolby Vision P8.1 + HDR10+", Details.VideoLabel(p7 with { DvProfile = 8, DvBlCompat = 1, Hdr10plus = true }));
        var file = new MediaFile { Id = 1, Path = "a.mkv", Streams = new[] { p7 } };
        var info = new PlaybackInfo { SessionId = "s", Method = "direct", Url = "/x", FileId = 1, VideoRange = "HDR10 (Dolby Vision base layer)" };
        Assert.Equal("HDR10 (Dolby Vision base layer)", Details.Build(info, file, null, null, 0, null).Rows.First(r => r.Label == "Shown as").Value);
        var transcode = info with { Method = "transcode", Hls = true, VideoRange = "SDR (tone-mapped)" };
        var converted = Details.Build(transcode, file, null, null, 0, new TranscodeInfo { JobId = "j", Video = "transcode", VideoOut = "H.264 1920×1080", Tonemap = "libplacebo" });
        Assert.Equal("SDR (tone-mapped with libplacebo)", converted.Rows.First(r => r.Label == "Shown as").Value);
        Assert.Equal("HEVC 4K Dolby Vision P7 → H.264 1080p SDR", converted.Rows.First().Value);
        var sdr = Details.Build(info with { VideoRange = null }, file with { Streams = new[] { p7 with { DynamicRange = "sdr" } } }, null, null, 0, null);
        Assert.DoesNotContain(sdr.Rows, r => r.Label == "Shown as");
    }

    [Fact]
    public void The_details_carry_the_streams_in_play_and_how_subtitles_reach_the_screen()
    {
        var video = new MediaStream { Index = 0, Kind = "video", Codec = "hevc", Width = 3840, Height = 2160 };
        var audio = new MediaStream { Index = 1, Kind = "audio", Codec = "eac3", Channels = 6 };
        var french = new MediaStream { Index = 2, Kind = "audio", Codec = "ac3", Channels = 6, Language = "fre" };
        var srt = new MediaStream { Index = 3, Kind = "subtitle", Codec = "subrip", TextBased = true };
        var pgs = new MediaStream { Index = 4, Kind = "subtitle", Codec = "hdmv_pgs_subtitle", TextBased = false };
        var file = new MediaFile { Id = 1, Path = "a.mkv", Streams = new[] { video, audio, french, srt, pgs } };
        var direct = new PlaybackInfo { SessionId = "s", Method = "direct", Url = "/x", FileId = 1 };
        var details = Details.Build(direct, file, 2, null, 0, null, srt);
        Assert.Equal((video, french, srt), (details.Video, details.Audio, details.Subtitle));
        // This app draws text from the server's WebVTT, in direct play too (unlike Android, whose engine reads the file's).
        Assert.Equal(SubtitleDelivery.Server, details.SubtitleDelivery);
        var hls = direct with { Method = "transcode", Hls = true };
        Assert.Equal(SubtitleDelivery.BurnedIn, Details.DeliveryOf(hls with { BurnSubtitles = true, SubtitleIndex = 4 }, pgs, sideload: false));
        Assert.Equal(SubtitleDelivery.ServerPictures, Details.DeliveryOf(hls, pgs, sideload: true));
        Assert.Null(Details.Build(direct, file, null, null, 0, null).SubtitleDelivery);
    }
}

public class ReadoutTests
{
    static readonly MediaStream Video = new MediaStream
    {
        Index = 0, Kind = "video", Codec = "hevc", Profile = "Main 10", Level = 153, Width = 3840, Height = 2160, FrameRate = 23.976, BitDepth = 10, PixFmt = "yuv420p10le",
        DynamicRange = "dolby_vision", DvProfile = 8, DvBlCompat = 1, DvLevel = 6, ColorPrimaries = "bt2020", ColorTransfer = "smpte2084", ColorSpace = "bt2020nc",
    };
    static readonly MediaStream Audio = new MediaStream { Index = 1, Kind = "audio", Codec = "eac3", Channels = 6, ChannelLayout = "5.1(side)", SampleRate = 48_000, Language = "eng", Default = true };
    static readonly MediaStream Srt = new MediaStream { Index = 2, Kind = "subtitle", Codec = "subrip", TextBased = true, Forced = true, Language = "eng" };
    static readonly MediaFile File = new MediaFile { Id = 1, Path = "/m/Heat (1995).mkv", Container = "mkv", Bitrate = 62_000_000, Streams = new[] { Video, Audio, Srt } };
    static readonly EngineStats Stats = new EngineStats { Decoder = "omx.hevc", Hardware = true, Width = 3840, Height = 2160, FrameRate = 23.976, Codec = "video/hevc hvc1.2.4.L153.B0", AudioOutput = "Passed through as E-AC-3", Dropped = 10, Frames = 1_000, Bandwidth = 120_000_000 };

    static PlaybackDetails DetailsOf(string method = "direct", TranscodeInfo? t = null) =>
        Details.Build(new PlaybackInfo { SessionId = "s", Method = method, Url = "/x", Hls = method != "direct", FileId = 1 }, File, null, null, 0, t, Srt);

    static Dictionary<string, Dictionary<string, string>> Facts(ReadoutInput input) =>
        Readout.Sections(input).ToDictionary(s => s.Title, s => s.Facts.GroupBy(f => f.Label).ToDictionary(g => g.Key, g => g.First().Value));

    [Fact]
    public void The_panel_shows_the_file_each_stream_in_full_the_decision_and_this_tv()
    {
        var s = Facts(new ReadoutInput(DetailsOf(), Stats, BufferedAheadMs: 45_000, Quality: "Original", Stalls: 2, FirstFrameMs: 830));
        Assert.Equal("Direct play", s["How it plays"]["Playback"]);
        Assert.False(s["How it plays"].ContainsKey("Why"));
        Assert.Equal("Original", s["How it plays"]["Quality"]);
        Assert.Equal("MKV", s["File"]["Container"]);
        Assert.Equal("62 Mbps", s["File"]["Overall bitrate"]);
        Assert.Equal("HEVC", s["Video"]["Codec"]);
        Assert.Equal("Dolby Vision P8.1", s["Video"]["HDR"]);
        Assert.Equal("As in the file", s["Video"]["Current bitrate"]);
        Assert.Equal("5.1 (5.1(side))", s["Audio"]["Channels"]);
        Assert.Equal("Passed through as E-AC-3", s["Audio"]["Output"]);
        Assert.Equal("SRT, text", s["Subtitles"]["Format"]);
        Assert.Equal("Text from the server", s["Subtitles"]["Shown"]);
        var tv = s["This TV"];
        Assert.Equal("omx.hevc (hardware)", tv["Video decoder"]);
        Assert.Equal("3840×2160 · 23.976 fps · HEVC", tv["Decoded"]);
        Assert.Equal("hvc1.2.4.L153.B0", tv["Codec string"]);
        Assert.Equal("10 of 1000", tv["Dropped frames"]);
        Assert.Equal("45 s", tv["Buffered ahead"]);
        Assert.Equal("120 Mbps", tv["Network"]);
        Assert.Equal("2", tv["Stalls"]);
        Assert.Equal("830 ms", tv["First frame"]);
    }

    [Fact]
    public void A_transcode_says_what_it_makes_and_the_bitrate_it_aims_for()
    {
        var t = new TranscodeInfo { JobId = "j", Video = "transcode", Audio = "transcode", VideoOut = "H.264 1920×1080", AudioOut = "AAC 2.0", BitrateOut = 8_000_000 };
        var d = DetailsOf("transcode", t);
        Assert.Equal("H.264 1080p · AAC 2.0", Readout.TranscodeTarget(d));
        var lines = Readout.Compact(new ReadoutInput(d, Stats with { Bitrate = null })).ToDictionary(f => f.Label, f => f.Value);
        Assert.Equal("H.264 1080p · AAC 2.0", lines["Converts to"]);
        Assert.Equal("62 Mbps → 8.0 Mbps", lines["Bitrate"]);
        Assert.StartsWith("HEVC Main 10 L5.1 · 3840×2160 · 23.976 fps · 10-bit · 4:2:0 · Dolby Vision P8.1", lines["Video"]);
        Assert.Equal("10 of 1000", lines["Dropped"]);
    }

    [Fact]
    public void Nothing_but_the_waiting_note_before_the_server_answers()
    {
        Assert.Empty(Readout.Sections(new ReadoutInput()));
        Assert.Empty(Readout.Compact(new ReadoutInput()));
        var panel = PlayerPanels.Build(PlayerPanelKind.Info, new PlayerState(), 0, new Settings(), new ReadoutInput());
        Assert.Equal("Playback details", panel.Title);
        Assert.Equal(new[] { new PlayerRow(PlayerRowKind.Note, "Asking the server…") }, panel.Rows);
        Assert.Equal("E-AC-3 JOC (Atmos)", Readout.CodecName("audio/eac3-joc"));
        Assert.Equal("x-unknown", Readout.CodecName("video/x-unknown"));
    }
}

/// <summary>The seek previews' arithmetic (the Android app's TrickplayTest).</summary>
public class TrickplayTests
{
    static readonly TrickplayInfo Info = new TrickplayInfo { Id = "v1", IntervalMs = 10_000, Width = 320, Height = 180, Columns = 10, Rows = 10, Count = 250, Sheets = 3 };

    static (int, int, int, int, int) Region(int k)
    {
        var f = Trickplay.At(Info, 7, k * Info.IntervalMs, (file, sheet, v) => $"/api/files/{file}/trickplay/{sheet}.jpg?v={v}")!;
        return (f.Sheet, f.Left, f.Top, f.Left + f.Width, f.Top + f.Height);
    }

    [Fact]
    public void Tile_k_is_on_sheet_k_over_columns_times_rows_at_its_column_and_row()
    {
        Assert.Equal((0, 0, 0, 320, 180), Region(0));
        Assert.Equal((0, 320 * 9, 0, 3_200, 180), Region(9));
        Assert.Equal((0, 0, 180, 320, 360), Region(10));
        Assert.Equal((0, 320 * 9, 180 * 9, 3_200, 1_800), Region(99));
        Assert.Equal((1, 0, 0, 320, 180), Region(100));
        Assert.Equal((2, 320 * 3, 180 * 4, 320 * 4, 180 * 5), Region(243));
        // Positions to thumbnails: k × interval, clamped to the last one.
        Assert.Equal(0, Trickplay.ThumbnailAt(Info, 9_999));
        Assert.Equal(1, Trickplay.ThumbnailAt(Info, 10_000));
        Assert.Equal(249, Trickplay.ThumbnailAt(Info, 99_000_000));
        var frame = Trickplay.At(Info, 7, 1_000_000, (file, sheet, v) => $"/api/files/{file}/trickplay/{sheet}.jpg?v={v}")!;
        Assert.Equal(("/api/files/7/trickplay/1.jpg?v=v1", 3_200, 1_800), (frame.SheetUrl, frame.SheetWidth, frame.SheetHeight));
        Assert.Null(Trickplay.At(null, 7, 0, (_, _, _) => ""));
        Assert.Null(Trickplay.At(Info with { Count = 0 }, 7, 0, (_, _, _) => ""));
    }
}
