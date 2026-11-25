package app.pley.android.ui.phone.account

import android.content.Context
import android.graphics.Bitmap
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.platform.LocalResources
import androidx.compose.runtime.remember
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.test.core.app.ApplicationProvider
import app.pley.android.common.Tier
import app.pley.android.designsystem.image.ArtworkInterceptor
import app.pley.android.designsystem.image.ArtworkPolicy
import app.pley.android.designsystem.image.LocalArtworkImageLoader
import app.pley.android.player.NowPlaying
import app.pley.android.screens.Navigator
import app.pley.android.screens.PleyKey
import app.pley.android.ui.phone.kit.LocalOffline
import app.pley.android.ui.phone.music.NowPlayingScreen
import app.pley.android.ui.phone.shell.LocalPhoneNavigator
import app.pley.android.ui.phone.shell.LocalPhoneShell
import app.pley.android.ui.phone.shell.PhoneShellState
import app.pley.android.ui.phone.shell.rememberPhoneNavigation
import app.pley.android.ui.phone.theme.PhoneTheme
import coil3.ImageLoader
import coil3.Uri
import coil3.asImage
import coil3.decode.DataSource
import coil3.fetch.FetchResult
import coil3.fetch.Fetcher
import coil3.fetch.ImageFetchResult
import coil3.intercept.Interceptor
import coil3.memory.MemoryCache
import coil3.request.ImageResult
import coil3.request.Options
import coil3.size.pxOrElse
import java.util.Collections
import kotlinx.coroutines.Dispatchers
import org.junit.Assert.assertEquals
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

/**
 * Now Playing's faint backdrop is the cover's own picture: one address,
 * fetched once, though it is drawn twice (the cover, and its copy across
 * the page), not a second picture at the window's width.
 */
@RunWith(RobolectricTestRunner::class)
@Config(application = AccountTestApplication::class, qualifiers = "w411dp-h891dp-port-xxhdpi")
class NowPlayingArtTest {
    @get:Rule
    val compose = createComposeRule()

    private val context: Context = ApplicationProvider.getApplicationContext()
    private val graph = AccountTestGraph()

    /** Every address the pictures were asked at, with its width, and those that went to the server. */
    private val asked: MutableList<String> = Collections.synchronizedList(mutableListOf())
    private val fetched: MutableList<String> = Collections.synchronizedList(mutableListOf())

    private val loader: ImageLoader by lazy {
        ImageLoader.Builder(context)
            .components {
                add(ArtworkInterceptor(ArtworkPolicy(Tier.T2, 1080)))
                add(
                    object : Interceptor {
                        override suspend fun intercept(chain: Interceptor.Chain): ImageResult {
                            asked += chain.request.data.toString()
                            return chain.proceed()
                        }
                    },
                )
                add(Pictures(fetched))
            }
            .memoryCache(MemoryCache.Builder().maxSizeBytes(64L shl 20).build())
            .diskCache(null)
            .coroutineContext(Dispatchers.Unconfined)
            .build()
    }

    private val navigator = object : Navigator {
        override fun open(key: PleyKey) = Unit

        override fun back() = Unit

        override fun restart(key: PleyKey) = Unit
    }

    @Before
    fun graph() {
        AccountTestApplication.graph = graph
        graph.music.now.value = NowPlaying(503, "Song 3", "Harbour Lights", playing = true, album = "Deep Water", count = 12, index = 2, durationMs = 180_000, artUrl = COVER)
    }

    @Test
    fun theBackdropIsTheCoversOwnPicture() {
        compose.setContent {
            PhoneTheme {
                val navigation = rememberPhoneNavigation(PleyKey.Home) {}
                val resources = LocalResources.current
                val shell = remember { PhoneShellState(navigation, resources, AccountTestGraph.SERVER) }
                CompositionLocalProvider(
                    LocalPhoneNavigator provides navigator,
                    LocalPhoneShell provides shell,
                    LocalOffline provides shell.offlineState,
                    LocalArtworkImageLoader provides loader,
                ) { NowPlayingScreen(car = false) }
            }
        }
        compose.waitUntil(5_000) { synchronized(asked) { asked.size >= 2 } }
        compose.waitForIdle()
        val all = synchronized(asked) { asked.toList() }
        // Asked for twice, the cover then its copy, at one width: the second from the memory cache.
        assertEquals("$all", 2, all.size)
        assertEquals("$all", 1, all.toSet().size)
        assertEquals(listOf(COVER.substringBefore('?')), synchronized(fetched) { fetched.toList() })
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
        const val COVER = "http://192.168.1.20:7539/api/items/50/images/poster?v=1"
    }
}
