using System;
using System.Globalization;
using System.Linq;
using Pley.Tv.Core.Api;
using Pley.Tv.Core.Ui;
using Xunit;
using static Pley.Tv.Core.Tests.ItemKit;

namespace Pley.Tv.Core.Tests;

/// <summary>An item page's rules: the Android app's ItemPagesTest.</summary>
public class ItemPagesTests
{
    static readonly CultureInfo Us = new CultureInfo("en-US");

    static ItemPage Page(ItemDetail detail, Func<PageInput, PageInput>? input = null)
    {
        var start = new PageInput(detail, Origin) { Culture = Us };
        return ItemPages.Page(input == null ? start : input(start));
    }

    static ItemDetail Show(long id = 10, string title = "Driftline", int? leafCount = null, int? childCount = null) =>
        new ItemDetail { Id = id, LibraryId = 2, Kind = "show", Title = title, LeafCount = leafCount, ChildCount = childCount };

    [Fact]
    public void A_movie_not_started_plays_with_no_From_start()
    {
        var p = Page(Movie());
        Assert.Equal(PrimaryKind.Play, p.Primary.Kind);
        Assert.False(p.FromStart);
        Assert.Null(p.Primary.Episode);
    }

    [Fact]
    public void A_movie_in_progress_resumes_with_what_is_left_and_From_start()
    {
        var p = Page(Movie(positionMs: 3_480_000, durationMs: 9_060_000));
        Assert.Equal(PrimaryKind.Resume, p.Primary.Kind);
        Assert.True(p.FromStart);
        Assert.Equal("1h 33m left", p.Primary.TimeLeft);
        Assert.Equal(3_480_000f / 9_060_000f, p.Primary.Progress, 3);
    }

    [Fact]
    public void A_watched_movie_plays_again_rather_than_resuming()
    {
        var p = Page(Movie(positionMs: 3_480_000, watched: true));
        Assert.Equal(PrimaryKind.Play, p.Primary.Kind);
        Assert.True(p.Watched);
    }

    [Fact]
    public void A_movie_without_files_is_unavailable() =>
        Assert.Equal(PrimaryKind.Unavailable, Page(Movie(files: Array.Empty<MediaFile>())).Primary.Kind);

    [Fact]
    public void The_metadata_line_carries_the_year_runtime_rating_and_short_badges()
    {
        Assert.Equal(
            new MetaPart[]
            {
                new MetaPart.Text("2021"),
                new MetaPart.Text("2h 31m"),
                new MetaPart.ContentRating("PG-13"),
                new MetaPart.Rating("8.1"),
                new MetaPart.Badges(new[] { "4K", "7.1" }),
            },
            Page(Movie()).Meta);
    }

    [Fact]
    public void Tracks_use_the_webs_labels_and_without_a_pick_the_audio_is_automatic_in_its_language()
    {
        var tracks = Page(Movie()).Tracks!;
        Assert.Equal(new[] { "English (TrueHD 7.1)", "French (AC3 5.1)" }, tracks.Audio.Select(a => a.Label));
        // Each device plays its best English track: none is named.
        Assert.True(tracks.AutomaticAudio);
        Assert.Equal("English", tracks.AutomaticLanguage);
        Assert.Null(tracks.AudioIndex);
        Assert.Null(tracks.AudioLabel);
        Assert.DoesNotContain(tracks.Audio, a => a.Selected);
        Assert.Null(tracks.SubtitleIndex);
        Assert.Equal(new[] { "English (SRT)" }, tracks.Subtitles.Select(s => s.Label));
        Assert.Empty(tracks.Versions);
    }

    [Fact]
    public void A_picked_track_is_named_and_ticked_and_Automatic_picked_here_names_none()
    {
        var streams = new[] { Stream(1, "audio", "truehd", "eng", isDefault: true, channels: 8), Stream(2, "audio", "ac3", "fra", channels: 6) };
        var picked = Movie(files: new[] { File(11, streams, selectedAudio: 2, audioPicked: true) });
        var tracks = Page(picked).Tracks!;
        Assert.False(tracks.AutomaticAudio);
        Assert.Equal(2, tracks.AudioIndex);
        Assert.Equal("French (AC3 5.1)", tracks.AudioLabel);
        var automatic = Page(picked, i => i with { Pick = new StreamPick(AudioFile: 11, Audio: null) }).Tracks!;
        Assert.True(automatic.AutomaticAudio);
        Assert.Null(automatic.AudioIndex);
    }

    [Fact]
    public void The_profiles_saved_choice_wins_and_a_pick_for_another_version_doesnt_apply()
    {
        var withChoice = Movie(files: new[]
        {
            File(11, new[] { Stream(1, "audio", "aac", "eng", isDefault: true), Stream(2, "audio", "ac3", "fra"), Stream(3, "subtitle", "subrip", "eng") },
                selectedAudio: 2, selectedSubtitle: 3, audioPicked: true),
            File(12, new[] { Stream(1, "audio", "aac", "eng", isDefault: true), Stream(2, "audio", "ac3", "deu") }),
        });
        var first = Page(withChoice).Tracks!;
        Assert.Equal(2, first.AudioIndex);
        Assert.Equal(3, first.SubtitleIndex);
        Assert.Equal(2, first.Versions.Count);
        var other = Page(withChoice, i => i with { Pick = new StreamPick(FileId: 12, AudioFile: 11, Audio: 1) }).Tracks!;
        Assert.Equal(12L, other.VersionId);
        // The second version has no saved choice: automatic, in its default track's language.
        Assert.Null(other.AudioIndex);
        Assert.True(other.AutomaticAudio);
        Assert.Equal("English", other.AutomaticLanguage);
    }

    [Fact]
    public void Nothing_to_choose_gives_no_selects()
    {
        var single = Movie(files: new[] { File(11, new[] { Stream(0, "video", "h264"), Stream(1, "audio", "aac", "eng", isDefault: true) }) });
        Assert.Null(Page(single).Tracks);
    }

    [Fact]
    public void A_show_plays_its_next_episode_labelled_with_it()
    {
        var show = Show(leafCount: 24, childCount: 3);
        var next = Summary(21, "episode", parentId: 30, parentIndex: 1, index: 3);
        var p = Page(show, i => i with { OnDeck = next });
        Assert.Equal(PrimaryKind.Play, p.Primary.Kind);
        Assert.Equal("S1 · E3", p.Primary.Episode);
        var resuming = Page(show, i => i with { OnDeck = next with { UserState = new UserState { PositionMs = 1_000 } } });
        Assert.Equal(PrimaryKind.Resume, resuming.Primary.Kind);
        Assert.True(resuming.FromStart);
    }

    [Fact]
    public void A_show_without_episodes_is_unavailable() =>
        Assert.Equal(PrimaryKind.Unavailable, Page(Show(title: "Empty", leafCount: 0)).Primary.Kind);

    [Fact]
    public void A_shows_meta_line_counts_seasons_and_spans_their_years()
    {
        var show = Show(childCount: 3) with { Year = 2021, ContentRating = "TV-14" };
        var seasons = new[] { 2021, 2023, 2025 }.Select((y, i) => Summary(30 + i, "season", index: i + 1) with { Year = y }).ToList();
        Assert.Equal(
            new MetaPart[] { new MetaPart.Text("3 seasons"), new MetaPart.Text("2021-2025"), new MetaPart.ContentRating("TV-14") },
            Page(show, i => i with { Seasons = seasons }).Meta);
    }

    [Fact]
    public void Episodes_get_Guest_Stars_their_show_and_season_and_the_seasons_row()
    {
        var episode = new ItemDetail
        {
            Id = 21, LibraryId = 2, Kind = "episode", Title = "Glasshouse", Index = 5, ParentId = 30, ParentIndex = 2,
            GrandparentId = 10, GrandparentTitle = "Driftline",
        };
        var p = Page(episode, i => i with { SelectedSeason = 30, Episodes = new[] { Summary(21, "episode", index: 5, parentIndex: 2) } });
        Assert.Equal(CastTitle.GuestStars, p.CastTitle);
        Assert.Equal("Driftline", p.Context?.ShowTitle);
        Assert.Equal("S2 · E5", p.Context?.EpisodeLabel);
        Assert.Equal("Season 2 · Episode 5", p.Context?.EpisodeLongLabel);
        Assert.Equal(21L, p.Seasons?.Focus);
        Assert.Equal("5. Item 21", Assert.Single(p.Seasons!.Episodes!).Title);
    }

    [Fact]
    public void Crew_lines_stop_at_four_names_then_say_how_many_more()
    {
        var writers = Enumerable.Range(1, 6).Select(i => new Credit { Id = i, Name = $"Writer {i}", Role = "writer" });
        var p = Page(Movie() with { Crew = writers.Append(new Credit { Id = 9, Name = "Producer", Role = "producer" }).ToList() });
        var line = Assert.Single(p.Details.Crew);
        Assert.Equal("writer", line.Role);
        Assert.Equal(4, line.People.Count);
        Assert.Equal(2, line.More);
        Assert.True(line.Plural);
    }

    [Fact]
    public void Media_info_shows_file_names_only()
    {
        var f = File(11, new[] { new MediaStream { Index = 0, Kind = "video", Codec = "hevc" } }) with { Path = "/srv/media/Movies/Saltwind (2021)/Saltwind.mkv" };
        var info = Assert.Single(Page(Movie(files: new[] { f })).MediaInfo);
        Assert.Equal(new InfoRow(InfoLabel.File, "Saltwind.mkv"), info.Rows[0]);
        Assert.Equal("Saltwind.mkv", ItemPages.FileName(@"C:\media\Saltwind.mkv"));
    }

    [Fact]
    public void The_cast_lists_each_person_once()
    {
        var cast = new[]
        {
            new Credit { Id = 1, Name = "A", Role = "actor", Character = "One" },
            new Credit { Id = 1, Name = "A", Role = "actor", Character = "Two" },
            new Credit { Id = 2, Name = "B", Role = "actor" },
        };
        Assert.Equal(new[] { 1L, 2L }, Page(Movie() with { Cast = cast }).Cast.Select(c => c.Id));
    }

    [Fact]
    public void A_show_opens_on_its_next_episodes_season_else_the_first_that_isnt_Specials()
    {
        var seasons = new[] { Summary(30, "season", index: 0), Summary(31, "season", index: 1), Summary(32, "season", index: 2) };
        Assert.Equal(32L, ItemPages.DefaultSeason(seasons, Summary(40, "episode", parentId: 32)));
        Assert.Equal(31L, ItemPages.DefaultSeason(seasons, null));
    }

    [Fact]
    public void Extras_say_what_they_are_and_how_long()
    {
        var trailer = Summary(5, "extra", durationMs: 120_000) with { ExtraType = "trailer" };
        Assert.Equal("Trailer · 2m", ItemPages.ExtraSubtitle(trailer));
        Assert.Equal("Extra", ItemPages.ExtraSubtitle(Summary(6, "extra", durationMs: null)));
    }

    // ---- beyond the Android app's tests: the labels its other tests pin

    [Fact]
    public void Versions_and_streams_read_as_the_webs()
    {
        var dv = new MediaStream
        {
            Index = 0, Kind = "video", Codec = "hevc", Width = 3840, Height = 2160, FrameRate = 23.976, BitDepth = 10,
            DynamicRange = "dolby_vision", DvProfile = 8, DvBlCompat = 1, Profile = "Main 10",
        };
        var atmos = new MediaStream { Index = 1, Kind = "audio", Codec = "truehd", Language = "eng", Default = true, Channels = 8, Profile = "Dolby TrueHD + Dolby Atmos" };
        var file = File(11, new[] { dv, atmos }) with { Size = 66_571_993_088 };
        Assert.Equal("4K · HEVC · Dolby Vision P8.1 · TrueHD Atmos 7.1 · 62.0 GB", ItemFormat.VersionDescription(file));
        Assert.Equal("4K · HEVC · MKV · 62.0 GB", ItemFormat.VersionLabel(file, 0));
        Assert.Equal(new[] { "4K", "DV P8.1", "Atmos" }, ItemFormat.QualityBadges(file));
        var info = Page(Movie(files: new[] { file })).MediaInfo.Single();
        Assert.Equal("4K · 3840×2160 · 23.98 fps · 10-bit · Dolby Vision P8.1 · Main 10", info.Streams[0].Details);
        Assert.Equal("English – Commentary (AAC Stereo)", ItemFormat.StreamLabel(new MediaStream { Kind = "audio", Codec = "aac", Language = "eng", Title = "Commentary", Channels = 2 }));
        Assert.Equal("French (SRT, Forced)", ItemFormat.StreamLabel(new MediaStream { Kind = "subtitle", Codec = "subrip", Language = "fre", Title = "French (Forced)" }));
        Assert.Equal("Portuguese (BR)", ItemFormat.LanguageName("pt-BR"));
    }

    [Fact]
    public void Dates_follow_the_order_of_the_language()
    {
        Assert.Equal("Apr 1, 2023", ItemFormat.Date("2023-04-01", Us));
        Assert.Equal("March 12, 2024", ItemFormat.LongDate("2024-03-12", Us));
        Assert.Equal("12 March 2024", ItemFormat.LongDate("2024-03-12", new CultureInfo("en-GB")));
        Assert.Equal("soon", ItemFormat.Date("soon", Us));
    }

    [Fact]
    public void A_season_waits_for_its_episodes_and_names_itself()
    {
        var season = new ItemDetail { Id = 31, LibraryId = 2, Kind = "season", Title = "", Index = 0, ParentId = 10, ParentTitle = "Driftline", LeafCount = 2 };
        var p = Page(season, i => i with { SelectedSeason = 31 });
        Assert.Equal("Specials", p.Title);
        Assert.Equal(PrimaryKind.Play, p.Primary.Kind);
        Assert.Null(p.Seasons!.Episodes);
        Assert.Equal(PrimaryKind.Unavailable, Page(season, i => i with { SelectedSeason = 31, Episodes = Array.Empty<ItemSummary>() }).Primary.Kind);
    }
}
