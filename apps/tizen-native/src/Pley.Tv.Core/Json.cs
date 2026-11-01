using System.Collections.Generic;
using System.Linq;
using System.Text.Json;

namespace Pley.Tv.Core;

/// <summary>
/// Reading answers whose shape isn't trusted, as the web TV app's <c>typeof</c> checks do: a
/// field of the wrong type counts as missing.
/// </summary>
static class Json
{
    public static JsonElement? Field(JsonElement o, string name) =>
        o.ValueKind == JsonValueKind.Object && o.TryGetProperty(name, out var v) ? v : null;

    public static string? String(JsonElement o, string name) =>
        Field(o, name) is { ValueKind: JsonValueKind.String } v ? v.GetString() : null;

    public static double? Number(JsonElement o, string name) =>
        Field(o, name) is { ValueKind: JsonValueKind.Number } v ? v.GetDouble() : null;

    public static bool? Bool(JsonElement o, string name) =>
        Field(o, name) is { ValueKind: JsonValueKind.True or JsonValueKind.False } v ? v.GetBoolean() : null;

    /// <summary>The non-empty strings of an array; null when it isn't one.</summary>
    public static IReadOnlyList<string>? Strings(JsonElement o, string name) =>
        Field(o, name) is { ValueKind: JsonValueKind.Array } v
            ? v.EnumerateArray().Where(e => e.ValueKind == JsonValueKind.String && e.GetString()!.Length > 0).Select(e => e.GetString()!).ToList()
            : null;
}
