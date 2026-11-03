using Pley.Tv.Core.Ui;
using Xunit;

namespace Pley.Tv.Core.Tests;

/// <summary>How much of a TV row is made as focus moves along it: search's rows hold up to 200.</summary>
public class RowReachTests
{
    [Fact]
    public void A_screen_holds_8_posters_9_people_and_5_channels()
    {
        Assert.Equal(8, RowReach.PerScreen(TileKind.Poster));
        Assert.Equal(9, RowReach.PerScreen(TileKind.Person));
        // Search's Live TV cards are episode-sized.
        Assert.Equal(5, RowReach.PerScreen(TileKind.Episode));
    }

    [Fact]
    public void A_row_makes_its_screen_and_a_few_past_it_at_once_then_a_screen_more()
    {
        // At the row's start: the first poster, the eight a screen holds after it, and three whose pictures come ahead.
        Assert.Equal(12, RowReach.Now(0, 8, 200));
        Assert.Equal(20, RowReach.Soon(0, 8, 200));
    }

    [Fact]
    public void A_row_of_200_makes_what_one_of_20_did()
    {
        Assert.Equal(RowReach.Soon(0, 8, 20), RowReach.Soon(0, 8, 200));
        Assert.Equal(20, RowReach.Soon(0, 8, 20));
    }

    [Fact]
    public void The_reach_moves_on_a_tile_at_a_time_with_focus()
    {
        // Focus on the 31st poster: the 30th is cut off at the gutter's left.
        Assert.Equal(41, RowReach.Now(29, 8, 200));
        Assert.Equal(42, RowReach.Now(30, 8, 200));
        Assert.Equal(50, RowReach.Soon(30, 8, 200));
    }

    [Fact]
    public void Never_past_the_rows_end()
    {
        Assert.Equal(200, RowReach.Now(195, 8, 200));
        Assert.Equal(200, RowReach.Soon(190, 8, 200));
        Assert.Equal(3, RowReach.Now(0, 5, 3));
        Assert.Equal(3, RowReach.Soon(0, 5, 3));
        Assert.Equal(0, RowReach.Soon(0, 8, 0));
    }
}
