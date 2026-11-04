package app.pley.android.ui.phone.shell

import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.semantics.SemanticsPropertiesAndroid
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.assert
import androidx.compose.ui.test.assertIsSelected
import androidx.compose.ui.test.junit4.StateRestorationTester
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onAllNodesWithTag
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import app.pley.android.auth.SignOutReason
import app.pley.android.common.TestTags
import app.pley.android.data.RemoteSession
import app.pley.android.data.SessionState
import app.pley.android.model.PlayingChannel
import app.pley.android.model.PlayingKind
import app.pley.android.model.PlayingSession
import app.pley.android.network.ApiException
import app.pley.android.screens.PleyKey
import app.pley.android.testing.screens.onboarding.FakeOnboardingSource
import app.pley.android.ui.phone.item.ITEM_SKELETON
import app.pley.android.ui.phone.onboarding.LocalOnboardingSource
import app.pley.android.ui.phone.remote.REMOTE_BAR
import app.pley.android.ui.phone.theme.PhoneTheme
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

/**
 * The phone's shell over its screens (Home, Libraries, items and getting
 * in, placeholders for the rest): the start gate, nothing reached before the
 * first frame, the tabs' back stacks, and what happens when the server
 * refuses or signs the app out.
 */
@RunWith(RobolectricTestRunner::class)
@Config(application = TestApplication::class)
class PhoneShellTest {
    @get:Rule
    val compose = createComposeRule()

    /** Home, over a library with nothing in it yet. */
    private fun showsHome() = shows("Nothing here yet")

    /** The Libraries tab, listing the one library. */
    private fun showsLibraries() = shows(TestGraph.FILMS.name)

    /**
     * Home and Libraries work their state out off the main thread, which
     * waiting for the main thread to be idle doesn't wait for: [text] is
     * waited for, then there once.
     */
    private fun shows(text: String) {
        compose.waitUntil(SHOWN_WITHIN_MS) { compose.onAllNodesWithText(text).fetchSemanticsNodes().isNotEmpty() }
        compose.onNodeWithText(text).assertExists()
    }

    /** An item page, still loading its details. */
    private fun itemPage() = compose.onNodeWithTag(ITEM_SKELETON)

    /** The sign-in page (Servers, and SignIn for a server the app was signed out of). */
    private fun signInPage() = compose.onNodeWithText("Sign in to Pley")

    private fun tab(tab: PhoneTab) = compose.onNodeWithTag(TAB_TAG + tab.name)

    private fun show(start: PhoneStart, graph: TestGraph = TestGraph(start.server)): Shell {
        TestApplication.graph = graph
        val shell = Shell(graph)
        compose.setContent {
            PhoneTheme {
                // Getting in reads a fake server: nobody on the network.
                CompositionLocalProvider(LocalOnboardingSource provides FakeOnboardingSource()) {
                    PhoneShell(start, { graph }) { state, navigation ->
                        shell.state = state
                        shell.navigation = navigation
                    }
                }
            }
        }
        compose.waitForIdle()
        return shell
    }

    class Shell(val graph: TestGraph) {
        lateinit var state: PhoneShellState
        lateinit var navigation: PhoneNavigation
    }

    @Test
    fun itsTagsAreResourceIdsForTheBenchmarks() {
        val graph = TestGraph(null)
        TestApplication.graph = graph
        val start = PhoneStart(server = null, signedIn = false)
        compose.setContent {
            CompositionLocalProvider(LocalOnboardingSource provides FakeOnboardingSource()) {
                PhoneApp(start, awaitStart = { start }, graph = { graph })
            }
        }
        compose.onNodeWithTag(TestTags.PHONE_ROOT).assert(SemanticsMatcher.expectValue(SemanticsPropertiesAndroid.TestTagsAsResourceId, true))
    }

    @Test
    fun withoutAServerItStartsAtServersFullScreen() {
        show(PhoneStart(server = null, signedIn = false))
        signInPage().assertExists()
        tab(PhoneTab.Home).assertDoesNotExist()
    }

    @Test
    fun withAServerButNoTokensItStartsSigningInToIt() {
        val shell = show(PhoneStart(server = TestGraph.SERVER, signedIn = false))
        signInPage().assertExists()
        tab(PhoneTab.Home).assertDoesNotExist()
        assertEquals(listOf(PleyKey.SignIn(TestGraph.SERVER)), shell.navigation.root.toList())
    }

    @Test
    fun signedInItStartsAtHomeWithTheTabs() {
        show(PhoneStart(server = TestGraph.SERVER, signedIn = true))
        showsHome()
        tab(PhoneTab.Home).assertIsSelected()
        tab(PhoneTab.Me).assertExists()
    }

    @Test
    fun nothingIsAskedOfSignInOrPlaybackBeforeTheFirstFrame() {
        val graph = TestGraph()
        TestApplication.graph = graph
        compose.mainClock.autoAdvance = false
        compose.setContent {
            PhoneTheme { PhoneShell(PhoneStart(TestGraph.SERVER, signedIn = true), { graph }) }
        }
        // Composed and laid out, but no frame has gone by: only Home's own data (what was saved) is read.
        assertEquals(emptyList<String>(), graph.touched.filter { it != "data" })
        compose.mainClock.advanceTimeByFrame()
        compose.mainClock.advanceTimeByFrame()
        compose.waitUntil(5_000) { graph.fakeCaps.warmUps == 1 && "auth" in graph.touched && "data" in graph.touched }
        compose.mainClock.autoAdvance = true
    }

    @Test
    fun eachTabKeepsItsBackStackAcrossTabsAndRecreation() {
        val graph = TestGraph()
        TestApplication.graph = graph
        val restore = StateRestorationTester(compose)
        lateinit var navigation: PhoneNavigation
        restore.setContent {
            PhoneTheme { PhoneShell(PhoneStart(TestGraph.SERVER, signedIn = true), { graph }) { _, it -> navigation = it } }
        }
        tab(PhoneTab.Libraries).performClick()
        showsLibraries()
        compose.runOnIdle { navigation.open(PleyKey.Item(101)) }
        itemPage().assertExists()

        tab(PhoneTab.Search).performClick()
        shows("Search your libraries")
        tab(PhoneTab.Libraries).performClick()
        itemPage().assertExists()

        tab(PhoneTab.Search).performClick()
        restore.emulateSavedInstanceStateRestore()
        shows("Search your libraries")
        tab(PhoneTab.Search).assertIsSelected()
        tab(PhoneTab.Libraries).performClick()
        itemPage().assertExists()
        // Choosing the tab shown takes it back to its first screen.
        tab(PhoneTab.Libraries).performClick()
        showsLibraries()
    }

    @Test
    fun backFromAnotherTabsFirstScreenGoesHome() {
        val shell = show(PhoneStart(TestGraph.SERVER, signedIn = true))
        tab(PhoneTab.Playlists).performClick()
        compose.runOnIdle { shell.navigation.back() }
        showsHome()
        tab(PhoneTab.Home).assertIsSelected()
    }

    @Test
    fun aMissingScopeOffersToSignInAgainKeepingTheServer() {
        val shell = show(PhoneStart(TestGraph.SERVER, signedIn = true))
        compose.runOnIdle { shell.state.report(ApiException(403, "insufficient_scope", "not allowed", scope = "play")) }
        compose.onNodeWithText("Pley wasn't allowed to do that: sign in again to allow it").assertExists()
        compose.onNodeWithText("Sign in").performClick()
        signInPage().assertExists()
        tab(PhoneTab.Home).assertDoesNotExist()
        assertEquals(listOf(PleyKey.SignIn(TestGraph.SERVER)), shell.navigation.root.toList())
    }

    @Test
    fun signedOutByTheServerItGoesBackToSigningInThere() {
        val shell = show(PhoneStart(TestGraph.SERVER, signedIn = true))
        compose.waitUntil(5_000) { "auth" in shell.graph.touched }
        compose.runOnIdle { shell.graph.fakeAuth.signOut(TestGraph.SERVER, SignOutReason.ENDED) }
        compose.waitUntil(5_000) { !shell.navigation.inside }
        signInPage().assertExists()
        assertEquals(listOf(PleyKey.SignIn(TestGraph.SERVER)), shell.navigation.root.toList())
        compose.onNodeWithText("You were signed out: sign in again.").assertExists()
    }

    @Test
    fun signedOutBeforeTheShellListenedItStillGoesToSigningIn() {
        // The tokens couldn't be read as the app started: the event went before anyone listened.
        val graph = TestGraph()
        graph.sessionState.value = SessionState.SignedOut(TestGraph.SERVER)
        val shell = show(PhoneStart(TestGraph.SERVER, signedIn = true), graph)
        compose.waitUntil(5_000) { !shell.navigation.inside }
        signInPage().assertExists()
        assertEquals(listOf(PleyKey.SignIn(TestGraph.SERVER)), shell.navigation.root.toList())
    }

    @Test
    fun signedOutOfAnotherServerLeavesThisOneAlone() {
        val shell = show(PhoneStart(TestGraph.SERVER, signedIn = true))
        compose.runOnIdle { shell.graph.sessionState.value = SessionState.SignedOut("http://10.0.0.9:7539") }
        compose.waitForIdle()
        assertTrue(shell.navigation.inside)
    }

    @Test
    fun aSignOutElsewhereLeavesThisServerAlone() {
        val shell = show(PhoneStart(TestGraph.SERVER, signedIn = true))
        compose.waitUntil(5_000) { "auth" in shell.graph.touched }
        compose.runOnIdle { shell.graph.fakeAuth.signOut("http://10.0.0.9:7539") }
        compose.waitForIdle()
        assertTrue(shell.navigation.inside)
        showsHome()
    }

    @Test
    fun signingInAgainStartsTheTabsOver() {
        val shell = show(PhoneStart(TestGraph.SERVER, signedIn = true))
        tab(PhoneTab.Libraries).performClick()
        compose.runOnIdle { shell.navigation.open(PleyKey.Item(101)) }
        compose.runOnIdle { shell.navigation.restart(PleyKey.SignIn(TestGraph.SERVER)) }
        signInPage().assertExists()
        compose.runOnIdle { shell.navigation.restart(PleyKey.Home) }
        showsHome()
        tab(PhoneTab.Libraries).performClick()
        showsLibraries()
    }

    @Test
    fun theRemotesBarShowsAboveTheTabsWhileAnotherDevicePlays() {
        val shell = show(PhoneStart(TestGraph.SERVER, signedIn = true))
        showsHome()
        compose.onNodeWithTag(REMOTE_BAR).assertDoesNotExist()
        compose.runOnIdle {
            shell.graph.remoteSessions.value = listOf(
                RemoteSession(PlayingSession("s1", PlayingKind.Live, "Living room TV", channel = PlayingChannel(1, "Pley News HD")), 0),
            )
        }
        compose.waitUntil(5_000) { compose.onAllNodesWithTag(REMOTE_BAR).fetchSemanticsNodes().isNotEmpty() }
        compose.onNodeWithText("Playing on Living room TV").assertExists()
        compose.runOnIdle { shell.graph.remoteSessions.value = emptyList() }
        compose.waitUntil(5_000) { compose.onAllNodesWithTag(REMOTE_BAR).fetchSemanticsNodes().isEmpty() }
    }

    private companion object {
        const val SHOWN_WITHIN_MS = 5_000L
    }
}
