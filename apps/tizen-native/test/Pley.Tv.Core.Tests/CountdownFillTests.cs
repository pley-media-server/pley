using System;
using System.Collections.Generic;
using System.Linq;
using System.Text.Json;
using Pley.Tv.Core.Player;
using Xunit;

namespace Pley.Tv.Core.Tests;

/// <summary>
/// How full Up Next's countdown is between its whole seconds (the Android app's countdownFill), as
/// the Samsung web app's is (packages/tv/test/fixtures/countdown-fill.json).
/// </summary>
public class CountdownFillTests
{
    public static IEnumerable<object[]> Cases() =>
        Enumerable.Range(0, Fixtures.Load("countdown-fill").GetProperty("cases").GetArrayLength()).Select(i => new object[] { i });

    [Theory]
    [MemberData(nameof(Cases))]
    public void The_countdown_fills_as_the_shared_fixture_has_it(int i)
    {
        var fixture = Fixtures.Load("countdown-fill");
        Assert.Equal(NextUp.CountdownSeconds, fixture.GetProperty("total").GetInt32());
        var c = fixture.GetProperty("cases")[i];
        var name = c.GetProperty("name").GetString();
        var smooth = !c.TryGetProperty("smooth", out var stepped) || stepped.GetBoolean();
        var fill = new CountdownFill();
        foreach (var step in c.GetProperty("steps").EnumerateArray())
        {
            var ms = step.GetProperty("ms").GetInt64();
            if (step.TryGetProperty("set", out var set))
            {
                fill.Set(set.GetInt32(), ms, smooth);
                continue;
            }
            Assert.Equal((name, ms, step.GetProperty("fill").GetDouble()), (name, ms, Math.Round((double)fill.At(ms), 3)));
            if (step.TryGetProperty("moving", out var moving))
                Assert.Equal((name, ms, moving.GetBoolean()), (name, ms, fill.Moving(ms)));
        }
    }

    [Fact]
    public void A_countdown_that_stops_empties_the_fill()
    {
        var fill = new CountdownFill();
        fill.Set(4, 0);
        fill.Reset();
        Assert.Equal(0f, fill.At(100), 3);
        Assert.False(fill.Moving(100));
    }
}
