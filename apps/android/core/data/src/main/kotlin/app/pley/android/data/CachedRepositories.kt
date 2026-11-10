@file:OptIn(ExperimentalCoroutinesApi::class)

package app.pley.android.data

import app.pley.android.model.CollectionDetail
import app.pley.android.model.CollectionList
import app.pley.android.model.CollectionSummary
import app.pley.android.model.HistoryPage
import app.pley.android.model.Hubs
import app.pley.android.model.ItemCollections
import app.pley.android.model.ItemDetail
import app.pley.android.model.ItemKind
import app.pley.android.model.ItemList
import app.pley.android.model.ItemSummary
import app.pley.android.model.Library
import app.pley.android.model.LibraryFilters
import app.pley.android.model.LibraryIds
import app.pley.android.model.MaybeItem
import app.pley.android.model.PersonDetail
import app.pley.android.model.PleyJson
import app.pley.android.model.SearchResults
import kotlin.coroutines.cancellation.CancellationException
import kotlinx.coroutines.Deferred
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.async
import kotlinx.coroutines.channels.BufferOverflow
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.serialization.SerializationException
import kotlinx.serialization.builtins.ListSerializer
import kotlinx.serialization.builtins.serializer

/** Asks the server again for [resource]; a failure shows in its flow, not here. */
internal suspend fun <T> ResourceCache.refreshQuietly(resource: Resource<T>) {
    try {
        fresh(resource)
    } catch (e: CancellationException) {
        throw e
    } catch (_: Exception) {
        // Shown as Failed by the answer's flow.
    }
}

internal class CachedLibraries(
    private val cache: ResourceCache,
    /** Hears every fresh list: the scans it shows running. */
    private val heard: (List<Library>) -> Unit = {},
) : LibrariesRepository {
    private val resource = Resource(SnapshotKeys.LIBRARIES, ListSerializer(Library.serializer())) { api, _ ->
        api.libraries().also(heard)
    }

    override fun libraries(): Flow<Loadable<List<Library>>> = cache.observe(resource)

    override suspend fun refresh() = cache.refreshQuietly(resource)

    override suspend fun sort(ids: List<Long>) = holds(cache.api(cache.profile()).sortLibraries(LibraryIds(ids)))

    override suspend fun forgetOrder() = holds(cache.api(cache.profile()).forgetLibraryOrder())

    /** The server's answer to a change is the list now; Home's rows of each library follow its order. */
    private fun holds(answer: List<Library>) {
        heard(answer)
        cache.put(resource, answer)
        cache.revalidate { key, _ -> key == SnapshotKeys.HOME }
    }
}

internal class CachedHubs(private val cache: ResourceCache) : HubsRepository {
    private val home = Resource(SnapshotKeys.HOME, Hubs.serializer()) { api, _ -> api.homeHubs() }

    private fun libraryHubs(libraryId: Long) =
        Resource(SnapshotKeys.libraryHubs(libraryId), Hubs.serializer()) { api, _ -> api.libraryHubs(libraryId) }

    override fun home(): Flow<Loadable<Hubs>> = cache.observe(home)

    override fun library(libraryId: Long): Flow<Loadable<Hubs>> = cache.observe(libraryHubs(libraryId))

    /** Home, and every library's rows shown. */
    override suspend fun refresh() {
        cache.revalidate { key, _ -> key.startsWith("libhubs:") }
        cache.refreshQuietly(home)
    }
}

internal class CachedLibraryItems(
    private val cache: ResourceCache,
    /** Above this many items a listing is windowed. */
    private val windowAbove: Int,
) : LibraryItemsRepository {
    private fun filtersOf(libraryId: Long) =
        Resource(SnapshotKeys.filters(libraryId), LibraryFilters.serializer()) { api, _ -> api.libraryFilters(libraryId) }

    override fun items(libraryId: Long, query: LibraryQuery): Flow<Loadable<LibraryItems>> = cache.whileReady { profile ->
        cache.node(profile, SnapshotKeys.libraryHead(libraryId, query), detail = true) {
            LibraryListing(profile, libraryId, query, cache, windowAbove)
        }.observe()
    }

    override fun filters(libraryId: Long): Flow<Loadable<LibraryFilters>> = cache.observe(filtersOf(libraryId))

    override fun showing(libraryId: Long, query: LibraryQuery, position: Int) {
        val key = SnapshotKeys.libraryHead(libraryId, query)
        (cache.nodes().firstOrNull { it.key == key } as? LibraryListing)?.showing(position)
    }
}

internal class CachedItems(private val cache: ResourceCache) : ItemRepository {
    /** Items asked for ahead, two at a time; the oldest asks are dropped when they pile up. */
    private val prefetches = Channel<Long>(PREFETCH_QUEUE, BufferOverflow.DROP_OLDEST)
    private val prefetching by lazy {
        repeat(PREFETCH_PARALLEL) {
            cache.app.launch {
                for (id in prefetches) {
                    val resource = detailOf(id)
                    if (cache.peek(resource) == null) cache.refreshQuietly(resource)
                }
            }
        }
    }

    fun detailOf(itemId: Long) =
        Resource(SnapshotKeys.item(itemId), ItemDetail.serializer(), detail = true) { api, _ -> api.item(itemId) }

    fun childrenOf(itemId: Long) =
        Resource(SnapshotKeys.children(itemId), ItemList.serializer(), detail = true) { api, _ -> api.children(itemId) }

    fun onDeckOf(itemId: Long) =
        Resource(SnapshotKeys.onDeck(itemId), MaybeItem.serializer(), detail = true) { api, _ -> api.onDeck(itemId) }

    private fun relatedOf(itemId: Long) =
        Resource(SnapshotKeys.related(itemId), ItemList.serializer(), detail = true) { api, _ -> api.related(itemId, RELATED) }

    private fun partOf(itemId: Long) =
        Resource<ItemCollections>(SnapshotKeys.partOf(itemId), null, detail = true) { api, _ -> api.itemCollections(itemId) }

    override fun detail(itemId: Long): Flow<Loadable<ItemDetail>> = cache.observe(detailOf(itemId))

    override fun children(itemId: Long): Flow<Loadable<List<ItemSummary>>> =
        cache.observe(childrenOf(itemId)).map { it.map(ItemList::items) }

    override fun onDeck(itemId: Long): Flow<Loadable<ItemSummary?>> = cache.observe(onDeckOf(itemId)).map { it.map(MaybeItem::item) }

    override fun related(itemId: Long): Flow<Loadable<List<ItemSummary>>> =
        cache.observe(relatedOf(itemId)).map { it.map(ItemList::items) }

    /**
     * The collections the item is in. The server names them only, so they
     * come with their library (the item's) and the collection's poster
     * address, which falls back to its items' artwork.
     */
    override fun collectionsOf(itemId: Long): Flow<Loadable<List<CollectionSummary>>> = combine(
        detail(itemId).map { it.valueOrNull?.libraryId }.distinctUntilChanged(),
        cache.observe(partOf(itemId)),
    ) { libraryId, refs ->
        refs.map { list ->
            list.collections.map { ref ->
                CollectionSummary(
                    id = ref.id,
                    libraryId = libraryId ?: 0,
                    title = ref.title,
                    posterUrl = "/api/collections/${ref.id}/images/poster",
                    backdropUrl = "/api/collections/${ref.id}/images/backdrop",
                )
            }
        }
    }

    override suspend fun next(itemId: Long): ItemSummary? = cache.api(cache.profile()).next(itemId).item

    override suspend fun warmChildren(itemId: Long) {
        val resource = childrenOf(itemId)
        if (cache.peek(resource) == null) cache.refreshQuietly(resource)
    }

    override fun seed(item: ItemSummary, next: ItemSummary?) {
        val detail = detailOf(item.id)
        cache.seed(detail, ItemDetail.of(item))
        // Asked for though a stand-in is held, which [prefetch] below would take for the answer.
        cache.prefetch(detail)
        if (item.kind == ItemKind.Show || item.kind == ItemKind.Season) {
            val onDeck = onDeckOf(item.id)
            if (next != null) cache.seed(onDeck, MaybeItem(next))
            cache.prefetch(onDeck)
        }
    }

    override fun held(itemId: Long): ItemDetail? = cache.peek(detailOf(itemId))

    override fun heldChildren(itemId: Long): List<ItemSummary>? = cache.peek(childrenOf(itemId))?.items

    override fun heldOnDeck(itemId: Long): Loadable<ItemSummary?>? = cache.peek(onDeckOf(itemId))?.let { Loadable.Ready(it.item, fresh = false) }

    /** Loads [itemId]'s page ahead, unless it is held: never every visible card (asking queues work on the server). */
    override fun prefetch(itemId: Long) {
        prefetching
        prefetches.trySend(itemId)
    }

    companion object {
        /** "More Like This" items asked for. */
        const val RELATED = 20
        private const val PREFETCH_QUEUE = 8
        private const val PREFETCH_PARALLEL = 2
    }
}

internal class CachedPeople(private val cache: ResourceCache) : PeopleRepository {
    override fun person(personId: Long): Flow<Loadable<PersonDetail>> = cache.observe(
        Resource(SnapshotKeys.person(personId), PersonDetail.serializer(), detail = true) { api, _ -> api.person(personId) },
    )
}

internal class CachedCollections(private val cache: ResourceCache) : CollectionsRepository {
    override fun collections(libraryId: Long): Flow<Loadable<List<CollectionSummary>>> = cache.observe(
        Resource(SnapshotKeys.collections(libraryId), CollectionList.serializer()) { api, _ -> api.libraryCollections(libraryId) },
    ).map { it.map(CollectionList::collections) }

    override fun collection(collectionId: Long): Flow<Loadable<CollectionDetail>> = cache.observe(
        Resource(SnapshotKeys.collection(collectionId), CollectionDetail.serializer(), detail = true) { api, _ ->
            api.collection(collectionId)
        },
    )
}

/**
 * Search: only the latest query is asked (a newer one cancels the older
 * request), and its answer is the one kept. Recent searches are kept per
 * profile, as a snapshot.
 */
internal class CachedSearch(private val cache: ResourceCache) : SearchRepository {
    private val lock = Any()
    private var running: Deferred<Result<SearchResults>>? = null
    private var last: Triple<ProfileScope, String, SearchResults>? = null

    private var recentOf: Pair<ProfileScope, MutableStateFlow<List<String>?>>? = null

    override fun search(query: String): Flow<Loadable<SearchResults>> = cache.whileReady { profile ->
        flow {
            val q = query.trim()
            if (q.isEmpty()) {
                emit(Loadable.Ready(SearchResults(), fresh = true))
                return@flow
            }
            synchronized(lock) { last?.takeIf { it.first == profile && it.second == q }?.third }?.let {
                emit(Loadable.Ready(it, fresh = true))
                return@flow
            }
            emit(Loadable.Loading)
            val request = synchronized(lock) {
                running?.cancel()
                cache.app.async {
                    try {
                        Result.success(cache.api(profile).search(q, LIMIT))
                    } catch (e: CancellationException) {
                        throw e
                    } catch (e: Exception) {
                        Result.failure(e)
                    }
                }.also { running = it }
            }
            val answer = try {
                request.await()
            } catch (e: CancellationException) {
                // A newer search took over: this one ends quietly.
                if (!currentCoroutineContext().isActive || !request.isCancelled) throw e
                return@flow
            }
            answer.fold(
                onSuccess = { results ->
                    synchronized(lock) { last = Triple(profile, q, results) }
                    emit(Loadable.Ready(results, fresh = true))
                },
                onFailure = { emit(Loadable.Failed(it)) },
            )
        }
    }

    override val recent: Flow<List<String>> = cache.scopes.flatMapLatest { state ->
        if (state is ScopeState.Ready) recentFlow(state.profile) else flowOf(emptyList())
    }

    private fun recentFlow(profile: ProfileScope): Flow<List<String>> = flow {
        val held = synchronized(lock) {
            recentOf?.takeIf { it.first == profile }?.second ?: MutableStateFlow<List<String>?>(null).also { recentOf = profile to it }
        }
        if (held.value == null) held.compareAndSet(null, read(profile))
        held.collect { if (it != null) emit(it) }
    }.distinctUntilChanged()

    override suspend fun remember(query: String) {
        val q = query.trim().takeIf { it.isNotEmpty() } ?: return
        save { list -> (listOf(q) + list.filterNot { it.equals(q, ignoreCase = true) }).take(RECENT) }
    }

    override suspend fun clearRecent() = save { emptyList() }

    private suspend fun save(change: (List<String>) -> List<String>) {
        val profile = cache.profile()
        val held = synchronized(lock) {
            recentOf?.takeIf { it.first == profile }?.second ?: MutableStateFlow<List<String>?>(null).also { recentOf = profile to it }
        }
        val before = held.value ?: read(profile)
        val after = change(before)
        held.value = after
        if (after.isEmpty()) {
            cache.snapshots.delete(profile, SnapshotKeys.SEARCHES)
        } else {
            cache.snapshots.write(profile, SnapshotKeys.SEARCHES, PleyJson.encodeToString(STRINGS, after).encodeToByteArray())
        }
    }

    private suspend fun read(profile: ProfileScope): List<String> {
        val bytes = cache.snapshots.read(profile, SnapshotKeys.SEARCHES) ?: return emptyList()
        return try {
            PleyJson.decodeFromString(STRINGS, bytes.decodeToString()).take(RECENT)
        } catch (_: SerializationException) {
            emptyList()
        } catch (_: IllegalArgumentException) {
            emptyList()
        }
    }

    companion object {
        /** Results asked for, of titles, people and channels each: the server's most, so no group is cut short. */
        const val LIMIT = 200

        /** Recent searches kept per profile. */
        const val RECENT = 10
        private val STRINGS = ListSerializer(String.serializer())
    }
}

/**
 * The profile's plays, newest first, in pages; the first page is the
 * snapshot. Asking again keeps as many entries as were shown.
 */
internal class CachedHistory(private val cache: ResourceCache) : HistoryRepository {
    private val resource = Resource(
        key = SnapshotKeys.HISTORY,
        serializer = HistoryPage.serializer(),
        trim = { it.copy(entries = it.entries.take(PAGE)) },
    ) { api, current -> api.history(offset = 0, limit = (current?.entries?.size ?: 0).coerceIn(PAGE, MAX_LIMIT)) }

    override fun history(): Flow<Loadable<HistoryPage>> = cache.observe(resource)

    override suspend fun loadMore() {
        val shown = cache.peek(resource) ?: return
        if (shown.entries.size >= shown.total) return
        val page = cache.api(cache.profile()).history(offset = shown.entries.size, limit = PAGE)
        val now = cache.peek(resource) ?: shown
        val known = now.entries.mapTo(HashSet()) { it.id }
        cache.put(resource, now.copy(total = page.total, entries = now.entries + page.entries.filter { it.id !in known }))
    }

    /** Removes the entry at once, and puts it back if the server refuses. */
    override suspend fun remove(entryId: Long) {
        val patched = cache.patchKey(SnapshotKeys.HISTORY) { value ->
            val page = value as HistoryPage
            if (page.entries.none { it.id == entryId }) {
                null
            } else {
                page.copy(total = (page.total - 1).coerceAtLeast(0), entries = page.entries.filterNot { it.id == entryId })
            }
        }
        try {
            cache.api(cache.profile()).deleteHistoryEntry(entryId)
        } catch (e: Exception) {
            patched.rollback()
            throw e
        }
    }

    companion object {
        /** Entries per page. */
        const val PAGE = 15

        /** The most the server gives at once. */
        private const val MAX_LIMIT = 200
    }
}
