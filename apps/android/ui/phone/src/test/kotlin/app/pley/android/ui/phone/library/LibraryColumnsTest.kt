package app.pley.android.ui.phone.library

import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.dp
import app.pley.android.data.GridDensity
import app.pley.android.designsystem.tile.TileKind
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/** The library grid's columns at each card size, on phones and tablets (design spec §3.2). */
class LibraryColumnsTest {
    /** A window [width] wide, less its 16 dp margins. */
    private fun columns(density: GridDensity, width: Int, margin: Int = 16) =
        libraryColumns(density, TileKind.Poster, (width - 2 * margin).dp)

    @Test
    fun aPhoneHasThreeColumnsAtMediumAsTheDesignSays() {
        assertEquals(3, columns(GridDensity.Medium, 412))
        assertEquals(3, columns(GridDensity.Medium, 393))
        assertEquals(3, columns(GridDensity.Medium, 360))
    }

    @Test
    fun smallerCardsFitMoreAndLargerFewer() {
        assertEquals(4, columns(GridDensity.Small, 412))
        assertEquals(2, columns(GridDensity.Large, 412))
        // Never fewer than two, however narrow.
        assertEquals(2, columns(GridDensity.Large, 280))
    }

    @Test
    fun widerWindowsGetMoreColumns() {
        // A medium-width tablet beside its 80 dp rail, 24 dp margins.
        val tablet = columns(GridDensity.Medium, 800 - 80, margin = 24)
        assertEquals(6, tablet)
        for (density in GridDensity.entries) {
            assertTrue(columns(density, 1200 - 80, margin = 32) > columns(density, 412))
        }
    }

    @Test
    fun cellsShareTheWidthAndNeverShrinkBelowTheirSlack() {
        val available = 380.dp
        val n = libraryColumns(GridDensity.Medium, TileKind.Poster, available)
        val width = cellWidth(n, available)
        assertEquals(118.67f, width.value, 0.01f)
        assertTrue(width >= TileKind.Poster.phoneMinWidth * 0.85f)
    }

    @Test
    fun everyCellIsTheSameWidth() {
        val cells = with(EqualCells(3)) { Density(2.625f).calculateCrossAxisCellSizes(998, 32) }
        assertEquals(listOf(311, 311, 311), cells)
        assertEquals(EqualCells(3), EqualCells(3))
    }
}
