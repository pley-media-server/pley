package app.pley.android.designsystem.tile

import android.content.Context
import android.graphics.Bitmap
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.PixelMap
import androidx.compose.ui.graphics.painter.ColorPainter
import androidx.compose.ui.graphics.toPixelMap
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.test.IdlingResource
import androidx.compose.ui.test.captureToImage
import androidx.compose.ui.test.hasClickAction
import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.ui.unit.dp
import androidx.test.core.app.ApplicationProvider
import app.pley.android.common.Tier
import app.pley.android.common.formatWords
import app.pley.android.designsystem.LocalReducedMotion
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.image.ArtworkImage
import app.pley.android.designsystem.image.ArtworkInterceptor
import app.pley.android.designsystem.image.ArtworkLoad
import app.pley.android.designsystem.image.ArtworkPolicy
import app.pley.android.designsystem.image.LocalArtworkImageLoader
import app.pley.android.designsystem.image.Placeholder
import app.pley.android.designsystem.image.TitleKey
import app.pley.android.designsystem.image.TitleLayouts
import app.pley.android.designsystem.image.artworkPlaceholder
import app.pley.android.designsystem.image.shownArtwork
import app.pley.android.model.ItemKind
import app.pley.android.model.ItemSummary
import app.pley.android.model.UserState
import coil3.ImageLoader
import coil3.asImage
import coil3.compose.AsyncImagePainter
import coil3.decode.DataSource
import coil3.intercept.Interceptor
import coil3.network.HttpException
import coil3.network.NetworkResponse
import coil3.request.ErrorResult
import coil3.request.ImageRequest
import coil3.request.SuccessResult
import coil3.request.crossfade
import coil3.request.transitionFactory
import coil3.test.FakeImage
import coil3.test.FakeImageLoaderEngine
import coil3.transition.CrossfadeTransition
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.awaitCancellation
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import kotlin.math.abs

@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(qualifiers = "w960dp-h540dp-xhdpi")
class ArtworkTileTest {
    @get:Rule
    val compose = createComposeRule()

    private val context: Context = ApplicationProvider.getApplicationContext()
    private val origin = "http://192.168.1.20:7539"
    private val red = 0xFFE5484D.toInt()

    /** Placeholders' titles come from a thread of their own: the tests wait for it as they wait for Compose. */
    @Before
    fun waitForTitles() = compose.registerIdlingResource(object : IdlingResource {
        override val isIdleNow: Boolean get() = TitleLayouts.idle
    })

    @After
    fun forgetProbe() {
        TileProbe.listener = null
    }

    /** A loader that has only [present] (as a solid [red] picture) and a 404 for anything else. */
    private fun loader(vararg present: String): ImageLoader {
        val engine = FakeImageLoaderEngine.Builder()
            .intercept({ it in present }, FakeImage(width = 216, height = 324, color = red))
            .default { chain -> ErrorResult(null, chain.request, HttpException(NetworkResponse(code = 404))) }
            .build()
        return ImageLoader.Builder(context)
            .components {
                add(ArtworkInterceptor(ArtworkPolicy(Tier.T0, 1920)))
                add(engine)
            }
            .memoryCache(null)
            .diskCache(null)
            .coroutineContext(Dispatchers.Unconfined)
            .build()
    }

    private fun model(id: Long = 1, title: String = "Northern Lights", progress: Float = 0f) = ArtworkTileModel(
        id = id,
        title = title,
        subtitle = "2024",
        image = ArtworkImage(listOf("$origin/api/items/$id/images/poster?v=1", "$origin/api/items/9/images/poster")),
        progress = progress,
        description = "$title, 2024",
    )

    private fun pixel(tag: String, x: Int, y: Int): Color = compose.onNodeWithTag(tag).captureToImage().toPixelMap()[x, y]

    private fun assertNear(expected: Color, actual: Color) {
        val close = abs(expected.red - actual.red) < 0.04f && abs(expected.green - actual.green) < 0.04f &&
            abs(expected.blue - actual.blue) < 0.04f
        assertTrue("expected about $expected, got $actual", close)
    }

    @Test
    fun theGradientStaysWhenNoAddressLoads() {
        val model = model()
        compose.setContent {
            CompositionLocalProvider(LocalArtworkImageLoader provides loader()) {
                ArtworkTile(model, TileKind.Poster, TileStyle.Tv, Modifier.tvTileSize(TileKind.Poster).testTag("tile"))
            }
        }
        compose.waitForIdle()
        // Near the top left corner the 135° gradient is at its first colour.
        assertNear(Placeholder.colors(model.title).first, pixel("tile", 14, 14))
    }

    @Test
    fun theGradientRunsToItsSecondColourBlendedInSrgb() {
        val model = model()
        compose.setContent {
            CompositionLocalProvider(LocalArtworkImageLoader provides loader()) {
                ArtworkTile(model, TileKind.Poster, TileStyle.Tv, Modifier.tvTileSize(TileKind.Poster).testTag("tile"))
            }
        }
        compose.waitForIdle()
        val image = compose.onNodeWithTag("tile").captureToImage().toPixelMap()
        val (from, to) = Placeholder.colors(model.title)
        // Near the bottom right corner, about its second colour; halfway along the diagonal (top right, clear of
        // the title in the middle), halfway between the two in sRGB, as CSS blends them.
        assertNear(to, image[image.width - 14, image.height - 14])
        val half = Color((from.red + to.red) / 2f, (from.green + to.green) / 2f, (from.blue + to.blue) / 2f)
        assertNear(half, image[image.width * 9 / 10, image.height / 10])
    }

    @Test
    fun aPictureShownBeforeWaitsInThePlainWell() {
        val model = model(id = 5, title = "Saltwind")
        // Its picture is on its way and doesn't come yet; it loaded once before.
        val engine = FakeImageLoaderEngine.Builder().default { awaitCancellation() }.build()
        val interceptor = ArtworkInterceptor(ArtworkPolicy(Tier.T0, 1920))
        val loader = ImageLoader.Builder(context)
            .components {
                add(interceptor)
                add(engine)
            }
            .memoryCache(null)
            .diskCache(null)
            .coroutineContext(Dispatchers.Unconfined)
            .build()
        loader.shownArtwork!!.add(model.image!!)
        compose.setContent {
            CompositionLocalProvider(LocalArtworkImageLoader provides loader) {
                ArtworkTile(model, TileKind.Poster, TileStyle.Tv, Modifier.tvTileSize(TileKind.Poster).testTag("tile"))
            }
        }
        compose.waitForIdle()
        // No title's gradient while it comes back: the well, as under the picture.
        assertNear(PleyColors.Surface2, pixel("tile", 14, 14))
        assertEquals(0, textLines(compose.onNodeWithTag("tile").captureToImage().toPixelMap()))
    }

    @Test
    fun withReducedMotionAPictureDoesntFadeIn() {
        val model = model(id = 7, title = "Stillwater")
        val asked = mutableListOf<ImageRequest>()
        val engine = FakeImageLoaderEngine.Builder().default(FakeImage(width = 216, height = 324, color = red)).build()
        // Where pictures fade (not T0), what the loader asks for after choosing the address.
        val loader = ImageLoader.Builder(context)
            .components {
                add(ArtworkInterceptor(ArtworkPolicy(Tier.T1, 1920)))
                add(Interceptor { chain -> chain.proceed().also { asked += chain.request } })
                add(engine)
            }
            .memoryCache(null)
            .diskCache(null)
            .coroutineContext(Dispatchers.Unconfined)
            .build()
        compose.setContent {
            CompositionLocalProvider(LocalArtworkImageLoader provides loader, LocalReducedMotion provides true) {
                ArtworkTile(model, TileKind.Poster, TileStyle.Tv, Modifier.tvTileSize(TileKind.Poster).testTag("tile"))
            }
        }
        compose.waitForIdle()
        assertTrue(asked.isNotEmpty())
        assertTrue(asked.none { it.transitionFactory is CrossfadeTransition.Factory })
    }

    @Test
    fun aPictureShownBeforeThatNoLongerLoadsHasItsGradient() {
        val model = model(id = 6, title = "Driftline")
        val loader = loader()
        loader.shownArtwork!!.add(model.image!!)
        compose.setContent {
            CompositionLocalProvider(LocalArtworkImageLoader provides loader) {
                ArtworkTile(model, TileKind.Poster, TileStyle.Tv, Modifier.tvTileSize(TileKind.Poster).testTag("tile"))
            }
        }
        compose.waitForIdle()
        assertNear(Placeholder.colors(model.title).first, pixel("tile", 14, 14))
    }

    @Test
    fun aWordWiderThanThePosterStaysWhole() {
        // "Extraterrestrial" in Title's 16 sp is wider than a poster's 84 dp of room: smaller, rather than broken.
        compose.setContent {
            CompositionLocalProvider(LocalArtworkImageLoader provides loader()) {
                ArtworkTile(model(title = "Extraterrestrial"), TileKind.Poster, TileStyle.Tv, Modifier.tvTileSize(TileKind.Poster).testTag("tile"))
            }
        }
        compose.waitForIdle()
        assertEquals(1, textLines(compose.onNodeWithTag("tile").captureToImage().toPixelMap()))
    }

    /** Lines of light text in [image]: bands of rows with a light pixel, 4 px or more apart, inside the rounded corners. */
    private fun textLines(image: PixelMap): Int {
        var lines = 0
        var gap = -1
        for (y in 16 until image.height - 16) {
            val light = (16 until image.width - 16).any { x -> image[x, y].let { it.alpha > 0.9f && maxOf(it.red, it.green, it.blue) > 0.5f } }
            if (light) {
                if (gap == -1 || gap >= 4) lines++
                gap = 0
            } else if (gap >= 0) {
                gap++
            }
        }
        return lines
    }

    /** A loader whose every picture is [bitmap]. */
    private fun loaderOf(bitmap: Bitmap): ImageLoader = ImageLoader.Builder(context)
        .components { add(FakeImageLoaderEngine.Builder().intercept({ true }, bitmap.asImage()).build()) }
        .memoryCache(null)
        .diskCache(null)
        .coroutineContext(Dispatchers.Unconfined)
        .build()

    @Test
    fun theWellStaysUnderAPictureWithTransparencyOnly() {
        // Clear: the well shows through it.
        val clear = Bitmap.createBitmap(216, 324, Bitmap.Config.ARGB_8888)
        // Opaque (as a JPEG decodes): the picture alone, no well drawn under it.
        val opaque = Bitmap.createBitmap(216, 324, Bitmap.Config.ARGB_8888).apply {
            eraseColor(red)
            setHasAlpha(false)
        }
        compose.setContent {
            Column {
                CompositionLocalProvider(LocalArtworkImageLoader provides loaderOf(clear)) {
                    ArtworkTile(model(id = 1), TileKind.Poster, TileStyle.Tv, Modifier.tvTileSize(TileKind.Poster).testTag("clear"))
                }
                CompositionLocalProvider(LocalArtworkImageLoader provides loaderOf(opaque)) {
                    ArtworkTile(model(id = 2), TileKind.Poster, TileStyle.Tv, Modifier.tvTileSize(TileKind.Poster).testTag("opaque"))
                }
            }
        }
        compose.waitForIdle()
        assertNear(PleyColors.Surface2, pixel("clear", 108, 162))
        assertNear(Color(red), pixel("opaque", 108, 162))
    }

    @Test
    fun aFallbackAddressThatLoadsReplacesTheGradient() {
        compose.setContent {
            CompositionLocalProvider(LocalArtworkImageLoader provides loader("$origin/api/items/9/images/poster?w=240")) {
                ArtworkTile(model(), TileKind.Poster, TileStyle.Tv, Modifier.tvTileSize(TileKind.Poster).testTag("tile"))
            }
        }
        compose.waitForIdle()
        assertNear(Color(red), pixel("tile", 108, 162))
    }

    @Test
    fun progressIsDrawnAlongTheBottom() {
        compose.setContent {
            CompositionLocalProvider(LocalArtworkImageLoader provides loader("$origin/api/items/9/images/poster?w=240")) {
                ArtworkTile(model(progress = 0.5f), TileKind.Poster, TileStyle.Tv, Modifier.tvTileSize(TileKind.Poster).testTag("tile"))
            }
        }
        compose.waitForIdle()
        // 216 by 324 px at xhdpi; the bar is the bottom 8 px: accent to half, then the dark track.
        assertNear(Color(0xFF38BDF8), pixel("tile", 52, 320))
        val track = pixel("tile", 166, 320)
        assertTrue("track $track", track.red < 0.45f)
    }

    @Test
    fun aPlaceholdersTitleIsLaidOutAwayFromTheMainThreadAndDrawn() {
        compose.setContent {
            CompositionLocalProvider(LocalArtworkImageLoader provides loader()) {
                ArtworkTile(model(title = "Northern Lights"), TileKind.Poster, TileStyle.Tv, Modifier.tvTileSize(TileKind.Poster).testTag("tile"))
            }
        }
        compose.waitForIdle()
        val image = compose.onNodeWithTag("tile").captureToImage().toPixelMap()
        // The title, in light text across the middle of the gradient (darker in every channel).
        var light = 0
        for (y in image.height / 3 until image.height * 2 / 3) {
            for (x in 0 until image.width) {
                val p = image[x, y]
                if (p.red > 0.6f && p.green > 0.6f && p.blue > 0.6f) light++
            }
        }
        assertTrue("$light light pixels", light > 100)
    }

    /**
     * A tile a lazy list uses again for another title, measured ahead of the
     * frame that shows it, asks for its new title as it's measured, as a new
     * tile does, so that it's ready by then. Here nothing draws it.
     */
    @Test
    fun aPlaceholderGivenAnotherTitleAsksForItAsItIsMeasured() {
        var title by mutableStateOf("A first title for a tile used again")
        val style = TileStyle.Tv.placeholderText
        val load = ArtworkLoad()
        compose.setContent {
            Box(Modifier.drawWithContent { }) {
                Box(Modifier.size(PlaceholderSide).artworkPlaceholder(title, style, load))
            }
        }
        compose.waitForIdle()
        assertNotNull(TitleLayouts.kept(titleKey(title, style)))
        title = "A second title for a tile used again"
        compose.waitForIdle()
        assertNotNull(TitleLayouts.kept(titleKey(title, style)))
    }

    /** What a placeholder [PlaceholderSide] square lays [title] out for: the box inside its 12 dp padding. */
    private fun titleKey(title: String, style: TextStyle): TitleKey = with(compose.density) {
        val inside = PlaceholderSide.roundToPx() - 2 * 12.dp.roundToPx()
        TitleKey(title, style, inside, inside, density, fontScale, LayoutDirection.Ltr)
    }

    @Test
    fun aTileWaitingForItsPictureIsRoundedWithoutAClip() = assertRoundCorners(loader())

    @Test
    fun aTileWithItsPictureIsRounded() = assertRoundCorners(loader("$origin/api/items/9/images/poster?w=240"), picture = true)

    /**
     * A tile on magenta, with its progress bar: its corners (16 px at xhdpi)
     * show the magenta, the gradient's or the picture's, and the bar's, and
     * inside them the tile is drawn.
     */
    private fun assertRoundCorners(loader: ImageLoader, picture: Boolean = false) {
        val magenta = Color(0xFFFF00FF)
        compose.setContent {
            CompositionLocalProvider(LocalArtworkImageLoader provides loader) {
                Box(Modifier.background(magenta).testTag("ground")) {
                    ArtworkTile(model(progress = 0.5f), TileKind.Poster, TileStyle.Tv, Modifier.tvTileSize(TileKind.Poster))
                }
            }
        }
        compose.waitForIdle()
        val image = compose.onNodeWithTag("ground").captureToImage().toPixelMap()
        val (w, h) = image.width to image.height
        // Top corners: outside the arc; bottom corners, the bar's too.
        for ((x, y) in listOf(1 to 1, w - 2 to 1, 1 to h - 2, w - 2 to h - 2)) assertNear(magenta, image[x, y])
        val inside = image[w / 2, h / 3]
        if (picture) assertNear(Color(red), inside) else assertTrue("the gradient, $inside", abs(inside.red - magenta.red) + abs(inside.green - magenta.green) > 0.3f)
        assertNear(Color(0xFF38BDF8), image[20, h - 3])
    }

    /**
     * A picture fading in (from disk or the network, where pictures fade) is
     * drawn over the whole box from the fade's first frame, while the
     * placeholder stays under it: the tile's corners are round then too, not
     * only once the fade is over.
     */
    @Test
    fun aPictureFadingInIsRounded() {
        val magenta = Color(0xFFFF00FF)
        val request = ImageRequest.Builder(context).data("$origin/api/items/1/images/poster?w=240").crossfade(150).build()
        val load = ArtworkLoad()
        val success = SuccessResult(FakeImage(width = 216, height = 324, color = red), request, DataSource.NETWORK)
        // Its fade counted on a scope that never runs: it stays partway.
        load.onState(AsyncImagePainter.State.Success(ColorPainter(Color(red)), success), CoroutineScope(Job().apply { cancel() }))
        assertTrue(load.settled && !load.loaded)
        compose.setContent {
            Box(Modifier.background(magenta).testTag("ground")) {
                // The fading picture stands in as a red box over the whole tile.
                Box(
                    Modifier
                        .tvTileSize(TileKind.Poster)
                        .artworkPlaceholder("Saltwind", null, load, PleyColors.Surface2, TileStyle.Tv.shape(TileKind.Poster))
                        .background(Color(red)),
                )
            }
        }
        compose.waitForIdle()
        val image = compose.onNodeWithTag("ground").captureToImage().toPixelMap()
        val (w, h) = image.width to image.height
        for ((x, y) in listOf(1 to 1, w - 2 to 1, 1 to h - 2, w - 2 to h - 2)) assertNear(magenta, image[x, y])
        assertNear(Color(red), image[w / 2, h / 2])
    }

    @Test
    fun aTileIsOneNodeThatAParentMerges() {
        compose.setContent {
            CompositionLocalProvider(LocalArtworkImageLoader provides loader()) {
                Column {
                    ArtworkTile(model(1, "Glasshouse"), TileKind.Landscape, TileStyle.Tv, Modifier.tvTileSize(TileKind.Landscape))
                    Box(Modifier.clickable {}) {
                        ArtworkTile(model(2, "Saltwind"), TileKind.Landscape, TileStyle.Tv, Modifier.tvTileSize(TileKind.Landscape))
                        TileCaption("Saltwind", "2024", TileStyle.Tv)
                    }
                }
            }
        }
        compose.onNode(hasContentDescription("Glasshouse, 2024")).assertExists()
        // The clickable parent reads the tile's description, and the caption adds nothing to it.
        compose.onNode(hasClickAction() and hasContentDescription("Saltwind, 2024")).assertExists()
        val merged = compose.onNode(hasClickAction()).fetchSemanticsNode()
        assertEquals(listOf("Saltwind, 2024"), merged.config[androidx.compose.ui.semantics.SemanticsProperties.ContentDescription])
    }

    @Test
    fun tilesSkipWhenAnotherItemChanges() {
        val models = mutableStateListOf(model(1, "A"), model(2, "B"), model(3, "C"))
        val compositions = mutableMapOf<Long, Int>()
        TileProbe.listener = { id -> compositions[id] = (compositions[id] ?: 0) + 1 }
        compose.setContent {
            CompositionLocalProvider(LocalArtworkImageLoader provides loader()) {
                Column {
                    for (m in models) {
                        key(m.id) { ArtworkTile(m, TileKind.Poster, TileStyle.Tv, Modifier.tvTileSize(TileKind.Poster)) }
                    }
                }
            }
        }
        compose.waitForIdle()
        val before = compositions.toMap()

        compose.runOnIdle { models[1] = models[1].copy(progress = 0.4f) }
        compose.waitForIdle()

        assertEquals(before[1L], compositions[1L])
        assertEquals(before[3L], compositions[3L])
        assertEquals((before[2L] ?: 0) + 1, compositions[2L])
    }

    @Test
    fun modelsSayWhatTheItemIs() {
        val episode = ItemSummary(
            id = 30, libraryId = 1, kind = ItemKind.Episode, title = "Glasshouse", index = 5, parentIndex = 2,
            grandparentTitle = "Driftline", grandparentId = 10, durationMs = 3_480_000,
            userState = UserState(positionMs = 1_920_000), thumbUrl = "/api/items/30/images/thumb?v=1",
        )
        val wide = ArtworkTileModel.of(episode, TileKind.Landscape, origin, locale = java.util.Locale.US)
        assertEquals("Driftline", wide.title)
        assertEquals("S2 · E5 · 26 min left", wide.subtitle)
        assertEquals(1_920_000f / 3_480_000f, wide.progress, 0.0001f)
        assertEquals("Glasshouse, Driftline, Season 2 Episode 5, 26 minutes left", wide.description)
        val row = ArtworkTileModel.of(episode, TileKind.Episode, origin, locale = java.util.Locale.US)
        assertEquals("5. Glasshouse", row.title)
        assertEquals("26 min left", row.subtitle)
        val show = ItemSummary(id = 2, libraryId = 1, kind = ItemKind.Show, title = "Severance", childCount = 2, leafCount = 19, viewedLeafCount = 13)
        val poster = ArtworkTileModel.of(show, TileKind.Poster, origin, locale = java.util.Locale.US)
        assertEquals(6, poster.unwatched)
        assertEquals("2 seasons", poster.subtitle)
    }

    @Test
    fun modelsSayItInTheLanguageShown() {
        fun wordsIn(tag: String) = formatWords(
            context.createConfigurationContext(
                android.content.res.Configuration().apply { setLocales(android.os.LocaleList(java.util.Locale.forLanguageTag(tag))) },
            ).resources,
        )
        val episode = ItemSummary(
            id = 30, libraryId = 1, kind = ItemKind.Episode, title = "Glasshouse", index = 5, parentIndex = 2,
            grandparentTitle = "Driftline", durationMs = 3_480_000, userState = UserState(positionMs = 1_920_000),
        )
        val french = ArtworkTileModel.of(episode, TileKind.Landscape, origin, wordsIn("fr"))
        assertEquals("S2 · É5 · Il reste 26 min", french.subtitle)
        assertEquals("Glasshouse, Driftline, Saison 2 Épisode 5, il reste 26 minutes", french.description)
        val show = ItemSummary(id = 2, libraryId = 1, kind = ItemKind.Show, title = "Severance", childCount = 2, leafCount = 19, viewedLeafCount = 13)
        val spanish = ArtworkTileModel.of(show, TileKind.Poster, origin, wordsIn("es"))
        assertEquals("2 temporadas", spanish.subtitle)
        assertEquals("Severance, 2 temporadas, 6 sin ver", spanish.description)
        // In English, as without words.
        assertEquals(ArtworkTileModel.of(episode, TileKind.Landscape, origin), ArtworkTileModel.of(episode, TileKind.Landscape, origin, wordsIn("en-US")))
    }

    private companion object {
        val PlaceholderSide = 200.dp
    }
}
