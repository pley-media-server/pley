package app.pley.android.ui.tv.livetv

import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.cancelChildren
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Test

/** The guide rows' turns to draw: a row waiting is never left without one. */
@OptIn(ExperimentalCoroutinesApi::class)
class TvGuideRowFillTest {
    @Test
    fun `a row waiting under an index another row takes, as the list reorders, draws rather than being dropped`() = runTest {
        val fill = RowFill()
        val drew = mutableListOf<String>()
        val moved = launch {
            fill.turn(3)
            drew += "moved"
        }
        runCurrent()
        // Recently watched reorders: another channel's row is at index 3 now, and waits its turn there.
        launch { fill.turn(3) }
        runCurrent()
        assertEquals(listOf("moved"), drew)
        assertFalse(moved.isCancelled)
        coroutineContext.cancelChildren()
    }
}
