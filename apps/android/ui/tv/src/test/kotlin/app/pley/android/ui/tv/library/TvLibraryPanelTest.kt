package app.pley.android.ui.tv.library

import android.content.Context
import androidx.compose.foundation.lazy.items
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.semantics.SemanticsActions
import androidx.compose.ui.test.assertIsFocused
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performSemanticsAction
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.navigation3.runtime.NavKey
import androidx.test.core.app.ApplicationProvider
import app.pley.android.data.Loadable
import app.pley.android.model.DynamicRangeFilter
import app.pley.android.model.Library
import app.pley.android.model.LibraryFilters
import app.pley.android.model.LibraryKind
import app.pley.android.model.ResolutionFilter
import app.pley.android.screens.LibraryTab
import app.pley.android.screens.PleyKey
import app.pley.android.screens.library.LibraryDeps
import app.pley.android.screens.library.LibraryViewModel
import app.pley.android.ui.tv.kit.TvSidePanel
import app.pley.android.ui.tv.shell.LocalTvNavigator
import app.pley.android.ui.tv.shell.RailState
import app.pley.android.ui.tv.shell.TvNavigator
import app.pley.android.ui.tv.shell.TvTestGraph
import app.pley.android.ui.tv.theme.TvTheme
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

/**
 * A library's side panels (design spec §2.7): the choice in force has
 * focus when the panel opens, however far down its list it is.
 */
@RunWith(RobolectricTestRunner::class)
@Config(qualifiers = "w960dp-h540dp-xhdpi")
class TvLibraryPanelTest {
    @get:Rule
    val compose = createComposeRule()

    @Test
    fun aChoiceFarDownTheListTakesFocus() {
        val genres = List(40) { "Genre ${it + 1}" }
        val chosen = "Genre 36"
        compose.setContent {
            TvTheme(reducedMotion = true) {
                val requester = remember { FocusRequester() }
                TvSidePanel(visible = true, onDismiss = {}) {
                    PanelList("Genre", requester) {
                        items(genres) { genre ->
                            PanelRow(
                                genre,
                                onClick = {},
                                checked = genre == chosen,
                                modifier = if (genre == chosen) Modifier.focusRequester(requester) else Modifier,
                            )
                        }
                    }
                }
            }
        }
        compose.waitForIdle()
        // Far past the first screen of rows, which is all a lazy list composes at first.
        compose.onNodeWithText(chosen).assertIsFocused()
    }

    @Test
    fun resolutionAndDynamicRangeAreChosenFromTheFilterPanel() {
        val graph = TvTestGraph()
        graph.sources.libraryList.value = Loadable.Ready(listOf(Library(1, "Movies", LibraryKind.Movies)), fresh = true)
        graph.filters.value = Loadable.Ready(
            LibraryFilters(resolutions = listOf("4k", "1080"), dynamicRanges = listOf("hdr", "dolby_vision", "sdr")),
            fresh = true,
        )
        val context = ApplicationProvider.getApplicationContext<Context>()
        val vm = LibraryViewModel(1, LibraryTab.Library, "resolution=1080", LibraryDeps.of(graph), TvTiles(context))
        var panel by mutableStateOf<TvPanel?>(TvPanel.Filter)
        val navigator = TvNavigator(mutableListOf<NavKey>(PleyKey.Library(1)), RailState()) {}
        compose.setContent {
            TvTheme(reducedMotion = true) {
                CompositionLocalProvider(LocalTvNavigator provides navigator) {
                    val state by vm.state.collectAsStateWithLifecycle()
                    TvLibraryPanel(panel, state, vm, onPanel = { panel = it })
                }
            }
        }
        compose.waitForIdle()
        // Rows after Year's, each with what is chosen.
        compose.onNodeWithText("1080p").assertExists()
        compose.onNodeWithText("All dynamic ranges").assertExists()
        compose.onNodeWithText("Resolution").performSemanticsAction(SemanticsActions.OnClick)
        compose.waitForIdle()

        // Its own panel: all of them, then the library's, the one chosen with the focus.
        compose.onNodeWithText("All resolutions").assertExists()
        compose.onNodeWithText("1080p").assertIsFocused()
        compose.onNodeWithText("4K").performSemanticsAction(SemanticsActions.OnClick)
        compose.waitForIdle()
        assertEquals(ResolutionFilter.Uhd, vm.state.value.params.resolution)
        assertNull("a choice closes the panel", panel)

        panel = TvPanel.DynamicRange
        compose.waitForIdle()
        compose.onNodeWithText("All dynamic ranges").assertIsFocused()
        compose.onNodeWithText("Dolby Vision").performSemanticsAction(SemanticsActions.OnClick)
        compose.waitForIdle()
        assertEquals(DynamicRangeFilter.DolbyVision, vm.state.value.params.dynamicRange)
    }
}
