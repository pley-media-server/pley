using System.Globalization;
using System.Linq;
using Pley.Tv.Core.Api;
using Pley.Tv.Core.Ui;
using Xunit;
using static Pley.Tv.Core.Tests.ItemKit;

namespace Pley.Tv.Core.Tests;

/// <summary>What the TV's item page says: the Android app's TV item composables, as words.</summary>
public class ItemTextTests
{
    static ItemPage Page(ItemDetail detail) => ItemPages.Page(new PageInput(detail, Origin) { Culture = new CultureInfo("en-US") });

    [Fact]
    public void The_main_button_says_what_it_plays()
    {
        Assert.Equal("Resume S2 · E5", ItemText.PrimaryLabel(new PrimaryAction(PrimaryKind.Resume, "S2 · E5")));
        Assert.Equal("Resume", ItemText.PrimaryLabel(new PrimaryAction(PrimaryKind.Resume)));
        Assert.Equal("Play S1 · E3", ItemText.PrimaryLabel(new PrimaryAction(PrimaryKind.Play, "S1 · E3")));
        Assert.Equal("Play", ItemText.PrimaryLabel(new PrimaryAction(PrimaryKind.Play)));
        Assert.Equal("Unavailable", ItemText.PrimaryLabel(new PrimaryAction(PrimaryKind.Unavailable)));
    }

    [Fact]
    public void The_hero_has_its_buttons_in_order_and_starts_on_the_one_that_plays()
    {
        var trailer = Summary(5, "extra") with { ExtraType = "trailer" };
        var page = Page(Movie(positionMs: 60_000) with { Extras = new[] { trailer } });
        Assert.Equal(
            new[] { "Resume", "From start", "Mark as watched", "Audio and subtitles", "Add to My List", "Trailer", "Add to playlist", "More" },
            ItemText.Buttons(page).Select(b => b.Label));
        Assert.Equal(HeroButtonKind.Primary, ItemText.FirstFocus(page));
        var nothing = Page(Movie(files: new MediaFile[0]));
        Assert.False(ItemText.Buttons(nothing)[0].Enabled);
        Assert.Equal(HeroButtonKind.Watched, ItemText.FirstFocus(nothing));
    }

    [Fact]
    public void An_episodes_hero_names_its_show_and_the_credits_name_who_made_it()
    {
        var episode = Page(new ItemDetail
        {
            Id = 21, LibraryId = 2, Kind = "episode", Title = "Glasshouse", Index = 5, ParentIndex = 2, GrandparentId = 10, GrandparentTitle = "Driftline",
            Crew = new[] { new Credit { Id = 7, Name = "Lena Okafor", Role = "director" } },
            Cast = new[] { new Credit { Id = 8, Name = "Idris Vale", Role = "actor", Character = "Tom" } },
        });
        Assert.Equal("Driftline", ItemText.HeroTitle(episode));
        Assert.Equal("S2 · E5 · Glasshouse", ItemText.ContextLine(episode));
        Assert.Equal(new CreditsLine("Directed by", "Lena Okafor", "Idris Vale"), ItemText.Credits(episode));
        Assert.Equal("Guest Stars & Crew", ItemText.CastRowTitle(episode));
        Assert.Equal(new[] { ("Idris Vale", "Tom"), ("Lena Okafor", "Director") }, ItemText.CastAndCrew(episode).Select(t => (t.Name, t.Subtitle)));
        Assert.Equal(new[] { ("Director", "Lena Okafor"), ("Network", "HBO") }, ItemText.Facts(Page(new ItemDetail
        {
            Id = 21, LibraryId = 2, Kind = "episode", Title = "Glasshouse", Studio = "HBO",
            Crew = new[] { new Credit { Id = 7, Name = "Lena Okafor", Role = "director" } },
        }).Details));
    }

    [Fact]
    public void The_tracks_panel_starts_on_what_plays()
    {
        var panel = ItemText.TracksPanel(Page(Movie()))!;
        Assert.Equal(
            new[] { "Audio", "Automatic (English)", "English (TrueHD 7.1)", "French (AC3 5.1)", "Subtitles", "Off", "English (SRT)" },
            panel.Rows.Select(r => r.Label));
        Assert.Equal("Automatic (English)", panel.Rows[panel.Focus].Label);
        Assert.Null(panel.Rows[panel.Focus].Value);
    }

    [Fact]
    public void More_offers_the_rating_with_half_stars_rounding_up_then_the_show()
    {
        var episode = new ItemDetail { Id = 21, LibraryId = 2, Kind = "episode", Title = "Glasshouse", ParentId = 30, GrandparentId = 10, UserState = new UserState { Rating = 7 } };
        var panel = ItemText.MorePanel(Page(episode), summaryCut: false);
        Assert.Equal(new[] { "Your rating", "No rating", "1 star", "2 stars", "3 stars", "4 stars", "5 stars", "Go to show", "Go to season" }, panel.Rows.Select(r => r.Label));
        Assert.Equal("4 stars", panel.Rows[panel.Focus].Label);
        Assert.Equal(new PageKey.Item(30), panel.Rows.Last().Key);
        // A season takes no rating and has no show to go to here: Add to playlist, then.
        var season = new ItemDetail { Id = 30, LibraryId = 2, Kind = "season", Title = "Season 1" };
        var only = Assert.Single(ItemText.MorePanel(Page(season), summaryCut: true).Rows, r => r.Kind == PanelRowKind.Open);
        Assert.Equal(new AddToPlaylistPage(30), only.Key);
    }

    [Fact]
    public void Media_info_in_full_names_every_fact_known()
    {
        var video = new MediaStream
        {
            Index = 0, Kind = "video", Codec = "hevc", Width = 3840, Height = 2160, FrameRate = 23.976, BitDepth = 10, PixFmt = "yuv420p10le",
            DynamicRange = "dolby_vision", DvProfile = 8, DvBlCompat = 1, DvLevel = 6, Profile = "Main 10", Level = 153,
            ColorPrimaries = "bt2020", ColorTransfer = "smpte2084", ColorSpace = "bt2020nc",
        };
        var audio = new MediaStream { Index = 1, Kind = "audio", Codec = "eac3", Language = "eng", Channels = 6, ChannelLayout = "5.1(side)", SampleRate = 48000, Default = true };
        var subtitle = new MediaStream { Index = 2, Kind = "subtitle", Codec = "hdmv_pgs_subtitle", Language = "fre", TextBased = false, Forced = true };
        var blocks = MediaFacts.Blocks(Page(Movie(files: new[] { File(11, new[] { subtitle, audio, video }) })).MediaInfo.Single());
        Assert.Equal(new[] { "File", "Video", "Audio", "Subtitles" }, blocks.Select(b => b.Title));
        Assert.Equal(
            new[]
            {
                ("Codec", "HEVC"), ("Profile", "Main 10 · Level 5.1"), ("Resolution", "3840×2160 (4K)"), ("Frame rate", "23.976 fps"),
                ("Bit depth", "10-bit · 4:2:0"), ("HDR", "Dolby Vision P8.1"), ("Dolby Vision", "Profile 8.1 · Level 6 · HDR10-compatible base layer"),
                ("Primaries", "BT.2020"), ("Transfer", "PQ (SMPTE ST 2084)"), ("Matrix", "BT.2020 non-constant"),
            },
            blocks[1].Facts.Select(f => (f.Label, f.Value)));
        Assert.Equal(new[] { ("Codec", "EAC3"), ("Channels", "5.1 (5.1(side))"), ("Sample rate", "48 kHz"), ("Language", "English"), ("Flags", "Default") },
            blocks[2].Facts.Select(f => (f.Label, f.Value)));
        Assert.Equal(new[] { ("Format", "PGS, picture"), ("Language", "French"), ("Flags", "Forced") }, blocks[3].Facts.Select(f => (f.Label, f.Value)));
    }
}
