package app.pley.android.ui.tv.shell

import androidx.activity.OnBackPressedDispatcher
import androidx.activity.compose.BackHandler
import androidx.activity.compose.LocalOnBackPressedDispatcherOwner
import androidx.compose.foundation.focusGroup
import androidx.compose.foundation.focusable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusManager
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.test.assertIsFocused
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.unit.dp
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.navigation3.runtime.NavEntry
import androidx.navigation3.runtime.NavKey
import androidx.navigation3.runtime.rememberSaveableStateHolderNavEntryDecorator
import androidx.navigation3.scene.SinglePaneSceneStrategy
import androidx.navigation3.ui.NavDisplay
import app.pley.android.screens.PleyKey
import app.pley.android.ui.tv.kit.initialFocus
import app.pley.android.ui.tv.kit.press
import app.pley.android.ui.tv.theme.TvTheme
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

/**
 * The rail's page under the pages opened on it ([RootUnderPages]): it is
 * composed once, hidden and stopped while covered, its Back handlers off,
 * and Back shows it with focus where it was. The pages between are
 * composed again, and a new root starts over.
 */
@RunWith(RobolectricTestRunner::class)
@Config(qualifiers = "w960dp-h540dp-xhdpi")
class TvPagesTest {
    @get:Rule
    val compose = createComposeRule()

    private val backStack = mutableStateListOf<NavKey>(PleyKey.Home)
    private val content = FocusRequester()
    private val pages = RootUnderPages { true }
    private lateinit var focusManager: FocusManager
    private val made = HashMap<NavKey, Int>()
    private val gone = HashMap<NavKey, Int>()
    private val lifecycles = HashMap<NavKey, Lifecycle>()

    /** Pages whose initial focus is their second element, b. */
    private val startOnB = HashSet<NavKey>()

    /** Back on the pages that handle it themselves: how many times each did. */
    private val backs = HashMap<NavKey, Int>()
    private lateinit var dispatcher: OnBackPressedDispatcher

    private fun shell() {
        compose.setContent {
            focusManager = LocalFocusManager.current
            dispatcher = LocalOnBackPressedDispatcherOwner.current!!.onBackPressedDispatcher
            TvTheme {
                // The shell's last resort of Back, below every page's own, as TvApp has it.
                BackHandler { if (backStack.size > 1) backStack.removeAt(backStack.lastIndex) }
                // The shell's page group, as TvApp has it.
                Box(Modifier.fillMaxSize().focusRequester(content).focusGroup()) {
                    NavDisplay(
                        backStack = backStack,
                        onBack = { backStack.removeAt(backStack.lastIndex) },
                        entryDecorators = listOf(rememberSaveableStateHolderNavEntryDecorator(), rememberRootPageDecorator(backStack, pages)),
                        sceneStrategies = listOf(pages, SinglePaneSceneStrategy()),
                        entryProvider = { key -> NavEntry(key) { Page(key) } },
                    )
                }
            }
        }
        compose.waitForIdle()
    }

    /**
     * A page: two elements, [key]/a (its initial focus, or b's in [startOnB])
     * and [key]/b right of it; counts its compositions, and the Backs it
     * takes while it is in [backs].
     */
    @Composable
    private fun Page(key: NavKey) {
        remember { made.merge(key, 1, Int::plus) }
        DisposableEffect(key) { onDispose { gone[key] = (gone[key] ?: 0) + 1 } }
        lifecycles[key] = LocalLifecycleOwner.current.lifecycle
        if (key in backs) BackHandler { backs[key] = backs.getValue(key) + 1 }
        val b = key in startOnB
        Box(Modifier.fillMaxSize()) {
            Box(Modifier.offset(100.dp, 100.dp).size(40.dp).testTag("$key/a").initialFocus(enabled = !b).focusable())
            Box(Modifier.offset(300.dp, 100.dp).size(40.dp).testTag("$key/b").initialFocus(enabled = b).focusable())
        }
    }

    private fun open(key: NavKey) {
        compose.runOnUiThread { backStack.add(key) }
        compose.waitForIdle()
    }

    private fun back() {
        compose.runOnUiThread { backStack.removeAt(backStack.lastIndex) }
        compose.waitForIdle()
    }

    @Test
    fun backShowsTheRailsPageAsItWas() {
        shell()
        compose.onNodeWithTag("${PleyKey.Home}/a").assertIsFocused()
        compose.press(Key.DirectionRight)
        compose.onNodeWithTag("${PleyKey.Home}/b").assertIsFocused()

        open(PleyKey.Search())
        compose.onNodeWithTag("${PleyKey.Search()}/a").assertIsFocused()
        // Home is still there, but neither shown nor running.
        compose.onNodeWithTag("${PleyKey.Home}/b").assertDoesNotExist()
        assertEquals(Lifecycle.State.CREATED, lifecycles.getValue(PleyKey.Home).currentState)
        assertEquals(null, gone[PleyKey.Home])

        back()
        // As it was: composed once, focus on the element that had it.
        compose.onNodeWithTag("${PleyKey.Home}/b").assertIsFocused()
        assertEquals(1, made[PleyKey.Home])
        assertEquals(Lifecycle.State.RESUMED, lifecycles.getValue(PleyKey.Home).currentState)
    }

    @Test
    fun thePagesBetweenAreComposedAgain() {
        shell()
        open(PleyKey.Search())
        open(PleyKey.Settings)
        compose.onNodeWithTag("${PleyKey.Settings}/a").assertIsFocused()
        back()
        // Search again, from its saved state; Home stays as it was under it.
        compose.onNodeWithTag("${PleyKey.Search()}/a").assertIsFocused()
        assertEquals(2, made[PleyKey.Search()])
        back()
        compose.onNodeWithTag("${PleyKey.Home}/a").assertIsFocused()
        assertEquals(1, made[PleyKey.Home])
    }

    @Test
    fun focusKeptForAnotherActivityComesBack() {
        shell()
        compose.press(Key.DirectionRight)
        compose.onNodeWithTag("${PleyKey.Home}/b").assertIsFocused()
        // As the shell does when the player takes the window, and when it gives it back.
        compose.runOnUiThread {
            pages.saveFocus()
            content.saveFocusedChild()
            focusManager.clearFocus(force = true)
        }
        compose.waitForIdle()
        compose.runOnUiThread { content.restoreFocusedChild() }
        compose.waitForIdle()
        compose.onNodeWithTag("${PleyKey.Home}/b").assertIsFocused()
    }

    @Test
    fun theRailsPageLeavesBackToThePageOverIt() {
        backs[PleyKey.Home] = 0
        shell()
        open(PleyKey.Search())
        // Home handles Back itself when it shows; covered, Back is the page's over it.
        compose.runOnUiThread { dispatcher.onBackPressed() }
        compose.waitForIdle()
        assertEquals(listOf<NavKey>(PleyKey.Home), backStack.toList())
        assertEquals(0, backs[PleyKey.Home])
        compose.onNodeWithTag("${PleyKey.Home}/a").assertIsFocused()
        // Shown again, it has Back again.
        compose.runOnUiThread { dispatcher.onBackPressed() }
        compose.waitForIdle()
        assertEquals(1, backs[PleyKey.Home])
    }

    @Test
    fun aRailPageNeverShownStartsOnItsOwnFirstElement() {
        // A page over Home from the start, as after process death or a Watch Next program.
        startOnB += PleyKey.Home
        backStack += PleyKey.Search()
        shell()
        compose.onNodeWithTag("${PleyKey.Search()}/a").assertIsFocused()
        back()
        compose.onNodeWithTag("${PleyKey.Home}/b").assertIsFocused()
    }

    @Test
    fun aNewRootStartsOver() {
        shell()
        open(PleyKey.Search())
        compose.runOnUiThread {
            backStack.clear()
            backStack.add(PleyKey.Settings)
        }
        compose.waitForIdle()
        compose.onNodeWithTag("${PleyKey.Settings}/a").assertIsFocused()
        assertEquals(1, gone[PleyKey.Home])
        assertTrue(compose.onAllNodes(androidx.compose.ui.test.hasTestTag("${PleyKey.Home}/a")).fetchSemanticsNodes().isEmpty())
    }
}
