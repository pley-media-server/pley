package app.pley.android.ui.tv.library

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.offset
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.assertContentDescriptionEquals
import androidx.compose.ui.test.assertIsFocused
import androidx.compose.ui.test.isFocused
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onRoot
import androidx.compose.ui.test.performKeyInput
import androidx.compose.ui.test.pressKey
import androidx.compose.ui.unit.dp
import app.pley.android.designsystem.tile.ArtworkTileModel
import app.pley.android.designsystem.tile.TileKind
import app.pley.android.designsystem.tile.rememberSkeleton
import app.pley.android.model.ItemKind
import app.pley.android.model.ItemSummary
import app.pley.android.screens.library.AlphaIndex
import app.pley.android.ui.tv.kit.KeyRepeatGate
import app.pley.android.ui.tv.kit.LocalKeyRepeatGate
import app.pley.android.ui.tv.kit.keyRepeatGate
import app.pley.android.ui.tv.kit.rememberFocusMemory
import app.pley.android.ui.tv.theme.TvTheme
import kotlinx.coroutines.launch
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

/**
 * The A to Z on TV with key events (design spec §4.2): Right from the
 * grid's last column reaches the jump control, Up and Down step through
 * the letters, Select lands on the letter's first title, and an empty
 * letter lands on the next that has titles. A held Right stays in the
 * grid: only a press of its own reaches the jump (rule 5).
 */
@RunWith(RobolectricTestRunner::class)
@Config(qualifiers = "w960dp-h540dp-xhdpi")
class TvLetterJumpTest {
    @get:Rule
    val compose = createComposeRule()

    private val titles = List(40) { "Alien ${it + 1}" } + List(30) { "Batman ${it + 1}" } + List(50) { "Zorro ${it + 1}" }
    private val items = titles.mapIndexed { i, title -> ItemSummary(i + 1L, 1, ItemKind.Movie, title, title) }
    private val tiles = items.map { ArtworkTileModel(id = it.id, title = it.title, subtitle = "2024") }
    private val index = AlphaIndex.of(items)

    /** On the test's clock. The test sends no repeats, as the system would: none is waited for. */
    private val gate = KeyRepeatGate(firstRepeatMs = 60_000) { compose.mainClock.currentTime }

    private fun grid() {
        compose.setContent {
            TvTheme {
                val focus = remember { TvGridFocus() }
                val state = rememberTvGridState()
                val jump = remember { FocusRequester() }
                val scope = rememberCoroutineScope()
                CompositionLocalProvider(LocalKeyRepeatGate provides gate) {
                    Box(Modifier.fillMaxSize().keyRepeatGate(gate)) {
                        TvPosterGrid(
                            total = tiles.size,
                            tileAt = { tiles.getOrNull(it) },
                            keyAt = { tiles[it].id },
                            kind = TileKind.LibraryGrid,
                            onOpen = { _, _ -> },
                            memory = rememberFocusMemory(),
                            memoryPrefix = "grid",
                            skeleton = rememberSkeleton(animated = false),
                            state = state,
                            focus = focus,
                            onRight = {
                                jump.requestFocus()
                                true
                            },
                            modifier = Modifier.fillMaxSize(),
                        )
                        JumpControl(
                            index = index,
                            focus = focus,
                            requester = jump,
                            onJump = { position ->
                                scope.launch {
                                    state.scrollToItem(position - position % 7)
                                    withFrameNanos { }
                                    focus.focus(position)
                                }
                            },
                            modifier = Modifier.offset(x = 872.dp, y = 0.dp),
                        )
                    }
                }
            }
        }
        compose.waitForIdle()
    }

    @OptIn(ExperimentalTestApi::class)
    private fun press(key: Key, times: Int = 1) = repeat(times) {
        compose.onRoot().performKeyInput { pressKey(key) }
        compose.waitForIdle()
    }

    private fun focusedTitle(title: String) = compose.onNode(isFocused()).assertContentDescriptionEquals(title)

    @Test
    fun rightFromTheLastColumnReachesTheJumpControl() {
        grid()
        focusedTitle("Alien 1")
        press(Key.DirectionRight, 6)
        focusedTitle("Alien 7")
        press(Key.DirectionRight)
        compose.onNodeWithTag(JUMP_TAG).assertIsFocused()
    }

    @OptIn(ExperimentalTestApi::class)
    @Test
    fun aHeldRightStaysInTheGridAndAPressReachesTheJump() {
        grid()
        press(Key.DirectionRight, 5)
        compose.mainClock.autoAdvance = false
        compose.onRoot().performKeyInput { keyDown(Key.DirectionRight) }
        // The press moves to the last column; held from 180 ms, Right goes no further.
        compose.mainClock.advanceTimeBy(600)
        compose.onRoot().performKeyInput { keyUp(Key.DirectionRight) }
        compose.mainClock.autoAdvance = true
        compose.waitForIdle()
        focusedTitle("Alien 7")
        press(Key.DirectionRight)
        compose.onNodeWithTag(JUMP_TAG).assertIsFocused()
    }

    @Test
    fun aLetterLandsOnItsFirstTitle() {
        grid()
        press(Key.DirectionRight, 7)
        press(Key.DirectionDown)
        press(Key.DirectionCenter)
        focusedTitle("Batman 1")

        // Back to the control: it starts from the focused title's letter.
        press(Key.DirectionRight, 7)
        compose.onNodeWithTag(JUMP_TAG).assertIsFocused().assertContentDescriptionEquals("Jump to a letter: B")
        press(Key.DirectionDown, 24)
        press(Key.DirectionCenter)
        focusedTitle("Zorro 1")
    }

    @Test
    fun anEmptyLetterLandsOnTheNextThatHasTitles() {
        grid()
        press(Key.DirectionRight, 7)
        // A to C: no title starts with C, so the jump goes on to Z.
        press(Key.DirectionDown, 2)
        compose.onNodeWithTag(JUMP_TAG).assertContentDescriptionEquals("Jump to a letter: C")
        press(Key.DirectionCenter)
        focusedTitle("Zorro 1")
        assertEquals(70, index.jumpTarget(AlphaIndex.LETTERS.indexOf("C")))
    }
}
