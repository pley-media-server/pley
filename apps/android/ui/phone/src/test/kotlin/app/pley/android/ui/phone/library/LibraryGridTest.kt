package app.pley.android.ui.phone.library

import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onAllNodesWithContentDescription
import androidx.compose.ui.test.onFirst
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.test.core.app.ApplicationProvider
import app.pley.android.data.AppSettings
import app.pley.android.data.CollectionsRepository
import app.pley.android.data.HubsRepository
import app.pley.android.data.LibrariesRepository
import app.pley.android.data.LibraryItems
import app.pley.android.data.LibraryItemsRepository
import app.pley.android.data.LibraryQuery
import app.pley.android.data.Loadable
import app.pley.android.data.Settings
import app.pley.android.data.valueOrNull
import app.pley.android.designsystem.tile.rememberSkeleton
import app.pley.android.model.CollectionDetail
import app.pley.android.model.CollectionSummary
import app.pley.android.model.DynamicRange
import app.pley.android.model.Hubs
import app.pley.android.model.ItemKind
import app.pley.android.model.ItemSummary
import app.pley.android.model.Library
import app.pley.android.model.LibraryFilters
import app.pley.android.model.LibraryKind
import app.pley.android.model.MediaSummary
import app.pley.android.model.ResolutionFilter
import app.pley.android.screens.LibraryTab
import app.pley.android.screens.Navigator
import app.pley.android.screens.PleyKey
import app.pley.android.screens.library.LibraryDeps
import app.pley.android.screens.library.LibraryViewModel
import app.pley.android.ui.phone.kit.KitProbe
import app.pley.android.ui.phone.shell.LocalPhoneNavigator
import app.pley.android.ui.phone.shell.LocalPhoneShell
import app.pley.android.ui.phone.shell.PhoneShellState
import app.pley.android.ui.phone.shell.rememberPhoneNavigation
import app.pley.android.ui.phone.theme.PhoneTheme
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.flowOf
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

/** A library's grid on a phone as its listing arrives: the cards already shown stay as they are. */
@RunWith(RobolectricTestRunner::class)
@Config(qualifiers = "w412dp-h915dp-xxhdpi")
class LibraryGridTest {
    @get:Rule
    val compose = createComposeRule()

    private val movies = Library(1, "Movies", LibraryKind.Movies, itemCount = 300)
    private val all = (1L..300L).map { id -> ItemSummary(id = id, libraryId = 1, kind = ItemKind.Movie, title = "Movie $id", sortTitle = "Movie $id") }
    private val listing = MutableStateFlow<Loadable<LibraryItems>>(Loadable.Loading)
    private val filters = MutableStateFlow<Loadable<LibraryFilters>>(Loadable.Ready(LibraryFilters(), fresh = true))

    private val deps = LibraryDeps(
        libraries = object : LibrariesRepository {
            override fun libraries(): Flow<Loadable<List<Library>>> = flowOf(Loadable.Ready(listOf(movies), fresh = true))
            override suspend fun refresh() = Unit
            override suspend fun sort(ids: List<Long>) = Unit
            override suspend fun forgetOrder() = Unit
        },
        items = object : LibraryItemsRepository {
            override fun items(libraryId: Long, query: LibraryQuery): Flow<Loadable<LibraryItems>> = listing
            override fun filters(libraryId: Long): Flow<Loadable<LibraryFilters>> = filters
        },
        hubs = object : HubsRepository {
            override fun home(): Flow<Loadable<Hubs>> = flowOf(Loadable.Loading)
            override fun library(libraryId: Long): Flow<Loadable<Hubs>> = flowOf(Loadable.Ready(Hubs(), fresh = true))
            override suspend fun refresh() = Unit
        },
        collections = object : CollectionsRepository {
            override fun collections(libraryId: Long): Flow<Loadable<List<CollectionSummary>>> = flowOf(Loadable.Ready(emptyList(), fresh = true))
            override fun collection(collectionId: Long): Flow<Loadable<CollectionDetail>> = flowOf(Loadable.Loading)
        },
        settings = object : AppSettings {
            override val settings: Flow<Settings> = MutableStateFlow(Settings())
            override suspend fun update(change: (Settings) -> Settings) = Unit
        },
        origin = MutableStateFlow("http://127.0.0.1:9"),
        compute = Dispatchers.Unconfined,
    )

    private val opened = mutableListOf<PleyKey>()
    private val navigator = object : Navigator {
        override fun open(key: PleyKey) {
            opened += key
        }

        override fun back() = Unit
        override fun restart(key: PleyKey) = Unit
    }

    @After
    fun forget() {
        KitProbe.cardComposed = null
    }

    @Composable
    private fun Page(content: @Composable () -> Unit) {
        PhoneTheme {
            val navigation = rememberPhoneNavigation(PleyKey.Home) { }
            val resources = LocalResources.current
            val shell = remember { PhoneShellState(navigation, resources, null) }
            CompositionLocalProvider(LocalPhoneNavigator provides navigator, LocalPhoneShell provides shell) { content() }
        }
    }

    @Test
    fun theCardsShownStayAsTheRestOfTheListingArrives() {
        val cards = mutableListOf<Long>()
        KitProbe.cardComposed = { synchronized(cards) { cards += it } }
        // The listing's head first, as a big library's comes.
        listing.value = Loadable.Ready(LibraryItems(total = all.size, items = all.take(60)), fresh = true)
        val resources = ApplicationProvider.getApplicationContext<android.content.Context>().resources
        val vm = LibraryViewModel(movies.id, LibraryTab.Library, null, deps, PhoneTiles(resources))
        compose.setContent {
            Page {
                val state by vm.state.collectAsStateWithLifecycle()
                LibraryPane(state, vm, rememberSkeleton(animated = false))
            }
        }
        compose.waitForIdle()
        val before = synchronized(cards) { cards.size }
        assertTrue(before > 0)

        // The rest arrives: the same items, and many more past the screen.
        listing.value = Loadable.Ready(LibraryItems(total = all.size, items = all), fresh = true)
        compose.waitForIdle()
        assertEquals("cards composed again: ${synchronized(cards) { cards.drop(before) }}", before, synchronized(cards) { cards.size })

        // A card still opens its page, read from the page as it is now.
        compose.onAllNodesWithContentDescription("Movie 1", substring = true).onFirst().performClick()
        assertEquals(PleyKey.Item(1), opened.last())
    }

    @Test
    fun postersCarryQualityBadgesOnlyWhileFilteredByWhatTheyShow() {
        val uhd = MediaSummary(bitrate = 58_000_000, width = 3840, height = 2160, videoCodec = "hevc", dynamicRange = DynamicRange.DolbyVision)
        listing.value = Loadable.Ready(LibraryItems(total = 3, items = all.take(3).map { it.copy(media = uhd) }), fresh = true)
        filters.value = Loadable.Ready(LibraryFilters(resolutions = listOf("4k", "1080"), dynamicRanges = listOf("dolby_vision", "sdr")), fresh = true)
        val resources = ApplicationProvider.getApplicationContext<android.content.Context>().resources
        val vm = LibraryViewModel(movies.id, LibraryTab.Library, null, deps, PhoneTiles(resources))
        compose.setContent {
            Page {
                val state by vm.state.collectAsStateWithLifecycle()
                LibraryPane(state, vm, rememberSkeleton(animated = false))
            }
        }
        compose.waitForIdle()
        fun badges() = vm.state.value.grid.cells.valueOrNull!![0]!!.badges
        // By title, the posters say nothing of their files, though the listing has them.
        assertEquals(emptyList<String>(), badges())
        compose.onNodeWithText("Resolution").assertExists()

        compose.runOnIdle { vm.resolution(ResolutionFilter.Uhd) }
        compose.waitForIdle()
        assertEquals(listOf("4K", "DV"), badges())
        // The chip names what is chosen, and the listing is filtered.
        compose.onNodeWithText("4K").assertExists()
        compose.onNodeWithText("Resolution").assertDoesNotExist()
        compose.runOnIdle { vm.clearFilters() }
        compose.waitForIdle()
        assertEquals(emptyList<String>(), badges())
        compose.onNodeWithText("Resolution").assertExists()
    }
}
