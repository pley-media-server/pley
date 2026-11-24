package app.pley.android.player.music.library

import android.content.Context
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.Canvas
import android.graphics.Color
import android.net.Uri
import androidx.test.core.app.ApplicationProvider
import java.io.ByteArrayOutputStream
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.GraphicsMode

/** Channels' logos on their plates in the car: their addresses, read back strictly, and how a plate is drawn. */
@RunWith(RobolectricTestRunner::class)
class LogoPlateTest {
    private val context = ApplicationProvider.getApplicationContext<Context>()
    private val authority = MusicArt.authority(context)

    @Test
    fun aPlatesAddressIsAsShortAsTheLogosAndNamesTheServersLogoAlone() {
        val uri = MusicArt.logoUri(context, "/api/live/channels/12/logo?v=0a1b2c3d")!!
        assertEquals("content://$authority/p/12/0a1b2c3d", uri.toString())
        assertEquals("/api/live/channels/12/logo?v=0a1b2c3d" to 12L, MusicArt.logoOf(context, uri))
        assertEquals("/api/live/channels/12/logo" to 12L, MusicArt.logoOf(context, MusicArt.logoUri(context, "/api/live/channels/12/logo")!!))
        // Only the server's logos: no item's picture, nothing else of the API, no address from elsewhere.
        for (path in listOf("/api/items/12/images/poster", "/api/live/channels/12/logo?v=ZZ", "/api/live/channels/12/logo?w=1", "https://elsewhere.example/logo.png")) {
            assertNull(path, MusicArt.logoUri(context, path))
        }
        for (text in listOf("p", "p/0", "p/-1", "p/1x", "p/12/0A1B", "p/12/0a/1", "p/%2E%2E", "p/12/../i", "p/99999999999999999999")) {
            assertNull(text, MusicArt.logoOf(context, Uri.parse("content://$authority/$text")))
        }
        assertNull(MusicArt.logoOf(context, Uri.parse("content://elsewhere/p/12")))
        // It is no address of another kind: an unplated logo isn't read as a plate, nor a plate as a server path.
        assertNull(MusicArt.logoOf(context, MusicArt.uri(context, "/api/live/channels/12/logo")!!))
        assertNull(MusicArt.pathOf(context, uri))
        assertNull(MusicArt.hueOf(context, uri))
    }

    @Test
    @GraphicsMode(GraphicsMode.Mode.NATIVE)
    fun aDarkLogoGoesOnALightPlateAndALightOneOnADarkPlateFittedInside() {
        // A wide black logo on nothing, 400 by 100: a light plate, the logo three quarters of its width.
        val dark = LogoPlate.png(logo(400, 100, Color.BLACK))!!.decoded()
        assertEquals(LogoPlate.SIZE, dark.width)
        assertEquals(LogoPlate.SIZE, dark.height)
        val middle = LogoPlate.SIZE / 2
        assertEquals(Color.BLACK, dark.getPixel(middle, middle))
        assertTrue(luminance(dark.getPixel(middle, 8)) > 0.8)
        // The logo stops short of the plate's sides, an eighth of it on each.
        assertTrue(luminance(dark.getPixel(LogoPlate.SIZE / 10, middle)) > 0.8)
        assertEquals(Color.BLACK, dark.getPixel(LogoPlate.SIZE / 8 + 4, middle))
        // Rounded: its corner is transparent.
        assertEquals(0, Color.alpha(dark.getPixel(0, 0)))

        val light = LogoPlate.png(logo(100, 100, Color.WHITE))!!.decoded()
        assertTrue(luminance(light.getPixel(middle, 8)) < 0.2)
        assertEquals(Color.WHITE, light.getPixel(middle, middle))

        // A colored logo of mid tones reads dark; a picture that isn't one, nothing.
        assertTrue(LogoPlate.isDark(Bitmap.createBitmap(10, 10, Bitmap.Config.ARGB_8888).apply { eraseColor(Color.rgb(200, 20, 20)) }))
        assertFalse(LogoPlate.isDark(Bitmap.createBitmap(10, 10, Bitmap.Config.ARGB_8888)))
        assertNull(LogoPlate.png(byteArrayOf(1, 2, 3)))
    }

    private fun logo(width: Int, height: Int, color: Int): ByteArray {
        val bitmap = Bitmap.createBitmap(width, height, Bitmap.Config.ARGB_8888)
        Canvas(bitmap).drawColor(color)
        return ByteArrayOutputStream().also { bitmap.compress(Bitmap.CompressFormat.PNG, 100, it) }.toByteArray()
    }

    private fun ByteArray.decoded(): Bitmap = BitmapFactory.decodeByteArray(this, 0, size)

    private fun luminance(pixel: Int) = (0.2126 * Color.red(pixel) + 0.7152 * Color.green(pixel) + 0.0722 * Color.blue(pixel)) / 255
}
