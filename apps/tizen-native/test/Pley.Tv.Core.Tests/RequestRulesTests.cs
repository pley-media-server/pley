using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Text.Json;
using Pley.Tv.Core.Api;
using Pley.Tv.Core.Net;
using Pley.Tv.Core.Requests;
using Pley.Tv.Core.Ui;
using Xunit;

namespace Pley.Tv.Core.Tests;

/// <summary>The Android app's request fixtures (apps/android/core/testing), the server's answers.</summary>
static class RequestFixtures
{
    public static T Load<T>(string name) =>
        JsonSerializer.Deserialize<T>(File.ReadAllText(Path.Combine(AppContext.BaseDirectory, "android", name)), ApiJson.Options)!;

    public static RequestSummary Summary => Load<RequestSummary>("requests/summary.json");
    public static DiscoverPage Trending => Load<DiscoverPage>("discover/trending.json");
    public static DiscoverDetail Movie => Load<DiscoverDetail>("discover/movie.json");
    public static DiscoverDetail Show => Load<DiscoverDetail>("discover/show.json");
    public static RequestList List => Load<RequestList>("requests/list.json");
}

/// <summary>Requests' rules as every client keeps them (the Android app's RequestRulesTest): badges, the main action, the picker, pills and actions.</summary>
public class RequestRulesTests
{
    readonly RequestSummary summary = RequestFixtures.Summary;
    readonly RequestsState open;
    readonly DiscoverDetail movie = RequestFixtures.Movie;
    readonly DiscoverDetail show = RequestFixtures.Show;
    readonly RequestList list = RequestFixtures.List;
    readonly DiscoverPage trending = RequestFixtures.Trending;

    public RequestRulesTests() => open = new RequestsState.Open(summary);

    DiscoverTitle Title(long tmdbId) => trending.Results.First(t => t.TmdbId == tmdbId);

    MediaRequest Request(long id) => list.Requests.First(r => r.Id == id);

    [Fact]
    public void Badges_say_what_a_title_is_for_this_account()
    {
        Assert.Null(RequestRules.BadgeOf(Title(1184918)));
        Assert.Equal(TitleBadge.PartlyInLibrary, RequestRules.BadgeOf(Title(95396)));
        Assert.Equal(TitleBadge.InLibrary, RequestRules.BadgeOf(Title(329865)));
        Assert.Equal(TitleBadge.Requested, RequestRules.BadgeOf(Title(933260)));
        // Partial without a library title: what's there is only asked for.
        Assert.Equal(TitleBadge.Requested, RequestRules.BadgeOf(Title(1396)));
        Assert.Equal(TitleBadge.Declined, RequestRules.BadgeOf(Title(945961)));
        Assert.Null(RequestRules.BadgeOf(TitleStatus.Restricted, null));
    }

    /// <summary>packages/tv's titlePage cases: where a poster opens, as the web TV app answers.</summary>
    public static IEnumerable<object[]> TitlePageCases =>
        Fixtures.Load("requests").GetProperty("titlePage").EnumerateArray()
            .Select(c => new object[] { c.GetProperty("in").GetRawText(), c.GetProperty("out").GetRawText() });

    [Theory]
    [MemberData(nameof(TitlePageCases))]
    public void A_title_the_library_has_whole_opens_there(string input, string output)
    {
        var opens = RequestRules.Opens(JsonSerializer.Deserialize<DiscoverTitle>(input, ApiJson.Options)!);
        var answer = opens switch
        {
            PageKey.Item item => Fixtures.Json(new { name = "item", id = item.ItemId }),
            PageKey.DiscoverTitle title => Fixtures.Json(new { name = "title", kind = title.Kind, tmdbId = title.TmdbId }),
            _ => throw new InvalidOperationException($"not a title's page: {opens}"),
        };
        Assert.Equal(Fixtures.Normalize(output), answer);
    }

    [Fact]
    public void Rows_follow_a_requests_status()
    {
        var robot = Title(1184918);
        var asked = Request(44) with { TmdbId = robot.TmdbId };
        Assert.Equal(TitleStatus.Requested, RequestRules.Patched(robot, asked).Status);
        Assert.Equal(44L, RequestRules.Patched(robot, asked).RequestId);
        Assert.Equal(TitleStatus.Declined, RequestRules.Patched(robot, asked with { Status = RequestStatus.Declined, Stage = RequestStage.Declined }).Status);
        var ready = RequestRules.Patched(robot, asked with { Status = RequestStatus.Available, Stage = RequestStage.Available, ItemId = 300 });
        Assert.Equal(TitleStatus.Library, ready.Status);
        Assert.Equal(300L, ready.ItemId);
        Assert.Equal(TitleBadge.InLibrary, RequestRules.BadgeOf(ready));
        // Gone: no badge.
        Assert.Null(RequestRules.BadgeOf(RequestRules.Patched(Title(933260), asked with { Status = RequestStatus.Available, Stage = RequestStage.Gone })));
        // A 404 says nothing of the title: others may still ask for it.
        var unread = RequestRules.Patched(Title(933260) with { RequestId = 44 }, null);
        Assert.Equal(TitleStatus.Requested, unread.Status);
        Assert.Null(unread.RequestId);
    }

    [Fact]
    public void The_main_action_is_the_first_that_applies()
    {
        Assert.Equal(new MainAction.Request(Ask.Yes), RequestRules.Main(movie, open));
        Assert.Equal(new MainAction.SignInAgain(), RequestRules.Main(movie, new RequestsState.SignInAgain(summary with { SignInAgain = RequestScope.Requests })));
        Assert.Equal(new MainAction.Restricted("Rated R: above this account's age limit"),
            RequestRules.Main(movie with { Status = TitleStatus.Restricted, Restriction = "Rated R: above this account's age limit" }, open));
        Assert.Equal(new MainAction.Open(101), RequestRules.Main(movie with { Status = TitleStatus.Library, ItemId = 101 }, open));
        Assert.Equal(new MainAction.Declined("Not for us", 1762000000),
            RequestRules.Main(movie with { Status = TitleStatus.Declined, Declined = new DeclinedTitle { Reason = "Not for us", At = 1761000000, AgainAt = 1762000000 } }, open));
        var pending = Request(44);
        var requested = movie with { Status = TitleStatus.Requested, RequestId = 44, Requests = new[] { pending } };
        Assert.Equal(new MainAction.Mine(pending), RequestRules.Main(requested, open));
        var followed = pending with { Mine = false, Watching = true };
        Assert.Equal(new MainAction.Following(followed), RequestRules.Main(requested with { Watching = true, Requests = new[] { followed } }, open));
        Assert.Equal(new MainAction.TellMeToo(Ask.Yes), RequestRules.Main(requested with { RequestId = null, Requests = Array.Empty<MediaRequest>() }, open));
        // Breaking Bad: seasons 3 and 5 can still be asked for.
        Assert.Equal(new MainAction.Partial(null, Ask.Yes), RequestRules.Main(show, open));
    }

    [Fact]
    public void Without_access_or_allowance_request_waits()
    {
        var off = new RequestsState.Open(summary with { Access = RequestAccess.Off, Own = 3 });
        Assert.Equal(new MainAction.Request(Ask.NoAccess), RequestRules.Main(movie, off));
        // Following costs nothing: Tell me too works at the limit.
        Assert.Equal(new MainAction.TellMeToo(Ask.Yes), RequestRules.Main(movie with { Status = TitleStatus.Requested, MoviesLeft = 0, MoviesNextAt = 1761700000 }, open));
        Assert.Equal(new MainAction.Request(Ask.Limit), RequestRules.Main(movie with { MoviesLeft = 0 }, open));
        Assert.Equal(new MainAction.Partial(null, Ask.Limit), RequestRules.Main(show with { SeasonsLeft = 0 }, open));
        // No more seasons to ask for: only Open.
        var whole = show with { Seasons = show.Seasons!.Select(s => s.Number is >= 3 and <= 5 ? s with { State = SeasonState.Library } : s).ToList(), ItemId = 7 };
        Assert.Equal(new MainAction.Partial(7, null), RequestRules.Main(whole, open));
    }

    [Fact]
    public void The_picker_ticks_what_can_be_asked_for_lowest_first()
    {
        var picker = SeasonPicker.Of(show, left: null);
        // Specials last.
        Assert.Equal(new[] { 1, 2, 3, 4, 5, 0 }, picker.Seasons.Select(s => s.Season.Number));
        // 1 and 2 are requested, 4 declined: 3 and 5 are ticked; the specials are offered unticked.
        Assert.Equal(new[] { 3, 5 }, picker.Picked.OrderBy(n => n));
        Assert.Equal(new[] { false, false, true, false, true, true }, picker.Seasons.Select(s => s.Askable));
        Assert.True(picker.Ready);
        // What can't be asked for can't be ticked.
        Assert.Same(picker, picker.Toggle(1));
        Assert.Equal(new[] { 0, 3, 5 }, picker.Toggle(0).Picked.OrderBy(n => n));
        Assert.Equal(new[] { 0, 3, 5 }, picker.Toggle(0).Asked());
        // Opened at a season's poster, that season is ticked too.
        Assert.Contains(0, SeasonPicker.Of(show, left: null, start: 0).Picked);
    }

    [Fact]
    public void The_picker_keeps_to_the_allowance()
    {
        var one = SeasonPicker.Of(show, left: 1);
        Assert.Equal(new[] { 3 }, one.Picked);
        var two = one.Toggle(5);
        Assert.True(two.OverLimit);
        Assert.False(two.Ready);
        Assert.False(SeasonPicker.Of(show, left: 0).Ready);
        Assert.False(one.Toggle(3).Ready);
        // Unaired seasons are offered, unticked.
        var coming = show with { Seasons = show.Seasons!.Concat(new[] { new DiscoverSeason { Number = 6, Name = "Season 6", Episodes = 8, State = SeasonState.Unaired } }).ToList() };
        var withSix = SeasonPicker.Of(coming, left: null);
        Assert.True(withSix.Seasons.First(s => s.Season.Number == 6).Askable);
        Assert.DoesNotContain(6, withSix.Picked);
    }

    [Fact]
    public void More_seasons_show_only_for_missing_or_part_seasons_but_the_specials()
    {
        Assert.True(RequestRules.MoreSeasons(show));
        var onlySpecials = show with { Seasons = show.Seasons!.Select(s => s.Number is >= 3 and <= 5 ? s with { State = SeasonState.Requested } : s).ToList() };
        Assert.False(RequestRules.MoreSeasons(onlySpecials));
        Assert.False(RequestRules.MoreSeasons(show with { Status = TitleStatus.Restricted }));
    }

    [Fact]
    public void Season_numbers_read_as_ranges()
    {
        Assert.Equal("1–3, 5", RequestRules.SeasonRanges(new[] { 5, 1, 3, 2 }));
        Assert.Equal("2", RequestRules.SeasonRanges(new[] { 2 }));
        Assert.Equal("1–2, 4–6, 9", RequestRules.SeasonRanges(new[] { 1, 2, 4, 5, 6, 9, 9 }));
        Assert.Equal("", RequestRules.SeasonRanges(new[] { 0 }));
    }

    [Fact]
    public void Tell_me_too_asks_for_the_seasons_others_asked_for()
    {
        var tell = RequestRules.TellMeToo(show);
        Assert.Equal((TitleKind.Show, 1396L), (tell.Kind, tell.TmdbId));
        Assert.Equal(new[] { 1, 2 }, tell.Seasons);
        Assert.Equal(new NewRequest(TitleKind.Movie, 1184918), RequestRules.TellMeToo(movie));
        var again = RequestRules.AskAgain(show.Requests.Single());
        Assert.Equal(new[] { 1, 2 }, again.Seasons);
    }

    [Fact]
    public void Pills_say_where_a_request_is()
    {
        var downloading = show.Requests.Single();
        Assert.Equal(new StagePill.Downloading(0.42f, 1761601200, DownloadState.Downloading), RequestRules.Pill(downloading, manage: false));
        Assert.Equal(PillTone.Blue, RequestRules.Pill(downloading, manage: false).Tone);
        Assert.Equal(new StagePill.Pending(), RequestRules.Pill(Request(44), manage: false));
        Assert.Equal(PillTone.Amber, new StagePill.Pending().Tone);
        var failed = Request(48);
        Assert.Equal(new StagePill.NotSentYet(failed.RetryAt), RequestRules.Pill(failed, manage: false));
        Assert.Equal(new StagePill.CouldNotSend(failed.Reason, failed.RetryAt), RequestRules.Pill(failed, manage: true));
        Assert.Equal(PillTone.Red, RequestRules.Pill(failed, manage: true).Tone);
        Assert.Equal(new StagePill.Waiting(true, 1762000000), RequestRules.Pill(downloading with { Stage = RequestStage.Waiting, StageAt = 1762000000 }, false));
        var partial = downloading with { Stage = RequestStage.Partial, AvailableSeasons = new[] { 1 } };
        Assert.Equal(new[] { 1 }, Assert.IsType<StagePill.Partial>(RequestRules.Pill(partial, false)).Seasons);
        // The bar shows while there's a download, a partial request's too.
        Assert.Equal(0.42f, RequestRules.DownloadProgress(partial));
        Assert.Equal(PillTone.Gray, RequestRules.Pill(downloading with { Stage = RequestStage.Gone }, false).Tone);
        Assert.True(downloading.Moving);
        Assert.False(Request(44).Moving);
        Assert.Equal(new StagePill.Unknown(), RequestRules.Pill(downloading with { Stage = "newer", Download = null }, false));
    }

    [Fact]
    public void Cards_offer_exactly_the_servers_actions()
    {
        var pending = Request(44) with { Actions = new[] { RequestAction.Approve, RequestAction.Decline, RequestAction.Handled, RequestAction.Delete, "newer" } };
        Assert.Equal(new[] { CardAction.Approve, CardAction.ApproveWith, CardAction.Decline, CardAction.Handled, CardAction.Delete }, RequestRules.CardActions(pending, dialogs: true));
        // TVs: plain Approve, and Decline with a reason… as a second step.
        Assert.Equal(new[] { CardAction.Approve, CardAction.Decline, CardAction.DeclineWithReason, CardAction.Handled, CardAction.Delete }, RequestRules.CardActions(pending, dialogs: false));
        Assert.Equal(new[] { CardAction.Withdraw }, RequestRules.CardActions(Request(44), dialogs: false));
        // A declined one is removed, not withdrawn.
        var declined = Request(46) with { Actions = new[] { RequestAction.Withdraw, RequestAction.AskAgain } };
        Assert.Equal(new[] { CardAction.Remove, CardAction.AskAgain }, RequestRules.CardActions(declined, dialogs: false));
        Assert.Equal(new[] { CardAction.StopFollowing }, RequestRules.CardActions(Request(47), dialogs: false));
        Assert.True(CardAction.Withdraw.Confirms() && CardAction.Remove.Confirms() && CardAction.Delete.Confirms());
        Assert.False(CardAction.Approve.Confirms());
    }

    [Fact]
    public void Cards_open_the_library_once_there()
    {
        var ready = Request(45);
        Assert.Equal(new PageKey.Item(101), RequestsModel.Opens(ready, mayAsk: true));
        Assert.Equal(new PageKey.DiscoverTitle(Request(44).Kind, Request(44).TmdbId), RequestsModel.Opens(Request(44), mayAsk: true));
        Assert.Null(RequestsModel.Opens(Request(44), mayAsk: false));
        Assert.True(RequestRules.ReadyElsewhere(ready with { ItemId = null }));
        Assert.False(RequestRules.ReadyElsewhere(ready));
    }

    static ApiException Refusal(int status, string json, string? challenge = null)
    {
        using var doc = JsonDocument.Parse(json);
        var body = doc.RootElement.Clone();
        var (error, scope) = Api.Api.Challenge(challenge);
        return new ApiException(status, Json.String(body, "code") ?? "", Json.String(body, "message") ?? "", body) { BearerError = error, Scope = scope };
    }

    [Fact]
    public void Refusals_say_what_they_mean()
    {
        Assert.Equal(new RequestProblem.Off(), RequestProblem.Of(Refusal(503, @"{""code"":""unavailable"",""message"":""off"",""setup"":false}")));
        Assert.Equal(new RequestProblem.TmdbKey(), RequestProblem.Of(Refusal(503, @"{""code"":""tmdb_key"",""message"":""refused""}")));
        Assert.Equal(new RequestProblem.TmdbDown(), RequestProblem.Of(Refusal(503, @"{""code"":""tmdb_down"",""message"":""away""}")));
        Assert.True(new RequestProblem.TmdbDown().Retry);
        Assert.False(new RequestProblem.TmdbKey().Retry);
        Assert.Equal(new RequestProblem.NoAccess(), RequestProblem.Of(Refusal(403, @"{""code"":""no_access"",""message"":""no""}")));
        Assert.Equal(new RequestProblem.SignInAgain(RequestScope.Approvals),
            RequestProblem.Of(Refusal(403, @"{""code"":""insufficient_scope"",""message"":""scope""}", @"Bearer error=""insufficient_scope"", scope=""approvals""")));
        Assert.Equal(new RequestProblem.Restricted("Rated R"), RequestProblem.Of(Refusal(403, @"{""code"":""restricted"",""message"":""Rated R""}")));
        Assert.Equal(new RequestProblem.Declined(1762000000), RequestProblem.Of(Refusal(409, @"{""code"":""declined"",""message"":""d"",""retry_at"":1762000000}")));
        Assert.Equal(new RequestProblem.Limit(null), RequestProblem.Of(Refusal(429, @"{""code"":""too_many_requests"",""message"":""limit""}")));
        var conflict = Assert.IsType<RequestProblem.Conflict>(RequestProblem.Of(Refusal(409, @"{""code"":""conflict"",""message"":""Already approved"",""request"":{""id"":44,""kind"":""movie"",""tmdb_id"":1,""title"":""Arrival"",""status"":""approved""}}")));
        Assert.Equal(("Already approved", 44L), (conflict.Message, conflict.Request!.Id));
        Assert.Equal(new RequestProblem.NotFound(), RequestProblem.Of(Refusal(404, @"{""code"":""not_found"",""message"":""gone""}")));
        // Only an unreachable server is offline: never a 503 with Pley's JSON, nor a proxy's 503 without it.
        Assert.IsType<RequestProblem.Offline>(RequestProblem.Of(new NetworkException(NetworkFailure.NothingAnswers)));
        Assert.IsType<RequestProblem.Other>(RequestProblem.Of(new ApiException(503, "unavailable", "the server refused (HTTP 503)")));
    }

    [Fact]
    public void The_allowance_says_what_is_left()
    {
        var allowance = Allowance.Of(summary);
        Assert.True(allowance.Limited);
        Assert.True(allowance.Allows(TitleKind.Movie));
        Assert.False((allowance with { MoviesLeft = 0 }).Allows(TitleKind.Movie));
        Assert.True((allowance with { MoviesLeft = 0 }).Allows(TitleKind.Show));
        Assert.False(Allowance.Of(summary with { MoviesLeft = null, SeasonsLeft = null }).Limited);
    }

    [Fact]
    public void A_decline_reason_is_cut_to_the_servers_limit_without_splitting_a_pair()
    {
        const int max = RequestRules.DeclineReasonMax;
        Assert.Equal(3, RequestRules.DeclineReasonEnd("abc"));
        Assert.Equal(max, RequestRules.DeclineReasonEnd(new string('a', max)));
        Assert.Equal(max, RequestRules.DeclineReasonEnd(new string('a', max + 5)));
        Assert.Equal(max - 1, RequestRules.DeclineReasonEnd(new string('a', max - 1) + "😀b"));
        Assert.Equal(max, RequestRules.DeclineReasonEnd(new string('a', max - 2) + "😀b"));
    }

    [Fact]
    public void Search_words_are_cut_to_the_servers_limit_in_characters()
    {
        Assert.Equal("dune", RequestRules.SearchText("  dune "));
        Assert.Equal(200, RequestRules.SearchText(new string('a', 250)).Length);
        // A pair counts once, and is never split.
        var pairs = string.Concat(Enumerable.Repeat("😀", 201));
        Assert.Equal(400, RequestRules.SearchText(pairs).Length);
    }

    [Fact]
    public void The_way_in_follows_the_summary()
    {
        Assert.Equal(new RequestsEntry(Discover: true, Requests: true, Badge: 1), RequestsEntry.Of(open));
        var manager = new RequestsState.Open(summary with { Manage = true, Pending = 2, Failed = 1, Unseen = 1 });
        Assert.Equal(new RequestsEntry(true, true, true, 4, 1), RequestsEntry.Of(manager));
        Assert.True(RequestsEntry.Of(manager).Alarm);
        Assert.Equal(RequestsEntry.None, RequestsEntry.Of(new RequestsState.Hidden()));
        // The rail: Discover where the profile may ask, else its requests.
        Assert.Equal(RailSlot.Discover, Rail.RequestsItem(RequestsEntry.Of(open))!.Slot);
        var noAsk = RequestsEntry.Of(new RequestsState.Open(summary with { Access = RequestAccess.Off, Own = 2 }));
        Assert.Equal(RailSlot.Requests, Rail.RequestsItem(noAsk)!.Slot);
        Assert.Null(Rail.RequestsItem(RequestsEntry.None));
        // After Home, before Live TV; the pages under it keep it current.
        var items = Rail.Items(Array.Empty<Library>(), liveTv: true, requests: Rail.RequestsItem(RequestsEntry.Of(open)));
        Assert.Equal(new[] { "search", "home", "discover", "live-tv", "playlists" }, items.Select(i => i.Id));
        Assert.Equal("discover", Rail.Current(new PageKey.DiscoverTitle(TitleKind.Movie, 1), items));
        Assert.Equal("discover", Rail.Current(new PageKey.Requests(), items));
    }

    [Fact]
    public void The_state_follows_the_summary()
    {
        Assert.IsType<RequestsState.Unknown>(RequestsState.Of(null));
        Assert.IsType<RequestsState.Hidden>(RequestsState.Of(new RequestSummary { Enabled = false }));
        Assert.IsType<RequestsState.SignInAgain>(RequestsState.Of(summary with { SignInAgain = RequestScope.Requests }));
        Assert.IsType<RequestsState.NoAccess>(RequestsState.Of(summary with { Access = RequestAccess.Off, Own = 0 }));
        Assert.True(RequestsState.Of(summary).MayAsk);
        Assert.False(RequestsState.Of(summary with { Access = RequestAccess.Off, Own = 1 }).MayAsk);
    }
}

/// <summary>Requests' words, on a fixed clock: 2025-10-30 12:00 UTC (the Android app's RequestTextsTest).</summary>
public class RequestTextTests
{
    const long Now = 1_761_825_600, Day = 86_400;
    readonly RequestText texts = new RequestText(() => Now, TimeZoneInfo.Utc);

    [Fact]
    public void Seasons_are_named_as_the_web_names_them()
    {
        Assert.Equal("Season 2", RequestText.Seasons(new[] { 2 }));
        Assert.Equal("Seasons 1–3, 5", RequestText.Seasons(new[] { 5, 3, 2, 1 }));
        Assert.Equal("Seasons 1–2 and specials", RequestText.Seasons(new[] { 0, 1, 2 }));
        Assert.Equal("Season 3 and specials", RequestText.Seasons(new[] { 3, 0 }));
        Assert.Equal("Specials", RequestText.Seasons(new[] { 0 }));
    }

    [Fact]
    public void Pills_say_the_stage()
    {
        Assert.Equal("Waiting for approval", texts.Pill(new StagePill.Pending()));
        Assert.Equal("Downloading 42% · in 20 min", texts.Pill(new StagePill.Downloading(0.42f, Now + 20 * 60, DownloadState.Downloading)));
        Assert.Equal("Downloading 42%", texts.Pill(new StagePill.Downloading(0.42f, null, DownloadState.Downloading)));
        Assert.Equal("Waiting to download", texts.Pill(new StagePill.Downloading(0f, null, DownloadState.Queued)));
        Assert.Equal("Download paused", texts.Pill(new StagePill.Downloading(0.5f, null, DownloadState.Paused)));
        Assert.Equal("The download has a problem", texts.Pill(new StagePill.Downloading(0.5f, null, DownloadState.Warning)));
        // A release known by its day comes at 12:00 UTC of that day.
        Assert.Equal("Out Nov 4", texts.Pill(new StagePill.Waiting(false, 1_762_257_600)));
        Assert.Equal("Airs Nov 4", texts.Pill(new StagePill.Waiting(true, 1_762_257_600)));
        Assert.Equal("Not out yet", texts.Pill(new StagePill.Waiting(false, null)));
        Assert.Equal("Seasons 1–2 ready", texts.Pill(new StagePill.Partial(new[] { 1, 2 })));
        Assert.Equal("Ready to watch", texts.Pill(new StagePill.Available()));
        Assert.Equal("No longer in a library", texts.Pill(new StagePill.Gone()));
        Assert.Equal("Adding to the library", texts.Pill(new StagePill.Importing()));
        Assert.Equal("Couldn't send: Radarr isn't answering", texts.Pill(new StagePill.CouldNotSend("Radarr isn't answering", null)));
        Assert.Equal("Not sent yet", texts.Pill(new StagePill.NotSentYet(Now + 300)));
        Assert.Equal("Trying again in 5 min.", texts.PillLine(new StagePill.NotSentYet(Now + 300)));
        Assert.Equal("It couldn't be sent to download yet: an administrator will look at it.", texts.PillLine(new StagePill.NotSentYet(null)));
    }

    [Fact]
    public void Times_are_written_here()
    {
        Assert.Equal("in 20 min", texts.Until(Now + 20 * 60));
        Assert.Equal("in 3 hours", texts.Until(Now + 3 * 3_600));
        Assert.Equal("tomorrow", texts.Until(Now + Day));
        Assert.Equal("in 12 days", texts.Until(Now + 12 * Day));
        Assert.Equal("in a moment", texts.Until(Now - 5));
        Assert.Equal("3 days ago", texts.Ago(Now - 3 * Day));
        Assert.Equal("today, 9:00 PM", texts.DayOf(Now + 9 * 3_600));
        Assert.Equal("tomorrow, 9:00 AM", texts.DayOf(Now + 21 * 3_600));
        Assert.Equal("Jan 2, 2026", texts.DayOf(1_767_355_200));
    }

    [Fact]
    public void The_allowance_line()
    {
        var allowance = new Allowance(2, 5, 3, 10, null, null);
        Assert.Equal(new[] { "You can ask for 2 more movies and 5 more seasons.", "Limits: 3 movies and 10 seasons per 7 days." }, texts.Allowance(allowance));
        Assert.Equal(new[] { "You can ask for 1 more movie.", "Limit: 3 movies per 7 days." }, texts.Allowance(allowance with { MoviesLeft = 1 }, TitleKind.Movie));
        var noMovies = allowance with { MoviesLeft = 0, MoviesNextAt = Now + 3 * Day };
        Assert.Equal(new[] { "You've asked for all the movies you can for now: you can ask for more in 3 days." }, texts.Allowance(noMovies, TitleKind.Movie));
        var nothing = noMovies with { SeasonsLeft = 0, SeasonsNextAt = Now + 5 * Day };
        Assert.Equal(new[] { "You've asked for all you can for now: more movies in 3 days, more seasons in 5 days." }, texts.Allowance(nothing));
        Assert.Empty(texts.Allowance(new Allowance(null, null, null, null, null, null)));
    }

    [Fact]
    public void A_decline_says_when_to_ask_again()
    {
        Assert.Equal("Declined: “Not for us”. You can ask again in 12 days.", texts.Declined(new MainAction.Declined("Not for us", Now + 12 * Day)));
        Assert.Equal("Declined. You can ask again in 12 days.", texts.Declined(new MainAction.Declined(null, Now + 12 * Day)));
        Assert.Equal("Declined.", texts.Declined(new MainAction.Declined(null, null)));
    }

    [Fact]
    public void Asking_says_what_happened()
    {
        var request = new MediaRequest { Id = 60, Kind = TitleKind.Movie, TmdbId = 1, Title = "Arrival", Status = RequestStatus.Pending };
        Assert.Equal("Asked for “Arrival”: an administrator decides.", RequestText.Asked(new Asked(request, true), "Arrival"));
        Assert.Equal("“Arrival” is approved and on its way.", RequestText.Asked(new Asked(request with { Status = RequestStatus.Approved }, true), "Arrival"));
        Assert.Equal("Asked for “Arrival”, but it couldn't be sent to download yet.", RequestText.Asked(new Asked(request with { Status = RequestStatus.Failed }, true), "Arrival"));
        Assert.Equal("You'll hear when “Arrival” is ready.", RequestText.Asked(new Asked(request, false), "Arrival"));
        Assert.Equal("Pick a season.", RequestText.PickerButton(new SeasonPicker(Array.Empty<PickerSeason>(), Array.Empty<int>(), null)));
        Assert.Equal("Request 1 season", RequestText.PickerButton(new SeasonPicker(Array.Empty<PickerSeason>(), new[] { 3 }, null)));
        Assert.Equal("Request 3 seasons", RequestText.PickerButton(new SeasonPicker(Array.Empty<PickerSeason>(), new[] { 3, 4, 5 }, null)));
    }

    [Fact]
    public void Notices_say_what_is_ready()
    {
        Assert.Equal("“Arrival” is ready to watch.", RequestText.Notice(new RequestNotice.Ready(1, 45, "Arrival", TitleKind.Movie, 101, null, true)));
        Assert.Equal("Season 2 of “Severance” is ready to watch.", RequestText.Notice(new RequestNotice.Ready(2, 47, "Severance", TitleKind.Show, 201, new[] { 2 }, false)));
        Assert.Equal("Seasons 1–2 of “Severance” are ready to watch.", RequestText.Notice(new RequestNotice.Ready(3, 47, "Severance", TitleKind.Show, 201, new[] { 1, 2 }, false)));
        Assert.Equal("3 of your requests have news", RequestText.Notice(new RequestNotice.News(10, 3)));
    }

    [Fact]
    public void The_number_on_the_way_in_is_said_in_words()
    {
        Assert.Equal("1 of your requests has news", RequestText.Count(new RequestsEntry(Requests: true, Badge: 1)));
        Assert.Equal("1 request needs you", RequestText.Count(new RequestsEntry(Requests: true, Manage: true, Badge: 1)));
        Assert.Equal("3 requests need you, 1 couldn't be sent", RequestText.Count(new RequestsEntry(Requests: true, Manage: true, Badge: 3, Failed: 1)));
    }

    [Fact]
    public void Refusals_are_said()
    {
        var summary = new RequestSummary { Enabled = true, SeasonsLimit = 5, Manage = true };
        Assert.Equal("You can ask for at most 5 seasons per 7 days: pick fewer.", texts.Problem(new RequestProblem.Limit(null), summary));
        Assert.Equal("You can ask for more in 3 days", texts.Problem(new RequestProblem.Limit(Now + 3 * Day), summary));
        Assert.Equal("You can ask again in 12 days", texts.Problem(new RequestProblem.Declined(Now + 12 * Day), summary));
        Assert.Equal("Requests are off", texts.Problem(new RequestProblem.Off(), null));
        Assert.Equal("This server doesn't take requests for movies and shows.", RequestText.ProblemBody(new RequestProblem.Off(), null));
        Assert.Equal("TMDB refused this server's key", texts.Problem(new RequestProblem.TmdbKey(), summary));
        Assert.Equal("An administrator needs to check it.", RequestText.ProblemBody(new RequestProblem.TmdbKey(), summary with { Manage = false }));
        Assert.Equal("Sign in again to ask for movies and shows", texts.Problem(new RequestProblem.SignInAgain(RequestScope.Requests), null));
    }

    [Fact]
    public void Seasons_say_how_they_stand()
    {
        Assert.Equal("4 of 10 episodes in library", RequestText.SeasonState(new DiscoverSeason { Number = 1, Episodes = 10, InLibrary = 4, State = SeasonState.Partial }));
        Assert.Equal("Airs Nov 4, 2026", RequestText.SeasonState(new DiscoverSeason { Number = 2, AirDate = "2026-11-04", State = SeasonState.Unaired }));
        Assert.Equal("10 episodes", RequestText.SeasonState(new DiscoverSeason { Number = 3, Episodes = 10, State = SeasonState.Missing }));
        Assert.Equal("1 episode", RequestText.SeasonState(new DiscoverSeason { Number = 3, Episodes = 1, State = SeasonState.Missing }));
        Assert.Equal("Declined", RequestText.SeasonState(new DiscoverSeason { Number = 4, Declined = true, State = SeasonState.Missing }));
    }
}
