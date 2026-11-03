package app.pley.android.designsystem.image

import androidx.compose.ui.graphics.Color
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Test
import kotlin.math.roundToInt

class PlaceholderTest {
    /** Hues from apps/web/src/lib/images.ts's placeholderGradient, run in Node for these titles. */
    @Test
    fun huesAreTheWebsForTheSameTitles() {
        val web = mapOf(
            "Heat" to 176, "Alien" to 159, "" to 0, "Glasshouse" to 124, "Northern Lights" to 121,
            "Ashfall" to 265, "A Film With No Poster Yet" to 71, "Amélie" to 331, "千と千尋の神隠し" to 156,
            "The Quiet Harbor" to 303,
        )
        for ((title, hue) in web) assertEquals(title, hue, Placeholder.hue(title))
    }

    @Test
    fun colorsAreTheWebsHsl() {
        // "" → hsl(0 30% 22%) to hsl(40 35% 12%), as the web's test pins.
        val (from, to) = Placeholder.colors("")
        assertEquals(rgb(73, 39, 39), bytes(from))
        assertEquals(rgb(41, 34, 20), bytes(to))
        assertNotEquals(Placeholder.colors("Heat"), Placeholder.colors("Alien"))
        assertEquals(Placeholder.colors("Heat"), Placeholder.colors("Heat"))
    }

    @Test
    fun hslMatchesCss() {
        assertEquals(rgb(255, 0, 0), bytes(Placeholder.hsl(0f, 1f, 0.5f)))
        assertEquals(rgb(0, 255, 0), bytes(Placeholder.hsl(120f, 1f, 0.5f)))
        assertEquals(rgb(0, 0, 255), bytes(Placeholder.hsl(240f, 1f, 0.5f)))
    }

    @Test
    fun theDiagonalBlendsInSrgbAsCssDoes() {
        val from = 0xFF492727.toInt()
        val to = 0xFF29221A.toInt()
        assertEquals(from, GradientBitmaps.lerpArgb(from, to, 0f))
        assertEquals(to, GradientBitmaps.lerpArgb(from, to, 1f))
        // Halfway, each channel halfway: 0x49→0x29 is 0x39, 0x27→0x22 rounds to 0x25, 0x27→0x1A to 0x21.
        assertEquals(0xFF392521.toInt(), GradientBitmaps.lerpArgb(from, to, 0.5f))
        assertEquals(0x80000000.toInt(), GradientBitmaps.lerpArgb(0x00000000, 0xFF000000.toInt(), 0.5f))
    }

    private fun rgb(r: Int, g: Int, b: Int) = Triple(r, g, b)

    private fun bytes(c: Color) = Triple((c.red * 255).roundToInt(), (c.green * 255).roundToInt(), (c.blue * 255).roundToInt())
}
