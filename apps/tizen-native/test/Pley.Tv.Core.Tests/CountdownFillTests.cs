using Pley.Tv.Core.Player;
using Xunit;

namespace Pley.Tv.Core.Tests;

/// <summary>Up Next's ring between the countdown's whole seconds (the Android app's countdownFill).</summary>
public class CountdownFillTests
{
    [Fact]
    public void Each_second_the_ring_glides_to_where_it_stands_at_that_seconds_end()
    {
        var fill = new CountdownFill();
        fill.Set(10, 0);
        Assert.Equal(0f, fill.At(0), 3);
        Assert.Equal(0.05f, fill.At(500), 3);
        Assert.True(fill.Moving(500));
        Assert.Equal(0.1f, fill.At(1_000), 3);
        Assert.False(fill.Moving(1_000));
        fill.Set(9, 1_000);
        Assert.Equal(0.15f, fill.At(1_500), 3);
    }

    [Fact]
    public void The_ring_is_full_as_the_countdown_reaches_nothing()
    {
        var fill = new CountdownFill();
        for (var left = 10; left >= 1; left--) fill.Set(left, (10 - left) * 1_000L);
        Assert.Equal(1f, fill.At(10_000), 3);
        fill.Set(0, 10_000);
        Assert.Equal(1f, fill.At(10_500), 3);
    }

    [Fact]
    public void A_second_that_comes_a_little_early_carries_on_from_where_the_ring_is()
    {
        var fill = new CountdownFill();
        fill.Set(10, 0);
        fill.Set(9, 960);
        Assert.Equal(0.096f, fill.At(960), 3);
        Assert.Equal(0.2f, fill.At(1_960), 3);
    }

    [Fact]
    public void A_countdown_started_again_jumps_back_rather_than_sliding_back()
    {
        var fill = new CountdownFill();
        for (var left = 10; left >= 6; left--) fill.Set(left, (10 - left) * 1_000L);
        fill.Set(10, 5_000);
        Assert.Equal(0f, fill.At(5_000), 3);
        Assert.Equal(0.05f, fill.At(5_500), 3);
    }

    [Fact]
    public void A_card_that_shows_midway_starts_the_ring_where_its_countdown_is()
    {
        var fill = new CountdownFill();
        fill.Set(4, 0);
        Assert.Equal(0.6f, fill.At(0), 3);
        fill.Reset();
        Assert.Equal(0f, fill.At(100), 3);
    }
}
