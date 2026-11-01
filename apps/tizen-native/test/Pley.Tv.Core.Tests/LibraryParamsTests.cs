using System.Linq;
using Pley.Tv.Core.Api;
using Pley.Tv.Core.Ui;
using Xunit;

namespace Pley.Tv.Core.Tests;

/// <summary>The library page's address, as the web's LibraryPage reads and writes it, and the listing it asks for: the Android app's LibraryParamsTest.</summary>
public class LibraryParamsTests
{
    [Fact]
    public void A_fresh_page_is_the_library_tab_by_title()
    {
        var p = LibraryParams.Parse("");
        Assert.Equal(new LibraryParams(), p);
        Assert.Equal("", p.ToQueryString());
        Assert.Equal(new LibraryQuery { Sort = LibrarySort.Title, Order = SortOrder.Ascending }, p.Query());
    }

    [Fact]
    public void The_query_round_trips()
    {
        var p = new LibraryParams(LibraryTab.Library, LibrarySort.Year, SortOrder.Ascending, false, "Science Fiction", 1999);
        var query = p.ToQueryString();
        Assert.Equal("sort=year&order=asc&watched=false&genre=Science+Fiction&year=1999", query);
        Assert.Equal(p, LibraryParams.Parse(query));
    }

    [Fact]
    public void Orders_at_their_default_are_left_out()
    {
        Assert.Equal("sort=added", new LibraryParams(Sort: LibrarySort.Added, Order: SortOrder.Descending).ToQueryString());
        Assert.Equal("order=desc", new LibraryParams(Order: SortOrder.Descending).ToQueryString());
        Assert.Equal("tab=collections", new LibraryParams(LibraryTab.Collections).ToQueryString());
    }

    [Fact]
    public void Sorts_have_the_webs_default_orders()
    {
        Assert.Equal(SortOrder.Ascending, new LibraryParams(Sort: LibrarySort.Title).EffectiveOrder);
        foreach (var sort in LibraryParams.AllSorts.Where(s => s != LibrarySort.Title))
            Assert.Equal(SortOrder.Descending, new LibraryParams(Sort: sort).EffectiveOrder);
        // Choosing a sort forgets the order chosen for the one before.
        var flipped = new LibraryParams(Sort: LibrarySort.Year).Flipped();
        Assert.Equal(SortOrder.Ascending, flipped.EffectiveOrder);
        Assert.Equal(SortOrder.Descending, flipped.SortedBy(LibrarySort.Rating).EffectiveOrder);
    }

    [Fact]
    public void Unknown_values_fall_back()
    {
        Assert.Equal(new LibraryParams(), LibraryParams.Parse("tab=nope&sort=loudness&order=up&watched=maybe&year=soon&genre="));
        // A route's tab, unless the query names one.
        Assert.Equal(LibraryTab.Recommended, LibraryParams.Parse("", LibraryTab.Recommended).Tab);
        Assert.Equal(LibraryTab.Collections, LibraryParams.Parse("tab=collections", LibraryTab.Recommended).Tab);
    }

    [Fact]
    public void Watched_is_the_webs_as_the_server_takes_it()
    {
        Assert.False(new LibraryParams(Watched: false).Query().Watched);
        Assert.True(new LibraryParams(Watched: true).Query().Watched);
        Assert.Null(new LibraryParams().Query().Watched);
    }

    [Fact]
    public void Each_kind_has_the_webs_sorts()
    {
        Assert.Equal(9, LibraryParams.SortsFor("movies").Count);
        Assert.DoesNotContain(LibrarySort.Duration, LibraryParams.SortsFor("shows"));
        Assert.Contains(LibrarySort.Bitrate, LibraryParams.SortsFor("shows"));
        Assert.Equal(new[] { LibrarySort.Title, LibrarySort.Added, LibrarySort.UserRating, LibrarySort.LastPlayed }, LibraryParams.SortsFor("music"));
        Assert.Equal(new[] { LibraryTab.Recommended, LibraryTab.Library }, LibraryParams.TabsFor("music"));
        Assert.Equal(new[] { LibraryTab.Recommended, LibraryTab.Library, LibraryTab.Collections }, LibraryParams.TabsFor(null));
    }

    [Fact]
    public void Music_keeps_no_filters_or_collections()
    {
        var p = new LibraryParams(LibraryTab.Collections, LibrarySort.Bitrate, SortOrder.Ascending, false, "Jazz");
        Assert.Equal(new LibraryParams(LibraryTab.Library), p.ValidFor("music"));
        // Shows can't sort by duration: back to titles, in their order.
        Assert.Equal(new LibraryParams(), new LibraryParams(Sort: LibrarySort.Duration, Order: SortOrder.Descending).ValidFor("shows"));
        // What a kind offers stays as it is.
        var movies = new LibraryParams(Sort: LibrarySort.Duration, Order: SortOrder.Ascending, Year: 2001);
        Assert.Equal(movies, movies.ValidFor("movies"));
    }

    [Fact]
    public void Clearing_keeps_the_sort()
    {
        var p = new LibraryParams(Sort: LibrarySort.Rating, Watched: true, Genre: "Drama", Year: 2020);
        Assert.True(p.Filtered);
        Assert.Equal(new LibraryParams(Sort: LibrarySort.Rating), p.Cleared());
        Assert.False(p.Cleared().Filtered);
    }

    [Fact]
    public void Values_are_escaped_as_the_android_app_escapes_them()
    {
        Assert.Equal("Sci-Fi+%26+Fantasy%21", LibraryParams.Encode("Sci-Fi & Fantasy!"));
        Assert.Equal("Am%C3%A9lie", LibraryParams.Encode("Amélie"));
        Assert.Equal("Sci-Fi & Fantasy!", LibraryParams.Decode("Sci-Fi+%26+Fantasy%21"));
        Assert.Equal("Amélie", LibraryParams.Decode("Am%C3%A9lie"));
        // One that can't be read is taken as it is.
        Assert.Equal("100%", LibraryParams.Decode("100%"));
        Assert.Equal("%zz", LibraryParams.Decode("%zz"));
        Assert.Equal("Rock & Roll", LibraryParams.Parse("genre=Rock+%26+Roll").Genre);
    }

    [Fact]
    public void A_library_key_carries_its_tab_and_view()
    {
        Assert.Equal(new PageKey.Library(3), new PageKey.Library(3, null, null));
        Assert.NotEqual(new PageKey.Library(3), new PageKey.Library(3, LibraryTab.Library, "sort=added&order=desc"));
    }
}
