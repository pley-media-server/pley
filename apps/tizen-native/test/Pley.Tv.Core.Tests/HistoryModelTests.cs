using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using Pley.Tv.Core.Api;
using Pley.Tv.Core.Ui;
using Xunit;
using static Pley.Tv.Core.Tests.HistoryKit;
using static Pley.Tv.Core.Tests.PagesKit;

namespace Pley.Tv.Core.Tests;

/// <summary>Watch history: the History tests of the Android app's AccountViewModelsTest, and the rules of its TvHistoryFocusTest.</summary>
public class HistoryModelTests
{
    /// <summary>A scheduler whose clock reads <see cref="HistoryKit.Now"/>.</summary>
    sealed class At : IScheduler
    {
        public long Now => HistoryKit.Now * 1000;
        public long Elapsed => 0;
        public IDisposable After(long ms, Action action) => new FakeScheduler.Cancelled();
        public void Post(Action action) => action();
    }

    readonly At scheduler = new At();

    [Fact]
    public void History_pages_by_15_and_removes_at_once()
    {
        var history = new FakeHistory(40);
        var model = new HistoryModel(history, scheduler);
        var first = Assert.IsType<HistoryState.Ready>(model.State.Value);
        Assert.Equal(15, first.Rows.Count);
        Assert.True(first.CanLoadMore);
        Assert.Equal("40 plays", HistoryText.Count(first.Total));
        Assert.Equal("Show more", HistoryText.MoreLabel(first));

        model.LoadMore();
        model.LoadMore();
        var all = Assert.IsType<HistoryState.Ready>(model.State.Value);
        Assert.Equal(40, all.Rows.Count);
        Assert.False(all.CanLoadMore);
        Assert.Equal(new[] { (0, 15), (15, 15), (30, 15) }, history.Asked);

        model.Remove(3);
        var after = Assert.IsType<HistoryState.Ready>(model.State.Value);
        Assert.Equal(39, after.Total);
        Assert.DoesNotContain(after.Rows, r => r.EntryId == 3);
        Assert.Equal(new[] { 3L }, history.Removed);
    }

    [Fact]
    public void Asked_again_it_keeps_as_many_plays_as_were_shown()
    {
        var history = new FakeHistory(40);
        var model = new HistoryModel(history, scheduler);
        model.LoadMore();
        history.Changed();
        Assert.Equal((0, 30), history.Asked.Last());
        Assert.Equal(30, Assert.IsType<HistoryState.Ready>(model.State.Value).Rows.Count);
    }

    [Fact]
    public void A_refused_removal_comes_back_and_is_told()
    {
        var history = new FakeHistory(5) { RefuseRemoval = new IOException("offline") };
        var model = new HistoryModel(history, scheduler);
        var errors = new List<Exception>();
        model.Failed += errors.Add;
        model.Remove(2);
        Assert.Equal(5, Assert.IsType<HistoryState.Ready>(model.State.Value).Rows.Count);
        Assert.Single(errors);
    }

    [Fact]
    public void History_rows_say_what_when_and_where()
    {
        var episode = new HistoryEntry
        {
            Id = 1,
            ViewedAt = Now - 3 * 3600,
            Device = "Chrome on Windows",
            Item = new ItemSummary { Id = 205, LibraryId = 2, Kind = "episode", Title = "Glasshouse", Index = 5, ParentIndex = 2, ParentId = 202, GrandparentId = 201, GrandparentTitle = "Driftline" },
        };
        var row = HistoryRules.Row(episode, Now, Origin);
        Assert.Equal("Driftline", row.Title);
        Assert.Equal("S2 · E5 · Glasshouse", row.Subtitle);
        Assert.Equal("3 hours ago", row.Time);
        Assert.Equal("Chrome on Windows", row.Device);
        Assert.Equal("S2 · E5 · Glasshouse · 3 hours ago · Chrome on Windows", row.Line);
        Assert.Equal(new PageKey.Item(205), row.Opens);
        Assert.Equal(new[] { "http://pley/api/items/202/images/poster", "http://pley/api/items/201/images/poster" }, row.Art);
        Assert.False(row.Square);

        var song = HistoryRules.Row(episode with
        {
            Device = " ",
            Item = new ItemSummary { Id = 9, LibraryId = 3, Kind = "track", Title = "Swell", ParentId = 301, ParentTitle = "Tides", GrandparentTitle = "Harbour Lights" },
        }, Now, Origin);
        Assert.Equal("Harbour Lights · Tides", song.Subtitle);
        Assert.Equal(new AlbumPage(301, TrackId: 9), song.Opens);
        Assert.True(song.Square);
        Assert.Null(song.Device);
        Assert.Equal("Remove Swell from history", song.RemoveLabel);
    }

    [Fact]
    public void Focus_goes_to_the_play_below_a_removed_one_else_above()
    {
        var model = new HistoryModel(new FakeHistory(3), scheduler);
        var rows = Assert.IsType<HistoryState.Ready>(model.State.Value).Rows;
        Assert.Equal(2L, HistoryRules.StandIn(rows, 1));
        Assert.Equal(2L, HistoryRules.StandIn(rows, 3));
        Assert.Null(HistoryRules.StandIn(rows.Take(1).ToList(), 1));
    }

    [Fact]
    public void No_plays_offer_back_and_a_failure_retry()
    {
        var empty = new HistoryModel(new FakeHistory(0), scheduler);
        var message = HistoryText.Message(empty.State.Value)!;
        Assert.Equal("No history yet", message.Title);
        Assert.Equal(MessageAction.Back, message.Action);

        var history = new FakeHistory(3) { RefusePage = new IOException("no") };
        var failed = new HistoryModel(history, scheduler);
        Assert.Equal("Couldn't load your watch history", HistoryText.Message(failed.State.Value)!.Title);
        history.RefusePage = null;
        failed.Retry();
        Assert.Equal(3, Assert.IsType<HistoryState.Ready>(failed.State.Value).Rows.Count);
        Assert.Equal("3 plays", HistoryText.Count(3));
        Assert.Equal("1 play", HistoryText.Count(1));
    }

    [Theory]
    [InlineData(0L, "now")]
    [InlineData(-45L, "45 seconds ago")]
    [InlineData(-60L, "1 minute ago")]
    [InlineData(-3 * 3600L, "3 hours ago")]
    [InlineData(-86_400L, "yesterday")]
    [InlineData(-8 * 86_400L, "last week")]
    [InlineData(5 * 3600L, "in 5 hours")]
    [InlineData(-400 * 86_400L, "last year")]
    public void Times_read_as_the_webs_relative_words(long ago, string words) => Assert.Equal(words, Format.Relative(Now + ago, Now));

    [Fact]
    public void No_time_is_never() => Assert.Equal("never", Format.Relative(null, Now));
}
