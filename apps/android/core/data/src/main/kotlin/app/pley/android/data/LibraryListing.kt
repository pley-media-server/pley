@file:OptIn(ExperimentalCoroutinesApi::class)

package app.pley.android.data

import app.pley.android.model.ItemList
import app.pley.android.model.ItemSummary
import app.pley.android.model.ItemsPage
import app.pley.android.model.PleyJson
import app.pley.android.network.PleyApi
import java.util.concurrent.atomic.AtomicInteger
import kotlin.coroutines.cancellation.CancellationException
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.Job
import kotlinx.coroutines.async
import kotlinx.coroutines.awaitAll
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.channelFlow
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import kotlinx.serialization.SerializationException

/**
 * One library listed one way ([query]), as one list for the grid:
 *
 * - the first [HEAD] items first, for the first frame (also kept as the
 *   snapshot);
 * - then pages of [PAGE], [PARALLEL] at a time, each pair making a new
 *   list with everything so far;
 * - above [windowAbove] items, only a window of two pages around what the
 *   grid shows ([showing]), the last [KEPT_PAGES] pages read kept for
 *   scrolling back.
 */
internal class LibraryListing(
    private val profile: ProfileScope,
    private val libraryId: Long,
    private val query: LibraryQuery,
    private val cache: ResourceCache,
    private val windowAbove: Int,
) : CacheNode {
    override val key: String = SnapshotKeys.libraryHead(libraryId, query)

    val state = MutableStateFlow<Loadable<LibraryItems>>(Loadable.Loading)
    private val subscribers = AtomicInteger()

    /** The position the grid shows. */
    private val viewport = MutableStateFlow(0)

    /** Set once the listing is known to be windowed: its total, and which load it is of. */
    private val windowed = MutableStateFlow<WindowSource?>(null)

    private val pages = LinkedHashMap<Int, List<ItemSummary>>(KEPT_PAGES * 2, 0.75f, true)
    private val lock = Any()
    private var load: Job? = null
    private var snapshotRead = false
    private var generation = 0

    @Volatile
    private var shownWindow: WindowShown? = null

    @Volatile
    override var fetchedAt: Long = ResourceCache.NEVER
        private set

    override val active: Boolean get() = subscribers.get() > 0
    override val current: Any? get() = state.value.valueOrNull

    /** The listing as the grid collects it: while collected, a windowed listing follows [showing]. */
    fun observe(): Flow<Loadable<LibraryItems>> = channelFlow {
        subscribers.incrementAndGet()
        try {
            start()
            launch { followViewport() }
            state.collect { send(it) }
        } finally {
            subscribers.decrementAndGet()
        }
    }

    fun showing(position: Int) {
        viewport.value = position.coerceAtLeast(0)
    }

    private fun start() {
        val readSnapshot = synchronized(lock) { !snapshotRead.also { snapshotRead = true } }
        if (readSnapshot) cache.app.launch { readSnapshot() }
        val stale = fetchedAt == ResourceCache.NEVER || cache.clock() - fetchedAt >= ResourceCache.REVALIDATE_MS
        if (stale) refresh()
    }

    override fun refresh() {
        synchronized(lock) {
            if (load?.isActive == true) return
            load = cache.app.launch { load() }
        }
    }

    override fun markStale() {
        fetchedAt = ResourceCache.NEVER
    }

    override fun patch(transform: (Any) -> Any?): (() -> Unit)? {
        val before = state.value
        val value = before.valueOrNull ?: return null
        val after = transform(value) as LibraryItems? ?: return null
        if (after === value) return null
        synchronized(lock) {
            for ((page, items) in pages.entries.toList()) {
                val list = ItemList(items)
                (transform(list) as ItemList?)?.let { pages[page] = it.items }
            }
        }
        val patched: Loadable<LibraryItems> = when (before) {
            is Loadable.Ready -> Loadable.Ready(after, before.fresh)
            is Loadable.Failed -> Loadable.Failed(before.error, after)
            Loadable.Loading -> return null
        }
        state.value = patched
        // The pages read again fix the cached ones, if the change is undone.
        return { if (state.compareAndSet(patched, before)) markStale() }
    }

    private suspend fun readSnapshot() {
        val bytes = cache.snapshots.read(profile, key) ?: return
        val head = withContext(cache.compute) {
            try {
                PleyJson.decodeFromString(ItemsPage.serializer(), bytes.decodeToString())
            } catch (_: SerializationException) {
                null
            } catch (_: IllegalArgumentException) {
                null
            }
        } ?: return
        val items = LibraryItems(head.total, head.items, windowed = head.total > windowAbove)
        state.update { shown ->
            when (shown) {
                Loadable.Loading -> Loadable.Ready(items, fresh = false)
                is Loadable.Failed -> if (shown.stale == null) Loadable.Failed(shown.error, items) else shown
                is Loadable.Ready -> shown
            }
        }
    }

    private suspend fun load() {
        try {
            val api = cache.api(profile)
            val head = page(api, 0, HEAD)
            fetchedAt = cache.clock()
            persist(head)
            if (head.total > windowAbove) {
                synchronized(lock) {
                    pages.clear()
                    generation++
                }
                // The head shows until the window around the grid's position is read;
                // a window shown before stays until the new one replaces it.
                val shown = state.value.valueOrNull
                if (shown == null || !shown.windowed || shown.items.size <= HEAD) {
                    state.value = Loadable.Ready(LibraryItems(head.total, head.items, windowed = true), fresh = true)
                }
                windowed.value = WindowSource(head.total, generation)
            } else {
                windowed.value = null
                loadAll(api, head)
            }
        } catch (e: CancellationException) {
            throw e
        } catch (e: Throwable) {
            state.update { Loadable.Failed(e, it.valueOrNull) }
        }
    }

    /** Every item, the head first, then pages of [PAGE] two at a time. */
    private suspend fun loadAll(api: PleyApi, head: ItemsPage) {
        var items: List<ItemSummary> = head.items
        var total = head.total
        state.value = Loadable.Ready(LibraryItems(total, items), fresh = true)
        var offset = items.size
        while (offset < total) {
            val offsets = (0 until PARALLEL).map { offset + it * PAGE }.filter { it < total }
            val read = coroutineScope { offsets.map { o -> async { page(api, o, PAGE) } }.awaitAll() }
            val next = ArrayList<ItemSummary>(items.size + read.sumOf { it.items.size })
            next.addAll(items)
            for (p in read) next.addAll(p.items)
            items = next
            total = read.last().total
            state.value = Loadable.Ready(LibraryItems(total, items), fresh = true)
            // The listing got shorter while it was read: what came is all.
            if (read.any { it.items.size < PAGE }) break
            offset = offsets.last() + PAGE
        }
    }

    /** While collected: the window follows the grid. */
    private suspend fun followViewport() {
        combine(viewport, windowed.filterNotNull()) { position, source -> position to source }
            .collectLatest { (position, source) -> moveWindow(position, source) }
    }

    private suspend fun moveWindow(position: Int, source: WindowSource) {
        val total = source.total
        val lastPage = ((total - 1) / PAGE).coerceAtLeast(0)
        val page = (position / PAGE).coerceAtMost(lastPage)
        val first = (if (position % PAGE < PAGE / 2) page - 1 else page).coerceIn(0, (lastPage - 1).coerceAtLeast(0))
        val wanted = listOf(first, first + 1).filter { it <= lastPage }
        if (shownWindow == WindowShown(source.generation, first) && state.value is Loadable.Ready) return
        try {
            val api = cache.api(profile)
            val missing = synchronized(lock) { wanted.filter { it !in pages } }
            val read = coroutineScope { missing.map { p -> async { p to page(api, p * PAGE, PAGE).items } }.awaitAll() }
            val window = synchronized(lock) {
                if (generation != source.generation) return
                for ((p, items) in read) pages[p] = items
                while (pages.size > KEPT_PAGES) {
                    val eldest = pages.keys.first { it !in wanted }
                    pages.remove(eldest)
                }
                val list = ArrayList<ItemSummary>(wanted.size * PAGE)
                for (p in wanted) list.addAll(pages[p].orEmpty())
                list
            }
            state.value = Loadable.Ready(LibraryItems(total, window, offset = first * PAGE, windowed = true), fresh = true)
            shownWindow = WindowShown(source.generation, first)
        } catch (e: CancellationException) {
            throw e
        } catch (e: Throwable) {
            state.update { Loadable.Failed(e, it.valueOrNull) }
        }
    }

    private suspend fun page(api: PleyApi, offset: Int, limit: Int): ItemsPage = api.libraryItems(
        libraryId = libraryId,
        sort = query.sort,
        order = query.order,
        watched = query.unwatched?.not(),
        genre = query.genre,
        year = query.year,
        resolution = query.resolution,
        dynamicRange = query.dynamicRange,
        offset = offset,
        limit = limit,
    )

    private fun persist(head: ItemsPage) {
        cache.app.launch {
            val bytes = withContext(cache.compute) { PleyJson.encodeToString(ItemsPage.serializer(), head).encodeToByteArray() }
            cache.snapshots.write(profile, key, bytes)
        }
    }

    private data class WindowSource(val total: Int, val generation: Int)

    private data class WindowShown(val generation: Int, val firstPage: Int)

    companion object {
        /** Items asked for first, for the first frame. */
        const val HEAD = 200

        /** Items per page after the head. */
        const val PAGE = 1_000

        /** Pages asked for at once. */
        const val PARALLEL = 2

        /** Above this many items a listing is windowed (phones and most TVs). */
        const val WINDOW_ABOVE = 5_000

        /** Above this many on the smallest TVs (T0). */
        const val WINDOW_ABOVE_T0 = 2_000

        /** Pages of a windowed listing kept in memory: the window and the two before it. */
        const val KEPT_PAGES = 4
    }
}
