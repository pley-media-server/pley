package app.pley.android.ui.phone.player

import androidx.compose.ui.unit.sp
import app.pley.android.ui.phone.theme.PhoneTypography
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

/**
 * The car's type: no text under 24 sp on its screen, and its titles still
 * a step above each other and above body text, as on the phone.
 */
@RunWith(RobolectricTestRunner::class)
class CarTypeTest {
    private val floor = (24f / CAR_SCALE).sp
    private val car = PhoneTypography.forCar(floor)

    /** [sp] before scaling, as the car's screen draws it. */
    private fun shown(sp: Float) = sp * CAR_SCALE

    @Test
    fun noTextIsUnder24Sp() {
        val styles = with(car) {
            listOf(
                displayLarge, displayMedium, displaySmall, headlineLarge, headlineMedium, headlineSmall, titleLarge,
                titleMedium, titleSmall, bodyLarge, bodyMedium, bodySmall, labelLarge, labelMedium, labelSmall,
            )
        }
        for (style in styles) assertTrue("${style.fontSize}", shown(style.fontSize.value) >= 24f - 0.01f)
    }

    @Test
    fun titlesStandAboveBodyText() {
        val ramp = with(car) { listOf(bodyLarge, titleMedium, titleLarge, headlineSmall, headlineMedium, displaySmall) }
        for ((lower, higher) in ramp.zipWithNext()) {
            assertTrue("${lower.fontSize} under ${higher.fontSize}", higher.fontSize.value > lower.fontSize.value)
        }
        assertEquals(24f, shown(car.bodyLarge.fontSize.value), 0.01f)
        assertEquals(24f, shown(car.labelSmall.fontSize.value), 0.01f)
        assertEquals(34f, shown(car.headlineSmall.fontSize.value), 0.01f)
        // The line keeps its share of the size: a title on two lines doesn't run into itself.
        val title = car.titleMedium
        assertEquals(22f / 16f, title.lineHeight.value / title.fontSize.value, 0.01f)
    }

    @Test
    fun aStyleAlreadyLargerKeepsItsSize() {
        assertEquals(PhoneTypography.displaySmall, car.displaySmall)
    }
}
