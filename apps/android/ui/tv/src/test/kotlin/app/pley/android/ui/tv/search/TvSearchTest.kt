package app.pley.android.ui.tv.search

import androidx.activity.ComponentActivity
import androidx.compose.foundation.text.input.TextFieldState
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.test.assert
import androidx.compose.ui.test.assertIsFocused
import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import app.pley.android.common.Tier
import app.pley.android.designsystem.tile.ArtworkTileModel
import app.pley.android.model.DiscoverPage
import app.pley.android.model.DiscoverTitle
import app.pley.android.model.LiveChannel
import app.pley.android.screens.requests.RequestProblem
import app.pley.android.screens.search.SearchGroup
import app.pley.android.screens.search.SearchGroupKind
import app.pley.android.screens.search.SearchResultsState
import app.pley.android.screens.search.SearchUiState
import app.pley.android.screens.search.TmdbSection
import app.pley.android.testing.Fixtures
import app.pley.android.ui.tv.kit.focused
import app.pley.android.ui.tv.kit.press
import app.pley.android.ui.tv.theme.TvTheme
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

/**
 * Search with the remote (design spec §4.5, D-pad rules 1 and 7): focus
 * starts on the field; Down goes into the results' rows and Up back to
 * the field; recent searches are chips under it. After the results, for
 * a profile that may ask, Discover's row of what TMDB has (packages/tv's
 * requests.test.tsx, "search").
 */
@RunWith(RobolectricTestRunner::class)
@Config(qualifiers = "w960dp-h540dp-xhdpi")
class TvSearchTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    private val opened = mutableListOf<Pair<SearchGroupKind, Long>>()
    private val chosen = mutableListOf<String>()
    private val watched = mutableListOf<Long>()
    private val tmdbOpened = mutableListOf<DiscoverTitle>()
    private val allOf = mutableListOf<String>()
    private var tmdbRetries = 0

    private fun show(state: SearchUiState<ArtworkTileModel>, tmdb: TmdbSection = TmdbSection.Hidden) {
        compose.setContent {
            TvTheme(Tier.T2, reducedMotion = true) {
                TvSearchContent(
                    state = state,
                    field = TextFieldState(state.query),
                    onSearch = {},
                    onRecent = { chosen += it },
                    onClearRecent = {},
                    onOpen = { group, id -> opened += group to id },
                    onActions = {},
                    onWatch = { watched += it.id },
                    tmdb = tmdb,
                    onTmdbRetry = { tmdbRetries++ },
                    onTmdbOpen = { tmdbOpened += it },
                    onTmdbAll = { allOf += it },
                )
            }
        }
        compose.waitForIdle()
    }

    private fun tile(id: Long, title: String) = ArtworkTileModel(id = id, title = title, description = title)

    @Test
    fun focusStartsOnTheFieldAndDownGoesIntoTheResults() {
        val groups = listOf(
            SearchGroup(SearchGroupKind.Movies, listOf(tile(1, "Northern Lights"), tile(2, "North Star")), listOf(1, 2)),
            SearchGroup(SearchGroupKind.Episodes, listOf(tile(3, "The North Wind")), listOf(3)),
        )
        show(SearchUiState("north", emptyList(), SearchResultsState.Ready("north", groups, loading = false)))
        compose.onNodeWithTag(TV_SEARCH_FIELD).assertIsFocused()
        compose.onNodeWithText("Movies").assertExists()
        compose.onNodeWithText("Episodes").assertExists()
        compose.press(Key.DirectionDown)
        compose.focused().assert(hasContentDescription("Northern Lights"))
        compose.press(Key.DirectionRight)
        compose.press(Key.Enter)
        assertEquals(listOf(SearchGroupKind.Movies to 2L), opened)
        compose.press(Key.DirectionLeft)
        compose.press(Key.DirectionUp)
        compose.onNodeWithTag(TV_SEARCH_FIELD).assertIsFocused()
    }

    @Test
    fun channelsFoundAreARowAfterPeopleThatPlays() {
        val groups = listOf(
            SearchGroup(SearchGroupKind.People, listOf(tile(70, "Lena North")), listOf(70)),
            SearchGroup(SearchGroupKind.Movies, listOf(tile(1, "Northern Lights")), listOf(1)),
        )
        val channels = listOf(LiveChannel(id = 7, name = "North Sport", number = "101"), LiveChannel(id = 8, name = "North News"))
        show(SearchUiState("north", emptyList(), SearchResultsState.Ready("north", groups, loading = false, channels = channels)))
        compose.onNodeWithText("Live TV").assertExists()
        compose.onNodeWithTag(TV_SEARCH_CHANNEL + 7).assertExists()
        compose.press(Key.DirectionDown)
        compose.focused().assert(hasContentDescription("Lena North"))
        compose.press(Key.DirectionDown)
        compose.press(Key.DirectionRight)
        compose.press(Key.Enter)
        assertEquals(listOf(8L), watched)
        compose.press(Key.DirectionDown)
        compose.focused().assert(hasContentDescription("Northern Lights"))
    }

    @Test
    fun recentSearchesAreChipsUnderTheField() {
        show(SearchUiState("", listOf("north", "halcyon"), SearchResultsState.Idle))
        compose.onNodeWithText("Recent searches").assertExists()
        compose.press(Key.DirectionDown)
        compose.focused().assert(hasText("north"))
        compose.press(Key.DirectionRight)
        compose.press(Key.Enter)
        assertEquals(listOf("halcyon"), chosen)
        // The chips go once results come: focus waits on the field.
        compose.onNodeWithTag(TV_SEARCH_FIELD).assertIsFocused()
    }

    /** TMDB's trending titles, as its search finds them for [query]. */
    private fun tmdbFound(query: String) =
        TmdbSection.Ready(query, Fixtures.decode<DiscoverPage>("discover/trending.json").results, origin = "http://127.0.0.1:1")

    @Test
    fun discoverComesAfterTheLibrariesResultsWithSeeAll() {
        val groups = listOf(SearchGroup(SearchGroupKind.Movies, listOf(tile(1, "Northern Lights")), listOf(1)))
        val tmdb = tmdbFound("north")
        show(SearchUiState("north", emptyList(), SearchResultsState.Ready("north", groups, loading = false)), tmdb)
        compose.onNodeWithText("Discover").assertExists()
        compose.press(Key.DirectionDown)
        compose.focused().assert(hasContentDescription("Northern Lights"))
        compose.press(Key.DirectionDown)
        compose.focused().assert(hasContentDescription("The Wild Robot", substring = true))
        compose.press(Key.Enter)
        assertEquals(listOf(tmdb.titles.first()), tmdbOpened)
        // See all, after the titles.
        compose.press(Key.DirectionRight, tmdb.titles.size)
        compose.press(Key.Enter)
        assertEquals(listOf("north"), allOf)
    }

    @Test
    fun withNothingInTheLibrariesTheLineSaysSoAboveDiscover() {
        show(SearchUiState("robot", emptyList(), SearchResultsState.Empty("robot")), tmdbFound("robot"))
        compose.onNodeWithText("Nothing in your libraries matches").assertExists()
        compose.onNodeWithText("No results for “robot”").assertDoesNotExist()
        compose.press(Key.DirectionDown)
        compose.focused().assert(hasContentDescription("The Wild Robot", substring = true))
        compose.press(Key.DirectionUp)
        compose.onNodeWithTag(TV_SEARCH_FIELD).assertIsFocused()
    }

    @Test
    fun whenTmdbDoesntAnswerTheRowSaysSoWithRetry() {
        show(SearchUiState("robot", emptyList(), SearchResultsState.Empty("robot")), TmdbSection.Failed("robot", RequestProblem.TmdbDown))
        compose.onNodeWithText("Nothing in your libraries matches").assertExists()
        compose.onNodeWithText("TMDB isn't answering").assertExists()
        compose.press(Key.DirectionDown)
        compose.focused().assert(hasText("Retry"))
        compose.press(Key.Enter)
        assertEquals(1, tmdbRetries)
        // Asked again: focus waits on the field.
        compose.onNodeWithTag(TV_SEARCH_FIELD).assertIsFocused()
    }

    @Test
    fun whileTmdbIsAskedTheRowIsStill() {
        show(SearchUiState("robot", emptyList(), SearchResultsState.Empty("robot")), TmdbSection.Loading("robot"))
        compose.onNodeWithText("Discover").assertExists()
        // Nothing in it takes focus: Down stays on the field.
        compose.press(Key.DirectionDown)
        compose.onNodeWithTag(TV_SEARCH_FIELD).assertIsFocused()
    }

    @Test
    fun noResultsSaysSo() {
        show(SearchUiState("zzz", emptyList(), SearchResultsState.Empty("zzz")))
        compose.onNodeWithText("No results for “zzz”").assertExists()
        compose.onNodeWithText("Try a different spelling or fewer words.").assertExists()
    }
}
