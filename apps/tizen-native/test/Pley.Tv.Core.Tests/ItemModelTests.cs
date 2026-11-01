using System;
using System.Collections.Generic;
using System.Linq;
using System.Text.Json;
using System.Threading.Tasks;
using Pley.Tv.Core.Api;
using Pley.Tv.Core.Auth;
using Pley.Tv.Core.Ui;
using Xunit;
using static Pley.Tv.Core.Tests.ItemKit;

namespace Pley.Tv.Core.Tests;

/// <summary>An item page and an item's actions: the Android app's ItemViewModelTest and MyListActionsTest.</summary>
public class ItemModelTests
{
    readonly FakeItems data = new FakeItems();
    readonly FakeScheduler scheduler = new FakeScheduler();
    readonly List<PlayRequest> played = new List<PlayRequest>();
    readonly List<ItemEvent> told = new List<ItemEvent>();

    ItemModel Model(long id)
    {
        var model = new ItemModel(data, id, scheduler, new System.Globalization.CultureInfo("en-US"));
        model.PlayRequested += played.Add;
        model.Told += told.Add;
        return model;
    }

    ItemActionsModel Actions(long id, bool continueWatching = false)
    {
        var model = new ItemActionsModel(data, id, scheduler, continueWatching);
        model.PlayRequested += played.Add;
        model.Told += told.Add;
        return model;
    }

    static ItemPage PageOf(ItemModel model) => Assert.IsType<ItemState.Ready>(model.State.Value).Page;

    static IReadOnlyList<ItemAction> ActionsOf(ItemActionsModel model) => Assert.IsType<ItemActionsState.Ready>(model.State.Value).Actions;

    static ItemDetail Episode() => new ItemDetail
    {
        Id = 41, LibraryId = 2, Kind = "episode", Title = "Half Loop", ParentId = 31, GrandparentId = 10,
        UserState = new UserState { PositionMs = 5_000 }, DurationMs = 3_480_000,
    };

    [Fact]
    public void Nothing_known_shows_the_skeleton_then_the_page()
    {
        var model = Model(1);
        Assert.IsType<ItemState.Loading>(model.State.Value);
        data.Set(Movie());
        Assert.Equal("Saltwind", PageOf(model).Title);
    }

    [Fact]
    public void What_was_saved_shows_at_once_then_the_servers_answer()
    {
        data.SavedDetails[1] = Movie() with { Title = "Saltwind (saved)" };
        var model = Model(1);
        Assert.Equal("Saltwind (saved)", PageOf(model).Title);
        data.Set(Movie());
        Assert.Equal("Saltwind", PageOf(model).Title);
    }

    [Fact]
    public void A_404_is_not_found_another_failure_says_why()
    {
        var gone = Model(1);
        data.Fail(1, new ApiException(404, "not_found", "no such item"));
        Assert.IsType<ItemState.NotFound>(gone.State.Value);
        var broken = Model(2);
        var error = new ApiException(500, "internal", "the server failed");
        data.Fail(2, error);
        Assert.Same(error, Assert.IsType<ItemState.Failed>(broken.State.Value).Error);
    }

    [Fact]
    public void A_failure_with_something_saved_shows_it_and_says_so()
    {
        data.SavedDetails[1] = Movie();
        var model = Model(1);
        var error = new ApiException(500, "internal", "the server failed");
        data.Fail(1, error);
        var ready = Assert.IsType<ItemState.Ready>(model.State.Value);
        Assert.Equal("Saltwind", ready.Page.Title);
        Assert.Same(error, ready.Error);
    }

    [Fact]
    public void Music_and_extras_open_their_own_pages()
    {
        var cases = new (ItemDetail Detail, PageKey Key)[]
        {
            (new ItemDetail { Id = 60, LibraryId = 3, Kind = "artist", Title = "A" }, new ArtistPage(60)),
            (new ItemDetail { Id = 61, LibraryId = 3, Kind = "album", Title = "B" }, new AlbumPage(61)),
            (new ItemDetail { Id = 62, LibraryId = 3, Kind = "track", Title = "C", ParentId = 61 }, new AlbumPage(61)),
            (new ItemDetail { Id = 63, LibraryId = 1, Kind = "extra", Title = "Trailer", ParentId = 1 }, new PageKey.Item(1)),
        };
        foreach (var (detail, key) in cases)
        {
            var model = Model(detail.Id);
            data.Set(detail);
            Assert.Equal(new ItemState.Elsewhere(key), model.State.Value);
        }
    }

    [Fact]
    public void Resume_plays_from_where_it_was_left_with_the_chosen_tracks()
    {
        var model = Model(1);
        data.Set(Movie(positionMs: 60_000));
        model.ChooseAudio(2);
        model.ChooseSubtitle(3);
        model.Play();
        Assert.Equal(new PlayRequest(1, 60_000, false, AudioIndex: 2, SubtitleIndex: 3), Assert.Single(played));
        model.Play(fromStart: true);
        Assert.Equal(0L, played.Last().StartMs);
        Assert.True(played.Last().FromStart);
    }

    [Fact]
    public void Choosing_tracks_saves_them_for_the_item_as_the_webs_streams_body()
    {
        var model = Model(1);
        data.Set(Movie());
        model.ChooseAudio(2);
        Assert.Equal(new FakeItems.StreamWrite(1, 11, 2, null), data.Streams.Last());
        model.ChooseSubtitle(3);
        Assert.Equal(new FakeItems.StreamWrite(1, 11, 2, 3), data.Streams.Last());
        model.ChooseSubtitle(null);
        Assert.Equal(new FakeItems.StreamWrite(1, 11, 2, null), data.Streams.Last());
        // The page shows the picks at once.
        Assert.Equal(2, PageOf(model).Tracks?.AudioIndex);
        // The other saved details name the tracks of before; this one, with the choice, is kept.
        Assert.Equal(2, data.TracksChangedFor.Last()!.Files[0].SelectedAudio);
    }

    [Fact]
    public void The_audio_goes_to_the_player_and_the_saved_choice_only_when_picked_here()
    {
        var model = Model(1);
        // The details' audio is the profile's, chosen for no device: the server picks for this one.
        data.Set(Movie(files: new[] { File(11, SaltwindStreams(), selectedAudio: 2) }));
        Assert.True(PageOf(model).Tracks?.AutomaticAudio);
        Assert.Equal("French", PageOf(model).Tracks?.AutomaticLanguage);
        model.Play();
        Assert.Null(Assert.Single(played).AudioIndex);
        // Subtitles alone: the audio chosen before stays.
        model.ChooseSubtitle(3);
        Assert.Equal(new FakeItems.StreamWrite(1, 11, null, 3), data.Streams.Last());
        model.Play();
        Assert.Equal(3, played.Last().SubtitleIndex);
        Assert.Null(played.Last().AudioIndex);
    }

    [Fact]
    public void Automatic_forgets_the_audio_picked_and_the_player_gets_none()
    {
        var model = Model(1);
        data.Set(Movie());
        model.ChooseAudio(2);
        var asks = data.DetailAsks;
        model.ChooseAudio(null);
        Assert.Equal(new FakeItems.StreamWrite(1, 11, null, null, AutomaticAudio: true), data.Streams.Last());
        Assert.True(PageOf(model).Tracks?.AutomaticAudio);
        // Which track plays is the server's to work out again: the page asks for it.
        Assert.Equal(asks + 1, data.DetailAsks);
        model.Play();
        Assert.Null(played.Last().AudioIndex);
    }

    [Fact]
    public async Task The_streams_body_is_the_servers_wire_format()
    {
        const string home = "https://pley.example.com";
        var clock = new FakeClock();
        var server = new FakeServer();
        var tokens = new Tokens(home, "client", new MemoryKeyValue(), server.Transport, null, clock.Read);
        await tokens.Install(new StoredTokens(home, "pley_at_0", "pley_rt_0", clock.Now + 3_600_000, "library play", home + "/api/oauth/token", home + "/api/oauth/revoke"), "library play");
        var api = new Api.Api(home, tokens, server.Transport);
        server.On("PUT", home + "/api/items/1/streams", 204);
        string Body() => server.Requests.Last().Body!;

        await AppItemSource.SendStreams(api, 1, 11, 2, null, automaticAudio: false);
        Assert.Equal(@"{""file_id"":11,""audio_index"":2,""subtitle_index"":null}", Body());
        // Subtitles alone keep the audio chosen before: it is left out.
        await AppItemSource.SendStreams(api, 1, 11, null, 3, automaticAudio: false);
        Assert.Equal(@"{""file_id"":11,""subtitle_index"":3}", Body());
        // Automatic forgets it: its null is sent.
        await AppItemSource.SendStreams(api, 1, 11, null, null, automaticAudio: true);
        using var automatic = JsonDocument.Parse(Body());
        Assert.Equal(JsonValueKind.Null, automatic.RootElement.GetProperty("audio_index").ValueKind);
    }

    [Fact]
    public void A_version_is_played_but_not_saved()
    {
        var model = Model(1);
        data.Set(Movie(files: new[] { File(11, new[] { Stream(1, "audio", "aac", isDefault: true) }), File(12, new[] { Stream(1, "audio", "aac", isDefault: true) }) }));
        model.ChooseVersion(12);
        Assert.Empty(data.Streams);
        model.Play();
        Assert.Equal(12L, Assert.Single(played).FileId);
    }

    [Fact]
    public void A_show_plays_its_next_episode_picked_on_its_season()
    {
        var model = Model(10);
        data.Set(new ItemDetail { Id = 10, LibraryId = 2, Kind = "show", Title = "Driftline", LeafCount = 6, ChildCount = 2 });
        data.SetChildren(10, Summary(30, "season", index: 1), Summary(31, "season", index: 2));
        // The season waits for the next episode, so the page doesn't jump to it.
        Assert.Null(PageOf(model).Seasons?.Selected);
        var next = Summary(41, "episode", parentId: 31, parentIndex: 2, index: 1, positionMs: 5_000);
        data.SetOnDeck(10, next);
        data.SetChildren(31, next, Summary(42, "episode", parentId: 31, parentIndex: 2, index: 2));
        var page = PageOf(model);
        Assert.Equal((PrimaryKind.Resume, "S2 · E1", "58 min left"), (page.Primary.Kind, page.Primary.Episode, page.Primary.TimeLeft));
        Assert.Equal(5_000f / 3_480_000f, page.Primary.Progress, 4);
        Assert.Equal(31L, page.Seasons?.Selected);
        Assert.Equal(41L, page.Seasons?.Focus);
        Assert.Equal(new[] { 41L, 42L }, page.Seasons!.Episodes!.Select(e => e.Item.Id));
        model.Play();
        Assert.Equal(new PlayRequest(41, 5_000, false), Assert.Single(played));
        model.SelectSeason(30);
        Assert.Equal(30L, PageOf(model).Seasons?.Selected);
        Assert.Null(PageOf(model).Seasons?.Episodes);
    }

    [Fact]
    public void Nothing_to_play_is_told_not_played()
    {
        var model = Model(10);
        data.Set(new ItemDetail { Id = 10, LibraryId = 2, Kind = "show", Title = "Empty", LeafCount = 0 });
        data.SetOnDeck(10, null);
        model.Play();
        Assert.IsType<ItemEvent.NothingToPlay>(Assert.Single(told));
        Assert.Empty(played);
    }

    [Fact]
    public void Watched_toggles_and_ratings_go_to_the_server()
    {
        var model = Model(1);
        data.Set(Movie());
        model.ToggleWatched();
        Assert.Equal((1L, true), Assert.Single(data.Watched));
        Assert.True(PageOf(model).Watched);
        model.Rate(7);
        Assert.Equal(7, PageOf(model).UserRating);
        model.Rate(null);
        Assert.Equal(new (long, double?)[] { (1, 7.0), (1, null) }, data.Ratings);
    }

    [Fact]
    public void Marking_an_episode_of_the_row_counts_it_for_its_season()
    {
        var model = Model(31);
        data.Set(new ItemDetail { Id = 31, LibraryId = 2, Kind = "season", Title = "Season 2", Index = 2, ParentId = 10, LeafCount = 2, ViewedLeafCount = 1 });
        data.SetOnDeck(31, null);
        data.SetChildren(10, Summary(31, "season", index: 2));
        data.SetChildren(31, Summary(41, "episode", parentId: 31, grandparentId: 10) with { UserState = new UserState { Watched = true } }, Summary(42, "episode", parentId: 31, grandparentId: 10));
        Assert.False(PageOf(model).Watched);
        model.SetWatched(42, true);
        Assert.True(PageOf(model).Watched);
        Assert.True(PageOf(model).Seasons!.Episodes!.Single(e => e.Item.Id == 42).Watched);
    }

    [Fact]
    public void The_page_asks_again_when_things_changed()
    {
        var model = Model(1);
        data.Set(Movie());
        data.Set(Movie(positionMs: 60_000));
        Assert.Equal(PrimaryKind.Play, PageOf(model).Primary.Kind);
        data.Changed();
        Assert.Equal(PrimaryKind.Resume, PageOf(model).Primary.Kind);
        model.Dispose();
        data.Set(Movie());
        data.Changed();
        Assert.Equal(PrimaryKind.Resume, PageOf(model).Primary.Kind);
    }

    [Fact]
    public void A_tiles_actions_play_mark_and_open()
    {
        var model = Model(10);
        data.Set(new ItemDetail { Id = 10, LibraryId = 2, Kind = "show", Title = "Driftline", LeafCount = 6 });
        var episode = Summary(41, "episode", parentId: 31, grandparentId: 10, positionMs: 5_000);
        var opened = new List<PageKey>();
        model.OpenRequested += opened.Add;
        model.Perform(ItemAction.FromStart, episode);
        Assert.Equal(new PlayRequest(41, 0, true), Assert.Single(played));
        model.Perform(ItemAction.MarkWatched, episode);
        Assert.Equal((41L, true), Assert.Single(data.Watched));
        model.Perform(ItemAction.GoToSeason, episode);
        model.Perform(ItemAction.AddToPlaylist, episode);
        Assert.Equal(new PageKey[] { new PageKey.Item(31), new AddToPlaylistPage(41) }, opened);
    }

    [Fact]
    public void Actions_are_never_an_administrators_whatever_the_profile_is()
    {
        var kinds = new[] { "movie", "show", "season", "episode", "extra", "album", "artist", "track" };
        var allowed = new HashSet<ItemAction>
        {
            ItemAction.Play, ItemAction.Resume, ItemAction.FromStart, ItemAction.MarkWatched, ItemAction.MarkUnwatched,
            ItemAction.AddToPlaylist, ItemAction.GoToShow, ItemAction.GoToSeason, ItemAction.Open,
            ItemAction.RemoveFromContinueWatching, ItemAction.AddToMyList, ItemAction.RemoveFromMyList,
        };
        foreach (var kind in kinds)
        {
            Assert.Subset(allowed, ItemActions.Of(Summary(1, kind, parentId: 2, grandparentId: 3)).ToHashSet());
            Assert.Subset(allowed, ItemActions.Of(Summary(1, kind), inWatchlist: true, continueWatching: true).ToHashSet());
        }
        Assert.Equal(allowed, Enum.GetValues(typeof(ItemAction)).Cast<ItemAction>().ToHashSet());
    }

    [Fact]
    public void An_episode_in_progress_offers_Resume_From_start_Watched_Playlist_its_show_and_season()
    {
        var episode = Summary(41, "episode", parentId: 31, grandparentId: 10, positionMs: 5_000);
        Assert.Equal(
            new[] { ItemAction.Resume, ItemAction.FromStart, ItemAction.MarkWatched, ItemAction.AddToPlaylist, ItemAction.GoToShow, ItemAction.GoToSeason },
            ItemActions.Of(episode));
        Assert.Equal(new PageKey.Item(10), ItemActions.Destination(ItemAction.GoToShow, episode));
        Assert.Equal(new PageKey.Item(31), ItemActions.Destination(ItemAction.GoToSeason, episode));
        Assert.Equal(new AddToPlaylistPage(41), ItemActions.Destination(ItemAction.AddToPlaylist, episode));
        Assert.Equal(new[] { ItemAction.Open, ItemAction.AddToPlaylist }, ItemActions.Of(Summary(61, "album")));
    }

    [Fact]
    public void The_action_sheet_plays_and_marks_and_its_work_outlives_it()
    {
        var model = Actions(1);
        data.Set(Movie(positionMs: 60_000));
        Assert.Equal(ItemAction.Resume, ActionsOf(model)[0]);
        Assert.True(model.Perform(ItemAction.FromStart));
        Assert.Equal(0L, Assert.Single(played).StartMs);
        Assert.True(model.Perform(ItemAction.MarkWatched));
        Assert.Equal((1L, true), Assert.Single(data.Watched));
        Assert.False(model.Perform(ItemAction.AddToPlaylist));
    }

    [Fact]
    public void A_play_asked_for_as_the_sheet_closes_still_comes()
    {
        var model = Actions(10);
        data.Set(new ItemDetail { Id = 10, LibraryId = 2, Kind = "show", Title = "Driftline", LeafCount = 6 });
        Assert.True(model.Perform(ItemAction.Play));
        model.Dispose();
        data.SetOnDeck(10, Summary(41, "episode", positionMs: 5_000));
        Assert.Equal(new PlayRequest(41, 5_000, false), Assert.Single(played));
    }

    // ---- My List (MyListActionsTest)

    [Fact]
    public void A_movies_page_says_whether_it_is_in_My_List_and_the_button_changes_it()
    {
        var model = Model(1);
        data.Set(Movie());
        Assert.False(PageOf(model).MyList);

        model.ToggleMyList();
        Assert.Equal(new[] { (1L, true) }, data.Listed);
        Assert.True(PageOf(model).MyList);

        model.ToggleMyList();
        Assert.Equal((1L, false), data.Listed.Last());
        Assert.False(PageOf(model).MyList);
    }

    [Fact]
    public void Episodes_and_seasons_have_no_My_List()
    {
        var model = Model(41);
        data.Set(Episode());
        Assert.Null(PageOf(model).MyList);
        model.ToggleMyList();
        Assert.Empty(data.Listed);
    }

    [Fact]
    public void A_refused_change_is_told_and_the_page_shows_what_it_was()
    {
        var model = Model(1);
        data.Set(Movie());
        data.ListFailure = new ApiException(500, "internal", "the server failed");

        model.ToggleMyList();

        Assert.False(PageOf(model).MyList);
        Assert.IsType<ItemEvent.Failed>(Assert.Single(told));
    }

    [Fact]
    public void The_actions_offer_My_List_for_movies_and_shows_by_what_the_page_knows()
    {
        var movie = Summary(1, "movie");
        Assert.Equal(new[] { ItemAction.Play, ItemAction.MarkWatched, ItemAction.AddToMyList, ItemAction.AddToPlaylist }, ItemActions.Of(movie, inWatchlist: false));
        Assert.Contains(ItemAction.RemoveFromMyList, ItemActions.Of(Summary(10, "show"), inWatchlist: true));
        // Unknown (a tile's summary), or not a movie or show: not offered.
        Assert.DoesNotContain(ItemActions.Of(movie), a => a is ItemAction.AddToMyList or ItemAction.RemoveFromMyList);
        Assert.DoesNotContain(ItemAction.AddToMyList, ItemActions.Of(Summary(41, "episode"), inWatchlist: false));
    }

    [Fact]
    public void Opened_from_Continue_Watching_the_actions_take_the_item_out_of_it()
    {
        data.Set(Episode());
        var fromRow = Actions(41, continueWatching: true);
        var elsewhere = Actions(41);
        Assert.Equal(
            new[] { ItemAction.Resume, ItemAction.FromStart, ItemAction.MarkWatched, ItemAction.RemoveFromContinueWatching },
            ActionsOf(fromRow).Take(4));
        Assert.DoesNotContain(ItemAction.RemoveFromContinueWatching, ActionsOf(elsewhere));

        Assert.True(fromRow.Perform(ItemAction.RemoveFromContinueWatching));
        Assert.Equal(new[] { 41L }, data.Hidden);
    }

    [Fact]
    public void The_actions_add_to_My_List_and_take_off_it()
    {
        data.Set(Movie());
        var model = Actions(1);
        Assert.Contains(ItemAction.AddToMyList, ActionsOf(model));

        Assert.True(model.Perform(ItemAction.AddToMyList));
        Assert.Equal(new[] { (1L, true) }, data.Listed);
        Assert.Contains(ItemAction.RemoveFromMyList, ActionsOf(model));

        Assert.True(model.Perform(ItemAction.RemoveFromMyList));
        Assert.Equal((1L, false), data.Listed.Last());
    }

    [Fact]
    public void The_sheet_names_an_episode_by_its_show()
    {
        data.Set(Episode() with { GrandparentTitle = "Driftline", ParentIndex = 2, Index = 5 });
        var ready = Assert.IsType<ItemActionsState.Ready>(Actions(41).State.Value);
        Assert.Equal(("Driftline", "S2 · E5 · Half Loop"), (ready.Title, ready.Subtitle));
    }
}
