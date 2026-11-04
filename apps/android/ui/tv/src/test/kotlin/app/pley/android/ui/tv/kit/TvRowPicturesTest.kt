package app.pley.android.ui.tv.kit

import android.content.Context
import android.graphics.Bitmap
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Modifier
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onRoot
import androidx.compose.ui.test.performKeyInput
import androidx.test.core.app.ApplicationProvider
import app.pley.android.common.Tier
import app.pley.android.designsystem.image.ArtworkImage
import app.pley.android.designsystem.image.ArtworkInterceptor
import app.pley.android.designsystem.image.ArtworkPolicy
import app.pley.android.designsystem.image.LocalArtworkHold
import app.pley.android.designsystem.image.LocalArtworkImageLoader
import app.pley.android.designsystem.image.addArtworkHold
import app.pley.android.designsystem.tile.ArtworkTileModel
import app.pley.android.designsystem.tile.TileKind
import app.pley.android.ui.tv.theme.TvTheme
import coil3.ImageLoader
import coil3.Uri
import coil3.asImage
import coil3.decode.DataSource
import coil3.fetch.FetchResult
import coil3.fetch.Fetcher
import coil3.fetch.ImageFetchResult
import coil3.memory.MemoryCache
import coil3.request.Options
import coil3.size.pxOrElse
import kotlinx.coroutines.Dispatchers
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

/**
 * Pictures under a held key (D-pad rule 5, as on the native Samsung app):
 * the tiles a row slides past ask for none, and those where the key comes
 * up ask then; the row asks for the pictures ahead of it, as every page's
 * rows do.
 */
@OptIn(ExperimentalTestApi::class)
@RunWith(RobolectricTestRunner::class)
@Config(qualifiers = "w960dp-h540dp-xhdpi")
class TvRowPicturesTest {
    @get:Rule
    val compose = createComposeRule()

    private val context: Context = ApplicationProvider.getApplicationContext()

    /** On the test's clock. The test sends no repeats, as the system would: none is waited for. */
    private val gate = KeyRepeatGate(firstRepeatMs = 60_000) { compose.mainClock.currentTime }

    /** The addresses fetched, without their widths. */
    private val fetched = mutableListOf<String>()

    private val loader: ImageLoader by lazy {
        ImageLoader.Builder(context)
            .components {
                addArtworkHold()
                add(ArtworkInterceptor(ArtworkPolicy(Tier.T2, 1920)))
                add(Pictures(fetched))
            }
            .memoryCache(MemoryCache.Builder().maxSizeBytes(32L shl 20).build())
            .diskCache(null)
            .coroutineContext(Dispatchers.Unconfined)
            .build()
    }

    private val tiles = (1L..40L).map { id ->
        ArtworkTileModel(id = id, title = "P $id", subtitle = "2024", image = ArtworkImage(listOf("$ORIGIN/$id")))
    }

    private fun row() {
        compose.setContent {
            TvTheme {
                CompositionLocalProvider(
                    LocalKeyRepeatGate provides gate,
                    LocalArtworkHold provides gate,
                    LocalArtworkImageLoader provides loader,
                ) {
                    Box(Modifier.fillMaxSize().keyRepeatGate(gate)) {
                        TvRow("p", "Pictures", tiles, TileKind.Landscape, onOpen = {}, initialFocus = true)
                    }
                }
            }
        }
        compose.waitForIdle()
    }

    @Test
    fun aRowAsksForThePicturesAheadOfIt() {
        row()
        // Four tiles show; the row asks for the next screen and a half too, before they compose.
        assertTrue("$fetched", fetched.containsAll((1..10).map { "$ORIGIN/$it" }))
    }

    @Test
    fun tilesASlidePassesAskForNothingAndWhereItLandsAsks() {
        row()
        compose.mainClock.autoAdvance = false
        compose.onRoot().performKeyInput { keyDown(Key.DirectionRight) }
        // The press moves a tile on, asking ahead as ever; the hold begins at 400 ms.
        compose.mainClock.advanceTimeBy(208)
        val before = fetched.toList()
        // A second and more: the slide passes some 30 tiles, far past what was asked for ahead.
        compose.mainClock.advanceTimeBy(1_300)
        assertEquals(before, fetched)
        compose.onRoot().performKeyInput { keyUp(Key.DirectionRight) }
        compose.mainClock.autoAdvance = true
        compose.waitForIdle()
        val landed = compose.focused().fetchSemanticsNode().config[SemanticsProperties.ContentDescription].single().removePrefix("P ")
        val asked = fetched.drop(before.size)
        assertTrue("$asked", "$ORIGIN/$landed" in asked)
        // Only around where it landed: the screen, and what is ahead of it.
        assertTrue("$asked", asked.size <= 16 && asked.none { it.removePrefix("$ORIGIN/").toInt() < landed.toInt() - 2 })
    }

    /** Fetches every address as a picture of the size asked for, remembering which. */
    private class Pictures(private val fetched: MutableList<String>) : Fetcher.Factory<Uri> {
        override fun create(data: Uri, options: Options, imageLoader: ImageLoader): Fetcher = object : Fetcher {
            override suspend fun fetch(): FetchResult {
                fetched += data.toString().substringBefore('?')
                val bitmap = Bitmap.createBitmap(options.size.width.pxOrElse { 1 }, options.size.height.pxOrElse { 1 }, Bitmap.Config.ARGB_8888)
                return ImageFetchResult(bitmap.asImage(), isSampled = false, dataSource = DataSource.NETWORK)
            }
        }
    }

    private companion object {
        const val ORIGIN = "http://pley.test/items"
    }
}
