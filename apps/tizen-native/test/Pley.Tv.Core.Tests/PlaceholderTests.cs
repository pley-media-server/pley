using System.Collections.Generic;
using Pley.Tv.Core.Ui;
using Xunit;

namespace Pley.Tv.Core.Tests;

/// <summary>The artwork placeholder's colours: the Android app's PlaceholderTest.</summary>
public class PlaceholderTests
{
    /// <summary>Hues from apps/web/src/lib/images.ts's placeholderGradient, run in Node for these titles.</summary>
    [Fact]
    public void Hues_are_the_webs_for_the_same_titles()
    {
        var web = new Dictionary<string, int>
        {
            ["Her"] = 77, ["Airport"] = 123, [""] = 0, ["Glasshouse"] = 124, ["Northern Lights"] = 121,
            ["Ashfall"] = 265, ["A Film With No Poster Yet"] = 71, ["Chéri"] = 323, ["千と千尋の神隠し"] = 156,
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
        Assert.NotEqual(Placeholder.Colors("Her"), Placeholder.Colors("Airport"));
        Assert.Equal(Placeholder.Colors("Her"), Placeholder.Colors("Her"));
    }

    [Fact]
    public void Hsl_matches_css()
    {
        Assert.Equal(0xFF0000u, Placeholder.Hsl(0, 1, 0.5f));
        Assert.Equal(0x00FF00u, Placeholder.Hsl(120, 1, 0.5f));
        Assert.Equal(0x0000FFu, Placeholder.Hsl(240, 1, 0.5f));
    }
}
