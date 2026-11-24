package app.pley.android.designsystem.image

import androidx.compose.ui.graphics.Color
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import kotlin.math.roundToInt

class PlaceholderTest {
    /**
     * packages/placeholder's examples, which every app's gradient is held
     * to: each title's hue, and its two colors as CSS makes them.
     */
    @Test
    fun eachExampleIsThePlaceholderFiles() {
        val examples = placeholderFile.getValue("examples").jsonArray.map { it.jsonObject }
        assertTrue(examples.isNotEmpty())
        for (example in examples) {
            val seed = example.getValue("seed").jsonPrimitive.content
            assertEquals(seed, example.getValue("hue").jsonPrimitive.int, Placeholder.hue(seed))
            val (from, to) = Placeholder.colors(seed)
            assertNear(seed, hex(example.getValue("from").jsonPrimitive.content), bytes(from))
            assertNear(seed, hex(example.getValue("to").jsonPrimitive.content), bytes(to))
        }
        // The brush draws CSS's 135deg: another angle in the file needs it drawn again.
        assertEquals(135, placeholderFile.getValue("gradient").jsonObject.getValue("angle").jsonPrimitive.int)
    }

    /** Within a step of a byte each, as the apps round colors their own ways. */
    private fun assertNear(seed: String, expected: Triple<Int, Int, Int>, actual: Triple<Int, Int, Int>) {
        val off = listOf(expected.first - actual.first, expected.second - actual.second, expected.third - actual.third)
        assertTrue("$seed: $actual, not $expected", off.all { it in -1..1 })
    }

    private fun hex(color: String) = Triple(color.substring(1, 3).toInt(16), color.substring(3, 5).toInt(16), color.substring(5, 7).toInt(16))

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

    private fun rgb(r: Int, g: Int, b: Int) = Triple(r, g, b)

    private fun bytes(c: Color) = Triple((c.red * 255).roundToInt(), (c.green * 255).roundToInt(), (c.blue * 255).roundToInt())
}
