package app.pley.android.ui.tv.item

import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * What a title's page gives up so that what follows its buttons shows
 * whole: the room under the buttons first, then the summary's third line,
 * then the credits, then its second. A summary line takes 22 dp and the
 * credits 25; what follows comes 28 dp nearer at the most.
 */
class TvHeroFitTest {
    private fun fit(room: Dp, wanted: Int = 3, credits: Boolean = true) = fitIn(room, wanted, credits).let { Triple(it.lines, it.credits, it.closer) }

    @Test
    fun withRoomNothingIsGivenUp() {
        assertEquals(Triple(3, true, 0.dp), fit(91.dp))
        assertEquals(Triple(3, false, 0.dp), fit(66.dp, credits = false))
    }

    @Test
    fun theRoomUnderTheButtonsClosesUpBeforeALineGoes() {
        assertEquals(Triple(3, true, 11.dp), fit(80.dp))
        assertEquals(Triple(3, true, 28.dp), fit(63.dp))
    }

    @Test
    fun theThirdLineGoesBeforeTheCreditsAndTheCreditsBeforeTheSecond() {
        assertEquals(Triple(2, true, 9.dp), fit(60.dp))
        assertEquals(Triple(2, false, 4.dp), fit(40.dp))
        assertEquals(Triple(1, false, 12.dp), fit(10.dp))
    }

    @Test
    fun aShortSummaryAsksForNoMoreThanItsLines() {
        assertEquals(Triple(1, true, 0.dp), fit(47.dp, wanted = 1))
        assertEquals(Triple(1, false, 12.dp), fit(10.dp, wanted = 1))
        assertEquals(Triple(0, true, 5.dp), fit(20.dp, wanted = 0))
    }

    @Test
    fun whereNothingFitsOneLineStaysAndWhatFollowsIsAsNearAsItComes() {
        assertEquals(Triple(1, false, 28.dp), fit((-20).dp))
        assertEquals(Triple(0, false, 28.dp), fit((-40).dp, wanted = 0))
    }
}
