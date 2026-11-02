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

/// <summary>A playlist's page: the Android app's PlaylistViewModelTest, and the rules of its TvPlaylistTest.</summary>
public class PlaylistModelTests
{
    readonly FakePlaylists repo = new FakePlaylists();
    readonly FakeScheduler scheduler = new FakeScheduler();
    readonly List<PlayRequest> played = new List<PlayRequest>();
    readonly List<PlaylistEvent> events = new List<PlaylistEvent>();

    static readonly PlaylistDetail RoadTrip = Playlist(1, "Road trip", new[]
    {
        Film(10, "Northern Lights"),
        Episode(20, "The End of the World", "Doctor Who", 1, 2, durationMs: 3_000_000, positionMs: 600_000),
        Film(30, "Low Orbit", durationMs: 3_000_000),
    });

    PlaylistModel Model(int seed = 1)
    {
        var model = new PlaylistModel(repo, 1, scheduler, new Random(seed));
        model.PlayRequested += played.Add;
        model.Told += events.Add;
        return model;
    }

    static PlaylistPage Page(PlaylistModel model) => model.State.Value.Value!;

    static IReadOnlyList<long> Order(PlaylistModel model) => Page(model).Rows.Select(r => r.EntryId).ToList();

    [Fact]
    public void Entries_are_numbered_and_worded_as_on_the_web()
    {
        repo.Set(RoadTrip);
        var page = Page(Model());
        Assert.Equal("3 items · 3h 20m", page.Meta);
        Assert.True(page.Video);
        Assert.True(page.Editable);
        Assert.Equal(new[] { 1, 2, 3 }, page.Rows.Select(r => r.Number));
        var episode = page.Rows[1];
        Assert.Equal("The End of the World", episode.Title);
        Assert.Equal("Doctor Who · S1 · E2", episode.Subtitle);
        Assert.Equal("50m", episode.Duration);
        Assert.Equal("1. Northern Lights, 2024, 1h 40m", page.Rows[0].Description);
        // Video entries are 16:9 stills, with how much was played.
        Assert.Equal(0.2f, episode.Tile.Progress, 3);
        Assert.Equal("Video playlist", PlaylistText.KindLine(page));
        Assert.Equal(PlaylistButton.Play, PlaylistText.FirstFocus(page));
        Assert.Equal(new[] { "Play", "Shuffle", "Rename", "Delete" }, PlaylistText.Buttons(page).Select(b => b.Label));
    }

    [Fact]
    public void A_smart_playlist_is_read_only_and_says_its_filter()
    {
        repo.Set(Playlist(1, "Comedies", new[] { Film(10, "A"), Film(11, "B") }, smart: "genre=Comedy&unwatched=1"));
        var model = Model();
        var page = Page(model);
        Assert.True(page.Smart);
        Assert.False(page.Editable);
        Assert.Equal(new[] { "Genre: Comedy", "Unwatched" }, page.Notes);
        Assert.Equal("Smart playlist", PlaylistText.KindLine(page));
        Assert.Equal("2 items · 3h 20m  ·  Genre: Comedy  ·  Unwatched", PlaylistText.FactsLine(page));
        // No Rename, and its entries offer no Move or Remove.
        Assert.Equal(new[] { "Play", "Shuffle", "Delete" }, PlaylistText.Buttons(page).Select(b => b.Label));
        Assert.Equal(new[] { EntryAction.PlayFromHere, EntryAction.GoTo }, PlaylistText.EntryActions(page.Rows[0], page).Select(a => a.Action));
        model.Move(0, 1);
        model.Remove(1, undoable: false);
        Assert.Equal(new[] { 1L, 2L }, Order(model));
        Assert.Empty(repo.Sent);
    }

    [Fact]
    public void An_empty_smart_playlist_starts_on_delete()
    {
        repo.Set(Playlist(1, "Comedies", Array.Empty<ItemSummary>(), smart: "genre=Comedy"));
        var page = Page(Model());
        Assert.Equal(PlaylistButton.Delete, PlaylistText.FirstFocus(page));
        Assert.Equal(PlaylistText.EmptySmart, PlaylistText.EmptyLine(page));
    }

    [Fact]
    public void The_order_maths_is_the_webs()
    {
        var ids = new[] { "a", "b", "c", "d" };
        Assert.Equal(new[] { "b", "c", "a", "d" }, PlaylistRules.Moved(ids, 0, 2));
        Assert.Equal(new[] { "d", "a", "b", "c" }, PlaylistRules.Moved(ids, 3, 0));
        Assert.Equal(new[] { "a", "c", "b", "d" }, PlaylistRules.Moved(ids, 2, 1));
        Assert.Equal(ids, PlaylistRules.Moved(ids, 4, 0));
        Assert.Equal(ids, PlaylistRules.Moved(ids, -1, 2));
        // Past the end goes last.
        Assert.Equal(new[] { "b", "c", "d", "a" }, PlaylistRules.Moved(ids, 0, 9));
    }

    [Fact]
    public void A_move_shows_at_once_and_goes_to_the_server()
    {
        repo.Set(RoadTrip);
        var model = Model();
        repo.Gate = true;
        model.Move(2, 0);
        // Shown before the server answers.
        Assert.Equal(new[] { 3L, 1L, 2L }, Order(model));
        Assert.Equal(new[] { 1, 2, 3 }, Page(model).Rows.Select(r => r.Number));
        repo.Open();
        Assert.Equal(new[] { "reorder 1 [3, 1, 2]" }, repo.Sent);
        Assert.Equal(new[] { 3L, 1L, 2L }, Order(model));
    }

    [Fact]
    public void A_refused_move_puts_the_order_back_and_says_why()
    {
        repo.Set(RoadTrip);
        var model = Model();
        repo.Gate = true;
        repo.Fail = new IOException("the server refused");
        model.Move(0, 2);
        Assert.Equal(new[] { 2L, 3L, 1L }, Order(model));
        repo.Open();
        Assert.Equal(new[] { 1L, 2L, 3L }, Order(model));
        Assert.Equal("the server refused", Assert.IsType<PlaylistEvent.Failed>(events.Single()).Error.Message);
    }

    [Fact]
    public void Moves_made_while_one_is_on_its_way_send_the_latest_order_only()
    {
        repo.Set(RoadTrip);
        var model = Model();
        repo.Gate = true;
        model.Move(0, 1);
        model.Move(1, 2);
        model.Move(0, 1);
        Assert.Equal(new[] { 3L, 2L, 1L }, Order(model));
        // The latest order waited for the first's answer, and goes after it.
        repo.Open();
        Assert.Equal(new[] { "reorder 1 [2, 1, 3]", "reorder 1 [3, 2, 1]" }, repo.Sent);
        Assert.Equal(new[] { 3L, 2L, 1L }, Order(model));
    }

    [Fact]
    public void A_tv_move_mode_sends_once_or_puts_the_order_back()
    {
        repo.Set(RoadTrip);
        var model = Model();
        // Up and Down move the lifted entry without sending.
        model.MoveBy(1, 1, send: false);
        model.MoveBy(1, 1, send: false);
        Assert.Equal(new[] { 2L, 3L, 1L }, Order(model));
        Assert.Empty(repo.Sent);
        // Back puts it back.
        model.CancelMoves();
        Assert.Equal(new[] { 1L, 2L, 3L }, Order(model));
        // OK drops it there and sends the order.
        model.Move(2, 0, send: false);
        model.SendOrder();
        Assert.Equal(new[] { "reorder 1 [3, 1, 2]" }, repo.Sent);
    }

    [Fact]
    public void A_removal_can_be_undone_until_it_is_sent()
    {
        repo.Set(RoadTrip);
        var model = Model();
        model.Remove(2, undoable: true);
        Assert.Equal(new[] { 1L, 3L }, Order(model));
        Assert.Equal("2 items · 2h 30m", Page(model).Meta);
        Assert.Equal(new PlaylistEvent.Removed(2, "The End of the World"), events.Single());
        Assert.Equal("Removed “The End of the World”", PlaylistText.Removed("The End of the World"));
        scheduler.Advance(PlaylistRules.UndoMs - 1);
        model.Undo(2);
        scheduler.Advance(10_000);
        Assert.Equal(new[] { 1L, 2L, 3L }, Order(model));
        Assert.Empty(repo.Sent);
        // Not undone: sent once the undo has had its time.
        model.Remove(2, undoable: true);
        scheduler.Advance(PlaylistRules.UndoMs + 1);
        Assert.Equal(new[] { "remove 1 [2]" }, repo.Sent);
        Assert.Equal(new[] { 1L, 3L }, Order(model));
    }

    [Fact]
    public void A_refused_removal_brings_the_entry_back()
    {
        repo.Set(RoadTrip);
        var model = Model();
        repo.Fail = new IOException("no");
        model.Remove(3, undoable: false);
        Assert.Equal(new[] { 1L, 2L, 3L }, Order(model));
        Assert.IsType<PlaylistEvent.Failed>(events.Last());
    }

    [Fact]
    public void Leaving_the_page_sends_removals_still_waiting()
    {
        repo.Set(RoadTrip);
        var model = Model();
        model.Remove(1, undoable: true);
        model.Dispose();
        Assert.Equal(new[] { "remove 1 [1]" }, repo.Sent);
        // The timer it had goes too: nothing is sent twice.
        scheduler.Advance(PlaylistRules.UndoMs * 2);
        Assert.Single(repo.Sent);
    }

    [Fact]
    public void Focus_goes_to_the_entry_below_a_removed_one_else_above_else_delete()
    {
        repo.Set(RoadTrip);
        var rows = Page(Model()).Rows;
        Assert.Equal(3L, PlaylistRules.StandIn(rows, 2));
        Assert.Equal(2L, PlaylistRules.StandIn(rows, 3));
        Assert.Null(PlaylistRules.StandIn(rows.Take(1).ToList(), 1));
    }

    [Fact]
    public void Renaming_checks_the_title()
    {
        repo.Set(RoadTrip);
        var model = Model();
        Assert.False(model.Rename("   "));
        Assert.False(model.Rename(new string('x', PlaylistRules.TitleMax + 1)));
        Assert.True(model.Rename("  Long drive  "));
        Assert.Equal(new[] { "rename 1 Long drive" }, repo.Sent);
        Assert.Equal("Long drive", Page(model).Title);
        Assert.Equal(TitleProblem.Empty, PlaylistRules.Check(""));
        Assert.Equal(TitleProblem.TooLong, PlaylistRules.Check(new string('x', 201)));
        Assert.Null(PlaylistRules.Check(new string('x', 200)));
    }

    [Fact]
    public void Video_plays_as_a_queue_of_the_playlist()
    {
        repo.Set(RoadTrip);
        var model = Model(seed: 3);
        model.Play();
        Assert.Equal(new PlayRequest(10, 0, false, Queue: new PlayQueue.Playlist(1)), played.Last());
        model.Play(from: 1);
        // Started before: it resumes.
        Assert.Equal(new PlayRequest(20, 600_000, false, Queue: new PlayQueue.Playlist(1)), played.Last());
        model.Play(shuffle: true);
        var start = RoadTrip.Entries[new Random(3).Next(3)].Item.Id;
        Assert.Equal(start, played.Last().ItemId);
        Assert.Equal(new PlayQueue.Playlist(1), played.Last().Queue);
    }

    [Fact]
    public void Music_opens_the_tracks_album()
    {
        repo.Set(Playlist(1, "Mix", new[] { Track(70, "Tidal", album: 50) }, kind: PlaylistKind.Audio));
        var model = Model();
        var page = Page(model);
        Assert.False(page.Video);
        Assert.Equal("Music playlist", PlaylistText.KindLine(page));
        model.Play();
        Assert.Empty(played);
        Assert.Equal(new PlaylistEvent.Open(new AlbumPage(50)), events.Single());
        Assert.Equal(new AlbumPage(50), model.Opens(page.Rows[0]));
    }

    [Fact]
    public void Deleting_says_when_it_is_gone()
    {
        repo.Set(RoadTrip);
        var model = Model();
        model.Delete();
        Assert.Equal(new[] { "delete 1" }, repo.Sent);
        Assert.IsType<PlaylistEvent.Deleted>(events.Single());
    }

    [Fact]
    public void A_missing_playlist_is_not_found_and_another_failure_offers_retry()
    {
        repo.Missing[1] = new ApiException(404, "not_found", "gone");
        var model = Model();
        Assert.IsType<Loadable<PlaylistPage>.Failed>(model.State.Value);
        var message = PlaylistText.PlaylistMessage(model.State.Value)!;
        Assert.Equal("Playlist not found", message.Title);
        Assert.Equal(MessageAction.Back, message.Action);
        repo.Missing[1] = new IOException("gone");
        model.Retry();
        Assert.Equal(MessageAction.Retry, PlaylistText.PlaylistMessage(model.State.Value)!.Action);
    }

    [Fact]
    public void An_entrys_actions_are_the_tvs()
    {
        repo.Set(RoadTrip);
        var page = Page(Model());
        var actions = PlaylistText.EntryActions(page.Rows[0], page);
        Assert.Equal(new[] { "Play from here", "Go to Northern Lights", "Move", "Remove from playlist" }, actions.Select(a => a.Label));
        Assert.True(actions[3].Danger);
        // Move needs another entry to move past.
        repo.Set(Playlist(1, "One", new[] { Film(10, "Northern Lights") }));
        var single = Page(Model());
        Assert.False(PlaylistText.EntryActions(single.Rows[0], single).Single(a => a.Action == EntryAction.Move).Enabled);
    }
}
