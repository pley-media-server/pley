using System.Collections.Generic;
using System.Linq;
using System.Text.Json;
using Pley.Tv.Core.Api;
using Pley.Tv.Core.Ui;
using Xunit;

namespace Pley.Tv.Core.Tests;

/// <summary>
/// The Collections tab's groups (the Android app's CollectionsView), against
/// packages/tv/test/fixtures/collections.json, which the web TV app runs too.
/// </summary>
public class CollectionGroupsTests
{
    public static IEnumerable<object[]> Names() =>
        Fixtures.Load("collections").GetProperty("cases").EnumerateArray().Select(c => new object[] { c.GetProperty("name").GetString()! });

    static CollectionCategory Category(JsonElement c) => new CollectionCategory(
        c.GetProperty("id").GetInt64(),
        c.GetProperty("key").ValueKind == JsonValueKind.Null ? null : c.GetProperty("key").GetString(),
        c.GetProperty("title").GetString()!);

    static CategoryChoice Choice(JsonElement c) => c.ValueKind == JsonValueKind.Number
        ? new CategoryChoice.Of(c.GetInt64())
        : new CategoryChoice.All();

    static readonly TileModel Tile = new TileModel(0, "", "", new string[0], 0, false, 0, new string[0], "");

    [Theory]
    [MemberData(nameof(Names))]
    public void Groups_as_the_fixture_says(string name)
    {
        var fixture = Fixtures.Load("collections");
        var all = fixture.GetProperty("categories").EnumerateArray().Select(Category).ToList();
        var c = fixture.GetProperty("cases").EnumerateArray().Single(x => x.GetProperty("name").GetString() == name);
        var categories = c.TryGetProperty("categories", out var own) ? own.EnumerateArray().Select(Category).ToList() : all;
        var tiles = c.GetProperty("collections").EnumerateArray().Select(x =>
        {
            var id = x.GetProperty("category").GetInt64();
            var known = all.FirstOrDefault(k => k.Id == id);
            var title = x.TryGetProperty("title", out var t) ? t.GetString()! : known?.Title ?? "";
            return new CollectionTile(x.GetProperty("id").GetInt64(), 1, Tile, new CategoryRef(id, known?.Key, title));
        }).ToList();

        var view = CollectionsView.Of(tiles, categories, Choice(c.GetProperty("chosen")));
        var o = c.GetProperty("out");
        Assert.Equal(
            o.GetProperty("groups").EnumerateArray().Select(g => (
                g.GetProperty("key").GetString()!,
                g.GetProperty("title").GetString(),
                string.Join(",", g.GetProperty("ids").EnumerateArray().Select(i => i.GetInt64())))),
            view.Groups.Select(g => (g.Key, (string?)g.Category.Title, string.Join(",", g.Collections.Select(x => x.Id)))));
        Assert.Equal(o.GetProperty("choices").EnumerateArray().Select(k => k.GetString()), view.Choices.Select(g => g.Key));
        Assert.Equal(Choice(o.GetProperty("category")), view.Category);
        Assert.Equal(o.GetProperty("headed").GetBoolean(), view.Headed);
        Assert.Equal(o.GetProperty("offered").GetBoolean(), view.Offered);
        Assert.Equal(o.GetProperty("count").GetInt32(), view.Count);
        Assert.Equal(o.GetProperty("total").GetInt32(), view.Total);
    }

    [Fact]
    public void A_choice_reads_back_from_its_panel_value()
    {
        foreach (var choice in new CategoryChoice[] { new CategoryChoice.All(), new CategoryChoice.Of(5) })
            Assert.Equal(choice, CategoryChoice.Parse(choice.Value));
        Assert.Equal(new CategoryChoice.All(), CategoryChoice.Parse("nonsense"));
    }
}
