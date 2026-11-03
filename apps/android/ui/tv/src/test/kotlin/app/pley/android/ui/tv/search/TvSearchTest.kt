package app.pley.android.ui.tv.search

import androidx.activity.ComponentActivity
import androidx.compose.foundation.text.input.TextFieldState
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.assert
import androidx.compose.ui.test.assertIsFocused
import androidx.compose.ui.test.getBoundsInRoot
import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.onRoot
import androidx.compose.ui.test.performKeyInput
import androidx.compose.ui.test.pressKey
import app.pley.android.common.Tier
import app.pley.android.designsystem.tile.ArtworkTileModel
import app.pley.android.model.LiveChannel
import app.pley.android.screens.search.SearchGroup
import app.pley.android.screens.search.SearchGroupKind
import app.pley.android.screens.search.SearchResultsState
import app.pley.android.screens.search.SearchUiState
import app.pley.android.ui.tv.kit.focused
import app.pley.android.ui.tv.kit.press
import app.pley.android.ui.tv.theme.TvTheme
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

/**
 * Search with the remote (design spec §4.5, D-pad rules 1 and 7): focus
 * starts on the field; Down goes into the results' rows and Up back to
 * the field; recent searches are chips under it, which move only when
 * one would pass the gutter or the safe area's edge.
 */
@RunWith(RobolectricTestRunner::class)
@Config(qualifiers = "w960dp-h540dp-xhdpi")
class TvSearchTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    private val opened = mutableListOf<Pair<SearchGroupKind, Long>>()
    private val chosen = mutableListOf<String>()
    private val watched = mutableListOf<Long>()

    private fun show(state: SearchUiState<ArtworkTileModel>, reduced: Boolean = true) {
        compose.setContent {
            TvTheme(Tier.T2, reducedMotion = reduced) {
                TvSearchContent(
                    state = state,
                    field = TextFieldState(state.query),
                    onSearch = {},
                    onRecent = { chosen += it },
                    onClearRecent = {},
                    onOpen = { group, id -> opened += group to id },
                    onActions = {},
                    onWatch = { watched += it.id },
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

    @Test
    fun recentChipsMoveOnlyWhenOneWouldPassTheEdge() {
        show(SearchUiState("", List(12) { "a longer search $it" }, SearchResultsState.Idle))
        compose.press(Key.DirectionDown)
        // Along the chips that show between the gutter and the safe area's edge, nothing moves.
        compose.press(Key.DirectionRight, times = 5)
        compose.focused().assert(hasText("a longer search 5"))
        assertEquals(96f, compose.onNodeWithText("a longer search 0").getBoundsInRoot().left.value, 1f)
        // On to Clear, past the edge: the row moves just far enough for it, to the safe area's edge.
        compose.press(Key.DirectionRight, times = 7)
        compose.press(Key.DirectionLeft)
        assertEquals(912f, compose.onNodeWithText("Clear").getBoundsInRoot().right.value, 1f)
    }

    @OptIn(ExperimentalTestApi::class)
    @Test
    fun recentChipsGlideOver150MsEasedOutWhenTheyMove() {
        show(SearchUiState("", List(12) { "a longer search $it" }, SearchResultsState.Idle), reduced = false)
        compose.press(Key.DirectionDown)
        val first = compose.onNodeWithText("a longer search 0")
        val start = first.getBoundsInRoot().left.value
        compose.mainClock.autoAdvance = false
        // Right until a chip would pass the edge: that press moves the row as rows move, over
        // 150 ms eased out, so over several frames, each moving it less than the one before.
        for (chip in 1..12) {
            compose.onRoot().performKeyInput { pressKey(Key.DirectionRight) }
            var at = start - first.getBoundsInRoot().left.value
            val steps = mutableListOf<Float>()
            repeat(20) {
                compose.mainClock.advanceTimeByFrame()
                val now = start - first.getBoundsInRoot().left.value
                if (now != at) steps += now - at
                at = now
            }
            if (steps.isEmpty()) continue
            assertTrue("$steps at chip $chip", steps.size >= 5 && steps.zipWithNext().all { (a, b) -> b <= a + 0.5f })
            return
        }
        fail("The chips never moved")
    }

    @Test
    fun noResultsSaysSo() {
        show(SearchUiState("zzz", emptyList(), SearchResultsState.Empty("zzz")))
        compose.onNodeWithText("No results for “zzz”").assertExists()
        compose.onNodeWithText("Try a different spelling or fewer words.").assertExists()
    }
}
