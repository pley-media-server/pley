package app.pley.android.designsystem.image

import app.pley.android.common.ImageUrls
import app.pley.android.common.Tier
import app.pley.android.designsystem.PleyMotion
import coil3.ImageLoader
import coil3.intercept.Interceptor
import coil3.network.HttpException
import coil3.request.ErrorResult
import coil3.request.ImageResult
import coil3.request.SuccessResult
import coil3.request.crossfade
import coil3.size.Dimension
import kotlinx.coroutines.CancellationException

/**
 * What the device allows pictures: how wide they may be asked for, and
 * whether they fade in.
 *
 * - Never wider than the screen needs ([screenWidthPx], rounded up to the
 *   server's next width) and never 1920 on T0.
 * - A 150 ms fade, only for pictures that weren't in the memory cache
 *   (Coil's crossfade skips those), none on T0, none for an
 *   [ArtworkImage] that says not to, and none for one that has loaded
 *   before ([ShownArtwork]).
 */
class ArtworkPolicy(val tier: Tier, screenWidthPx: Int) {
    /** The widest picture this device asks for. */
    val maxWidth: Int = run {
        val screen = ImageUrls.BUCKETS.firstOrNull { it >= screenWidthPx } ?: ImageUrls.LARGEST
        if (tier == Tier.T0) minOf(screen, T0_MAX) else screen
    }

    /** How long a picture fades in; 0 for none. */
    val fadeMillis: Int = if (tier == Tier.T0) 0 else PleyMotion.IMAGE_FADE_MS

    /** The server's width for a box [px] wide ([maxWidth] when unknown), under [cap] when it has one. */
    fun width(px: Int?, cap: Int = 0): Int = ImageUrls.bucket(px ?: limit(cap), limit(cap))

    /** [url] asking for [width]`(px, cap)`; a picture on the device (a download's) as it is. */
    fun sized(url: String, px: Int?, cap: Int = 0): String =
        if (url.startsWith("file:")) url else ImageUrls.sized(url, px ?: limit(cap), limit(cap))

    private fun limit(cap: Int): Int = if (cap > 0) minOf(cap, maxWidth) else maxWidth

    companion object {
        /** 1280: a T0 device never decodes 1920-wide pictures. */
        const val T0_MAX = 1280
    }
}

/**
 * Loads [ArtworkImage]s: asks for each address at the width of the box it
 * fills (Coil resolves the box's size before this runs), tries them in
 * order until one loads, and remembers addresses the server said don't
 * exist, so a missing poster costs one request, not one per frame it shows.
 * It also remembers the pictures that loaded ([shown]), which come back
 * at once when they show again. Any other request passes through.
 */
class ArtworkInterceptor(private val policy: ArtworkPolicy) : Interceptor {
    private val missing = object : LinkedHashMap<String, Unit>(64, 0.75f, true) {
        override fun removeEldestEntry(eldest: MutableMap.MutableEntry<String, Unit>?): Boolean = size > MISSING_KEPT
    }

    /** The pictures that have loaded, for [Artwork] to know one shown before ([shownArtwork]). */
    internal val shown = ShownArtwork()

    override suspend fun intercept(chain: Interceptor.Chain): ImageResult {
        val image = chain.request.data as? ArtworkImage ?: return chain.proceed()
        val px = (chain.size.width as? Dimension.Pixels)?.px
        // One that loaded before comes back without fading in, from the memory cache or not.
        val fade = policy.fadeMillis > 0 && image.fade && !shown.has(image)
        var last: ImageResult? = null
        for (candidate in image.candidates) {
            val url = policy.sized(candidate, px, image.maxWidth)
            if (isMissing(url)) continue
            val request = chain.request.newBuilder()
                .data(url)
                .apply { if (fade) crossfade(policy.fadeMillis) }
                .build()
            val result = try {
                chain.withRequest(request).proceed()
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                ErrorResult(null, request, e)
            }
            if (result is SuccessResult) {
                shown.add(image)
                return result
            }
            val code = ((result as? ErrorResult)?.throwable as? HttpException)?.response?.code
            if (code == 404 || code == 410) markMissing(url)
            last = result
        }
        return last ?: ErrorResult(null, chain.request, NoArtworkException())
    }

    private fun isMissing(url: String): Boolean = synchronized(missing) { url in missing }

    private fun markMissing(url: String) = synchronized(missing) { missing[url] = Unit }

    private companion object {
        const val MISSING_KEPT = 512
    }
}

/** An item without a picture to try, or none that loaded: the placeholder stays. */
class NoArtworkException : RuntimeException("No artwork")

/**
 * Pictures that have loaded, by their addresses (the same picture asked
 * for at another box's width is the same picture), up to [KEPT], the one
 * asked about least recently leaving first. One shown again, when a page
 * is composed again or the memory cache let it go, shows at once: no
 * title in its well while it comes back ([ArtworkLoad.known]), and no
 * fade. Asked on the main thread, told by the loader's.
 */
internal class ShownArtwork {
    private val keys = object : LinkedHashMap<Int, Unit>(256, 0.75f, true) {
        override fun removeEldestEntry(eldest: MutableMap.MutableEntry<Int, Unit>?): Boolean = size > KEPT
    }

    fun has(image: ArtworkImage): Boolean = synchronized(keys) { keys[image.candidates.hashCode()] != null }

    fun add(image: ArtworkImage) = synchronized(keys) { keys[image.candidates.hashCode()] = Unit }

    companion object {
        const val KEPT = 5_000
    }
}

/** The pictures this loader has loaded: the app's, which has an [ArtworkInterceptor]; `null` for another. */
internal val ImageLoader.shownArtwork: ShownArtwork?
    get() = components.interceptors.firstNotNullOfOrNull { (it as? ArtworkInterceptor)?.shown }
