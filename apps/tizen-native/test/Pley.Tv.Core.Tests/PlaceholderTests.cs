using System;
using System.Collections.Generic;
using System.Globalization;
using System.IO;
using System.Linq;
using System.Text.Json;
using System.Xml.Linq;
using Pley.Tv.Core.Api;
using Pley.Tv.Core.Ui;
using Xunit;

namespace Pley.Tv.Core.Tests;

/// <summary>The artwork placeholder's colors (the Android app's PlaceholderTest) and what it draws on them.</summary>
public class PlaceholderTests
{
    /// <summary>
    /// packages/placeholder's examples, which every app's gradient is held to: each title's hue, and
    /// its two colors as CSS makes them, within a step of a byte.
    /// </summary>
    [Fact]
    public void Colors_are_the_placeholder_files_examples()
    {
        using var file = JsonDocument.Parse(File.ReadAllText(Path.Combine(AppContext.BaseDirectory, "placeholder", "placeholder.json")));
        // Gradients.Diagonal draws CSS's 135°: another angle needs it drawn again.
        Assert.Equal(135, file.RootElement.GetProperty("gradient").GetProperty("angle").GetInt32());
        var examples = file.RootElement.GetProperty("examples").EnumerateArray().ToList();
        Assert.NotEmpty(examples);
        static bool Near(uint a, uint b) => new[] { 16, 8, 0 }.All(shift => Math.Abs((int)((a >> shift) & 0xFF) - (int)((b >> shift) & 0xFF)) <= 1);
        static uint Hex(JsonElement e) => uint.Parse(e.GetString()!.Substring(1), NumberStyles.HexNumber, CultureInfo.InvariantCulture);
        foreach (var example in examples)
        {
            var seed = example.GetProperty("seed").GetString()!;
            Assert.Equal(example.GetProperty("hue").GetInt32(), Placeholder.Hue(seed));
            var (from, to) = Placeholder.Colors(seed);
            Assert.True(Near(Hex(example.GetProperty("from")), from) && Near(Hex(example.GetProperty("to")), to), $"{seed}: {from:x6} {to:x6}");
        }
    }

    [Fact]
    public void Colours_are_the_webs_hsl()
    {
        // "" → hsl(0 30% 22%) to hsl(40 35% 12%), as the web's test pins.
        var (from, to) = Placeholder.Colors("");
        Assert.Equal(0x492727u, from);
        Assert.Equal(0x292214u, to);
        Assert.NotEqual(Placeholder.Colors("Heat"), Placeholder.Colors("Alien"));
        Assert.Equal(Placeholder.Colors("Heat"), Placeholder.Colors("Heat"));
    }

    [Fact]
    public void Hsl_matches_css()
    {
        Assert.Equal(0xFF0000u, Placeholder.Hsl(0, 1, 0.5f));
        Assert.Equal(0x00FF00u, Placeholder.Hsl(120, 1, 0.5f));
        Assert.Equal(0x0000FFu, Placeholder.Hsl(240, 1, 0.5f));
    }

    [Fact]
    public void A_tile_that_is_no_titles_keeps_its_words()
    {
        // Discover's Show more: no picture, no kind. Its words stay on its well, with no mark.
        var more = new TileModel(-1, "Show more", "", Array.Empty<string>(), 0f, false, 0, Array.Empty<string>(), "Show more");
        Assert.Equal("Show more", Placeholder.Words(more, TileKind.Poster));
        Assert.False(Placeholder.Tile(more, TileKind.Poster).Mark);
        // A title's tile has its caption; a person's round one shows its picture's circle alone.
        Assert.Null(Placeholder.Words(Tile(), TileKind.Poster));
        Assert.Null(Placeholder.Words(Tile(kind: null), TileKind.Person));
    }

    static TileModel Tile(string? kind = "movie", string? image = null) =>
        new TileModel(1, "Northern Lights", "2021", image == null ? new string[0] : new[] { image }, 0, false, 0, new string[0], "", kind);

    [Fact]
    public void A_tile_without_a_picture_shows_Pleys_mark_with_its_librarys_glyph_and_not_its_title()
    {
        // The caption under the tile names it: the placeholder doesn't say it again.
        var look = Placeholder.Tile(Tile(), TileKind.Poster);
        Assert.True(look.Mark);
        Assert.Equal("movies", look.Glyph);
        Assert.False(look.Title);
        // 40 % of the shorter side, centered.
        Assert.Equal(108 * 0.4f, look.MarkSide, 3);
        Assert.Equal((162 - 108 * 0.4f) / 2, look.MarkTop, 3);
        Assert.Equal("shows", Placeholder.Tile(Tile("episode"), TileKind.Episode).Glyph);
        Assert.Equal("music", Placeholder.Tile(Tile("album"), TileKind.Square).Glyph);
        Assert.Equal("play", Placeholder.Tile(Tile("collection"), TileKind.Poster).Glyph);
    }

    [Fact]
    public void A_tile_without_a_kind_shows_no_mark()
    {
        // People's circles: the gradient alone.
        var look = Placeholder.Tile(Tile(kind: null), TileKind.Person);
        Assert.False(look.Mark);
        Assert.Null(look.Glyph);
        Assert.False(look.Title);
    }

    [Fact]
    public void A_tile_whose_picture_loads_shows_neither_the_mark_nor_its_title()
    {
        // The gradient alone: a page of tiles scrolling in doesn't flash marks before their pictures.
        var look = Placeholder.Tile(Tile(image: "/api/items/1/images/poster"), TileKind.Poster);
        Assert.False(look.Mark);
        Assert.False(look.Title);
    }

    [Fact]
    public void A_tile_whose_picture_failed_shows_the_mark()
    {
        var look = Placeholder.Tile(Tile(image: "/api/items/1/images/poster"), TileKind.Landscape, failed: true);
        Assert.True(look.Mark);
        Assert.Equal("movies", look.Glyph);
        Assert.False(look.Title);
        Assert.Equal(115 * 0.4f, look.MarkSide, 3);
    }

    [Fact]
    public void The_mark_is_at_most_120_dp()
    {
        Assert.Equal(120, Placeholder.Layout(960, 540, missing: true, "play").MarkSide);
    }

    [Fact]
    public void Over_a_title_the_mark_is_smaller_above_it_and_left_out_without_room()
    {
        // A playlist's cover keeps its title: two lines of 22 dp leave the mark room above them.
        var two = Placeholder.Layout(132, 132, missing: true, "music", titleHeight: 44);
        Assert.True(two.Title);
        Assert.Equal("music", two.Glyph);
        Assert.Equal(132 * 0.26f, two.MarkSide, 3);
        var top = (132 - 132 * 0.26f - 10 - 44) / 2;
        Assert.Equal(top, two.MarkTop, 3);
        Assert.Equal(top + 132 * 0.26f + 10, two.TitleTop, 3);
        // Four lines leave it none: the title alone, centered.
        var four = Placeholder.Layout(132, 132, missing: true, "music", titleHeight: 88);
        Assert.False(four.Mark);
        Assert.Null(four.Glyph);
        Assert.True(four.Title);
        Assert.Equal(22, four.TitleTop, 3);
        // While its picture loads, or without a glyph, the title alone.
        var loading = Placeholder.Layout(132, 132, missing: false, "music", titleHeight: 44);
        Assert.False(loading.Mark);
        Assert.Equal(44, loading.TitleTop, 3);
        Assert.False(Placeholder.Layout(132, 132, missing: true, null, titleHeight: 44).Mark);
    }

    [Fact]
    public void Tiles_carry_their_items_kind_and_playlist_covers_their_own_glyph()
    {
        var album = new ItemSummary { Id = 7, Kind = "album", Title = "Tidewater" };
        Assert.Equal("album", TileModel.Of(album, TileKind.Square, "https://pley.example.com").ItemKind);
        Assert.Equal("music", TileModel.Of(album, TileKind.Square, "https://pley.example.com").MarkGlyph);
        Assert.Null(Tile(kind: null).MarkGlyph);
        Assert.Equal("music", new PlaylistCard(1, "Late", "", Video: false, false, new string[0]).MarkGlyph);
        Assert.Equal("play", new PlaylistCard(2, "Saturday", "", Video: true, false, new string[0]).MarkGlyph);
    }

    /// <summary>packages/placeholder/placeholder.json: the mark every app draws.</summary>
    static JsonElement Marks()
    {
        using var file = JsonDocument.Parse(File.ReadAllText(Path.Combine(AppContext.BaseDirectory, "placeholder", "placeholder.json")));
        return file.RootElement.Clone();
    }

    static Dictionary<string, string> Map(JsonElement map) => map.EnumerateObject().ToDictionary(p => p.Name, p => p.Value.GetString()!);

    [Fact]
    public void Kinds_take_the_glyphs_the_placeholder_file_gives_them()
    {
        var marks = Marks();
        var kinds = Map(marks.GetProperty("kinds"));
        Assert.Equal(kinds.OrderBy(k => k.Key), Placeholder.Kinds.OrderBy(k => k.Key));
        foreach (var (kind, glyph) in kinds) Assert.Equal(glyph, Placeholder.Glyph(kind));
        Assert.Equal(Placeholder.DefaultGlyph, marks.GetProperty("default").GetString());
        Assert.Equal(Placeholder.DefaultGlyph, Placeholder.Glyph("extra"));
        Assert.Equal(Placeholder.DefaultGlyph, Placeholder.Glyph(""));
        Assert.Equal(Placeholder.DefaultGlyph, Placeholder.Glyph("Movie"));
        // Every glyph a kind takes is one placeholder.json draws, so the app has its picture.
        var glyphs = marks.GetProperty("glyphs").EnumerateObject().Select(p => p.Name).ToList();
        Assert.Contains(Placeholder.DefaultGlyph, glyphs);
        foreach (var glyph in kinds.Values) Assert.Contains(glyph, glyphs);
    }

    [Fact]
    public void The_marks_sizes_are_the_placeholder_files()
    {
        var marks = Marks();
        Assert.Equal(Placeholder.MarkShare, marks.GetProperty("share").GetSingle());
        Assert.Equal(Placeholder.MarkShareOverTitle, marks.GetProperty("shareOverTitle").GetSingle());
        Assert.Equal(Placeholder.MarkMax, marks.GetProperty("max").GetSingle());
        Assert.Equal(Placeholder.MarkGap, marks.GetProperty("gap").GetSingle());
    }

    /// <summary>
    /// The app draws the mark from an SVG for each glyph (scripts/render-placeholder-marks.mjs):
    /// placeholder.json's posters, white at their alphas, the glyph cut out of the front one, even-odd.
    /// </summary>
    [Fact]
    public void Each_glyphs_svg_is_the_placeholder_files()
    {
        var marks = Marks();
        var grid = marks.GetProperty("grid").GetInt32();
        var posters = marks.GetProperty("posters").EnumerateArray().ToList();
        foreach (var (glyph, cut) in Map(marks.GetProperty("glyphs")))
        {
            var svg = XDocument.Load(Path.Combine(AppContext.BaseDirectory, "res", $"placeholder-mark-{glyph}.svg")).Root!;
            Assert.Equal($"0 0 {grid} {grid}", (string?)svg.Attribute("viewBox"));
            var paths = svg.Descendants().Where(e => e.Name.LocalName == "path").ToList();
            Assert.Equal(posters.Count, paths.Count);
            for (var i = 0; i < paths.Count; i++)
            {
                var front = i == paths.Count - 1;
                var poster = posters[i];
                Assert.Equal(poster.GetProperty("path").GetString() + (front ? cut : ""), (string?)paths[i].Attribute("d"));
                Assert.Equal("#ffffff", (string?)paths[i].Attribute("fill"));
                Assert.Equal(poster.GetProperty("alpha").GetSingle(), float.Parse((string)paths[i].Attribute("fill-opacity")!, CultureInfo.InvariantCulture), 3);
                Assert.Equal(front ? "evenodd" : null, (string?)paths[i].Attribute("fill-rule"));
            }
        }
    }
}
