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
import androidx.compose.ui.unit.dp
import androidx.navigation3.runtime.NavKey
import app.pley.android.common.TestTags
import app.pley.android.common.Tier
import app.pley.android.designsystem.TvSafeArea
import app.pley.android.designsystem.tile.TileKind
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
 * goes to the shown season's tab, and tabs past the screen's edge scroll in.
 */
@RunWith(RobolectricTestRunner::class)
@Config(qualifiers = "w960dp-h540dp-xhdpi")
@OptIn(ExperimentalTestApi::class)
class TvItemPageTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    private val stack = mutableListOf<NavKey>(PleyKey.Home, PleyKey.Item(1))

    private fun show(data: ItemData, reduced: Boolean = true, id: Long = 1) {
        val model = ItemViewModel(data, data.launcher, id, compute = Dispatchers.Main.immediate)
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
    @Config(qualifiers = "fr-w960dp-h540dp-xhdpi")
    fun theButtonsAreInFrench() {
        show(ItemData(movie(positionMs = 60_000)))
        compose.onNodeWithText("Reprendre").assertExists()
        press(Key.DirectionRight, 3)
        compose.onNode(isFocused()).assert(hasContentDescription("Audio et sous-titres"))
    }

    @Test
    @Config(qualifiers = "es-w960dp-h540dp-xhdpi")
    fun theButtonsAreInSpanish() {
        show(ItemData(movie(positionMs = 60_000)))
        compose.onNodeWithText("Reanudar").assertExists()
        press(Key.DirectionRight, 3)
        compose.onNode(isFocused()).assert(hasContentDescription("Audio y subtítulos"))
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

    /**
     * A show of [count] seasons of four episodes each; with [posters], the first season has a poster of
     * its own. [next] is the season of the episode to watch, its first.
     */
    private fun showOf(count: Int, posters: Boolean = false, next: Int? = null): ItemData {
        val show = movie().copy(kind = ItemKind.Show, title = "Breaking Bad", files = emptyList(), leafCount = count * 4)
        val children = seasonsOf(count, posters)
        val onDeck = next?.let { children.getValue(100L + it).first() }
        return ItemData(show, children = children, onDeck = listOfNotNull(onDeck?.let { 1L to it }).toMap())
    }

    /** Show 1's [count] seasons, and each season's four episodes, by the item they are under. */
    private fun seasonsOf(count: Int, posters: Boolean): Map<Long, List<ItemSummary>> {
        val seasons = (1..count).map { s ->
            val poster = if (s == 1) "/api/items/${100 + s}/images/poster?v=2" else "/api/items/1/images/poster?v=1"
            ItemSummary(
                id = 100L + s, libraryId = 1, kind = ItemKind.Season, title = "Season $s", index = s, parentId = 1, leafCount = 4,
                posterUrl = poster.takeIf { posters },
            )
        }
        val episodes = seasons.associate { season ->
            season.id to (1..4).map { e ->
                ItemSummary(
                    id = season.id * 10 + e, libraryId = 1, kind = ItemKind.Episode, title = "Episode $e", index = e,
                    parentId = season.id, parentIndex = season.index, grandparentId = 1, durationMs = 2_700_000,
                )
            }
        }
        return mapOf(1L to seasons) + episodes
    }

    @Test
    fun upFromAnEpisodeGoesToItsSeasonsTab() {
        show(showOf(3))
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
    fun seasonTabsPastTheScreenScrollIntoViewAndShowTheirSeason() {
        // More than the screen holds, even with Robolectric's narrow text; that many are listed newest first.
        show(showOf(25))
        val episode = hasAnyAncestor(hasTestTag(TV_EPISODE_ROW)) and isFocused()
        while (compose.onAllNodes(episode).fetchSemanticsNodes().isEmpty()) press(Key.DirectionDown)
        press(Key.DirectionUp)
        compose.onNode(isFocused()).assert(hasText("Season 25"))
        for (s in 24 downTo 1) {
            press(Key.DirectionRight)
            compose.onNode(isFocused()).assert(hasText("Season $s"))
            // Each tab keeps its own size, a pill one line tall and wider than that, and comes on screen.
            val tab = compose.onNode(isFocused()).getBoundsInRoot()
            assertTrue("Season $s: $tab", tab.left.value >= 0f && tab.right.value <= 960f)
            assertTrue("Season $s: $tab", tab.bottom - tab.top < 50.dp && tab.right - tab.left > tab.bottom - tab.top)
        }
        // Focus resting on Season 1 shows it, and Down goes to its episodes, captions on screen.
        compose.mainClock.advanceTimeBy(2_000)
        compose.waitForIdle()
        compose.onNode(hasText("Season 1") and isSelected()).assertExists()
        press(Key.DirectionDown)
        compose.onNode(isFocused()).assert(hasContentDescription("Episode 1", substring = true))
        assertEquals(TabbedEpisodeTilesTop.value, focusedTop(), 1f)
        // Back to the first tab: the row scrolls back with it.
        press(Key.DirectionUp)
        compose.onNode(isFocused()).assert(hasText("Season 1"))
        press(Key.DirectionLeft, 24)
        compose.onNode(isFocused()).assert(hasText("Season 25"))
        assertTrue(compose.onNode(isFocused()).getBoundsInRoot().left.value >= 0f)
    }

    @Test
    fun aLongShowsTabsOpenWithTheSeasonToWatchInSight() {
        // The newest season is the first tab, so the first season, the one to watch, is the last.
        show(showOf(25, next = 1))
        val shown = compose.onNode(hasText("Season 1") and isSelected()).getBoundsInRoot()
        assertTrue("$shown", shown.left.value >= 0f && shown.right.value <= 960f - TvSafeArea.Horizontal.value + 1f)
        // Up from its episodes goes to its tab, which stays where it is.
        press(Key.DirectionDown)
        compose.onNode(isFocused()).assert(hasContentDescription("Episode 1", substring = true))
        press(Key.DirectionUp)
        compose.onNode(isFocused()).assert(hasText("Season 1"))
        press(Key.DirectionLeft)
        compose.onNode(isFocused()).assert(hasText("Season 2"))
    }

    @Test
    fun aLongShowsSeasonPostersOpenAtTheSeasonToWatch() {
        show(showOf(14, posters = true, next = 1))
        // Down from the buttons lands on the season shown, the last poster of fourteen, on screen.
        press(Key.DirectionDown)
        val season = hasAnyAncestor(hasTestTag(TV_SEASON_TABS)) and isFocused()
        compose.onNode(season).assert(hasContentDescription("Season 1, 4 episodes", substring = true))
        val tile = compose.onNode(isFocused()).getBoundsInRoot()
        assertTrue("$tile", tile.left.value >= 0f && tile.right.value <= 960f)
        // Resting there leaves Season 1 shown; the seasons before it are the newer ones.
        compose.mainClock.advanceTimeBy(2_000)
        compose.waitForIdle()
        press(Key.DirectionLeft)
        compose.onNode(season).assert(hasContentDescription("Season 2, 4 episodes", substring = true))
        press(Key.DirectionLeft, 12)
        compose.onNode(season).assert(hasContentDescription("Season 14, 4 episodes", substring = true))
    }

    @Test
    fun aSeasonWithItsOwnPosterShowsTheSeasonsAsPosterTiles() {
        show(showOf(12, posters = true))
        // Down from the buttons comes to the seasons, on the season shown.
        press(Key.DirectionDown)
        val season = hasAnyAncestor(hasTestTag(TV_SEASON_TABS)) and isFocused()
        compose.onNode(season).assert(hasContentDescription("Season 1, 4 episodes", substring = true))
        assertEquals(TabsTop.value, focusedTop(), 2f)
        for (s in 2..12) {
            press(Key.DirectionRight)
            // Every season a poster tile, those without a poster of their own too, and on screen.
            compose.onNode(season).assert(hasContentDescription("Season $s, 4 episodes", substring = true))
            val tile = compose.onNode(isFocused()).getBoundsInRoot()
            assertTrue("Season $s: $tile", tile.left.value >= 0f && tile.right.value <= 960f)
            assertEquals("Season $s: $tile", TileKind.Poster.tvHeight.value, (tile.bottom - tile.top).value, 14f)
        }
        // Focus resting on Season 12 shows it, named over its episodes.
        compose.mainClock.advanceTimeBy(2_000)
        compose.waitForIdle()
        press(Key.DirectionDown)
        compose.onNode(isFocused()).assert(hasContentDescription("Season 12 Episode 1", substring = true))
        compose.onNode(hasText("Season 12")).assertExists()
        // The episodes sit under the season's name, as a single season's do.
        assertEquals(EpisodeTilesTop.value, focusedTop(), 1f)
        // Up comes back to the season shown. Select on another, before focus has rested there, shows
        // it and goes down into its episodes: no other page opens.
        press(Key.DirectionUp)
        compose.onNode(season).assert(hasContentDescription("Season 12", substring = true))
        compose.mainClock.autoAdvance = false
        compose.onRoot().performKeyInput { pressKey(Key.DirectionLeft, 1) }
        compose.onRoot().performKeyInput { pressKey(Key.Enter, 1) }
        compose.mainClock.autoAdvance = true
        compose.waitForIdle()
        compose.onNode(isFocused()).assert(hasContentDescription("Season 11 Episode 1", substring = true))
        compose.onNode(hasText("Season 11")).assertExists()
        assertEquals(listOf<NavKey>(PleyKey.Home, PleyKey.Item(1)), stack)
        // Select on the season shown goes down too.
        press(Key.DirectionUp)
        press(Key.Enter)
        compose.onNode(isFocused()).assert(hasContentDescription("Season 11 Episode 1", substring = true))
    }

    @Test
    fun aShowWithoutSeasonPostersKeepsItsWordTabs() {
        show(showOf(3))
        val episode = hasAnyAncestor(hasTestTag(TV_EPISODE_ROW)) and isFocused()
        while (compose.onAllNodes(episode).fetchSemanticsNodes().isEmpty()) press(Key.DirectionDown)
        assertEquals(TabbedEpisodeTilesTop.value, focusedTop(), 1f)
        press(Key.DirectionUp)
        compose.onNode(isFocused()).assert(hasText("Season 1"))
        compose.onNode(hasText("4 episodes")).assertDoesNotExist()
    }

    @Test
    fun aSeasonsPageOfAShowWithSeasonPostersShowsItsEpisodesUnderTheButtons() {
        val season = movie().copy(
            id = 102, kind = ItemKind.Season, title = "Season 2", index = 2, parentId = 1, parentTitle = "Breaking Bad",
            files = emptyList(), leafCount = 4,
        )
        show(ItemData(season, children = seasonsOf(3, posters = true)), id = 102)
        // The page Select on Season 2's poster opens: no row of posters between its buttons and its episodes.
        compose.onNode(hasTestTag(TV_SEASON_TABS)).assertDoesNotExist()
        compose.onNodeWithTag(TestTags.ITEM_PLAY).assertIsFocused()
        press(Key.DirectionDown)
        compose.onNode(isFocused()).assert(hasContentDescription("Season 2 Episode 1", substring = true))
        assertEquals(EpisodeTilesTop.value, focusedTop(), 1f)
        press(Key.DirectionUp)
        compose.onNodeWithTag(TestTags.ITEM_PLAY).assertIsFocused()
    }

    @Test
    fun onAnEpisodesPageSelectOnItPlaysItAndAnotherEpisodeTakesThePagesPlace() {
        val episodes = seasonsOf(2, posters = false)
        val episode = movie().copy(
            id = 1012, kind = ItemKind.Episode, title = "Episode 2", index = 2, parentId = 101, parentIndex = 1,
            grandparentId = 1, grandparentTitle = "Breaking Bad",
        )
        val data = ItemData(episode, children = episodes)
        stack.clear()
        stack += listOf(PleyKey.Home, PleyKey.Item(1), PleyKey.Item(1012))
        show(data, id = 1012)
        // Down from Play: its season's row, on the page's own episode.
        press(Key.DirectionDown)
        compose.onNode(isFocused()).assert(hasContentDescription("Episode 2", substring = true))
        // Select there plays it; the page stays.
        press(Key.Enter)
        assertEquals(1012L, data.played.single().itemId)
        assertEquals(listOf<NavKey>(PleyKey.Home, PleyKey.Item(1), PleyKey.Item(1012)), stack)
        // Another episode's page takes this one's place: Back is still the show.
        press(Key.DirectionRight)
        press(Key.Enter)
        assertEquals(listOf<NavKey>(PleyKey.Home, PleyKey.Item(1), PleyKey.Item(1013)), stack)
    }

    @Test
    fun selectOnASeasonsTabShowsItsEpisodesWithoutTheRest() {
        show(showOf(3))
        val episode = hasAnyAncestor(hasTestTag(TV_EPISODE_ROW)) and isFocused()
        while (compose.onAllNodes(episode).fetchSemanticsNodes().isEmpty()) press(Key.DirectionDown)
        press(Key.DirectionUp)
        compose.mainClock.autoAdvance = false
        // Short presses: the clock stays well short of the rest.
        compose.onRoot().performKeyInput { repeat(2) { pressKey(Key.DirectionRight, 1) } }
        compose.onNode(isFocused()).assert(hasText("Season 3"))
        compose.onRoot().performKeyInput { pressKey(Key.Enter, 1) }
        compose.mainClock.advanceTimeBy(32)
        compose.onNode(hasText("Season 3") and isSelected()).assertExists()
        compose.mainClock.autoAdvance = true
        press(Key.DirectionDown)
        compose.onNode(isFocused()).assert(hasContentDescription("Episode 1", substring = true))
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
        // Up glides back to the top the same way: from where the buttons went with the page, back to where they were.
        val away = compose.onNodeWithTag(TestTags.ITEM_PLAY).getBoundsInRoot().top.value
        assertEquals(play - (from - to), away, 0.25f)
        compose.onRoot().performKeyInput { pressKey(Key.DirectionUp) }
        compose.onNodeWithTag(TestTags.ITEM_PLAY).assertIsFocused()
        compose.mainClock.advanceTimeBy(80)
        val back = (focusedTop() - away) / (play - away)
        assertTrue("$back", back > 0.5f && back < 1f)
        compose.mainClock.advanceTimeBy(80)
        assertEquals(play, focusedTop(), 0.25f)
    }

    @Test
    fun whoMadeItAndTheFactsSitUnderTheCastInSight() {
        val cast = listOf(Credit(1, "Idris Vale", CreditRole.Actor, character = "Ras"), Credit(2, "Mara Holt", CreditRole.Actor, character = "Ines"))
        val crew = listOf(
            Credit(3, "Ada Brook", CreditRole.Director),
            Credit(4, "Theo Lane", CreditRole.Writer),
            Credit(5, "June Park", CreditRole.Writer),
        )
        val movie = movie().copy(cast = cast, crew = crew, originalTitle = "Le Phare", releaseDate = "2023-04-01", studio = "Northlight")
        show(ItemData(movie))
        press(Key.DirectionDown)
        compose.onNode(isFocused()).assert(hasContentDescription("Idris Vale"))
        val facts = compose.onNodeWithTag(TAG_FACTS)
        facts.assert(hasText("Director", substring = true)).assert(hasText("Ada Brook", substring = true))
        facts.assert(hasText("Theo Lane, June Park", substring = true)).assert(hasText("Northlight", substring = true))
        // Under the cast's names, and above the safe area while the cast has focus.
        val tile = compose.onNode(isFocused()).getBoundsInRoot()
        val bounds = facts.getBoundsInRoot()
        assertTrue("$bounds", bounds.top > tile.bottom && bounds.bottom <= 540.dp - TvSafeArea.Vertical)
    }
}
