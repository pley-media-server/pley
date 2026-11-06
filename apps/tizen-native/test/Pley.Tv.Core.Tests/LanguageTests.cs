using System;
using System.Collections.Generic;
using System.Globalization;
using System.Linq;
using System.Text.Json;
using System.Text.RegularExpressions;
using System.Threading.Tasks;
using Pley.Tv.Core.Api;
using Pley.Tv.Core.Auth;
using Pley.Tv.Core.Net;
using Pley.Tv.Core.Ui;
using Xunit;

namespace Pley.Tv.Core.Tests;

/// <summary>Which language the app shows: the web Samsung app's answers (packages/tv/test/fixtures/language.json).</summary>
public class LanguageFixtureTests
{
    public static IEnumerable<object[]> PickCases() =>
        Fixtures.Load("language").GetProperty("pickLanguage").EnumerateArray().Select(c => new object[] { c.GetRawText() });

    [Theory]
    [MemberData(nameof(PickCases))]
    public void Picks_the_language_as_the_web_Samsung_app_does(string json)
    {
        var c = JsonDocument.Parse(json).RootElement;
        string? Text(string name) => c.GetProperty(name).ValueKind == JsonValueKind.String ? c.GetProperty(name).GetString() : null;
        var device = c.GetProperty("device").EnumerateArray().Select(d => d.GetString()!).ToList();
        Assert.Equal(c.GetProperty("out").GetString(), Languages.Pick(Text("account"), Text("server"), device));
    }

    public static IEnumerable<object[]> ChoiceCases() =>
        Fixtures.Load("language").GetProperty("pickLanguageWithChoice").EnumerateArray().Select(c => new object[] { c.GetRawText() });

    [Theory]
    [MemberData(nameof(ChoiceCases))]
    public void The_one_picked_on_the_TV_comes_after_the_accounts_as_the_web_Samsung_app_has_it(string json)
    {
        var c = JsonDocument.Parse(json).RootElement;
        string? Text(string name) => c.GetProperty(name).ValueKind == JsonValueKind.String ? c.GetProperty(name).GetString() : null;
        var device = c.GetProperty("device").EnumerateArray().Select(d => d.GetString()!).ToList();
        Assert.Equal(c.GetProperty("out").GetString(), Languages.Pick(Text("account"), Text("server"), device, Text("choice")));
    }

    [Fact]
    public void Tags_match_by_their_language()
    {
        Assert.Equal("fr", Languages.Supported("fr-CA"));
        Assert.Equal("es", Languages.Supported("ES_mx"));
        Assert.Equal("en", Languages.Supported("en_US.UTF-8"));
        Assert.Null(Languages.Supported("de-DE"));
        Assert.Null(Languages.Supported(""));
        Assert.Null(Languages.Supported(null));
    }
}

/// <summary>
/// The catalogs (Locales/&lt;language&gt;/&lt;area&gt;.json): each language has English's words, every
/// plural form it counts in and the same values, as the web app's i18n test asks of its own.
/// </summary>
public class CatalogTests
{
    static readonly Regex PluralSuffix = new Regex("_(zero|one|two|few|many|other)$", RegexOptions.CultureInvariant);
    static readonly Regex Value = new Regex(@"\{\{\s*([^,}\s]+)[^}]*\}\}", RegexOptions.CultureInvariant);

    static Dictionary<string, Dictionary<string, string>> Areas(string language) =>
        Strings.Sources(language).ToDictionary(s => s.Area, s => Strings.Flatten(s.Area, s.Json));

    /// <summary>The words' <c>{{values}}</c>, sorted; a form for one may leave the count out ("un film").</summary>
    static string Values(string key, string text) => string.Join(", ", Value.Matches(text).Cast<Match>().Select(m => m.Groups[1].Value)
        .Where(v => !(v == "count" && key.EndsWith("_one", StringComparison.Ordinal))).OrderBy(v => v, StringComparer.Ordinal));

    public static IEnumerable<object[]> Others() => Languages.All.Where(l => l != Languages.English).Select(l => new object[] { l });

    [Fact]
    public void Every_language_has_the_same_areas()
    {
        var english = Areas("en").Keys.OrderBy(a => a, StringComparer.Ordinal).ToList();
        Assert.Contains("common", english);
        Assert.Contains("signin", english);
        Assert.Contains("settings", english);
        // An area's name is the first part of its keys: a dot in it would split it.
        Assert.All(english, area => Assert.Matches("^[a-z][a-zA-Z0-9]*$", area));
        foreach (var language in Languages.All) Assert.Equal(english, Areas(language).Keys.OrderBy(a => a, StringComparer.Ordinal));
    }

    [Theory]
    [MemberData(nameof(Others))]
    public void A_language_has_Englishs_words_with_every_plural_form_and_the_same_values(string language)
    {
        var areas = Areas(language);
        foreach (var (area, english) in Areas("en"))
        {
            var words = areas[area];
            var expected = new SortedSet<string>(StringComparer.Ordinal);
            foreach (var key in english.Keys)
            {
                if (!PluralSuffix.IsMatch(key)) expected.Add(key);
                else foreach (var form in Strings.PluralForms(language)) expected.Add(PluralSuffix.Replace(key, "_" + form));
            }
            Assert.Equal(expected, words.Keys.OrderBy(k => k, StringComparer.Ordinal));
            foreach (var (key, text) in words)
            {
                Assert.False(string.IsNullOrWhiteSpace(text), $"{language}:{key} is empty");
                var source = english.TryGetValue(key, out var same) ? same : english[PluralSuffix.Replace(key, "_other")];
                Assert.Equal($"{key}: {Values(key, source)}", $"{key}: {Values(key, text)}");
            }
        }
    }

    [Fact]
    public void English_counts_with_one_and_other_together()
    {
        foreach (var english in Areas("en").Values)
            foreach (var key in english.Keys.Where(k => PluralSuffix.IsMatch(k)))
            {
                var stem = PluralSuffix.Replace(key, "");
                Assert.True(english.ContainsKey(stem + "_one") && english.ContainsKey(stem + "_other"), $"{stem} needs _one and _other");
            }
    }

    [Fact]
    public void Every_key_the_code_names_is_in_English()
    {
        // Strings.T("area.key"…) and Strings.Count("area.key", …), as written in Core's and the app's sources.
        var named = new Regex(@"Strings\.(T|Count)\(\s*""([^""]+)""", RegexOptions.CultureInvariant);
        var english = Strings.Of(Languages.English);
        var missing = new List<string>();
        var found = 0;
        foreach (var folder in new[] { "core", "app" })
            foreach (var file in System.IO.Directory.EnumerateFiles(System.IO.Path.Combine(AppContext.BaseDirectory, folder), "*.cs", System.IO.SearchOption.AllDirectories))
                foreach (Match m in named.Matches(System.IO.File.ReadAllText(file)))
                {
                    found++;
                    var key = m.Groups[2].Value;
                    var known = m.Groups[1].Value == "Count" ? english.ContainsKey(key + "_one") && english.ContainsKey(key + "_other") : english.ContainsKey(key);
                    if (!known) missing.Add($"{System.IO.Path.GetFileName(file)}: {key}");
                }
        Assert.True(found > 50, "the sources weren't found");
        Assert.Empty(missing);
    }

    [Fact]
    public void A_catalog_holds_words_only()
    {
        foreach (var language in Languages.All)
            foreach (var (area, json) in Strings.Sources(language))
            {
                void Check(JsonElement e, string at)
                {
                    if (e.ValueKind == JsonValueKind.Object)
                        foreach (var p in e.EnumerateObject()) Check(p.Value, at + "." + p.Name);
                    else Assert.True(e.ValueKind == JsonValueKind.String, $"{language}/{area}.json: {at} isn't words");
                }
                Check(JsonDocument.Parse(json).RootElement, area);
            }
    }

    [Fact]
    public void French_has_no_break_spaces_before_its_tall_marks_and_curly_apostrophes()
    {
        foreach (var (key, text) in Areas("fr").SelectMany(a => a.Value))
        {
            Assert.False(text.Contains('\''), $"fr:{key} has a straight apostrophe (’): {text}");
            Assert.False(text.Contains("..."), $"fr:{key} has three dots (…): {text}");
            // A no-break space (U+00A0) before : ; ? ! » and after «, never an ordinary one or none.
            Assert.False(Regex.IsMatch(text, @" [:;?!»]|« "), $"fr:{key} has an ordinary space where a no-break one goes: {text}");
            Assert.False(Regex.IsMatch(text, @"[\w}][;?!»]|[\w}]:(\s|$)|«[\w{]"), $"fr:{key} needs a no-break space: {text}");
        }
    }

    [Fact]
    public void Spanish_opens_its_questions_and_exclamations_and_has_curly_apostrophes()
    {
        foreach (var (key, text) in Areas("es").SelectMany(a => a.Value))
        {
            Assert.False(text.Contains('\''), $"es:{key} has a straight apostrophe (’): {text}");
            Assert.False(text.Contains("..."), $"es:{key} has three dots (…): {text}");
            Assert.True(text.Count(c => c == '?') <= text.Count(c => c == '¿'), $"es:{key} closes a question it doesn't open (¿…?): {text}");
            Assert.True(text.Count(c => c == '!') <= text.Count(c => c == '¡'), $"es:{key} closes an exclamation it doesn't open (¡…!): {text}");
        }
    }
}

/// <summary>Looking words up: English where a language has none, values filled in, counts in each language's forms.</summary>
public class StringsTests
{
    static Catalog Words(string language, string json, string? english = null) =>
        new Catalog(language, Strings.Flatten("t", json), english == null ? null : Strings.Flatten("t", english), CultureInfo.GetCultureInfo(language));

    [Fact]
    public void Words_fall_back_to_English_then_to_their_key()
    {
        var fr = Words("fr", @"{""a"":""Bonjour""}", @"{""a"":""Hello"",""b"":""Only in English""}");
        Assert.Equal("Bonjour", fr.T("t.a"));
        Assert.Equal("Only in English", fr.T("t.b"));
        Assert.Equal("t.c", fr.T("t.c"));
        // The app's own catalogs, whichever language is shown.
        Assert.Equal("Choose your server", Strings.In("en").T("signin.servers.title"));
        Assert.Equal("Choisissez votre serveur", Strings.In("fr").T("signin.servers.title"));
        Assert.Equal("Elige tu servidor", Strings.In("es").T("signin.servers.title"));
        Assert.Equal("Choose your server", Strings.In("de").T("signin.servers.title"));
        Assert.Equal("Choose your server", Strings.T("signin.servers.title"));
    }

    [Fact]
    public void Values_fill_in_by_name_and_numbers_are_written_the_languages_way()
    {
        var en = Words("en", @"{""a"":""{{name}} has {{n}} of {{ total, number }}"",""b"":""{{missing}} stays""}");
        Assert.Equal("Ann has 1234 of 5,678", en.T("t.a", ("name", "Ann"), ("n", 1234), ("total", 5678)));
        Assert.Equal("{{missing}} stays", en.T("t.b", ("name", "Ann")));
        Assert.Equal("{{missing}} stays", en.T("t.b"));
        var fr = Words("fr", @"{""a"":""{{x}} et {{y, number}}""}");
        var numbers = fr.Culture.NumberFormat;
        Assert.Equal($"1{numbers.NumberDecimalSeparator}5 et 5{numbers.NumberGroupSeparator}678", fr.T("t.a", ("x", 1.5), ("y", 5678)));
    }

    [Theory]
    [InlineData("en", 0, "other")]
    [InlineData("en", 1, "one")]
    [InlineData("en", 2, "other")]
    [InlineData("en", 1_000_000, "other")]
    [InlineData("fr", 0, "one")]
    [InlineData("fr", 1, "one")]
    [InlineData("fr", 2, "other")]
    [InlineData("fr", 1_000_000, "many")]
    [InlineData("fr", 3_000_000, "many")]
    [InlineData("fr", 1_000_001, "other")]
    [InlineData("es", 0, "other")]
    [InlineData("es", 1, "one")]
    [InlineData("es", 1_000_000, "many")]
    [InlineData("es", 1_500_000, "other")]
    public void A_count_takes_the_plural_form_CLDR_gives(string language, long count, string form) =>
        Assert.Equal(form, Strings.Plural(language, count));

    [Fact]
    public void A_count_picks_its_form_and_falls_back_to_other_then_to_English()
    {
        const string English = @"{""files_one"":""{{count}} file"",""files_other"":""{{count}} files"",""left_one"":""one left"",""left_other"":""{{count}} left""}";
        var en = Words("en", English);
        Assert.Equal("1 file", en.Count("t.files", 1));
        Assert.Equal("0 files", en.Count("t.files", 0));
        Assert.Equal("1234 files", en.Count("t.files", 1234));
        Assert.Equal("one left", en.Count("t.left", 1));
        var fr = Words("fr", @"{""files_one"":""{{count}} fichier"",""files_many"":""{{count, number}} de fichiers"",""files_other"":""{{count}} fichiers""}", English);
        var group = fr.Culture.NumberFormat.NumberGroupSeparator;
        Assert.Equal("0 fichier", fr.Count("t.files", 0));
        Assert.Equal("2 fichiers", fr.Count("t.files", 2));
        Assert.Equal($"1{group}000{group}000 de fichiers", fr.Count("t.files", 1_000_000));
        // A form the language lacks is its "other"; a key it lacks, English's.
        var es = Words("es", @"{""files_one"":""{{count}} archivo"",""files_other"":""{{count}} archivos""}", English);
        Assert.Equal("1000000 archivos", es.Count("t.files", 1_000_000));
        Assert.Equal("3 left", es.Count("t.left", 3));
        Assert.Equal("t.gone", es.Count("t.gone", 3));
    }

    [Fact]
    public void Dates_and_numbers_follow_the_language_with_the_TVs_region_when_it_is_in_it()
    {
        Assert.Equal("en-GB", Languages.CultureOf("en", new[] { "en_GB.UTF-8" }).Name);
        Assert.Equal("fr-CA", Languages.CultureOf("fr", new[] { "en-GB", "fr_CA" }).Name);
        Assert.Equal("es", Languages.CultureOf("es", new[] { "en-US" }).Name);
        Assert.Equal("fr", Strings.In("fr").Culture.Name);
    }
}

/// <summary>The language the app shows, as the server and the account say (docs/API.md, "Language").</summary>
public class AppLanguageTests
{
    const string Home = "https://pley.example.com";

    static AppState State(MemoryKeyValue store, FakeServer server, FakeScheduler scheduler, params string[] tv) =>
        new AppState(new AppPlatform("tizen", "client", store) { Languages = tv }, server.Transport, (_, _, _, _, _) => Task.FromResult(new EventEnd(0, false)), scheduler);

    static void SignIn(AppState state, FakeScheduler scheduler) =>
        state.TokenStore!.Install(new StoredTokens(Home, "pley_at_0", "pley_rt_0", scheduler.Now + 3_600_000, "library play", Home + "/t", Home + "/r"), "library play").Wait();

    static string ServerJson(string? language) => $@"{{""name"":""Home"",""version"":""0.1.0"",""language"":{(language == null ? "null" : $@"""{language}""")}}}";

    [Fact]
    public void Starts_in_the_last_language_shown_else_the_TVs()
    {
        var store = new MemoryKeyValue();
        Assert.Equal("es", State(store, new FakeServer(), new FakeScheduler(), "de-DE", "es-MX").Language.Value);
        Assert.Equal("en", State(store, new FakeServer(), new FakeScheduler(), "de-DE").Language.Value);
        // Starting writes nothing: the TV's language is the TV's.
        Assert.Empty(store.Values);
        store.Set("pley.language", "fr");
        Assert.Equal("fr", State(store, new FakeServer(), new FakeScheduler(), "es-MX").Language.Value);
    }

    [Fact]
    public async Task Signing_in_goes_by_the_accounts_language_then_the_TVs_then_the_servers()
    {
        var store = new MemoryKeyValue();
        var scheduler = new FakeScheduler();
        var server = new FakeServer();
        server.On("GET", Home + "/api/server", 200, ServerJson("fr"));
        // A TV in a language Pley doesn't have: the server's decides.
        var state = State(store, server, scheduler, "de-DE");
        state.Use(Home);
        // Until the server says, the language shown stays.
        Assert.Equal("en", state.Language.Value);
        await state.LoadServer();
        Assert.Equal(("fr", "fr"), (state.Language.Value, state.ServerLanguage));
        Assert.Equal("fr", store.Get("pley.language"));

        // Signed in, who is watching is waited for: the server's language stays meanwhile.
        SignIn(state, scheduler);
        state.HeardServer(Home, null);
        Assert.Equal("fr", state.Language.Value);
        server.On("GET", Home + "/api/auth/me", 200, @"{""id"":1,""username"":""a"",""name"":""A"",""language"":""es""}");
        server.On("GET", Home + "/api/libraries", 200, "[]");
        await state.LoadAccount();
        Assert.Equal("es", state.Language.Value);
        // The account's own comes first; without one, the TV's; without either, the server's.
        state.HeardServer(Home, "fr");
        Assert.Equal("es", state.Language.Value);
        state.Me.Value = state.Me.Value! with { Language = null };
        Assert.Equal("fr", state.Language.Value);
        state.HeardServer(Home, "de");
        Assert.Equal("en", state.Language.Value);
        state.TvLanguagesChanged(new[] { "es-ES" });
        Assert.Equal("es", state.Language.Value);
        state.HeardServer(Home, "fr");
        Assert.Equal("es", state.Language.Value);
        // The next start shows the last language before anyone answers.
        Assert.Equal("es", State(store, new FakeServer(), scheduler, "en-US").Language.Value);
    }

    [Fact]
    public async Task Signed_out_the_servers_language_shows_for_signing_in_again()
    {
        var scheduler = new FakeScheduler();
        var server = new FakeServer();
        server.On("GET", Home + "/api/server", 200, ServerJson("es"));
        server.On("GET", Home + "/api/auth/me", 200, @"{""id"":1,""username"":""a"",""name"":""A"",""language"":""fr""}");
        server.On("GET", Home + "/api/libraries", 200, "[]");
        var state = State(new MemoryKeyValue(), server, scheduler);
        state.Use(Home);
        SignIn(state, scheduler);
        await state.LoadServer();
        await state.LoadAccount();
        Assert.Equal("fr", state.Language.Value);
        await state.SignOutNow();
        Assert.Equal("es", state.Language.Value);
    }

    [Fact]
    public void The_globes_choice_shows_at_once_is_kept_on_the_TV_and_Automatic_forgets_it()
    {
        var store = new MemoryKeyValue();
        var state = State(store, new FakeServer(), new FakeScheduler(), "fr-FR");
        var globe = new LanguageGlobe(state);
        Assert.Equal("Français", globe.Label);
        Assert.Equal(new[] { "Automatic (Français)", "English", "Français", "Español" }, globe.Choices.Select(c => c.Label));
        Assert.Equal(new[] { true, false, false, false }, globe.Choices.Select(c => c.Checked));
        Assert.Equal(0, globe.Focus);

        globe.Choose(3);
        Assert.Equal(("es", "es", "es"), (state.Language.Value, state.DeviceLanguage, store.Get("pley.deviceLanguage")));
        Assert.Equal("Español", globe.Label);
        Assert.Equal(3, globe.Focus);
        Assert.Equal(new[] { false, false, false, true }, globe.Choices.Select(c => c.Checked));
        // The screen drawn again in Spanish gives the globe focus, once.
        Assert.True(globe.TakeFocus());
        Assert.False(globe.TakeFocus());
        // The next start shows it, before the TV's.
        store.Remove("pley.language");
        Assert.Equal("es", State(store, new FakeServer(), new FakeScheduler(), "fr-FR").Language.Value);

        globe.Choose(0);
        Assert.Equal("fr", state.Language.Value);
        Assert.Null(state.DeviceLanguage);
        Assert.Null(store.Get("pley.deviceLanguage"));
        // Choosing what shows already draws nothing again.
        globe.Choose(2);
        Assert.False(globe.TakeFocus());
    }

    [Fact]
    public void Automatic_on_the_globe_is_the_servers_on_a_TV_in_a_language_Pley_doesnt_have_else_English()
    {
        var state = State(new MemoryKeyValue(), new FakeServer(), new FakeScheduler(), "de-DE");
        var globe = new LanguageGlobe(state);
        globe.Choose(2);
        Assert.Equal("fr", state.Language.Value);
        // No server says yet: Automatic is English, and shows at once.
        Assert.Equal("Automatic (English)", globe.Choices[0].Label);
        globe.Choose(0);
        Assert.Equal("en", state.Language.Value);
        state.Use(Home);
        state.HeardServer(Home, "es");
        Assert.Equal("es", state.Language.Value);
        Assert.Equal("Automatic (Español)", new LanguageGlobe(state).Choices[0].Label);
    }

    [Fact]
    public async Task Signed_in_the_one_picked_on_the_TV_comes_after_the_accounts_and_before_the_TVs()
    {
        var store = new MemoryKeyValue();
        store.Set("pley.deviceLanguage", "es");
        var scheduler = new FakeScheduler();
        var server = new FakeServer();
        server.On("GET", Home + "/api/server", 200, ServerJson("fr"));
        server.On("GET", Home + "/api/auth/me", 200, @"{""id"":1,""username"":""a"",""name"":""A"",""language"":null}");
        server.On("GET", Home + "/api/libraries", 200, "[]");
        var state = State(store, server, scheduler, "en-US");
        Assert.Equal(("es", "es"), (state.Language.Value, state.DeviceLanguage));
        state.Use(Home);
        SignIn(state, scheduler);
        await state.LoadServer();
        await state.LoadAccount();
        Assert.Equal("es", state.Language.Value);
        state.Me.Value = state.Me.Value! with { Language = "fr" };
        Assert.Equal("fr", state.Language.Value);
        state.Me.Value = state.Me.Value! with { Language = null };
        state.ChooseDeviceLanguage(null);
        Assert.Equal("en", state.Language.Value);
    }

    [Fact]
    public async Task Requests_ask_for_answers_in_the_language_shown()
    {
        var server = new FakeServer();
        server.On("GET", Home + "/api/server", 200, ServerJson("es"));
        // A TV in a language Pley doesn't have: English, then the server's once it has said.
        var state = State(new MemoryKeyValue(), server, new FakeScheduler(), "de-DE");
        state.Use(Home);
        await state.LoadServer();
        await state.LoadServer();
        Assert.Equal(new[] { "en", "es" }, server.Requests.Select(r => r.Headers!["Accept-Language"]));
    }
}

/// <summary>Tests that show another language than English: alone, as every other test reads English.</summary>
[CollectionDefinition(Name, DisableParallelization = true)]
public sealed class LanguageShown
{
    public const string Name = "the language shown";
}

/// <summary>Choosing a server and signing in, with their messages, in French and Spanish; English is the other tests'.</summary>
[Collection(LanguageShown.Name)]
public sealed class SignInWordsTests : IDisposable
{
    public void Dispose() => Strings.Reset();

    [Fact]
    public void Choosing_a_server_and_signing_in_read_in_French()
    {
        Strings.Use("fr");
        Assert.Equal("fr", Strings.Language);
        Assert.Equal("Choisissez votre serveur", Strings.T("signin.servers.title"));
        Assert.Equal("Oublier Salon ?", Strings.T("signin.servers.forgetTitle", ("name", "Salon")));
        Assert.Equal("Quelque chose répond à nas:7539, mais ce n’est pas Pley.", new ConnectProblem.NotPley("nas:7539").Text);
        Assert.Equal("Impossible de joindre nas:7539 : rien ne répond à cette adresse",
            new ConnectProblem.Unreachable("nas:7539", Transports.Reason(NetworkFailure.NothingAnswers)).Text);
        Assert.Equal("Saisissez une adresse comme nas, 192.168.1.20 ou pley.example.com.", new ConnectProblem.BadAddress().Text);
        Assert.Equal("Ce serveur exige la connexion en deux étapes : votre téléphone la configure si ce n’est pas déjà fait.", LinkText.TwoStepRequired);
        Assert.Equal("Ce serveur est trop ancien pour cette application.", Errors.Message(new AuthException(Metadata.TooOld)));
        Assert.Equal("Le serveur a refusé (HTTP 429).", Errors.Message(new TokenException(429, null)));
        Assert.Equal("Pas de réponse à temps.", Errors.Message(new NetworkException(NetworkFailure.NoAnswerInTime)));
        Assert.Equal("Impossible de joindre votre serveur", Errors.OfflineTitle);
    }

    [Fact]
    public void Choosing_a_server_and_signing_in_read_in_Spanish()
    {
        Strings.Use("es");
        Assert.Equal("Elige tu servidor", Strings.T("signin.servers.title"));
        Assert.Equal("¿Olvidar Salón?", Strings.T("signin.servers.forgetTitle", ("name", "Salón")));
        Assert.Equal("No se pudo conectar con nas:7539: no hubo respuesta a tiempo",
            new ConnectProblem.Unreachable("nas:7539", Transports.Reason(NetworkFailure.NoAnswerInTime)).Text);
        Assert.Equal("Sesión iniciada como Ana", Strings.T("signin.link.signedInAs", ("name", "Ana")));
        Assert.Equal("Este código ya no funciona: obtén uno nuevo.", Errors.AsSentence(Strings.T("signin.errors.codeSpent")));
        Assert.Equal("Este servidor es demasiado antiguo para esta aplicación.", Errors.Message(new AuthException(Metadata.TooOld)));
    }

    [Fact]
    public void A_network_failure_reads_in_the_language_shown_whichever_it_failed_in()
    {
        var failure = new NetworkException(NetworkFailure.NothingAnswers);
        Strings.Use("es");
        Assert.Equal("Nada responde en esa dirección.", Errors.Message(failure));
        Assert.Equal("Nada responde en esa dirección. Comprueba que el servidor esté encendido y que este dispositivo esté en su red.", Errors.OfflineBody(failure));
    }

    [Fact]
    public void The_settings_language_row_reads_in_the_language_shown()
    {
        Strings.Use("fr", new[] { "en-GB" });
        Assert.Equal("fr", Strings.Culture.Name);
        Assert.Equal("Automatique (English)", SettingsText.Automatic(Languages.NameOf("en")));
        Strings.Use("en", new[] { "en-GB" });
        Assert.Equal("en-GB", Strings.Culture.Name);
        // A language Pley hasn't is English.
        Strings.Use("de");
        Assert.Equal("en", Strings.Language);
    }
}
