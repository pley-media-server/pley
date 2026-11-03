using System.Collections.Generic;
using System.Linq;
using Pley.Tv.Core.Api;
using Pley.Tv.Core.Ui;
using Xunit;
using static Pley.Tv.Core.Tests.LibraryKit;

namespace Pley.Tv.Core.Tests;

/// <summary>The TV's Libraries page: the Android app's LibrariesViewModelTest, and the sorting TvLibraries.kt does.</summary>
public class LibrariesModelTests
{
    readonly FakeLibraries data = new FakeLibraries(Movies, Shows, Music);
    readonly FakeScheduler scheduler = new FakeScheduler();
    readonly List<LibrariesEvent> told = new List<LibrariesEvent>();

    LibrariesModel Model()
    {
        var model = new LibrariesModel(data, scheduler);
        model.Told += told.Add;
        return model;
    }

    static LibrariesState.Ready Ready(LibrariesModel model) => Assert.IsType<LibrariesState.Ready>(model.State.Value);

    static IEnumerable<string> Names(LibrariesModel model) => Ready(model).Libraries.Select(e => e.Library.Name);

    [Fact]
    public void A_move_shows_at_once_and_is_saved_for_the_profile()
    {
        var model = Model();
        Assert.True(Ready(model).Sortable);

        model.Move(2, 0);
        Assert.Equal(new[] { "Music", "Movies", "Shows" }, Names(model));
        Assert.Equal(new long[] { 2, 1, 4 }, data.Orders.Single());

        // By one place, from where it is shown.
        model.MoveBy(Movies.Id, 1);
        Assert.Equal(new[] { "Music", "Shows", "Movies" }, Names(model));
        Assert.Equal(new long[] { 2, 4, 1 }, data.Orders.Last());
        // Not past either end.
        model.MoveBy(Movies.Id, 1);
        model.MoveBy(Music.Id, -1);
        Assert.Equal(2, data.Orders.Count);
        Assert.Empty(told);
    }

    [Fact]
    public void A_tvs_moves_are_saved_when_the_library_is_dropped_or_forgotten()
    {
        var model = Model();
        model.MoveBy(Movies.Id, 1, save: false);
        model.MoveBy(Movies.Id, 1, save: false);
        Assert.Equal(new[] { "Shows", "Music", "Movies" }, Names(model));
        Assert.Empty(data.Orders);
        // Back puts it where it was.
        model.CancelMoves();
        Assert.Equal(new[] { "Movies", "Shows", "Music" }, Names(model));

        model.MoveBy(Shows.Id, -1, save: false);
        model.SaveOrder();
        Assert.Equal(new long[] { 4, 1, 2 }, data.Orders.Single());
        Assert.Equal(new[] { "Shows", "Movies", "Music" }, Names(model));

        model.UseServerOrder();
        Assert.Null(data.Orders.Last());
    }

    [Fact]
    public void An_order_the_server_refuses_goes_back_and_says_so()
    {
        var refused = new ApiException(409, "conflict", "your libraries have changed: the order must list each of them once");
        data.OrderFailure = refused;
        var model = Model();
        model.Move(0, 2);
        Assert.Equal(new[] { "Movies", "Shows", "Music" }, Names(model));
        Assert.Equal(new LibrariesEvent[] { new LibrariesEvent.Failed(refused) }, told);
    }

    [Fact]
    public void A_server_older_than_sorting_says_so()
    {
        data.Libraries.Value = new[] { Movies, Shows };
        data.OrderFailure = new ApiException(404, "not_found", "no such endpoint");
        var model = Model();
        model.Move(0, 1);
        Assert.Equal(new[] { "Movies", "Shows" }, Names(model));
        Assert.Equal(new LibrariesEvent[] { new LibrariesEvent.NeedsNewerServer() }, told);
        Assert.Equal("Sorting libraries needs a newer Pley on the server.", LibraryText.SortNeedsNewerServer);
    }

    [Fact]
    public void Only_two_or_more_libraries_with_a_sign_in_allowed_to_play_can_be_sorted()
    {
        data.Libraries.Value = new[] { Movies };
        var model = Model();
        Assert.False(Ready(model).Sortable);

        data.Libraries.Value = new[] { Movies, Shows };
        Assert.True(Ready(model).Sortable);
        data.Me.Value = new User { Id = 4, Username = "ann", Name = "Ann", Scope = "library" };
        Assert.False(Ready(model).Sortable);
        model.Move(0, 1);
        Assert.Empty(data.Orders);
        data.Me.Value = new User { Id = 4, Username = "ann", Name = "Ann", Scope = "library play" };
        Assert.True(Ready(model).Sortable);
    }

    [Fact]
    public void Each_library_says_what_it_holds_and_whether_it_is_scanning()
    {
        data.Libraries.Value = new[] { Movies, Shows with { Scanning = true }, Music, new Library { Id = 5, Name = "Home videos", Kind = "other", ItemCount = 1 } };
        data.Scans.Value = new[] { new ScanState(Movies.Id, 0.5f) };
        var entries = Ready(Model()).Libraries;
        Assert.Equal(new[] { "342 movies · Scanning…", "38 shows · Scanning…", "120 artists", "1 item" }, entries.Select(LibraryText.CardLine));
    }

    [Fact]
    public void Nothing_known_waits_and_a_failure_with_nothing_saved_says_why()
    {
        data.Libraries.Value = null;
        var model = Model();
        Assert.IsType<LibrariesState.Loading>(model.State.Value);
        var error = new ApiException(500, "internal", "the server failed");
        data.LibrariesError.Value = error;
        Assert.Same(error, Assert.IsType<LibrariesState.Failed>(model.State.Value).Error);
        Assert.Equal(new LibraryMessage(LibraryText.LibrariesFailed, "The server failed.", MessageAction.Retry), LibraryText.LibrariesMessage(model.State.Value));
        model.Refresh();
        Assert.Equal(1, data.Refreshes);
        data.Libraries.Value = new Library[0];
        Assert.Equal(new LibraryMessage(LibraryText.NoLibraries, LibraryText.NoLibrariesBody), LibraryText.LibrariesMessage(model.State.Value));
    }

    [Fact]
    public void Sorting_on_tv_lifts_moves_and_drops_a_card()
    {
        var model = Model();
        Assert.Equal(new PageKey.Library(Movies.Id), model.Select(Movies.Id));
        model.StartSorting();
        Assert.True(model.Sorting.Value);
        Assert.Equal(LibraryText.SortLibraries, LibraryText.LibrariesHeading(model.Sorting.Value));

        // OK lifts it; the arrows move it without saving; OK drops it and saves.
        Assert.Null(model.Select(Movies.Id));
        Assert.Equal(Movies.Id, model.Moving.Value);
        Assert.Equal(LibraryText.MovingHint, LibraryText.SortHint(true, true));
        Assert.All(LibraryText.LibrariesButtons(true, true, true), b => Assert.False(b.Enabled));
        model.MoveLifted(1);
        model.MoveLifted(1);
        Assert.Equal(new[] { "Shows", "Music", "Movies" }, Names(model));
        Assert.Empty(data.Orders);
        Assert.Null(model.Select(Movies.Id));
        Assert.Null(model.Moving.Value);
        Assert.Equal(new long[] { 4, 2, 1 }, data.Orders.Single());

        // Back puts a lifted card back, then ends sorting, then is the page's.
        model.Select(Music.Id);
        model.MoveLifted(-1);
        Assert.True(model.Back());
        Assert.Equal(new[] { "Shows", "Music", "Movies" }, Names(model));
        Assert.True(model.Sorting.Value);
        Assert.True(model.Back());
        Assert.False(model.Sorting.Value);
        Assert.False(model.Back());
    }

    [Fact]
    public void A_card_moved_down_onto_a_shorter_last_row_lands_at_its_end()
    {
        Assert.Equal(3, LibrariesRules.MoveTarget(1, LibrariesRules.Columns, 4));
        Assert.Equal(3, LibrariesRules.MoveTarget(2, LibrariesRules.Columns, 4));
        // From the last row, down goes nowhere; up from the first neither.
        Assert.Equal(3, LibrariesRules.MoveTarget(3, LibrariesRules.Columns, 4));
        Assert.Equal(1, LibrariesRules.MoveTarget(1, -LibrariesRules.Columns, 4));
        Assert.Equal(0, LibrariesRules.MoveTarget(3, -LibrariesRules.Columns, 4));
    }

    [Fact]
    public void Move_in_a_cards_actions_sorts_with_it_lifted_and_sorting_ends_when_nothing_can_be_sorted()
    {
        var model = Model();
        Assert.Equal(new[] { LibrariesAction.Open, LibrariesAction.Move, LibrariesAction.ServerOrder }, LibraryText.CardActions(true).Select(a => a.Action));
        model.MoveFromActions(Shows.Id);
        Assert.True(model.Sorting.Value);
        Assert.Equal(Shows.Id, model.Moving.Value);

        data.Libraries.Value = new[] { Movies };
        Assert.False(model.Sorting.Value);
        Assert.Null(model.Moving.Value);
        Assert.Equal(new[] { LibrariesAction.Open }, LibraryText.CardActions(Ready(model).Sortable).Select(a => a.Action));
        Assert.Empty(LibraryText.LibrariesButtons(false, false, Ready(model).Sortable));
    }
}
