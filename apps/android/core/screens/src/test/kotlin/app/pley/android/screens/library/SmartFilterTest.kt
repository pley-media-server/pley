package app.pley.android.screens.library

import app.pley.android.model.DynamicRangeFilter
import app.pley.android.model.LibrarySort
import app.pley.android.model.ResolutionFilter
import app.pley.android.model.SortOrder
import app.pley.android.screens.inEnglish
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

/** apps/web/src/lib/smartFilter.test.ts's cases. */
@RunWith(RobolectricTestRunner::class)
class SmartFilterTest {
    /** [filter]'s words in English, as a screen shows them. */
    private fun english(filter: String): List<String?> = SmartFilter.words(filter).map { it.inEnglish() }

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
            english("type=1&genre=Comedy&unwatched=1&sort=year%3Adesc"),
        )
        // As python-plexapi writes them.
        assertEquals(
            listOf("After 2019", "Recently added first", "At most 20"),
            english("year%3E%3E=2019&sort=movie.addedAt:desc&includeGuids=1&limit=20"),
        )
        // A field it has no words for reads as it is.
        assertEquals(listOf("country: France"), english("country=France"))
    }

    @Test
    fun resolutionAndDynamicRangeReadAsTheFiltersNameThem() {
        assertEquals(listOf("4K"), english("resolution=4k"))
        assertEquals(listOf("1080p", "Dolby Vision"), english("resolution=1080&dynamicRange=dolby_vision"))
        assertEquals(listOf("Any HDR", "HDR+"), english("dynamicRange=hdr&dynamicRange=hdr10plus"))
        // A value Pley's filters don't name (Plex's own): as it is.
        assertEquals(listOf("resolution: 576"), english("resolution=576"))
    }

    @Test
    fun whatItSavesReadsBackAsTheSameWords() {
        val filter = SmartFilter.of(LibraryParams(sort = LibrarySort.Added, watched = false, genre = "Drama", year = 2001))
        assertEquals(listOf("Genre: Drama", "From 2001", "Unwatched", "Recently added first"), english(filter))
        val quality = SmartFilter.of(LibraryParams(resolution = ResolutionFilter.Sd, dynamicRange = DynamicRangeFilter.Sdr))
        assertEquals(listOf("SD", "SDR", "By title"), english(quality))
    }
}
