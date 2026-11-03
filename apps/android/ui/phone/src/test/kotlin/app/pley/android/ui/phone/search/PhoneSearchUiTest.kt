package app.pley.android.ui.phone.search

import androidx.compose.foundation.text.input.TextFieldState
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.test.assertIsFocused
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.test.core.app.ApplicationProvider
import app.pley.android.designsystem.image.ArtworkImage
import app.pley.android.designsystem.image.LocalArtworkImageLoader
import app.pley.android.designsystem.tile.ArtworkTileModel
import app.pley.android.model.LiveChannel
import app.pley.android.model.DiscoverPage
import app.pley.android.model.TitleKind
import app.pley.android.screens.Navigator
import app.pley.android.screens.PleyKey
import app.pley.android.screens.requests.RequestProblem
import app.pley.android.screens.search.SearchGroup
import app.pley.android.screens.search.SearchGroupKind
import app.pley.android.screens.search.SearchResultsState
import app.pley.android.screens.search.SearchUiState
import app.pley.android.screens.search.TmdbSection
import app.pley.android.testing.Fixtures
import app.pley.android.ui.phone.shell.LocalPhoneNavigator
import app.pley.android.ui.phone.theme.PhoneTheme
import coil3.ImageLoader
import coil3.intercept.Interceptor
import coil3.request.ErrorResult
import coil3.request.ImageResult
import kotlinx.coroutines.Dispatchers
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import java.util.Collections

/**
 * The phone's search: the field up first, recent searches, the groups
 * with their counts, the empty page's words, and Discover in it: its way
 * in before typing, and what TMDB has after the libraries' results, or why
 * TMDB can't be searched.
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
    private var tmdbRetries = 0
    private val trending: DiscoverPage = Fixtures.decode("discover/trending.json")

    private fun show(
        state: SearchUiState<ArtworkTileModel>,
        field: TextFieldState = TextFieldState(state.query),
        tmdb: TmdbSection = TmdbSection.Hidden,
        discover: Boolean = false,
        pictures: ImageLoader? = null,
    ) {
        compose.setContent {
            PhoneTheme {
                CompositionLocalProvider(LocalPhoneNavigator provides navigator, LocalArtworkImageLoader provides pictures) {
                    SearchContent(
                        state = state,
                        field = field,
                        onSubmit = {},
                        onRecent = { chosen += it },
                        onClearRecent = { cleared++ },
                        open = { group, id -> if (group == SearchGroupKind.People) PleyKey.Person(id) else PleyKey.Item(id) },
                        onWatch = { watched += it },
                        tmdb = tmdb,
                        onRetryTmdb = { tmdbRetries++ },
                        discover = discover,
                    )
                }
            }
        }
        compose.waitForIdle()
    }

    private fun tile(id: Long, title: String) = ArtworkTileModel(id = id, title = title)

    private fun poster(id: Long) = ArtworkImage(listOf("http://127.0.0.1:9/api/items/$id/images/poster"))

    /** A loader that answers nothing, remembering what it was asked for. */
    private fun recording(asked: MutableSet<Any>): ImageLoader = ImageLoader.Builder(ApplicationProvider.getApplicationContext())
        .components {
            add(
                object : Interceptor {
                    override suspend fun intercept(chain: Interceptor.Chain): ImageResult {
                        asked += chain.request.data
                        return ErrorResult(null, chain.request, IllegalStateException("no pictures here"))
                    }
                },
            )
        }
        .memoryCache(null)
        .diskCache(null)
        .coroutineContext(Dispatchers.Unconfined)
        .build()

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
            SearchGroup(SearchGroupKind.Movies, listOf(tile(1, "Northern Lights"), tile(2, "North Star")), listOf(1, 2)),
            SearchGroup(SearchGroupKind.People, listOf(tile(70, "Lena Okafor")), listOf(70)),
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
    fun aGroupsRowAsksForThePicturesPastTheScreenBeforeTheyScrollIn() {
        val asked = Collections.synchronizedSet(mutableSetOf<Any>())
        val movies = (1L..20L).map { tile(it, "Movie $it").copy(image = poster(it)) }
        val groups = listOf(SearchGroup(SearchGroupKind.Movies, movies, movies.map { it.id }))
        show(SearchUiState("movie", emptyList(), SearchResultsState.Ready("movie", groups, loading = false)), pictures = recording(asked))
        // Four posters show on a 412 dp phone, and about a screen more is composed: the tenth is
        // only asked for ahead, as Home's rows ask; the last, far past the next screen and a half, isn't.
        assertTrue("asked for ${asked.size} pictures", poster(10) in asked)
        assertFalse(poster(20) in asked)
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
        assertEquals(listOf<PleyKey>(PleyKey.Discover), opened)
        // The recent searches follow it.
        compose.onNodeWithText("north").assertExists()
    }

    @Test
    fun withoutRequestsThereIsNoWayToDiscover() {
        show(SearchUiState("", emptyList(), SearchResultsState.Idle))
        compose.onNodeWithTag(SEARCH_DISCOVER).assertDoesNotExist()
    }

    @Test
    fun whatTmdbHasFollowsTheResultsAndSeeAllOpensEveryTitleItFinds() {
        val groups = listOf(SearchGroup(SearchGroupKind.Movies, listOf(tile(1, "Northern Lights")), listOf(1)))
        show(
            SearchUiState("robot", emptyList(), SearchResultsState.Ready("robot", groups, loading = false)),
            tmdb = TmdbSection.Ready("robot", trending.results.take(2)),
        )
        compose.onNodeWithText("Discover").assertExists()
        compose.onNodeWithText("The Wild Robot", useUnmergedTree = true).performClick()
        compose.onNodeWithText("See all").performClick()
        assertEquals(listOf(PleyKey.DiscoverTitle(TitleKind.Movie, 1184918), PleyKey.DiscoverResults("robot")), opened)
    }

    @Test
    fun whenTmdbCantBeSearchedOnTmdbSaysWhyWithRetry() {
        show(
            SearchUiState("robot", emptyList(), SearchResultsState.Empty("robot")),
            tmdb = TmdbSection.Failed("robot", RequestProblem.TmdbDown),
        )
        compose.onNodeWithText("Nothing in your libraries matches").assertExists()
        compose.onNodeWithText("TMDB isn't answering").assertExists()
        compose.onNodeWithText("See all").assertDoesNotExist()
        compose.onNodeWithText("Retry").performClick()
        assertEquals(1, tmdbRetries)
    }

    @Test
    fun aKeyTmdbRefusedHasNoRetry() {
        // An administrator fixes it.
        show(
            SearchUiState("robot", emptyList(), SearchResultsState.Ready("robot", emptyList(), loading = false)),
            tmdb = TmdbSection.Failed("robot", RequestProblem.TmdbKey),
        )
        compose.onNodeWithText("TMDB refused this server's key").assertExists()
        compose.onNodeWithText("Retry").assertDoesNotExist()
    }

    @Test
    fun withNothingInTheLibrariesTmdbStillAnswers() {
        show(
            SearchUiState("robot", emptyList(), SearchResultsState.Empty("robot")),
            tmdb = TmdbSection.Ready("robot", trending.results.take(1)),
        )
        compose.onNodeWithText("Nothing in your libraries matches").assertExists()
        compose.onNodeWithText("Discover").assertExists()
        compose.onNodeWithText("No results for “robot”").assertDoesNotExist()
    }
}
