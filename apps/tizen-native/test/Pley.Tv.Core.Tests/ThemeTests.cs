using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Text.Json;
using System.Threading.Tasks;
using Pley.Tv.Core.Api;
using Pley.Tv.Core.Auth;
using Pley.Tv.Core.Net;
using Pley.Tv.Core.Ui;
using Xunit;

namespace Pley.Tv.Core.Tests;

/// <summary>
/// The app's colors held to packages/themes/themes.json, the file every app's colors come from
/// (docs/THEMES.md, "In every app"), read here from the repository as it is: every role the app
/// draws with, in every theme under every accent, is the token it stands for.
/// </summary>
public class ThemeTests
{
    static JsonDocument Spec() => JsonDocument.Parse(File.ReadAllText(Path.Combine(AppContext.BaseDirectory, "themes", "themes.json")));

    /// <summary>The web's colorOf (apps/web/src/theme/spec.ts), on the file itself.</summary>
    static string ColorOf(JsonElement spec, string theme, string? accent, string token)
    {
        var t = spec.GetProperty("themes").EnumerateArray().First(x => x.GetProperty("id").GetString() == theme);
        if (accent != null && t.GetProperty("follows").EnumerateArray().Any(f => f.GetString() == token))
        {
            var a = spec.GetProperty("accents").EnumerateArray().First(x => x.GetProperty("id").GetString() == accent);
            if (a.GetProperty(t.GetProperty("kind").GetString()!).TryGetProperty(token, out var color)) return color.GetString()!;
        }
        return t.GetProperty("colors").GetProperty(token).GetString()!;
    }

    /// <summary>The app's roles (PleyColors) and the token each one is.</summary>
    static readonly (string Role, Func<PleyPalette, Rgba> Of, string Token)[] Roles =
    {
        ("Bg", p => p.Bg, "bg"),
        ("Surface", p => p.Surface, "surface"),
        ("Surface2", p => p.Surface2, "surface-2"),
        ("Surface3", p => p.Surface3, "surface-3"),
        ("Surface4", p => p.Surface4, "border"),
        ("Outline", p => p.Outline, "border-strong"),
        ("Control", p => p.Control, "subtle"),
        ("Fg", p => p.Fg, "fg"),
        ("Muted", p => p.Muted, "muted"),
        ("Subtle", p => p.Subtle, "subtle"),
        ("Accent", p => p.Accent, "accent"),
        ("AccentBright", p => p.AccentBright, "accent-hover"),
        ("OnAccent", p => p.OnAccent, "accent-fg"),
        ("BrandLight", p => p.BrandLight, "brand-light"),
        ("Brand", p => p.Brand, "brand"),
        ("BrandDeep", p => p.BrandDeep, "brand-deep"),
        ("BrandFar", p => p.BrandFar, "brand-far"),
        ("BrandTile", p => p.BrandTile, "brand-tile"),
        ("Success", p => p.Success, "success"),
        ("Danger", p => p.Danger, "danger"),
        ("DangerFg", p => p.DangerFg, "danger-fg"),
        ("Warning", p => p.Warning, "warning"),
        ("Info", p => p.Info, "info"),
        ("Shade", p => p.Shade, "shade"),
    };

    /// <summary>The washes the app draws: a token at a set strength.</summary>
    static readonly (string Role, Func<PleyPalette, Rgba> Of, string Token, float Alpha)[] Washes =
    {
        ("Disabled", p => p.Disabled, "subtle", PleyPalette.DisabledAlpha),
        ("OnArt", p => p.OnArt, "bg", 0.8f),
        ("TileHairline", p => p.TileHairline, "fg", 0.06f),
        ("Scrim", p => p.Scrim, "shade", PleyPalette.ScrimAlpha),
    };

    [Fact]
    public void The_themes_and_accents_are_the_files_in_its_order()
    {
        using var spec = Spec();
        Assert.Equal(spec.RootElement.GetProperty("themes").EnumerateArray().Select(t => t.GetProperty("id").GetString()), Themes.Ids);
        Assert.Equal(spec.RootElement.GetProperty("accents").EnumerateArray().Select(a => a.GetProperty("id").GetString()), Themes.AccentIds);
        Assert.Equal(new[] { "midnight", "midday", "twilight", "starlight", "moonlight", "nightshade", "greenlight", "sunshine", "blacklight" }, Themes.Ids);
        Assert.Equal(new[] { "sky", "violet", "miami", "gold", "mint", "amber" }, Themes.AccentIds);
        // Every color a theme sets is one the app knows and draws.
        foreach (var t in spec.RootElement.GetProperty("themes").EnumerateArray())
            Assert.Equal(t.GetProperty("colors").EnumerateObject().Select(c => c.Name).OrderBy(n => n, StringComparer.Ordinal), Themes.Tokens.OrderBy(n => n, StringComparer.Ordinal));
        Assert.Equal(Themes.Tokens.OrderBy(n => n, StringComparer.Ordinal), Roles.Select(r => r.Token).Distinct().OrderBy(n => n, StringComparer.Ordinal));
    }

    [Fact]
    public void Every_color_the_app_draws_is_the_files_in_every_theme_under_every_accent()
    {
        using var spec = Spec();
        var checkedCount = 0;
        foreach (var theme in Themes.Ids)
            foreach (var accent in new string?[] { null }.Concat(Themes.AccentIds))
            {
                var palette = PleyPalette.Of(theme, accent);
                foreach (var (role, of, token) in Roles)
                {
                    var expected = Rgba.Parse(ColorOf(spec.RootElement, theme, accent, token));
                    Assert.True(expected == of(palette), $"{theme} {accent ?? "(own)"}: {role} is {of(palette)}, themes.json's {token} is {expected}");
                    checkedCount++;
                }
                foreach (var (role, of, token, alpha) in Washes)
                {
                    var expected = Rgba.Parse(ColorOf(spec.RootElement, theme, accent, token))!.Value.Times(alpha);
                    Assert.True(expected == of(palette), $"{theme} {accent ?? "(own)"}: {role} is {of(palette)}, {token} at {alpha} is {expected}");
                }
            }
        Assert.Equal(9 * 7 * Roles.Length, checkedCount);
    }

    [Fact]
    public void A_fresh_TV_looks_as_the_app_always_has()
    {
        var p = PleyPalette.Of(ThemePick.Default);
        Assert.Equal("midnight", p.ThemeId);
        Assert.Null(p.AccentId);
        Assert.Equal("#0b0d10", p.Bg.ToString());
        Assert.Equal("#38bdf8", p.Accent.ToString());
        Assert.Equal("#7dd3fc", p.AccentBright.ToString());
        Assert.Equal("#03131f", p.OnAccent.ToString());
        Assert.Equal("#e9ebef", p.Fg.ToString());
    }

    [Fact]
    public void Players_stay_dark_in_the_twins_colors_with_the_same_accent()
    {
        using var spec = Spec();
        foreach (var t in spec.RootElement.GetProperty("themes").EnumerateArray())
        {
            var id = t.GetProperty("id").GetString()!;
            foreach (var accent in new string?[] { null, "violet", "amber" })
            {
                var player = PleyPalette.Of(id, accent).Player;
                Assert.True(player.Dark, $"{id}'s player is light");
                Assert.Equal(accent, player.AccentId);
                if (t.GetProperty("kind").GetString() == "dark") Assert.Same(PleyPalette.Of(id, accent), player);
                else Assert.Equal(t.GetProperty("twin").GetString(), player.ThemeId);
            }
        }
        Assert.Equal("moonlight", PleyPalette.Of("starlight", "sky").Player.ThemeId);
        Assert.Equal(Rgba.Parse("#38bdf8"), PleyPalette.Of("starlight", "sky").Player.Accent);
    }

    [Fact]
    public void An_accent_colors_what_the_theme_follows_and_nothing_else()
    {
        // Sunshine keeps its toy-box logo under any accent; Blacklight's states stay gray.
        Assert.Equal(PleyPalette.Of("sunshine", null).Brand, PleyPalette.Of("sunshine", "mint").Brand);
        Assert.NotEqual(PleyPalette.Of("sunshine", null).Accent, PleyPalette.Of("sunshine", "mint").Accent);
        Assert.Equal(PleyPalette.Of("blacklight", null).Success, PleyPalette.Of("blacklight", "mint").Success);
        Assert.Equal(Rgba.Parse("#b39afd"), PleyPalette.Of("blacklight", "violet").Accent);
        // A light theme takes an accent's deeper, light set.
        Assert.Equal(Rgba.Parse("#5a2ff0"), PleyPalette.Of("midday", "violet").Accent);
        // The surfaces and the words stay the theme's.
        Assert.Equal(PleyPalette.Of("twilight", null).Bg, PleyPalette.Of("twilight", "gold").Bg);
    }

    [Theory]
    [InlineData(null, null, "midnight", null)]
    [InlineData("sunshine", "violet", "sunshine", "violet")]
    [InlineData("daylight", "rose", "midnight", null)]
    [InlineData("", "", "midnight", null)]
    [InlineData("Midday", "Sky", "midnight", null)]
    public void An_id_the_app_doesnt_know_shows_Midnight_or_the_themes_own(string? theme, string? accent, string shown, string? accentShown)
    {
        Assert.Equal(new ThemePick(shown, accentShown), ThemePick.Of(theme, accent));
        Assert.Equal(shown, PleyPalette.Of(theme, accent).ThemeId);
    }

    [Theory]
    [InlineData("#38bdf8", 0x38, 0xbd, 0xf8, 1f)]
    [InlineData("rgb(16 26 44 / 0.42)", 16, 26, 44, 0.42f)]
    [InlineData("rgb(58 30 100/0.34)", 58, 30, 100, 0.34f)]
    [InlineData("rgb(1 2 3)", 1, 2, 3, 1f)]
    public void Colors_read_as_the_file_writes_them(string text, int r, int g, int b, float a) =>
        Assert.Equal(new Rgba((byte)r, (byte)g, (byte)b, a), Rgba.Parse(text));

    [Theory]
    [InlineData("#38bdf")]
    [InlineData("rgb(300 0 0)")]
    [InlineData("rgb(1 2 3 / 2)")]
    [InlineData("red")]
    public void Anything_else_isnt_a_color(string text) => Assert.Null(Rgba.Parse(text));

    [Fact]
    public void The_logo_is_drawn_in_the_themes_colors_from_the_webs_shapes()
    {
        var p = PleyPalette.Of("twilight", "gold");
        var svg = LogoMark.Svg(p);
        Assert.Contains($"stop-color=\"{p.BrandLight}\"", svg);
        Assert.Contains($"stop-color=\"{p.Brand}\"", svg);
        Assert.Contains($"<rect width=\"64\" height=\"64\" rx=\"15\" fill=\"{p.BrandTile}\"/>", svg);
        Assert.Contains($"<path fill=\"{p.BrandFar}\" d=\"{LogoMark.Far}\"/>", svg);
        Assert.Contains($"<path fill=\"{p.BrandDeep}\" d=\"{LogoMark.Deep}\"/>", svg);
        Assert.Contains($"d=\"{LogoMark.Front}\"", svg);
        // The web's Logo.tsx draws the same shapes.
        var web = File.ReadAllText(Path.Combine(AppContext.BaseDirectory, "themes", "Logo.tsx"));
        foreach (var shape in new[] { LogoMark.Far, LogoMark.Deep, LogoMark.Front }) Assert.Contains(shape, web);
        // A file per set of colors: the same colors, the same name; a theme with other colors, another.
        Assert.Equal(LogoMark.FileName(p), LogoMark.FileName(PleyPalette.Of("twilight", "gold")));
        Assert.NotEqual(LogoMark.FileName(p), LogoMark.FileName(PleyPalette.Of("twilight", null)));
        Assert.Matches("^pley-mark-[0-9a-f]+\\.svg$", LogoMark.FileName(p));
    }

    [Fact]
    public void Appearance_offers_the_nine_themes_then_Default_and_the_accents()
    {
        var pick = new ThemePick("starlight", "mint");
        var themes = Appearance.Themes(pick);
        Assert.Equal(Themes.Ids, themes.Select(t => t.Id));
        Assert.Equal("starlight", Assert.Single(themes, t => t.Checked).Id);
        // Each preview under the accent picked, as the web's take the page's.
        Assert.All(themes, t => Assert.Equal(PleyPalette.Of(t.Id, "mint"), t.Palette));
        Assert.Equal("Twilight", themes[2].Name);
        Assert.Equal("Sunset, no sunburn", themes[2].About);
        var accents = Appearance.Accents(pick);
        Assert.Equal(new string?[] { null, "sky", "violet", "miami", "gold", "mint", "amber" }, accents.Select(a => a.Id));
        Assert.Equal(new[] { "Default", "Sky", "Violet", "Miami", "Gold", "Mint", "Amber" }, accents.Select(a => a.Name));
        Assert.Equal("mint", Assert.Single(accents, a => a.Checked).Id);
        // Each swatch in the theme shown: Default is Starlight's own bronze.
        Assert.Equal(Rgba.Parse("#835613"), accents[0].Palette.Accent);
        Assert.True(Appearance.Accents(new ThemePick("midnight", null))[0].Checked);
    }

    [Fact]
    public void Every_theme_and_accent_has_its_words_in_every_language()
    {
        foreach (var language in Languages.All)
        {
            var words = Strings.In(language);
            foreach (var id in Themes.Ids)
            {
                Assert.NotEqual($"settings.appearance.names.{id}", words.T($"settings.appearance.names.{id}"));
                Assert.NotEqual($"settings.appearance.about.{id}", words.T($"settings.appearance.about.{id}"));
            }
            foreach (var id in Themes.AccentIds) Assert.NotEqual($"settings.appearance.accents.{id}", words.T($"settings.appearance.accents.{id}"));
        }
    }

    [Fact]
    public void The_words_are_the_webs()
    {
        foreach (var language in Languages.All)
        {
            using var web = JsonDocument.Parse(File.ReadAllText(Path.Combine(AppContext.BaseDirectory, "themes", "pages", language, "pages.json")));
            var theme = web.RootElement.GetProperty("account").GetProperty("theme");
            var words = Strings.In(language);
            // Apostrophes curl in French and Spanish here (CatalogTests).
            string Same(string s) => language is "fr" or "es" ? s.Replace('\'', '’') : s;
            foreach (var p in theme.GetProperty("names").EnumerateObject()) Assert.Equal(Same(p.Value.GetString()!), words.T($"settings.appearance.names.{p.Name}"));
            foreach (var p in theme.GetProperty("about").EnumerateObject()) Assert.Equal(Same(p.Value.GetString()!), words.T($"settings.appearance.about.{p.Name}"));
            foreach (var p in theme.GetProperty("accent").GetProperty("names").EnumerateObject()) Assert.Equal(Same(p.Value.GetString()!), words.T($"settings.appearance.accents.{p.Name}"));
            Assert.Equal(Same(theme.GetProperty("accent").GetProperty("own").GetString()!), words.T("settings.appearance.own"));
            Assert.Equal(Same(theme.GetProperty("title").GetString()!), words.T("settings.appearance.theme"));
            Assert.Equal(Same(theme.GetProperty("accent").GetProperty("title").GetString()!), words.T("settings.appearance.accent"));
            Assert.Equal(Same(theme.GetProperty("accent").GetProperty("hint").GetString()!), words.T("settings.appearance.accentHint"));
        }
    }
}

/// <summary>
/// The pick is the TV's own for each profile, kept under the web apps' keys and never sent to the
/// server; outside a profile (the servers, signing in, Who's watching?) the app is in Midnight.
/// </summary>
public class ThemePickTests
{
    const string Server = "https://pley.example.com";

    static AppState State(MemoryKeyValue store, FakeServer? server = null) =>
        new AppState(new AppPlatform("tizen", "client", store) { Version = "0.1.0", OsVersion = "9.0", Model = "QE55" }, (server ?? new FakeServer()).Transport,
            (_, _, _, _, _) => Task.FromResult(new EventEnd(0, false)), new FakeScheduler());

    static User Profile(long id) => new User { Id = id, Name = "P" + id };

    /// <summary>Signed in to <see cref="Server"/>, inside profile 1's pages.</summary>
    static AppState Inside(MemoryKeyValue store)
    {
        var state = State(store);
        state.Use(Server);
        state.ShowingOutside(false);
        state.Me.Value = Profile(1);
        return state;
    }

    [Fact]
    public void A_TV_that_picked_none_shows_Midnight_in_its_own_accent()
    {
        Assert.Equal(ThemePick.Default, State(new MemoryKeyValue()).Theme.Value);
        Assert.Equal(new ThemePick("midnight", null), Inside(new MemoryKeyValue()).Theme.Value);
    }

    [Fact]
    public void Each_profile_keeps_its_own_pick_on_the_TV_and_switching_shows_it()
    {
        var store = new MemoryKeyValue();
        var state = Inside(store);
        var heard = new List<ThemePick>();
        state.Theme.Changed += heard.Add;
        state.ChooseTheme("starlight");
        state.ChooseAccent("violet");
        Assert.Equal(new[] { new ThemePick("starlight", null), new ThemePick("starlight", "violet") }, heard);
        // A profile new to this TV: Midnight in its own accent.
        state.Me.Value = Profile(2);
        Assert.Equal(ThemePick.Default, state.Theme.Value);
        state.ChooseTheme("twilight");
        state.Me.Value = Profile(1);
        Assert.Equal(new ThemePick("starlight", "violet"), state.Theme.Value);
        state.Me.Value = Profile(2);
        Assert.Equal(new ThemePick("twilight", null), state.Theme.Value);
        // Default forgets the accent.
        state.ChooseAccent("gold");
        state.ChooseAccent(null);
        Assert.Equal(new ThemePick("twilight", null), state.Theme.Value);
        // Nothing of it goes to the server: the TV keeps it, by server and profile.
        Assert.Equal("starlight", store.Get($"pley.theme.{Server}.1"));
        Assert.Equal("violet", store.Get($"pley.accent.{Server}.1"));
        Assert.Equal("twilight", store.Get($"pley.theme.{Server}.2"));
        Assert.Null(store.Get($"pley.accent.{Server}.2"));
    }

    [Fact]
    public void Outside_a_profile_the_app_is_in_Midnight_and_the_profiles_pick_comes_back_inside()
    {
        var state = Inside(new MemoryKeyValue());
        state.ChooseTheme("sunshine");
        state.ChooseAccent("mint");
        // Who's watching?, the servers, signing in: Midnight in its own accent, whoever was watching.
        state.ShowingOutside(true);
        Assert.Equal(ThemePick.Default, state.Theme.Value);
        // A pick can't be made there: there is no profile to keep it for.
        state.ChooseTheme("blacklight");
        Assert.Equal(ThemePick.Default, state.Theme.Value);
        // Back inside the profile (the same one chosen again): its pick.
        state.ShowingOutside(false);
        Assert.Equal(new ThemePick("sunshine", "mint"), state.Theme.Value);
        // Signed out: Midnight again.
        state.Me.Value = null;
        Assert.Equal(ThemePick.Default, state.Theme.Value);
    }

    [Fact]
    public async Task A_start_with_the_profile_saved_is_in_its_pick_from_the_first_page()
    {
        var store = new MemoryKeyValue();
        var scheduler = new FakeScheduler();
        var server = new FakeServer();
        server.On("GET", Server + "/api/auth/me", 200, @"{""id"":1,""username"":""a"",""name"":""A""}");
        server.On("GET", Server + "/api/libraries", 200, "[]");
        var state = State(store, server);
        state.Use(Server);
        await state.TokenStore!.Install(new StoredTokens(Server, "pley_at_0", "pley_rt_0", scheduler.Now + 3_600_000, "library play", Server + "/t", Server + "/r"), "library play");
        await state.LoadAccount();
        state.ShowingOutside(false);
        state.ChooseTheme("greenlight");
        state.ChooseAccent("amber");
        // The next start knows who was watching before the server answers: their pick, with no Midnight first.
        var next = State(store);
        Assert.Equal(new ThemePick("greenlight", "amber"), next.Theme.Value);
        // One that starts on Who's watching? draws it in Midnight.
        next.ShowingOutside(true);
        Assert.Equal(ThemePick.Default, next.Theme.Value);
        // Without a profile saved (signed out), the start is in Midnight.
        store.Remove("pley.server");
        Assert.Equal(ThemePick.Default, State(store).Theme.Value);
    }

    [Fact]
    public void A_profile_known_again_with_the_same_pick_changes_nothing()
    {
        var store = new MemoryKeyValue();
        var state = Inside(store);
        state.ChooseTheme("greenlight");
        var heard = 0;
        state.Theme.Changed += _ => heard++;
        store.Written.Clear();
        // Who is watching heard again from the server: no redraw, nothing written.
        state.Me.Value = Profile(1) with { Name = "Renamed" };
        Assert.Equal(0, heard);
        Assert.Empty(store.Written);
    }

    [Fact]
    public void Ids_kept_that_the_app_doesnt_know_show_Midnight_and_the_themes_own()
    {
        var store = new MemoryKeyValue();
        store.Set($"pley.theme.{Server}.1", "daylight");
        store.Set($"pley.accent.{Server}.1", "rose");
        Assert.Equal(ThemePick.Default, Inside(store).Theme.Value);
        store.Set($"pley.theme.{Server}.1", "twilight");
        Assert.Equal(new ThemePick("twilight", null), Inside(store).Theme.Value);
    }
}

/// <summary>
/// The pick drawn and kept, as the web TV app draws and keeps it: packages/tv/test/fixtures/appearance.json,
/// which both Samsung apps answer alike.
/// </summary>
public class AppearanceFixtureTests
{
    static readonly JsonElement Fixture = Fixtures.Load("appearance");

    static AppState State(MemoryKeyValue store) =>
        new AppState(new AppPlatform("tizen", "client", store) { Version = "0.1.0", OsVersion = "9.0", Model = "QE55" }, new FakeServer().Transport,
            (_, _, _, _, _) => Task.FromResult(new EventEnd(0, false)), new FakeScheduler());

    static MemoryKeyValue Kept(JsonElement c)
    {
        var store = new MemoryKeyValue();
        foreach (var kept in c.GetProperty("kept").EnumerateObject()) store.Values[kept.Name] = kept.Value.GetString()!;
        return store;
    }

    /// <summary>A case's pick, as the fixture writes it.</summary>
    static ThemePick Pick(JsonElement pick) => new ThemePick(pick.GetProperty("theme").GetString()!, pick.GetProperty("accent").GetString());

    /// <summary>The TV signed in to the case's profile's server, its pages or a screen outside a profile on, that profile watching once known.</summary>
    static AppState Showing(MemoryKeyValue store, JsonElement profile, bool outside)
    {
        var state = State(store);
        if (profile.ValueKind != JsonValueKind.Null) state.Use(profile.GetProperty("server").GetString()!);
        state.ShowingOutside(outside);
        state.Me.Value = profile.ValueKind == JsonValueKind.Null ? null : new User { Id = profile.GetProperty("id").GetInt64(), Name = "P" };
        return state;
    }

    [Fact]
    public void It_is_Midnight_outside_a_profile_and_the_profiles_own_inside_it_once_known()
    {
        foreach (var c in Fixture.GetProperty("shown").EnumerateArray())
        {
            var state = Showing(Kept(c), c.GetProperty("profile"), c.GetProperty("outside").GetBoolean());
            Assert.Equal((c.GetProperty("name").GetString(), Pick(c.GetProperty("out"))), (c.GetProperty("name").GetString(), state.Theme.Value));
        }
    }

    /// <summary>The fixture's screens as this app's pages; the player isn't one here (it opens over a page, in the dark twin of its pick).</summary>
    static IEnumerable<PageKey> Pages(string screen) => screen switch
    {
        "servers" => new PageKey[] { new PageKey.Servers() },
        "link" => new PageKey[] { new PageKey.Link("http://pley.local:7539") },
        "profiles" => new PageKey[] { new PageKey.Profiles(), new PageKey.Profiles(Pin: true) },
        "home" => new PageKey[] { new PageKey.Home() },
        "settings" => new PageKey[] { new PageKey.Settings(), new AppearancePage(), new AppearancePage(Accents: true) },
        "player" => Array.Empty<PageKey>(),
        _ => throw new ArgumentException($"appearance.json names a screen this test doesn't know: {screen}"),
    };

    [Fact]
    public void It_knows_the_screens_outside_a_profile()
    {
        foreach (var c in Fixture.GetProperty("screens").EnumerateArray())
        {
            var outside = c.GetProperty("outside").GetBoolean();
            foreach (var page in Pages(c.GetProperty("screen").GetString()!)) Assert.Equal((page, outside), (page, page.OutsideProfile));
        }
    }

    [Fact]
    public void It_is_kept_for_the_profile_alone()
    {
        foreach (var c in Fixture.GetProperty("picks").EnumerateArray())
        {
            var store = Kept(c);
            var state = Showing(store, c.GetProperty("profile"), outside: false);
            var pick = c.GetProperty("pick");
            state.ChooseTheme(pick.GetProperty("theme").GetString()!);
            state.ChooseAccent(pick.GetProperty("accent").GetString());
            // What is kept of the picks, "key=value" in the keys' order.
            var picks = string.Join("; ", store.Values.Where(v => v.Key.StartsWith("pley.theme.", StringComparison.Ordinal) || v.Key.StartsWith("pley.accent.", StringComparison.Ordinal))
                .OrderBy(v => v.Key, StringComparer.Ordinal).Select(v => $"{v.Key}={v.Value}"));
            var expected = string.Join("; ", c.GetProperty("out").EnumerateObject().OrderBy(p => p.Name, StringComparer.Ordinal).Select(p => $"{p.Name}={p.Value.GetString()}"));
            Assert.Equal((c.GetProperty("name").GetString(), expected), (c.GetProperty("name").GetString(), picks));
        }
    }
}
