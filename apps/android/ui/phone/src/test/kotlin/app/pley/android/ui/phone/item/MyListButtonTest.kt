package app.pley.android.ui.phone.item

import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.assert
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import app.pley.android.model.ItemKind
import app.pley.android.screens.Navigator
import app.pley.android.screens.PleyKey
import app.pley.android.screens.item.ItemUiState
import app.pley.android.screens.item.ItemViewModel
import app.pley.android.testing.caps.FakeCapsGraph
import app.pley.android.ui.phone.theme.PhoneTheme
import kotlinx.coroutines.Dispatchers
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

/** The item page's My List button, in the row under Play. */
@RunWith(RobolectricTestRunner::class)
@Config(qualifiers = "w412dp-h915dp-xxhdpi")
class MyListButtonTest {
    @get:Rule
    val compose = createComposeRule()

    private val navigator = object : Navigator {
        override fun open(key: PleyKey) = Unit
        override fun back() = Unit
        override fun restart(key: PleyKey) = Unit
    }

    private fun show(data: ItemData, id: Long) {
        val model = ItemViewModel(data, data.launcher, FakeCapsGraph(), id, compute = Dispatchers.Main.immediate)
        compose.setContent {
            PhoneTheme {
                val state by model.state.collectAsState()
                (state as? ItemUiState.Ready)?.let { ItemPageContent(it.page, model, navigator) }
            }
        }
        compose.waitForIdle()
    }

    private fun says(state: String) = SemanticsMatcher.expectValue(SemanticsProperties.StateDescription, state)

    @Test
    fun aMovieGoesInMyListAndOut() {
        val data = ItemData(movie())
        show(data, 1)
        val button = compose.onNodeWithTag(ITEM_MY_LIST)
        button.assert(says("Not in My List"))

        button.performClick()
        compose.waitForIdle()
        assertEquals(listOf(1L to true), data.listed)
        button.assert(says("In My List"))

        button.performClick()
        compose.waitForIdle()
        assertEquals(1L to false, data.listed.last())
    }

    @Test
    fun anEpisodeHasNone() {
        show(ItemData(movie().copy(id = 5, kind = ItemKind.Episode, parentId = 4, grandparentId = 3)), 5)
        compose.onNodeWithTag(ITEM_MY_LIST).assertDoesNotExist()
    }
}
