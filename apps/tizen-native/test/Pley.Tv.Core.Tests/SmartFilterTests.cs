using Pley.Tv.Core.Api;
using Pley.Tv.Core.Ui;
using Xunit;

namespace Pley.Tv.Core.Tests;

/// <summary>apps/web/src/lib/smartFilter.test.ts's cases, as the Android app's SmartFilterTest has them.</summary>
public class SmartFilterTests
{
    [Fact]
    public void The_library_pages_filters_in_plexs_syntax()
    {
        Assert.Equal("genre=Science+Fiction&unwatched=1&sort=year%3Adesc",
            SmartFilter.Of(new LibraryParams(Sort: LibrarySort.Year, Order: SortOrder.Descending, Watched: false, Genre: "Science Fiction")));
        Assert.Equal("year=1999&unwatched=0&sort=titleSort",
            SmartFilter.Of(new LibraryParams(Sort: LibrarySort.Title, Order: SortOrder.Ascending, Watched: true, Year: 1999)));
        // Plex's resolution and Pley's dynamic range, after the year and before unwatched.
        Assert.Equal("genre=Drama&year=1999&resolution=4k&dynamicRange=hdr&unwatched=1&sort=titleSort",
            SmartFilter.Of(new LibraryParams(Watched: false, Genre: "Drama", Year: 1999, Resolution: "4k", DynamicRange: "hdr")));
        Assert.Equal("dynamicRange=dolby_vision&sort=mediaBitrate%3Adesc",
            SmartFilter.Of(new LibraryParams(Sort: LibrarySort.Bitrate, DynamicRange: "dolby_vision")));
    }

    [Fact]
    public void Filters_in_words()
    {
        Assert.Equal(new[] { "Genre: Comedy", "Unwatched", "Newest first" }, SmartFilter.Describe("type=1&genre=Comedy&unwatched=1&sort=year%3Adesc"));
        // As python-plexapi writes them.
        Assert.Equal(new[] { "After 2019", "Recently added first", "At most 20" }, SmartFilter.Describe("year%3E%3E=2019&sort=movie.addedAt:desc&includeGuids=1&limit=20"));
        // A resolution and a dynamic range by their words, never the tokens; one Pley doesn't save, as it is.
        Assert.Equal(new[] { "4K", "Dolby Vision", "By title" }, SmartFilter.Describe("resolution=4k&dynamicRange=dolby_vision&sort=titleSort"));
        Assert.Equal(new[] { "1080p", "HDR10+" }, SmartFilter.Describe("resolution=1080&dynamicRange=hdr10plus"));
        Assert.Equal(new[] { "resolution: 480" }, SmartFilter.Describe("resolution=480"));
    }

    [Fact]
    public void What_it_saves_reads_back_as_the_same_words()
    {
        var filter = SmartFilter.Of(new LibraryParams(Sort: LibrarySort.Added, Watched: false, Genre: "Drama", Year: 2001, Resolution: "720", DynamicRange: "sdr"));
        Assert.Equal(new[] { "Genre: Drama", "From 2001", "720p", "SDR", "Unwatched", "Recently added first" }, SmartFilter.Describe(filter));
    }
}
