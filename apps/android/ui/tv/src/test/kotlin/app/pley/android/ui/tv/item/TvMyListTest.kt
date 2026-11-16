package app.pley.android.ui.tv.item

import androidx.activity.ComponentActivity
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.assert
import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.isFocused
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onRoot
import androidx.compose.ui.test.performKeyInput
import androidx.compose.ui.test.pressKey
import androidx.navigation3.runtime.NavKey
import app.pley.android.common.Tier
import app.pley.android.model.ItemKind
import app.pley.android.screens.PleyKey
import app.pley.android.screens.item.ItemViewModel
import app.pley.android.testing.caps.FakeCapsGraph
import app.pley.android.ui.tv.shell.LocalTvNavigator
import app.pley.android.ui.tv.shell.RailState
import app.pley.android.ui.tv.shell.TvNavigator
import app.pley.android.ui.tv.theme.TvTheme
import kotlinx.coroutines.Dispatchers
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

/** The hero's My List button: after Audio and subtitles, and a toggle. */
@RunWith(RobolectricTestRunner::class)
@Config(qualifiers = "w960dp-h540dp-xhdpi")
@OptIn(ExperimentalTestApi::class)
class TvMyListTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    private fun show(data: ItemData, id: Long = 1) {
        val model = ItemViewModel(data, data.launcher, FakeCapsGraph(), id, compute = Dispatchers.Main.immediate)
        val navigator = TvNavigator(mutableListOf<NavKey>(PleyKey.Home, PleyKey.Item(id)), RailState()) {}
        compose.setContent {
            TvTheme(Tier.T2, reducedMotion = true) {
                CompositionLocalProvider(LocalTvNavigator provides navigator) { TvItemScreen(model) }
            }
        }
        compose.waitForIdle()
    }

    private fun press(key: Key, times: Int = 1) = repeat(times) {
        compose.onRoot().performKeyInput { pressKey(key) }
        compose.waitForIdle()
    }

    @Test
    fun theButtonAddsToMyListAndSaysSo() {
        val data = ItemData(movie())
        show(data)
        // Play → Watched → Audio and subtitles → My List.
        press(Key.DirectionRight, 3)
        compose.onNode(isFocused()).assert(hasContentDescription("Add to My List"))

        press(Key.Enter)
        assertEquals(listOf(1L to true), data.listed)
        compose.onNode(isFocused()).assert(hasContentDescription("In My List"))

        press(Key.Enter)
        assertEquals(1L to false, data.listed.last())
        compose.onNode(isFocused()).assert(hasContentDescription("Add to My List"))
    }

    @Test
    fun episodesHaveNoMyListButton() {
        show(ItemData(movie().copy(id = 5, kind = ItemKind.Episode, parentId = 4, grandparentId = 3)), id = 5)
        compose.onNodeWithTag(TV_MY_LIST_BUTTON).assertDoesNotExist()
    }
}
