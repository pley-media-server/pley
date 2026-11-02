package app.pley.android.ui.phone.home

import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.remember
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.longClick
import androidx.compose.ui.test.onAllNodesWithContentDescription
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onFirst
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performCustomAccessibilityActionWithLabel
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performTouchInput
import app.pley.android.data.Loadable
import app.pley.android.model.Hubs
import app.pley.android.model.Library
import app.pley.android.model.LibraryKind
import app.pley.android.screens.LibraryTab
import app.pley.android.screens.Navigator
import app.pley.android.screens.PleyKey
import app.pley.android.screens.home.HomeViewModel
import app.pley.android.screens.home.LibrariesViewModel
import app.pley.android.testing.Fixtures
import app.pley.android.ui.phone.kit.KitProbe
import app.pley.android.ui.phone.shell.LocalPhoneNavigator
import app.pley.android.ui.phone.shell.LocalPhoneShell
import app.pley.android.ui.phone.shell.PhoneShellState
import app.pley.android.ui.phone.shell.rememberPhoneNavigation
import app.pley.android.ui.phone.theme.PhoneTheme
import kotlinx.coroutines.Dispatchers
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

/**
 * Home and the Libraries tab on a phone: drawn from what was saved without
 * the network, the web's empty and error states, and where cards, long
 * presses and row titles go.
 */
@RunWith(RobolectricTestRunner::class)
@Config(qualifiers = "w412dp-h915dp-xxhdpi")
class PhoneHomeTest {
    @get:Rule
    val compose = createComposeRule()

    private val sources = TestHomeSources().apply {
        home.value = Loadable.Ready(Fixtures.decode<Hubs>("hubs/home.json"), fresh = false)
    }

    private val opened = mutableListOf<PleyKey>()
    private val navigator = object : Navigator {
        override fun open(key: PleyKey) {
            opened += key
        }

        override fun back() = Unit
        override fun restart(key: PleyKey) = Unit
    }

    @Composable
    private fun Page(content: @Composable () -> Unit) {
        PhoneTheme {
            val navigation = rememberPhoneNavigation(PleyKey.Home) { }
            val resources = LocalResources.current
            val shell = remember { PhoneShellState(navigation, resources, null) }
            CompositionLocalProvider(LocalPhoneNavigator provides navigator, LocalPhoneShell provides shell) { content() }
        }
    }

    private fun home() {
        val vm = HomeViewModel(sources, Dispatchers.Unconfined)
        compose.setContent { Page { PhoneHomeScreen(vm) } }
        compose.waitForIdle()
    }

    @After
    fun forget() {
        KitProbe.rowComposed = null
        KitProbe.cardComposed = null
    }

    @Test
    fun theServerAnsweringWithWhatShowsRecomposesNoRowOrCard() {
        val rows = mutableListOf<String>()
        val cards = mutableListOf<Long>()
        KitProbe.rowComposed = { synchronized(rows) { rows += it } }
        KitProbe.cardComposed = { synchronized(cards) { cards += it } }
        home()
        val rowsBefore = synchronized(rows) { rows.size }
        val cardsBefore = synchronized(cards) { cards.size }
        assertTrue(rowsBefore > 0 && cardsBefore > 0)

        // The server answers what was saved: only Home's freshness changes.
        sources.home.value = Loadable.Ready(Fixtures.decode<Hubs>("hubs/home.json"), fresh = true)
        compose.waitForIdle()
        assertEquals("rows composed again: ${synchronized(rows) { rows.drop(rowsBefore) }}", rowsBefore, synchronized(rows) { rows.size })
        assertEquals(cardsBefore, synchronized(cards) { cards.size })
    }

    @Test
    fun rowsAreDrawnFromWhatWasSaved() {
        home()
        compose.onNodeWithText("Continue Watching").assertExists()
        compose.onNodeWithText("Recently Added in Movies").assertExists()
        // Continue Watching names the show, the episode's number and what is left.
        // (Captions are drawn; the card says the whole tile to screen readers.)
        compose.onAllNodesWithText("Severance", useUnmergedTree = true).onFirst().assertExists()
        compose.onNodeWithText("S1 · E2 · 34 min left", useUnmergedTree = true).assertExists()
        compose.onNodeWithContentDescription("Dune: Part Two", substring = true).assertExists()
        assertTrue(sources.prefetched.containsAll(listOf(204L, 101L)))
    }

    @Test
    fun cardsOpenTheirPagesAndALongPressTheirActions() {
        home()
        compose.onAllNodesWithContentDescription("Dune: Part Two", substring = true).onFirst().performClick()
        assertEquals(PleyKey.Item(104), opened.last())
        compose.onAllNodesWithContentDescription("Half Loop", substring = true).onFirst().performTouchInput { longClick() }
        // From Continue Watching, the actions offer to take it out of the row; elsewhere not.
        assertEquals(PleyKey.ItemActions(204, continueWatching = true), opened.last())
        compose.onAllNodesWithContentDescription("Dune: Part Two", substring = true).onFirst().performTouchInput { longClick() }
        assertEquals(PleyKey.ItemActions(104), opened.last())
    }

    @Test
    fun aRowTitleOpensItsLibrary() {
        home()
        compose.onNodeWithText("Recently Added in Movies").performClick()
        assertEquals(PleyKey.Library(1, LibraryTab.Library, "sort=added&order=desc"), opened.last())
    }

    @Test
    fun noLibrariesAndScanningSayWhatTheWebSays() {
        sources.home.value = Loadable.Loading
        sources.libraryList.value = Loadable.Ready(emptyList(), fresh = true)
        home()
        compose.onNodeWithText("No libraries yet").assertExists()
        compose.onNodeWithText("Ask the server owner to add a library.").assertExists()

        sources.libraryList.value = Loadable.Ready(listOf(Library(1, "Movies", LibraryKind.Movies, scanning = true)), fresh = true)
        sources.home.value = Loadable.Ready(Hubs(), fresh = true)
        compose.waitForIdle()
        compose.onNodeWithText("Scanning your libraries…").assertExists()
        compose.onNodeWithText("New items will show up here as soon as they're found.").assertExists()
    }

    @Test
    fun anErrorWithNothingSavedOffersRetry() {
        sources.home.value = Loadable.Failed(java.net.ConnectException("refused"))
        home()
        compose.onNodeWithText("Couldn't load your home screen").assertExists()
        compose.onNodeWithText("Nothing answers there.").assertExists()
        compose.onNodeWithText("Retry").performClick()
        compose.waitForIdle()
        assertEquals(1, sources.hubRefreshes)
    }

    @Test
    fun theLibrariesTabListsKindNameCountAndScans() {
        sources.libraryList.value = Loadable.Ready(
            listOf(
                Library(1, "Movies", LibraryKind.Movies, itemCount = 1500),
                Library(2, "TV Shows", LibraryKind.Shows, itemCount = 300, scanning = true),
            ),
            fresh = true,
        )
        val vm = LibrariesViewModel({ sources.libraries }, { sources.scans }, Dispatchers.Unconfined)
        compose.setContent { Page { PhoneLibrariesScreen(vm) } }
        compose.waitForIdle()
        compose.onNodeWithText("1,500 movies").assertExists()
        compose.onNodeWithText("300 shows").assertExists()
        compose.onNodeWithText("TV Shows").performClick()
        assertEquals(PleyKey.Library(2), opened.last())
    }

    @OptIn(ExperimentalTestApi::class)
    @Test
    fun sortingTheLibrariesSavesEachMoveForTheProfile() {
        sources.libraryList.value = Loadable.Ready(
            listOf(
                Library(1, "Movies", LibraryKind.Movies, itemCount = 1500),
                Library(2, "TV Shows", LibraryKind.Shows, itemCount = 300),
                Library(3, "Music", LibraryKind.Music, itemCount = 80),
            ),
            fresh = true,
        )
        val vm = LibrariesViewModel({ sources.libraries }, { sources.scans }, Dispatchers.Unconfined)
        compose.setContent { Page { PhoneLibrariesScreen(vm) } }
        compose.waitForIdle()

        compose.onNodeWithContentDescription("Sort libraries").performClick()
        compose.onNodeWithText("Sort libraries").assertExists()
        compose.onNodeWithText("Drag a library by its handle to move it. Your order applies on all your devices.").assertExists()
        compose.onNodeWithTag(LIBRARY_HANDLE + 1, useUnmergedTree = true).assertExists()
        // A row doesn't open its library while sorting; TalkBack moves it.
        compose.onNodeWithText("Movies").performClick()
        assertTrue(opened.isEmpty())
        compose.onNodeWithTag(LIBRARY_SORT_ROW + 1).performCustomAccessibilityActionWithLabel("Move down")
        assertEquals(listOf(listOf(2L, 1L, 3L)), sources.sorts)
        compose.onNodeWithTag(LIBRARY_SORT_ROW + 3).performCustomAccessibilityActionWithLabel("Move up")
        assertEquals(listOf(2L, 3L, 1L), sources.sorts.last())
        compose.onNodeWithText("Use the server's order").performClick()
        assertEquals(1, sources.orderForgotten)

        compose.onNodeWithText("Done").performClick()
        compose.onNodeWithText("Libraries").assertExists()
        compose.onNodeWithTag(LIBRARY_HANDLE + 1, useUnmergedTree = true).assertDoesNotExist()
        compose.onNodeWithText("Movies").performClick()
        assertEquals(PleyKey.Library(1), opened.last())
    }
}
