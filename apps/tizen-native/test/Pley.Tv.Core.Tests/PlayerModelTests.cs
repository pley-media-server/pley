using System.Collections.Generic;
using System.Linq;
using Pley.Tv.Core.Api;
using Pley.Tv.Core.Net;
using Pley.Tv.Core.Player;
using Pley.Tv.Core.Ui;
using Xunit;

namespace Pley.Tv.Core.Tests;

/// <summary>A playback start to end, on a fake engine (the Android app's session tests: SessionHarness, TracksTest, MarkersAndUpNextTest…).</summary>
public class PlayerModelTests
{
    readonly PlayerHarness h = new PlayerHarness();

    [Fact]
    public void Starts_a_session_for_the_item_and_plays_it_from_where_it_was()
    {
        h.Add(PlayerHarness.Movie());
        h.Start(new PlayRequest(1, 120_000, false));
        var body = h.LastBody;
        Assert.Equal((1L, 10L, 120_000L, "auto", true), ((long)body["item_id"]!, (long)body["file_id"]!, (long)body["start_ms"]!, (string)body["mode"]!, (bool)body["keyed_urls"]!));
        // Automatic audio, as the details give it: named to no server.
        Assert.False(body.ContainsKey("audio_index"));
        var source = h.Engine.Prepared.Single();
        Assert.Equal((ItemKit.Origin + "/api/media/k3y/files/10/stream", false, "video/x-matroska", 120_000L, "eac3"), (source.Url, source.Hls, source.Mime, source.StartMs, source.AudioCodec));
        Assert.Equal(("Heat (1995)", (string?)null, "Direct play"), (h.State.Title, h.State.Subtitle, h.State.Method));
        Assert.Equal(PhaseKind.Playing, h.State.Phase.Kind);
        Assert.Equal(2_700_000, h.Model.Clock.Value.DurationMs);
        Assert.NotNull(h.State.FirstFrameMs);
        // The server's audio is switched to by the engine in direct play.
        Assert.Equal(new[] { (0, (string?)"eng") }, h.Engine.AudioSwitches);
    }

    [Fact]
    public void Reports_progress_every_10_s_while_playing_on_pause_and_ends_the_session_on_close()
    {
        h.Add(PlayerHarness.Movie());
        h.Start(new PlayRequest(1, 0, true));
        h.Play(10_000);
        Assert.Equal(new[] { (1L, 10_000L, "playing", "s1") }, h.Source.Reports);
        h.Model.Pause();
        Assert.Equal(("paused", PhaseKind.Paused), (h.Source.Reports.Last().State, h.State.Phase.Kind));
        h.Play(29_000);
        Assert.Equal(2, h.Source.Reports.Count);
        h.Play(1_000);
        Assert.Equal(3, h.Source.Reports.Count);
        h.Model.Close();
        Assert.Equal(("stopped", "s1"), (h.Source.Reports.Last().State, h.Source.Reports.Last().Session));
        Assert.Equal(new[] { "s1" }, h.Source.Stopped);
        Assert.True(h.Engine.Released);
        // Once.
        h.Model.Close();
        Assert.Single(h.Source.Stopped);
    }

    [Fact]
    public void A_seek_reports_a_second_after_and_stops_short_of_the_end()
    {
        h.Add(PlayerHarness.Movie());
        h.Start(new PlayRequest(1, 0, true));
        h.Model.SeekTo(9_999_999);
        Assert.Equal(2_700_000 - PlayerModel.SeekEndMarginMs, h.Engine.Seeks.Single());
        h.Clock.Advance(1_000);
        Assert.Equal(("playing", 2_700_000 - PlayerModel.SeekEndMarginMs), (h.Source.Reports.Single().State, h.Source.Reports.Single().Position));
    }

    /// <summary>
    /// The audio chosen is switched to once the engine is prepared, before its picture: a TV's
    /// player that says it's ready only once its picture moves would play the file's first track
    /// until then.
    /// </summary>
    [Fact]
    public void The_audio_chosen_is_switched_to_once_prepared_before_the_picture()
    {
        h.Add(PlayerHarness.Movie());
        h.Start(new PlayRequest(1, 0, true, AudioIndex: 2), show: false);
        Assert.Empty(h.Engine.AudioSwitches);
        h.Engine.Open();
        Assert.Equal((1, "fre"), h.Engine.AudioSwitches.Single());
        h.Engine.Show();
        Assert.Single(h.Engine.AudioSwitches);
    }

    [Fact]
    public void Direct_play_switches_audio_in_the_engine_and_remembers_the_choice()
    {
        h.Add(PlayerHarness.Movie());
        h.Start(new PlayRequest(1, 0, true));
        h.Model.SelectAudio(2);
        Assert.Equal((1, "fre"), h.Engine.AudioSwitches.Last());
        Assert.Single(h.Source.Bodies);
        Assert.Equal((1L, (long?)10, (int?)2, (int?)null), h.Source.Streams.Single());
        Assert.True(h.State.Audio.Single(a => a.Index == 2).Selected);
        // Subtitles changed alone leave the audio the server chose out of what is saved.
        var other = new PlayerHarness().Add(PlayerHarness.Movie());
        other.Start(new PlayRequest(1, 0, true));
        other.Model.SelectSubtitle(3);
        Assert.Equal((1L, (long?)10, (int?)null, (int?)3), other.Source.Streams.Single());
    }

    [Fact]
    public void A_streams_audio_switch_starts_a_session_with_audio_index()
    {
        h.Add(PlayerHarness.Movie());
        h.Source.Answer = body => FakePlayerSource.Stream((long)body["file_id"]!) with { AudioIndex = body.TryGetValue("audio_index", out var a) ? (int?)a : 1 };
        h.Start(new PlayRequest(1, 0, true));
        h.Play(5_000);
        h.Model.SelectAudio(2);
        Assert.Equal(2, h.Source.Bodies.Count);
        Assert.Equal(2, h.LastBody["audio_index"]);
        Assert.Equal(5_000L, h.LastBody["start_ms"]);
        // The session before ended first.
        Assert.Equal(new[] { "s1" }, h.Source.Stopped);
        Assert.Equal("stopped", h.Source.Reports.Last().State);
    }

    [Fact]
    public void Text_subtitles_come_from_the_servers_webvtt_and_follow_the_playhead_and_the_offset()
    {
        h.Add(PlayerHarness.Movie());
        h.Source.Vtt[ItemKit.Origin + "/api/files/10/subtitles/3.vtt"] = new SubtitleText("WEBVTT\n\n00:00:01.000 --> 00:00:04.000\nHello\n", Complete: true);
        h.Start(new PlayRequest(1, 0, true, SubtitleIndex: 3));
        Assert.True(h.State.SubtitleSync);
        Assert.True(h.State.SubtitleStyled);
        Assert.Empty(h.Model.Cues.Value);
        h.Play(1_000);
        Assert.Equal("Hello", h.Model.Cues.Value.Single().Text);
        // Moved 3.5 s later: gone until then, and kept for the file and the profile.
        h.Model.SetSubtitleOffset(3_500);
        Assert.Empty(h.Model.Cues.Value);
        Assert.Equal(3_500, new SubtitleOffsets(h.Source.Storage, () => 1).Get(ItemKit.Origin, 10));
        h.Play(4_000);
        Assert.Equal("Hello", h.Model.Cues.Value.Single().Text);
        // Off is off.
        h.Model.SelectSubtitle(null);
        Assert.Empty(h.Model.Cues.Value);
        Assert.False(h.State.SubtitleSync);
        Assert.False(h.State.SubtitleStyled);
    }

    [Fact]
    public void Subtitles_that_cant_be_loaded_say_so()
    {
        h.Add(PlayerHarness.Movie());
        h.Start(new PlayRequest(1, 0, true, SubtitleIndex: 3));
        Assert.Equal(SessionText.SubtitlesFailed, h.Toasts.Single().Text);
    }

    const string PgsMks = ItemKit.Origin + "/api/files/10/subtitles/4.mks";

    /// <summary>A PGS track that shows a picture from 1 s and clears at 3 s.</summary>
    static SubtitlePictures PgsTrack()
    {
        var shown = PictureKit.PgsDisplaySet(0x80, true, rle: new[] { 1, 1, 1, 1, 0, 0, 1, 1, 1, 1, 0, 0 });
        var cleared = PictureKit.PgsDisplaySet(0x00, false);
        return new SubtitlePictures(PictureKit.Mks("S_HDMV/PGS", new[] { new PictureKit.MksBlock(1000, shown), new PictureKit.MksBlock(3000, cleared) }), Complete: true);
    }

    /// <summary>The server burns in pictures it is asked to, into a stream; without, the file plays as it is.</summary>
    void BurnsWhenAsked() => h.Source.Answer = body => body.ContainsKey("subtitle_index")
        ? FakePlayerSource.Stream(10) with { BurnSubtitles = true, SubtitleIndex = (int)body["subtitle_index"]! }
        : FakePlayerSource.Direct(10);

    /// <summary>
    /// The player draws picture subtitles from the server's .mks, as the web TV app does: chosen,
    /// the file plays on as it is, and they follow the playhead and the offset like text.
    /// </summary>
    [Fact]
    public void Picture_subtitles_are_drawn_from_the_servers_mks_in_the_same_session()
    {
        h.Add(PlayerHarness.Movie());
        BurnsWhenAsked();
        h.Source.Mks[PgsMks] = PgsTrack();
        h.Start(new PlayRequest(1, 0, true));
        Assert.False(h.State.Subtitles.Single(t => t.Index == 4).BurnIn);
        h.Model.SelectSubtitle(4);
        Assert.Single(h.Source.Bodies);
        Assert.False(h.LastBody.ContainsKey("subtitle_index"));
        Assert.True(h.State.SubtitleSync);
        // They move with Sync but keep their own look: the panel has none to choose.
        Assert.False(h.State.SubtitleStyled);
        Assert.DoesNotContain(h.Model.Panel(PlayerPanelKind.Subtitles).Rows, r => r.Kind == PlayerRowKind.Heading);
        Assert.Null(h.Model.Picture.Value);
        h.Play(1_000);
        var frame = h.Model.Picture.Value!;
        Assert.Equal((1920, 1080), (frame.PlaneWidth, frame.PlaneHeight));
        Assert.Equal((100, 900, 4, 2), (frame.Bitmaps.Single().X, frame.Bitmaps.Single().Y, frame.Bitmaps.Single().Width, frame.Bitmaps.Single().Height));
        Assert.Empty(h.Model.Cues.Value);
        // Moved 1.5 s later: not yet at 1.5 s, shown at 2.5 s.
        h.Play(500);
        h.Model.SetSubtitleOffset(1_500);
        Assert.Null(h.Model.Picture.Value);
        h.Play(1_000);
        Assert.NotNull(h.Model.Picture.Value);
        // Cleared by the next display set, 3 s in the track's time.
        h.Play(2_000);
        Assert.Null(h.Model.Picture.Value);
        h.Model.SelectSubtitle(null);
        Assert.Null(h.Model.Picture.Value);
        Assert.Single(h.Source.Bodies);
        // Asked for once, whole, from where the player was as they were chosen: the start, which is said too.
        Assert.Equal(new[] { PgsMks + "?at=0" }, h.Source.MksAsked);
    }

    /// <summary>
    /// A server that can't make the .mks (404 from an older server, 503 from an older transcoder):
    /// pictures are burned in from then on, by a new session from where it was, as the web TV app does.
    /// </summary>
    [Theory]
    [InlineData(404)]
    [InlineData(503)]
    public void Picture_subtitles_are_burned_in_when_the_server_cant_make_them(int status)
    {
        h.Add(PlayerHarness.Movie());
        BurnsWhenAsked();
        h.Source.MksFailure = new ApiException(status, "unavailable", "no pictures here");
        h.Start(new PlayRequest(1, 0, true));
        h.Play(2_000);
        h.Model.SelectSubtitle(4);
        Assert.Equal(2, h.Source.Bodies.Count);
        Assert.Equal(4, h.LastBody["subtitle_index"]);
        Assert.Equal(2_000L, h.LastBody["start_ms"]);
        Assert.True(h.Engine.Prepared.Last().Hls);
        Assert.True(h.State.Subtitles.Single(t => t.Index == 4).BurnIn);
        Assert.False(h.State.SubtitleSync);
        Assert.Null(h.Model.Picture.Value);
        Assert.Empty(h.Toasts);
        h.Engine.Show();
        h.Model.SelectSubtitle(null);
        Assert.Equal(3, h.Source.Bodies.Count);
        Assert.False(h.LastBody.ContainsKey("subtitle_index"));
        // Chosen again, they are burned in at once: the .mks isn't asked for again.
        h.Engine.Show();
        h.Model.SelectSubtitle(4);
        Assert.Equal(4, h.Source.Bodies.Count);
        Assert.Equal(4, h.LastBody["subtitle_index"]);
        Assert.Single(h.Source.MksAsked);
    }

    /// <summary>
    /// A whole .mks with Cues is read in parts: the pictures around where the film resumed, then
    /// those around a seek; never the whole file.
    /// </summary>
    [Fact]
    public void Picture_subtitles_are_read_around_the_film_in_parts()
    {
        h.Add(PlayerHarness.Movie());
        h.Source.Mks[PgsMks] = new SubtitlePictures(CuedMks.Pgs(Enumerable.Range(0, 10).Select(i => i * 120_000).ToArray()), Complete: true);
        h.Start(new PlayRequest(1, 600_000, false, SubtitleIndex: 4), show: false);
        h.Engine.PositionMs = 600_000;
        h.Engine.Show();
        // Its header, its Cues, the clusters around 600 s.
        Assert.Equal(3, h.Source.MksParts.Count);
        Assert.Empty(h.Source.MksAsked);
        h.Play(500);
        Assert.NotNull(h.Model.Picture.Value);
        h.Model.SeekTo(1_000_000);
        Assert.Equal(4, h.Source.MksParts.Count);
        h.Play(500);
        Assert.NotNull(h.Model.Picture.Value);
        // Back where it was read: not asked for again.
        h.Model.SeekTo(650_000);
        h.Play(500);
        Assert.Equal(4, h.Source.MksParts.Count);
    }

    /// <summary>
    /// A picture track chosen as the film starts is asked for once its picture shows: its .mks is
    /// tens of MB, and asked for at once it took the network and the CPUs from the video's first
    /// seconds. A new session of the same film keeps it.
    /// </summary>
    [Fact]
    public void Picture_subtitles_are_asked_for_once_the_picture_shows()
    {
        h.Add(PlayerHarness.Movie());
        h.Source.Mks[PgsMks] = PgsTrack();
        h.Start(new PlayRequest(1, 0, true, SubtitleIndex: 4), show: false);
        Assert.Empty(h.Source.MksAsked);
        h.Engine.Show();
        Assert.Single(h.Source.MksAsked);
        h.Play(1_000);
        Assert.NotNull(h.Model.Picture.Value);
        h.Engine.AskRestart(500);
        Assert.Single(h.Source.MksAsked);
        h.Engine.Show();
        Assert.Single(h.Source.MksAsked);
    }

    [Fact]
    public void Picture_subtitles_that_cant_be_read_say_so_and_play_on()
    {
        h.Add(PlayerHarness.Movie());
        h.Source.Mks[PgsMks] = new SubtitlePictures(new byte[] { 1, 2, 3, 4 }, Complete: true);
        h.Start(new PlayRequest(1, 0, true, SubtitleIndex: 4));
        Assert.Equal(SessionText.SubtitlesFailed, h.Toasts.Single().Text);
        Assert.Single(h.Source.Bodies);
        Assert.Null(h.Model.Picture.Value);
    }

    /// <summary>A report that claims no picture subtitles (an app that can't draw them) has them burned in.</summary>
    [Fact]
    public void Picture_subtitles_are_burned_in_when_the_report_doesnt_claim_them()
    {
        h.Add(PlayerHarness.Movie());
        BurnsWhenAsked();
        h.Source.Report = TizenCaps.Of(new TizenFacts { OutputMode = "DOLBY_DIGITAL_PLUS", PictureSubtitles = false });
        h.Start(new PlayRequest(1, 0, true));
        Assert.True(h.State.Subtitles.Single(t => t.Index == 4).BurnIn);
        h.Model.SelectSubtitle(4);
        Assert.Equal(4, h.LastBody["subtitle_index"]);
        Assert.Empty(h.Source.MksAsked);
    }

    [Fact]
    public void Skip_intro_shows_inside_the_intro_and_lands_750_ms_before_its_end()
    {
        var intro = new Marker { FileId = 10, Kind = "intro", StartMs = 10_000, EndMs = 70_000, Source = "chapter" };
        h.Add(PlayerHarness.Episode(1, 1, markers: new[] { intro }));
        h.Start(new PlayRequest(1, 9_000, false));
        h.Play(500);
        Assert.Null(h.State.SkipLabel);
        h.Play(1_000);
        Assert.Equal("Skip Intro", h.State.SkipLabel);
        h.Model.Skip();
        Assert.Equal(69_250, h.Engine.Seeks.Single());
        h.Play(500);
        Assert.Null(h.State.SkipLabel);
    }

    [Fact]
    public void Entering_the_credits_brings_up_next_whose_countdown_plays_it_on_the_same_engine()
    {
        var credits = new Marker { FileId = 10, Kind = "credits", StartMs = 2_600_000, EndMs = 2_700_000, Source = "chapter" };
        var next = PlayerHarness.Episode(2, 2);
        h.Add(PlayerHarness.Episode(1, 1, markers: new[] { credits })).Add(next);
        h.Source.NextOf[1] = next with { ThumbUrl = "/api/items/2/images/thumb?v=1", UserState = new UserState { PositionMs = 30_000 } };
        h.Start(new PlayRequest(1, 2_599_000, false));
        Assert.Equal(("Ashfall", "S1 · E1 · Episode 1"), (h.State.Title, h.State.Subtitle));
        h.Play(1_000);
        var card = h.State.UpNext!;
        Assert.Equal((2L, "Episode 2", "Up next · S1 · E2", 10), (card.ItemId, card.Title, card.Heading, card.SecondsLeft));
        Assert.Equal(ItemKit.Origin + "/api/items/2/images/thumb?v=1&w=480", card.ImageUrl);
        h.Play(5_000);
        Assert.Equal(5, h.State.UpNext!.SecondsLeft);
        h.Play(5_000);
        // The one before ended, the next starts where it was left.
        Assert.Null(h.State.UpNext);
        Assert.Equal(new[] { "s1" }, h.Source.Stopped);
        Assert.Equal((2L, 20L, 30_000L), ((long)h.LastBody["item_id"]!, (long)h.LastBody["file_id"]!, (long)h.LastBody["start_ms"]!));
        Assert.Equal(2, h.State.ItemId);
        Assert.Equal("S1 · E2 · Episode 2", h.State.Subtitle);
        Assert.Equal(2, h.Engine.Prepared.Count);
    }

    [Fact]
    public void An_engine_that_cant_go_on_from_a_place_gets_a_session_there_under_the_cover()
    {
        h.Add(PlayerHarness.Episode(1, 1));
        h.Start(new PlayRequest(1, 1_800_000, false));
        h.Play(1_000);
        Assert.NotNull(h.State.FirstFrameMs);
        // A seek before where the stream it has begins (a resumed film's, trimmed to where it resumed).
        h.Engine.AskRestart(600_000);
        Assert.Equal(new[] { "s1" }, h.Source.Stopped);
        Assert.Equal(600_000L, (long)h.LastBody["start_ms"]!);
        Assert.Equal(2, h.Engine.Prepared.Count);
        Assert.Equal(600_000, h.Engine.Prepared[1].StartMs);
        // Black until the new stream's first picture, as for any new stream.
        Assert.Null(h.State.FirstFrameMs);
        Assert.Equal(PhaseKind.Preparing, h.State.Phase.Kind);
    }

    /// <summary>
    /// A file that ends a minute or more before the item says: the engine opens it again where it
    /// ended, and it ends there again. That is its end, weighed from where the film started, not a
    /// stream that doesn't hold that part (which falls back to a transcode).
    /// </summary>
    [Fact]
    public void A_film_that_ends_again_where_the_engine_opened_it_again_ends()
    {
        h.Add(PlayerHarness.Movie());
        h.Start(new PlayRequest(1, 1_000_000, false));
        h.Play(10_000);
        h.Engine.AskRestart(2_000_000);
        Assert.Equal(2, h.Source.Bodies.Count);
        h.Engine.Show();
        h.Engine.PositionMs = 2_000_500;
        h.Engine.Tell(EngineStatus.Ended);
        Assert.Equal(PhaseKind.Ended, h.State.Phase.Kind);
        Assert.Equal(2, h.Source.Bodies.Count);
    }

    [Fact]
    public void Without_autoplay_the_card_waits_and_cancelling_it_at_the_end_ends_playback()
    {
        var next = PlayerHarness.Episode(2, 2);
        h.Add(PlayerHarness.Episode(1, 1)).Add(next);
        h.Source.NextOf[1] = next;
        h.Source.Settings = h.Source.Settings with { AutoplayNext = false };
        h.Start(new PlayRequest(1, 0, true));
        h.End();
        Assert.Equal(PhaseKind.Paused, h.State.Phase.Kind);
        Assert.Null(h.State.UpNext!.SecondsLeft);
        h.Play(30_000);
        Assert.Single(h.Source.Bodies);
        h.Model.CancelUpNext();
        Assert.Equal(PhaseKind.Ended, h.State.Phase.Kind);
        Assert.Null(h.State.UpNext);
    }

    [Fact]
    public void Play_next_says_so_when_there_is_nothing_next_and_a_playlist_plays_its_next_video()
    {
        h.Add(PlayerHarness.Episode(1, 1));
        h.Start(new PlayRequest(1, 0, true));
        h.Model.PlayNext();
        Assert.Equal("This is the last episode", h.Toasts.Single().Text);

        var p = new PlayerHarness().Add(PlayerHarness.Movie(1)).Add(PlayerHarness.Movie(5));
        p.Source.Queues[9] = new[] { new ItemSummary { Id = 1, Kind = "movie" }, new ItemSummary { Id = 3, Kind = "track" }, new ItemSummary { Id = 5, Kind = "movie" } };
        p.Start(new PlayRequest(1, 0, true, Queue: new PlayQueue.Playlist(9)));
        p.End();
        Assert.Equal(5, p.State.UpNext!.ItemId);
        Assert.Equal("Up next", p.State.UpNext.Heading);
        p.Model.PlayNext();
        Assert.Equal(5L, p.LastBody["item_id"]);
    }

    [Fact]
    public void The_end_of_a_movie_is_the_end_and_reports_the_whole_of_it()
    {
        h.Add(PlayerHarness.Movie());
        h.Start(new PlayRequest(1, 2_000_000, false));
        h.Play(10_000);
        h.Engine.PositionMs = 2_699_000;
        h.Engine.Tell(EngineStatus.Ended);
        Assert.Equal(PhaseKind.Ended, h.State.Phase.Kind);
        Assert.Equal(("stopped", 2_700_000L), (h.Source.Reports.Last().State, h.Source.Reports.Last().Position));
    }

    [Fact]
    public void Media_that_ends_where_it_started_is_asked_again_as_a_transcode_never_the_end()
    {
        h.Add(PlayerHarness.Movie());
        h.Start(new PlayRequest(1, 1_000_000, false));
        h.Engine.PositionMs = 1_001_000;
        h.Engine.Tell(EngineStatus.Ended);
        Assert.Equal(2, h.Source.Bodies.Count);
        Assert.Equal(("transcode", 1_000_000L), ((string)h.LastBody["mode"]!, (long)h.LastBody["start_ms"]!));
        // Nothing reported at the media's end.
        Assert.DoesNotContain(h.Source.Reports, r => r.Position > 1_000_000);
    }

    [Fact]
    public void An_unsupported_track_in_direct_play_falls_back_to_a_remux_then_a_transcode_then_fails()
    {
        h.Add(PlayerHarness.Movie());
        h.Start(new PlayRequest(1, 0, true));
        h.Engine.Fail(new EngineError.Unsupported("audio", Audio: true, Container: false));
        Assert.Equal("remux", h.LastBody["mode"]);
        h.Source.Answer = _ => FakePlayerSource.Stream(10, "remux");
        h.Engine.Fail(new EngineError.Unsupported("audio", Audio: true, Container: false));
        Assert.Equal("transcode", h.LastBody["mode"]);
        h.Source.Answer = _ => FakePlayerSource.Stream(10);
        h.Engine.Fail(new EngineError.Unsupported("audio", Audio: true, Container: false));
        Assert.Equal(PlayerPhase.Error(SessionText.ErrorUnplayable, canRetry: false), h.State.Phase);
    }

    [Fact]
    public void An_engine_failure_names_its_code_and_retry_starts_again_where_it_was()
    {
        h.Add(PlayerHarness.Movie());
        h.Start(new PlayRequest(1, 0, true));
        h.Play(3_000);
        h.Engine.Fail(new EngineError.Other("PLAYER_ERROR_NOT_SUPPORTED_FORMAT: boom"));
        Assert.Equal(("Playback failed (PLAYER_ERROR_NOT_SUPPORTED_FORMAT).", true, "Can't play this item"), (h.State.Phase.Message, h.State.Phase.CanRetry, h.State.Phase.Title));
        h.Model.Retry();
        Assert.Equal(2, h.Source.Bodies.Count);
        Assert.Equal(3_000L, h.LastBody["start_ms"]);
    }

    [Fact]
    public void A_server_out_of_reach_at_the_start_says_so()
    {
        h.Add(PlayerHarness.Movie());
        h.Source.StartFailure = new NetworkException(NetworkFailure.NoAnswerInTime);
        h.Start(new PlayRequest(1, 0, true));
        Assert.Equal(PhaseKind.Error, h.State.Phase.Kind);
        Assert.Equal(PlayerText.OfflineTitle, h.State.Phase.Title);
        Assert.EndsWith("Check that the server is on and that this device is on its network.", h.State.Phase.Message);
        Assert.True(h.State.Phase.CanRetry);
        // An item that isn't there can't be tried again.
        var missing = new PlayerHarness();
        missing.Start(new PlayRequest(99, 0, true));
        Assert.False(missing.State.Phase.CanRetry);
    }

    [Fact]
    public void A_server_that_stops_answering_mid_playback_is_waited_for_then_a_new_session_starts_there()
    {
        h.Add(PlayerHarness.Movie());
        h.Start(new PlayRequest(1, 0, true));
        h.Play(20_000);
        h.Source.StatusFailure = new NetworkException(NetworkFailure.NothingAnswers);
        h.Engine.Fail(new EngineError.Network("connection reset"));
        Assert.True(h.State.Reconnecting);
        Assert.Equal(PhaseKind.Buffering, h.State.Phase.Kind);
        h.Clock.Advance(3_500);
        Assert.True(h.State.Reconnecting);
        h.Source.StatusFailure = null;
        h.Clock.Advance(2_000);
        Assert.False(h.State.Reconnecting);
        Assert.Equal(2, h.Source.Bodies.Count);
        Assert.Equal(20_000L, h.LastBody["start_ms"]);
    }

    [Fact]
    public void A_server_gone_for_90_s_fails_as_out_of_reach()
    {
        h.Add(PlayerHarness.Movie());
        h.Start(new PlayRequest(1, 0, true));
        h.Play(5_000);
        h.Source.StatusFailure = new NetworkException(NetworkFailure.NothingAnswers);
        h.Engine.Fail(new EngineError.Network("gone"));
        h.Clock.Advance(PlayerModel.OutageMs + 2_000);
        Assert.Equal(PlayerText.OfflineTitle, h.State.Phase.Title);
        Assert.False(h.State.Reconnecting);
    }

    [Fact]
    public void A_session_the_server_stopped_says_who_and_plays_no_more()
    {
        h.Add(PlayerHarness.Movie());
        h.Start(new PlayRequest(1, 0, true));
        h.Play(4_000);
        h.Source.Emit(new ServerEvent { Type = "session_stopped", SessionId = "s1", Message = "Bedtime", ByAdmin = true });
        Assert.True(h.State.StoppedByServer);
        Assert.Equal(PlayerPhase.Error("Playback was stopped by the server's administrator.\nBedtime", canRetry: false, PlayerText.StoppedTitle), h.State.Phase);
        Assert.Equal(1, h.Engine.Stops);
        // "stopped" for the resume point; nothing deleted, nothing started again.
        Assert.Equal(("stopped", 4_000L), (h.Source.Reports.Last().State, h.Source.Reports.Last().Position));
        Assert.Empty(h.Source.Stopped);
        h.Model.Retry();
        h.Model.Play();
        Assert.Single(h.Source.Bodies);
    }

    [Fact]
    public void A_stalled_direct_play_becomes_a_transcode_a_slow_server_suggests_a_lower_quality()
    {
        h.Add(PlayerHarness.Movie());
        h.Start(new PlayRequest(1, 0, true));
        h.Play(2_000);
        // Ready, meant to play, but not moving.
        h.Clock.Advance(PlayerModel.StallMs + 1_000);
        Assert.Equal("transcode", h.LastBody["mode"]);
        Assert.Equal(SessionText.FallbackNotSmooth, h.State.Note);

        var s = new PlayerHarness().Add(PlayerHarness.Movie());
        s.Source.Answer = _ => FakePlayerSource.Stream(10);
        s.Source.Status = id => new SessionStatus { SessionId = id, Method = "transcode", Transcode = new TranscodeInfo { JobId = "j", Speed = 0.7 } };
        s.Start(new PlayRequest(1, 0, true));
        s.Play(2_000);
        s.Clock.Advance(PlayerModel.StallMs + 1_000);
        Assert.True(s.State.SuggestLowerQuality);
        Assert.Equal(SessionText.SlowServer, s.State.Note);
        Assert.Equal(PlayerText.SlowServerToast(SessionText.SlowServer), s.Toasts.Single().Text);
        Assert.True(s.Toasts.Single().Error);
        Assert.Single(s.Source.Bodies);
    }

    [Fact]
    public void Dropped_frames_switch_the_original_to_a_transcode_once_and_play_original_goes_back()
    {
        h.Add(PlayerHarness.Movie());
        h.Source.Settings = h.Source.Settings with { Quality = "original" };
        h.Start(new PlayRequest(1, 0, true));
        long rendered = 0, dropped = 0;
        for (var i = 0; i < 8; i++)
        {
            rendered += 48;
            dropped += 2;
            h.Engine.Frames = new FrameCounts(rendered, dropped);
            h.Play(2_000);
        }
        Assert.Equal("transcode", h.LastBody["mode"]);
        Assert.True(h.State.CanPlayOriginal);
        Assert.Equal(PlayerText.TranscodeToast(SessionText.FallbackNotSmooth), h.Toasts.Single().Text);
        var quality = h.Model.Panel(PlayerPanelKind.Quality);
        Assert.Equal(new PlayerRow(PlayerRowKind.Note, SessionText.FallbackNotSmooth), quality.Rows[0]);
        Assert.Equal(PlayerText.PlayOriginal, quality.Rows[1].Label);
        h.Model.Choose(quality.Rows[1]);
        Assert.Equal("auto", h.LastBody["mode"]);
        Assert.False(h.State.CanPlayOriginal);
    }

    [Fact]
    public void Picking_a_quality_starts_a_session_at_it_and_is_remembered_original_and_automatic_play_the_same()
    {
        h.Add(PlayerHarness.Movie());
        h.Start(new PlayRequest(1, 0, true));
        Assert.Equal("Automatic · Original", h.State.Quality);
        Assert.Equal(new[] { "original", "auto", "8m", "4m", "2m" }, h.State.Qualities.Select(q => q.Id));
        Assert.Equal(("1080p · 10 Mbps", "Dynamically adjust to conditions"), (h.State.Qualities[0].Hint, h.State.Qualities[1].Hint));
        h.Model.SelectQuality("original");
        Assert.Single(h.Source.Bodies);
        Assert.Equal(("Original", "Original"), (h.Source.Settings.Quality, h.State.Quality));
        h.Model.SelectQuality("8m");
        Assert.Equal((8_000, 1_080, "transcode"), ((int)h.LastBody["max_bitrate"]!, (int)h.LastBody["max_height"]!, (string)h.LastBody["mode"]!));
        Assert.Equal("8 Mbps 1080p", h.State.Quality);
        // A quality the file isn't offered changes nothing.
        h.Model.SelectQuality("30m");
        Assert.Equal(2, h.Source.Bodies.Count);
    }

    [Fact]
    public void Automatic_names_the_quality_the_server_converts_the_original_at()
    {
        h.Add(PlayerHarness.Movie());
        h.Source.Answer = _ => FakePlayerSource.Stream(10) with { AutoQuality = "8m" };
        h.Start(new PlayRequest(1, 0, true));
        Assert.Equal("Automatic · 8 Mbps 1080p", h.State.Quality);
        Assert.Equal("Now 8 Mbps 1080p", h.State.Qualities.Single(q => q.Id == "auto").Hint);
        Assert.Equal("Transcoding", h.State.Method);
    }

    [Fact]
    public void A_dolby_vision_profile_5_remux_is_asked_again_as_a_transcode()
    {
        var p5 = PlayerHarness.Video with { DynamicRange = "dolby_vision", DvProfile = 5 };
        var item = PlayerHarness.Movie() with { Files = new[] { PlayerHarness.FileOf(10) with { Streams = new[] { p5, PlayerHarness.English } } } };
        h.Add(item);
        h.Source.Answer = body => (string)body["mode"]! == "transcode" ? FakePlayerSource.Stream(10) : FakePlayerSource.Stream(10, "remux");
        h.Start(new PlayRequest(1, 0, true));
        Assert.Equal(new[] { "auto", "transcode" }, h.Source.Bodies.Select(b => (string)b["mode"]!));
        // The first never played: deleted without "stopped".
        Assert.Equal(new[] { "s1" }, h.Source.Stopped);
        Assert.DoesNotContain(h.Source.Reports, r => r.Session == "s1");
        Assert.Single(h.Engine.Prepared);
    }

    [Fact]
    public void An_address_without_its_own_key_wont_play_on_this_tv()
    {
        h.Add(PlayerHarness.Movie());
        h.Source.Answer = _ => FakePlayerSource.Direct(10) with { Url = "/api/files/10/stream" };
        h.Start(new PlayRequest(1, 0, true));
        Assert.Equal("This server is too old to play on this TV: update it.", h.State.Phase.Message);
        Assert.Empty(h.Engine.Prepared);
        Assert.True(PlayerModel.IsKeyed("http://nas:7539/api/media/abc/files/1/stream"));
        Assert.False(PlayerModel.IsKeyed("http://nas:7539/api/media/abc"));
    }

    [Fact]
    public void At_the_end_of_the_episode_the_sleep_timer_pauses_and_the_next_one_waits()
    {
        var next = PlayerHarness.Episode(2, 2);
        h.Add(PlayerHarness.Episode(1, 1)).Add(next);
        h.Source.NextOf[1] = next;
        h.Start(new PlayRequest(1, 0, true));
        h.Model.SetSleep(new SleepChoice.EndOfItem());
        Assert.Equal(new SleepChoice.EndOfItem(), h.State.Sleep.Set);
        h.End();
        Assert.True(h.State.Sleep.Fired);
        Assert.False(h.Model.WantsToPlay);
        Assert.Null(h.State.UpNext!.SecondsLeft);
        h.Play(20_000);
        Assert.Single(h.Source.Bodies);
    }

    [Fact]
    public void Minutes_of_sleep_pause_the_playback_and_playing_again_ends_that()
    {
        h.Add(PlayerHarness.Movie());
        h.Start(new PlayRequest(1, 0, true));
        h.Model.SetSleep(new SleepChoice.Minutes(15));
        h.Play(15 * 60_000);
        Assert.False(h.Engine.Playing);
        Assert.True(h.State.Sleep.Fired);
        h.Model.Play();
        Assert.False(h.State.Sleep.Fired);
    }

    [Fact]
    public void The_speed_is_this_titles_only_at_the_speeds_the_tvs_player_plays()
    {
        var next = PlayerHarness.Episode(2, 2);
        h.Add(PlayerHarness.Episode(1, 1)).Add(next);
        h.Source.NextOf[1] = next;
        h.Engine.Speeds = new[] { 0.5, 1, 1.5 };
        h.Start(new PlayRequest(1, 0, true));
        h.Model.SetSpeed(2);
        Assert.Equal(1, h.State.Speed);
        h.Model.SetSpeed(1.5);
        Assert.Equal(1.5, h.State.Speed);
        Assert.Equal(new[] { "0.5×", "1×", "1.5×" }, h.Model.Panel(PlayerPanelKind.Speed).Rows.Select(r => r.Label));
        h.Model.PlayNext();
        Assert.Equal(1, h.State.Speed);
        Assert.Equal(1, h.Engine.SpeedsSet.Last());
        // A player that only plays at normal speed offers none.
        var plain = new PlayerHarness().Add(PlayerHarness.Movie());
        plain.Start(new PlayRequest(1, 0, true));
        Assert.False(RowOptions.Of(plain.State).Speed);
    }

    [Fact]
    public void Seek_previews_are_asked_for_every_20_s_while_the_server_makes_them()
    {
        h.Add(PlayerHarness.Movie());
        h.Start(new PlayRequest(1, 0, true));
        Assert.Equal(1, h.Source.TrickplayAsks);
        Assert.Null(h.Model.Preview(0));
        h.Clock.Advance(20_000);
        Assert.Equal(2, h.Source.TrickplayAsks);
        h.Source.TrickplayAnswer = _ => new TrickplayInfo { Id = "v1", IntervalMs = 10_000, Width = 320, Height = 180, Columns = 10, Rows = 10, Count = 270, Sheets = 3 };
        h.Clock.Advance(20_000);
        var frame = h.Model.Preview(1_005_000)!;
        Assert.Equal((ItemKit.Origin + "/api/files/10/trickplay/1.jpg?v=v1", 0, 0), (frame.SheetUrl, frame.Left, frame.Top));
        h.Clock.Advance(600_000);
        Assert.Equal(3, h.Source.TrickplayAsks);
    }

    [Fact]
    public void The_captions_key_turns_subtitles_off_and_back_on_saying_so()
    {
        h.Add(PlayerHarness.Movie());
        h.Source.Vtt[ItemKit.Origin + "/api/files/10/subtitles/3.vtt"] = new SubtitleText("WEBVTT\n", Complete: true);
        h.Start(new PlayRequest(1, 0, true));
        var remembered = h.Model.ToggleCaptions(null);
        Assert.Equal("Subtitles: English (SRT)", h.Toasts.Last().Text);
        Assert.True(h.State.Subtitles.Single(t => t.Index == 3).Selected);
        remembered = h.Model.ToggleCaptions(remembered);
        Assert.Equal((3, "Subtitles off"), (remembered, h.Toasts.Last().Text));
        h.Model.ToggleCaptions(remembered);
        Assert.True(h.State.Subtitles.Single(t => t.Index == 3).Selected);
    }

    [Fact]
    public void The_details_are_asked_for_while_info_is_open()
    {
        h.Add(PlayerHarness.Movie());
        h.Source.Answer = _ => FakePlayerSource.Stream(10, "remux") with { Reasons = new[] { "EAC3 audio isn't supported by this player." } };
        var asked = 0;
        h.Source.Status = id =>
        {
            asked++;
            return new SessionStatus { SessionId = id, Transcode = new TranscodeInfo { JobId = "j", Video = "copy", Audio = "transcode", AudioIn = "EAC3 5.1", AudioOut = "AAC 2.0" } };
        };
        h.Start(new PlayRequest(1, 0, true));
        Assert.Null(h.State.Details);
        h.Model.SetDetailsOpen(true);
        Assert.Equal(("Direct stream", "EAC3 audio isn't supported by this player."), (h.State.Details!.Method, h.State.Details.Why));
        Assert.Contains(h.State.Details.Rows, r => r.Value == "EAC3 5.1 → AAC 2.0");
        h.Clock.Advance(2_000);
        Assert.Equal(2, asked);
        var info = h.Model.Panel(PlayerPanelKind.Info);
        Assert.Equal(new PlayerRow(PlayerRowKind.Heading, Readout.HowItPlays), info.Rows[0]);
        Assert.Equal(new PlayerRow(PlayerRowKind.Fact, "Playback", Value: "Direct stream"), info.Rows[1]);
        h.Model.SetDetailsOpen(false);
        Assert.Null(h.State.Details);
        h.Clock.Advance(10_000);
        Assert.Equal(2, asked);
    }

    [Fact]
    public void A_failure_the_TV_says_twice_starts_one_session_at_the_next_step()
    {
        h.Add(PlayerHarness.Movie());
        h.Start(new PlayRequest(1, 0, true));
        var again = true;
        h.Source.Answer = _ =>
        {
            // As on the TV: the second report comes while the new session is still being asked for.
            if (again)
            {
                again = false;
                h.Engine.Fail(new EngineError.Unsupported("audio", Audio: true, Container: false));
            }
            return FakePlayerSource.Stream(10, "remux");
        };
        h.Engine.Fail(new EngineError.Unsupported("audio", Audio: true, Container: false));
        Assert.Equal(new[] { "auto", "remux" }, h.Source.Bodies.Select(b => (string)b["mode"]!));
    }

}

/// <summary>The panels as the screen builds them (the Android app's TvPlayerPanels and its screen test).</summary>
public class PlayerPanelTests
{
    readonly PlayerHarness h = new PlayerHarness();

    [Fact]
    public void The_subtitles_panel_ticks_the_current_track_with_sync_above_and_the_look_below()
    {
        h.Add(PlayerHarness.Movie());
        h.Source.Vtt[ItemKit.Origin + "/api/files/10/subtitles/3.vtt"] = new SubtitleText("WEBVTT\n", Complete: true);
        h.Start(new PlayRequest(1, 0, true, SubtitleIndex: 3));
        var panel = h.Model.Panel(PlayerPanelKind.Subtitles);
        Assert.Equal("Subtitles", panel.Title);
        Assert.Equal(new PlayerRow(PlayerRowKind.Step, "Sync", Supporting: "OK resets", Value: "0.0 s", Action: new PlayerRowAction.SubtitleSync()), panel.Rows[0]);
        Assert.Equal(new[] { "Off", "English (SRT)", "French (PGS)" }, panel.Rows.Skip(1).Take(3).Select(r => r.Label));
        // Focus starts on the track playing.
        Assert.Equal(2, panel.Focus);
        Assert.True(panel.Rows[2].Checked);
        Assert.Equal(new[] { "Subtitle size", "Subtitle background", "Subtitle font", "Subtitle position" }, panel.Rows.Where(r => r.Kind == PlayerRowKind.Heading).Select(r => r.Label));
        Assert.Equal("Normal", panel.Rows.First(r => r.Action is PlayerRowAction.SubtitleSize && r.Checked).Label);

        // Sync steps 0.1 s, OK puts it back; both keep the panel open.
        h.Model.Step(panel.Rows[0], 1);
        h.Model.Step(panel.Rows[0], 1);
        Assert.Equal("+0.2 s", h.Model.Panel(PlayerPanelKind.Subtitles).Rows[0].Value);
        Assert.True(h.Model.Choose(panel.Rows[0]));
        Assert.Equal(0, h.State.SubtitleOffsetMs);
        // The look is saved in Settings, the panel stays.
        Assert.True(h.Model.Choose(panel.Rows.First(r => r.Label == "Huge")));
        Assert.Equal(0.076f, h.Source.Settings.SubtitleSize);
        Assert.True(h.Model.Choose(panel.Rows.First(r => r.Label == "Dark box")));
        Assert.Equal(SubtitleBackground.DarkBox, h.Source.Settings.SubtitleBackground);
        // A track closes it.
        Assert.False(h.Model.Choose(panel.Rows[1]));
        Assert.Null(h.State.Subtitles.Single(t => t.Selected).Index);
        // Off, there is nothing to style.
        Assert.DoesNotContain(h.Model.Panel(PlayerPanelKind.Subtitles).Rows, r => r.Kind == PlayerRowKind.Heading);
    }

    [Fact]
    public void The_audio_quality_chapters_and_sleep_panels()
    {
        var item = PlayerHarness.Movie() with
        {
            Files = new[]
            {
                PlayerHarness.FileOf(10) with
                {
                    Chapters = new[]
                    {
                        new Chapter { StartMs = 0, EndMs = 600_000, Title = "Opening" },
                        new Chapter { StartMs = 600_000, EndMs = 1_200_000 },
                        new Chapter { StartMs = 1_200_000, EndMs = 2_700_000, Title = "The heist" },
                    },
                },
            },
        };
        h.Add(item);
        h.Start(new PlayRequest(1, 700_000, false));
        var audio = h.Model.Panel(PlayerPanelKind.Audio);
        Assert.Equal(new[] { "English (EAC3 5.1)", "French (AC3 5.1)" }, audio.Rows.Select(r => r.Label));
        Assert.Equal(0, audio.Focus);
        var quality = h.Model.Panel(PlayerPanelKind.Quality);
        Assert.Equal(new[] { "Original", "Automatic", "8 Mbps 1080p", "4 Mbps 720p", "2 Mbps 480p" }, quality.Rows.Select(r => r.Label));
        Assert.Equal(1, quality.Focus);
        var chapters = h.Model.Panel(PlayerPanelKind.Chapters);
        Assert.Equal(new[] { ("Opening", "0:00"), ("Chapter 2", "10:00"), ("The heist", "20:00") }, chapters.Rows.Select(r => (r.Label, r.Supporting!)));
        Assert.Equal(1, chapters.Focus);
        Assert.False(h.Model.Choose(chapters.Rows[2]));
        Assert.Equal(1_200_000, h.Engine.Seeks.Last());
        Assert.True(RowOptions.Of(h.State).Chapters);
        Assert.True(RowOptions.Of(h.State).PreviousNext);
        // Next and Previous go by chapter for a film.
        h.Model.Next(1_250_000);
        Assert.Equal(1_200_000, h.Engine.Seeks.Last());
        // More than 3 s into a chapter, Previous starts it again; less, it goes to the one before.
        h.Model.Previous(1_250_000);
        Assert.Equal(1_200_000, h.Engine.Seeks.Last());
        h.Model.Previous(1_202_000);
        Assert.Equal(600_000, h.Engine.Seeks.Last());

        h.Model.SetSleep(new SleepChoice.Minutes(30));
        var sleep = h.Model.Panel(PlayerPanelKind.Sleep);
        Assert.Equal(new[] { "Off", "15 min", "30 min", "45 min", "60 min", "90 min", "End of this episode" }, sleep.Rows.Select(r => r.Label));
        Assert.Equal((2, "30 min left"), (sleep.Focus, sleep.Rows[2].Supporting));
        Assert.False(h.Model.Choose(sleep.Rows[0]));
        Assert.Equal(new SleepChoice.Off(), h.State.Sleep.Set);
    }

    [Fact]
    public void A_dolby_vision_profile_7_file_on_a_drive_plays_as_it_is()
    {
        var video = PlayerHarness.Video with { DynamicRange = "dolby_vision", DvProfile = 7, ColorTransfer = "smpte2084" };
        h.Source.Local = true;
        h.Add(PlayerHarness.Movie() with { Files = new[] { PlayerHarness.FileOf(10) with { Streams = new[] { video, PlayerHarness.English } } } });
        h.Start(new PlayRequest(1, 0, true));
        // A server would be asked again for a remux; a drive has none, and refused it.
        Assert.Single(h.Source.Bodies);
        Assert.Single(h.Engine.Prepared);
        Assert.Equal(PhaseKind.Playing, h.State.Phase.Kind);
    }
}
