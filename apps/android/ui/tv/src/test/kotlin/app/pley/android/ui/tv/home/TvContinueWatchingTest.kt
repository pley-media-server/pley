package app.pley.android.ui.tv.home

import androidx.activity.ComponentActivity
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.isFocused
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.test.core.app.ApplicationProvider
import app.pley.android.data.Loadable
import app.pley.android.model.HubKeys
import app.pley.android.model.Hubs
import app.pley.android.model.ItemDetail
import app.pley.android.model.ItemKind
import app.pley.android.model.Library
import app.pley.android.model.LibraryKind
import app.pley.android.model.UserState
import app.pley.android.testing.Fixtures
import app.pley.android.testing.screens.onboarding.FakeOnboardingSource
import app.pley.android.ui.tv.kit.press
import app.pley.android.ui.tv.onboarding.LocalOnboardingSource
import app.pley.android.ui.tv.shell.RailProfile
import app.pley.android.ui.tv.shell.StartFacts
import app.pley.android.ui.tv.shell.TvApp
import app.pley.android.ui.tv.shell.TvShellSource
import app.pley.android.ui.tv.shell.TvTestApplication
import app.pley.android.ui.tv.shell.TvTestGraph
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

/**
 * Remove from Continue Watching on TV, in the app's frame: Menu on a
 * Continue Watching tile opens its actions, which offer it; after it, Home
 * is back with the tile gone and focus on its neighbour, never on the
 * hero. Also when the row goes with its last tile.
 */
@RunWith(RobolectricTestRunner::class)
@Config(qualifiers = "w960dp-h540dp-land-television-xhdpi", application = TvTestApplication::class)
class TvContinueWatchingTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    private class Source : TvShellSource {
        override suspend fun startFacts() = StartFacts(server = "http://192.168.1.20:7539", signedIn = true)

        override val libraries: Flow<List<Library>> = MutableStateFlow(
            listOf(Library(1, "Movies", LibraryKind.Movies), Library(2, "Shows", LibraryKind.Shows), Library(3, "Music", LibraryKind.Music)),
        )

        override val profile: Flow<RailProfile?> = MutableStateFlow(RailProfile("Anna"))

        override suspend fun keepUpdated() = Unit
    }

    private val sources get() = (ApplicationProvider.getApplicationContext<TvTestApplication>().screensGraph as TvTestGraph).sources

    // Nothing to feature (no backdrops in Recently Added): the hero is Continue Watching's first, and follows it.
    private val fixtures: Hubs = Fixtures.decode<Hubs>("hubs/home.json").let { hubs ->
        Hubs(hubs.hubs.map { hub -> if (hub.key.startsWith("recently_added")) hub.copy(items = hub.items.map { it.copy(backdropUrl = null) }) else hub })
    }

    private fun app(hubs: Hubs = fixtures) {
        sources.home.value = Loadable.Ready(hubs, fresh = true)
        // The actions are worked out from the items' details.
        for (item in hubs.hubs.flatMap { it.items }) {
            sources.detail(item.id).value = Loadable.Ready(
                ItemDetail(
                    id = item.id, libraryId = item.libraryId, kind = item.kind, title = item.title, parentId = item.parentId,
                    grandparentId = item.grandparentId, grandparentTitle = item.grandparentTitle, durationMs = item.durationMs,
                    userState = item.userState,
                ),
                fresh = true,
            )
        }
        val onboarding = FakeOnboardingSource().apply { active.value = "http://s" }
        compose.setContent {
            CompositionLocalProvider(LocalOnboardingSource provides onboarding) {
                TvApp(Source(), onExit = {}, reducedMotion = true)
            }
        }
        waitUntil { compose.onAllNodes(isFocused()).fetchSemanticsNodes().isNotEmpty() }
        compose.onNodeWithText("Resume").assertExists()
    }

    private fun waitUntil(condition: () -> Boolean) {
        compose.waitUntil(WITHIN_MS, condition)
        compose.waitForIdle()
    }

    private fun focusedDescription(): String = compose.onAllNodes(isFocused()).fetchSemanticsNodes().firstOrNull()
        ?.config?.getOrElse(SemanticsProperties.ContentDescription) { emptyList() }?.joinToString().orEmpty()

    /** Menu on the focused tile, then Remove from Continue Watching in its actions. */
    private fun removeFocused() {
        compose.press(Key.Menu)
        waitUntil { compose.onAllNodes(hasText(REMOVE)).fetchSemanticsNodes().isNotEmpty() }
        // Focus starts on the first action; down to Remove.
        while (compose.onAllNodes(isFocused() and hasText(REMOVE)).fetchSemanticsNodes().isEmpty()) compose.press(Key.DirectionDown)
        compose.press(Key.DirectionCenter)
    }

    @Test
    fun aRemovedTileHandsFocusToItsNeighbour() {
        app()
        compose.press(Key.DirectionDown)
        assertTrue(focusedDescription(), focusedDescription().startsWith("Half Loop"))

        removeFocused()

        waitUntil { focusedDescription().startsWith("Arrival") }
        assertEquals(listOf(204L), sources.hidden)
        compose.onAllNodes(hasText(REMOVE)).fetchSemanticsNodes().let { assertTrue(it.isEmpty()) }
        // The hero follows Continue Watching: Arrival now.
        compose.onNodeWithText("Resume").assertExists()
    }

    @Test
    fun theLastTileTakesItsRowAndFocusGoesToTheNextRow() {
        val single = Hubs(fixtures.hubs.map { if (it.key == HubKeys.CONTINUE_WATCHING) it.copy(items = it.items.take(1)) else it })
        app(single)
        compose.press(Key.DirectionDown)
        assertTrue(focusedDescription(), focusedDescription().startsWith("Half Loop"))

        removeFocused()

        waitUntil { focusedDescription().startsWith("Dune: Part Two") }
        assertEquals(listOf(204L), sources.hidden)
    }

    @Test
    fun aFocusedTileLeavingItsRowHandsFocusOn() {
        app()
        compose.press(Key.DirectionDown)
        assertTrue(focusedDescription().startsWith("Half Loop"))

        // Taken away under focus (another device, or the answer arriving late).
        val shown = (sources.home.value as Loadable.Ready).value
        sources.home.value = Loadable.Ready(
            Hubs(shown.hubs.map { if (it.key == HubKeys.CONTINUE_WATCHING) it.copy(items = it.items.drop(1)) else it }),
            fresh = true,
        )

        waitUntil { focusedDescription().startsWith("Arrival") }
        // The page stays where it was, the hero above the row: focus went nowhere else on the way.
        compose.onNodeWithText("Resume").assertExists()
    }

    @Test
    fun aFocusedRowLeavingHandsFocusToTheNextRow() {
        app()
        compose.press(Key.DirectionDown)
        assertTrue(focusedDescription().startsWith("Half Loop"))

        val shown = (sources.home.value as Loadable.Ready).value
        sources.home.value = Loadable.Ready(Hubs(shown.hubs.filterNot { it.key == HubKeys.CONTINUE_WATCHING }), fresh = true)

        waitUntil { focusedDescription().startsWith("Dune: Part Two") }
    }

    private companion object {
        const val WITHIN_MS = 5_000L
        const val REMOVE = "Remove from Continue Watching"
    }
}
