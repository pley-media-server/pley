package app.pley.android.ui.phone.shell

import android.app.Application
import android.content.ComponentName
import android.content.Intent
import android.content.IntentFilter
import android.graphics.drawable.ColorDrawable
import android.view.View
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.semantics.SemanticsPropertiesAndroid
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.assert
import androidx.compose.ui.test.assertCountEquals
import androidx.compose.ui.test.assertIsSelected
import androidx.compose.ui.test.junit4.StateRestorationTester
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onAllNodesWithContentDescription
import androidx.compose.ui.test.onAllNodesWithTag
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.onRoot
import androidx.compose.ui.test.performClick
import androidx.test.core.app.ApplicationProvider
import app.pley.android.auth.SignOutReason
import app.pley.android.common.SavedServer
import app.pley.android.common.TestTags
import app.pley.android.data.OtherDevice
import app.pley.android.data.RemoteSession
import app.pley.android.data.SessionState
import app.pley.android.designsystem.theme.Appearance
import app.pley.android.designsystem.theme.PleyPalette
import app.pley.android.designsystem.theme.PleyTheme
import app.pley.android.model.CastKind
import app.pley.android.model.DeviceKind
import app.pley.android.model.PlayingChannel
import app.pley.android.model.PlayingKind
import app.pley.android.model.PlayingSession
import app.pley.android.model.RemoteDevice
import app.pley.android.network.ApiException
import app.pley.android.player.NowPlaying
import app.pley.android.screens.PleyKey
import app.pley.android.testing.screens.onboarding.FakeOnboardingSource
import app.pley.android.ui.phone.item.ITEM_SKELETON
import app.pley.android.ui.phone.onboarding.LocalOnboardingSource
import app.pley.android.ui.phone.player.CarScreenChecks
import app.pley.android.ui.phone.player.CarSized
import app.pley.android.ui.phone.remote.REMOTE_BAR
import app.pley.android.ui.phone.remote.REMOTE_BUTTON
import app.pley.android.ui.phone.theme.PhoneTheme
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
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

    /**
     * The mini player, once the music playing reaches it: the shell starts
     * listening to the music after the first frame, off the main thread,
     * which waiting for the main thread to be idle doesn't wait for.
     */
    private fun showsMiniPlayer() {
        compose.waitUntil(SHOWN_WITHIN_MS) { compose.onAllNodesWithTag(MINI_PLAYER).fetchSemanticsNodes().isNotEmpty() }
        compose.onNodeWithTag(MINI_PLAYER).assertExists()
    }

    /** An item page, still loading its details. */
    private fun itemPage() = compose.onNodeWithTag(ITEM_SKELETON)

    /** The sign-in page (Servers, and SignIn for a server the app was signed out of). */
    private fun signInPage() = compose.onNodeWithText("Sign in to Pley")

    private fun tab(tab: PhoneTab) = compose.onNodeWithTag(TAB_TAG + tab.name)

    private fun show(
        start: PhoneStart,
        graph: TestGraph = TestGraph(start.server),
        source: FakeOnboardingSource = FakeOnboardingSource(),
        car: Boolean = false,
    ): Shell {
        TestApplication.graph = graph
        val shell = Shell(graph)
        compose.setContent {
            PhoneTheme {
                // Getting in reads a fake server: nobody on the network.
                CompositionLocalProvider(LocalOnboardingSource provides source) {
                    // On a car's screen the whole app is car-sized, as PhoneApp has it.
                    CarSized(car) {
                        PhoneShell(start, { graph }) { state, navigation ->
                            shell.state = state
                            shell.navigation = navigation
                        }
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

    /** The window's color in the shell started at [start]: the page's, which ThemedWindow sets as the shell draws. */
    private fun windowColor(start: PhoneStart): Int {
        val graph = TestGraph(start.server)
        TestApplication.graph = graph
        lateinit var view: View
        compose.setContent {
            view = LocalView.current
            PhoneTheme {
                CompositionLocalProvider(LocalOnboardingSource provides FakeOnboardingSource()) { PhoneShell(start, { graph }) }
            }
        }
        compose.waitForIdle()
        return compose.runOnIdle { (view.rootView.background as ColorDrawable).color }
    }

    /** Midday picked on this phone: a theme without a glow, so the window is its plain color. */
    private fun pickMidday() {
        val context = ApplicationProvider.getApplicationContext<android.content.Context>()
        context.getSharedPreferences("appearance", android.content.Context.MODE_PRIVATE).edit().clear().commit()
        Appearance.attach(context)
        Appearance.pick(context, PleyTheme.Midday)
    }

    @After
    fun midnightAgain() {
        val context = ApplicationProvider.getApplicationContext<android.content.Context>()
        context.getSharedPreferences("appearance", android.content.Context.MODE_PRIVATE).edit().clear().commit()
        Appearance.attach(context)
    }

    @Test
    fun signingInIsMidnightWhateverThePick() {
        pickMidday()
        assertEquals(PleyPalette.Default.bg.toArgb(), windowColor(PhoneStart(TestGraph.SERVER, signedIn = false)))
        signInPage().assertExists()
    }

    @Test
    fun aStartInsideAProfileIsInItsPick() {
        pickMidday()
        assertEquals(PleyPalette.of(PleyTheme.Midday, null).bg.toArgb(), windowColor(PhoneStart(TestGraph.SERVER, signedIn = true)))
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

    /** Every tab of the rail on screen, clear of its edges, each a car's 64 dp at least and none over another. */
    private fun railFitsTheCar(screen: String) {
        show(PhoneStart(server = TestGraph.SERVER, signedIn = true), car = true)
        showsHome()
        val density = compose.density.density
        val window = compose.onRoot().fetchSemanticsNode().boundsInRoot
        val tabs = PhoneTab.entries.map { it to tab(it).fetchSemanticsNode().boundsInRoot }
        for ((tab, bounds) in tabs) {
            val inside = bounds.top >= CAR_EDGE * density && bounds.bottom <= window.bottom - CAR_EDGE * density && bounds.left >= 0f
            assertTrue("$screen: $tab at $bounds, in $window", inside)
            assertTrue("$screen: $tab is ${bounds.width / density} × ${bounds.height / density} dp", minOf(bounds.width, bounds.height) / density >= 63.5f)
        }
        for ((above, below) in tabs.zipWithNext()) {
            assertTrue("$screen: ${above.first} over ${below.first}", above.second.bottom <= below.second.top + 0.5f)
        }
        // Each in the middle of the rail, not at its start: the rail is as wide as the page beside it starts.
        val middle = compose.onNodeWithTag(TABS_PAGE).fetchSemanticsNode().boundsInRoot.left / 2f
        for ((tab, bounds) in tabs) {
            assertEquals("$screen: $tab at $bounds, the rail's middle at $middle", middle, bounds.center.x, 2f)
        }
        // And about the middle of its height (Material's rail keeps a little more room above its tabs than below).
        val between = (tabs.first().second.top + tabs.last().second.bottom) / 2f
        assertEquals("$screen: the tabs from ${tabs.first().second.top} to ${tabs.last().second.bottom}", window.height / 2f, between, 12f * density)
        // A tab low on the rail is there to be pressed.
        tab(PhoneTab.Playlists).performClick()
        tab(PhoneTab.Playlists).assertIsSelected()
    }

    @Test
    @Config(qualifiers = CarScreenChecks.SMALL)
    fun onTheSmallestCarScreenEveryTabOfTheRailIsOnScreen() = railFitsTheCar("small")

    @Test
    @Config(qualifiers = CarScreenChecks.WIDE)
    fun onAWideCarScreenEveryTabOfTheRailIsOnScreen() = railFitsTheCar("wide")

    @Test
    @Config(qualifiers = CarScreenChecks.DENSE)
    fun onADenseCarScreenEveryTabOfTheRailIsOnScreen() = railFitsTheCar("dense")

    @Test
    @Config(qualifiers = CarScreenChecks.TALL)
    fun onATallCarScreenEveryTabOfTheRailIsOnScreen() = railFitsTheCar("tall")

    @Test
    @Config(qualifiers = CarScreenChecks.TALL_SPLIT)
    fun onATallCarScreenSplitInTwoEveryTabOfTheRailIsOnScreen() = railFitsTheCar("tall-split")

    /** Every tab of the bar under a narrow car page on screen, its icon alone and named, each a car's 64 dp at least and none over another. */
    private fun barFitsTheCar(screen: String) {
        show(PhoneStart(server = TestGraph.SERVER, signedIn = true), car = true)
        showsHome()
        val density = compose.density.density
        val window = compose.onRoot().fetchSemanticsNode().boundsInRoot
        val tabs = PhoneTab.entries.map { it to tab(it).fetchSemanticsNode().boundsInRoot }
        for ((tab, bounds) in tabs) {
            // No name drawn (five of 24 sp ran into each other): the icon says it to whoever listens.
            tab(tab).assert(SemanticsMatcher.keyNotDefined(SemanticsProperties.Text))
            assertTrue("$screen: $tab at $bounds, in $window", bounds.left >= 0f && bounds.right <= window.right + 0.5f && bounds.bottom <= window.bottom + 0.5f)
            assertTrue("$screen: $tab is ${bounds.width / density} × ${bounds.height / density} dp", minOf(bounds.width, bounds.height) / density >= 63.5f)
        }
        for ((before, after) in tabs.zipWithNext()) {
            assertTrue("$screen: ${before.first} over ${after.first}", before.second.right <= after.second.left + 0.5f)
        }
        compose.onAllNodesWithContentDescription("Libraries").fetchSemanticsNodes().let { assertTrue("$screen: Libraries isn't named", it.isNotEmpty()) }
        tab(PhoneTab.Playlists).performClick()
        tab(PhoneTab.Playlists).assertIsSelected()
    }

    @Test
    @Config(qualifiers = CarScreenChecks.PORTRAIT)
    fun onAPortraitCarScreenEveryTabOfTheRailIsOnScreen() = railFitsTheCar("portrait")

    @Test
    @Config(qualifiers = CarScreenChecks.ULTRAWIDE)
    fun onAWideLowCarScreenEveryTabOfTheRailIsOnScreen() = railFitsTheCar("ultrawide")

    @Test
    @Config(qualifiers = CarScreenChecks.FULL_HD)
    fun onAFullHdCarScreenEveryTabOfTheRailIsOnScreen() = railFitsTheCar("full-hd")

    @Test
    @Config(qualifiers = CarScreenChecks.TALL_PORTRAIT)
    fun underATallPortraitCarPageTheBarShowsItsTabsIconsAloneEachNamedAndACarsSize() = barFitsTheCar("tall-portrait")

    @Test
    @Config(qualifiers = CarScreenChecks.NARROW)
    fun underANarrowCarPageTheBarShowsItsTabsIconsAloneEachNamedAndACarsSize() = barFitsTheCar("narrow")

    @Test
    fun theMiniPlayerLeavesAsNowPlayingDrawsAndIsBackAsItCloses() {
        val graph = TestGraph(TestGraph.SERVER)
        val shell = show(PhoneStart(server = TestGraph.SERVER, signedIn = true), graph)
        showsHome()
        compose.onAllNodesWithTag(MINI_PLAYER).assertCountEquals(0)
        graph.music.value = NowPlaying(503, "Song 3", "Harbour Lights", playing = true, album = "Deep Water", count = 12, index = 2, durationMs = 180_000)
        showsMiniPlayer()

        // Asked for, Now Playing isn't there yet and the bar still is: it goes once the page has drawn.
        compose.mainClock.autoAdvance = false
        compose.runOnUiThread { shell.navigation.open(PleyKey.NowPlaying) }
        assertFalse(shell.navigation.nowPlayingDrawn)
        compose.onNodeWithTag(MINI_PLAYER).assertExists()
        compose.mainClock.autoAdvance = true
        compose.waitForIdle()
        assertTrue(shell.navigation.nowPlayingDrawn)
        compose.onAllNodesWithTag(MINI_PLAYER).assertCountEquals(0)

        compose.runOnUiThread { shell.navigation.back() }
        compose.waitForIdle()
        compose.onNodeWithTag(MINI_PLAYER).assertExists()
        assertFalse(shell.navigation.nowPlayingDrawn)

        // The music stopping takes the bar away.
        graph.music.value = null
        compose.waitForIdle()
        compose.onAllNodesWithTag(MINI_PLAYER).assertCountEquals(0)
    }

    @Test
    fun backToATabWithNowPlayingFromAnotherWithItsOwnTheMiniPlayerStaysAway() {
        val graph = TestGraph(TestGraph.SERVER)
        val shell = show(PhoneStart(server = TestGraph.SERVER, signedIn = true), graph)
        showsHome()
        graph.music.value = NowPlaying(503, "Song 3", "Harbour Lights", playing = true, album = "Deep Water", count = 12, index = 2, durationMs = 180_000)
        showsMiniPlayer()

        compose.runOnUiThread { shell.navigation.open(PleyKey.NowPlaying) }
        compose.waitForIdle()
        compose.runOnUiThread { shell.navigation.select(PhoneTab.Search) }
        compose.waitForIdle()
        compose.runOnUiThread { shell.navigation.open(PleyKey.NowPlaying) }
        compose.waitForIdle()

        // Home's Now Playing comes as Search's fades out: the one leaving doesn't bring the bar back over it.
        compose.runOnUiThread { shell.navigation.select(PhoneTab.Home) }
        compose.waitForIdle()
        assertTrue(shell.navigation.nowPlayingDrawn)
        compose.onAllNodesWithTag(MINI_PLAYER).assertCountEquals(0)
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
    fun signingInAgainOpensTheServersPageThoughTheAppIsSignedInThere() {
        // A permission the sign-in didn't give (Manage the server left unticked): Continue
        // opens the page to give it, rather than going back in as the app was.
        installBrowser()
        val source = FakeOnboardingSource(saved = listOf(SavedServer(TestGraph.SERVER, "Living room NAS", id = null, lastUsed = 1))).apply {
            answer(TestGraph.SERVER)
            tokens += TestGraph.SERVER
        }
        val shell = show(PhoneStart(TestGraph.SERVER, signedIn = true), source = source)
        compose.runOnIdle { shell.state.signInAgain() }
        signInPage().assertExists()
        compose.onNodeWithText("Continue in browser").performClick()
        compose.waitUntil(5_000) { source.browserSignIns.isNotEmpty() }
        assertEquals(listOf(TestGraph.SERVER), source.browserSignIns.toList())
        assertEquals(listOf(PleyKey.SignIn(TestGraph.SERVER)), shell.navigation.root.toList())
    }

    /** A browser, for Continue in browser to open the server's page in. */
    private fun installBrowser() {
        val app = ApplicationProvider.getApplicationContext<Application>()
        val activity = ComponentName("com.example.browser", "com.example.browser.Main")
        shadowOf(app.packageManager).run {
            addActivityIfNotPresent(activity)
            addIntentFilterForActivity(
                activity,
                IntentFilter(Intent.ACTION_VIEW).apply {
                    addCategory(Intent.CATEGORY_DEFAULT)
                    addCategory(Intent.CATEGORY_BROWSABLE)
                    addDataScheme("http")
                    addDataScheme("https")
                },
            )
        }
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
    fun theRemotesButtonShowsInHomesHeaderWhileAnotherDeviceIsOn() {
        val shell = show(PhoneStart(TestGraph.SERVER, signedIn = true))
        showsHome()
        compose.onNodeWithTag(REMOTE_BUTTON).assertDoesNotExist()
        val session = RemoteSession(PlayingSession("s1", PlayingKind.Live, "Living room TV", "app-12", channel = PlayingChannel(1, "Pley News HD")), 0)
        compose.runOnIdle { shell.graph.remoteDevices.value = listOf(OtherDevice("app-12", session = session)) }
        compose.waitUntil(5_000) { compose.onAllNodesWithTag(REMOTE_BUTTON).fetchSemanticsNodes().isNotEmpty() }
        compose.onNodeWithContentDescription("Remote, 1 device playing").assertExists()
        // Playing there, not controlled from here: no bar above the tabs.
        compose.onNodeWithTag(REMOTE_BAR).assertDoesNotExist()
        // On, playing nothing: still there.
        compose.runOnIdle {
            shell.graph.remoteDevices.value = listOf(OtherDevice("app-12", RemoteDevice("tv-1", "app-12", "Living room TV", DeviceKind.Tv, listOf(CastKind.Item))))
        }
        compose.waitUntil(5_000) { compose.onAllNodesWithContentDescription("Remote, 1 device ready to play").fetchSemanticsNodes().isNotEmpty() }
        compose.onNodeWithTag(REMOTE_BUTTON).performClick()
        compose.waitUntil(5_000) { compose.onAllNodesWithText("Your devices").fetchSemanticsNodes().isNotEmpty() }
        compose.runOnIdle { shell.graph.remoteDevices.value = emptyList() }
        compose.waitUntil(5_000) { compose.onAllNodesWithTag(REMOTE_BUTTON).fetchSemanticsNodes().isEmpty() }
    }

    private companion object {
        const val SHOWN_WITHIN_MS = 5_000L

        /** The least a tab of the car's rail keeps from the screen's top and bottom, in the screen's dp. */
        const val CAR_EDGE = 8f
    }
}
