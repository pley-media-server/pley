package app.pley.android.ui.tv.shell

import androidx.compose.foundation.background
import androidx.compose.foundation.focusGroup
import androidx.compose.foundation.focusable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusProperties
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.toPixelMap
import androidx.compose.ui.input.InputMode
import androidx.compose.ui.input.InputModeManager
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.platform.LocalInputModeManager
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.assertCountEquals
import androidx.compose.ui.test.assertIsFocused
import androidx.compose.ui.test.captureToImage
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.isFocused
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.getBoundsInRoot
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.onRoot
import androidx.compose.ui.test.performKeyInput
import androidx.compose.ui.test.pressKey
import androidx.compose.ui.unit.dp
import app.pley.android.common.TestTags
import app.pley.android.designsystem.PleyColors
import app.pley.android.model.Library
import app.pley.android.model.LibraryKind
import app.pley.android.screens.requests.RequestsEntry
import app.pley.android.ui.tv.kit.press
import app.pley.android.ui.tv.kit.railOnLeftStopOnRight
import app.pley.android.ui.tv.theme.TvTheme
import kotlin.math.abs
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

/**
 * The rail (design spec §2.1): Left from the page opens it over 200 ms, in
 * place with reduced motion. Only Left goes into it, and a key pressed
 * with nothing focused brings focus back to the page. Its entries are 40
 * tall on a 44 pitch, and scroll between the avatar and Settings when they
 * don't all fit.
 */
@OptIn(ExperimentalTestApi::class)
@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(qualifiers = "w960dp-h540dp-xhdpi")
class TvRailTest {
    @get:Rule
    val compose = createComposeRule()

    private val content = FocusRequester()
    private val nearFocus = FocusRequester()
    private val rail = RailState(content).apply {
        shown = true
        current = "home"
    }
    private lateinit var input: InputModeManager

    /**
     * The rail over a page, as the shell has them: [FAR] on the page (while
     * [far] can take focus), and [NEAR] above and left of it while [near].
     * Focus starts on the page.
     */
    private fun shell(
        reduced: Boolean,
        near: () -> Boolean = { false },
        far: () -> Boolean = { true },
        items: List<RailItem> = railItems(emptyList()),
    ) {
        compose.setContent {
            input = LocalInputModeManager.current
            TvTheme(reducedMotion = reduced) {
                Box(Modifier.fillMaxSize().background(Color.Black)) {
                    Box(
                        Modifier
                            .fillMaxSize()
                            .focusRequester(content)
                            .onFocusChanged { rail.contentFocused = it.hasFocus }
                            .railOnLeftStopOnRight(rail, stopRight = false)
                            .focusGroup(),
                    ) {
                        if (near()) Box(Modifier.offset(300.dp, 100.dp).size(40.dp).testTag(NEAR).focusRequester(nearFocus).focusable())
                        Box(Modifier.offset(600.dp, 300.dp).size(40.dp).testTag(FAR).focusProperties { canFocus = far() }.focusable())
                    }
                    TvRail(items, RailProfile("Anna"), rail, onSelect = {}, onProfile = {})
                }
            }
        }
        compose.waitForIdle()
        // A TV isn't in touch mode, where the rail's entries (clickable, as tiles are) take no focus.
        compose.runOnUiThread {
            input.requestInputMode(InputMode.Keyboard)
            content.requestFocus()
        }
        compose.waitForIdle()
    }

    /** The colour at [xDp] across the rail's panel, halfway down, below its entries. */
    private fun panelAt(xDp: Int): Color {
        val image = compose.onRoot().captureToImage().toPixelMap()
        return image[xDp * 2, image.height / 2]
    }

    private fun isPanel(color: Color): Boolean {
        val panel = PleyColors.Surface
        return abs(color.red - panel.red) < 0.05f && abs(color.green - panel.green) < 0.05f && abs(color.blue - panel.blue) < 0.05f
    }

    private fun pressNow(key: Key) = compose.onRoot().performKeyInput { pressKey(key) }

    @Test
    fun leftOpensTheRailOverItsTime() {
        shell(reduced = false)
        compose.onNodeWithTag(FAR).assertIsFocused()
        compose.mainClock.autoAdvance = false
        pressNow(Key.DirectionLeft)
        compose.mainClock.advanceTimeByFrame()
        compose.mainClock.advanceTimeByFrame()
        // Two frames in, the panel is still narrow: 250 dp across is the page.
        assertFalse(isPanel(panelAt(250)))
        compose.mainClock.advanceTimeBy(300)
        assertTrue(isPanel(panelAt(250)))
        compose.onNodeWithTag("${TestTags.TV_RAIL}.home").assertIsFocused()
    }

    @Test
    fun labelsSlideInAsTheRailOpens() {
        shell(reduced = false)
        compose.mainClock.autoAdvance = false
        pressNow(Key.DirectionLeft)
        compose.mainClock.advanceTimeByFrame()
        compose.mainClock.advanceTimeByFrame()
        // Search's, as Home's is focused and grows.
        val search = compose.onNodeWithText("Search", useUnmergedTree = true)
        val sliding = search.getBoundsInRoot().left
        compose.mainClock.advanceTimeBy(300)
        // In place 56 dp into the pill, 8 dp into the rail; on its way, short of it.
        val rest = search.getBoundsInRoot().left
        assertEquals(64f, rest.value, 0.5f)
        assertTrue("$sliding", sliding.value < rest.value - 1f)
    }

    @Test
    fun withReducedMotionTheRailOpensAndClosesInPlace() {
        shell(reduced = true)
        compose.mainClock.autoAdvance = false
        pressNow(Key.DirectionLeft)
        compose.mainClock.advanceTimeByFrame()
        compose.mainClock.advanceTimeByFrame()
        assertTrue(isPanel(panelAt(250)))
        compose.onNodeWithTag("${TestTags.TV_RAIL}.home").assertIsFocused()
        // Right goes back to the page, and the panel is gone with it.
        pressNow(Key.DirectionRight)
        compose.mainClock.advanceTimeByFrame()
        compose.mainClock.advanceTimeByFrame()
        assertFalse(isPanel(panelAt(250)))
        compose.onNodeWithTag(FAR).assertIsFocused()
    }

    @Test
    fun aKeyWithNothingFocusedBringsFocusBackToThePage() {
        var near by mutableStateOf(true)
        var far by mutableStateOf(true)
        shell(reduced = true, near = { near }, far = { far })
        // Right and Down from nowhere would find the rail's entries first, at the top left.
        for (key in listOf(Key.DirectionRight, Key.DirectionDown)) {
            near = true
            compose.waitForIdle()
            compose.runOnUiThread { nearFocus.requestFocus() }
            compose.waitForIdle()
            compose.onNodeWithTag(NEAR).assertIsFocused()
            // The focused element goes with a redraw of its page while nothing else there can take
            // focus, which then comes back: nothing has focus.
            far = false
            near = false
            compose.waitForIdle()
            far = true
            compose.waitForIdle()
            compose.onAllNodes(isFocused()).assertCountEquals(0)
            // The next key doesn't open the rail, nor is it lost: focus is back on the page.
            compose.press(key)
            compose.onNodeWithTag(FAR).assertIsFocused()
        }
    }

    @Test
    fun upFromThePageStaysThere() {
        shell(reduced = true)
        compose.onNodeWithTag(FAR).assertIsFocused()
        compose.press(Key.DirectionUp)
        compose.onNodeWithTag(FAR).assertIsFocused()
    }

    private fun entry(id: String) = compose.onNodeWithTag("${TestTags.TV_RAIL}.$id")

    /** Where an entry's middle is: the focused one grows about it, 1 dp at either end, into the gaps. */
    private fun middle(id: String): Float = entry(id).getBoundsInRoot().let { (it.top + it.bottom).value / 2f }

    /** Where entry [id] starts and ends at rest, without its focus growth. */
    private fun top(id: String): Float = middle(id) - ENTRY / 2f

    private fun bottom(id: String): Float = middle(id) + ENTRY / 2f

    /** Wholly between the avatar's entry and Settings. */
    private fun assertInView(id: String) {
        assertTrue("$id starts at ${top(id)}", top(id) >= ENTRIES_TOP - 0.5f)
        assertTrue("$id ends at ${bottom(id)}", bottom(id) <= ENTRIES_BOTTOM + 0.5f)
    }

    /** Six libraries and more, Discover, Live TV, Now Playing and a USB drive: fourteen entries, where nine fit. */
    private val many = railItems(
        (1L..8L).map { Library(it, "Library $it", LibraryKind.Movies) },
        nowPlaying = true,
        liveTv = true,
        requests = requestsItem(RequestsEntry(discover = true)),
        drive = true,
    )

    @Test
    fun entriesAre40TallOnA44PitchWithSettingsAt473() {
        shell(reduced = true)
        assertEquals(ENTRIES_TOP, top("search"), 0.5f)
        assertEquals(ENTRY, entry("search").getBoundsInRoot().let { it.bottom - it.top }.value, 0.5f)
        assertEquals(ENTRIES_TOP + 44f, top("home"), 0.5f)
        assertEquals(ENTRIES_TOP + 88f, top("playlists"), 0.5f)
        assertEquals(473f, top("settings"), 0.5f)
        assertEquals(513f, bottom("settings"), 0.5f)
    }

    @Test
    fun theFocusedEntryGrowsFromItsIconClearOfTheScreenAndThePanelsEdge() {
        shell(reduced = true)
        pressNow(Key.DirectionLeft)
        compose.waitForIdle()
        entry("home").assertIsFocused()
        // At rest an open entry is 246 dp from x = 8.
        val rest = entry("search").getBoundsInRoot()
        assertEquals(8f, rest.left.value, 0.5f)
        assertEquals(254f, rest.right.value, 0.5f)
        // Focused, it is 1.05 as large about its icon's middle, x = 36: the icon stays in the rail's
        // column, and the pill ends 7 dp from the 272 dp panel's edge, as it starts 7 from the screen's.
        val focused = entry("home").getBoundsInRoot()
        assertEquals(6.6f, focused.left.value, 0.5f)
        assertEquals(264.9f, focused.right.value, 0.5f)
    }

    /**
     * Where entry [i] of [many] starts while it has focus: down to the fourth
     * place, then there while the entries scroll by whole entries, until the
     * last nine show.
     */
    private fun restingTop(i: Int): Float = ENTRIES_TOP + i * 44f - ((i - 3) * 44f).coerceIn(0f, (many.size - 9) * 44f)

    @Test
    fun entriesThatDontFitScrollUnderFocusFourthFromTheTopTheSameBothWays() {
        assertEquals(14, many.size)
        shell(reduced = true, items = many)
        compose.press(Key.DirectionLeft)
        entry("home").assertIsFocused()
        // Down through them all: focus goes down to the fourth entry shown, stays there while the
        // entries move under it an entry a press, then goes on to the last.
        for (i in 1 until many.size) {
            if (i > 1) compose.press(Key.DirectionDown)
            entry(many[i].id).assertIsFocused()
            assertEquals(many[i].id, restingTop(i), top(many[i].id), 0.5f)
            assertInView(many[i].id)
        }
        // The last ends at y = 459, the eight above it whole too.
        assertEquals(ENTRIES_TOP + 9 * 44f - 4f, bottom(many.last().id), 0.5f)
        // Settings hasn't moved, and focus on it moves nothing.
        assertEquals(473f, top("settings"), 0.5f)
        compose.press(Key.DirectionDown)
        entry("settings").assertIsFocused()
        assertEquals(restingTop(many.lastIndex), top(many.last().id), 0.5f)
        // Back up, each entry where it was on the way down, to the first at the top.
        for (i in many.lastIndex downTo 0) {
            compose.press(Key.DirectionUp)
            entry(many[i].id).assertIsFocused()
            assertEquals(many[i].id, restingTop(i), top(many[i].id), 0.5f)
        }
        assertEquals(ENTRIES_TOP, top("search"), 0.5f)
        // Up goes to the avatar, Down from it to the first entry.
        compose.press(Key.DirectionUp)
        assertTrue(rail.hasFocus)
        compose.onAllNodes(isFocused() and hasTestTag("${TestTags.TV_RAIL}.search")).assertCountEquals(0)
        assertEquals(ENTRIES_TOP, top("search"), 0.5f)
        compose.press(Key.DirectionDown)
        entry("search").assertIsFocused()
    }

    @Test
    fun theClosedRailPlacesThePagesEntryAsFocusWouldSoOpeningMovesNothing() {
        // The seventh of fourteen: fourth from the top, the entries scrolled three.
        val page = many[6].id
        rail.current = page
        shell(reduced = true, items = many)
        assertEquals(restingTop(6), top(page), 0.5f)
        assertEquals(ENTRIES_TOP + 3 * 44f, top(page), 0.5f)
        compose.press(Key.DirectionLeft)
        entry(page).assertIsFocused()
        assertEquals(restingTop(6), top(page), 0.5f)
    }

    @Test
    fun theScrollPutsTheFocusedEntryFourthFromTheTopByWholeEntries() {
        val viewport = ENTRIES_BOTTOM - ENTRIES_TOP
        val rule = { focused: Int, count: Int -> railScroll(0f, focused, count, viewport) }
        // Nine fit: nothing moves.
        for (i in 0 until 9) assertEquals(0f, rule(i, 9), 0.01f)
        // Down to the fourth place, then the entries move under it until the last nine show.
        assertEquals(listOf(0f, 0f, 0f, 0f, 44f, 88f, 132f, 132f, 132f, 132f, 132f, 132f), (0 until 12).map { rule(it, 12) })
        assertEquals(listOf(0f, 0f, 0f, 0f, 44f, 88f, 88f, 88f, 88f, 88f, 88f), (0 until 11).map { rule(it, 11) })
        assertEquals(listOf(0f, 0f, 0f, 0f, 44f, 44f, 44f, 44f, 44f, 44f), (0 until 10).map { rule(it, 10) })
        // An entry's place is its own, whatever the scroll was.
        assertEquals(88f, railScroll(500f, 5, 12, viewport), 0.01f)
        assertEquals(88f, railScroll(13f, 5, 12, viewport), 0.01f)
        // Avatar or Settings (no entry): the scroll kept, within the ends, as when entries go.
        assertEquals(50f, railScroll(50f, -1, 12, viewport), 0.01f)
        assertEquals(132f, railScroll(200f, 12, 12, viewport), 0.01f)
        assertEquals(0f, railScroll(44f, -1, 7, viewport), 0.01f)
        // A taller view, on a screen that isn't 16:9, holds ten whole entries.
        assertEquals(176f, railScroll(0f, 13, 14, viewport + 44f), 0.01f)
    }

    private companion object {
        const val NEAR = "near"
        const val FAR = "far"

        /** The entries' room: from 4 below the avatar's entry to a gap above Settings. */
        const val ENTRIES_TOP = 67f
        const val ENTRIES_BOTTOM = 469f
        const val ENTRY = 40f
    }
}
