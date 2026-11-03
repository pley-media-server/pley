package app.pley.android.designsystem

import android.content.Context
import app.pley.android.common.DeviceProfile
import app.pley.android.common.Tier
import app.pley.android.designsystem.image.ArtworkInterceptor
import app.pley.android.designsystem.image.ArtworkPolicy
import app.pley.android.designsystem.image.addArtworkHold
import coil3.ComponentRegistry
import coil3.ImageLoader
import coil3.bitmapFactoryMaxParallelism
import coil3.disk.DiskCache
import coil3.memory.MemoryCache
import coil3.memoryCacheMaxSizePercentWhileInBackground
import coil3.network.okhttp.OkHttpNetworkFetcherFactory
import coil3.request.allowHardware
import okhttp3.OkHttpClient
import okio.Path.Companion.toOkioPath

/**
 * The app's one image loader (Coil), on the images client so pictures
 * share its connections, sized by device (architecture.md, "Images"):
 *
 * - Memory: 20 % of the app's memory class on phones; 24, 48 or 96 MB on
 *   T0, T1 and T2 TVs. While the app is in the background it keeps 10 %,
 *   none on T0.
 * - Disk: cacheDir/images, 512 MB on phones (256 MB on low-RAM ones),
 *   128 MB on TVs. The server's `?v=` addresses never change content, so
 *   cached pictures never go stale.
 * - Hardware bitmaps; two decodes at a time on T0.
 * - [ArtworkImage][app.pley.android.designsystem.image.ArtworkImage]s are
 *   asked for at their box's width and fall back in order ([ArtworkInterceptor]),
 *   and wait while a TV's direction key is held unless the memory cache
 *   has them ([addArtworkHold]).
 * - No crossfade by default: artwork fades only when it came from disk or
 *   the network, and never on T0.
 */
object PleyImageLoader {
    /** [client] is called on the first picture, not before: the first frame waits for nothing. */
    fun create(context: Context, client: () -> OkHttpClient, profile: DeviceProfile): ImageLoader =
        build(context, client, profile) {}

    /** [create], with [extra] components (the debug gallery's drawn pictures). */
    internal fun build(
        context: Context,
        client: () -> OkHttpClient,
        profile: DeviceProfile,
        extra: ComponentRegistry.Builder.() -> Unit,
    ): ImageLoader {
        val app = context.applicationContext
        val metrics = app.resources.displayMetrics
        val policy = ArtworkPolicy(profile.tier, maxOf(metrics.widthPixels, metrics.heightPixels))
        return ImageLoader.Builder(app)
            .components {
                addArtworkHold()
                add(ArtworkInterceptor(policy))
                add(OkHttpNetworkFetcherFactory(callFactory = { client() }))
                extra()
            }
            .memoryCache { memoryCache(app, profile) }
            .diskCache {
                DiskCache.Builder()
                    .directory(app.cacheDir.resolve("images").toOkioPath())
                    .maxSizeBytes(diskCacheBytes(profile))
                    .build()
            }
            .allowHardware(true)
            .memoryCacheMaxSizePercentWhileInBackground(if (profile.tier == Tier.T0) 0.0 else 0.1)
            .apply { if (profile.tier == Tier.T0) bitmapFactoryMaxParallelism(2) }
            .build()
    }

    private fun memoryCache(context: Context, profile: DeviceProfile): MemoryCache {
        val builder = MemoryCache.Builder()
        if (profile.isTv) builder.maxSizeBytes(tvMemoryCacheBytes(profile.tier)) else builder.maxSizePercent(context, 0.2)
        return builder.build()
    }

    /** A TV's memory cache: 24, 48 or 96 MB by tier. */
    fun tvMemoryCacheBytes(tier: Tier): Long = when (tier) {
        Tier.T0 -> 24L * MB
        Tier.T1 -> 48L * MB
        Tier.T2 -> 96L * MB
    }

    /** The disk cache: 128 MB on TVs, 512 MB on phones, 256 MB on low-RAM phones. */
    fun diskCacheBytes(profile: DeviceProfile): Long = when {
        profile.isTv -> 128L * MB
        profile.lowRam -> 256L * MB
        else -> 512L * MB
    }

    private const val MB = 1024L * 1024
}
