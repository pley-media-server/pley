using System.Collections.Generic;
using System.Linq;
using System.Text.Json;
using Pley.Tv.Core.Ui;
using Xunit;

namespace Pley.Tv.Core.Tests;

/// <summary>
/// A profile's avatar is drawn as every app draws it (packages/tv/test/fixtures/avatar.json; the
/// web's Avatar.tsx): its color, its initial and the initial's ink.
/// </summary>
public class AvatarTests
{
    public static IEnumerable<object[]> Cases() =>
        Enumerable.Range(0, Fixtures.Load("avatar").GetProperty("cases").GetArrayLength()).Select(i => new object[] { i });

    static string Hex(uint rgb) => $"#{rgb:x6}";

    [Theory]
    [MemberData(nameof(Cases))]
    public void An_avatar_is_the_shared_fixtures(int i)
    {
        var c = Fixtures.Load("avatar").GetProperty("cases")[i];
        var name = c.GetProperty("name").GetString()!;
        var color = c.GetProperty("avatar_color");
        var avatar = ProfileAvatar.Of(name, color.ValueKind == JsonValueKind.Null ? null : color.GetString());
        Assert.Equal((name, "color", c.GetProperty("color").GetString()), (name, "color", Hex(avatar.Color)));
        Assert.Equal((name, "initial", c.GetProperty("initial").GetString()), (name, "initial", avatar.Initial));
        Assert.Equal((name, "ink", c.GetProperty("ink").GetString()), (name, "ink", Hex(avatar.Ink)));
    }

    [Fact]
    public void The_colors_and_inks_are_the_fixtures()
    {
        var fixture = Fixtures.Load("avatar");
        Assert.Equal(fixture.GetProperty("colors").EnumerateArray().Select(c => c.GetString()), ProfileAvatar.Colors.Select(Hex));
        Assert.Equal(fixture.GetProperty("ink").GetProperty("dark").GetString(), Hex(ProfileAvatar.DarkInk));
        Assert.Equal(fixture.GetProperty("ink").GetProperty("light").GetString(), Hex(ProfileAvatar.LightInk));
    }
}
