package app.pley.android.ui.tv.kit

import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Modifier
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.assertContentDescriptionEquals
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onRoot
import androidx.compose.ui.test.performKeyInput
import app.pley.android.designsystem.tile.TileKind
import app.pley.android.ui.tv.theme.TvTheme
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

/**
 * Tiles: Select opens, holding Select or pressing Menu gives the actions (rule 6), and a
 * held direction key moves on the gate's own clock (rule 5).
 */
@OptIn(ExperimentalTestApi::class)
@RunWith(RobolectricTestRunner::class)
@Config(qualifiers = "w960dp-h540dp-xhdpi")
class TvTileTest {
    @get:Rule
    val compose = createComposeRule()

    private val gate = KeyRepeatGate(firstRepeatMs = 500) { compose.mainClock.currentTime }
    private val opened = mutableListOf<Long>()
    private val actions = mutableListOf<Long>()

    private fun row() {
        compose.setContent {
            TvTheme {
                CompositionLocalProvider(LocalKeyRepeatGate provides gate) {
                    Row(Modifier.fillMaxSize().keyRepeatGate(gate)) {
                        TvRow(
                            rowKey = "r",
                            title = "Row",
                            tiles = tiles("T", 20),
                            kind = TileKind.Poster,
                            onOpen = { opened += it.id },
                            onActions = { actions += it.id },
                            initialFocus = true,
                            state = rememberLazyListState(),
                        )
                    }
                }
            }
        }
        compose.waitForIdle()
    }

    @Test
    fun selectOpensTheTile() {
        row()
        compose.press(Key.DirectionCenter)
        assertEquals(listOf(1L), opened)
        assertTrue(actions.isEmpty())
    }

    @Test
    fun holdingSelectOpensTheActions() {
        row()
        compose.onRoot().performKeyInput {
            keyDown(Key.DirectionCenter)
            advanceEventTime(1_000)
            keyUp(Key.DirectionCenter)
        }
        compose.waitForIdle()
        assertEquals(listOf(1L), actions)
        assertTrue(opened.isEmpty())
    }

    @Test
    fun menuOpensTheActionsOnceHoweverLongItIsHeld() {
        row()
        compose.onRoot().performKeyInput {
            keyDown(Key.Menu)
            advanceEventTime(1_000)
            keyUp(Key.Menu)
        }
        compose.waitForIdle()
        assertEquals(listOf(1L), actions)
        assertTrue(opened.isEmpty())
    }

    @Test
    fun aHeldKeyMovesEvery45MsFrom180Ms() {
        row()
        compose.mainClock.autoAdvance = false
        compose.onRoot().performKeyInput { keyDown(Key.DirectionRight) }
        // The press moves at once; held, the key moves again from 180 ms, every 45 ms: five more by 400 ms.
        compose.mainClock.advanceTimeBy(400)
        compose.onRoot().performKeyInput { keyUp(Key.DirectionRight) }
        compose.mainClock.autoAdvance = true
        compose.waitForIdle()
        compose.focused().assertContentDescriptionEquals("T 7")
    }
}
