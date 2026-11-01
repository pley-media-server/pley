package app.pley.android.ui.phone.search

import androidx.compose.foundation.text.input.TextFieldState
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.test.assertIsFocused
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import app.pley.android.designsystem.tile.ArtworkTileModel
import app.pley.android.model.LiveChannel
import app.pley.android.model.DiscoverPage
import app.pley.android.model.TitleKind
import app.pley.android.screens.Navigator
import app.pley.android.screens.PleyKey
import app.pley.android.screens.search.SearchGroup
import app.pley.android.screens.search.SearchGroupKind
import app.pley.android.screens.search.SearchResultsState
import app.pley.android.screens.search.SearchUiState
import app.pley.android.screens.search.TmdbSection
import app.pley.android.testing.Fixtures
import app.pley.android.ui.phone.shell.LocalPhoneNavigator
import app.pley.android.ui.phone.theme.PhoneTheme
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

/**
 * The phone's search: the field up first, recent searches, the groups
 * with their counts, the empty page's words, and Discover in it: its way
 * in before typing, and what TMDB has after the libraries' results.
 */
@RunWith(RobolectricTestRunner::class)
@Config(qualifiers = "w412dp-h915dp-xxhdpi")
class PhoneSearchUiTest {
    @get:Rule
    val compose = createComposeRule()

    private val opened = mutableListOf<PleyKey>()
    private val navigator = object : Navigator {
        override fun open(key: PleyKey) {
            opened += key
        }

        override fun back() = Unit

        override fun restart(key: PleyKey) = Unit
    }

    private val chosen = mutableListOf<String>()
    private val watched = mutableListOf<LiveChannel>()
    private var cleared = 0
    private val trending: DiscoverPage = Fixtures.decode("discover/trending.json")

    private fun show(
        state: SearchUiState<ArtworkTileModel>,
        field: TextFieldState = TextFieldState(state.query),
        tmdb: TmdbSection = TmdbSection.Hidden,
        discover: Boolean = false,
    ) {
        compose.setContent {
            PhoneTheme {
                CompositionLocalProvider(LocalPhoneNavigator provides navigator) {
                    SearchContent(
                        state = state,
                        field = field,
                        onSubmit = {},
                        onRecent = { chosen += it },
                        onClearRecent = { cleared++ },
                        open = { group, id -> if (group == SearchGroupKind.People) PleyKey.Person(id) else PleyKey.Item(id) },
                        onWatch = { watched += it },
                        tmdb = tmdb,
                        discover = discover,
                    )
                }
            }
        }
        compose.waitForIdle()
    }

    private fun tile(id: Long, title: String) = ArtworkTileModel(id = id, title = title)

    @Test
    fun anEmptyFieldHasFocusAndOffersTheRecentSearches() {
        show(SearchUiState("", listOf("north", "halcyon"), SearchResultsState.Idle))
        compose.onNodeWithTag(SEARCH_FIELD).assertIsFocused()
        compose.onNodeWithText("Recent searches").assertExists()
        compose.onNodeWithText("halcyon").performClick()
        compose.onNodeWithText("Clear").performClick()
        assertEquals(listOf("halcyon"), chosen)
        assertEquals(1, cleared)
    }

    @Test
    fun withoutRecentSearchesItSaysWhatSearchFinds() {
        show(SearchUiState("", emptyList(), SearchResultsState.Idle))
        compose.onNodeWithText("Search your libraries").assertExists()
    }

    @Test
    fun resultsComeAsCountedGroupsThatOpen() {
        val groups = listOf(
            SearchGroup(SearchGroupKind.People, listOf(tile(70, "Lena Okafor")), listOf(70)),
            SearchGroup(SearchGroupKind.Movies, listOf(tile(1, "Northern Lights"), tile(2, "North Star")), listOf(1, 2)),
        )
        show(SearchUiState("north", emptyList(), SearchResultsState.Ready("north", groups, loading = false)))
        compose.onNodeWithText("Results for “north”").assertExists()
        compose.onNodeWithContentDescription("People, 1 result").assertExists()
        compose.onNodeWithContentDescription("Movies, 2 results").assertExists()
        compose.onNodeWithText("Northern Lights", useUnmergedTree = true).performClick()
        compose.onNodeWithText("Lena Okafor", useUnmergedTree = true).performClick()
        assertEquals(listOf(PleyKey.Item(1), PleyKey.Person(70)), opened)
    }

    @Test
    fun channelsFoundShowAsLiveTvAndPlay() {
        val sport = LiveChannel(id = 7, name = "North Sport", number = "101", group = "Sport")
        val groups = listOf(SearchGroup(SearchGroupKind.Movies, listOf(tile(1, "Northern Lights")), listOf(1)))
        show(SearchUiState("north", emptyList(), SearchResultsState.Ready("north", groups, loading = false, channels = listOf(sport))))
        compose.onNodeWithContentDescription("Live TV, 1 result").assertExists()
        compose.onNodeWithText("Sport", useUnmergedTree = true).assertExists()
        compose.onNodeWithTag(SEARCH_CHANNEL + 7).performClick()
        assertEquals(listOf(sport), watched)
        assertEquals(emptyList<PleyKey>(), opened)
    }

    @Test
    fun noResultsSaysSoInTheWebsWords() {
        show(SearchUiState("zzz", emptyList(), SearchResultsState.Empty("zzz")))
        compose.onNodeWithText("No results for “zzz”").assertExists()
        compose.onNodeWithText("Try a different spelling or fewer words.").assertExists()
    }

    @Test
    fun discoverHeadsThePageBeforeTypingWhereRequestsAreOpen() {
        show(SearchUiState("", listOf("north"), SearchResultsState.Idle), discover = true)
        compose.onNodeWithText("Find movies and shows to ask for").assertExists()
        compose.onNodeWithTag(SEARCH_DISCOVER).performClick()
        assertEquals(listOf<PleyKey>(PleyKey.Discover()), opened)
        // The recent searches follow it.
        compose.onNodeWithText("north").assertExists()
    }

    @Test
    fun withoutRequestsThereIsNoWayToDiscover() {
        show(SearchUiState("", emptyList(), SearchResultsState.Idle))
        compose.onNodeWithTag(SEARCH_DISCOVER).assertDoesNotExist()
    }

    @Test
    fun whatTmdbHasFollowsTheResultsAndSeeAllOpensDiscover() {
        val groups = listOf(SearchGroup(SearchGroupKind.Movies, listOf(tile(1, "Northern Lights")), listOf(1)))
        show(
            SearchUiState("robot", emptyList(), SearchResultsState.Ready("robot", groups, loading = false)),
            tmdb = TmdbSection.Ready("robot", trending.results.take(2)),
        )
        compose.onNodeWithText("On TMDB").assertExists()
        compose.onNodeWithText("The Wild Robot", useUnmergedTree = true).performClick()
        compose.onNodeWithText("See all on Discover").performClick()
        assertEquals(listOf(PleyKey.DiscoverTitle(TitleKind.Movie, 1184918), PleyKey.Discover("robot")), opened)
    }

    @Test
    fun withNothingInTheLibrariesTmdbStillAnswers() {
        show(
            SearchUiState("robot", emptyList(), SearchResultsState.Empty("robot")),
            tmdb = TmdbSection.Ready("robot", trending.results.take(1)),
        )
        compose.onNodeWithText("Nothing in your libraries matches").assertExists()
        compose.onNodeWithText("On TMDB").assertExists()
        compose.onNodeWithText("No results for “robot”").assertDoesNotExist()
    }
}
