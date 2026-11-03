package app.pley.android.ui.tv.item

import androidx.activity.ComponentActivity
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.hasAnyAncestor
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.isFocused
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onRoot
import androidx.compose.ui.test.performKeyInput
import androidx.compose.ui.test.pressKey
import androidx.test.core.app.ApplicationProvider
import app.pley.android.common.TestTags
import app.pley.android.data.Loadable
import app.pley.android.model.Hubs
import app.pley.android.model.ItemDetail
import app.pley.android.model.Library
import app.pley.android.model.LibraryKind
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
import java.net.SocketTimeoutException
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
 * A tile's actions on TV, in the app's frame (D-pad rule 6): Menu on a
 * Home tile opens them in a side panel over Home, which stays as it was.
 * Back closes them with focus back on the tile; a choice runs, with focus
 * back on it too; actions that can't load say why, and no panel opens.
 */
@OptIn(ExperimentalTestApi::class)
@RunWith(RobolectricTestRunner::class)
@Config(qualifiers = "w960dp-h540dp-land-television-xhdpi", application = TvTestApplication::class)
class TvTileActionsTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    private class Source : TvShellSource {
        override suspend fun startFacts() = StartFacts(server = "http://192.168.1.20:7539", signedIn = true)

        override val libraries: Flow<List<Library>> = MutableStateFlow(listOf(Library(1, "Movies", LibraryKind.Movies), Library(2, "Shows", LibraryKind.Shows)))

        override val profile: Flow<RailProfile?> = MutableStateFlow(RailProfile("Anna"))

        override suspend fun keepUpdated() = Unit
    }

    private val sources get() = (ApplicationProvider.getApplicationContext<TvTestApplication>().screensGraph as TvTestGraph).sources

    private val panel = hasAnyAncestor(hasTestTag(TV_TILE_ACTIONS))

    /** Home, with every tile's details in, and focus on the first Continue Watching tile ("Half Loop"). */
    private fun home(reduced: Boolean = true) {
        val hubs: Hubs = Fixtures.decode("hubs/home.json")
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
                TvApp(Source(), onExit = {}, reducedMotion = reduced)
            }
        }
        waitUntil { compose.onAllNodes(isFocused()).fetchSemanticsNodes().isNotEmpty() }
        compose.press(Key.DirectionDown)
        assertTrue(focusedDescription(), focusedDescription().startsWith(HALF_LOOP))
    }

    private fun waitUntil(condition: () -> Boolean) {
        compose.waitUntil(WITHIN_MS, condition)
        compose.waitForIdle()
    }

    private fun exists(matcher: SemanticsMatcher): Boolean = compose.onAllNodes(matcher).fetchSemanticsNodes().isNotEmpty()

    private fun focusedDescription(): String = compose.onAllNodes(isFocused()).fetchSemanticsNodes().firstOrNull()
        ?.config?.getOrElse(SemanticsProperties.ContentDescription) { emptyList() }?.joinToString().orEmpty()

    private fun back() {
        compose.runOnUiThread { compose.activity.onBackPressedDispatcher.onBackPressed() }
        compose.waitForIdle()
    }

    @Test
    fun menuOpensTheActionsBesideThePage() {
        home()
        compose.press(Key.Menu)
        waitUntil { exists(hasTestTag(TV_TILE_ACTIONS)) }
        // The show's name, then its first action, with focus; Home is still there, under it.
        assertTrue(exists(panel and hasText("Severance")))
        waitUntil { exists(panel and isFocused() and hasText("Resume")) }
        compose.onNodeWithTag(TestTags.HOME_ROWS).assertExists()
        compose.onNodeWithTag(TV_ITEM_ACTIONS).assertDoesNotExist()

        // Back closes them, and focus is on the tile again.
        back()
        waitUntil { !exists(hasTestTag(TV_TILE_ACTIONS)) }
        assertTrue(focusedDescription(), focusedDescription().startsWith(HALF_LOOP))
    }

    private fun frame() {
        compose.mainClock.advanceTimeByFrame()
        ShadowLooper.idleMainLooper()
    }

    /** Menu, then Back [frames] frames after the panel comes in, as it slides in; it has gone once this returns. */
    private fun backAsThePanelComesIn(frames: Int) {
        compose.mainClock.autoAdvance = false
        compose.onRoot().performKeyInput { pressKey(Key.Menu) }
        var waited = 0
        while (!exists(hasTestTag(TV_TILE_ACTIONS)) && waited++ < 300) frame()
        assertTrue("the panel came", exists(hasTestTag(TV_TILE_ACTIONS)))
        repeat(frames) { frame() }
        back()
        compose.mainClock.autoAdvance = true
        waitUntil { !exists(hasTestTag(TV_TILE_ACTIONS)) }
    }

    @Test
    fun backAsThePanelComesInLeavesFocusOnTheTile() {
        home(reduced = false)
        // The frame the panel comes in, before it takes focus.
        backAsThePanelComesIn(frames = 0)
        assertTrue(focusedDescription(), focusedDescription().startsWith(HALF_LOOP))
    }

    @Test
    fun backJustBeforeThePanelsFirstRowTakesFocusLeavesFocusOnTheTile() {
        home(reduced = false)
        // The panel has kept where focus was on the page and waits a frame more for its first row:
        // Back then leaves focus on the tile, and the row doesn't take it as the panel goes.
        backAsThePanelComesIn(frames = 2)
        assertTrue(focusedDescription(), focusedDescription().startsWith(HALF_LOOP))
    }

    @Test
    fun aChoiceRunsWithFocusBackOnTheTile() {
        home()
        compose.press(Key.Menu)
        waitUntil { exists(panel and isFocused()) }
        while (!exists(panel and isFocused() and hasText(MARK_WATCHED))) compose.press(Key.DirectionDown)
        compose.press(Key.DirectionCenter)

        waitUntil { sources.watched.isNotEmpty() }
        assertEquals(listOf(HALF_LOOP_ID to true), sources.watched.toList())
        waitUntil { !exists(hasTestTag(TV_TILE_ACTIONS)) }
        assertTrue(focusedDescription(), focusedDescription().startsWith(HALF_LOOP))
    }

    @Test
    fun actionsThatCantLoadSayWhy() {
        home()
        sources.detail(HALF_LOOP_ID).value = Loadable.Failed(SocketTimeoutException())
        compose.press(Key.Menu)

        waitUntil { exists(hasText("No answer in time.")) }
        compose.onNodeWithTag(TV_TILE_ACTIONS).assertDoesNotExist()
        assertTrue(focusedDescription(), focusedDescription().startsWith(HALF_LOOP))
    }

    private companion object {
        const val WITHIN_MS = 5_000L
        const val HALF_LOOP = "Half Loop"
        const val HALF_LOOP_ID = 204L
        const val MARK_WATCHED = "Mark as watched"
    }
}
