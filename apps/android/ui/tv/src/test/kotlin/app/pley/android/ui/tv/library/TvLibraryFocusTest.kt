package app.pley.android.ui.tv.library

import androidx.activity.ComponentActivity
import app.pley.android.model.CollectionSummary
import app.pley.android.model.CollectionCategory
import app.pley.android.model.CategoryRef
import androidx.compose.ui.test.requestFocus
import androidx.compose.ui.test.performSemanticsAction
import androidx.compose.ui.test.onFirst
import androidx.compose.ui.test.hasClickAction
import androidx.compose.ui.semantics.SemanticsActions
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.hasAnyAncestor
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.isFocused
import androidx.compose.ui.test.isNotSelected
import androidx.compose.ui.test.isSelected
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.navigation3.runtime.NavKey
import androidx.test.core.app.ApplicationProvider
import app.pley.android.common.TestTags
import app.pley.android.data.LibraryItems
import app.pley.android.data.Loadable
import app.pley.android.designsystem.tile.TileKind
import app.pley.android.model.ItemDetail
import app.pley.android.model.ItemKind
import app.pley.android.model.ItemSummary
import app.pley.android.model.Library
import app.pley.android.model.LibraryFilters
import app.pley.android.model.LibraryKind
import app.pley.android.network.ApiException
import app.pley.android.screens.LibraryTab
import app.pley.android.screens.PleyKey
import app.pley.android.testing.screens.onboarding.FakeOnboardingSource
import app.pley.android.ui.tv.item.TV_TILE_ACTIONS
import app.pley.android.ui.tv.kit.press
import app.pley.android.ui.tv.onboarding.LocalOnboardingSource
import app.pley.android.ui.tv.shell.LocalTvNavigator
import app.pley.android.ui.tv.shell.RailProfile
import app.pley.android.ui.tv.shell.RailState
import app.pley.android.ui.tv.shell.StartFacts
import app.pley.android.ui.tv.shell.TvApp
import app.pley.android.ui.tv.shell.TvNavigator
import app.pley.android.ui.tv.shell.TvShellSource
import app.pley.android.ui.tv.shell.TvTestApplication
import app.pley.android.ui.tv.shell.TvTestGraph
import app.pley.android.ui.tv.theme.TvTheme
import java.util.concurrent.TimeUnit
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.shadows.ShadowLooper

/**
 * A library's grid in the app's frame (D-pad rule 2): Back from a tile's
 * actions, and from the title a tile opened, lands on that tile, the grid
 * scrolled as it was; so does Right from the rail.
 */
@RunWith(RobolectricTestRunner::class)
@Config(qualifiers = "w960dp-h540dp-land-television-xhdpi", application = TvTestApplication::class)
class TvLibraryFocusTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    private val movies = Library(1, "Movies", LibraryKind.Movies, itemCount = TOTAL)

    private class Source(library: Library) : TvShellSource {
        override suspend fun startFacts() = StartFacts(server = "http://192.168.1.20:7539", signedIn = true)

        override val libraries: Flow<List<Library>> = MutableStateFlow(listOf(library))

        override val profile: Flow<RailProfile?> = MutableStateFlow(RailProfile("Anna"))

        override suspend fun keepUpdated() = Unit
    }

    private val graph get() = ApplicationProvider.getApplicationContext<TvTestApplication>().screensGraph as TvTestGraph

    private val items = (1..TOTAL).map { n ->
        ItemSummary(id = n.toLong(), libraryId = 1, kind = ItemKind.Movie, title = title(n), sortTitle = title(n))
    }

    private val columns = tvGridColumns(TileKind.LibraryGrid)

    /** Movies' grid, in the frame, with focus on its first tile. */
    private fun library() {
        val sources = graph.sources
        // Left on its Library tab, the grid these are about: a library never opened shows Recommended.
        graph.left[1] = "tab=library"
        sources.libraryList.value = Loadable.Ready(listOf(movies), fresh = true)
        graph.listing.value = Loadable.Ready(LibraryItems(total = items.size, items = items), fresh = true)
        // The tiles' actions are worked out from the items' details.
        for (item in items) {
            sources.detail(item.id).value = Loadable.Ready(
                ItemDetail(id = item.id, libraryId = 1, kind = ItemKind.Movie, title = item.title, durationMs = 6_000_000),
                fresh = true,
            )
        }
        val onboarding = FakeOnboardingSource().apply { active.value = "http://s" }
        compose.setContent {
            CompositionLocalProvider(LocalOnboardingSource provides onboarding) {
                TvApp(Source(movies), onExit = {}, reducedMotion = true)
            }
        }
        waitUntil { exists(isFocused()) }
        compose.press(Key.DirectionLeft)
        while (!exists(hasTestTag("${TestTags.TV_RAIL}.library:1") and isFocused())) compose.press(Key.DirectionDown)
        compose.press(Key.DirectionCenter)
        waitUntil { focusedTitle() == title(1) }
    }

    private fun waitUntil(condition: () -> Boolean) {
        compose.waitUntil(WITHIN_MS, condition)
        compose.waitForIdle()
    }

    private fun exists(matcher: SemanticsMatcher): Boolean = compose.onAllNodes(matcher).fetchSemanticsNodes().isNotEmpty()

    private fun focusedTitle(): String = compose.onAllNodes(isFocused()).fetchSemanticsNodes().firstOrNull()
        ?.config?.getOrElse(SemanticsProperties.ContentDescription) { emptyList() }?.joinToString().orEmpty()
        .substringBefore(',')

    private fun back() {
        compose.runOnUiThread { compose.activity.onBackPressedDispatcher.onBackPressed() }
        compose.waitForIdle()
    }

    /** Focus [rows] rows down and [right] tiles right of the first: the tile's number. */
    private fun moveTo(rows: Int, right: Int): Int {
        compose.press(Key.DirectionDown, rows)
        compose.press(Key.DirectionRight, right)
        val n = rows * columns + right + 1
        assertEquals(title(n), focusedTitle())
        return n
    }

    private fun backFromActionsLandsOnTheTile(rows: Int) {
        library()
        val n = moveTo(rows, 2)
        compose.press(Key.Menu)
        waitUntil { exists(hasAnyAncestor(hasTestTag(TV_TILE_ACTIONS)) and isFocused()) }
        back()
        waitUntil { !exists(hasTestTag(TV_TILE_ACTIONS)) }
        assertEquals(title(n), focusedTitle())
    }

    @Test
    fun backFromATilesActionsLandsOnTheTile() = backFromActionsLandsOnTheTile(rows = 1)

    @Test
    fun backFromATilesActionsLandsOnTheTileWithTheGridScrolled() = backFromActionsLandsOnTheTile(rows = 3)

    @Test
    fun backFromATitleLandsOnItsTileWithTheGridAsItWas() {
        library()
        val n = moveTo(rows = 3, right = 2)
        val top = compose.onAllNodes(isFocused()).fetchSemanticsNodes().single().boundsInRoot.top
        compose.press(Key.DirectionCenter)
        waitUntil { !exists(hasTestTag(TestTags.LIBRARY_GRID)) }
        // Long enough for the page's data to stop while it is covered.
        ShadowLooper.idleMainLooper(10, TimeUnit.SECONDS)
        compose.mainClock.advanceTimeBy(10_000)
        back()
        waitUntil { exists(hasTestTag(TestTags.LIBRARY_GRID)) && exists(isFocused()) }
        assertEquals(title(n), focusedTitle())
        val again = compose.onAllNodes(isFocused()).fetchSemanticsNodes().single().boundsInRoot.top
        assertEquals(top, again, 1f)
    }

    private fun backFromATitleInTheFirstRow(beforehand: () -> Unit) {
        library()
        beforehand()
        val n = moveTo(rows = 0, right = 1)
        compose.press(Key.DirectionCenter)
        waitUntil { !exists(hasTestTag(TestTags.LIBRARY_GRID)) }
        back()
        waitUntil { exists(hasTestTag(TestTags.LIBRARY_GRID)) && exists(isFocused()) }
        assertEquals(title(n), focusedTitle())
    }

    @Test
    fun backFromATitleInTheFirstRowLandsOnItsTile() = backFromATitleInTheFirstRow {}

    @Test
    fun backFromATitleInTheFirstRowLandsOnItsTileAfterTheChips() = backFromATitleInTheFirstRow {
        // Up to the chips and down again, as a filter is chosen.
        compose.press(Key.DirectionUp)
        compose.press(Key.DirectionDown)
        assertEquals(title(1), focusedTitle())
    }

    @Test
    fun rightFromTheRailLandsOnAFirstRowTileFocusLeft() {
        library()
        val n = moveTo(rows = 0, right = 1)
        back()
        waitUntil { exists(hasTestTag("${TestTags.TV_RAIL}.library:1") and isFocused()) }
        compose.press(Key.DirectionRight)
        assertEquals(title(n), focusedTitle())
    }

    @Test
    fun rightFromTheRailLandsOnTheTileFocusLeft() {
        library()
        val n = moveTo(rows = 3, right = 2)
        // Back on the rail's page goes to the rail.
        back()
        waitUntil { exists(hasTestTag("${TestTags.TV_RAIL}.library:1") and isFocused()) }
        compose.press(Key.DirectionRight)
        assertEquals(title(n), focusedTitle())
    }

    /** The focused element's words: a chip's label. */
    private fun focusedText(): String = compose.onAllNodes(isFocused()).fetchSemanticsNodes().firstOrNull()
        ?.config?.getOrElse(SemanticsProperties.Text) { emptyList() }?.joinToString().orEmpty()

    private fun focusedBounds() = compose.onAllNodes(isFocused()).fetchSemanticsNodes().single().boundsInRoot

    @Test
    fun aLibraryNeverOpenedShowsRecommended() {
        graph.sources.libraryList.value = Loadable.Ready(listOf(movies), fresh = true)
        graph.listing.value = Loadable.Ready(LibraryItems(total = items.size, items = items), fresh = true)
        val key = PleyKey.Library(1)
        val navigator = TvNavigator(mutableListOf<NavKey>(key), RailState()) {}
        compose.setContent {
            TvTheme(reducedMotion = true) {
                CompositionLocalProvider(LocalTvNavigator provides navigator) { TvLibraryScreen(key) }
            }
        }
        // Its rows, not the whole grid; left on the Library tab, it opens there ([library]).
        waitUntil { exists(hasText("Recommended") and isSelected()) }
        assertTrue(exists(hasText("Library") and isNotSelected()))
    }

    @Test
    fun chipsThatDontAllFitGoToTheirEndAndBackWithFocus() {
        graph.sources.libraryList.value = Loadable.Ready(listOf(movies), fresh = true)
        graph.listing.value = Loadable.Ready(LibraryItems(total = items.size, items = items), fresh = true)
        graph.filters.value = Loadable.Ready(
            LibraryFilters(listOf("Science Fiction & Fantasy"), listOf(2001), listOf("4k", "1080"), listOf("hdr", "dolby_vision", "sdr")),
            fresh = true,
        )
        // Every filter chosen, a long genre among them: the chips and the count are wider than the page.
        val view = "watched=false&genre=Science+Fiction+%26+Fantasy&year=2001&resolution=4k&dynamic_range=dolby_vision"
        val key = PleyKey.Library(1, LibraryTab.Library, view)
        val navigator = TvNavigator(mutableListOf<NavKey>(key), RailState()) {}
        compose.setContent {
            TvTheme(reducedMotion = true) {
                CompositionLocalProvider(LocalTvNavigator provides navigator) { TvLibraryScreen(key) }
            }
        }
        waitUntil { focusedTitle() == title(1) }
        val density = compose.activity.resources.displayMetrics.density
        val gutter = 96f * density
        val safeEdge = (960f - 48f) * density
        // A chip's focus growth reaches a few dp past its place.
        val growth = 4f * density
        compose.press(Key.DirectionUp)
        assertEquals("Title", focusedText())
        assertTrue(focusedBounds().left >= gutter - growth)
        var guard = 0
        while (focusedText() != "Dolby Vision" && guard++ < 8) compose.press(Key.DirectionRight)
        assertEquals("Dolby Vision", focusedText())
        assertTrue("${focusedBounds()}", focusedBounds().right <= safeEdge + growth)
        // At its end, the count shows too, short of the safe edge.
        val count = compose.onNodeWithText("of 120 movies", substring = true).fetchSemanticsNode().boundsInRoot
        assertTrue("$count", count.right <= safeEdge + 1f)
        compose.press(Key.DirectionLeft, 6)
        assertEquals("Title", focusedText())
        assertTrue("${focusedBounds()}", focusedBounds().left >= gutter - growth)
    }

    @Test
    fun clearingAFilterThatTakesTheChipsAwayLeavesFocusOnTheCollections() {
        val highlights = CategoryRef(1, "highlights", "Highlights")
        val franchises = CategoryRef(2, "franchises", "Franchises")
        graph.sources.libraryList.value = Loadable.Ready(listOf(movies), fresh = true)
        graph.categoryList.value = Loadable.Ready(
            listOf(CollectionCategory(1, "highlights", "Highlights"), CollectionCategory(2, "franchises", "Franchises")),
            fresh = true,
        )
        val alien = CollectionSummary(8, 1, "Alien Collection", automatic = true, category = franchises, itemCount = 4)
        graph.collectionList.value = Loadable.Ready(listOf(CollectionSummary(7, 1, "Family Night", category = highlights, itemCount = 5), alien), fresh = true)
        // In the app's frame, the rail and all, as on a TV: left on its Collections tab.
        graph.left[1] = "tab=collections"
        val onboarding = FakeOnboardingSource().apply { active.value = "http://s" }
        compose.setContent {
            CompositionLocalProvider(LocalOnboardingSource provides onboarding) {
                TvApp(Source(movies), onExit = {}, reducedMotion = true)
            }
        }
        waitUntil { exists(isFocused()) }
        compose.press(Key.DirectionLeft)
        while (!exists(hasTestTag("${TestTags.TV_RAIL}.library:1") and isFocused())) compose.press(Key.DirectionDown)
        compose.press(Key.DirectionCenter)
        waitUntil { exists(hasText("Category")) }
        compose.onNodeWithText("Category").performSemanticsAction(SemanticsActions.OnClick)
        waitUntil { exists(hasText("All categories")) }
        compose.onAllNodes(hasText("Franchises") and hasClickAction()).onFirst().performSemanticsAction(SemanticsActions.OnClick)
        waitUntil { exists(hasText("Clear filters")) }
        // The server's collections change: Franchises is the only category left with any.
        graph.collectionList.value = Loadable.Ready(listOf(alien), fresh = true)
        compose.waitForIdle()
        compose.onNodeWithText("Clear filters").requestFocus()
        compose.press(Key.DirectionCenter)
        // Every collection again, of one category: no chips. Focus went to the Category chip, and leaves
        // with it for the collections below, not nowhere.
        waitUntil { !exists(hasText("Clear filters")) && exists(isFocused()) }
        assertTrue(!exists(hasText("Category")))
        assertEquals("Alien Collection", focusedTitle())
    }

    @Test
    fun collectionsByCategoryKeepFocusAsTheyArriveAndUpReachesTheChip() {
        library()
        val highlights = CategoryRef(1, "highlights", "Highlights")
        val franchises = CategoryRef(2, "franchises", "Franchises")
        // The categories are slow to come: the tab shows its skeleton meanwhile.
        graph.categoryList.value = Loadable.Loading
        graph.collectionList.value = Loadable.Ready(
            listOf(
                CollectionSummary(7, 1, "Family Night", category = highlights, itemCount = 5),
                CollectionSummary(8, 1, "Alien Collection", automatic = true, category = franchises, itemCount = 4),
            ),
            fresh = true,
        )
        // Up to the chips, up to the tabs, right to Collections.
        compose.press(Key.DirectionUp)
        compose.press(Key.DirectionUp)
        compose.press(Key.DirectionRight)
        compose.press(Key.DirectionCenter)
        waitUntil { !exists(hasText(title(1))) }
        // Down into the skeleton while it loads.
        compose.press(Key.DirectionDown)
        waitUntil { exists(hasAnyAncestor(hasTestTag(TestTags.LIBRARY_GRID)) and isFocused()) }
        graph.categoryList.value = Loadable.Ready(
            listOf(CollectionCategory(1, "highlights", "Highlights"), CollectionCategory(3, "other", "Other"), CollectionCategory(2, "franchises", "Franchises")),
            fresh = true,
        )
        // A row per category takes the grid's place: focus stays on the tab, on the Category chip above them,
        // where Down from the tabs lands, and goes on down to the first collection.
        waitUntil { exists(hasText("Franchises")) && exists(isFocused()) }
        assertTrue(exists(hasText("Category") and isFocused()))
        compose.press(Key.DirectionDown)
        assertEquals("Family Night", focusedTitle())
        // And Up from the first row comes back to the chip, not past it to the tabs.
        compose.press(Key.DirectionUp)
        assertTrue(exists(hasText("Category") and isFocused()))
        // So does Up from the grid of one category.
        compose.press(Key.DirectionCenter)
        waitUntil { exists(hasText("All categories")) }
        compose.onAllNodes(hasText("Franchises") and hasClickAction()).onFirst().performSemanticsAction(SemanticsActions.OnClick)
        waitUntil { exists(hasText("Clear filters")) && exists(hasText("Franchises") and isFocused()) }
        compose.press(Key.DirectionDown)
        assertEquals("Alien Collection", focusedTitle())
        compose.press(Key.DirectionUp)
        assertTrue(exists(hasText("Franchises") and isFocused()))
    }

    @Test
    fun aServerWithoutCategoriesShowsTheCollectionsInAGrid() {
        // A server from before categories answers GET /api/collection-categories 404.
        graph.categoryList.value = Loadable.Failed(ApiException(404, "not_found", "no such endpoint"))
        graph.collectionList.value = Loadable.Ready(
            listOf(CollectionSummary(7, 1, "Family Night", itemCount = 5), CollectionSummary(8, 1, "Alien Collection", automatic = true, itemCount = 4)),
            fresh = true,
        )
        graph.sources.libraryList.value = Loadable.Ready(listOf(movies), fresh = true)
        graph.left[1] = "tab=collections"
        val onboarding = FakeOnboardingSource().apply { active.value = "http://s" }
        compose.setContent {
            CompositionLocalProvider(LocalOnboardingSource provides onboarding) {
                TvApp(Source(movies), onExit = {}, reducedMotion = true)
            }
        }
        waitUntil { exists(isFocused()) }
        compose.press(Key.DirectionLeft)
        while (!exists(hasTestTag("${TestTags.TV_RAIL}.library:1") and isFocused())) compose.press(Key.DirectionDown)
        compose.press(Key.DirectionCenter)
        // The tab opens as it did before categories: every collection in the grid, the first focused, no Category chip.
        waitUntil { focusedTitle() == "Family Night" }
        assertTrue(exists(hasAnyAncestor(hasTestTag(TestTags.LIBRARY_GRID)) and isFocused()))
        compose.press(Key.DirectionRight)
        assertEquals("Alien Collection", focusedTitle())
        assertTrue(!exists(hasText("Category")))
    }

    private companion object {
        const val WITHIN_MS = 5_000L
        const val TOTAL = 120

        fun title(n: Int) = "Movie %03d".format(n)
    }
}
