package app.pley.android.screens.library

import app.pley.android.model.DynamicRangeFilter
import app.pley.android.model.LibrarySort
import app.pley.android.model.ResolutionFilter
import app.pley.android.model.SortOrder
import org.junit.Assert.assertEquals
import org.junit.Test

/** apps/web/src/lib/smartFilter.test.ts's cases. */
class SmartFilterTest {
    @Test
    fun theLibraryPagesFiltersInPlexsSyntax() {
        assertEquals(
            "genre=Science+Fiction&unwatched=1&sort=year%3Adesc",
            SmartFilter.of(LibraryParams(sort = LibrarySort.Year, order = SortOrder.Descending, watched = false, genre = "Science Fiction")),
        )
        assertEquals(
            "year=1999&unwatched=0&sort=titleSort",
            SmartFilter.of(LibraryParams(sort = LibrarySort.Title, order = SortOrder.Ascending, watched = true, year = 1999)),
        )
    }

    @Test
    fun resolutionAndDynamicRangeComeAfterTheYearAndBeforeUnwatched() {
        assertEquals(
            "genre=Drama&year=1999&resolution=4k&dynamicRange=hdr&unwatched=1&sort=mediaBitrate%3Adesc",
            SmartFilter.of(
                LibraryParams(
                    sort = LibrarySort.Bitrate,
                    watched = false,
                    genre = "Drama",
                    year = 1999,
                    resolution = ResolutionFilter.Uhd,
                    dynamicRange = DynamicRangeFilter.Hdr,
                ),
            ),
        )
        assertEquals(
            "dynamicRange=dolby_vision&sort=titleSort",
            SmartFilter.of(LibraryParams(dynamicRange = DynamicRangeFilter.DolbyVision)),
        )
    }

    @Test
    fun filtersInWords() {
        assertEquals(
            listOf("Genre: Comedy", "Unwatched", "Newest first"),
            SmartFilter.describe("type=1&genre=Comedy&unwatched=1&sort=year%3Adesc"),
        )
        // As python-plexapi writes them.
        assertEquals(
            listOf("After 2019", "Recently added first", "At most 20"),
            SmartFilter.describe("year%3E%3E=2019&sort=movie.addedAt:desc&includeGuids=1&limit=20"),
        )
        // A field it has no words for reads as it is.
        assertEquals(listOf("country: France"), SmartFilter.describe("country=France"))
    }

    @Test
    fun resolutionAndDynamicRangeReadAsTheFiltersNameThem() {
        assertEquals(listOf("4K"), SmartFilter.describe("resolution=4k"))
        assertEquals(listOf("1080p", "Dolby Vision"), SmartFilter.describe("resolution=1080&dynamicRange=dolby_vision"))
        assertEquals(listOf("HDR", "HDR10+"), SmartFilter.describe("dynamicRange=hdr&dynamicRange=hdr10plus"))
        // A value Pley's filters don't name (Plex's own): as it is.
        assertEquals(listOf("resolution: 576"), SmartFilter.describe("resolution=576"))
    }

    @Test
    fun whatItSavesReadsBackAsTheSameWords() {
        val filter = SmartFilter.of(LibraryParams(sort = LibrarySort.Added, watched = false, genre = "Drama", year = 2001))
        assertEquals(listOf("Genre: Drama", "From 2001", "Unwatched", "Recently added first"), SmartFilter.describe(filter))
        val quality = SmartFilter.of(LibraryParams(resolution = ResolutionFilter.Sd, dynamicRange = DynamicRangeFilter.Sdr))
        assertEquals(listOf("SD", "SDR", "By title"), SmartFilter.describe(quality))
    }
}
