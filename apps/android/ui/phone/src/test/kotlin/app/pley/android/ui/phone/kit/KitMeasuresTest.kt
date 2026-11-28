package app.pley.android.ui.phone.kit

import androidx.compose.ui.unit.dp
import app.pley.android.designsystem.tile.TileKind
import app.pley.android.ui.phone.theme.PhoneMetrics
import app.pley.android.ui.phone.theme.WidthClass
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** Card widths, grid columns and page metrics, as the design spec and the web give them. */
class KitMeasuresTest {
    /** A 412 dp phone less its two 16 dp margins. */
    private val phone = 380.dp

    @Test
    fun aPhoneRowShowsThreePostersAndAPeek() {
        assertEquals(118.dp, rowCardWidth(TileKind.Poster, phone))
    }

    @Test
    fun aNarrowerPhoneShrinksCardsALittleRatherThanShowingTwo() {
        // A 393 dp phone (Pixel 7 and the like): three posters of 112, not two of 174.
        assertEquals(112.dp, rowCardWidth(TileKind.Poster, 361.dp))
        assertEquals(3, gridColumns(TileKind.LibraryGrid, 361.dp))
    }

    @Test
    fun landscapeCardsKeepTheirWidthWhenOnlyOneWouldFit() {
        // The mockup's Continue Watching: 236 wide, the next card peeking.
        assertEquals(236.dp, rowCardWidth(TileKind.Landscape, phone))
    }

    @Test
    fun cardsFillTheRowWithWholeCards() {
        // 600 dp of page: as many posters as fit, never a cut one, none much under the minimum.
        val width = rowCardWidth(TileKind.Poster, 600.dp)
        val perPage = ((600.dp + 12.dp) / (width + 12.dp)).toInt()
        assertTrue(width >= 118.dp * (1f - FIT_SLACK))
        assertTrue(width * perPage + 12.dp * (perPage - 1) <= 600.dp)
    }

    @Test
    fun wideScreensGrowCardsUpToOneAndAThirdOfTheirSize() {
        val wide = rowCardWidth(TileKind.Poster, 2000.dp)
        assertTrue(wide >= (118 * 1.35f).dp - 20.dp)
        assertTrue(wide <= (118 * 1.35f).dp + 20.dp)
    }

    @Test
    fun gridsHaveAtLeastTwoColumns() {
        assertEquals(3, gridColumns(TileKind.LibraryGrid, phone))
        assertEquals(2, gridColumns(TileKind.LibraryGrid, 200.dp))
        assertEquals(2, gridColumns(TileKind.Landscape, phone))
    }

    @Test
    fun marginsFollowTheWidthClass() {
        assertEquals(WidthClass.Compact, PhoneMetrics.of(412.dp).widthClass)
        assertEquals(16.dp, PhoneMetrics.of(412.dp).margin)
        assertFalse(PhoneMetrics.of(412.dp).rail)
        val tablet = PhoneMetrics.of(800.dp)
        assertEquals(24.dp, tablet.margin)
        assertTrue(tablet.rail)
        // Beside the 80 dp rail.
        assertEquals(720.dp, tablet.contentWidth)
        assertEquals(32.dp, PhoneMetrics.of(1280.dp).margin)
    }
}
