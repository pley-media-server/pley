package app.pley.android.ui.tv.shell

import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.semantics.SemanticsPropertiesAndroid
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.assert
import androidx.compose.ui.test.assertContentDescriptionEquals
import androidx.compose.ui.test.assertIsFocused
import androidx.compose.ui.test.hasAnyAncestor
import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.isFocused
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.navigation3.runtime.NavKey
import androidx.test.core.app.ApplicationProvider
import app.pley.android.common.TestTags
import app.pley.android.data.Loadable
import app.pley.android.data.RequestNotice
import app.pley.android.model.Library
import app.pley.android.model.LibraryKind
import app.pley.android.model.Me
import app.pley.android.model.TitleKind
import app.pley.android.screens.onboarding.ProfileGate
import app.pley.android.screens.requests.RequestsEntry
import app.pley.android.testing.screens.onboarding.FakeOnboardingSource
import app.pley.android.ui.tv.item.TV_ITEM_PAGE
import app.pley.android.ui.tv.onboarding.LocalOnboardingSource
import app.pley.android.ui.tv.kit.focused
import app.pley.android.ui.tv.kit.press
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.update
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

/**
 * The frame with the D-pad and Back (D-pad rule 11): initial focus on every
 * start screen, the rail and content, the Back sequence, and a row's focus
 * coming back after Back. Home and the item page are the real ones, over
 * the JSON fixtures ([TvTestGraph]; an item's details stay loading); the
 * screens not built yet are placeholders.
 */
@RunWith(RobolectricTestRunner::class)
@Config(qualifiers = "w960dp-h540dp-land-television-xhdpi", application = TvTestApplication::class)
class TvAppTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    private var exits = 0

    private class FakeSource(
        private val facts: StartFacts,
        libraries: List<Library>,
        override val requests: Flow<RequestsEntry>,
        private val queue: MutableStateFlow<List<TvNotice>>,
    ) : TvShellSource {
        override suspend fun startFacts() = facts

        override val libraries: Flow<List<Library>> = MutableStateFlow(libraries)

        override val profile: Flow<RailProfile?> = MutableStateFlow(RailProfile("Anna"))

        override suspend fun keepUpdated() = Unit

        override val notices: Flow<List<TvNotice>> = queue

        override fun dismiss(notice: RequestNotice) = queue.update { it.filterNot { n -> n.notice.key == notice.key } }
    }

    /** The way into requests the rail shows, and the news to pop up. */
    private val requests = MutableStateFlow(RequestsEntry.None)
    private val notices = MutableStateFlow<List<TvNotice>>(emptyList())

    private val onboarding = FakeOnboardingSource().apply {
        active.value = "http://s"
        profiles = FakeOnboardingSource.household()
    }

    private val home = StartFacts(server = "http://192.168.1.20:7539", signedIn = true)
    private val threeLibraries = listOf(Library(1, "Movies", LibraryKind.Movies), Library(2, "Shows", LibraryKind.Shows), Library(3, "Music", LibraryKind.Music))

    private fun app(facts: StartFacts = home, placeholder: @androidx.compose.runtime.Composable (NavKey) -> Unit = { Placeholder(it) }) {
        start(facts, placeholder)
        // Every screen starts with focus (D-pad rule 1): Home once its rows are shaped, off the
        // main thread, which waiting for the main thread to be idle doesn't wait for.
        compose.waitUntil(STARTS_WITHIN_MS) { compose.onAllNodes(isFocused()).fetchSemanticsNodes().isNotEmpty() }
    }

    /** The app, without waiting for its first screen. */
    private fun start(facts: StartFacts = home, placeholder: @androidx.compose.runtime.Composable (NavKey) -> Unit = { Placeholder(it) }) {
        compose.setContent {
            // Getting in reads a fake server: a household of four, a code to show, nobody on the network.
            CompositionLocalProvider(LocalOnboardingSource provides onboarding) {
                TvApp(FakeSource(facts, threeLibraries, requests, notices), onExit = { exits++ }, reducedMotion = true, placeholder = placeholder)
            }
        }
    }

    /** Home's data in the test graph, which a test can hold back. */
    private val homeData get() = (ApplicationProvider.getApplicationContext<TvTestApplication>().screensGraph as TvTestGraph).sources.home

    /** Waits for [text], which a screen shows once its state is worked out off the main thread. */
    private fun waitFor(text: String, ofContentDescription: Boolean = false) {
        val matcher = if (ofContentDescription) hasContentDescription(text) else hasText(text)
        compose.waitUntil(STARTS_WITHIN_MS) { compose.onAllNodes(matcher).fetchSemanticsNodes().isNotEmpty() }
        compose.waitForIdle()
    }

    private fun back() {
        compose.runOnUiThread { compose.activity.onBackPressedDispatcher.onBackPressed() }
        compose.waitForIdle()
    }

    private fun railEntry(id: String) = compose.onNodeWithTag("${TestTags.TV_RAIL}.$id")

    @Test
    fun serversStartsFocused() {
        app(StartFacts(null, false))
        compose.onNodeWithText("Choose your server").assertExists()
        compose.onNodeWithText("Enter an address").assertIsFocused()
    }

    @Test
    fun linkStartsFocused() {
        app(StartFacts("http://s", false))
        compose.onNodeWithText("Sign in to Pley").assertExists()
        compose.onNodeWithText("Change server").assertIsFocused()
    }

    @Test
    fun whoIsWatchingStartsFocused() {
        app(StartFacts("http://s", true, profiles = 2))
        compose.onNodeWithText("Who's watching?").assertExists()
        // The profile in use, so one OK continues.
        compose.onNodeWithTag("pley.profiles.profile.1").assertIsFocused()
    }

    @Test
    fun alwaysStartAsOpensOnThePadOfAProfileWithAPin() {
        ProfileGate.passed = false
        onboarding.signIn("http://s", Me(1, "anna", "Anna", hasPin = true))
        app(StartFacts("http://s", true, profiles = 2, alwaysStartAs = 1, hasPin = true))
        waitFor("Enter Anna's PIN")
        back()
        compose.onNodeWithText("Who's watching?").assertExists()
    }

    @Test
    fun homeStartsFocused() {
        app()
        compose.onNodeWithText("Resume").assertIsFocused()
    }

    @Test
    fun itsTagsAreResourceIdsForTheBenchmarks() {
        app()
        compose.onNodeWithTag(TestTags.TV_ROOT).assert(SemanticsMatcher.expectValue(SemanticsPropertiesAndroid.TestTagsAsResourceId, true))
    }

    @Test
    fun leftFromContentEntersTheRailAtTheCurrentPage() {
        app()
        compose.onNodeWithText("Resume").assertIsFocused()
        compose.press(Key.DirectionLeft)
        railEntry("home").assertIsFocused()
        // Right goes back to where focus was.
        compose.press(Key.DirectionRight)
        compose.onNodeWithText("Resume").assertIsFocused()
    }

    @Test
    fun theRailOpensItsPagesAndFocusFollows() {
        app()
        compose.press(Key.DirectionLeft)
        compose.press(Key.DirectionDown)
        railEntry("library:1").assertIsFocused()
        compose.press(Key.DirectionDown, 3)
        railEntry("playlists").assertIsFocused()
        compose.press(Key.DirectionCenter)
        // The playlists page, worked out off the main thread: none yet, so New playlist has focus.
        waitFor("No playlists yet")
        compose.onNodeWithText("New playlist").assertIsFocused()
    }

    @Test
    fun backGoesFromThePageToTheRailThenLeaves() {
        app()
        // A page opened from Home: Back returns to Home, on the tile it was opened from.
        compose.press(Key.DirectionDown)
        compose.press(Key.DirectionCenter)
        compose.onNodeWithTag(TV_ITEM_PAGE).assertExists()
        back()
        compose.focused().assertContentDescriptionEquals(HALF_LOOP)
        assertEquals(0, exits)
        // On the rail's page: Back goes to the rail's current entry.
        back()
        railEntry("home").assertIsFocused()
        assertEquals(0, exits)
        // On the rail: Back leaves the app.
        back()
        assertEquals(1, exits)
    }

    @Test
    fun aPageLoadingLeavesTheRailClosed() {
        app()
        compose.press(Key.DirectionDown)
        compose.press(Key.DirectionCenter)
        // The item's details are still on their way: nothing on its page to focus yet, and focus
        // isn't handed to the rail meanwhile (which would open it, and make Back leave the app).
        compose.onNodeWithTag(TV_ITEM_PAGE).assertExists()
        compose.onNode(isFocused() and hasAnyAncestor(hasTestTag(TestTags.TV_RAIL))).assertDoesNotExist()
        back()
        compose.focused().assertContentDescriptionEquals(HALF_LOOP)
        assertEquals(0, exits)
        // Left with nothing focused goes to the rail's current entry, as from the page.
        compose.press(Key.DirectionDown)
        compose.press(Key.DirectionCenter)
        compose.press(Key.DirectionLeft)
        railEntry("home").assertIsFocused()
    }

    @Test
    fun leftWhileHomeLoadsStaysOnTheRailOnceItHasLoaded() {
        val ready = homeData.value
        homeData.value = Loadable.Loading
        start()
        waitFor("Loading your home screen", ofContentDescription = true)
        // The skeleton has nothing to focus: Left goes to the rail.
        compose.press(Key.DirectionLeft)
        railEntry("home").assertIsFocused()
        // Home comes in with Resume, its initial focus, which leaves focus where it was sent.
        homeData.value = ready
        waitFor("Resume")
        railEntry("home").assertIsFocused()
        // Right goes to the page, starting on Resume.
        compose.press(Key.DirectionRight)
        compose.onNodeWithText("Resume").assertIsFocused()
    }

    @Test
    fun theSignInScreensHaveNoRail() {
        app(StartFacts(null, false))
        compose.onNode(hasTestTag(TestTags.TV_RAIL)).assertDoesNotExist()
        compose.press(Key.DirectionLeft)
        compose.onNodeWithText("Enter an address").assertIsFocused()
        back()
        assertEquals(1, exits)
    }

    @Test
    fun rightFromTheRailComesBackToTheTile() {
        app()
        compose.press(Key.DirectionDown, times = 2)
        compose.focused().assertContentDescriptionEquals(DUNE)
        compose.press(Key.DirectionLeft)
        railEntry("home").assertIsFocused()
        compose.press(Key.DirectionRight)
        compose.focused().assertContentDescriptionEquals(DUNE)
    }

    @Test
    fun aRowsFocusComesBackAfterBack() {
        app()
        compose.press(Key.DirectionDown, times = 2)
        compose.focused().assertContentDescriptionEquals(DUNE)
        compose.press(Key.DirectionCenter)
        compose.onNodeWithTag(TV_ITEM_PAGE).assertExists()
        back()
        compose.focused().assertContentDescriptionEquals(DUNE)
    }

    @Test
    fun theRailLeadsToDiscoverAfterHomeWithItsNumber() {
        requests.value = RequestsEntry(discover = true, requests = true, badge = 2)
        app()
        compose.press(Key.DirectionLeft)
        compose.press(Key.DirectionDown)
        railEntry("discover").assertIsFocused()
        railEntry("discover").assert(hasText("Discover")).assert(hasContentDescription("2 of your requests have news"))
    }

    @Test
    fun aManagersRailLeadsToTheRequestsWithWhatWaitsForThem() {
        requests.value = RequestsEntry(requests = true, manage = true, badge = 3, failed = 1)
        app()
        compose.press(Key.DirectionLeft)
        compose.press(Key.DirectionDown)
        railEntry("requests").assertIsFocused()
        railEntry("requests").assert(hasText("Requests")).assert(hasContentDescription("3 requests need you, 1 couldn't be sent"))
    }

    @Test
    fun newsOfARequestPopsUpOverThePageAndBackGivesFocusBack() {
        app()
        compose.onNodeWithText("Resume").assertIsFocused()
        notices.value = listOf(arrivalIsReady)
        waitFor("“Arrival” is ready to watch.")
        compose.onNodeWithText("Open").assertIsFocused()
        back()
        compose.onNodeWithTag(TV_REQUEST_NOTICE).assertDoesNotExist()
        compose.onNodeWithText("Resume").assertIsFocused()
        assertEquals(0, exits)
    }

    @Test
    fun aNoticesOpenOpensTheTitlesPage() {
        app()
        notices.value = listOf(arrivalIsReady)
        waitFor("“Arrival” is ready to watch.")
        compose.mainClock.advanceTimeBy(OK_GUARD_MS + 100)
        compose.press(Key.Enter)
        compose.onNodeWithTag(TV_ITEM_PAGE).assertExists()
    }

    private val arrivalIsReady = TvNotice(
        RequestNotice.Ready(1, requestId = 1, title = "Arrival", kind = TitleKind.Movie, itemId = 7, seasons = null, complete = true),
    )

    private companion object {
        const val STARTS_WITHIN_MS = 5_000L

        /** What the fixtures' tiles say to screen readers. */
        const val HALF_LOOP = "Half Loop, Severance, Season 1 Episode 2, 34 minutes left"
        const val DUNE = "Dune: Part Two, 2024"
    }
}
