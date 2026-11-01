using System;
using System.Collections.Generic;
using System.Linq;
using System.Threading;
using System.Threading.Tasks;
using Pley.Tv.Core.Api;
using Pley.Tv.Core.Auth;
using Pley.Tv.Core.Net;
using Xunit;

namespace Pley.Tv.Core.Tests;

/// <summary>What the TV keeps, and how it follows the server.</summary>
public class StateTests
{
    const string Home = "https://pley.example.com";
    const string MeJson = @"{""id"":1,""username"":""a"",""name"":""A"",""subtitle_mode"":""off""}";

    static AppState State(MemoryKeyValue store, FakeServer server, FakeScheduler scheduler, EventConnect? connect = null) =>
        new AppState(new AppPlatform("tizen", "client", store) { Version = "0.1.0", OsVersion = "9.0", Model = "QE55" }, server.Transport,
            connect ?? ((_, _, _, _, _) => Task.FromResult(new EventEnd(0, false))), scheduler);

    static void SignIn(AppState state, MemoryKeyValue store, FakeScheduler scheduler)
    {
        state.Use(Home);
        state.TokenStore!.Install(new StoredTokens(Home, "pley_at_0", "pley_rt_0", scheduler.Now + 3_600_000, "library play", Home + "/t", Home + "/r"), "library play").Wait();
    }

    static string Hubs(string title) => $@"{{""hubs"":[{{""key"":""k"",""title"":""{title}"",""items"":[]}}]}}";

    [Fact]
    public void Starts_on_the_first_screen_the_TV_kept_for()
    {
        var store = new MemoryKeyValue();
        var scheduler = new FakeScheduler();
        var state = State(store, new FakeServer(), scheduler);
        Assert.Equal(StartPage.Servers, state.Start());
        state.Use(Home + "/");
        Assert.Equal(Home, state.Server.Value);
        Assert.Equal(StartPage.Link, state.Start());
        SignIn(state, store, scheduler);
        Assert.Equal(StartPage.Home, state.Start());
        state.KeepHousehold(new[] { new HouseholdProfile { Id = 1, Name = "A" }, new HouseholdProfile { Id = 2, Name = "B" } });
        Assert.Equal(StartPage.Profiles, state.Start());
        state.SetAlwaysStartAs(2);
        Assert.Equal(StartPage.Home, state.Start());
        // A new launch reads the same from the store.
        Assert.Equal(StartPage.Home, State(store, new FakeServer(), scheduler).Start());
        // "Always start as" is the TV's, whichever server it uses.
        Assert.Equal(2, State(store, new FakeServer(), scheduler).AlwaysStartAs());
        Assert.Equal("2", store.Get("pley.alwaysStartAs"));
    }

    [Fact]
    public void Lists_servers_most_recently_used_first_and_keeps_an_install_id()
    {
        var store = new MemoryKeyValue();
        var scheduler = new FakeScheduler();
        var state = State(store, new FakeServer(), scheduler);
        state.RememberServer("http://nas:7539", "NAS", "a");
        scheduler.Advance(1000);
        state.RememberServer(Home + "/", "Home");
        scheduler.Advance(1000);
        // The same server, found at a new address by its id, replaces its row.
        state.RememberServer("http://192.168.1.20:7539/", "NAS again", "a");
        Assert.Equal(new[] { ("http://192.168.1.20:7539", "NAS again", "a"), (Home, "Home", (string?)null) }, state.SavedServers().Select(s => (s.Url, s.Name, s.Id)));
        // As many as are used: none drop off.
        for (var i = 0; i < 25; i++) state.RememberServer($"http://10.0.0.{i + 1}:7539", "S");
        Assert.Equal(27, state.SavedServers().Count);
        var id = state.InstallId();
        Assert.Matches("^tizen-[0-9a-f]{32}$", id);
        Assert.Equal(id, State(store, new FakeServer(), scheduler).InstallId());
    }

    [Fact]
    public void Keeps_the_android_apps_settings_as_they_may_be()
    {
        var store = new MemoryKeyValue();
        var scheduler = new FakeScheduler();
        var state = State(store, new FakeServer(), scheduler);
        var defaults = state.Settings.Value;
        Assert.Equal(("auto", 0.048f, SubtitleBackground.None, SubtitleFont.Sans, 0f), (defaults.Quality, defaults.SubtitleSize, defaults.SubtitleBackground, defaults.SubtitleFont, defaults.SubtitleRaise));
        Assert.Equal((FrameRateMatching.Auto, false, true, false, false, GridDensity.Medium),
            (defaults.FrameRateMatching, defaults.Tunneling, defaults.AutoplayNext, defaults.ReduceMotion, defaults.PlaybackDetails, defaults.GridDensity));
        Assert.All(Settings.PassthroughFormats, f => Assert.Equal(PassthroughMode.Auto, defaults.PassthroughOf(f)));
        state.UpdateSettings(s => s with
        {
            SubtitleSize = 0.07f,
            SubtitleRaise = 0.11f,
            SubtitleFont = SubtitleFont.Casual,
            Passthrough = new Dictionary<string, PassthroughMode> { ["truehd"] = PassthroughMode.On, ["mp3"] = PassthroughMode.On },
        });
        var kept = State(store, new FakeServer(), scheduler).Settings.Value;
        Assert.Equal((0.076f, 0.1f, SubtitleFont.Casual), (kept.SubtitleSize, kept.SubtitleRaise, kept.SubtitleFont));
        Assert.Equal((PassthroughMode.On, PassthroughMode.Auto), (kept.PassthroughOf("truehd"), kept.PassthroughOf("mp3")));
        // The web app's settings, saved before, aren't these: the defaults stand.
        store.Set("pley.settings", "{\"quality\":\"original\",\"subtitleSize\":\"large\"}");
        Assert.Equal("auto", State(store, new FakeServer(), scheduler).Settings.Value.Quality);
    }

    [Fact]
    public async Task Shows_what_was_saved_for_each_profile_and_keeps_the_last_three()
    {
        var store = new MemoryKeyValue();
        var scheduler = new FakeScheduler();
        var server = new FakeServer();
        var me = 1;
        server.On("GET", Home + "/api/auth/me", _ => (200, $@"{{""id"":{me},""username"":""u{me}"",""name"":""U{me}""}}"));
        server.On("GET", Home + "/api/libraries", 200, "[]");
        server.On("GET", Home + "/api/oauth/profiles", 200, "[]");
        server.On("GET", Home + "/api/hubs/home", _ => (200, Hubs($"Home of {me}")));
        var state = State(store, server, scheduler);
        SignIn(state, store, scheduler);
        await state.LoadAccount();
        await state.LoadHome();
        Assert.Equal("Home of 1", state.Home.Value!.Single().Title);
        // Another profile: its own Home, saved apart.
        me = 2;
        await state.Switched(2);
        await Task.Delay(50);
        Assert.Equal("Home of 2", state.Home.Value!.Single().Title);
        // Back to the first: its saved Home shows at once, before the server answers.
        server.On("GET", Home + "/api/hubs/home", _ => (0, null));
        me = 1;
        var switching = state.Switched(1);
        Assert.Equal("Home of 1", state.Home.Value!.Single().Title);
        await switching;
        // A new launch shows the last profile's.
        Assert.Equal("Home of 1", State(store, new FakeServer(), scheduler).Home.Value!.Single().Title);
        // Three profiles keep theirs; a fourth's takes the oldest one's place.
        server.On("GET", Home + "/api/hubs/home", _ => (200, Hubs($"Home of {me}")));
        foreach (var next in new[] { 3, 4 })
        {
            me = next;
            await state.Switched(next);
            await Task.Delay(50);
        }
        Assert.Contains($"pley.home.{Home}.4", store.Values.Keys);
        Assert.Contains($"pley.home.{Home}.1", store.Values.Keys);
        Assert.DoesNotContain($"pley.home.{Home}.2", store.Values.Keys);
        // Signing out forgets them all.
        await state.SignOutNow();
        Assert.DoesNotContain(store.Values.Keys, k => k.StartsWith("pley.home."));
    }

    [Fact]
    public async Task Signing_out_after_signing_in_again_to_the_same_server_is_heard()
    {
        var store = new MemoryKeyValue();
        var scheduler = new FakeScheduler();
        var state = State(store, new FakeServer(), scheduler);
        var heard = 0;
        state.SignedOut.Changed += signedOut => heard += signedOut ? 1 : 0;
        SignIn(state, store, scheduler);
        await state.SignOutNow();
        SignIn(state, store, scheduler);
        Assert.False(state.SignedOut.Value);
        await state.SignOutNow();
        Assert.Equal(2, heard);
    }

    [Fact]
    public async Task A_profile_switch_the_server_made_stands_when_the_account_then_fails_to_load()
    {
        var store = new MemoryKeyValue();
        var scheduler = new FakeScheduler();
        var server = new FakeServer();
        server.On("POST", Home + "/t", 200, @"{""access_token"":""pley_at_2"",""token_type"":""Bearer"",""expires_in"":3600,""refresh_token"":""pley_rt_2"",""scope"":""library play""}");
        server.On("GET", Home + "/api/auth/me", 500, @"{""error"":""internal""}");
        server.On("GET", Home + "/api/libraries", 200, "[]");
        server.On("GET", Home + "/api/hubs/home", 200, @"{""hubs"":[]}");
        var state = State(store, server, scheduler);
        SignIn(state, store, scheduler);
        await new Pley.Tv.Core.Ui.AppProfilesSource(state).SwitchTo(2, null);
        Assert.Equal("pley_at_2", state.TokenStore!.Load()!.AccessToken);
        // Kept for the pages' Retry.
        Assert.NotNull(state.AccountError.Value);
    }

    [Fact]
    public async Task The_same_home_again_is_neither_shown_nor_saved_again()
    {
        var store = new MemoryKeyValue();
        var scheduler = new FakeScheduler();
        var server = new FakeServer();
        var title = "Continue watching";
        server.On("GET", Home + "/api/auth/me", 200, MeJson);
        server.On("GET", Home + "/api/libraries", 200, "[]");
        server.On("GET", Home + "/api/oauth/profiles", 200, "[]");
        server.On("GET", Home + "/api/hubs/home", _ => (200, Hubs(title)));
        var state = State(store, server, scheduler);
        SignIn(state, store, scheduler);
        await state.LoadAccount();
        await state.LoadHome();
        var key = $"pley.home.{Home}.1";
        var shown = state.Home.Value;
        var changes = 0;
        state.Home.Changed += _ => changes++;
        store.Written.Clear();
        await state.LoadHome();
        Assert.Same(shown, state.Home.Value);
        Assert.Equal(0, changes);
        Assert.DoesNotContain(key, store.Written);
        // After a launch, from what was saved: the same.
        var again = State(store, server, scheduler);
        shown = again.Home.Value;
        await again.LoadAccount();
        await again.LoadHome();
        Assert.Same(shown, again.Home.Value);
        Assert.DoesNotContain(key, store.Written);
        // Something changed: shown and saved.
        title = "Recently added";
        await again.LoadHome();
        Assert.Equal("Recently added", again.Home.Value!.Single().Title);
        Assert.Contains(key, store.Written);
    }

    [Fact]
    public async Task Shows_the_notice_for_6_s_when_the_server_goes_and_comes_back_when_its_events_connect()
    {
        var store = new MemoryKeyValue();
        var scheduler = new FakeScheduler();
        var server = new FakeServer();
        Action? open = null;
        var state = State(store, server, scheduler, (url, headers, onOpen, onText, cancel) =>
        {
            open = onOpen;
            return new TaskCompletionSource<EventEnd>().Task;
        });
        SignIn(state, store, scheduler);
        await state.LoadHome();
        Assert.True(state.Offline.Value);
        Assert.True(state.OfflineNotice.Value);
        Assert.Equal("Nothing answers there.", Errors.UnreachableReason(state.HomeError.Value!));
        scheduler.Advance(6_000);
        Assert.False(state.OfflineNotice.Value);
        Assert.True(state.Offline.Value);
        // Nothing asks the server meanwhile: its events, reconnecting by themselves, say when it is back.
        Assert.DoesNotContain(server.Requests, r => r.Url.EndsWith("/api/server"));
        server.On("GET", Home + "/api/hubs/home", 200, @"{""hubs"":[]}");
        server.On("GET", Home + "/api/auth/me", 200, MeJson);
        server.On("GET", Home + "/api/libraries", 200, "[]");
        server.On("GET", Home + "/api/oauth/profiles", 200, "[]");
        state.Listen(true);
        await Task.Delay(50);
        open!();
        await Task.Delay(50);
        Assert.False(state.Offline.Value);
        Assert.Empty(state.Home.Value!);
    }

    [Fact]
    public async Task A_refusal_is_an_answer_not_the_server_away()
    {
        var store = new MemoryKeyValue();
        var scheduler = new FakeScheduler();
        var server = new FakeServer().On("GET", Home + "/api/hubs/home", 502, "<html>Bad gateway</html>");
        var state = State(store, server, scheduler);
        SignIn(state, store, scheduler);
        await state.LoadHome();
        Assert.NotNull(state.HomeError.Value);
        Assert.False(state.Offline.Value);
    }

    [Fact]
    public void Keeps_the_last_50_item_details_and_says_how_old_they_are()
    {
        var store = new MemoryKeyValue();
        var scheduler = new FakeScheduler();
        var state = State(store, new FakeServer(), scheduler);
        for (var id = 1; id <= 50; id++) state.KeepItem(new ItemDetail { Id = id });
        // Read: last to go.
        Assert.NotNull(state.CachedItem(1));
        state.KeepItem(new ItemDetail { Id = 51 });
        Assert.Null(state.CachedItem(2));
        Assert.NotNull(state.CachedItem(1));
        Assert.False(state.CachedItemStale(51));
        scheduler.Advance(30_000);
        // No longer fresh, but kept: shown while the page asks again.
        Assert.Equal(30_000, state.CachedItemAge(51));
        Assert.True(state.CachedItemStale(51));
        scheduler.Advance(10 * 60_000);
        Assert.NotNull(state.CachedItem(51));
        Assert.True(state.CachedItemStale(2));
    }

    [Fact]
    public async Task Gathers_watch_state_changes_and_asks_again_once()
    {
        var store = new MemoryKeyValue();
        var scheduler = new FakeScheduler();
        var server = new FakeServer();
        var homes = 0;
        server.On("GET", Home + "/api/hubs/home", _ =>
        {
            homes++;
            return (200, @"{""hubs"":[]}");
        });
        Action<string>? push = null;
        Action? open = null;
        IReadOnlyDictionary<string, string>? sent = null;
        var state = State(store, server, scheduler, (url, headers, onOpen, onText, cancel) =>
        {
            push = onText;
            open = onOpen;
            sent = headers;
            return new TaskCompletionSource<EventEnd>().Task;
        });
        SignIn(state, store, scheduler);
        var heard = new List<string>();
        state.OnEvent(e => heard.Add(e.Type));
        state.Listen(true);
        await Task.Delay(50);
        Assert.Equal(("Bearer pley_at_0", "pleyTizen/0.1.0 (SMART-TV; Tizen 9.0; QE55)"), (sent!["Authorization"], sent["User-Agent"]));
        open!();
        await Task.Delay(50);
        state.KeepItem(new ItemDetail { Id = 5, ParentId = 7, GrandparentId = 3 });
        state.KeepItem(new ItemDetail { Id = 9 });
        var homesAfterConnecting = homes;
        push!("data: {\"type\":\"user_state_changed\",\"item_ids\":[7,3],\"children\":true}\n\n");
        push!("data: {\"type\":\"user_state_changed\",\"item_ids\":[8]}\n\ndata: {\"type\":\"sessions_");
        push!("changed\"}\n\n");
        Assert.Equal(new[] { "user_state_changed", "user_state_changed", "sessions_changed" }, heard);
        scheduler.Advance(300);
        await Task.Delay(50);
        // Everything under item 7, the first named with `children`, goes; item 9 stays; Home is asked again once.
        Assert.Null(state.CachedItem(5));
        Assert.NotNull(state.CachedItem(9));
        Assert.Equal(homesAfterConnecting + 1, homes);
    }

    [Fact]
    public async Task A_profile_switch_starts_the_events_over_and_only_when_they_are_followed()
    {
        var store = new MemoryKeyValue();
        var scheduler = new FakeScheduler();
        var server = new FakeServer();
        server.On("GET", Home + "/api/auth/me", 200, MeJson);
        server.On("GET", Home + "/api/libraries", 200, "[]");
        server.On("GET", Home + "/api/oauth/profiles", 200, "[]");
        server.On("GET", Home + "/api/hubs/home", 200, @"{""hubs"":[]}");
        var connections = new List<CancellationToken>();
        var state = State(store, server, scheduler, (url, headers, onOpen, onText, cancel) =>
        {
            connections.Add(cancel);
            onOpen();
            return new TaskCompletionSource<EventEnd>().Task;
        });
        SignIn(state, store, scheduler);
        await state.Switched(2);
        Assert.Empty(connections);
        state.Listen(true);
        await Task.Delay(50);
        Assert.Single(connections);
        await state.Switched(3);
        await Task.Delay(50);
        Assert.Equal(2, connections.Count);
        Assert.True(connections[0].IsCancellationRequested);
        Assert.False(connections[1].IsCancellationRequested);
    }

    [Fact]
    public async Task Gathers_each_librarys_changes_apart_and_follows_its_scans()
    {
        var store = new MemoryKeyValue();
        var scheduler = new FakeScheduler();
        var server = new FakeServer();
        var homes = 0;
        var libraries = 0;
        var scanning = "true";
        server.On("GET", Home + "/api/hubs/home", _ =>
        {
            homes++;
            return (200, @"{""hubs"":[]}");
        });
        server.On("GET", Home + "/api/libraries", _ =>
        {
            libraries++;
            return (200, $@"[{{""id"":1,""name"":""Movies"",""kind"":""movies"",""scanning"":{scanning}}},{{""id"":2,""name"":""Shows"",""kind"":""shows""}}]");
        });
        Action<string>? push = null;
        var state = State(store, server, scheduler, (url, headers, onOpen, onText, cancel) =>
        {
            push = onText;
            onOpen();
            return new TaskCompletionSource<EventEnd>().Task;
        });
        SignIn(state, store, scheduler);
        state.Listen(true);
        await Task.Delay(50);
        scheduler.Advance(1);
        await Task.Delay(50);
        state.KeepItem(new ItemDetail { Id = 10, LibraryId = 1 });
        state.KeepItem(new ItemDetail { Id = 20, LibraryId = 2 });
        var (homesBefore, librariesBefore) = (homes, libraries);

        push!("data: {\"type\":\"scan_started\",\"library_id\":1}\n\n");
        push!("data: {\"type\":\"scan_progress\",\"library_id\":1,\"processed\":5,\"total\":20}\n\n");
        Assert.Equal(new[] { new ScanState(1, 0.25f) }, state.Scans.Value);
        push!("data: {\"type\":\"library_changed\",\"library_id\":1}\n\n");
        scheduler.Advance(1_000);
        push!("data: {\"type\":\"library_changed\",\"library_id\":1}\n\n");
        scheduler.Advance(500);
        push!("data: {\"type\":\"library_changed\",\"library_id\":2}\n\n");
        scheduler.Advance(500);
        // Library 1 changed 1 s ago, library 2 half a second ago: both are still gathering.
        Assert.Equal((homesBefore, librariesBefore), (homes, libraries));
        scheduler.Advance(500);
        await Task.Delay(50);
        // Library 1: once, 1.5 s after its last change; only its details go.
        Assert.Null(state.CachedItem(10));
        Assert.NotNull(state.CachedItem(20));
        Assert.Equal((homesBefore + 1, librariesBefore + 1), (homes, libraries));
        // The libraries' `scanning` keeps the scan on.
        Assert.Single(state.Scans.Value);
        scheduler.Advance(500);
        await Task.Delay(50);
        Assert.Null(state.CachedItem(20));
        Assert.Equal((homesBefore + 2, librariesBefore + 2), (homes, libraries));

        // The scan ends: Home is asked once more, whatever the events said.
        scanning = "false";
        push!("data: {\"type\":\"scan_finished\",\"library_id\":1}\n\n");
        await Task.Delay(50);
        Assert.Empty(state.Scans.Value);
        Assert.Equal(homesBefore + 3, homes);
    }

    [Fact]
    public void Reads_server_sent_events_split_anywhere_and_skips_those_it_cant_use()
    {
        var reader = new SseReader();
        Assert.Empty(reader.Feed("event: x\r\ndata: {\"type\":\"library_"));
        var events = reader.Feed("changed\",\"library_id\":4}\r\n\r\n: comment\n\ndata: not json\n\ndata: {\"type\":\"watchlist_changed\"}\n\n" +
            "data: {\"type\":\"something_new\"}\n\ndata: {\"type\":\"library_changed\"}\n\ndata: {\"type\":\"session_stopped\"}\n\n");
        Assert.Equal(new[] { ("library_changed", (long?)4), ("watchlist_changed", null) }, events.Select(e => (e.Type, e.LibraryId)));
    }

    /// <summary>An event stream whose every connection ends as <paramref name="end"/> says, at once or when the test says; opened first with <paramref name="open"/>.</summary>
    static async Task<(EventStream Stream, List<long> Connects, FakeScheduler Scheduler, FakeServer Server)> Stream(Func<int, Task<EventEnd>> end,
        string? refresh = null, bool open = false)
    {
        var scheduler = new FakeScheduler();
        var server = new FakeServer();
        if (refresh != null) server.On("POST", Home + "/t", 200, refresh);
        var tokens = new Tokens(Home, "client", new MemoryKeyValue(), server.Transport, null, () => scheduler.Now);
        await tokens.Install(new StoredTokens(Home, "a", "r", scheduler.Now + 3_600_000, "library", Home + "/t", Home + "/r"), "library");
        var connects = new List<long>();
        var stream = new EventStream(Home, tokens, _ => { }, () => { }, (url, headers, onOpen, onText, cancel) =>
        {
            connects.Add(scheduler.Now);
            if (open) onOpen();
            return end(connects.Count);
        }, scheduler);
        return (stream, connects, scheduler, server);
    }

    static Task<(EventStream Stream, List<long> Connects, FakeScheduler Scheduler, FakeServer Server)> Stream(Func<int, EventEnd> end, string? refresh = null) =>
        Stream(n => Task.FromResult(end(n)), refresh);

    static async Task Run(FakeScheduler scheduler, int steps, long ms = 30_000)
    {
        await Task.Delay(20);
        for (var i = 0; i < steps; i++)
        {
            scheduler.Advance(ms);
            await Task.Delay(20);
        }
    }

    [Fact]
    public async Task Reconnects_with_growing_waits()
    {
        var (stream, connects, scheduler, _) = await Stream(_ => new EventEnd(0, false));
        stream.Start();
        await Run(scheduler, 6);
        var waits = connects.Zip(connects.Skip(1), (a, b) => b - a).ToList();
        Assert.Equal(new long[] { 2_000, 4_000, 8_000, 16_000, 30_000 }, waits.Take(5));
        stream.Stop();
    }

    [Theory]
    [InlineData(true, 0)]
    [InlineData(false, 2_000)]
    public async Task Reconnects_at_once_only_after_a_long_stream_the_server_ended(bool clean, long wait)
    {
        var first = new TaskCompletionSource<EventEnd>();
        var (stream, connects, scheduler, _) = await Stream(n => n == 1 ? first.Task : new TaskCompletionSource<EventEnd>().Task, open: true);
        stream.Start();
        await Run(scheduler, 1, 31_000);
        // Ended by the server after 31 s (its token expired): at once. Gone silent or failed: as any failure.
        first.SetResult(new EventEnd(200, clean));
        await Run(scheduler, 1, wait);
        Assert.Equal(new long[] { 0, 31_000 + wait }, connects.Select(c => c - connects[0]));
        stream.Stop();
    }

    [Fact]
    public async Task Refreshes_once_on_an_invalid_token_then_stops_when_refused()
    {
        const string Challenge = @"Bearer error=""invalid_token""";
        var (stream, connects, scheduler, server) = await Stream(_ => new EventEnd(401, false, Challenge),
            refresh: @"{""access_token"":""b"",""token_type"":""Bearer"",""expires_in"":3600,""scope"":""library"",""refresh_token"":""r2""}");
        stream.Start();
        await Run(scheduler, 3);
        // Refreshed once, asked again at once, refused again: stopped until the next start.
        Assert.Equal(2, connects.Count);
        Assert.Single(server.Requests, r => r.Url == Home + "/t");
        Assert.False(stream.Running);
        stream.Start();
        await Run(scheduler, 1);
        Assert.True(connects.Count > 2);
        stream.Stop();
    }

    [Theory]
    [InlineData(403, null)]
    [InlineData(401, null)]
    [InlineData(401, @"Bearer error=""insufficient_scope""")]
    public async Task Stops_when_the_server_refuses_the_app(int status, string? challenge)
    {
        var (stream, connects, scheduler, server) = await Stream(_ => new EventEnd(status, false, challenge));
        stream.Start();
        await Run(scheduler, 3);
        Assert.Single(connects);
        Assert.Empty(server.Requests);
        Assert.False(stream.Running);
    }

    [Fact]
    public async Task Stops_without_a_token()
    {
        var scheduler = new FakeScheduler();
        var tokens = new Tokens(Home, "client", new MemoryKeyValue(), new FakeServer().Transport, null, () => scheduler.Now);
        var connects = 0;
        var stream = new EventStream(Home, tokens, _ => { }, () => { }, (_, _, _, _, _) =>
        {
            connects++;
            return Task.FromResult(new EventEnd(0, false));
        }, scheduler);
        stream.Start();
        await Run(scheduler, 3);
        Assert.Equal(0, connects);
        Assert.False(stream.Running);
    }
}
