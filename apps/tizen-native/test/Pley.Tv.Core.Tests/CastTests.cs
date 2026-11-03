using System;
using System.Collections.Generic;
using System.Linq;
using System.Text.Json;
using System.Threading;
using System.Threading.Tasks;
using Pley.Tv.Core.Api;
using Pley.Tv.Core.Auth;
using Pley.Tv.Core.Player;
using Xunit;

namespace Pley.Tv.Core.Tests;

/// <summary>
/// Being cast to: the commands the TV reads from its event stream, the stream's query, and what it
/// says it plays, as the web TV app answers them (fixtures/cast.json); then the stream and the
/// app's state around them.
/// </summary>
public class CastTests
{
    const string Home = "https://pley.example.com";

    static IEnumerable<JsonElement> Cases(string rule) => Fixtures.Load("cast").GetProperty(rule).EnumerateArray();

    static CastView? View(JsonElement v) => v.ValueKind == JsonValueKind.Null
        ? null
        : new CastView(v.GetProperty("itemId").GetInt64(), Enum.Parse<PhaseKind>(v.GetProperty("phase").GetString()!, ignoreCase: true),
            v.GetProperty("positionMs").GetInt64(), v.GetProperty("durationMs").GetInt64(), v.GetProperty("ended").GetBoolean());

    [Fact]
    public void Query_as_the_fixture_has_it()
    {
        foreach (var c in Cases("query"))
        {
            var i = c.GetProperty("in");
            var cast = i.ValueKind == JsonValueKind.Null ? null : new CastIdentity(i.GetProperty("id").GetString()!, i.GetProperty("name").GetString()!, i.GetProperty("platform").GetString()!);
            Assert.Equal(c.GetProperty("out").GetString(), Cast.Query(cast));
        }
    }

    [Fact]
    public void Commands_as_the_fixture_has_them()
    {
        foreach (var c in Cases("command"))
        {
            var i = c.GetProperty("in");
            // Read as the stream reads it: an event that doesn't read as its kind never comes.
            var e = new SseReader().Feed("data: " + i.GetProperty("event").GetRawText() + "\n\n").SingleOrDefault();
            var command = e == null ? null : Cast.Command(e, i.GetProperty("device").GetString()!);
            object? answer = command switch
            {
                CastCommand.Play p => new { kind = "play", itemId = p.ItemId, fileId = p.FileId, positionMs = p.PositionMs },
                CastCommand.Control k => new { kind = "control", action = k.Action, positionMs = k.PositionMs },
                _ => null,
            };
            Assert.Equal((i.GetRawText(), Fixtures.Normalize(c.GetProperty("out").GetRawText())), (i.GetRawText(), Fixtures.Json(answer)));
        }
    }

    [Fact]
    public void State_as_the_fixture_has_it()
    {
        foreach (var c in Cases("state"))
        {
            var i = c.GetProperty("in");
            Assert.Equal((i.GetRawText(), Fixtures.Normalize(c.GetProperty("out").GetRawText())), (i.GetRawText(), Fixtures.Json(Cast.Report(View(i)).Body())));
        }
    }

    [Fact]
    public void Reports_as_the_fixture_has_them()
    {
        foreach (var c in Cases("reports"))
        {
            var scheduler = new FakeScheduler();
            CastView? view = null;
            var sent = new List<object>();
            var reporter = new CastReporter(() => view, r => sent.Add(new { at = scheduler.Elapsed, body = r.Body() }), scheduler);
            foreach (var step in c.GetProperty("in").EnumerateArray())
            {
                scheduler.Advance(step.GetProperty("at").GetInt64() - scheduler.Elapsed);
                if (step.TryGetProperty("view", out var v))
                {
                    view = View(v);
                    reporter.Changed();
                }
                if (step.TryGetProperty("seek", out _) || step.TryGetProperty("connected", out _)) reporter.Now();
                if (step.TryGetProperty("close", out _)) reporter.Close();
            }
            // Nothing more once it closed.
            scheduler.Advance(60_000);
            var name = c.GetProperty("name").GetString();
            Assert.Equal((name, Fixtures.Normalize(c.GetProperty("out").GetRawText())), (name, Fixtures.Json(sent)));
        }
    }

    [Fact]
    public void A_title_cast_starts_where_it_was_sent_else_where_the_profile_left_off()
    {
        var detail = new ItemDetail { Id = 42, UserState = new UserState { PositionMs = 61_000 } };
        Assert.Equal(new Ui.PlayRequest(42, 61_000, false, FileId: 7), Cast.Request(new CastCommand.Play(42, 7, null), detail));
        Assert.Equal(new Ui.PlayRequest(42, 90_000, false), Cast.Request(new CastCommand.Play(42, null, 90_000), detail));
        Assert.Equal(new Ui.PlayRequest(42, 0, true), Cast.Request(new CastCommand.Play(42, null, 0), detail));
        // Watched: from the start, as the item page's Play.
        var watched = detail with { UserState = new UserState { PositionMs = 61_000, Watched = true } };
        Assert.Equal(new Ui.PlayRequest(42, 0, false), Cast.Request(new CastCommand.Play(42, null, null), watched));
        Assert.Equal(new Ui.PlayRequest(42, 0, false), Cast.Request(new CastCommand.Play(42, null, null), null));
    }

    [Fact]
    public void The_TV_makes_its_id_once_and_keeps_it()
    {
        Assert.Matches("^[0-9a-f]{32}$", Cast.NewId());
        Assert.NotEqual(Cast.NewId(), Cast.NewId());
        var store = new MemoryKeyValue();
        var first = new AppState(new AppPlatform("tizen", "client", store), new FakeServer().Transport, scheduler: new FakeScheduler()).CastIdentity();
        Assert.Matches("^[0-9a-f]{32}$", first.Id);
        Assert.Equal(new CastIdentity(first.Id, "Samsung TV", "samsung"), first);
        Assert.Equal(first.Id, store.Get("pley.castDevice"));
        Assert.Equal(first, new AppState(new AppPlatform("tizen", "client", store), new FakeServer().Transport, scheduler: new FakeScheduler()).CastIdentity());
        // A damaged one is made again.
        store.Set("pley.castDevice", "x");
        Assert.Matches("^[0-9a-f]{32}$", new AppState(new AppPlatform("tizen", "client", store), new FakeServer().Transport, scheduler: new FakeScheduler()).CastIdentity().Id);
    }

    [Fact]
    public async Task Says_what_plays_only_while_its_events_list_it()
    {
        var store = new MemoryKeyValue();
        var scheduler = new FakeScheduler();
        var server = new FakeServer();
        var opened = new List<string>();
        var connects = 0;
        var state = new AppState(new AppPlatform("tizen", "client", store), server.Transport, (url, headers, onOpen, onText, cancel) =>
        {
            opened.Add(url);
            connects++;
            onOpen();
            var ended = new TaskCompletionSource<EventEnd>();
            cancel.Register(() => ended.TrySetResult(new EventEnd(0, false)));
            return ended.Task;
        }, scheduler);
        state.Use(Home);
        await state.TokenStore!.Install(new StoredTokens(Home, "pley_at_0", "pley_rt_0", scheduler.Now + 3_600_000, "library play", Home + "/t", Home + "/r"), "library play");
        var id = state.CastIdentity().Id;
        server.On("PUT", $"{Home}/api/cast/devices/{id}/state", 204);
        var heard = new List<int>();
        state.CastConnects.Changed += heard.Add;

        // Not listening: nothing goes.
        state.CastState(CastReport.Nothing);
        state.Listen(true);
        await Task.Delay(50);
        Assert.Equal(new[] { $"{Home}/api/events?cast_device={id}&cast_name=Samsung%20TV&cast_platform=samsung" }, opened);
        Assert.Equal(new[] { 1 }, heard);
        state.CastState(new CastReport(42, 1_000, 5_400_000, "playing"));
        state.CastState(CastReport.Nothing);
        await Task.Delay(50);
        var puts = server.Requests.Where(r => r.Method == "PUT").ToList();
        Assert.Equal(new[] { @"{""item_id"":42,""position_ms"":1000,""duration_ms"":5400000,""state"":""playing""}", @"{""item_id"":null}" }, puts.Select(r => r.Body));
        state.Listen(false);
        state.CastState(CastReport.Nothing);
        await Task.Delay(50);
        Assert.Equal(2, server.Requests.Count(r => r.Method == "PUT"));
    }

    /// <summary>An event stream listing the TV, whose every connection ends as <paramref name="end"/> says.</summary>
    static async Task<(EventStream Stream, List<string> Urls, FakeScheduler Scheduler)> Stream(Func<int, EventEnd> end)
    {
        var scheduler = new FakeScheduler();
        var tokens = new Tokens(Home, "client", new MemoryKeyValue(), new FakeServer().Transport, null, () => scheduler.Now);
        await tokens.Install(new StoredTokens(Home, "a", "r", scheduler.Now + 3_600_000, "library play", Home + "/t", Home + "/r"), "library play");
        var urls = new List<string>();
        var stream = new EventStream(Home, tokens, _ => { }, () => { }, (url, headers, onOpen, onText, cancel) =>
        {
            urls.Add(url);
            return Task.FromResult(end(urls.Count));
        }, scheduler, cast: Cast.Identity("0123456789abcdef0123456789abcdef"));
        return (stream, urls, scheduler);
    }

    [Theory]
    [InlineData(400)]
    [InlineData(403)]
    [InlineData(429)]
    public async Task Goes_on_without_being_listed_once_the_server_refuses_it_until_the_next_start(int status)
    {
        const string Listed = Home + "/api/events?cast_device=0123456789abcdef0123456789abcdef&cast_name=Samsung%20TV&cast_platform=samsung";
        var (stream, urls, scheduler) = await Stream(n => n == 1 || n == 3 ? new EventEnd(status, false) : new EventEnd(0, false));
        stream.Start();
        await Task.Delay(20);
        scheduler.Advance(0);
        await Task.Delay(20);
        Assert.Equal(new[] { Listed, Home + "/api/events" }, urls);
        Assert.True(stream.Running);
        Assert.False(stream.Casting);
        stream.Stop();
        stream.Start();
        await Task.Delay(20);
        Assert.Equal(Listed, urls[2]);
        stream.Stop();
    }
}
