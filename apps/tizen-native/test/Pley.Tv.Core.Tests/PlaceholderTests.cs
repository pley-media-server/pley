using System;
using System.Collections.Generic;
using System.Globalization;
using System.IO;
using System.Linq;
using System.Xml.Linq;
using Pley.Tv.Core.Ui;
using Xunit;

namespace Pley.Tv.Core.Tests;

/// <summary>The artwork placeholder's colors (the Android app's PlaceholderTest) and what it draws on them.</summary>
public class PlaceholderTests
{
    /// <summary>Hues from apps/web/src/lib/images.ts's placeholderGradient, run in Node for these titles.</summary>
    [Fact]
    public void Hues_are_the_webs_for_the_same_titles()
    {
        var web = new Dictionary<string, int>
        {
            ["Heat"] = 176, ["Alien"] = 159, [""] = 0, ["Glasshouse"] = 124, ["Northern Lights"] = 121,
            ["Ashfall"] = 265, ["A Film With No Poster Yet"] = 71, ["Amélie"] = 331, ["千と千尋の神隠し"] = 156,
            ["The Quiet Harbor"] = 303,
        };
        foreach (var (title, hue) in web) Assert.Equal(hue, Placeholder.Hue(title));
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

    static TileModel Tile(params string[] images) => new TileModel(1, "Northern Lights", "2021", images, 0, false, 0, new string[0], "");

    [Fact]
    public void A_tile_without_a_picture_shows_Pleys_mark_and_not_its_title()
    {
        // The caption under the tile names it: the placeholder doesn't say it again.
        var look = Placeholder.Tile(Tile(), TileKind.Poster);
        Assert.True(look.Mark);
        Assert.False(look.Title);
        // 40 % of the shorter side, centered.
        Assert.Equal(108 * 0.4f, look.MarkSide, 3);
        Assert.Equal((162 - 108 * 0.4f) / 2, look.MarkTop, 3);
    }

    [Fact]
    public void A_tile_whose_picture_loads_shows_neither_the_mark_nor_its_title()
    {
        // The gradient alone: a page of tiles scrolling in doesn't flash marks before their pictures.
        var look = Placeholder.Tile(Tile("/api/items/1/images/poster"), TileKind.Poster);
        Assert.False(look.Mark);
        Assert.False(look.Title);
    }

    [Fact]
    public void A_tile_whose_picture_failed_shows_the_mark()
    {
        var look = Placeholder.Tile(Tile("/api/items/1/images/poster"), TileKind.Landscape, failed: true);
        Assert.True(look.Mark);
        Assert.False(look.Title);
        Assert.Equal(115 * 0.4f, look.MarkSide, 3);
    }

    [Fact]
    public void The_mark_is_at_most_120_dp()
    {
        Assert.Equal(120, Placeholder.Layout(960, 540, missing: true).MarkSide);
    }

    [Fact]
    public void Over_a_title_the_mark_is_smaller_above_it_and_left_out_without_room()
    {
        // A playlist's cover keeps its title: two lines of 22 dp leave the mark room above them.
        var two = Placeholder.Layout(132, 132, missing: true, titleHeight: 44);
        Assert.True(two.Title);
        Assert.Equal(132 * 0.26f, two.MarkSide, 3);
        var top = (132 - 132 * 0.26f - 10 - 44) / 2;
        Assert.Equal(top, two.MarkTop, 3);
        Assert.Equal(top + 132 * 0.26f + 10, two.TitleTop, 3);
        // Four lines leave it none: the title alone, centered.
        var four = Placeholder.Layout(132, 132, missing: true, titleHeight: 88);
        Assert.False(four.Mark);
        Assert.True(four.Title);
        Assert.Equal(22, four.TitleTop, 3);
        // While its picture loads, the title alone.
        var loading = Placeholder.Layout(132, 132, missing: false, titleHeight: 44);
        Assert.False(loading.Mark);
        Assert.Equal(44, loading.TitleTop, 3);
    }

    /// <summary>The app draws the mark from its SVG: the logo's paths, white at the layers' alphas.</summary>
    [Fact]
    public void The_marks_svg_is_the_logos_posters_at_the_layers_alphas()
    {
        static List<XElement> Paths(string file) =>
            XDocument.Load(Path.Combine(AppContext.BaseDirectory, "res", file)).Descendants().Where(e => e.Name.LocalName == "path").ToList();
        var logo = Paths("pley-mark.svg");
        var mark = Paths("pley-mark-placeholder.svg");
        Assert.Equal(Placeholder.MarkLayers.Count, mark.Count);
        // The logo's last three paths are its posters, after its tile.
        Assert.Equal(logo.Skip(logo.Count - 3).Select(p => (string?)p.Attribute("d")), mark.Select(p => (string?)p.Attribute("d")));
        for (var i = 0; i < mark.Count; i++)
        {
            var layer = Placeholder.MarkLayers[i];
            Assert.Equal(layer.Path, (string?)mark[i].Attribute("d"));
            Assert.Equal("#ffffff", (string?)mark[i].Attribute("fill"));
            Assert.Equal(layer.Alpha, float.Parse((string)mark[i].Attribute("fill-opacity")!, CultureInfo.InvariantCulture), 3);
            Assert.Equal(layer.EvenOdd ? "evenodd" : null, (string?)mark[i].Attribute("fill-rule"));
        }
    }
}
