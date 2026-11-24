package app.pley.android.ui.phone.player

import android.graphics.Bitmap
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.captureToImage
import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.isRoot
import androidx.compose.ui.test.onFirst
import androidx.compose.ui.test.junit4.ComposeContentTestRule
import androidx.compose.ui.test.onRoot
import java.io.File
import org.junit.Assert.assertTrue

/**
 * What a screen drawn for a car must hold at each of the screens Android
 * Auto runs on (Google's car app quality criteria, UX-1 and UX-2): every
 * control on screen, 64 dp at least in the car's own dp, and none over
 * another. The screens are the Desktop Head Unit's test configurations
 * (developer.android.com/training/cars/parked/auto): small landscape,
 * the wide landscape strip and portrait, with a denser screen besides,
 * a car's tall screen whole and split, and the other shapes cars have.
 */
internal object CarScreenChecks {
    /** [labels]' controls, found by content description or text, checked as above. */
    fun controls(compose: ComposeContentTestRule, screen: String, labels: List<String>) {
        compose.waitForIdle()
        val density = compose.density.density
        val window = compose.onRoot().fetchSemanticsNode().boundsInRoot
        val found = labels.associateWith { label ->
            val nodes = compose.onAllNodes(matcher(label)).fetchSemanticsNodes()
            assertTrue("$screen: no $label", nodes.isNotEmpty())
            nodes.first().boundsInRoot
        }
        for ((label, bounds) in found) {
            assertTrue("$screen: $label at $bounds, outside $window", window.contains(bounds))
            val width = bounds.width / density
            val height = bounds.height / density
            assertTrue("$screen: $label is ${width.toInt()} × ${height.toInt()} dp", minOf(width, height) >= TARGET - 0.5f)
        }
        val list = found.entries.toList()
        for (i in list.indices) {
            for (j in i + 1 until list.size) {
                assertTrue("$screen: ${list[i].key} over ${list[j].key}", !list[i].value.overlaps(list[j].value))
            }
        }
    }

    /** [labels]' controls 24 dp apart from each other and from the screen's edges at least (UX-2), as touch targets. */
    fun apart(compose: ComposeContentTestRule, screen: String, labels: List<String>) {
        compose.waitForIdle()
        val density = compose.density.density
        val window = compose.onRoot().fetchSemanticsNode().boundsInRoot
        val found = labels.associateWith { label -> compose.onAllNodes(matcher(label)).fetchSemanticsNodes().first().boundsInRoot }
        val close = mutableListOf<String>()
        for ((label, bounds) in found) {
            val edge = minOf(bounds.left - window.left, bounds.top - window.top, window.right - bounds.right, window.bottom - bounds.bottom) / density
            if (edge < SPACE - 0.5f) close += "$label is ${edge.toInt()} dp from an edge"
        }
        val list = found.entries.toList()
        for (i in list.indices) {
            for (j in i + 1 until list.size) {
                val (a, b) = list[i].value to list[j].value
                val gap = maxOf(b.left - a.right, a.left - b.right, b.top - a.bottom, a.top - b.bottom) / density
                if (gap < SPACE - 0.5f) close += "${list[i].key} is ${gap.toInt()} dp from ${list[j].key}"
            }
        }
        val where = found.entries.joinToString { (label, b) -> "$label ${(b.top / density).toInt()}..${(b.bottom / density).toInt()}" }
        assertTrue("$screen, ${(window.height / density).toInt()} dp tall: ${close.joinToString("; ")} ($where)", close.isEmpty())
    }

    /**
     * A picture of the screen, kept when `PLEY_CAR_SHOTS` names a folder: to look at a layout, not to compare. The
     * page's window, also with a sheet open over it (Robolectric draws no other window).
     */
    fun save(compose: ComposeContentTestRule, name: String) {
        val folder = System.getenv("PLEY_CAR_SHOTS") ?: return
        val bitmap = compose.onAllNodes(isRoot()).onFirst().captureToImage().asAndroidBitmap()
        File(folder).mkdirs()
        File(folder, "$name.png").outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG, 100, it) }
    }

    private fun matcher(label: String): SemanticsMatcher = hasContentDescription(label) or hasText(label)

    private fun Rect.contains(other: Rect) =
        other.left >= left - 0.5f && other.top >= top - 0.5f && other.right <= right + 0.5f && other.bottom <= bottom + 0.5f

    /** The smallest touch target on a car's screen, and the least room between targets and from the screen's edges. */
    private const val TARGET = 64f
    private const val SPACE = 24f

    /** The Desktop Head Unit's test screens, as Robolectric qualifiers (the window Android Auto gives the app). */
    const val SMALL = "w800dp-h480dp-land-mdpi"
    const val WIDE = "w1920dp-h484dp-land-mdpi"
    const val PORTRAIT = "w1042dp-h1080dp-port-mdpi"

    /** 1280 × 720 at 240 dpi, as many wide head units report. */
    const val DENSE = "w853dp-h480dp-land-hdpi"

    /**
     * What Android Auto gives a parked app on a car's tall screen: 1042 × 991 px at 185 dpi on some, a little
     * wider than tall, and half of that height when the car splits its screen.
     */
    const val TALL = "w901dp-h857dp-land-185dpi"
    const val TALL_SPLIT = "w901dp-h428dp-land-185dpi"

    /**
     * Other shapes cars have: a wide, low screen across the dashboard (1920 × 720 at 240 dpi), a full HD one
     * (1920 × 1080 at 320 dpi), a tall portrait one (1080 × 1920 at 240 dpi), and the narrowest portrait window
     * (480 × 800 at 160 dpi).
     */
    const val ULTRAWIDE = "w1280dp-h480dp-land-hdpi"
    const val FULL_HD = "w960dp-h540dp-land-xhdpi"
    const val TALL_PORTRAIT = "w720dp-h1280dp-port-hdpi"
    const val NARROW = "w480dp-h800dp-port-mdpi"
}
