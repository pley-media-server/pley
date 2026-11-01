using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using Pley.Tv.Core.Api;
using Pley.Tv.Core.Ui;
using Xunit;
using static Pley.Tv.Core.Tests.PagesKit;
using static Pley.Tv.Core.Tests.PlaylistKit;

namespace Pley.Tv.Core.Tests;

/// <summary>Add to playlist, New playlist, Save these filters and the playlists page: the Android app's PlaylistSheetsTest.</summary>
public class PlaylistSheetsTests
{
    readonly FakePlaylists repo = new FakePlaylists();
    readonly FakeScheduler scheduler = new FakeScheduler();

    static readonly IReadOnlyList<Playlist> Lists = new[]
    {
        new Playlist { Id = 1, Title = "Road trip", Kind = PlaylistKind.Video, ItemCount = 12, DurationMs = 11_520_000, Artwork = new[] { "/a", "/b" } },
        new Playlist { Id = 2, Title = "Comedies", Kind = PlaylistKind.Video, SmartFilter = "genre=Comedy", LibraryId = 1 },
        new Playlist { Id = 3, Title = "Mix", Kind = PlaylistKind.Audio, ItemCount = 1 },
    };

    (AddToPlaylistModel Model, List<AddEvent> Events) Sheet(params long[] ids)
    {
        var model = new AddToPlaylistModel(repo, ids, scheduler);
        var events = new List<AddEvent>();
        model.Told += events.Add;
        return (model, events);
    }

    static ItemSummary Show(long id, string title, int? episodes) =>
        new ItemDetail { Id = id, LibraryId = 2, Kind = "show", Title = title, Year = 2022, LeafCount = episodes };

    static ItemSummary EpisodeSeven => new ItemDetail { Id = 7, LibraryId = 2, Kind = "episode", Title = "Half Loop", Index = 2, ParentIndex = 1, GrandparentTitle = "Severance" };

    [Fact]
    public void It_says_what_is_added_and_offers_only_playlists_that_take_it()
    {
        repo.List = Lists;
        repo.Items[7] = EpisodeSeven;
        var state = Sheet(7).Model.State.Value;
        Assert.Equal("Severance — S1 · E2 — Half Loop", state.Title);
        Assert.Equal(PlaylistKind.Video, state.Kind);
        // Smart playlists fill themselves; music ones don't take episodes.
        var cards = state.Playlists.Value!;
        Assert.Equal(new[] { "Road trip" }, cards.Select(c => c.Title));
        Assert.Equal("12 items · 3h 12m", cards.Single().Meta);
        Assert.Equal(new[] { "http://pley/a", "http://pley/b" }, cards.Single().Artwork);
        Assert.Equal("New video playlist", PlaylistText.AddNewPlaceholder(state));
        Assert.Null(PlaylistText.AddNone(state));
    }

    [Fact]
    public void While_the_item_loads_nothing_is_known()
    {
        repo.List = Lists;
        var state = Sheet(7).Model.State.Value;
        Assert.Null(state.Title);
        Assert.Null(state.Kind);
        Assert.False(state.CanCreate);
    }

    [Fact]
    public void Adding_says_how_many_entries_went_in()
    {
        repo.List = Lists;
        repo.Items[8] = Show(8, "Severance", episodes: 19);
        var (model, events) = Sheet(8);
        model.Add(model.State.Value.Playlists.Value!.Single());
        Assert.Equal(new[] { "add 1 [8]" }, repo.Sent);
        var added = Assert.IsType<AddEvent.Added>(events.Single());
        Assert.Equal(new AddEvent.Added("Road trip", 19), added);
        Assert.Equal("Added 19 items to Road trip", PlaylistText.Added(added));
        Assert.False(model.State.Value.Busy);
    }

    [Fact]
    public void The_toasts_counts_are_the_webs()
    {
        var one = new ItemSummary { Id = 1, LibraryId = 1, Kind = "movie", Title = "Heat" };
        var empty = Show(2, "Nothing yet", episodes: 0);
        var unknown = Show(3, "Unknown", episodes: null);
        Assert.Equal(1, PlaylistRules.EntriesOf(new[] { one }));
        Assert.Equal(0, PlaylistRules.EntriesOf(new[] { empty }));
        Assert.Null(PlaylistRules.EntriesOf(new[] { one, unknown }));
        Assert.Equal("3 items", PlaylistRules.TitleOf(new[] { one, empty, unknown }));
        Assert.Equal("Heat", PlaylistRules.TitleOf(new[] { one }));
        Assert.Equal("Heat (1995)", PlaylistRules.FullTitle(one with { Year = 1995 }));
        Assert.Equal("Nothing to add", PlaylistText.Added(new AddEvent.Added("Road trip", 0)));
        Assert.Equal("Added to Road trip", PlaylistText.Added(new AddEvent.Added("Road trip", 1)));
        Assert.Equal("Added to Road trip", PlaylistText.Added(new AddEvent.Added("Road trip", null)));
    }

    [Fact]
    public void A_new_playlist_is_made_on_the_spot_with_the_items()
    {
        repo.List = Lists;
        repo.Items[7] = EpisodeSeven;
        var (model, events) = Sheet(7);
        Assert.False(model.Create("   "));
        Assert.True(model.Create(" Sunday "));
        Assert.Equal(new[] { "create Sunday video [7]" }, repo.Sent);
        Assert.Equal(new AddEvent.Added("Sunday", 1), events.Single());
    }

    [Fact]
    public void A_refusal_is_told_and_the_sheet_stays()
    {
        repo.List = Lists;
        repo.Items[7] = EpisodeSeven;
        var (model, events) = Sheet(7);
        repo.Fail = new IOException("refused");
        model.Add(model.State.Value.Playlists.Value!.Single());
        Assert.Equal("refused", Assert.IsType<AddEvent.Failed>(events.Single()).Error.Message);
        Assert.False(model.State.Value.Busy);
    }

    [Fact]
    public void Music_offers_music_playlists()
    {
        repo.List = Lists.Where(p => p.Kind == PlaylistKind.Video).ToList();
        repo.Items[9] = Track(9, "Swell");
        var state = Sheet(9).Model.State.Value;
        Assert.Equal("Swell — Harbour Lights", state.Title);
        Assert.True(state.Music);
        Assert.Empty(state.Playlists.Value!);
        Assert.Equal("No music playlists yet: name one above.", PlaylistText.AddNone(state));
    }

    [Fact]
    public void New_playlist_asks_what_it_plays_only_when_empty()
    {
        var empty = new NewPlaylistModel(repo, NewPlaylistMode.Of(new NewPlaylistPage())!, scheduler);
        var withItems = new NewPlaylistModel(repo, new NewPlaylistMode.Items(new long[] { 5 }), scheduler);
        var events = new List<CreateEvent>();
        empty.Told += events.Add;
        Assert.True(empty.AsksKind);
        Assert.False(withItems.AsksKind);
        Assert.False(empty.Create(""));
        Assert.False(empty.Create(new string('x', 201)));
        Assert.True(empty.Create("Chill", PlaylistKind.Audio));
        Assert.True(withItems.Create("Picks"));
        Assert.Equal(new[] { "create Chill audio []", "create Picks null [5]" }, repo.Sent);
        Assert.Equal(new CreateEvent[] { new CreateEvent.Created(100, "Chill") }, events);
        Assert.Equal("New playlist", PlaylistText.NewTitle(empty));
        Assert.Equal(("Create", "plus"), PlaylistText.NewAction(empty));
    }

    [Fact]
    public void Saving_filters_makes_a_smart_playlist()
    {
        var model = new NewPlaylistModel(repo, NewPlaylistMode.Of(new SaveSmartPlaylistPage(1, "genre=Comedy&unwatched=1"))!, scheduler);
        Assert.Equal(new[] { "Genre: Comedy", "Unwatched" }, model.Filters);
        Assert.False(model.AsksKind);
        Assert.Equal("Save these filters", PlaylistText.NewTitle(model));
        Assert.Equal("Genre: Comedy · Unwatched. It keeps up with the library by itself.", PlaylistText.NewBodyOf(model));
        Assert.Equal(("Save", "save"), PlaylistText.NewAction(model));
        Assert.True(model.Create("Comedies"));
        Assert.Equal(new[] { "smart Comedies 1 genre=Comedy&unwatched=1" }, repo.Sent);
    }

    [Fact]
    public void The_playlists_page_makes_one_and_opens_it()
    {
        repo.List = Lists;
        var model = new PlaylistsModel(repo, scheduler);
        var events = new List<CreateEvent>();
        model.Told += events.Add;
        var cards = model.State.Value.Value!;
        Assert.Equal(new[] { false, true, false }, cards.Select(c => c.Smart));
        Assert.False(cards[2].Video);
        Assert.Equal("12 items · 3h 12m", cards[0].Caption);
        Assert.Equal("0 items · Smart", cards[1].Caption);
        Assert.True(model.Create("Weekend", PlaylistKind.Video));
        Assert.Equal(new CreateEvent[] { new CreateEvent.Created(100, "Weekend") }, events);
        Assert.False(model.Busy.Value);
    }

    [Fact]
    public void The_playlists_page_says_when_there_are_none_or_they_failed()
    {
        repo.List = Array.Empty<Playlist>();
        var model = new PlaylistsModel(repo, scheduler);
        Assert.Equal("No playlists yet", PlaylistText.PlaylistsMessage(model.State.Value)!.Title);
        Assert.True(PlaylistText.ShowsNew(model.State.Value));
        // A playlist made elsewhere in the app shows when the page hears of it.
        repo.List = Lists;
        repo.Changed();
        Assert.Equal(3, model.State.Value.Value!.Count);
    }
}
