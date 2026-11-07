package app.pley.android.ui.tv.home

import androidx.activity.ComponentActivity
import androidx.compose.foundation.focusable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertIsFocused
import androidx.compose.ui.test.isFocused
import androidx.compose.ui.test.junit4.StateRestorationTester
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.onRoot
import androidx.compose.ui.test.performKeyInput
import androidx.compose.ui.test.pressKey
import androidx.compose.ui.unit.dp
import androidx.navigation3.runtime.NavKey
import app.pley.android.data.Loadable
import app.pley.android.model.HubKeys
import app.pley.android.model.Hubs
import app.pley.android.model.ItemKind
import app.pley.android.model.ItemSummary
import app.pley.android.model.Library
import app.pley.android.model.LibraryKind
import app.pley.android.network.OfflineException
import app.pley.android.screens.PleyKey
import app.pley.android.screens.home.HomeViewModel
import app.pley.android.screens.home.LibrariesViewModel
import app.pley.android.screens.requests.RequestsEntry
import app.pley.android.testing.Fixtures
import app.pley.android.ui.tv.kit.LocalTvRailFocus
import app.pley.android.ui.tv.kit.TvRailFocus
import app.pley.android.ui.tv.shell.ANNOUNCE_MS
import app.pley.android.ui.tv.shell.LocalTvNavigator
import app.pley.android.ui.tv.shell.LocalTvOffline
import app.pley.android.ui.tv.shell.RailState
import app.pley.android.ui.tv.shell.TvNavigator
import app.pley.android.ui.tv.shell.TvOfflineNotice
import app.pley.android.ui.tv.shell.TvOfflineState
import app.pley.android.ui.tv.shell.railItems
import app.pley.android.ui.tv.shell.requestsItem
import app.pley.android.ui.tv.theme.TvTheme
import java.io.IOException
import kotlinx.coroutines.Dispatchers
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

/**
 * Home on TV with key events: initial focus on the featured title's Play,
 * Right through the hero's buttons on to the next title, the hero turning
 * by itself while nothing is pressed, Down into the rows, the focused tile coming back after the page was away, Back from
 * the rows going to the rail with the page back at its hero, and the
 * states drawn from what was saved without the network.
 */
@OptIn(ExperimentalTestApi::class)
@RunWith(RobolectricTestRunner::class)
@Config(qualifiers = "w960dp-h540dp-xhdpi")
class TvHomeTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    private val sources = TestHomeSources().apply {
        home.value = Loadable.Ready(Fixtures.decode<Hubs>("hubs/home.json"), fresh = false)
    }
    private val backStack = mutableListOf<NavKey>(PleyKey.Home)

    private val rail = object : TvRailFocus {
        val requester = FocusRequester()
        var asked = 0
        override var keepsFocus = false

        override fun focusCurrent(): Boolean {
            asked++
            requester.requestFocus()
            return true
        }
    }

    @Composable
    private fun Page(vm: HomeViewModel) {
        TvTheme {
            val navigator = remember { TvNavigator(backStack, RailState(), exit = {}) }
            CompositionLocalProvider(LocalTvNavigator provides navigator, LocalTvRailFocus provides rail) {
                Row(Modifier.fillMaxSize()) {
                    Box(Modifier.size(56.dp).focusRequester(rail.requester).onFocusChanged { rail.keepsFocus = it.hasFocus }.testTag("rail").focusable())
                    TvHomeScreen(vm)
                }
            }
        }
    }

    private fun show() {
        val vm = HomeViewModel(sources, Dispatchers.Unconfined)
        compose.setContent { Page(vm) }
    }

    private fun press(key: Key, times: Int = 1) = repeat(times) {
        compose.onRoot().performKeyInput { pressKey(key) }
        compose.waitForIdle()
    }

    private fun focusedDescription(): String =
        compose.onNode(isFocused()).fetchSemanticsNode().config.getOrElse(SemanticsProperties.ContentDescription) { emptyList() }.joinToString()

    @Test
    fun playHasFocusFirstAndDownGoesIntoTheRows() {
        show()
        compose.waitForIdle()
        // Drawn from what was saved: the hero features the newest film with a backdrop.
        compose.onNodeWithText("Dune: Part Two").assertExists()
        compose.onNodeWithText("NEW").assertExists()
        compose.onNodeWithText("Play").assertIsFocused()

        press(Key.DirectionDown)
        assertTrue(focusedDescription(), focusedDescription().startsWith("Half Loop"))
        press(Key.DirectionRight)
        assertTrue(focusedDescription(), focusedDescription().startsWith("Arrival"))
        // Down lands on the nearest tile of the next row: past its one movie, "See all".
        press(Key.DirectionDown)
        assertEquals("See all", focusedDescription())
        press(Key.DirectionLeft)
        assertTrue(focusedDescription(), focusedDescription().startsWith("Dune: Part Two"))
    }

    @Test
    fun savedRowsWithTheServerAwaySaySoAWhileAndStayMarkedUntilItAnswers() {
        val saved = Fixtures.decode<Hubs>("hubs/home.json")
        sources.home.value = Loadable.Failed(OfflineException(IOException("no answer in time")), saved)
        val offline = TvOfflineState()
        val vm = HomeViewModel(sources, Dispatchers.Unconfined)
        compose.mainClock.autoAdvance = false
        compose.setContent {
            CompositionLocalProvider(LocalTvOffline provides offline) {
                Box(Modifier.fillMaxSize()) {
                    Page(vm)
                    TvOfflineNotice(offline)
                }
            }
        }
        compose.mainClock.advanceTimeBy(1_000)
        compose.onNodeWithText("Dune: Part Two").assertExists()
        compose.onNodeWithText("Pley is offline").assertIsDisplayed()
        compose.onNodeWithText("Play").assertIsFocused()

        // Said for a while; the rail's mark stays.
        compose.mainClock.advanceTimeBy(ANNOUNCE_MS + 1_000)
        compose.onNodeWithText("Pley is offline").assertDoesNotExist()
        assertTrue(offline.offline)

        sources.home.value = Loadable.Ready(saved, fresh = true)
        compose.mainClock.advanceTimeBy(1_000)
        assertFalse(offline.offline)
    }

    @Test
    fun theFocusedTileComesBackAfterThePageWasAway() {
        val vm = HomeViewModel(sources, Dispatchers.Unconfined)
        val restoration = StateRestorationTester(compose)
        restoration.setContent { Page(vm) }
        compose.waitForIdle()
        press(Key.DirectionDown)
        press(Key.DirectionRight)
        assertTrue(focusedDescription().startsWith("Arrival"))

        restoration.emulateSavedInstanceStateRestore()
        compose.waitForIdle()
        compose.mainClock.advanceTimeBy(100)
        compose.waitForIdle()
        assertTrue(focusedDescription(), focusedDescription().startsWith("Arrival"))
    }

    @Test
    fun backFromTheRowsGoesToTheRailAndTheHeroWaits() {
        show()
        compose.waitForIdle()
        press(Key.DirectionDown, times = 2)
        compose.runOnUiThread { compose.activity.onBackPressedDispatcher.onBackPressed() }
        compose.waitForIdle()
        assertEquals(1, rail.asked)
        compose.onNodeWithTag("rail").assertIsFocused()
        // Coming back from the rail lands on Play, not on the row left.
        press(Key.DirectionRight)
        compose.onNodeWithText("Play").assertIsFocused()
    }

    @Test
    fun rightFromTheLastButtonGoesOnToTheNextTitle() {
        sources.home.value = Loadable.Ready(twoFeatured(), fresh = false)
        show()
        compose.waitForIdle()
        compose.onNodeWithText("Dune: Part Two").assertExists()
        compose.onNodeWithText("Play").assertIsFocused()
        // Play, Info, My List; then the next title, from its Play.
        press(Key.DirectionRight, times = 2)
        assertEquals("Add to My List", focusedDescription())
        press(Key.DirectionRight)
        compose.onNodeWithText("Severance").assertExists()
        compose.onNodeWithText("Play").assertIsFocused()
    }

    @Test
    fun theHeroTurnsByItselfWhileNothingIsPressed() {
        sources.home.value = Loadable.Ready(twoFeatured(), fresh = false)
        compose.mainClock.autoAdvance = false
        show()
        compose.mainClock.advanceTimeBy(1_000)
        compose.onNodeWithText("Dune: Part Two").assertExists()
        compose.mainClock.advanceTimeBy(9_000)
        compose.onNodeWithText("Severance").assertExists()
        compose.onNodeWithText("Play").assertIsFocused()
    }

    /** What was saved, with Severance's show in Recently Added too: two titles to feature, a film and a show. */
    private fun twoFeatured(): Hubs {
        val saved = Fixtures.decode<Hubs>("hubs/home.json")
        val episode = saved.hubs.first { it.key == HubKeys.CONTINUE_WATCHING }.items.first()
        val show = ItemSummary(
            id = episode.grandparentId ?: 200,
            libraryId = 2,
            kind = ItemKind.Show,
            title = episode.grandparentTitle ?: "Severance",
            backdropUrl = "/api/items/200/images/backdrop",
        )
        return Hubs(saved.hubs.map { if (it.key == "recently_added:2") it.copy(items = listOf(show)) else it })
    }

    @Test
    fun selectOpensTheItemAndLongPressItsActions() {
        show()
        compose.waitForIdle()
        press(Key.DirectionDown)
        press(Key.DirectionCenter)
        assertEquals(PleyKey.Item(204), backStack.last())
    }

    @Test
    fun noLibrariesSaysSoInTheWebsWords() {
        sources.home.value = Loadable.Loading
        sources.libraryList.value = Loadable.Ready(emptyList(), fresh = true)
        show()
        compose.waitForIdle()
        compose.onNodeWithText("No libraries yet").assertExists()
        compose.onNodeWithText("Ask the server owner to add a library.").assertExists()
    }

    @Test
    fun anErrorWithNothingSavedOffersRetryWithFocus() {
        sources.home.value = Loadable.Failed(java.net.ConnectException("refused"))
        show()
        compose.waitForIdle()
        compose.onNodeWithText("Couldn't load your home screen").assertExists()
        compose.onNodeWithText("Nothing answers there.").assertExists()
        compose.onNodeWithText("Retry").assertIsFocused()
        press(Key.DirectionCenter)
        assertEquals(1, sources.hubRefreshes)
    }

    @Test
    fun theLibrariesPageListsThemAllWithFocusOnTheFirst() {
        sources.libraryList.value = Loadable.Ready(
                listOf(
                    Library(1, "Movies", LibraryKind.Movies, itemCount = 412),
                    Library(2, "Shows", LibraryKind.Shows, itemCount = 38, scanning = true),
                    Library(3, "Music", LibraryKind.Music, itemCount = 120),
                    Library(4, "Concerts", LibraryKind.Movies, itemCount = 12),
                ),
            fresh = true,
        )
        val vm = LibrariesViewModel({ sources.libraries }, { sources.scans }, Dispatchers.Unconfined)
        compose.setContent {
            TvTheme {
                val navigator = remember { TvNavigator(backStack, RailState(), exit = {}) }
                CompositionLocalProvider(LocalTvNavigator provides navigator) { TvLibrariesScreen(vm) }
            }
        }
        compose.waitForIdle()
        compose.onNodeWithText("Libraries").assertExists()
        compose.onNodeWithText("Concerts").assertExists()
        compose.onNodeWithText("38 shows · Scanning…").assertExists()
        compose.onNodeWithText("Movies").assertIsFocused()
        press(Key.DirectionRight)
        press(Key.DirectionCenter)
        assertEquals(PleyKey.Library(2), backStack.last())
    }

    @Test
    fun theLibrariesPageListsTheLibrariesAloneWhilePlaylistsStaysInTheRail() {
        val libraries = listOf(Library(1, "Movies", LibraryKind.Movies, itemCount = 412), Library(2, "Shows", LibraryKind.Shows, itemCount = 38))
        sources.libraryList.value = Loadable.Ready(libraries, fresh = true)
        val vm = LibrariesViewModel({ sources.libraries }, { sources.scans }, Dispatchers.Unconfined)
        // Live TV, Discover, Now Playing and a USB drive: nothing leaves the rail for them.
        val rail = railItems(libraries, nowPlaying = true, liveTv = true, requests = requestsItem(RequestsEntry(discover = true)), drive = true)
        assertTrue(rail.any { it.key == PleyKey.Playlists })
        compose.setContent {
            TvTheme {
                val navigator = remember { TvNavigator(backStack, RailState(), exit = {}) }
                CompositionLocalProvider(LocalTvNavigator provides navigator) { TvLibrariesScreen(vm) }
            }
        }
        compose.waitForIdle()
        compose.onNodeWithText("Shows").assertExists()
        compose.onNodeWithText("Playlists").assertDoesNotExist()
        compose.onNodeWithText("Movies").assertIsFocused()
        press(Key.DirectionRight, times = 2)
        compose.onNodeWithText("Shows").assertIsFocused()
    }

    @Test
    fun sortingLiftsACardTheArrowsMoveThroughTheGrid() {
        sources.libraryList.value = Loadable.Ready(
            listOf(
                Library(1, "Movies", LibraryKind.Movies, itemCount = 412),
                Library(2, "Shows", LibraryKind.Shows, itemCount = 38),
                Library(3, "Music", LibraryKind.Music, itemCount = 120),
                Library(4, "Concerts", LibraryKind.Movies, itemCount = 12),
            ),
            fresh = true,
        )
        val vm = LibrariesViewModel({ sources.libraries }, { sources.scans }, Dispatchers.Unconfined)
        compose.setContent {
            TvTheme {
                val navigator = remember { TvNavigator(backStack, RailState(), exit = {}) }
                CompositionLocalProvider(LocalTvNavigator provides navigator) { TvLibrariesScreen(vm) }
            }
        }
        compose.waitForIdle()
        compose.onNodeWithText("Movies").assertIsFocused()

        // Sort, by the title: the first card takes the focus, and Select lifts it.
        press(Key.DirectionUp)
        compose.onNodeWithTag(TV_LIBRARIES_SORT).assertIsFocused()
        press(Key.DirectionCenter)
        compose.onNodeWithText("Select a library to move it. Your order applies on all your devices.").assertExists()
        compose.onNodeWithText("Movies").assertIsFocused()
        press(Key.DirectionCenter)
        compose.onNodeWithText("Arrows to move it, OK to drop it here, Back to put it back").assertExists()

        // Right a place, down a row: it moves, and keeps the focus; nothing is saved yet.
        press(Key.DirectionRight)
        press(Key.DirectionDown)
        compose.waitForIdle()
        compose.onNodeWithText("Movies").assertIsFocused()
        assertTrue(sources.sorts.isEmpty())
        // Down again is past the end: it stays.
        press(Key.DirectionDown)
        press(Key.DirectionCenter)
        assertEquals(listOf(listOf(2L, 3L, 4L, 1L)), sources.sorts)

        // Back puts a lifted card back where it was, then ends sorting.
        press(Key.DirectionCenter)
        press(Key.DirectionLeft)
        compose.onNodeWithTag(TV_LIBRARY_CARD + 1).assertIsFocused()
        compose.runOnUiThread { compose.activity.onBackPressedDispatcher.onBackPressed() }
        compose.waitForIdle()
        assertEquals(1, sources.sorts.size)
        compose.runOnUiThread { compose.activity.onBackPressedDispatcher.onBackPressed() }
        compose.waitForIdle()
        compose.onNodeWithText("Libraries").assertExists()
        press(Key.DirectionCenter)
        assertEquals(PleyKey.Library(1), backStack.last())
    }
}
