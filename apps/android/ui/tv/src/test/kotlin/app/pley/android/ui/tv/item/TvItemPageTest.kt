package app.pley.android.ui.tv.item

import androidx.activity.ComponentActivity
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.assert
import androidx.compose.ui.test.assertIsFocused
import androidx.compose.ui.test.getBoundsInRoot
import androidx.compose.ui.test.hasAnyAncestor
import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.isFocused
import androidx.compose.ui.test.isSelected
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.onRoot
import androidx.compose.ui.test.performKeyInput
import androidx.compose.ui.test.pressKey
import androidx.navigation3.runtime.NavKey
import app.pley.android.common.TestTags
import app.pley.android.common.Tier
import app.pley.android.model.Credit
import app.pley.android.model.CreditRole
import app.pley.android.model.ItemKind
import app.pley.android.model.ItemRequest
import app.pley.android.model.ItemSummary
import app.pley.android.model.RequestList
import app.pley.android.model.TitleKind
import app.pley.android.model.TitleStatus
import app.pley.android.screens.PleyKey
import app.pley.android.screens.item.ItemViewModel
import app.pley.android.testing.Fixtures
import app.pley.android.testing.data.FakeRequests
import app.pley.android.ui.tv.shell.LocalTvNavigator
import app.pley.android.ui.tv.shell.RailState
import app.pley.android.ui.tv.shell.TvNavigator
import app.pley.android.ui.tv.theme.TvTheme
import kotlinx.coroutines.Dispatchers
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

/**
 * The TV item page with the remote (D-pad rules 1 and 6, design spec §2.7):
 * focus starts on Play or Resume; the Audio and subtitles panel takes focus,
 * keeps it, saves a choice, and Back closes it with focus back on its button;
 * the page glides to a row and back to its buttons as a page of rows does.
 * A movie or show from a request shows its stage, and a show with seasons
 * left to ask for offers Request more seasons. Up from a show's episodes
 * goes to the shown season's tab.
 */
@RunWith(RobolectricTestRunner::class)
@Config(qualifiers = "w960dp-h540dp-xhdpi")
@OptIn(ExperimentalTestApi::class)
class TvItemPageTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    private val stack = mutableListOf<NavKey>(PleyKey.Home, PleyKey.Item(1))

    private fun show(data: ItemData, reduced: Boolean = true) {
        val model = ItemViewModel(data, data.launcher, 1, compute = Dispatchers.Main.immediate)
        val navigator = TvNavigator(stack, RailState()) {}
        compose.setContent {
            TvTheme(Tier.T2, reducedMotion = reduced) {
                CompositionLocalProvider(LocalTvNavigator provides navigator) { TvItemScreen(model) }
            }
        }
        compose.waitForIdle()
    }

    private fun press(key: Key, times: Int = 1) = repeat(times) {
        compose.onRoot().performKeyInput { pressKey(key) }
        compose.waitForIdle()
    }

    /** The focused element's top, in dp from the screen's. */
    private fun focusedTop(): Float = compose.onNode(isFocused()).getBoundsInRoot().top.value

    @Test
    fun focusStartsOnPlay() {
        show(ItemData(movie()))
        compose.onNodeWithTag(TestTags.ITEM_PLAY).assertIsFocused()
        compose.onNodeWithText("Play").assertExists()
    }

    @Test
    fun aMovieInProgressStartsOnResume() {
        show(ItemData(movie(positionMs = 60_000)))
        compose.onNodeWithTag(TestTags.ITEM_PLAY).assertIsFocused()
        compose.onNodeWithText("Resume").assertExists()
    }

    @Test
    fun theTracksPanelTakesFocusSavesAChoiceAndBackClosesIt() {
        val data = ItemData(movie())
        show(data)
        // Play → Watched → Audio and subtitles.
        press(Key.DirectionRight, 2)
        compose.onNode(isFocused()).assert(hasContentDescription("Audio and subtitles"))
        press(Key.Enter)
        compose.onNodeWithTag(TV_PANEL).assertExists()
        // Nothing picked: focus starts on Automatic, in the language, and stays in the panel.
        compose.onNode(isFocused()).assertExists()
        press(Key.DirectionLeft, 3)
        compose.onNode(isFocused()).assert(hasText("Automatic (English)"))
        press(Key.DirectionDown, 2)
        compose.onNode(isFocused()).assert(hasText("French (AC3 5.1)"))
        press(Key.Enter)
        assertEquals(listOf(listOf<Long?>(1, 11, 2, null)), data.streams)
        // Automatic again: the pick is forgotten.
        press(Key.DirectionUp, 2)
        press(Key.Enter)
        assertEquals(listOf<Long?>(1, 11, null, null), data.streams.last())
        compose.runOnUiThread { compose.activity.onBackPressedDispatcher.onBackPressed() }
        compose.waitForIdle()
        compose.onNode(isFocused()).assert(hasContentDescription("Audio and subtitles"))
    }

    @Test
    fun aShowFromARequestShowsItsStageAndAsksForMoreSeasonsOnItsPage() {
        val requests = FakeRequests()
        val downloading = Fixtures.decode<RequestList>("requests/list.json").requests.first { it.id == 42L }
        requests.api.answer("itemRequest", ItemRequest(TitleKind.Show, 1396, TitleStatus.Requested, downloading, askable = listOf(3)))
        val show = movie().copy(kind = ItemKind.Show, title = "Breaking Bad", files = emptyList(), leafCount = 20)
        show(ItemData(show, requests = requests))
        // Asked once the page has drawn.
        compose.waitUntil(5_000) { compose.onAllNodes(hasText("Downloading 42%", substring = true)).fetchSemanticsNodes().isNotEmpty() }
        compose.onNodeWithTag(TestTags.ITEM_PLAY).assertIsFocused()
        press(Key.DirectionRight)
        compose.onNode(isFocused()).assert(hasContentDescription("Request more seasons"))
        press(Key.Enter)
        assertEquals(PleyKey.DiscoverTitle(TitleKind.Show, 1396, pick = true), stack.last())
    }

    @Test
    fun upFromAnEpisodeGoesToItsSeasonsTab() {
        val show = movie().copy(kind = ItemKind.Show, title = "Breaking Bad", files = emptyList(), leafCount = 12)
        val seasons = (1..3).map { s -> ItemSummary(id = 10L + s, libraryId = 1, kind = ItemKind.Season, title = "Season $s", index = s, parentId = 1) }
        val episodes = seasons.associate { season ->
            season.id to (1..4).map { e ->
                ItemSummary(
                    id = season.id * 10 + e, libraryId = 1, kind = ItemKind.Episode, title = "Episode $e", index = e,
                    parentId = season.id, parentIndex = season.index, grandparentId = 1, durationMs = 2_700_000,
                )
            }
        }
        show(ItemData(show, children = mapOf(1L to seasons) + episodes))
        val episode = hasAnyAncestor(hasTestTag(TV_EPISODE_ROW)) and isFocused()
        while (compose.onAllNodes(episode).fetchSemanticsNodes().isEmpty()) press(Key.DirectionDown)
        compose.onNode(isFocused()).assert(hasContentDescription("Episode 1", substring = true))
        press(Key.DirectionUp)
        compose.onNode(isFocused()).assert(hasText("Season 1"))
        // Focus resting on it leaves Season 1 shown.
        compose.mainClock.advanceTimeBy(2_000)
        compose.waitForIdle()
        compose.onNode(hasTestTag(TV_SEASON_TABS)).assertExists()
        compose.onNode(hasText("Season 1") and isSelected()).assertExists()
        compose.onNode(hasText("Season 2") and isSelected()).assertDoesNotExist()
    }

    @Test
    fun thePageGlidesToARowAndBackToItsButtonsOver150MsEasedOut() {
        val cast = listOf(Credit(1, "Idris Vale", CreditRole.Actor, character = "Ras"), Credit(2, "Mara Holt", CreditRole.Actor, character = "Ines"))
        show(ItemData(movie().copy(cast = cast)), reduced = false)
        val play = focusedTop()
        val from = compose.onNode(hasContentDescription("Idris Vale")).getBoundsInRoot().top.value
        val to = RowTilesTop.value
        compose.mainClock.autoAdvance = false
        compose.onRoot().performKeyInput { pressKey(Key.DirectionDown) }
        compose.onNode(isFocused()).assert(hasContentDescription("Idris Vale"))
        // Eased out, as Home's rows: well over half the way a third of the way through, and there
        // after 150 ms, where Compose's own scrolling would still be easing in.
        compose.mainClock.advanceTimeBy(80)
        val partway = (from - focusedTop()) / (from - to)
        assertTrue("$partway", partway > 0.5f && partway < 1f)
        compose.mainClock.advanceTimeBy(80)
        assertEquals(to, focusedTop(), 0.25f)
        // Up glides back to the top the same way.
        compose.onRoot().performKeyInput { pressKey(Key.DirectionUp) }
        compose.onNodeWithTag(TestTags.ITEM_PLAY).assertIsFocused()
        compose.mainClock.advanceTimeBy(80)
        val back = (focusedTop() - to) / (play - to)
        assertTrue("$back", back > 0.5f && back < 1f)
        compose.mainClock.advanceTimeBy(80)
        assertEquals(play, focusedTop(), 0.25f)
    }
}
