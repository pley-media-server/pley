package app.pley.android.ui.tv.requests

import androidx.activity.ComponentActivity
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.assert
import androidx.compose.ui.test.assertIsFocused
import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.navigation3.runtime.NavKey
import app.pley.android.common.Tier
import app.pley.android.model.DiscoverDetail
import app.pley.android.model.DiscoverList
import app.pley.android.model.DiscoverPage
import app.pley.android.model.NewRequest
import app.pley.android.model.RequestList
import app.pley.android.model.TitleKind
import app.pley.android.model.TitleStatus
import app.pley.android.screens.PleyKey
import app.pley.android.screens.requests.DiscoverResultsViewModel
import app.pley.android.screens.requests.DiscoverSource
import app.pley.android.screens.requests.DiscoverTitleViewModel
import app.pley.android.screens.requests.DiscoverViewModel
import app.pley.android.screens.requests.RequestsDeps
import app.pley.android.testing.Fixtures
import app.pley.android.testing.data.FakeRequests
import app.pley.android.ui.tv.kit.focused
import app.pley.android.ui.tv.kit.press
import app.pley.android.ui.tv.shell.LocalTvNavigator
import app.pley.android.ui.tv.shell.RailState
import app.pley.android.ui.tv.shell.TvNavigator
import app.pley.android.ui.tv.theme.TvTheme
import kotlinx.coroutines.flow.MutableStateFlow
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

/**
 * Discover, its grids and a title's page with the remote, as the Samsung
 * and LG apps have them (packages/tv's requests.test.tsx): focus starts
 * on the hero's action, Down goes into the rows and Back back to it; a
 * row's See all opens the whole list; a grid that read five pages without
 * a title offers Show more; a title's page asks for a movie, and a show's
 * seasons are picked in Which seasons?, opened from Request more seasons
 * or from a season's poster.
 */
@RunWith(RobolectricTestRunner::class)
@Config(qualifiers = "w960dp-h540dp-xhdpi")
class TvDiscoverUiTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    private val requests = FakeRequests()
    private val trending: DiscoverPage = Fixtures.decode("discover/trending.json")
    private val stack = mutableStateListOf<NavKey>(PleyKey.Home, PleyKey.Discover)
    private val navigator = TvNavigator(stack, RailState()) {}

    private fun deps() = RequestsDeps(requests, MutableStateFlow(SERVER), MutableStateFlow(true))

    private fun show(content: @Composable () -> Unit) {
        compose.setContent {
            TvTheme(Tier.T2, reducedMotion = true) {
                CompositionLocalProvider(LocalTvNavigator provides navigator) { content() }
            }
        }
        compose.waitForIdle()
    }

    /** Waits for [text] on the page or a tile (whose words screen readers hear), which shows once the page's answer is shaped. */
    private fun waitFor(text: String) {
        val shown = hasText(text, substring = true) or hasContentDescription(text, substring = true)
        compose.waitUntil(WAIT_MS) { compose.onAllNodes(shown).fetchSemanticsNodes().isNotEmpty() }
        compose.waitForIdle()
    }

    private fun back() {
        compose.runOnUiThread { compose.activity.onBackPressedDispatcher.onBackPressed() }
        compose.waitForIdle()
    }

    @Test
    fun discoverStartsOnTheHerosActionAndBackFromTheRowsGoesBackToIt() {
        show { TvDiscoverScreen(DiscoverViewModel(deps())) }
        waitFor("Trending this week")
        compose.onNodeWithTag(TV_DISCOVER_HERO).assertExists()
        // The hero's page was read: a movie this profile may ask for.
        compose.focused().assert(hasText("Request"))
        // The way to the requests, with the profile's news.
        compose.onNodeWithTag(TV_DISCOVER_REQUESTS).assert(hasContentDescription("Your requests, 1 of your requests has news"))
        compose.press(Key.DirectionDown)
        compose.focused().assert(hasContentDescription("The Wild Robot", substring = true))
        compose.press(Key.DirectionRight)
        compose.focused().assert(hasContentDescription("Severance", substring = true))
        back()
        compose.focused().assert(hasText("Request"))
    }

    @Test
    fun backFromARowFurtherDownGoesBackToTheHero() {
        show { TvDiscoverScreen(DiscoverViewModel(deps())) }
        waitFor("Trending this week")
        // Three rows down, the hero has scrolled out of the list.
        compose.press(Key.DirectionDown, 3)
        compose.onNodeWithTag(TV_DISCOVER_HERO).assertDoesNotExist()
        back()
        compose.onNodeWithTag(TV_DISCOVER_HERO).assertExists()
        compose.focused().assert(hasText("Request"))
    }

    @Test
    fun theHerosRequestMoreSeasonsOpensWhichSeasons() {
        // Severance: some of it in a library, more seasons to ask for.
        val partly = trending.results.first { it.tmdbId == 95396L }
        requests.pages = { _, page -> DiscoverPage(page, 1, listOf(partly)) }
        show { TvDiscoverScreen(DiscoverViewModel(deps())) }
        waitFor("Request more seasons")
        compose.focused().assert(hasText("Open"))
        compose.press(Key.DirectionRight)
        compose.focused().assert(hasText("Request more seasons"))
        compose.press(Key.Enter)
        assertEquals(PleyKey.DiscoverTitle(TitleKind.Show, 95396, pick = true), stack.last())
    }

    @Test
    fun theHerosStopFollowingLetsGoAndFocusStaysOnItsButtons() {
        val movie: DiscoverDetail = Fixtures.decode("discover/movie.json")
        val followed = Fixtures.decode<RequestList>("requests/list.json").requests.first { it.id == 47L }.copy(kind = TitleKind.Movie, tmdbId = 1184918)
        requests.api.answer("discoverTitle", movie.copy(tmdbId = 1184918, status = TitleStatus.Requested, requestId = 47, watching = true, requests = listOf(followed)))
        show { TvDiscoverScreen(DiscoverViewModel(deps())) }
        waitFor("Following")
        compose.focused().assert(hasText("Following"))
        compose.press(Key.DirectionRight)
        compose.focused().assert(hasText("Stop following") or hasContentDescription("Stop following"))
        // The server lets go; read again, the title says Tell me too.
        requests.api.answer("discoverTitle", movie.copy(tmdbId = 1184918, status = TitleStatus.Requested, requestId = null, requests = emptyList()))
        compose.press(Key.Enter)
        compose.waitUntil(WAIT_MS) { requests.api.calls("unwatchRequest").isNotEmpty() }
        assertEquals(listOf<Any?>(47L), requests.api.calls("unwatchRequest").single().args)
        compose.runOnUiThread { requests.refreshTitle(TitleKind.Movie, 1184918) }
        waitFor("Tell me too")
        compose.focused().assert(hasText("Tell me too"))
    }

    @Test
    fun aRowsSeeAllOpensTheWholeListAndATileItsTitle() {
        show { TvDiscoverScreen(DiscoverViewModel(deps())) }
        waitFor("Trending this week")
        compose.press(Key.DirectionDown)
        compose.press(Key.Enter)
        assertEquals(PleyKey.DiscoverTitle(TitleKind.Movie, 1184918), stack.last())
        stack.removeAt(stack.lastIndex)
        compose.waitForIdle()
        compose.press(Key.DirectionRight, trending.results.size)
        compose.press(Key.Enter)
        assertEquals(PleyKey.DiscoverList(DiscoverList.Trending), stack.last())
    }

    @Test
    fun withNothingToAskForDiscoverSaysSo() {
        requests.pages = { _, page -> DiscoverPage(page, 1, emptyList()) }
        show { TvDiscoverScreen(DiscoverViewModel(deps())) }
        waitFor("Nothing here right now")
        compose.onNodeWithText("TMDB's lists have nothing this profile can ask for. Search for a title instead.").assertExists()
    }

    @Test
    fun aGridThatReadFivePagesWithoutATitleOffersShowMore() {
        val asked = mutableListOf<Int>()
        // An age limit left nothing on the first seven pages.
        requests.searches = { _, page, _ ->
            asked += page
            DiscoverPage(page, 30, if (page < 8) emptyList() else trending.results.map { it.copy(tmdbId = it.tmdbId * 100 + page) })
        }
        show { TvDiscoverGridScreen(DiscoverResultsViewModel(DiscoverSource.Words("robot"), deps(), 28, DiscoverResultsViewModel.TV_FILL)) }
        waitFor("Show more")
        compose.onNodeWithText("Results for “robot”").assertExists()
        compose.onNodeWithText("Nothing yet for “robot”").assertExists()
        compose.onNodeWithTag(TV_DISCOVER_GRID_MORE).assert(focusedWithin())
        assertEquals((1..5).toList(), asked)
        compose.press(Key.Enter)
        compose.waitUntil(WAIT_MS) { asked.size >= 10 }
        waitFor("The Wild Robot")
        assertEquals((1..10).toList(), asked)
        // Still short of four rows: Show more again at the end.
        compose.onNodeWithText("Show more").assertExists()
    }

    @Test
    fun aMovieIsAskedForFromItsPage() {
        show { TvDiscoverTitleScreen(DiscoverTitleViewModel(PleyKey.DiscoverTitle(TitleKind.Movie, 1184918), deps())) }
        waitFor("Discover your true nature.")
        compose.focused().assert(hasText("Request"))
        compose.onNodeWithText("You can ask for 2 more movies.").assertExists()
        compose.press(Key.Enter)
        waitFor("Asked for “The Wild Robot”: an administrator decides.")
        assertEquals(NewRequest(TitleKind.Movie, 1184918), requests.api.calls("ask").single().args.single())
    }

    @Test
    fun requestMoreSeasonsOpensWhichSeasonsAndAsksForWhatIsTicked() {
        show { TvDiscoverTitleScreen(DiscoverTitleViewModel(PleyKey.DiscoverTitle(TitleKind.Show, 1396, pick = true), deps())) }
        waitFor("Which seasons?")
        // Opened with the page: the missing seasons ticked; focus on the first that may be asked for.
        compose.onNodeWithTag(TV_PICKER_SEASON + 3).assertIsFocused()
        // Season 4 was declined: focus passes it.
        compose.press(Key.DirectionDown)
        compose.onNodeWithTag(TV_PICKER_SEASON + 5).assertIsFocused()
        compose.press(Key.DirectionDown, 2)
        compose.onNodeWithTag(TV_PICKER_ASK).assertIsFocused()
        compose.onNodeWithText("Request 2 seasons").assertExists()
        compose.press(Key.Enter)
        compose.waitForIdle()
        assertEquals(NewRequest(TitleKind.Show, 1396, listOf(3, 5)), requests.api.calls("ask").single().args.single())
    }

    @Test
    fun aSeasonsPosterOpensWhichSeasonsAtIt() {
        show { TvDiscoverTitleScreen(DiscoverTitleViewModel(PleyKey.DiscoverTitle(TitleKind.Show, 1396), deps())) }
        waitFor("Breaking Bad")
        compose.focused().assert(hasText("Request more seasons"))
        compose.press(Key.DirectionDown)
        compose.focused().assert(hasContentDescription("Season 1", substring = true))
        compose.press(Key.DirectionRight, 2)
        compose.press(Key.Enter)
        waitFor("Which seasons?")
        compose.onNodeWithTag(TV_PICKER_SEASON + 3).assertIsFocused()
        // Back closes it, and focus is on the page again.
        back()
        compose.waitForIdle()
        compose.onNodeWithText("Which seasons?").assertDoesNotExist()
    }

    /** The node, or a node in it, has focus. */
    private fun focusedWithin() = SemanticsMatcher("has focus within") { node ->
        fun focused(n: androidx.compose.ui.semantics.SemanticsNode): Boolean =
            n.config.getOrElseNullable(androidx.compose.ui.semantics.SemanticsProperties.Focused) { null } == true || n.children.any { focused(it) }
        focused(node)
    }

    private companion object {
        /** Refused at once: the pictures stay placeholders. */
        const val SERVER = "http://127.0.0.1:1"
        const val WAIT_MS = 5_000L
    }
}
