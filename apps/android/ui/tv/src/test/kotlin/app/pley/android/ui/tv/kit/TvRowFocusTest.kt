package app.pley.android.ui.tv.kit

import androidx.compose.foundation.focusable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.assertContentDescriptionEquals
import androidx.compose.ui.test.assertIsFocused
import androidx.compose.ui.test.getBoundsInRoot
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onRoot
import androidx.compose.ui.test.performKeyInput
import androidx.compose.ui.test.pressKey
import androidx.compose.ui.unit.dp
import app.pley.android.designsystem.tile.TileKind
import app.pley.android.ui.tv.theme.TvTheme
import kotlin.math.ceil
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

/**
 * D-pad rules 2 to 5 on a page of rows, with key events: up and down
 * between rows, Left from a row's first tile into the rail, Right stopping
 * at the end, each row coming back to the tile it had, a press moving the
 * row over 150 ms, and a held key sliding it under a still ring, on the
 * gate's clock, to land on the tile under the ring when the key comes up;
 * the page gliding to a row over 150 ms for a press, and on without a
 * stop or a jump under a held Up or Down.
 */
@OptIn(ExperimentalTestApi::class)
@RunWith(RobolectricTestRunner::class)
@Config(qualifiers = "w960dp-h540dp-xhdpi")
class TvRowFocusTest {
    @get:Rule
    val compose = createComposeRule()

    private val rail = object : TvRailFocus {
        val requester = FocusRequester()
        var asked = 0
        override var keepsFocus = false

        override fun focusCurrent(): Boolean {
            asked++
            requester.requestFocus()
            return true
        }
    }

    /** On the test's clock. The test sends no repeats, as the system would: none is waited for. */
    private val gate = KeyRepeatGate(firstRepeatMs = 60_000) { compose.mainClock.currentTime }

    private lateinit var list: LazyListState
    private lateinit var rowA: LazyListState

    private fun page(withHero: Boolean = false, reduced: Boolean? = null) {
        compose.setContent {
            list = rememberLazyListState()
            rowA = rememberTvRowState()
            TvTheme(reducedMotion = reduced) {
                CompositionLocalProvider(LocalTvRailFocus provides rail, LocalKeyRepeatGate provides gate) {
                    Row(Modifier.fillMaxSize().keyRepeatGate(gate)) {
                        Box(
                            Modifier
                                .size(56.dp)
                                .focusRequester(rail.requester)
                                .onFocusChanged { rail.keepsFocus = it.hasFocus }
                                .semantics { contentDescription = "rail" }
                                .testTag("rail")
                                .focusable(),
                        )
                        TvRowColumn(
                            state = list,
                            contentPadding = PaddingValues(top = if (withHero) 0.dp else 172.dp),
                            heroHeight = if (withHero) 400.dp else 0.dp,
                            modifier = Modifier.fillMaxSize(),
                        ) {
                            if (withHero) {
                                item("hero") {
                                    Column(Modifier.height(400.dp)) {
                                        Spacer(Modifier.height(341.dp))
                                        Box(
                                            Modifier
                                                .padding(start = 40.dp)
                                                .size(148.dp, 48.dp)
                                                .testTag("resume")
                                                .initialFocus()
                                                .focusable(),
                                        )
                                    }
                                }
                            }
                            item("a") {
                                TvRow("a", "Row A", tiles("A", 20), TileKind.Landscape, onOpen = {}, initialFocus = !withHero, state = rowA)
                            }
                            item("b") { TvRow("b", "Row B", tiles("B", 8, from = 100), TileKind.Poster, onOpen = {}) }
                        }
                    }
                }
            }
        }
        compose.waitForIdle()
    }

    /** A page of twelve rows of posters, the first row's header at the line (y = 172). */
    private fun rows(reduced: Boolean? = null) {
        compose.setContent {
            list = rememberLazyListState()
            TvTheme(reducedMotion = reduced) {
                CompositionLocalProvider(LocalTvRailFocus provides rail, LocalKeyRepeatGate provides gate) {
                    Box(Modifier.fillMaxSize().keyRepeatGate(gate)) {
                        TvRowColumn(state = list, contentPadding = PaddingValues(top = 172.dp, bottom = LastRowRoom), modifier = Modifier.fillMaxSize()) {
                            repeat(12) { r ->
                                item("r$r") { TvRow("r$r", "Row $r", tiles("R$r", 8, from = r * 100L + 1), TileKind.Poster, onOpen = {}, initialFocus = r == 0) }
                            }
                        }
                    }
                }
            }
        }
        compose.waitForIdle()
    }

    /** Row A's tiles' pitch: a landscape tile and its gap, in pixels. */
    private val pitch: Float
        get() = with(compose.density) { (TileKind.Landscape.tvWidth.roundToPx() + TileKind.Landscape.tvGap.roundToPx()).toFloat() }

    /** How far row A has scrolled, in pixels: its tiles all have the one width. */
    private fun scrolledA(): Float = rowA.firstVisibleItemIndex * pitch + rowA.firstVisibleItemScrollOffset

    /** From one row's top to the next's on the page of [rows], in pixels: they all have the one height. */
    private fun rowPitch(): Float = list.layoutInfo.visibleItemsInfo.let { (it[1].offset - it[0].offset).toFloat() }

    /** How far the page of [rows] has scrolled, in pixels. */
    private fun scrolledPage(): Float = list.firstVisibleItemIndex * rowPitch() + list.firstVisibleItemScrollOffset

    /** The focused tile's top, in dp from the screen's. */
    private fun focusedTop(): Float = compose.focused().getBoundsInRoot().top.value

    private fun keyDown(key: Key) = compose.onRoot().performKeyInput { keyDown(key) }

    private fun keyUp(key: Key) = compose.onRoot().performKeyInput { keyUp(key) }

    @Test
    fun theFirstTileTakesFocusFirst() {
        page()
        compose.focused().assertContentDescriptionEquals("A 1")
    }

    @Test
    fun downFromTheHeroEntersTheRowsAndUpScrollsBackToIt() {
        page(withHero = true)
        compose.onNodeWithTag("resume").assertIsFocused()
        compose.press(Key.DirectionDown)
        compose.focused().assertContentDescriptionEquals("A 1")
        // The row's header is at y = 172: the hero (400 dp) scrolled 400 + 28 - 172.
        compose.mainClock.advanceTimeBy(2_000)
        compose.waitForIdle()
        assertEquals(0, list.firstVisibleItemIndex)
        assertEquals(with(compose.density) { 256.dp.roundToPx() }.toFloat(), list.firstVisibleItemScrollOffset.toFloat(), 2f)
        compose.press(Key.DirectionUp)
        compose.onNodeWithTag("resume").assertIsFocused()
        compose.mainClock.advanceTimeBy(2_000)
        compose.waitForIdle()
        assertEquals(0, list.firstVisibleItemScrollOffset)
    }

    @Test
    fun rightMovesAlongAndStopsAtTheEnd() {
        page()
        compose.press(Key.DirectionRight, times = 3)
        compose.focused().assertContentDescriptionEquals("A 4")
        compose.press(Key.DirectionRight, times = 20)
        compose.focused().assertContentDescriptionEquals("A 20")
    }

    @Test
    fun leftFromTheFirstTileEntersTheRail() {
        page()
        compose.press(Key.DirectionRight)
        compose.press(Key.DirectionLeft)
        compose.focused().assertContentDescriptionEquals("A 1")
        compose.press(Key.DirectionLeft)
        assertEquals(1, rail.asked)
        compose.onNodeWithTag("rail").assertIsFocused()
    }

    @Test
    fun aHeldLeftStopsAtTheFirstTileAndOnlyAPressOpensTheRail() {
        page()
        compose.press(Key.DirectionRight)
        compose.mainClock.autoAdvance = false
        keyDown(Key.DirectionLeft)
        // The press moves back to A 1; held from 180 ms, Left goes no further, however long.
        compose.mainClock.advanceTimeBy(600)
        keyUp(Key.DirectionLeft)
        compose.mainClock.autoAdvance = true
        compose.waitForIdle()
        compose.focused().assertContentDescriptionEquals("A 1")
        assertEquals(0, rail.asked)
        compose.press(Key.DirectionLeft)
        assertEquals(1, rail.asked)
        compose.onNodeWithTag("rail").assertIsFocused()
    }

    @Test
    fun leftFromAScrolledRowsFirstTileStillGoesToTheRail() {
        page()
        // Row A scrolled along, then down to row B's first tile: row A's tiles are under the rail.
        compose.press(Key.DirectionRight, times = 4)
        compose.press(Key.DirectionDown)
        compose.focused().assertContentDescriptionEquals("B 100")
        compose.press(Key.DirectionLeft)
        compose.onNodeWithTag("rail").assertIsFocused()
    }

    @Test
    fun eachRowComesBackToItsTile() {
        page()
        compose.press(Key.DirectionRight, times = 2)
        compose.focused().assertContentDescriptionEquals("A 3")
        compose.press(Key.DirectionDown)
        compose.press(Key.DirectionRight, times = 3)
        compose.focused().assertContentDescriptionEquals("B 103")
        compose.press(Key.DirectionUp)
        compose.focused().assertContentDescriptionEquals("A 3")
        compose.press(Key.DirectionDown)
        compose.focused().assertContentDescriptionEquals("B 103")
    }

    @Test
    fun aPressMovesTheRowOver150MsEasedOut() {
        page()
        compose.press(Key.DirectionRight, times = 3)
        assertEquals(3 * pitch, scrolledA(), 1f)
        compose.mainClock.autoAdvance = false
        compose.onRoot().performKeyInput { pressKey(Key.DirectionRight) }
        // Eased out: well over half the way a third of the way through.
        compose.mainClock.advanceTimeBy(80)
        val partway = scrolledA() - 3 * pitch
        assertTrue("$partway of $pitch", partway > pitch / 2 && partway < pitch)
        compose.mainClock.advanceTimeBy(128)
        assertEquals(4 * pitch, scrolledA(), 1f)
        compose.focused().assertContentDescriptionEquals("A 5")
    }

    @Test
    fun aHeldKeySlidesTheRowEvenlyWhileFocusWaits() {
        page()
        compose.mainClock.autoAdvance = false
        keyDown(Key.DirectionRight)
        // The press moves to A 2; from 180 ms the row slides on by itself, a tile every 45 ms.
        compose.mainClock.advanceTimeBy(304)
        compose.focused().assertContentDescriptionEquals("A 2")
        val early = scrolledA()
        assertTrue("$early", early > 2 * pitch)
        compose.mainClock.advanceTimeBy(96)
        assertEquals(96f / KeyRepeatGate.MOVE_INTERVAL_MS * pitch, scrolledA() - early, pitch / 10)
        compose.focused().assertContentDescriptionEquals("A 2")
        keyUp(Key.DirectionRight)
        compose.mainClock.autoAdvance = true
        compose.waitForIdle()
    }

    @Test
    fun aSlideLandsOnTheTileUnderTheRingAndSettlesThere() {
        page()
        compose.mainClock.autoAdvance = false
        keyDown(Key.DirectionRight)
        compose.mainClock.advanceTimeBy(400)
        val under = scrolledA() / pitch
        keyUp(Key.DirectionRight)
        compose.mainClock.autoAdvance = true
        compose.waitForIdle()
        // Moving right, the tile under the ring rounded up: the row only ever settles forward.
        val landed = ceil(under - 0.01f).toInt()
        compose.focused().assertContentDescriptionEquals("A ${landed + 1}")
        assertEquals(landed * pitch, scrolledA(), 1f)
        // And from there the keys go on as ever.
        compose.press(Key.DirectionRight)
        compose.focused().assertContentDescriptionEquals("A ${landed + 2}")
    }

    @Test
    fun nearTheRowsEndAHeldKeyStepsOnToTheLastTile() {
        page()
        compose.mainClock.autoAdvance = false
        keyDown(Key.DirectionRight)
        // The slide reaches the row's end, where the tiles stop coming to the gutter; focus steps on from there.
        compose.mainClock.advanceTimeBy(2_000)
        compose.focused().assertContentDescriptionEquals("A 20")
        keyUp(Key.DirectionRight)
        compose.mainClock.autoAdvance = true
        compose.waitForIdle()
        compose.focused().assertContentDescriptionEquals("A 20")
    }

    @Test
    fun withReducedMotionAHeldKeyStepsATileAtATime() {
        page(reduced = true)
        compose.mainClock.autoAdvance = false
        keyDown(Key.DirectionRight)
        // The press, then a move every 45 ms from 180 ms: five more by 400 ms, each in place.
        compose.mainClock.advanceTimeBy(400)
        keyUp(Key.DirectionRight)
        compose.mainClock.autoAdvance = true
        compose.waitForIdle()
        compose.focused().assertContentDescriptionEquals("A 7")
        assertEquals(6 * pitch, scrolledA(), 1f)
    }

    @Test
    fun aPressGlidesThePageToTheNextRowOver150MsEasedOut() {
        rows()
        compose.focused().assertContentDescriptionEquals("R0 1")
        val rowPitch = rowPitch()
        compose.mainClock.autoAdvance = false
        compose.onRoot().performKeyInput { pressKey(Key.DirectionDown) }
        // Eased out: well over half the way a third of the way through.
        compose.mainClock.advanceTimeBy(80)
        val partway = scrolledPage()
        assertTrue("$partway of $rowPitch", partway > rowPitch / 2 && partway < rowPitch)
        compose.mainClock.advanceTimeBy(128)
        assertEquals(rowPitch, scrolledPage(), 1f)
        // The row's header at the line, so its tiles' tops at 172 + 24 + 14.
        compose.focused().assertContentDescriptionEquals("R1 101")
        assertEquals(210f, focusedTop(), 1f)
    }

    @Test
    fun aHeldDownGlidesThePageOnWithoutStoppingOrJumping() {
        rows()
        val rowPitch = rowPitch()
        compose.mainClock.autoAdvance = false
        keyDown(Key.DirectionDown)
        var last = scrolledPage()
        // 25 frames, 400 ms: the press, then a step every 45 ms from 180 ms, each glide running into the next.
        for (frame in 1..25) {
            compose.mainClock.advanceTimeByFrame()
            val moved = scrolledPage() - last
            last += moved
            assertTrue("frame $frame moved $moved of $rowPitch", moved < rowPitch / 2)
            // Once the held steps have begun (192 ms, gliding from the frame after), every frame moves on.
            if (frame > 13) assertTrue("frame $frame stood still", moved > 0f)
        }
        keyUp(Key.DirectionDown)
        compose.mainClock.autoAdvance = true
        compose.waitForIdle()
        // The press and five held steps: the sixth row, its tiles at the line.
        compose.focused().assertContentDescriptionEquals("R6 601")
        assertEquals(210f, focusedTop(), 1f)
    }

    @Test
    fun withReducedMotionAHeldDownStepsThePageARowAtATime() {
        rows(reduced = true)
        compose.mainClock.autoAdvance = false
        keyDown(Key.DirectionDown)
        compose.mainClock.advanceTimeBy(400)
        // Each step's row is at the line by the next frame: nothing glides.
        compose.focused().assertContentDescriptionEquals("R6 601")
        assertEquals(210f, focusedTop(), 1f)
        keyUp(Key.DirectionDown)
        compose.mainClock.autoAdvance = true
        compose.waitForIdle()
        assertEquals(210f, focusedTop(), 1f)
    }
}
