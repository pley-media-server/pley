package app.pley.android.ui.phone.kit

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import app.pley.android.designsystem.tile.TileKind
import app.pley.android.ui.phone.theme.PhoneMetrics
import app.pley.android.ui.phone.theme.WidthClass
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** Card widths, grid columns, page metrics and avatar colors, as the design spec and the web give them. */
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

    @Test
    fun avatarsUseTheWebsColours() {
        assertEquals(Color(0xFFA78BFA), AvatarColors.of("Alex", "#a78bfa"))
        assertEquals(Color(0xFFAABBCC), AvatarColors.of("Alex", "abc"))
        // The web's hash: "A" is 65, and 65 % 8 is 1.
        assertEquals(AvatarColors.palette[1], AvatarColors.of("A", null))
        assertEquals(AvatarColors.palette[1], AvatarColors.of("A", "not a color"))
    }

    @Test
    fun avatarTextIsDarkOnLightColours() {
        assertEquals(Color.White, AvatarColors.textOn(Color(0xFF7C3AED)))
        assertEquals(Color(0xFF0B0D10), AvatarColors.textOn(Color(0xFFFACC15)))
        assertEquals(Color(0xFF0B0D10), AvatarColors.textOn(Color(0xFF38BDF8)))
    }
}
