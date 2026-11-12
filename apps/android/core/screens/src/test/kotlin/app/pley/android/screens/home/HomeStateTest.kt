package app.pley.android.screens.home

import app.pley.android.common.UiText
import app.pley.android.data.Loadable
import app.pley.android.model.Hub
import app.pley.android.model.Hubs
import app.pley.android.model.ItemDetail
import app.pley.android.model.ItemKind
import app.pley.android.model.ItemSummary
import app.pley.android.model.UserState
import app.pley.android.screens.LibraryTab
import app.pley.android.screens.PleyKey
import app.pley.android.screens.R
import app.pley.android.screens.home.libraryCountText
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
        assertEquals(UiText.plural(R.plurals.library_entry_movies, 412), libraryCountText(MOVIES))
        assertEquals(UiText.plural(R.plurals.library_entry_shows, 1), libraryCountText(SHOWS.copy(itemCount = 1)))
        assertEquals(UiText.plural(R.plurals.library_entry_artists, 1200), libraryCountText(MUSIC.copy(itemCount = 1200)))
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
        assertEquals(UiText.plural(R.plurals.library_entry_shows, 38), state.libraries[1].countText)
        assertSame(LibrariesState.Loading, librariesState(Loadable.Loading, emptyList()))
        val error = IOException("no")
        assertEquals(LibrariesState.Failed(error), librariesState(Loadable.Failed(error), emptyList()))
    }

    @Test
    fun theBannerFeaturesTheNewestOfEachLibraryInTurn() {
        val movies = Hub("recently_added:1", "Recently Added in Movies", 1, listOf(art(10), art(11), art(12)))
        val shows = Hub("recently_added:2", "Recently Added in Shows", 2, listOf(art(20, kind = ItemKind.Show, library = 2)))
        val rows = homeRows(listOf(Hub("continue_watching", "Continue Watching", null, listOf(art(1))), movies, shows), null)
        assertEquals(listOf(10L, 20L, 11L, 12L), featuredOf(rows).map { it.item.id })
        assertFalse(featuredOf(rows).any { it.fromList })
    }

    @Test
    fun theBannerMixesTopRatedTitlesAmongTheNewestTheSameOnesAllDay() {
        val movies = Hub("recently_added:1", "Recently Added in Movies", 1, (10L..15L).map { art(it) })
        val rows = homeRows(listOf(movies), null)
        val rated = (30L..39L).map { art(it) } + art(10) + art(40, watched = true) + art(41).copy(backdropUrl = null)
        val today = featuredOf(rows, rated, day = 20_000)
        // Six titles: the newest in order, a top rated one in the third and the sixth place.
        assertEquals(listOf(10L, 11L, 12L, 13L), today.filter { !it.topRated }.map { it.item.id })
        assertEquals(listOf(2, 5), today.indices.filter { today[it].topRated })
        // Not one that is new already, watched, or without a backdrop.
        assertTrue(today.filter { it.topRated }.all { it.item.id in 30L..39L })
        // The same all day, whatever order the libraries give them in, and whatever else is watched or added.
        assertEquals(today, featuredOf(rows, rated.reversed(), day = 20_000))
        val kept = today.filter { it.topRated }.map { it.item.id }
        val others = rated.filter { it.id !in kept }.drop(3)
        assertEquals(kept, featuredOf(rows, rated.filter { it.id in kept } + others, day = 20_000).filter { it.topRated }.map { it.item.id })
        // Another day, other ones (of ten, two: the days differ somewhere in a week).
        assertTrue((20_001L..20_007L).any { day -> featuredOf(rows, rated, day).filter { it.topRated }.map { it.item.id } != kept })
        // Without top rated titles, the newest alone; with little that is new, the top rated fill up.
        assertEquals(listOf(10L, 11L, 12L, 13L, 14L, 15L), featuredOf(rows, emptyList(), day = 20_000).map { it.item.id })
        val few = homeRows(listOf(Hub("recently_added:1", "Recently Added in Movies", 1, listOf(art(10)))), null)
        assertEquals(listOf(false, true, true, true, true, true), featuredOf(few, rated, day = 20_000).map { it.topRated })
        assertEquals(UiText.res(R.string.home_featured_top_rated_in, "Movies"), featuredLabel(today[2], libraries.value))
        assertEquals(UiText.res(R.string.home_featured_top_rated), featuredLabel(today[2], null))
    }

    @Test
    fun theBannerLeavesOutWhatWasWatchedHasNoBackdropOrIsNotAMovieOrShow() {
        val added = Hub(
            "recently_added:1", "Recently Added", 1,
            listOf(
                art(1, watched = true),
                art(2).copy(backdropUrl = null),
                art(3, kind = ItemKind.Album),
                art(4, kind = ItemKind.Episode),
                art(5),
            ),
        )
        assertEquals(listOf(5L), featuredOf(homeRows(listOf(added), null)).map { it.item.id })
        // Without backdrops, nothing: Home is its rows.
        assertTrue(featuredOf(homeRows(HOME.hubs, null)).isEmpty())
    }

    @Test
    fun theBannerFillsUpFromMyListOnceEachAndStopsAtItsCount() {
        val added = Hub("recently_added:1", "Recently Added", 1, listOf(art(1), art(2)))
        val list = Hub(app.pley.android.model.HubKeys.MY_LIST, "My List", null, listOf(art(2), art(3), art(4), art(5)))
        val picked = featuredOf(homeRows(listOf(list, added), null), count = 4)
        assertEquals(listOf(1L to false, 2L to false, 3L to true, 4L to true), picked.map { it.item.id to it.fromList })
    }

    @Test
    fun theBannerFeaturesWatchedTitlesWhenNothingElseIsLeft() {
        val movies = Hub("recently_added:1", "Recently Added", 1, listOf(art(1, watched = true), art(2, watched = true).copy(backdropUrl = null)))
        val shows = Hub("recently_added:2", "Recently Added", 2, listOf(art(3, kind = ItemKind.Show, library = 2, watched = true)))
        assertEquals(listOf(1L, 3L), featuredOf(homeRows(listOf(movies, shows), null)).map { it.item.id })
    }

    @Test
    fun theBannerSaysWhyATitleIsThere() {
        val new = FeaturedPick(art(1), fromList = false)
        assertEquals(UiText.res(R.string.home_featured_new_in, "Movies"), featuredLabel(new, libraries.value))
        assertEquals(UiText.res(R.string.home_featured_new), featuredLabel(new, null))
        assertEquals(UiText.res(R.string.home_featured_on_list), featuredLabel(FeaturedPick(art(1), fromList = true), libraries.value))
    }

    @Test
    fun theBannerGetsGenresAndMyListWithTheDetails() {
        val added = Hub("recently_added:1", "Recently Added in Movies", 1, listOf(art(10)))
        val state = homeState(Loadable.Ready(Hubs(listOf(added)), true), libraries, false, FakeHomeSources.ORIGIN) as HomeState.Ready
        val featured = state.featured.single()
        assertEquals(UiText.res(R.string.home_featured_new_in, "Movies"), featured.label)
        assertTrue(featured.info.listable)
        assertFalse(featured.info.complete)
        val detail = ItemDetail(id = 10, libraryId = 1, kind = ItemKind.Movie, title = "Title 10", genres = listOf("Drama", "Comedy"), inWatchlist = true)
        val told = state.withDetails(mapOf(10L to detail)).featured.single().info
        assertEquals(listOf("Drama", "Comedy"), told.genres)
        assertTrue(told.inList)
        assertTrue(told.complete)
    }

    private fun art(id: Long, kind: ItemKind = ItemKind.Movie, library: Long = 1, watched: Boolean = false) = ItemSummary(
        id = id,
        libraryId = library,
        kind = kind,
        title = "Title $id",
        backdropUrl = "/api/items/$id/images/backdrop",
        userState = UserState(watched = watched),
    )
}
