package app.pley.android.ui.phone.home

import androidx.compose.ui.graphics.Color
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The home banner's bottom scrim: as it always was on a phone's tall
 * banner, and 75 % dark by the words' top where a short screen (a car's)
 * puts them higher up.
 */
class HomeBannerScrimTest {
    private fun stops(wordsTop: Float) = scrimStops(wordsTop, Color.Black).map { it.first }

    @Test
    fun aPhonesTallBannerKeepsItsScrim() {
        val expected = listOf(0.3f, 0.615f, 0.895f)
        // Words low on the banner, and before they are measured.
        assertStops(expected, stops(0.7f))
        assertStops(expected, stops(-1f))
    }

    @Test
    fun aShortScreensWordsSitOnTheDarkPart() {
        val (clear, dark, full) = stops(0.35f)
        assertEquals(0.35f, dark, 0.001f)
        assertTrue(clear < dark && dark < full && full <= 1f)
        // Even words at the very top get a scrim that rises to them, in order.
        val (c, d, f) = stops(0.05f)
        assertTrue(c >= 0f && c <= d && d < f && f <= 1f)
    }

    private fun assertStops(expected: List<Float>, actual: List<Float>) {
        expected.zip(actual).forEach { (e, a) -> assertEquals(e, a, 0.001f) }
    }
}
