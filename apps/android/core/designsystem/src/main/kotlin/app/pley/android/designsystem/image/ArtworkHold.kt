package app.pley.android.designsystem.image

import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.runtime.staticCompositionLocalOf
import coil3.ComponentRegistry
import coil3.Extras
import coil3.ImageLoader
import coil3.PlatformContext
import coil3.Uri
import coil3.fetch.FetchResult
import coil3.fetch.Fetcher
import coil3.getExtra
import coil3.compose.LocalPlatformContext
import coil3.intercept.Interceptor
import coil3.request.ImageRequest
import coil3.request.ImageResult
import coil3.request.Options
import coil3.request.SuccessResult
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlin.coroutines.CoroutineContext
import kotlin.coroutines.EmptyCoroutineContext

/**
 * Pictures held back while a TV's direction key is held (D-pad rule 5),
 * as the native Samsung app holds them: the tiles a row slides past or a
 * page glides past ask for nothing, and those still there when the key
 * comes up load then. A picture the memory cache has shows all the same
 * (a frame later: [HeldDispatch]), as it costs no download and no
 * decode. The TV's shell provides the window's ([LocalArtworkHold]);
 * without one, pictures never wait.
 */
interface ArtworkHold {
    /** Whether pictures wait now. Snapshot state: read it in effects and loaders, never in composition. */
    val holding: Boolean

    /** Returns once they may load: at once when nothing is held. */
    suspend fun awaitRelease()
}

/** The window's [ArtworkHold]: the TV's key-repeat gate; none on phones. */
val LocalArtworkHold = staticCompositionLocalOf<ArtworkHold?> { null }

/**
 * What [Artwork] asks the loader for: [image], with the window's hold when
 * there is one, which the loader reads as the request runs
 * ([addArtworkHold]); composition never reads whether it holds.
 */
@Composable
internal fun rememberArtworkModel(image: ArtworkImage): Any {
    val hold = LocalArtworkHold.current ?: return image
    val context = LocalPlatformContext.current
    return remember(image, hold, context) { heldRequest(image, hold, context) }
}

/** A request for [image] that waits while [hold] holds, its loader's work off the main thread meanwhile ([HeldDispatch]). */
internal fun heldRequest(image: ArtworkImage, hold: ArtworkHold, context: PlatformContext): ImageRequest =
    ImageRequest.Builder(context).data(image).apply { extras[HoldKey] = hold }.interceptorCoroutineContext(HeldDispatch(hold)).build()

/**
 * Where a held request's interceptors run: right there, on the main thread,
 * as the loader's own do, so that a picture in the memory cache shows in
 * the frame its tile first draws; but while [hold] holds, on a thread of
 * the pool. A tile a held key brings in starts its request as it's first
 * measured, in the frame that shows it, and looking it up in the memory
 * cache and setting it aside ([HoldInterceptor]) took a Google TV
 * Streamer's main thread 0.6 ms a tile, a dozen tiles for each step of a
 * grid. Those pictures can come a frame later.
 *
 * A request sent to the pool stays there: those set aside go on as the key
 * comes up, and went on right there, on the main thread that ends the
 * hold, a screenful of tiles at once.
 */
private class HeldDispatch(private val hold: ArtworkHold) : CoroutineDispatcher() {
    @Volatile
    private var away = false

    override fun isDispatchNeeded(context: CoroutineContext): Boolean = away || hold.holding

    override fun dispatch(context: CoroutineContext, block: Runnable) {
        away = true
        Dispatchers.Default.dispatch(context, block)
    }
}

/**
 * Lets the loader hold pictures back ([ArtworkHold]): before
 * [ArtworkInterceptor], an [ArtworkImage] asked for with a hold that holds
 * is first looked for in the memory cache only, and otherwise waits for
 * the key to come up; if its tile goes meanwhile, it is never asked for.
 */
fun ComponentRegistry.Builder.addArtworkHold(): ComponentRegistry.Builder =
    add(HoldInterceptor).add(MemoryOnly)

private val HoldKey = Extras.Key<ArtworkHold?>(default = null)

/** Marks a request that may come only from the memory cache. */
private val MemoryOnlyKey = Extras.Key(default = false)

private object HoldInterceptor : Interceptor {
    override suspend fun intercept(chain: Interceptor.Chain): ImageResult {
        val hold = chain.request.getExtra(HoldKey) ?: return chain.proceed()
        if (hold.holding) {
            val cached = chain.withRequest(memoryOnly(chain.request)).proceed()
            if (cached is SuccessResult) return cached
            hold.awaitRelease()
        }
        return chain.proceed()
    }

    /** [request] answered from the memory cache or not at all: its fetch fails at once, on the caller's thread. */
    private fun memoryOnly(request: ImageRequest): ImageRequest = request.newBuilder()
        .apply { extras[MemoryOnlyKey] = true }
        .fetcherCoroutineContext(EmptyCoroutineContext)
        .build()
}

/** The fetch of a memory-only request ([MemoryOnlyKey]): there is none. Other requests go on to the network's. */
private object MemoryOnly : Fetcher.Factory<Uri> {
    override fun create(data: Uri, options: Options, imageLoader: ImageLoader): Fetcher? =
        if (options.getExtra(MemoryOnlyKey)) NoFetch else null

    private object NoFetch : Fetcher {
        override suspend fun fetch(): FetchResult = throw HeldBack
    }
}

/** A picture held back: not in the memory cache while a key is held. One, without a stack: tiles a key passes throw it often. */
private object HeldBack : RuntimeException("Held back while a key is held") {
    override fun fillInStackTrace(): Throwable = this
}
