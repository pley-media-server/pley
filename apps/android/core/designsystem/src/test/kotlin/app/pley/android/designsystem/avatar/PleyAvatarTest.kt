package app.pley.android.designsystem.avatar

import android.content.Context
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.PixelMap
import androidx.compose.ui.graphics.toPixelMap
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.test.captureToImage
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.test.core.app.ApplicationProvider
import app.pley.android.common.Tier
import app.pley.android.designsystem.PleyType
import app.pley.android.designsystem.image.ArtworkImage
import app.pley.android.designsystem.image.ArtworkInterceptor
import app.pley.android.designsystem.image.ArtworkPolicy
import app.pley.android.designsystem.image.LocalArtworkImageLoader
import coil3.ImageLoader
import coil3.network.HttpException
import coil3.network.NetworkResponse
import coil3.request.ErrorResult
import coil3.test.FakeImage
import coil3.test.FakeImageLoaderEngine
import kotlin.math.abs
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.awaitCancellation
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

/**
 * What an avatar draws (docs/THEMES.md, "Profile colors"): the picture once
 * it is in; before that, without one, and when it fails, the initial on the
 * profile's color, never an empty circle or the title's gradient.
 */
@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(qualifiers = "w960dp-h540dp-xhdpi")
class PleyAvatarTest {
    @get:Rule
    val compose = createComposeRule()

    private val context: Context = ApplicationProvider.getApplicationContext()
    private val picture = ArtworkImage(listOf("http://192.168.1.20:7539/api/users/2/avatar?v=1"))
    private val red = 0xFFE5484D.toInt()

    private fun loader(engine: FakeImageLoaderEngine): ImageLoader = ImageLoader.Builder(context)
        .components {
            add(ArtworkInterceptor(ArtworkPolicy(Tier.T0, 1920)))
            add(engine)
        }
        .memoryCache(null)
        .diskCache(null)
        .coroutineContext(Dispatchers.Unconfined)
        .build()

    /** Anna's avatar, 64 dp (128 px) across, its picture from [engine]. */
    private fun show(color: String?, picture: ArtworkImage?, engine: FakeImageLoaderEngine = FakeImageLoaderEngine.Builder().build()): PixelMap {
        compose.setContent {
            CompositionLocalProvider(LocalArtworkImageLoader provides loader(engine)) {
                PleyAvatar("Anna", color, picture, 64.dp, PleyType.Tv.PageTitle.copy(fontSize = 28.sp), Modifier.testTag("avatar"))
            }
        }
        compose.waitForIdle()
        return compose.onNodeWithTag("avatar").captureToImage().toPixelMap()
    }

    private fun near(a: Color, b: Color) = abs(a.red - b.red) < 0.04f && abs(a.green - b.green) < 0.04f && abs(a.blue - b.blue) < 0.04f

    /** The pixels about [color]. */
    private fun count(pixels: PixelMap, color: Color): Int {
        var n = 0
        for (x in 0 until pixels.width) for (y in 0 until pixels.height) if (near(pixels[x, y], color)) n++
        return n
    }

    /** Its color just inside the circle's top, above the initial; and some of the initial in its ink. */
    private fun assertInitialOn(pixels: PixelMap, fill: Color) {
        assertTrue("expected about $fill, got ${pixels[64, 10]}", near(pixels[64, 10], fill))
        assertTrue(count(pixels, ProfileLook.ink(fill)) > 40)
    }

    @Test
    fun withoutAPictureTheInitialIsOnTheColorPickedFromTheName() {
        // Dark on Anna's sky blue.
        assertEquals(Color(0xFF0B0D10), ProfileLook.ink(Color(0xFF38BDF8)))
        assertInitialOn(show(null, null), Color(0xFF38BDF8))
    }

    @Test
    fun withoutAPictureTheInitialIsOnTheProfilesOwnColor() {
        // White on a deep violet.
        assertEquals(Color.White, ProfileLook.ink(Color(0xFF7C3AED)))
        assertInitialOn(show("#7c3aed", null), Color(0xFF7C3AED))
    }

    @Test
    fun whileThePictureLoadsTheInitialShows() {
        val engine = FakeImageLoaderEngine.Builder().default { awaitCancellation() }.build()
        assertInitialOn(show("#f472b6", picture, engine), Color(0xFFF472B6))
    }

    @Test
    fun whenThePictureFailsTheInitialShows() {
        val engine = FakeImageLoaderEngine.Builder()
            .default { chain -> ErrorResult(null, chain.request, HttpException(NetworkResponse(code = 404))) }
            .build()
        assertInitialOn(show("#f472b6", picture, engine), Color(0xFFF472B6))
    }

    @Test
    fun aPictureThatLoadsIsDrawnAlone() {
        val engine = FakeImageLoaderEngine.Builder().default(FakeImage(width = 256, height = 256, color = red)).build()
        val pixels = show("#f472b6", picture, engine)
        assertTrue(near(Color(red), pixels[64, 64]))
        assertTrue(near(Color(red), pixels[64, 10]))
        assertEquals(0, count(pixels, ProfileLook.ink(Color(0xFFF472B6))))
    }
}
