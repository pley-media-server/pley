using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using Pley.Tv.Core.Api;
using Pley.Tv.Core.Ui;
using Xunit;
using static Pley.Tv.Core.Tests.PagesKit;

namespace Pley.Tv.Core.Tests;

/// <summary>The collections' pages: the Android app's CollectionViewModelTest.</summary>
public class CollectionModelTests
{
    static readonly Library Movies = new Library { Id = 1, Name = "Movies", Kind = "movies" };
    static readonly Library Shows = new Library { Id = 2, Name = "Shows", Kind = "shows" };
    static readonly Library Music = new Library { Id = 3, Name = "Music", Kind = "music" };

    readonly FakeCollections data = new FakeCollections(Movies, Shows, Music);
    readonly FakeScheduler scheduler = new FakeScheduler();
    readonly List<PlayRequest> played = new List<PlayRequest>();
    readonly List<CollectionEvent> told = new List<CollectionEvent>();

    CollectionModel Model(long id, int seed = 1)
    {
        var model = new CollectionModel(data, id, scheduler, new Random(seed));
        model.PlayRequested += played.Add;
        model.Told += told.Add;
        return model;
    }

    static CollectionDetail Detail(long id, long libraryId, string title, ItemSummary[] items, string? sort = null, string? smart = null, bool automatic = false) =>
        new CollectionDetail { Id = id, LibraryId = libraryId, Title = title, Sort = sort, SmartFilter = smart, Automatic = automatic, ItemCount = items.Length, Items = items };

    static ItemSummary Show(long id) => Item(id, "show", $"Show {id}") with { LibraryId = 2 };

    [Fact]
    public void A_collection_says_what_it_holds_in_words()
    {
        data.Details[5] = Detail(5, 1, "Alien Collection", new[] { Film(1, "Alien"), Film(2, "Aliens") }, sort: "release", automatic: true) with
        {
            BackdropUrl = "/api/collections/5/images/backdrop",
        };
        data.Details[6] = Detail(6, 1, "Nineties comedies", Array.Empty<ItemSummary>(), smart: "genre=Comedy&decade=1990&sort=year");
        var page = Model(5).State.Value.Value!;
        Assert.True(page.Automatic);
        Assert.False(page.Smart);
        Assert.Equal("Movies", page.LibraryName);
        Assert.Equal(new[] { "By release date" }, page.Notes);
        Assert.True(page.Movies);
        Assert.True(page.CanShuffle);
        Assert.Equal("http://pley/api/collections/5/images/backdrop", page.Backdrop);
        Assert.Equal("2 movies  ·  Movies  ·  By release date", CollectionText.FactsLine(page));
        Assert.Equal(7, page.Columns);
        var smart = Model(6).State.Value.Value!;
        Assert.True(smart.Smart);
        Assert.Equal(new[] { "Genre: Comedy", "The 1990s", "Oldest first" }, smart.Notes);
        Assert.False(smart.CanPlay);
    }

    [Fact]
    public void Movies_play_in_order_as_a_queue()
    {
        data.Details[5] = Detail(5, 1, "Trilogy", new[] { Film(1, "One"), Film(2, "Two"), Film(3, "Three") });
        var model = Model(5, seed: 7);
        model.Play(shuffle: false);
        Assert.Equal(new PlayRequest(1, 0, false, Queue: new PlayQueue.Collection(5)), played.Last());
        model.Play(shuffle: true);
        var start = new Random(7).Next(3) + 1L;
        Assert.Equal(new PlayRequest(start, 0, false, Queue: new PlayQueue.Collection(5)), played.Last());
        Assert.Equal(new PageKey.Item(2), model.Opens(1));
        Assert.Null(model.Opens(3));
    }

    [Fact]
    public void Shows_play_their_next_episode()
    {
        data.Next[10] = Item(1_000, "episode");
        data.Details[7] = Detail(7, 2, "Shows", new[] { Show(10), Show(99) });
        var model = Model(7);
        Assert.False(model.State.Value.Value!.Movies);
        model.Play(shuffle: false);
        Assert.Equal(new[] { 10L }, data.Resolved);
        Assert.Equal(new PlayRequest(1_000, 0, false), played.Last());
        // A show with nothing to play says so.
        data.Details[7] = Detail(7, 2, "Shows", new[] { Show(99), Show(10) });
        data.Changed();
        model.Play(shuffle: false);
        Assert.Equal(new CollectionEvent[] { new CollectionEvent.NothingToPlay() }, told);
        Assert.Equal("Nothing to play: this show has no episodes yet", CollectionText.Said(told.Single()));
    }

    [Fact]
    public void A_failure_offers_retry()
    {
        data.Failures[5] = new IOException("the server refused");
        var model = Model(5);
        var message = CollectionText.CollectionMessage(model.State.Value)!;
        Assert.Equal("Couldn't load this collection", message.Title);
        Assert.Equal(MessageAction.Retry, message.Action);
        data.Failures.Remove(5);
        data.Details[5] = Detail(5, 1, "Trilogy", new[] { Film(1, "One") });
        model.Retry();
        Assert.Null(CollectionText.CollectionMessage(model.State.Value));
    }

    [Fact]
    public void The_collections_page_has_a_shelf_per_video_library()
    {
        data.Lists[1] = new[] { new Collection { Id = 5, LibraryId = 1, Title = "Alien Collection", ItemCount = 4 } };
        data.Lists[2] = new[] { new Collection { Id = 7, LibraryId = 2, Title = "Sitcoms", ItemCount = 3 } };
        var all = new CollectionsModel(data, scheduler);
        var one = new CollectionsModel(data, scheduler, only: 2);
        var shelves = all.State.Value.Value!;
        Assert.Equal(new[] { "Movies", "Shows" }, shelves.Select(s => s.LibraryName));
        Assert.Equal("3 shows", shelves[1].Collections.Value!.Single().Tile.Subtitle);
        Assert.Equal(new[] { 2L }, one.State.Value.Value!.Select(s => s.LibraryId));
        Assert.IsType<Loadable<IReadOnlyList<CollectionShelf>>.Ready>(one.State.Value);
        Assert.Null(CollectionText.CollectionsMessage(all.State.Value));
    }

    [Fact]
    public void Libraries_without_collections_are_left_out_and_none_at_all_says_so()
    {
        data.Lists[1] = new[] { new Collection { Id = 5, LibraryId = 1, Title = "Alien Collection", ItemCount = 4 } };
        var model = new CollectionsModel(data, scheduler);
        Assert.Equal(new[] { 1L }, CollectionText.Shelves(model.State.Value).Select(s => s.LibraryId));
        data.Lists.Clear();
        data.Changed();
        Assert.Empty(CollectionText.Shelves(model.State.Value));
        Assert.Equal("No collections yet", CollectionText.CollectionsMessage(model.State.Value)!.Title);
    }

    [Fact]
    public void Libraries_that_failed_offer_retry()
    {
        var failing = new FakeCollections();
        failing.Libraries.Value = null;
        failing.LibrariesError.Value = new IOException("refused");
        var model = new CollectionsModel(failing, scheduler);
        var message = CollectionText.CollectionsMessage(model.State.Value)!;
        Assert.Equal("Couldn't load collections", message.Title);
        model.Retry();
        Assert.Equal(1, failing.Refreshes);
    }
}
