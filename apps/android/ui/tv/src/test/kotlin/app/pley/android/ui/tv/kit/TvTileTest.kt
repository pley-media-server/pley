package app.pley.android.ui.tv.kit

import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Modifier
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onRoot
import androidx.compose.ui.test.performKeyInput
import app.pley.android.designsystem.tile.TileKind
import app.pley.android.ui.tv.theme.TvTheme
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

/**
 * Tiles: Select opens, and holding Select or pressing Menu gives the actions (rule 6);
 * a quiet tile (a row sliding, a grid gliding under a held key) shows none of its focus
 * look until the quiet ends.
 * Held direction keys are [TvRowFocusTest]'s.
 */
@OptIn(ExperimentalTestApi::class)
@RunWith(RobolectricTestRunner::class)
@Config(qualifiers = "w960dp-h540dp-xhdpi")
class TvTileTest {
    @get:Rule
    val compose = createComposeRule()

    private val gate = KeyRepeatGate { compose.mainClock.currentTime }
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
    fun aQuietTileShowsNoneOfItsFocusLookTillTheQuietEnds() = runTest {
        val quiet = TileQuiet()
        val look = TileFocusLook(this, gate, quiet).apply { reduced = true }
        look.focus(true)
        runCurrent()
        assertEquals(1f, look.shown())
        assertTrue(look.emphasized)
        // Its row slides: no ring, no growth, a regular caption.
        quiet.start()
        assertEquals(0f, look.shown())
        assertEquals(1f, look.scale())
        assertFalse(look.emphasized)
        quiet.end(reduced = true)
        assertEquals(1f, look.shown())
        assertTrue(look.emphasized)

        // Focus reaching a tile while quiet: it takes its look at once, unseen, and shows it as the quiet ends.
        quiet.start()
        val other = TileFocusLook(this, gate, quiet)
        other.focus(true)
        runCurrent()
        assertEquals(0f, other.shown())
        quiet.end(reduced = true)
        assertEquals(1f, other.shown())
    }
}
