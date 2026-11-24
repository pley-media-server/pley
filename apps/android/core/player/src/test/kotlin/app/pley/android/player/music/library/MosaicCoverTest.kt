package app.pley.android.player.music.library

import android.content.Context
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.Color
import android.net.Uri
import androidx.test.core.app.ApplicationProvider
import java.io.ByteArrayOutputStream
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.GraphicsMode

/** A playlist's mosaic in the car: its address, read back strictly, and how it is drawn. */
@RunWith(RobolectricTestRunner::class)
class MosaicCoverTest {
    private val context = ApplicationProvider.getApplicationContext<Context>()
    private val authority = MusicArt.authority(context)

    @Test
    fun aMosaicsAddressNamesTheServersCoversByIdAndVersion() {
        val paths = (1..5).map { "/api/items/5$it/images/poster?v=$it" }
        val uri = MusicArt.mosaicUri(context, paths, "Night drive")!!
        // The first four, its drawing first, and the hue of the playlist's placeholder.
        assertEquals("content://$authority/m/${MosaicCover.DRAWING}/51/1/52/2/53/3/54/4?h=${PlaceholderCover.hue("Night drive")}", uri.toString())
        assertEquals(paths.take(4), MusicArt.mosaicOf(context, uri))
        // A cover named twice counts once; with fewer than two of the server's usual form, no mosaic.
        assertEquals(listOf(paths[0], paths[1]), MusicArt.mosaicOf(context, MusicArt.mosaicUri(context, listOf(paths[0], paths[0], paths[1]), "x")!!))
        assertNull(MusicArt.mosaicUri(context, listOf(paths[0], paths[0]), "x"))
        assertNull(MusicArt.mosaicUri(context, listOf(paths[0], "/api/items/9/images/backdrop?v=1", "/api/items/9/images/poster", "https://elsewhere.example/a.jpg"), "x"))
        for (text in listOf("m/1", "m/1/51", "m/1/51/1", "m/1/51/1/52", "m/1/51/1/5x/2", "m/1/0/1/52/2", "m/1/51/1/52/v2", "m/2/51/1/52/2", "m/1/51/1/52/2/53/3/54/4/55/5", "m/1/51/1/%2F/2")) {
            assertNull(text, MusicArt.mosaicOf(context, Uri.parse("content://$authority/$text")))
        }
        assertNull(MusicArt.mosaicOf(context, Uri.parse("content://elsewhere/m/1/51/1/52/2")))
        assertNull(MusicArt.pathOf(context, uri))
    }

    @Test
    @GraphicsMode(GraphicsMode.Mode.NATIVE)
    fun fourCoversTakeAQuarterEachAndFewerRepeatTheFirst() {
        val (red, green, blue, white) = listOf(Color.RED, Color.GREEN, Color.BLUE, Color.WHITE).map { cover(it) }
        val quarter = MosaicCover.SIZE / 4
        fun corners(png: ByteArray): List<Int> {
            val image = BitmapFactory.decodeByteArray(png, 0, png.size)
            assertEquals(MosaicCover.SIZE, image.width)
            return listOf(image.getPixel(quarter, quarter), image.getPixel(3 * quarter, quarter), image.getPixel(quarter, 3 * quarter), image.getPixel(3 * quarter, 3 * quarter))
        }
        assertEquals(listOf(Color.RED, Color.GREEN, Color.BLUE, Color.WHITE), corners(MosaicCover.png(listOf(red, green, blue, white))!!))
        assertEquals(listOf(Color.RED, Color.GREEN, Color.GREEN, Color.RED), corners(MosaicCover.png(listOf(red, green))!!))
        assertEquals(listOf(Color.RED, Color.GREEN, Color.BLUE, Color.RED), corners(MosaicCover.png(listOf(red, byteArrayOf(1), green, blue))!!))
        // One that can be read is shown as it is; none, nothing.
        assertArrayEquals(red, MosaicCover.png(listOf(byteArrayOf(1), red)))
        assertNull(MosaicCover.png(listOf(byteArrayOf(1))))
    }

    private fun cover(color: Int): ByteArray {
        val bitmap = Bitmap.createBitmap(300, 200, Bitmap.Config.ARGB_8888).apply { eraseColor(color) }
        return ByteArrayOutputStream().also { bitmap.compress(Bitmap.CompressFormat.PNG, 100, it) }.toByteArray()
    }
}
