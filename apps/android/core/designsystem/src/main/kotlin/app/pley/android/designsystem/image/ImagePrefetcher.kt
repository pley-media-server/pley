package app.pley.android.designsystem.image

import android.content.Context
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.lazy.grid.LazyGridState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.snapshotFlow
import app.pley.android.common.Tier
import coil3.ImageLoader
import coil3.SingletonImageLoader
import coil3.compose.LocalPlatformContext
import coil3.decode.BlackholeDecoder
import coil3.request.CachePolicy
import coil3.request.Disposable
import coil3.request.ImageRequest
import kotlinx.coroutines.flow.distinctUntilChanged

/**
 * Loads the pictures of the next 1.5 screens of a lazy list ahead of the
 * scroll (performance_plan.md, "Images"), at the exact size their tiles
 * draw them ([widthPx] by [heightPx]), so they share cache entries with
 * the tiles. On T0 only the first screen ahead is decoded into memory;
 * the half beyond it is only downloaded to the disk cache, which costs no
 * memory.
 *
 * Nothing is asked for while [hold] holds (a TV's held key passing tiles
 * by); where the key comes up asks then. A picture still loading once
 * the list has gone past it, or back from it, is let go.
 */
class ImagePrefetcher(
    private val context: Context,
    private val loader: ImageLoader,
    private val tier: Tier,
    private val hold: ArtworkHold? = null,
) {
    private val requested = object : LinkedHashMap<ArtworkImage, Boolean>(64, 0.75f, true) {
        override fun removeEldestEntry(eldest: MutableMap.MutableEntry<ArtworkImage, Boolean>?) = size > KEPT
    }

    /** What is still loading, by picture, with the index it was asked for at. */
    private val loading = HashMap<ArtworkImage, Loading>()

    /** Whether pictures wait now ([hold]). */
    val holding: Boolean get() = hold?.holding == true

    /**
     * After items [firstVisible] to [lastVisible] are on screen, asks for
     * the next 1.5 screens' worth of [images] (by index).
     */
    fun onVisible(firstVisible: Int, lastVisible: Int, images: List<ArtworkImage?>, widthPx: Int, heightPx: Int) {
        if (lastVisible < firstVisible || widthPx <= 0 || heightPx <= 0 || holding) return
        val perScreen = lastVisible - firstVisible + 1
        val ahead = (perScreen * 3 + 1) / 2
        letGo(firstVisible, lastVisible + ahead)
        for (offset in 1..ahead) {
            val index = lastVisible + offset
            val image = images.getOrNull(index) ?: continue
            if (image.isEmpty) continue
            val diskOnly = tier == Tier.T0 && offset > perScreen
            if (requested[image] == false || (requested[image] == true && diskOnly)) continue
            requested[image] = diskOnly
            loading[image] = Loading(index, loader.enqueue(request(image, widthPx, heightPx, diskOnly)))
        }
    }

    /** Stops what is still loading for items outside [from] to [to]: those can be asked for again. */
    private fun letGo(from: Int, to: Int) {
        val each = loading.entries.iterator()
        while (each.hasNext()) {
            val entry = each.next()
            val load = entry.value
            when {
                load.job.isDisposed -> each.remove()
                load.index !in from..to -> {
                    load.job.dispose()
                    requested.remove(entry.key)
                    each.remove()
                }
            }
        }
    }

    /** The request a tile of [widthPx] by [heightPx] makes for [image]; only to disk with [diskOnly]. */
    internal fun request(image: ArtworkImage, widthPx: Int, heightPx: Int, diskOnly: Boolean): ImageRequest =
        ImageRequest.Builder(context)
            .data(image)
            .size(widthPx, heightPx)
            .apply {
                if (diskOnly) {
                    memoryCachePolicy(CachePolicy.DISABLED)
                    decoderFactory(BlackholeDecoder.Factory())
                }
            }
            .build()

    private class Loading(val index: Int, val job: Disposable)

    private companion object {
        const val KEPT = 512
    }
}

/**
 * Prefetches [images] (one per item, by index; null for none) for a
 * [LazyListState]'s row or column whose tiles draw [widthPx] by [heightPx].
 * The prefetcher it returns can also be asked directly, for a TV row's
 * neighbours when focus moves.
 */
@Composable
fun rememberImagePrefetcher(state: LazyListState, images: List<ArtworkImage?>, widthPx: Int, heightPx: Int, tier: Tier): ImagePrefetcher {
    val prefetcher = rememberPrefetcher(tier)
    val current = rememberUpdatedState(images)
    LaunchedEffect(state, prefetcher, widthPx, heightPx) {
        // While pictures wait the scroll isn't read at all; the hold's end asks for where it stopped.
        snapshotFlow { if (prefetcher.holding) null else state.layoutInfo.visibleItemsInfo.let { (it.firstOrNull()?.index ?: 0) to (it.lastOrNull()?.index ?: -1) } }
            .distinctUntilChanged()
            .collect { range -> if (range != null) prefetcher.onVisible(range.first, range.second, current.value, widthPx, heightPx) }
    }
    return prefetcher
}

/** [rememberImagePrefetcher] for a grid. */
@Composable
fun rememberImagePrefetcher(state: LazyGridState, images: List<ArtworkImage?>, widthPx: Int, heightPx: Int, tier: Tier): ImagePrefetcher {
    val prefetcher = rememberPrefetcher(tier)
    val current = rememberUpdatedState(images)
    LaunchedEffect(state, prefetcher, widthPx, heightPx) {
        snapshotFlow { if (prefetcher.holding) null else state.layoutInfo.visibleItemsInfo.let { (it.firstOrNull()?.index ?: 0) to (it.lastOrNull()?.index ?: -1) } }
            .distinctUntilChanged()
            .collect { range -> if (range != null) prefetcher.onVisible(range.first, range.second, current.value, widthPx, heightPx) }
    }
    return prefetcher
}

@Composable
private fun rememberPrefetcher(tier: Tier): ImagePrefetcher {
    val context = LocalPlatformContext.current
    val loader = LocalArtworkImageLoader.current ?: SingletonImageLoader.get(context)
    val hold = LocalArtworkHold.current
    return remember(context, loader, tier, hold) { ImagePrefetcher(context, loader, tier, hold) }
}
