package app.pley.android.screens.home

import app.pley.android.data.Loadable
import app.pley.android.model.Hub
import app.pley.android.model.Hubs
import app.pley.android.model.ItemDetail
import app.pley.android.model.ItemKind
import app.pley.android.model.ItemSummary
import app.pley.android.model.UserState
import app.pley.android.screens.LibraryTab
import app.pley.android.screens.PleyKey
import java.io.IOException
import java.util.Locale
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test

class HomeStateTest {
    private val libraries = Loadable.Ready(listOf(MOVIES, SHOWS, MUSIC), fresh = true)

    @Test
    fun rowsAreShapedAsTheWebShapesThem() {
        val rows = homeRows(HOME.hubs, libraries.value)
        assertEquals(listOf(RowShape.Landscape, RowShape.Poster, RowShape.Poster, RowShape.Square), rows.map { it.shape })
        assertEquals(listOf("Continue Watching", "Recently Added in Movies", "Recently Added in Shows", "Recently Added in Music"), rows.map { it.title })
        assertTrue(rows[0].continueWatching)
        // A row of episodes is stills, wherever it is; albums are squares even when the library isn't known.
        val episodes = Hub("recently_added:2", "Recently Added in Shows", 2, listOf(GLASSHOUSE))
        assertEquals(RowShape.Landscape, rowShape(episodes))
        assertEquals(RowShape.Square, rowShape(ADDED_MUSIC))
    }

    @Test
    fun emptyRowsAreLeftOut() {
        val rows = homeRows(listOf(Hub("continue_watching", "Continue Watching"), ADDED_MOVIES), null)
        assertEquals(listOf("recently_added:1"), rows.map { it.key })
    }

    @Test
    fun seeAllOpensTheLibraryForItsListedRowsOnly() {
        assertNull(seeAllOf(CONTINUE))
        assertEquals(PleyKey.Library(1, LibraryTab.Library, "sort=added&order=desc"), seeAllOf(ADDED_MOVIES))
        assertEquals(PleyKey.Library(2, LibraryTab.Library, "sort=rating&order=desc"), seeAllOf(Hub("top_rated:2", "Top Rated", 2, listOf(OVERGROWTH))))
        assertNull(seeAllOf(Hub("continue_watching:2", "Continue Watching", 2, listOf(GLASSHOUSE))))
    }

    @Test
    fun nothingSavedAndNothingYetIsLoading() {
        assertSame(HomeState.Loading, homeState(Loadable.Loading, Loadable.Loading, scanning = false, origin = ""))
        assertSame(HomeState.Loading, homeState(Loadable.Loading, libraries, scanning = false, origin = ""))
    }

    @Test
    fun anErrorShowsOnlyWithoutASnapshot() {
        val error = IOException("nothing answers there")
        val failed = homeState(Loadable.Failed(error), libraries, false, "")
        assertEquals(HomeState.Failed(error), failed)
        assertEquals(Loadable.Failed(error, Unit), homeStatus(Loadable.Failed(error), failed))
        val hubs = Loadable.Failed(error, HOME)
        val stale = homeState(hubs, libraries, false, FakeHomeSources.ORIGIN)
        stale as HomeState.Ready
        assertEquals(4, stale.rows.size)
        // What was saved shows, and is said to be what was saved.
        assertEquals(Loadable.Failed(error, Unit), homeStatus(hubs, stale))
    }

    @Test
    fun theSnapshotIsReadyButNotFresh() {
        val saved = Loadable.Ready(HOME, fresh = false)
        val state = homeState(saved, Loadable.Loading, false, FakeHomeSources.ORIGIN) as HomeState.Ready
        assertEquals(Loadable.Ready(Unit, fresh = false), homeStatus(saved, state))
        assertEquals(FakeHomeSources.ORIGIN, state.origin)
        val answer = Loadable.Ready(HOME, fresh = true)
        val fresh = homeState(answer, libraries, false, FakeHomeSources.ORIGIN)
        assertEquals(Loadable.Ready(Unit, fresh = true), homeStatus(answer, fresh))
        // The server saying what was saved is the same Home: only the status says it is fresh.
        assertEquals(homeState(saved, libraries, false, FakeHomeSources.ORIGIN), fresh)
    }

    @Test
    fun nothingToShowHasNothingToTell() {
        assertNull(homeStatus(Loadable.Loading, HomeState.Loading))
        val none = Loadable.Ready(emptyList<app.pley.android.model.Library>(), fresh = true)
        val empty = Loadable.Ready(Hubs(), fresh = true)
        assertNull(homeStatus(empty, homeState(empty, none, false, "")))
        assertNull(homeStatus(empty, homeState(empty, libraries, false, "")))
    }

    @Test
    fun noLibrariesAndEmptyLibrariesSayDifferentThings() {
        val none = Loadable.Ready(emptyList<app.pley.android.model.Library>(), fresh = true)
        assertSame(HomeState.NoLibraries, homeState(Loadable.Loading, none, false, ""))
        assertSame(HomeState.NoLibraries, homeState(Loadable.Ready(Hubs(), true), none, false, ""))
        assertEquals(HomeState.Empty(scanning = false), homeState(Loadable.Ready(Hubs(), true), libraries, false, ""))
        assertEquals(HomeState.Empty(scanning = true), homeState(Loadable.Ready(Hubs(), true), libraries, true, ""))
        val scanningOne = Loadable.Ready(listOf(MOVIES.copy(scanning = true)), true)
        assertEquals(HomeState.Empty(scanning = true), homeState(Loadable.Ready(Hubs(), true), scanningOne, false, ""))
    }

    @Test
    fun theHeroIsTheFirstInProgressElseTheNewestVideo() {
        assertEquals(GLASSHOUSE, heroOf(homeRows(HOME.hubs, null)))
        val noContinue = homeRows(listOf(ADDED_MOVIES, ADDED_SHOWS, ADDED_MUSIC), null)
        // The album is newer, but a video comes first.
        assertEquals(OVERGROWTH, heroOf(noContinue))
        assertEquals(ALBUM, heroOf(homeRows(listOf(ADDED_MUSIC), null)))
        assertNull(heroOf(emptyList()))
    }

    @Test
    fun anEpisodeIsTitledByItsShowWithItsNumberAndTimeLeft() {
        val info = homeInfo(GLASSHOUSE, null, Locale.US)
        assertEquals("The Lantern House", info.title)
        assertEquals("S1 · E3 · Glasshouse", info.context)
        assertEquals("26 min left", info.timeLeft)
        assertEquals("58 min", info.meta.runtime)
        assertEquals("2024", info.meta.year)
        assertTrue(info.inProgress)
        assertTrue(info.video)
        assertFalse(info.complete)
        assertEquals("The lamp goes out for the first time in a hundred years.", info.summary)
        assertEquals(32f / 58f, info.progress, 0.001f)
    }

    @Test
    fun aMovieGetsItsSummaryRatingAndBadgesWithItsDetails() {
        val before = homeInfo(ASHFALL, null, Locale.US)
        assertEquals("Ashfall", before.title)
        assertEquals("", before.context)
        assertEquals("1h 12m left", before.timeLeft)
        assertEquals("2h 2m", before.meta.runtime)
        assertEquals("7.4", before.meta.rating)
        assertNull(before.meta.contentRating)
        assertEquals("", before.summary)

        val detail = ItemDetail(
            id = ASHFALL.id, libraryId = 1, kind = ItemKind.Movie, title = "Ashfall",
            summary = "A volcanologist and her estranged brother race the ash cloud.", contentRating = "R",
        )
        val after = homeInfo(ASHFALL, detail, Locale.US)
        assertEquals("R", after.meta.contentRating)
        assertEquals("A volcanologist and her estranged brother race the ash cloud.", after.summary)
        assertTrue(after.complete)
        // Another item's details are never mixed in.
        assertFalse(homeInfo(SALTWIND, detail, Locale.US).complete)
    }

    @Test
    fun aShowSaysItsSeasonsAndAWatchedMovieNoProgress() {
        val show = homeInfo(OVERGROWTH, null, Locale.US)
        assertEquals("2 seasons", show.meta.runtime)
        assertFalse(show.video)
        val watched = homeInfo(LOW_ORBIT.copy(durationMs = 90 * 60_000L, userState = UserState(positionMs = 0, watched = true)), null, Locale.US)
        assertFalse(watched.inProgress)
        assertEquals("", watched.timeLeft)
        assertTrue(watched.watched)
    }

    @Test
    fun librariesCountWhatTheyHoldByKind() {
        assertEquals("412 movies", libraryCount(MOVIES, Locale.US))
        assertEquals("1 show", libraryCount(SHOWS.copy(itemCount = 1), Locale.US))
        assertEquals("1,200 artists", libraryCount(MUSIC.copy(itemCount = 1200), Locale.US))
    }

    @Test
    fun itemsOpenTheirOwnPages() {
        assertEquals(PleyKey.Item(GLASSHOUSE.id), pageOf(GLASSHOUSE))
        assertEquals(PleyKey.Album(ALBUM.id), pageOf(ALBUM))
        assertEquals(PleyKey.Artist(7), pageOf(ItemSummary(7, 3, ItemKind.Artist, "Harbour Lights")))
        assertEquals(PleyKey.Album(301), pageOf(ItemSummary(9, 3, ItemKind.Track, "Swell", parentId = 301)))
    }

    @Test
    fun theLibrariesListMarksTheOnesBeingScanned() {
        val state = librariesState(libraries, listOf(app.pley.android.data.ScanState(2, 0.5f))) as LibrariesState.Ready
        assertEquals(listOf(false, true, false), state.libraries.map { it.scanning })
        assertEquals("38 shows", state.libraries[1].count.replace(' ', ' '))
        assertSame(LibrariesState.Loading, librariesState(Loadable.Loading, emptyList()))
        val error = IOException("no")
        assertEquals(LibrariesState.Failed(error), librariesState(Loadable.Failed(error), emptyList()))
    }
}
