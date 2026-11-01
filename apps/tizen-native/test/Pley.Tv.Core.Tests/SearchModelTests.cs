using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using Pley.Tv.Core.Api;
using Pley.Tv.Core.Ui;
using Xunit;
using static Pley.Tv.Core.Tests.PagesKit;

namespace Pley.Tv.Core.Tests;

/// <summary>Search: the Android app's SearchViewModelTest.</summary>
public class SearchModelTests
{
    readonly FakeSearch search = new FakeSearch();
    readonly FakeScheduler scheduler = new FakeScheduler();

    SearchModel Model(string initial = "") => new SearchModel(search, scheduler, initial);

    static SearchAnswer Answer(params ItemSummary[] items) => new SearchAnswer(items, Array.Empty<PersonSummary>());

    [Fact]
    public void Typing_waits_for_250ms_of_quiet_before_asking()
    {
        var model = Model();
        model.Type("n");
        scheduler.Advance(100);
        model.Type("no");
        scheduler.Advance(100);
        model.Type("nor");
        scheduler.Advance(249);
        Assert.Empty(search.Asked);
        scheduler.Advance(2);
        Assert.Equal(new[] { "nor" }, search.Asked);
        Assert.Equal(new SearchResultsState.Loading("nor"), model.State.Value.Results);
        Assert.True(model.State.Value.Searching);
    }

    [Fact]
    public void Only_the_latest_querys_answer_counts()
    {
        var model = Model();
        model.Type("north");
        scheduler.Advance(300);
        model.Type("south");
        scheduler.Advance(300);
        Assert.Equal(new[] { "north", "south" }, search.Asked);
        // The older request was given up.
        Assert.Equal(new[] { "north" }, search.Cancelled);
        search.Answer("north").SetResult(Answer(Item(1, "movie", "Northern Lights")));
        Assert.Equal(new SearchResultsState.Loading("south"), model.State.Value.Results);
        search.Answer("south").SetResult(Answer(Item(2, "movie", "South Wind")));
        var ready = Assert.IsType<SearchResultsState.Ready>(model.State.Value.Results);
        Assert.Equal("south", ready.Query);
        Assert.Equal(new[] { "South Wind" }, ready.Groups.Single().Tiles.Select(t => t.Title));
    }

    [Fact]
    public void Results_stay_while_the_next_ones_load()
    {
        var model = Model();
        model.Type("north");
        search.Answer("north").SetResult(Answer(Item(1, "movie")));
        scheduler.Advance(300);
        Assert.False(Assert.IsType<SearchResultsState.Ready>(model.State.Value.Results).Refreshing);
        model.Type("northe");
        scheduler.Advance(300);
        var kept = Assert.IsType<SearchResultsState.Ready>(model.State.Value.Results);
        Assert.Equal("north", kept.Query);
        Assert.True(kept.Refreshing);
        search.Answer("northe").SetResult(Answer());
        Assert.Equal(new SearchResultsState.Empty("northe"), model.State.Value.Results);
        Assert.Equal("No results for “northe”", SearchText.Message(model.State.Value)!.Title);
    }

    [Fact]
    public void Results_come_in_the_webs_groups_and_order_with_counts()
    {
        var model = Model("lights");
        search.Answer("lights").SetResult(new SearchAnswer(
            new[]
            {
                Item(1, "track", "Lights", parentId: 40),
                Item(2, "episode"),
                Item(3, "movie"),
                Item(4, "album"),
                Item(5, "show"),
                Item(6, "movie"),
                Item(7, "extra"),
                Item(8, "season"),
                Item(9, "artist"),
            },
            new[] { new PersonSummary { Id = 70, Name = "Lena Okafor", PhotoUrl = "/api/people/70/photo" } }));
        scheduler.Advance(300);
        var groups = Assert.IsType<SearchResultsState.Ready>(model.State.Value.Results).Groups;
        Assert.Equal(
            new[]
            {
                SearchGroupKind.People, SearchGroupKind.Movies, SearchGroupKind.Shows, SearchGroupKind.Seasons,
                SearchGroupKind.Episodes, SearchGroupKind.Artists, SearchGroupKind.Albums, SearchGroupKind.Tracks,
            },
            groups.Select(g => g.Kind));
        var movies = groups.First(g => g.Kind == SearchGroupKind.Movies);
        Assert.Equal(2, movies.Count);
        Assert.Equal("Movies", movies.Title);
        // The server's order of relevance holds inside a group.
        Assert.Equal(new[] { 3L, 6L }, movies.Ids);
        Assert.Equal(TileKind.Landscape, groups.First(g => g.Kind == SearchGroupKind.Episodes).TileKind);
        Assert.Equal(TileKind.Square, groups.First(g => g.Kind == SearchGroupKind.Albums).TileKind);
        Assert.Equal(TileKind.Poster, movies.TileKind);
        var person = groups.First();
        Assert.Equal(TileKind.Person, person.TileKind);
        Assert.Equal(new[] { "http://pley/api/people/70/photo" }, person.Tiles.Single().Images);
        // Opening: a person's page, a track's album, an album's page, the rest their item page.
        Assert.Equal(new PageKey.Person(70), model.Open(SearchGroupKind.People, 70));
        Assert.Equal(new AlbumPage(40), model.Open(SearchGroupKind.Tracks, 1));
        Assert.Equal(new AlbumPage(4), model.Open(SearchGroupKind.Albums, 4));
        Assert.Equal(new ArtistPage(9), model.Open(SearchGroupKind.Artists, 9));
        Assert.Equal(new PageKey.Item(3), model.Open(SearchGroupKind.Movies, 3));
        Assert.Null(model.Open(SearchGroupKind.Movies, 99));
    }

    [Fact]
    public void A_query_is_remembered_when_submitted_or_one_of_its_results_is_opened()
    {
        var model = Model();
        Assert.Equal(new[] { "halcyon" }, model.State.Value.Recent);
        Assert.Equal(new SearchResultsState.Idle(), model.State.Value.Results);
        Assert.True(model.State.Value.HasBelow);
        model.Type(" north ");
        model.Submit();
        Assert.Equal(new[] { "north", "halcyon" }, model.State.Value.Recent);
        Assert.Equal(new[] { "north", "halcyon" }, search.Kept);
        model.Type("orbit");
        search.Answer("orbit").SetResult(Answer(Item(1, "movie")));
        scheduler.Advance(300);
        model.Open(SearchGroupKind.Movies, 1);
        Assert.Equal(new[] { "orbit", "north", "halcyon" }, model.State.Value.Recent);
        // A recent search searches again, and moves to the top.
        model.SearchAgain("halcyon");
        scheduler.Advance(300);
        Assert.Equal("halcyon", model.State.Value.Query);
        Assert.Equal("halcyon", search.Asked.Last());
        Assert.Equal(new[] { "halcyon", "orbit", "north" }, model.State.Value.Recent);
        model.ClearRecent();
        Assert.Empty(model.State.Value.Recent);
        Assert.Empty(search.Kept);
    }

    [Fact]
    public void Recent_searches_are_ten_at_most_each_once_whatever_its_case()
    {
        var recent = Enumerable.Range(1, 10).Select(i => $"q{i}").ToList();
        Assert.Equal(new[] { "Q3", "q1", "q2", "q4", "q5", "q6", "q7", "q8", "q9", "q10" }, SearchRules.Remembered(recent, " Q3 "));
        Assert.Equal("new", SearchRules.Remembered(recent, "new")[0]);
        Assert.Equal(10, SearchRules.Remembered(recent, "new").Count);
        Assert.Same(recent, SearchRules.Remembered(recent, "  "));
    }

    [Fact]
    public void Clearing_the_field_goes_back_to_recent_searches_at_once()
    {
        var model = Model("north");
        search.Answer("north").SetResult(Answer(Item(1, "movie")));
        scheduler.Advance(300);
        model.Type("  ");
        Assert.Equal(new SearchResultsState.Idle(), model.State.Value.Results);
        Assert.Equal("  ", model.State.Value.Query);
    }

    [Fact]
    public void A_failure_says_why()
    {
        var model = Model("north");
        search.Answer("north").SetException(new IOException("Can't reach the server"));
        scheduler.Advance(300);
        var failed = Assert.IsType<SearchResultsState.Failed>(model.State.Value.Results);
        Assert.Equal("north", failed.Query);
        Assert.Equal("Can't reach the server", failed.Error.Message);
        var message = SearchText.Message(model.State.Value)!;
        Assert.Equal("Search failed", message.Title);
        Assert.Equal(MessageAction.Retry, message.Action);
    }

    [Fact]
    public void Retry_asks_for_the_same_query_again()
    {
        var model = Model("north");
        search.Answer("north").SetException(new IOException("Can't reach the server"));
        scheduler.Advance(300);
        Assert.IsType<SearchResultsState.Failed>(model.State.Value.Results);
        search.Forget("north");
        model.Retry();
        search.Answer("north").SetResult(Answer(Item(1, "movie", "Northern Lights")));
        Assert.Equal(new[] { "north", "north" }, search.Asked);
        Assert.IsType<SearchResultsState.Ready>(model.State.Value.Results);
    }

    [Fact]
    public void Nothing_typed_and_nothing_recent_says_what_search_finds()
    {
        search.Kept = new List<string>();
        var model = Model();
        Assert.False(model.State.Value.HasBelow);
        Assert.Equal(SearchText.EmptyTitle, SearchText.Message(model.State.Value)!.Title);
    }

    [Fact]
    public void Leaving_gives_up_a_search_on_its_way()
    {
        var model = Model("north");
        scheduler.Advance(300);
        model.Dispose();
        Assert.Equal(new[] { "north" }, search.Cancelled);
    }
}
