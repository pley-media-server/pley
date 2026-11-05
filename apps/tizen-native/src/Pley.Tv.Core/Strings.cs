using System;
using System.Collections.Concurrent;
using System.Collections.Generic;
using System.Globalization;
using System.IO;
using System.Linq;
using System.Text;
using System.Text.Json;

namespace Pley.Tv.Core;

/// <summary>
/// The app's words, in the language shown. They are in catalogs,
/// <c>Locales/&lt;language&gt;/&lt;area&gt;.json</c>, in the format of the server's and the web
/// app's (i18next's): nested keys, read as <c>area.key.subkey</c>; <c>{{name}}</c> for what goes
/// in; and <c>_one</c>, <c>_many</c> and <c>_other</c> after a key whose words depend on a count.
/// The catalogs are built into the assembly, and a language's are read the first time it shows.
/// A word a language lacks is English's; one English lacks is its key.
///
/// Words are looked up from any thread. Until the app chooses a language
/// (<see cref="Use"/>), they are English, with dates and numbers in the thread's culture.
/// </summary>
public static class Strings
{
    /// <summary>Where the catalogs are among the assembly's resources: then the language, the area, ".json".</summary>
    const string Prefix = "Pley.Tv.Core.Locales.";

    static readonly ConcurrentDictionary<string, Lazy<IReadOnlyDictionary<string, string>>> Words = new ConcurrentDictionary<string, Lazy<IReadOnlyDictionary<string, string>>>();

    static volatile Catalog current = new Catalog(Languages.English, null);

    /// <summary>The language shown: "en", "fr" or "es".</summary>
    public static string Language => current.Language;

    /// <summary>What dates and numbers are written in: the language shown's culture (<see cref="Languages.CultureOf"/>).</summary>
    public static CultureInfo Culture => current.Culture;

    /// <summary>The words in <paramref name="language"/> from now on, dates and numbers in its culture with <paramref name="device"/>'s region (<see cref="Languages.CultureOf"/>).</summary>
    public static void Use(string language, IEnumerable<string>? device = null)
    {
        var code = Languages.Supported(language) ?? Languages.English;
        current = new Catalog(code, Languages.CultureOf(code, device ?? Array.Empty<string>()));
    }

    /// <summary>The words of <paramref name="language"/>, whichever is shown: for tests, and for words that must be in a language of their own.</summary>
    public static Catalog In(string language, CultureInfo? culture = null)
    {
        var code = Languages.Supported(language) ?? Languages.English;
        return new Catalog(code, culture ?? Languages.CultureOf(code, Array.Empty<string>()));
    }

    /// <summary>English again, dates and numbers in the thread's culture, as before a language was chosen: tests that showed another put it back.</summary>
    internal static void Reset() => current = new Catalog(Languages.English, null);

    /// <summary>The words for <paramref name="key"/> ("signin.servers.title").</summary>
    public static string T(string key) => current.T(key);

    /// <summary>The words for <paramref name="key"/>, each <c>{{name}}</c> in them filled in from <paramref name="values"/>.</summary>
    public static string T(string key, params (string Name, object? Value)[] values) => current.T(key, values);

    /// <summary>The words for <paramref name="count"/> of something: <paramref name="key"/>'s form for it (<c>key_one</c>…), with <c>{{count}}</c> and <paramref name="values"/> filled in.</summary>
    public static string Count(string key, long count, params (string Name, object? Value)[] values) => current.Count(key, count, values);

    /// <summary>
    /// The plural form <paramref name="count"/> takes in <paramref name="language"/>, as CLDR has it:
    /// "one" for 1 (in French 0 and 1 too), "many" in French and Spanish for a whole number of
    /// millions, "other" for the rest.
    /// </summary>
    public static string Plural(string language, long count)
    {
        var n = Math.Abs(count);
        var millions = n != 0 && n % 1_000_000 == 0;
        return language switch
        {
            "fr" => n <= 1 ? "one" : millions ? "many" : "other",
            "es" => n == 1 ? "one" : millions ? "many" : "other",
            _ => n == 1 ? "one" : "other",
        };
    }

    /// <summary>The plural forms a counted key has in <paramref name="language"/>: English "one" and "other", French and Spanish "many" too.</summary>
    public static IReadOnlyList<string> PluralForms(string language) =>
        language is "fr" or "es" ? new[] { "one", "many", "other" } : new[] { "one", "other" };

    /// <summary><paramref name="language"/>'s words, every area's, by their whole key.</summary>
    internal static IReadOnlyDictionary<string, string> Of(string language) =>
        Words.GetOrAdd(language, l => new Lazy<IReadOnlyDictionary<string, string>>(() => Read(l))).Value;

    /// <summary><paramref name="language"/>'s catalogs as they are built in: each area's name and its JSON.</summary>
    internal static IEnumerable<(string Area, string Json)> Sources(string language)
    {
        var assembly = typeof(Strings).Assembly;
        var prefix = Prefix + language + ".";
        foreach (var name in assembly.GetManifestResourceNames().OrderBy(n => n, StringComparer.Ordinal))
        {
            if (!name.StartsWith(prefix, StringComparison.Ordinal) || !name.EndsWith(".json", StringComparison.Ordinal)) continue;
            using var stream = assembly.GetManifestResourceStream(name)!;
            using var reader = new StreamReader(stream, Encoding.UTF8);
            yield return (name.Substring(prefix.Length, name.Length - prefix.Length - ".json".Length), reader.ReadToEnd());
        }
    }

    /// <summary>The words of <paramref name="json"/>, under <paramref name="area"/>: <c>{"a": {"b": "x"}}</c> → <c>area.a.b</c> = "x".</summary>
    internal static Dictionary<string, string> Flatten(string area, string json)
    {
        var words = new Dictionary<string, string>(StringComparer.Ordinal);
        using var doc = JsonDocument.Parse(json);
        Flatten(area, doc.RootElement, words);
        return words;
    }

    static void Flatten(string prefix, JsonElement value, Dictionary<string, string> into)
    {
        switch (value.ValueKind)
        {
            case JsonValueKind.Object:
                foreach (var p in value.EnumerateObject()) Flatten(prefix + "." + p.Name, p.Value, into);
                break;
            case JsonValueKind.String:
                into[prefix] = value.GetString()!;
                break;
        }
    }

    static IReadOnlyDictionary<string, string> Read(string language)
    {
        var words = new Dictionary<string, string>(StringComparer.Ordinal);
        foreach (var (area, json) in Sources(language))
            foreach (var w in Flatten(area, json)) words[w.Key] = w.Value;
        return words;
    }
}

/// <summary>One language's words (<see cref="Strings"/>), English's where it has none.</summary>
public sealed class Catalog
{
    readonly IReadOnlyDictionary<string, string> words;
    readonly IReadOnlyDictionary<string, string>? english;
    readonly CultureInfo? culture;

    /// <param name="culture">Dates and numbers; null: the thread's culture, until a language is chosen.</param>
    internal Catalog(string language, CultureInfo? culture)
        : this(language, Strings.Of(language), language == Languages.English ? null : Strings.Of(Languages.English), culture)
    {
    }

    /// <summary>A catalog of the words given: <paramref name="english"/> fills in what <paramref name="words"/> lacks.</summary>
    internal Catalog(string language, IReadOnlyDictionary<string, string> words, IReadOnlyDictionary<string, string>? english, CultureInfo? culture)
    {
        Language = language;
        this.words = words;
        this.english = english;
        this.culture = culture;
    }

    public string Language { get; }

    public CultureInfo Culture => culture ?? CultureInfo.CurrentCulture;

    /// <summary>The words for <paramref name="key"/>.</summary>
    public string T(string key) => Find(key) ?? key;

    /// <summary>The words for <paramref name="key"/>, each <c>{{name}}</c> in them filled in from <paramref name="values"/>.</summary>
    public string T(string key, params (string Name, object? Value)[] values) => Fill(T(key), values);

    /// <summary>The words for <paramref name="count"/> of something, in this language's plural form for it (<see cref="Strings.Plural"/>).</summary>
    public string Count(string key, long count, params (string Name, object? Value)[] values)
    {
        var text = Own($"{key}_{Strings.Plural(Language, count)}") ?? Own(key + "_other")
            ?? (english != null && english.TryGetValue($"{key}_{Strings.Plural(Languages.English, count)}", out var e) ? e : null)
            ?? key;
        var all = new (string, object?)[values.Length + 1];
        all[0] = ("count", count);
        values.CopyTo(all, 1);
        return Fill(text, all);
    }

    string? Own(string key) => words.TryGetValue(key, out var w) ? w : null;

    string? Find(string key) => Own(key) ?? (english != null && english.TryGetValue(key, out var e) ? e : null);

    /// <summary>Each <c>{{name}}</c> or <c>{{name, number}}</c> of <paramref name="text"/> with its value; one without a value stays as it is.</summary>
    string Fill(string text, (string Name, object? Value)[] values)
    {
        if (values.Length == 0 || text.IndexOf("{{", StringComparison.Ordinal) < 0) return text;
        var filled = new StringBuilder(text.Length + 16);
        var at = 0;
        while (true)
        {
            var open = text.IndexOf("{{", at, StringComparison.Ordinal);
            var close = open < 0 ? -1 : text.IndexOf("}}", open + 2, StringComparison.Ordinal);
            if (close < 0) break;
            var inside = text.Substring(open + 2, close - open - 2);
            var comma = inside.IndexOf(',');
            var name = (comma < 0 ? inside : inside.Substring(0, comma)).Trim();
            var format = comma < 0 ? null : inside.Substring(comma + 1).Trim();
            var found = Array.FindIndex(values, v => v.Name == name);
            filled.Append(text, at, open - at);
            if (found < 0) filled.Append(text, open, close + 2 - open);
            else filled.Append(Format(values[found].Value, format));
            at = close + 2;
        }
        filled.Append(text, at, text.Length - at);
        return filled.ToString();
    }

    /// <summary>
    /// A value as the language writes it: numbers in its culture, and with <c>, number</c> grouped
    /// by thousands ("1,234", "1 234"), as i18next's number format does. Without it a number is
    /// written plainly ("1234"), as the app wrote counts before it had catalogs.
    /// </summary>
    string Format(object? value, string? format) => value switch
    {
        null => "",
        string s => s,
        IFormattable f when format == "number" => f.ToString(value is double or float or decimal ? "#,0.##" : "N0", Culture),
        IFormattable f => f.ToString(null, Culture),
        _ => value.ToString() ?? "",
    };
}
