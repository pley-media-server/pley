using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Text.Json;

namespace Pley.Tv.Core.Tests;

/// <summary>The web TV app's test fixtures (packages/tv/test/fixtures): inputs and the answers both apps give.</summary>
static class Fixtures
{
    public static JsonElement Load(string name)
    {
        var path = Path.Combine(AppContext.BaseDirectory, "fixtures", name + ".json");
        return JsonDocument.Parse(File.ReadAllText(path)).RootElement.Clone();
    }

    /// <summary>One rule's cases as xUnit rows: the input and the expected answer, as JSON.</summary>
    public static IEnumerable<object[]> Cases(string fixture, string rule) =>
        Load(fixture).GetProperty(rule).EnumerateArray()
            .Select(c => new object[] { c.GetProperty("in").GetString()!, c.GetProperty("out").GetRawText() });

    /// <summary>An answer as the fixture writes it, to compare with what it expects.</summary>
    public static string Json(object? value) => JsonSerializer.Serialize(value);

    /// <summary>The fixture's expected answer, written the same way.</summary>
    public static string Normalize(string json) => JsonSerializer.Serialize(JsonDocument.Parse(json).RootElement);
}
