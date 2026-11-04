using System;
using System.Collections.Generic;
using System.Linq;
using System.Threading.Tasks;
using Pley.Tv.Core.Api;
using Pley.Tv.Core.Auth;
using Pley.Tv.Core.Requests;
using Pley.Tv.Core.Ui;
using Xunit;

namespace Pley.Tv.Core.Tests;

/// <summary>Requests for a test (the Android app's FakeRequests): answers set by hand, every call kept.</summary>
sealed class FakeRequests : IRequestsSource
{
    public Signal<RequestsState> State { get; } = new Signal<RequestsState>(new RequestsState.Open(RequestFixtures.Summary));
    public Signal<IReadOnlyList<RequestNotice>> Notices { get; } = new Signal<IReadOnlyList<RequestNotice>>(Array.Empty<RequestNotice>());
    public Signal<bool> Streaming { get; } = new Signal<bool>(true);
    public event Action<RequestChange>? Changed;
    public string Origin => "https://pley.example.com";

    public Dictionary<(DiscoverList, int), Func<DiscoverPage>> Pages { get; } = new Dictionary<(DiscoverList, int), Func<DiscoverPage>>();
    public Func<string, int, DiscoverPage> Searched { get; set; } = (_, _) => new DiscoverPage();
    public Dictionary<(string, long), DiscoverDetail> Titles { get; } = new Dictionary<(string, long), DiscoverDetail>();
    public Func<RequestQuery, RequestList> Listed { get; set; } = _ => new RequestList();
    public Dictionary<long, MediaRequest?> Requests { get; } = new Dictionary<long, MediaRequest?>();
    public Func<NewRequest, Task<Asked>> Asking { get; set; } = r => Task.FromResult(new Asked(new MediaRequest { Id = 99, Kind = r.Kind, TmdbId = r.TmdbId, Status = RequestStatus.Pending }, true));
    public List<string> Calls { get; } = new List<string>();
    public List<NewRequest> Asked { get; } = new List<NewRequest>();
    public List<RequestQuery> Queries { get; } = new List<RequestQuery>();

    public void Change(RequestChange change) => Changed?.Invoke(change);

    public void Start() => Calls.Add("start");
    public void Refresh() => Calls.Add("refresh");
    public DiscoverPage? SavedRow(DiscoverList list) => null;

    public Task<DiscoverPage> Page(DiscoverList list, int page)
    {
        Calls.Add($"page {list} {page}");
        return Pages.TryGetValue((list, page), out var answer) ? Task.FromResult(answer()) : Task.FromResult(new DiscoverPage { Page = page, TotalPages = page });
    }

    public Task<DiscoverPage> Search(string query, int page = 1, bool missing = false)
    {
        Calls.Add($"search {query} {page}");
        return Task.FromResult(Searched(query, page));
    }

    public DiscoverDetail? CachedTitle(string kind, long tmdbId) => null;

    /// <summary>A title page that comes when the test says, as the server's does.</summary>
    public TaskCompletionSource<DiscoverDetail>? Later { get; set; }

    public Task<DiscoverDetail> Title(string kind, long tmdbId)
    {
        Calls.Add($"title {kind} {tmdbId}");
        if (Later != null) return Later.Task;
        return Titles.TryGetValue((kind, tmdbId), out var d) ? Task.FromResult(d) : Task.FromException<DiscoverDetail>(new ApiException(404, "not_found", "gone"));
    }

    public Task<ItemRequest?> ItemRequest(long itemId) => Task.FromResult<ItemRequest?>(null);

    public Task<RequestList> List(RequestQuery query)
    {
        Queries.Add(query);
        return Task.FromResult(Listed(query));
    }

    public Task<MediaRequest?> Request(long id) => Task.FromResult(Requests.TryGetValue(id, out var r) ? r : null);

    public Task<Asked> Ask(NewRequest request)
    {
        Asked.Add(request);
        return Asking(request);
    }

    public Task Delete(MediaRequest request)
    {
        Calls.Add($"delete {request.Id}");
        return Task.CompletedTask;
    }

    public Task Unwatch(MediaRequest request)
    {
        Calls.Add($"unwatch {request.Id}");
        return Task.CompletedTask;
    }

    public Task<MediaRequest> Approve(long id, Approval approval)
    {
        Calls.Add($"approve {id}");
        return Task.FromResult(Requests[id]! with { Status = RequestStatus.Approved, Stage = RequestStage.Approved });
    }

    public Task<MediaRequest> Decline(long id, string? reason)
    {
        Calls.Add($"decline {id} {reason}");
        return Task.FromResult(Requests[id]! with { Status = RequestStatus.Declined, Stage = RequestStage.Declined, Reason = reason });
    }

    public Task<MediaRequest> Handled(long id) => Task.FromResult(Requests[id]! with { Status = RequestStatus.Approved });

    public Task Seen()
    {
        Calls.Add("seen");
        return Task.CompletedTask;
    }

    public void Dismiss(RequestNotice notice) => Notices.Value = Notices.Value.Where(n => n.Key != notice.Key).ToList();
}

/// <summary>Discover, its grids, a title's page and the requests (the Android app's view model tests, as the TV uses them).</summary>
public class RequestModelTests
{
    readonly FakeRequests source = new FakeRequests();
    readonly FakeScheduler scheduler = new FakeScheduler();

    static DiscoverTitle Title(long id, string kind = TitleKind.Movie, string status = TitleStatus.None) =>
        new DiscoverTitle { Kind = kind, TmdbId = id, Title = $"T{id}", Status = status };

    static DiscoverPage PageOf(int page, int total, params DiscoverTitle[] titles) => new DiscoverPage { Page = page, TotalPages = total, Results = titles };

    [Fact]
    public void Discover_shows_the_first_trending_title_as_its_hero_and_reads_a_screenful()
    {
        source.Pages[(DiscoverList.Trending, 1)] = () => PageOf(1, 9, Enumerable.Range(1, 8).Select(i => Title(i)).ToArray());
        source.Pages[(DiscoverList.Trending, 2)] = () => PageOf(2, 9, Enumerable.Range(9, 8).Select(i => Title(i)).ToArray());
        using var model = new DiscoverModel(source, scheduler);
        var state = model.State.Value;
        Assert.Equal(1, state.Hero!.Title.TmdbId);
        Assert.Equal(new MainAction.Request(Ask.Yes), state.Hero.Action);
        // A short first page reads on until a screenful (12): two pages here.
        Assert.Equal(16, state.Rows[0].Pages.Titles.Count);
        Assert.Contains("page Trending 2", source.Calls);
        Assert.DoesNotContain("page Movies 1", source.Calls);
        // The next rows are read as they come into view.
        model.ShowRow(DiscoverList.Movies);
        Assert.Contains("page Movies 1", source.Calls);
    }

    [Fact]
    public void With_nothing_trending_every_row_is_read_for_a_hero()
    {
        source.Pages[(DiscoverList.Trending, 1)] = () => PageOf(1, 1);
        source.Pages[(DiscoverList.Shows, 1)] = () => PageOf(1, 1, Title(7, TitleKind.Show));
        using var model = new DiscoverModel(source, scheduler);
        Assert.Equal(7, model.State.Value.Hero!.Title.TmdbId);
        Assert.False(model.State.Value.Nothing);
        source.Pages[(DiscoverList.Shows, 1)] = () => PageOf(1, 1);
        using var empty = new DiscoverModel(source, scheduler);
        Assert.True(empty.State.Value.Nothing);
        Assert.Null(empty.State.Value.Hero);
    }

    [Fact]
    public void The_heros_request_asks_for_a_movie_here_once_and_opens_a_shows_picker()
    {
        source.Pages[(DiscoverList.Trending, 1)] = () => PageOf(1, 1, Title(1));
        var told = new List<RequestEvent>();
        var pending = new TaskCompletionSource<Asked>();
        source.Asking = _ => pending.Task;
        using var model = new DiscoverModel(source, scheduler);
        model.Told += told.Add;
        model.HeroAction();
        model.HeroAction();
        Assert.Single(source.Asked);
        Assert.True(model.State.Value.Asking);
        pending.SetResult(new Asked(new MediaRequest { Id = 5, Status = RequestStatus.Pending }, true));
        Assert.False(model.State.Value.Asking);
        Assert.IsType<RequestEvent.Asked>(Assert.Single(told));

        source.Pages[(DiscoverList.Trending, 1)] = () => PageOf(1, 1, Title(2, TitleKind.Show));
        using var shows = new DiscoverModel(source, scheduler);
        var opened = new List<RequestEvent>();
        shows.Told += opened.Add;
        shows.HeroAction();
        Assert.Equal(new RequestEvent.Open(new PageKey.DiscoverTitle(TitleKind.Show, 2, Pick: true)), Assert.Single(opened));
    }

    [Fact]
    public void Rows_badges_follow_a_requests_change_and_your_requests_are_read_again()
    {
        source.Pages[(DiscoverList.Trending, 1)] = () => PageOf(1, 1, Title(1), Title(2));
        var mine = new MediaRequest { Id = 44, Kind = TitleKind.Movie, TmdbId = 2, Title = "T2", Status = RequestStatus.Pending, Stage = RequestStage.Pending, Mine = true };
        source.Requests[44] = mine;
        source.Listed = q => q.Mine ? new RequestList { Total = 1, Requests = new[] { mine } } : new RequestList();
        using var model = new DiscoverModel(source, scheduler);
        Assert.Single(model.State.Value.Yours);
        var reads = source.Queries.Count;
        source.Change(new RequestChange(44, TitleKind.Movie, 2));
        Assert.Equal(TitleStatus.Requested, model.State.Value.Rows[0].Pages.Titles[1].Status);
        Assert.Equal(TitleStatus.None, model.State.Value.Rows[0].Pages.Titles[0].Status);
        // "Your requests" once changes pause.
        scheduler.Advance(DiscoverModel.ChangesMs);
        Assert.Equal(reads + 1, source.Queries.Count);
    }

    [Fact]
    public void Without_access_discover_says_why_and_reads_nothing()
    {
        source.State.Value = new RequestsState.Open(RequestFixtures.Summary with { Access = RequestAccess.Off, Own = 2 });
        using var model = new DiscoverModel(source, scheduler);
        Assert.Equal(new RequestProblem.NoAccess(), model.State.Value.Problem);
        Assert.DoesNotContain(source.Calls, c => c.StartsWith("page"));
        source.State.Value = new RequestsState.SignInAgain(RequestFixtures.Summary with { SignInAgain = RequestScope.Requests });
        Assert.Equal(new RequestProblem.SignInAgain(RequestScope.Requests), model.State.Value.Problem);
    }

    [Fact]
    public void A_grid_reads_five_pages_at_most_then_offers_show_more()
    {
        // An age limit: page after page with one title.
        source.Searched = (_, page) => PageOf(page, 50, Title(page));
        using var grid = new DiscoverGridModel(source, new DiscoverSource.Words("  dune "), screenful: 28);
        Assert.Equal("dune", grid.Query);
        var ready = Assert.IsType<DiscoverSearch.Ready>(grid.State.Value.Search);
        Assert.Equal(DiscoverGridModel.TvFill, ready.Pages.Titles.Count);
        Assert.True(ready.Pages.Stopped && ready.Pages.Settled);
        // Focus near the end doesn't go past a stop; Show more does.
        grid.Want(40);
        Assert.Equal(5, ((DiscoverSearch.Ready)grid.State.Value.Search).Pages.Titles.Count);
        grid.ShowMore();
        Assert.Equal(10, ((DiscoverSearch.Ready)grid.State.Value.Search).Pages.Titles.Count);
        Assert.Contains("search dune 10", source.Calls);
    }

    [Fact]
    public void A_grid_says_when_nothing_matches_and_why_it_failed()
    {
        source.Searched = (_, _) => PageOf(1, 1);
        using var none = new DiscoverGridModel(source, new DiscoverSource.Words("zzz"), 28);
        Assert.IsType<DiscoverSearch.Empty>(none.State.Value.Search);
        source.Pages[(DiscoverList.Anime, 1)] = () => throw new ApiException(503, "tmdb_down", "away", System.Text.Json.JsonDocument.Parse(@"{""code"":""tmdb_down""}").RootElement);
        using var down = new DiscoverGridModel(source, new DiscoverSource.Row(DiscoverList.Anime), 28);
        Assert.Equal(new RequestProblem.TmdbDown(), Assert.IsType<DiscoverSearch.Failed>(down.State.Value.Search).Problem);
        Assert.Equal(DiscoverList.Anime, down.State.Value.List);
    }

    [Fact]
    public void A_shows_page_opens_its_picker_when_asked_and_asks_for_what_was_ticked()
    {
        var show = RequestFixtures.Show;
        source.Titles[(TitleKind.Show, show.TmdbId)] = show;
        var told = new List<RequestEvent>();
        using var page = new DiscoverTitleModel(source, scheduler, new PageKey.DiscoverTitle(TitleKind.Show, show.TmdbId, Pick: true));
        page.Told += told.Add;
        var ready = Assert.IsType<TitleState.Ready>(page.State.Value);
        Assert.Equal(new MainAction.Partial(null, Ask.Yes), ready.Action);
        var picker = page.Picker.Value!;
        Assert.Equal(new[] { 3, 5 }, picker.Asked());
        page.Toggle(5);
        page.AskPicked();
        Assert.Equal(new[] { 3 }, Assert.Single(source.Asked).Seasons);
        Assert.Null(page.Picker.Value);
        Assert.IsType<RequestEvent.Asked>(Assert.Single(told));
    }

    [Fact]
    public void Which_seasons_opens_over_the_page_once_it_has_come()
    {
        var show = RequestFixtures.Show;
        source.Later = new TaskCompletionSource<DiscoverDetail>();
        using var page = new DiscoverTitleModel(source, scheduler, new PageKey.DiscoverTitle(TitleKind.Show, show.TmdbId, Pick: true));
        // As the screen does: the picker is drawn only over a page that shows.
        var drawn = new List<SeasonPicker>();
        page.Picker.Changed += p =>
        {
            if (p != null && page.State.Value is TitleState.Ready) drawn.Add(p);
        };
        Assert.IsType<TitleState.Loading>(page.State.Value);
        source.Later.SetResult(show);
        Assert.Single(drawn);
    }

    [Fact]
    public void A_title_page_reads_itself_again_as_its_requests_change()
    {
        var movie = RequestFixtures.Movie;
        source.Titles[(TitleKind.Movie, movie.TmdbId)] = movie;
        using var page = new DiscoverTitleModel(source, scheduler, new PageKey.DiscoverTitle(TitleKind.Movie, movie.TmdbId));
        Assert.IsType<TitleState.Ready>(page.State.Value);
        source.Titles[(TitleKind.Movie, movie.TmdbId)] = movie with { Status = TitleStatus.Library, ItemId = 300 };
        source.Change(new RequestChange(12, TitleKind.Movie, movie.TmdbId));
        source.Change(new RequestChange(12, TitleKind.Movie, movie.TmdbId));
        scheduler.Advance(DiscoverModel.ChangesMs);
        Assert.Equal(2, source.Calls.Count(c => c == $"title movie {movie.TmdbId}"));
        Assert.Equal(new MainAction.Open(300), ((TitleState.Ready)page.State.Value).Action);
        Assert.Equal(new PageKey.Item(300), page.Opens());
        // Another title's news isn't this page's.
        source.Change(new RequestChange(13, TitleKind.Movie, 1));
        scheduler.Advance(DiscoverModel.ChangesMs);
        Assert.Equal(2, source.Calls.Count(c => c == $"title movie {movie.TmdbId}"));
    }

    [Fact]
    public void A_title_not_on_tmdb_says_so()
    {
        using var page = new DiscoverTitleModel(source, scheduler, new PageKey.DiscoverTitle(TitleKind.Movie, 5));
        Assert.Equal(new RequestProblem.NotFound(), Assert.IsType<TitleState.Problem>(page.State.Value).Value);
    }

    [Fact]
    public void Managers_open_on_needs_attention_and_their_actions_change_the_list()
    {
        var attention = RequestFixtures.Load<RequestList>("requests/attention.json");
        source.State.Value = new RequestsState.Open(RequestFixtures.Summary with { Manage = true, Pending = 1, Unseen = 2 });
        source.Listed = _ => attention;
        foreach (var r in attention.Requests) source.Requests[r.Id] = r;
        using var model = new RequestsModel(source, scheduler);
        var state = model.State.Value;
        Assert.Equal(RequestView.Attention, state.Filter.View);
        var query = source.Queries.First();
        Assert.True(query.Attention && query.Oldest);
        var ready = Assert.IsType<RequestListState.Ready>(state.List);
        // Once the first page shows, the news is seen.
        Assert.Contains("seen", source.Calls);
        // Approved, a request leaves Needs attention.
        var first = ready.Requests[0];
        model.Act(first, CardAction.Approve);
        Assert.Contains($"approve {first.Id}", source.Calls);
        Assert.DoesNotContain(((RequestListState.Ready)model.State.Value.List).Requests, r => r.Id == first.Id);
        // Filters read the list again.
        model.Kind(TitleKind.Show);
        Assert.Equal(TitleKind.Show, source.Queries.Last().Kind);
        model.TypeTitle("arc");
        Assert.Null(source.Queries.Last().Q);
        scheduler.Advance(RequestsModel.TypingMs);
        Assert.Equal("arc", source.Queries.Last().Q);
    }

    [Fact]
    public void Your_requests_say_when_there_are_none_and_read_again_without_events()
    {
        source.Streaming.Value = false;
        source.Listed = _ => new RequestList();
        using var model = new RequestsModel(source, scheduler);
        Assert.Equal(RequestView.All, model.State.Value.Filter.View);
        Assert.Equal(new RequestListState.Empty(false), model.State.Value.List);
        var reads = source.Queries.Count;
        scheduler.Advance(RequestsModel.PollMs);
        Assert.Equal(reads + 1, source.Queries.Count);
    }

    [Fact]
    public void Search_shows_tmdbs_first_titles_the_libraries_lack()
    {
        // Every page short of 20, three read at most.
        source.Searched = (_, page) => PageOf(page, 10, Enumerable.Range(page * 10, 5).Select(i => Title(i)).ToArray());
        using var search = new SearchDiscoverModel(source);
        search.Search("  dune ");
        var ready = Assert.IsType<TmdbSection.Ready>(search.State.Value);
        Assert.Equal(("dune", 15), (ready.Query, ready.Titles.Count));
        Assert.Equal(3, source.Calls.Count(c => c.StartsWith("search dune")));
        // The same words aren't asked again; nothing typed hides the row.
        search.Search("dune");
        Assert.Equal(3, source.Calls.Count(c => c.StartsWith("search dune")));
        search.Search("");
        Assert.IsType<TmdbSection.Hidden>(search.State.Value);
        // Without access, no row.
        source.State.Value = new RequestsState.Open(RequestFixtures.Summary with { Access = RequestAccess.Off, Own = 1 });
        search.Search("alien");
        Assert.IsType<TmdbSection.Hidden>(search.State.Value);
    }

    [Fact]
    public void Search_says_why_tmdb_cant_be_searched()
    {
        source.Searched = (_, _) => throw new ApiException(503, "tmdb_key", "refused", System.Text.Json.JsonDocument.Parse(@"{""code"":""tmdb_key""}").RootElement);
        using var search = new SearchDiscoverModel(source);
        search.Search("dune");
        Assert.Equal(new RequestProblem.TmdbKey(), Assert.IsType<TmdbSection.Failed>(search.State.Value).Problem);
    }

    [Fact]
    public void A_library_title_shows_its_request_and_more_seasons_once_drawn()
    {
        var asked = new MediaRequest { Id = 50, Kind = TitleKind.Show, TmdbId = 1396, Status = RequestStatus.Approved, Stage = RequestStage.Waiting };
        var answer = new ItemRequest { Kind = TitleKind.Show, TmdbId = 1396, Status = TitleStatus.Partial, Request = asked };
        var reads = 0;
        var item = new ItemRequestSource(source, () =>
        {
            reads++;
            return reads == 1 ? answer : answer with { Askable = new[] { 3, 5 } };
        });
        using var model = new ItemRequestModel(item, scheduler, 7);
        Assert.Null(model.State.Value);
        model.Shown("show");
        Assert.Equal((50L, false), (model.State.Value!.Request!.Id, model.State.Value.More));
        // TMDB's details hadn't come: read again 3 s later, Request more seasons with them.
        scheduler.Advance(ItemRequestModel.AgainMs);
        Assert.True(model.State.Value!.More);
        Assert.Equal(new PageKey.DiscoverTitle(TitleKind.Show, 1396, Pick: true), model.State.Value.Picker);
        // Episodes have none.
        using var episode = new ItemRequestModel(item, scheduler, 8);
        episode.Shown("episode");
        Assert.Null(episode.State.Value);
    }
}

/// <summary>The app's requests over a server: the summary, what's kept, and what's told.</summary>
public class AppRequestsTests
{
    const string Home = "https://pley.example.com";
    const string MeJson = @"{""id"":1,""username"":""a"",""name"":""A"",""subtitle_mode"":""off""}";
    readonly MemoryKeyValue store = new MemoryKeyValue();
    readonly FakeScheduler scheduler = new FakeScheduler();
    readonly FakeServer server = new FakeServer();
    /// <summary>Hands the event stream text, as the server would send it.</summary>
    Action<string>? feed;

    async Task<(AppState, AppRequests)> Started()
    {
        server.On("GET", Home + "/api/auth/me", 200, MeJson);
        server.On("GET", Home + "/api/libraries", 200, "[]");
        var state = new AppState(new AppPlatform("tizen", "client", store) { Version = "0.1.0" }, server.Transport,
            (_, _, onOpen, onText, _) =>
            {
                feed = onText;
                onOpen();
                return new TaskCompletionSource<EventEnd>().Task;
            }, scheduler);
        state.Use(Home);
        await state.TokenStore!.Install(new StoredTokens(Home, "pley_at_0", "pley_rt_0", scheduler.Now + 3_600_000, "library play requests", Home + "/t", Home + "/r"), "library play requests");
        await state.LoadAccount();
        var requests = new AppRequests(state, scheduler);
        requests.Start();
        return (state, requests);
    }

    static string Summary(int unseen = 0, bool enabled = true) =>
        $@"{{""enabled"":{(enabled ? "true" : "false")},""access"":""ask"",""unseen"":{unseen},""own"":2,""changed_at"":1}}";

    const string ReadyList = @"{""total"":1,""requests"":[{""id"":45,""kind"":""movie"",""tmdb_id"":3,""title"":""Arrival"",""status"":""available"",""stage"":""available"",""item_id"":101,""updated_at"":500}]}";

    [Fact]
    public async Task The_summary_shows_the_way_in_and_is_kept_for_the_next_start()
    {
        server.On("GET", Home + "/api/requests/summary", 200, Summary());
        var (state, requests) = await Started();
        Assert.True(requests.State.Value.MayAsk);
        // The next start shows it before the server answers.
        server.On("GET", Home + "/api/requests/summary", 0);
        var again = new AppRequests(state, scheduler);
        again.Start();
        Assert.True(again.State.Value.MayAsk);
    }

    [Fact]
    public async Task Signing_in_again_to_the_same_server_reads_the_summary_again_on_Home()
    {
        server.On("GET", Home + "/api/requests/summary", 200, @"{""enabled"":true,""access"":""ask"",""sign_in_again"":""requests""}");
        var (state, requests) = await Started();
        Assert.IsType<RequestsState.SignInAgain>(requests.State.Value);
        // The new sign-in's tokens replace the old ones; the server, its API and the profile stay the same.
        server.On("GET", Home + "/api/requests/summary", 200, Summary());
        state.Use(Home);
        await state.TokenStore!.Install(new StoredTokens(Home, "pley_at_1", "pley_rt_1", scheduler.Now + 3_600_000, "library play requests", Home + "/t", Home + "/r"), "library play requests");
        Assert.IsType<RequestsState.SignInAgain>(requests.State.Value);
        // Home shows again after the sign-in.
        requests.Start();
        await Task.Yield();
        Assert.True(requests.State.Value.MayAsk);
    }

    [Fact]
    public async Task An_older_server_is_taken_as_requests_being_off()
    {
        server.On("GET", Home + "/api/requests/summary", 404, @"{""code"":""not_found"",""message"":""no""}");
        var (_, requests) = await Started();
        Assert.IsType<RequestsState.Hidden>(requests.State.Value);
        var asked = server.Requests.Count(r => r.Url.EndsWith("/summary"));
        requests.Refresh();
        Assert.Equal(asked, server.Requests.Count(r => r.Url.EndsWith("/summary")));
    }

    [Fact]
    public async Task What_became_ready_while_away_is_told_once_across_starts()
    {
        server.On("GET", Home + "/api/requests/summary", 200, Summary(unseen: 1));
        server.On("GET", Home + "/api/requests?unseen=1&offset=0&limit=20", 200, ReadyList);
        var (state, requests) = await Started();
        var notice = Assert.IsType<RequestNotice.Ready>(Assert.Single(requests.Notices.Value));
        Assert.Equal(("Arrival", 101L, true), (notice.Title, notice.ItemId, notice.Complete));
        // The mark is kept: a new start tells nothing again.
        var next = new AppRequests(state, scheduler);
        next.Start();
        Assert.Empty(next.Notices.Value);
        requests.Dismiss(notice);
        Assert.Empty(requests.Notices.Value);
    }

    [Fact]
    public async Task Events_tell_what_is_ready_and_gather_into_one_summary_read()
    {
        server.On("GET", Home + "/api/requests/summary", 200, Summary());
        var (state, requests) = await Started();
        var changes = new List<RequestChange>();
        requests.Changed += changes.Add;
        var reads = server.Requests.Count(r => r.Url.EndsWith("/summary"));
        state.Listen(true);
        Assert.True(state.Streaming.Value);
        changes.Clear();
        reads = server.Requests.Count(r => r.Url.EndsWith("/summary"));
        void Hear(string json) => feed!("data: " + json + "\n\n");
        Hear(@"{""type"":""request_available"",""request_id"":47,""title"":""Severance"",""kind"":""show"",""tmdb_id"":95396,""item_id"":201,""seasons"":[2],""complete"":false}");
        Hear(@"{""type"":""request_decided"",""request_id"":46,""title"":""Alien"",""kind"":""movie"",""tmdb_id"":9,""status"":""declined""}");
        Hear(@"{""type"":""requests_changed""}");
        // On a TV only what is ready pops up.
        var ready = Assert.IsType<RequestNotice.Ready>(Assert.Single(requests.Notices.Value));
        Assert.Equal("Season 2 of “Severance” is ready to watch.", RequestText.Notice(ready));
        Assert.Equal(3, changes.Count);
        Assert.True(changes[2].Many);
        scheduler.Advance(AppRequests.DebounceMs);
        Assert.Equal(reads + 1, server.Requests.Count(r => r.Url.EndsWith("/summary")));
    }

    [Fact]
    public async Task Seeing_the_requests_clears_the_badge_at_once()
    {
        server.On("GET", Home + "/api/requests/summary", 200, Summary(unseen: 3));
        server.On("GET", Home + "/api/requests?unseen=1&offset=0&limit=20", 200, @"{""total"":0,""requests"":[]}");
        server.On("POST", Home + "/api/requests/seen", 204);
        var (_, requests) = await Started();
        Assert.Equal(3, RequestsEntry.Of(requests.State.Value).Badge);
        await requests.Seen();
        Assert.Equal(0, RequestsEntry.Of(requests.State.Value).Badge);
    }

    [Fact]
    public async Task Asking_sends_the_title_and_tells_whether_it_was_new()
    {
        server.On("GET", Home + "/api/requests/summary", 200, Summary());
        server.On("POST", Home + "/api/requests", 201, @"{""id"":60,""kind"":""show"",""tmdb_id"":1396,""title"":""Breaking Bad"",""status"":""pending""}");
        var (_, requests) = await Started();
        var asked = await requests.Ask(new NewRequest(TitleKind.Show, 1396, new[] { 3, 5 }));
        Assert.True(asked.Created);
        Assert.Equal(@"{""kind"":""show"",""tmdb_id"":1396,""seasons"":[3,5]}", server.Requests.Last(r => r.Method == "POST").Body);
    }
}

/// <summary>A source whose item request comes from <c>answer</c>; the rest is <c>inner</c>'s.</summary>
sealed class ItemRequestSource : IRequestsSource
{
    readonly FakeRequests inner;
    readonly Func<ItemRequest?> answer;

    public ItemRequestSource(FakeRequests inner, Func<ItemRequest?> answer)
    {
        this.inner = inner;
        this.answer = answer;
    }

    public Signal<RequestsState> State => inner.State;
    public Signal<IReadOnlyList<RequestNotice>> Notices => inner.Notices;
    public Signal<bool> Streaming => inner.Streaming;
    public event Action<RequestChange>? Changed { add => inner.Changed += value; remove => inner.Changed -= value; }
    public string Origin => inner.Origin;
    public void Start() => inner.Start();
    public void Refresh() => inner.Refresh();
    public DiscoverPage? SavedRow(DiscoverList list) => inner.SavedRow(list);
    public Task<DiscoverPage> Page(DiscoverList list, int page) => inner.Page(list, page);
    public Task<DiscoverPage> Search(string query, int page = 1, bool missing = false) => inner.Search(query, page, missing);
    public DiscoverDetail? CachedTitle(string kind, long tmdbId) => inner.CachedTitle(kind, tmdbId);
    public Task<DiscoverDetail> Title(string kind, long tmdbId) => inner.Title(kind, tmdbId);
    public Task<ItemRequest?> ItemRequest(long itemId) => Task.FromResult(answer());
    public Task<RequestList> List(RequestQuery query) => inner.List(query);
    public Task<MediaRequest?> Request(long id) => inner.Request(id);
    public Task<Asked> Ask(NewRequest request) => inner.Ask(request);
    public Task Delete(MediaRequest request) => inner.Delete(request);
    public Task Unwatch(MediaRequest request) => inner.Unwatch(request);
    public Task<MediaRequest> Approve(long id, Approval approval) => inner.Approve(id, approval);
    public Task<MediaRequest> Decline(long id, string? reason) => inner.Decline(id, reason);
    public Task<MediaRequest> Handled(long id) => inner.Handled(id);
    public Task Seen() => inner.Seen();
    public void Dismiss(RequestNotice notice) => inner.Dismiss(notice);
}
