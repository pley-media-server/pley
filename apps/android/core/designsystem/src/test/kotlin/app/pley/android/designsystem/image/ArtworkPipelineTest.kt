package app.pley.android.designsystem.image

import android.content.Context
import androidx.test.core.app.ApplicationProvider
import app.pley.android.common.Tier
import app.pley.android.designsystem.tile.TileKind
import app.pley.android.model.ItemKind
import app.pley.android.model.ItemSummary
import coil3.ImageLoader
import coil3.Uri
import coil3.fetch.FetchResult
import coil3.fetch.Fetcher
import coil3.fetch.ImageFetchResult
import coil3.intercept.Interceptor
import coil3.memory.MemoryCache
import coil3.network.HttpException
import coil3.network.NetworkResponse
import coil3.request.CachePolicy
import coil3.request.ErrorResult
import coil3.request.ImageRequest
import coil3.request.ImageResult
import coil3.request.Options
import coil3.request.SuccessResult
import coil3.request.transitionFactory
import coil3.size.pxOrElse
import coil3.test.FakeImage
import coil3.transition.CrossfadeTransition
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.async
import kotlinx.coroutines.awaitCancellation
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import kotlin.math.ceil

@RunWith(RobolectricTestRunner::class)
class ArtworkPipelineTest {
    private val context: Context = ApplicationProvider.getApplicationContext()
    private val origin = "http://192.168.1.20:7539"

    /** The server's width for a [dp]-wide box at [density] on [tier], on a screen [screenPx] wide. */
    private fun width(dp: Float, density: Float, tier: Tier, screenPx: Int, cap: Int = 0): Int =
        ArtworkPolicy(tier, screenPx).width(ceil(dp * density).toInt(), cap)

    @Test
    fun widthsFollowTheBoxDensityAndTier() {
        // A 1080p TV (2 px per dp), the design spec's §6 list.
        assertEquals(240, width(TileKind.Poster.tvWidth.value, 2f, Tier.T1, 1920))
        assertEquals(480, width(TileKind.Landscape.tvWidth.value, 2f, Tier.T1, 1920))
        assertEquals(960, width(360f, 2f, Tier.T2, 1920))
        assertEquals(1280, width(960f, 2f, Tier.T2, 1920, ArtworkImage.backdropCap(Tier.T2)))
        assertEquals(960, width(960f, 2f, Tier.T0, 1920, ArtworkImage.backdropCap(Tier.T0)))
        // A 720p TV (1.5 px per dp).
        assertEquals(240, width(TileKind.Poster.tvWidth.value, 1.5f, Tier.T0, 1280))
        // Never 1920 on T0, never above what the screen needs.
        assertEquals(1280, width(960f, 2f, Tier.T0, 1920))
        assertEquals(1920, width(960f, 2f, Tier.T2, 1920))
        assertEquals(1280, width(412f, 2.625f, Tier.T2, 1080))
        assertEquals(960, width(960f, 1f, Tier.T2, 960))
        // A phone poster at 3 px per dp.
        assertEquals(480, width(TileKind.Poster.phoneMinWidth.value, 3f, Tier.T2, 1080))
        // Unknown box: the widest allowed.
        assertEquals(1280, ArtworkPolicy(Tier.T0, 1920).width(null))
    }

    @Test
    fun fadesExceptOnT0() {
        assertEquals(0, ArtworkPolicy(Tier.T0, 1920).fadeMillis)
        assertEquals(150, ArtworkPolicy(Tier.T1, 1920).fadeMillis)
    }

    @Test
    fun candidatesFollowTheWebsChains() {
        val episode = ItemSummary(
            id = 30, libraryId = 1, kind = ItemKind.Episode, title = "Glasshouse", parentId = 20, grandparentId = 10,
            thumbUrl = "/api/items/30/images/thumb?v=1",
        )
        assertEquals(
            listOf("$origin/api/items/20/images/poster", "$origin/api/items/10/images/poster"),
            ArtworkCandidates.poster(episode, origin).candidates,
        )
        assertEquals(
            listOf("$origin/api/items/10/images/backdrop", "$origin/api/items/20/images/backdrop"),
            ArtworkCandidates.backdrop(episode, origin, Tier.T1).candidates,
        )
        assertEquals(
            listOf("$origin/api/items/30/images/thumb?v=1", "$origin/api/items/10/images/backdrop"),
            ArtworkCandidates.thumb(episode, origin).candidates,
        )
        val movie = ItemSummary(id = 1, libraryId = 1, kind = ItemKind.Movie, title = "Heat", posterUrl = "/p", backdropUrl = "/b", thumbUrl = "/t")
        assertEquals(listOf("$origin/p"), ArtworkCandidates.poster(movie, origin).candidates)
        assertEquals(listOf("$origin/b", "$origin/t"), ArtworkCandidates.thumb(movie, origin).candidates)
        assertEquals(960, ArtworkCandidates.backdrop(movie, origin, Tier.T0).maxWidth)
        val show = ItemSummary(id = 2, libraryId = 1, kind = ItemKind.Show, title = "S", backdropUrl = "/b")
        assertEquals(listOf("$origin/b"), ArtworkCandidates.thumb(show, origin).candidates)
    }

    /** Serves [present] and answers 404 for anything else, remembering what was asked. */
    private class FakeServer(private val present: Set<String>) : Interceptor {
        val asked = mutableListOf<String>()
        val policies = mutableListOf<CachePolicy>()

        override suspend fun intercept(chain: Interceptor.Chain): ImageResult {
            val url = chain.request.data as String
            asked += url
            policies += chain.request.memoryCachePolicy
            return if (url in present) {
                SuccessResult(FakeImage(), chain.request, coil3.decode.DataSource.NETWORK)
            } else {
                ErrorResult(null, chain.request, HttpException(NetworkResponse(code = 404)))
            }
        }
    }

    private fun loader(server: Interceptor, tier: Tier = Tier.T1): ImageLoader = ImageLoader.Builder(context)
        .components {
            add(ArtworkInterceptor(ArtworkPolicy(tier, 1920)))
            add(server)
        }
        .memoryCache(null)
        .diskCache(null)
        .coroutineContext(Dispatchers.Unconfined)
        .build()

    @Test
    fun theFirstAddressThatLoadsWinsAtTheBoxWidth() = runTest {
        val server = FakeServer(setOf("$origin/api/items/10/images/poster?w=320"))
        val image = ArtworkImage(listOf("$origin/api/items/30/images/poster?v=7", "$origin/api/items/20/images/poster", "$origin/api/items/10/images/poster"))
        val request = ImageRequest.Builder(context).data(image).size(248, 372).build()
        val loader = loader(server)

        val result = loader.execute(request)

        assertTrue(result is SuccessResult)
        assertEquals(
            listOf(
                "$origin/api/items/30/images/poster?v=7&w=320",
                "$origin/api/items/20/images/poster?w=320",
                "$origin/api/items/10/images/poster?w=320",
            ),
            server.asked,
        )
        // A missing picture is asked for once, not on every frame it shows.
        server.asked.clear()
        loader.execute(request)
        assertEquals(listOf("$origin/api/items/10/images/poster?w=320"), server.asked)
    }

    @Test
    fun aPictureThatLoadedComesBackWithoutFadingIn() = runTest {
        val url = "$origin/api/items/77/images/poster"
        val image = ArtworkImage(listOf(url))
        val loader = loader(FakeServer(setOf("$url?w=320")))
        val request = ImageRequest.Builder(context).data(image).size(248, 372).build()

        val first = loader.execute(request) as SuccessResult
        assertTrue(first.request.transitionFactory is CrossfadeTransition.Factory)
        assertTrue(loader.shownArtwork!!.has(image))
        // Asked for again (its page composed again, the memory cache had let it go): no fade.
        val again = loader.execute(request) as SuccessResult
        assertFalse(again.request.transitionFactory is CrossfadeTransition.Factory)
        // At another box's width it is the same picture.
        assertTrue(loader.shownArtwork!!.has(ArtworkImage(listOf(url), maxWidth = 960)))
    }

    @Test
    fun onlyPicturesThatLoadedAreKnown() = runTest {
        val image = ArtworkImage(listOf("$origin/api/items/78/images/poster"))
        val loader = loader(FakeServer(emptySet()))
        loader.execute(ImageRequest.Builder(context).data(image).size(248, 372).build())
        assertFalse(loader.shownArtwork!!.has(image))
    }

    @Test
    fun nothingLoadsGivesAnError() = runTest {
        val result = loader(FakeServer(emptySet())).execute(ImageRequest.Builder(context).data(ArtworkImage(listOf("$origin/x"))).size(10, 10).build())
        assertTrue(result is ErrorResult)
        val none = loader(FakeServer(emptySet())).execute(ImageRequest.Builder(context).data(ArtworkImage(emptyList())).size(10, 10).build())
        assertTrue((none as ErrorResult).throwable is NoArtworkException)
    }

    @Test
    fun prefetchLoadsTheNextScreenAndAHalfAndOnlyToDiskBeyondOneOnT0() = runTest {
        val images = (0 until 20).map { ArtworkImage(listOf("$origin/api/items/$it/images/poster")) }
        val present = images.map { "${it.candidates[0]}?w=320" }.toSet()

        val t0 = FakeServer(present)
        ImagePrefetcher(context, loader(t0, Tier.T0), Tier.T0).onVisible(0, 3, images, 248, 372)
        assertEquals((4..9).map { "$origin/api/items/$it/images/poster?w=320" }, t0.asked)
        assertEquals(List(4) { CachePolicy.ENABLED } + List(2) { CachePolicy.DISABLED }, t0.policies)

        val t2 = FakeServer(present)
        val prefetcher = ImagePrefetcher(context, loader(t2, Tier.T2), Tier.T2)
        prefetcher.onVisible(0, 3, images, 248, 372)
        prefetcher.onVisible(1, 4, images, 248, 372)
        assertEquals((4..10).map { "$origin/api/items/$it/images/poster?w=320" }, t2.asked)
        assertTrue(t2.policies.all { it == CachePolicy.ENABLED })
    }

    @Test
    fun prefetchWaitsWhileAKeyIsHeldThenAsksWhereItCameUp() = runTest {
        val images = (0 until 20).map { ArtworkImage(listOf("$origin/api/items/$it/images/poster")) }
        val server = FakeServer(images.map { "${it.candidates[0]}?w=320" }.toSet())
        val hold = FakeHold()
        val prefetcher = ImagePrefetcher(context, loader(server, Tier.T2), Tier.T2, hold)
        prefetcher.onVisible(0, 3, images, 248, 372)
        assertTrue(server.asked.isEmpty())
        hold.release()
        prefetcher.onVisible(4, 7, images, 248, 372)
        assertEquals((8..13).map { "$origin/api/items/$it/images/poster?w=320" }, server.asked)
    }

    @Test
    fun prefetchLetsGoOfWhatTheListHasPassed() = runTest {
        val images = (0 until 40).map { ArtworkImage(listOf("$origin/api/items/$it/images/poster")) }
        val server = SlowServer()
        val prefetcher = ImagePrefetcher(context, loader(server, Tier.T2), Tier.T2)
        prefetcher.onVisible(0, 3, images, 248, 372)
        assertEquals((4..9).map { "$origin/api/items/$it/images/poster?w=320" }, server.asked)
        // A long way on: the pictures still loading for what went by stop, and those ahead start.
        prefetcher.onVisible(20, 23, images, 248, 372)
        assertEquals((4..9).map { "$origin/api/items/$it/images/poster?w=320" }.toSet(), server.cancelled.toSet())
        assertEquals((24..29).map { "$origin/api/items/$it/images/poster?w=320" }, server.asked.drop(6))
    }

    @Test
    fun aHeldPictureWaitsForTheKeyUnlessTheMemoryCacheHasIt() = runTest {
        val fetched = mutableListOf<String>()
        val loader = ImageLoader.Builder(context)
            .components {
                addArtworkHold()
                add(ArtworkInterceptor(ArtworkPolicy(Tier.T1, 1920)))
                add(FakeFetcher(fetched))
            }
            .memoryCache(MemoryCache.Builder().maxSizeBytes(1L shl 20).build())
            .diskCache(null)
            .coroutineContext(Dispatchers.Unconfined)
            .build()
        val hold = FakeHold()
        val image = ArtworkImage(listOf("$origin/api/items/1/images/poster"))
        val request = heldRequest(image, hold, context).newBuilder().size(248, 372).build()

        val first = async { loader.execute(request) }
        runCurrent()
        // Held: nothing is fetched, and the request waits.
        assertFalse(first.isCompleted)
        assertTrue(fetched.isEmpty())
        hold.release()
        runCurrent()
        assertTrue(first.await() is SuccessResult)
        assertEquals(listOf("$origin/api/items/1/images/poster?w=320"), fetched)

        // Held again: in the memory cache now, it comes, fetching nothing.
        hold.hold()
        assertTrue(loader.execute(request) is SuccessResult)
        assertEquals(1, fetched.size)
    }

    @Test
    fun aHeldRequestsLoaderWorkLeavesTheCallingThreadOnlyWhileHeld() = runTest {
        val threads = mutableListOf<Thread>()
        val loader = ImageLoader.Builder(context)
            .components {
                add(
                    object : Interceptor {
                        override suspend fun intercept(chain: Interceptor.Chain): ImageResult {
                            synchronized(threads) { threads += Thread.currentThread() }
                            return chain.proceed()
                        }
                    },
                )
                addArtworkHold()
                add(ArtworkInterceptor(ArtworkPolicy(Tier.T1, 1920)))
                add(FakeFetcher(mutableListOf()))
            }
            .memoryCache(MemoryCache.Builder().maxSizeBytes(1L shl 20).build())
            .diskCache(null)
            .coroutineContext(Dispatchers.Unconfined)
            .build()
        val hold = FakeHold()
        val image = ArtworkImage(listOf("$origin/api/items/2/images/poster"))
        val request = heldRequest(image, hold, context).newBuilder().size(248, 372).build()
        val caller = Thread.currentThread()

        // Not held: on the caller's thread, so that a picture in the memory cache shows in the same frame.
        hold.release()
        assertTrue(loader.execute(request) is SuccessResult)
        assertEquals(listOf(caller), threads)

        // Held: on another thread, and from the memory cache all the same.
        threads.clear()
        hold.hold()
        assertTrue(loader.execute(request) is SuccessResult)
        assertTrue("ran on ${threads.single()}", threads.single() != caller)
    }

    @Test
    fun aHeldRequestGoesOnAwayFromTheThreadThatEndsTheHold() = runTest {
        val after = mutableListOf<Thread>()
        val loader = ImageLoader.Builder(context)
            .components {
                addArtworkHold()
                add(
                    object : Interceptor {
                        override suspend fun intercept(chain: Interceptor.Chain): ImageResult {
                            synchronized(after) { after += Thread.currentThread() }
                            return chain.proceed()
                        }
                    },
                )
                add(ArtworkInterceptor(ArtworkPolicy(Tier.T1, 1920)))
                add(FakeFetcher(mutableListOf()))
            }
            .memoryCache(MemoryCache.Builder().maxSizeBytes(1L shl 20).build())
            .diskCache(null)
            .coroutineContext(Dispatchers.Unconfined)
            .build()
        val hold = FakeHold()
        val image = ArtworkImage(listOf("$origin/api/items/3/images/poster"))
        val request = heldRequest(image, hold, context).newBuilder().size(248, 372).build()
        val pending = async { loader.execute(request) }
        runCurrent()
        // Set aside, not in the memory cache: it waits for the key to come up.
        val until = System.currentTimeMillis() + 5_000
        while (!hold.waiting && System.currentTimeMillis() < until) Thread.sleep(10)
        assertTrue(hold.waiting)
        Thread.sleep(50)

        // The key comes up, on the main thread: the request goes on elsewhere, as every one set aside does, rather
        // than all of them there and then.
        val main = Thread.currentThread()
        hold.release()
        assertTrue(pending.await() is SuccessResult)
        assertTrue("went on on ${after.last()}", after.last() != main)
    }

    /** A hold that holds until [release]. */
    private class FakeHold : ArtworkHold {
        override var holding = true
        private var released = CompletableDeferred<Unit>()

        /** Whether a request waits for [release]. */
        @Volatile
        var waiting = false

        override suspend fun awaitRelease() {
            if (holding) {
                waiting = true
                released.await()
            }
        }

        fun release() {
            holding = false
            released.complete(Unit)
        }

        fun hold() {
            holding = true
            released = CompletableDeferred()
        }
    }

    /** Answers nothing until cancelled, remembering what was asked and what was let go. */
    private class SlowServer : Interceptor {
        val asked = mutableListOf<String>()
        val cancelled = mutableListOf<String>()

        override suspend fun intercept(chain: Interceptor.Chain): ImageResult {
            val url = chain.request.data as String
            asked += url
            try {
                awaitCancellation()
            } finally {
                cancelled += url
            }
        }
    }

    /** Fetches every address as a picture of the size asked for (Coil keeps no other for an exact size), remembering which. */
    private class FakeFetcher(private val fetched: MutableList<String>) : Fetcher.Factory<Uri> {
        override fun create(data: Uri, options: Options, imageLoader: ImageLoader): Fetcher = object : Fetcher {
            override suspend fun fetch(): FetchResult {
                fetched += data.toString()
                val image = FakeImage(width = options.size.width.pxOrElse { 1 }, height = options.size.height.pxOrElse { 1 })
                return ImageFetchResult(image, isSampled = false, dataSource = coil3.decode.DataSource.NETWORK)
            }
        }
    }
}
