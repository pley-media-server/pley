package app.pley.android.ui.phone.library

import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.hasScrollToIndexAction
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performScrollToIndex
import androidx.test.core.app.ApplicationProvider
import app.pley.android.data.Loadable
import app.pley.android.designsystem.tile.rememberSkeleton
import app.pley.android.model.CategoryRef
import app.pley.android.model.CollectionCategory
import app.pley.android.model.CollectionSummary
import app.pley.android.model.LibraryKind
import app.pley.android.screens.Navigator
import app.pley.android.screens.PleyKey
import app.pley.android.screens.collection.CategoryChoice
import app.pley.android.screens.collection.CollectionsView
import app.pley.android.screens.collection.tile
import app.pley.android.ui.phone.shell.LocalPhoneNavigator
import app.pley.android.ui.phone.shell.LocalPhoneShell
import app.pley.android.ui.phone.shell.PhoneShellState
import app.pley.android.ui.phone.shell.rememberPhoneNavigation
import app.pley.android.ui.phone.theme.PhoneTheme
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

/** A library's Collections tab on a phone, by category. */
@RunWith(RobolectricTestRunner::class)
@Config(qualifiers = "w412dp-h915dp-xxhdpi")
class CollectionsPaneTest {
    @get:Rule
    val compose = createComposeRule()

    private val navigator = object : Navigator {
        override fun open(key: PleyKey) = Unit
        override fun back() = Unit
        override fun restart(key: PleyKey) = Unit
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

    private fun top(title: String): Float = compose.onNodeWithText(title, useUnmergedTree = true).fetchSemanticsNode().boundsInRoot.top

    @Test
    fun anotherCategoryStartsAtTheTop() {
        val resources = ApplicationProvider.getApplicationContext<android.content.Context>().resources
        val tiles = PhoneTiles { resources }
        // As a server sends them: Other before Franchises, and Other the category of those without one.
        val other = CategoryRef(3, "other", "Other")
        val franchises = CategoryRef(2, "franchises", "Franchises")
        val collections = (1..24).map { n -> CollectionSummary(n.toLong(), 1, "Shelf %02d".format(n), category = other, itemCount = 2) } +
            (25..48).map { n -> CollectionSummary(n.toLong(), 1, "Saga %02d".format(n), category = franchises, itemCount = 2) }
        val shown = collections.map { it.tile(tiles.collection(it, LibraryKind.Movies, "")) }
        val categories = listOf(CollectionCategory(3, "other", "Other"), CollectionCategory(2, "franchises", "Franchises"))
        var chosen by mutableStateOf<CategoryChoice>(CategoryChoice.All)
        compose.setContent {
            Page {
                CollectionsPane(
                    collections = Loadable.Ready(CollectionsView.of(shown, categories, chosen), fresh = true),
                    kind = LibraryKind.Movies,
                    onRetry = {},
                    skeleton = rememberSkeleton(animated = false),
                    filters = CollectionFilters(category = { chosen = it }, clear = { chosen = CategoryChoice.All }),
                )
            }
        }
        // Down to the last of Franchises, then Other chosen: its first collections show, not its last.
        compose.onNode(hasScrollToIndexAction() and SemanticsMatcher.keyIsDefined(SemanticsProperties.VerticalScrollAxisRange)).performScrollToIndex(49)
        assertTrue(top("Saga 48") > 0f)
        compose.runOnIdle { chosen = CategoryChoice.Of(3) }
        val first = top("Shelf 01")
        assertTrue("$first", first > 0f && first < top("Shelf 04"))
    }
}
