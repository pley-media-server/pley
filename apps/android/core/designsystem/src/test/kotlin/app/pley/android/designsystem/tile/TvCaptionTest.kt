package app.pley.android.designsystem.tile

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.PixelMap
import androidx.compose.ui.graphics.toPixelMap
import androidx.compose.ui.platform.LocalLayoutDirection
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.test.captureToImage
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.LayoutDirection
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyType
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import kotlin.math.abs

/**
 * The TV caption ([tvCaption]) draws as the two lines of BasicText it
 * stands for: the same size and, pixel for pixel, the same text.
 */
@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(qualifiers = "w960dp-h540dp-xhdpi")
class TvCaptionTest {
    @get:Rule
    val compose = createComposeRule()

    @Test
    fun aTitleAndItsYear() = assertSame("Saltwind", "2024")

    @Test
    fun aTitleTooLongIsCutWithAnEllipsis() = assertSame("The Long Night of the Glasshouse Orchestra", "2019 · Drama, Mystery, Thriller")

    @Test
    fun theFocusedTitleIsBold() = assertSame("Saltwind and the Harbour Lights", "2024", emphasized = true)

    @Test
    fun withoutASubtitleItIsOneLine() = assertSame("Glasshouse", "")

    @Test
    fun accentsCyrillicAndPunctuation() = assertSame("Amélie’s Café – «Брат»", "1999 · Comédie")

    @Test
    fun aTitleInAnotherFontKeepsItsTallerLines() = assertSame("رحلة إلى الشاطئ البعيد في الليل", "٢٠٢٤")

    /**
     * In a right-to-left layout the lines sit at the right edge. BasicText
     * sat a little short of it, its box the text's width rounded up and a
     * half pixel added for the tracking, so this compares where the ink is:
     * the same rows, within 2 pixels across.
     */
    @Test
    fun inARightToLeftLayoutTheLinesSitAtTheRight() {
        val (node, text) = draw("Saltwind", "2024", direction = LayoutDirection.Rtl)
        assertEquals(text.height, node.height)
        for (line in 0 until 2) {
            val rows = line * node.height / 2 until (line + 1) * node.height / 2
            val ours = ink(node, rows)
            val theirs = ink(text, rows)
            assertEquals("line $line's rows", theirs.second, ours.second)
            assertTrue("line $line: $ours, BasicText's $theirs", abs(ours.first.first - theirs.first.first) <= 2 && abs(ours.first.last - theirs.first.last) <= 2)
            assertTrue("line $line is at the right: $ours", ours.first.last >= node.width - 3)
        }
    }

    @Test
    fun onlyRobotosScriptsAreDrawnByTheNode() {
        assertTrue(inCaptionFont("Amélie’s Café – «Брат» (1999) · Ω…"))
        assertFalse(inCaptionFont("رحلة"))
        assertFalse(inCaptionFont("千と千尋の神隠し"))
        assertFalse(inCaptionFont("Up 🎈"))
    }

    private fun assertSame(title: String, subtitle: String, emphasized: Boolean = false) {
        val (node, text) = draw(title, subtitle, emphasized)
        assertEquals("width", text.width, node.width)
        assertEquals("height", text.height, node.height)
        assertTrue("the caption draws its text", ink(node, 0 until node.height).first.count() > 10)
        val off = differing(node, text)
        assertTrue("$off of ${node.width * node.height} pixels differ from BasicText's", off <= node.width * node.height / 200)
    }

    /** The caption and, under it, what TileCaption was on TV: a line of BasicText for each; both on the page's background. */
    private fun draw(
        title: String,
        subtitle: String,
        emphasized: Boolean = false,
        direction: LayoutDirection = LayoutDirection.Ltr,
    ): Pair<ImageBitmap, ImageBitmap> {
        val width = TileKind.Poster.tvWidth
        compose.setContent {
            CompositionLocalProvider(LocalLayoutDirection provides direction) {
                Column {
                    Box(Modifier.width(width).background(PleyColors.Bg).testTag("node")) {
                        TileCaption(title, subtitle, TileStyle.Tv, emphasized = emphasized)
                    }
                    Box(Modifier.width(width).background(PleyColors.Bg).testTag("text")) {
                        Column {
                            val style = TileStyle.Tv
                            BasicText(
                                title,
                                style = style.captionTitle.copy(color = PleyColors.Fg, fontWeight = if (emphasized) PleyType.Bold else style.captionTitle.fontWeight),
                                maxLines = 1,
                                overflow = TextOverflow.Ellipsis,
                            )
                            if (subtitle.isNotEmpty()) {
                                BasicText(subtitle, style = style.captionSubtitle.copy(color = PleyColors.Muted), maxLines = 1, overflow = TextOverflow.Ellipsis)
                            }
                        }
                    }
                }
            }
        }
        return compose.onNodeWithTag("node").captureToImage() to compose.onNodeWithTag("text").captureToImage()
    }

    /** The columns and rows of [rows] where [image] differs from its top right pixel, its background. */
    private fun ink(image: ImageBitmap, rows: IntRange): Pair<IntRange, IntRange> {
        val pixels = image.toPixelMap()
        val background = pixels[image.width - 1, 0]
        var x0 = Int.MAX_VALUE
        var x1 = -1
        var y0 = Int.MAX_VALUE
        var y1 = -1
        for (y in rows) {
            for (x in 0 until image.width) {
                if (far(pixels, x, y, background)) {
                    x0 = minOf(x0, x)
                    x1 = maxOf(x1, x)
                    y0 = minOf(y0, y)
                    y1 = maxOf(y1, y)
                }
            }
        }
        return x0..x1 to y0..y1
    }

    /** Pixels that differ by more than a little in any channel. */
    private fun differing(a: ImageBitmap, b: ImageBitmap): Int {
        val pa = a.toPixelMap()
        val pb = b.toPixelMap()
        var n = 0
        for (y in 0 until a.height) for (x in 0 until a.width) if (far(pa, x, y, pb[x, y])) n++
        return n
    }

    private fun far(pixels: PixelMap, x: Int, y: Int, other: androidx.compose.ui.graphics.Color): Boolean {
        val p = pixels[x, y]
        return abs(p.alpha - other.alpha) > 0.1f || abs(p.red - other.red) > 0.1f || abs(p.green - other.green) > 0.1f || abs(p.blue - other.blue) > 0.1f
    }
}
