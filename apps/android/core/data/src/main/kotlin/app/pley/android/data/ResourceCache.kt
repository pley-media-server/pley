@file:OptIn(ExperimentalCoroutinesApi::class)

package app.pley.android.data

import app.pley.android.auth.SignedOutException
import app.pley.android.model.PleyJson
import app.pley.android.network.PleyApi
import java.util.concurrent.atomic.AtomicInteger
import kotlin.coroutines.cancellation.CancellationException
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Deferred
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.Job
import kotlinx.coroutines.async
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.emitAll
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import kotlinx.serialization.KSerializer
import kotlinx.serialization.SerializationException

/** Whose answers the repositories read now. */
internal sealed interface ScopeState {
    /** Nobody's yet: starting, no server, or asking the server who. */
    data object Waiting : ScopeState

    data class Ready(val profile: ProfileScope) : ScopeState

    /** Signed in, but the profile is unknown and the server can't say: [error]. */
    data class Unavailable(val error: Throwable) : ScopeState

    /** Signed out of the server in use: nothing loads until the person signs in again. */
    data object SignedOut : ScopeState
}

/**
 * How to load one answer: under [key], kept on disk with [serializer]
 * (`null`: in memory only), [detail] answers (item pages and the like)
 * in a memory LRU of [ResourceCache.DETAILS] entries. [fetch] asks the
 * server, with what is shown now; [trim] is what of an answer goes to disk.
 */
internal class Resource<T>(
    val key: String,
    val serializer: KSerializer<T>?,
    val detail: Boolean = false,
    val trim: (T) -> T = { it },
    val fetch: suspend (api: PleyApi, current: T?) -> T,
)

/** Something the cache holds for the profile in use: an answer, or a library's listing. */
internal interface CacheNode {
    val key: String

    /** Collected now. */
    val active: Boolean

    /** The value shown now, if any. */
    val current: Any?

    /** When the server last answered, on the cache's clock; [ResourceCache.NEVER] when it hasn't. */
    val fetchedAt: Long

    /**
     * Changes the value shown with [transform] (`null`: unchanged); the
     * undo, or `null` when nothing changed.
     */
    fun patch(transform: (Any) -> Any?): (() -> Unit)?

    /** Asks the server again now. */
    fun refresh()

    /** Asks the server again the next time it is collected. */
    fun markStale()
}

/**
 * What the repositories share: every answer of the profile in use, in
 * memory, read from its snapshot first and then from the server.
 *
 * - Collecting an answer gives Loading, then the snapshot as Ready(fresh =
 *   false), then the server's as Ready(fresh = true), which is written
 *   back as the snapshot (when it changed). A failure keeps what was
 *   shown, as Failed(stale).
 * - One request per answer at a time, however many collect it; requests
 *   run in [app], so leaving a screen doesn't waste them.
 * - An answer collected again asks the server again when it is older than
 *   [REVALIDATE_MS].
 * - Switching profiles ([reset]) drops everything held in memory.
 *
 * [clock] is monotonic, in ms (the test's virtual time in tests).
 */
internal class ResourceCache(
    val scopes: StateFlow<ScopeState>,
    private val api: (ProfileScope) -> PleyApi,
    val snapshots: FileSnapshotStore,
    val app: CoroutineScope,
    val clock: () -> Long,
    val compute: CoroutineDispatcher = Dispatchers.Default,
) {
    private val lock = Any()
    private var owner: ProfileScope? = null

    /** Item pages and the like: the least recently used go first, beyond [DETAILS]. */
    private val details = LinkedHashMap<String, CacheNode>(DETAILS * 2, 0.75f, true)

    /** The rest: a few per library at most. */
    private val others = HashMap<String, CacheNode>()

    /** [resource] for whoever is signed in: follows profile switches. */
    fun <T> observe(resource: Resource<T>): Flow<Loadable<T>> = whileReady { profile -> observe(profile, resource) }

    /**
     * [block]'s flow for the profile in use, Loading while there is none
     * yet, Failed when there won't be one (signed out).
     */
    fun <T> whileReady(block: (ProfileScope) -> Flow<Loadable<T>>): Flow<Loadable<T>> = scopes.flatMapLatest { state ->
        when (state) {
            is ScopeState.Ready -> block(state.profile)
            is ScopeState.Unavailable -> flowOf(Loadable.Failed(state.error))
            ScopeState.SignedOut -> flowOf(Loadable.Failed(SignedOutException()))
            ScopeState.Waiting -> flowOf(Loadable.Loading)
        }
    }.distinctUntilChanged()

    private fun <T> observe(profile: ProfileScope, resource: Resource<T>): Flow<Loadable<T>> = flow {
        val entry = entry(profile, resource)
        entry.subscribers.incrementAndGet()
        try {
            entry.start()
            emitAll(entry.state)
        } finally {
            entry.subscribers.decrementAndGet()
        }
    }

    /** [resource] from the server now, for the profile in use (waiting for one); throws when the server can't answer. */
    suspend fun <T> fresh(resource: Resource<T>): T {
        val profile = profile()
        return entry(profile, resource).load().await().getOrThrow()
    }

    /** [resource]'s value held in memory, if any. */
    fun <T> peek(resource: Resource<T>): T? {
        val profile = (scopes.value as? ScopeState.Ready)?.profile ?: return null
        @Suppress("UNCHECKED_CAST")
        return synchronized(lock) { if (owner == profile) nodeOf(resource.key)?.current as T? else null }
    }

    /** Loads [resource] ahead, unless it is held and fresh. */
    fun <T> prefetch(resource: Resource<T>) {
        val profile = (scopes.value as? ScopeState.Ready)?.profile ?: return
        val entry = entry(profile, resource)
        if (entry.state.value !is Loadable.Ready || entry.stale()) entry.start()
    }

    /**
     * [value] stands in for [resource]'s answer while nothing is known of
     * it: shown, not fresh, until its snapshot or the server's answer
     * comes, which it is still asked for. Never written to disk, and it
     * replaces nothing that is held.
     */
    fun <T> seed(resource: Resource<T>, value: T) {
        val profile = (scopes.value as? ScopeState.Ready)?.profile ?: return
        entry(profile, resource).seed(value)
    }

    /** [value] is [resource]'s answer now (the server gave it with a change). */
    fun <T> put(resource: Resource<T>, value: T) {
        val profile = (scopes.value as? ScopeState.Ready)?.profile ?: return
        entry(profile, resource).put(value)
    }

    /** The profile in use, waiting while there is none yet; throws when it can't be known. */
    suspend fun profile(): ProfileScope = when (val state = scopes.first { it !is ScopeState.Waiting }) {
        is ScopeState.Ready -> state.profile
        is ScopeState.Unavailable -> throw state.error
        ScopeState.SignedOut -> throw SignedOutException()
        ScopeState.Waiting -> error("unreachable")
    }

    fun api(profile: ProfileScope): PleyApi = api.invoke(profile)

    /** The node under [key] for [profile], made by [create] when there's none. */
    fun <N : CacheNode> node(profile: ProfileScope, key: String, detail: Boolean, create: () -> N): N = synchronized(lock) {
        if (owner != profile) {
            details.clear()
            others.clear()
            owner = profile
        }
        @Suppress("UNCHECKED_CAST")
        val known = (if (detail) details[key] else others[key]) as N?
        known ?: create().also {
            if (detail) {
                details[key] = it
                trimDetails()
            } else {
                others[key] = it
            }
        }
    }

    private fun <T> entry(profile: ProfileScope, resource: Resource<T>): Entry<T> =
        node(profile, resource.key, resource.detail) { Entry(profile, resource, this) }

    /** Must hold [lock]. */
    private fun nodeOf(key: String): CacheNode? = details[key] ?: others[key]

    /** Must hold [lock]. */
    private fun trimDetails() {
        if (details.size <= DETAILS) return
        val it = details.values.iterator()
        while (details.size > DETAILS && it.hasNext()) {
            if (!it.next().active) it.remove()
        }
    }

    /** Every node held for the profile in use. */
    fun nodes(): List<CacheNode> {
        val profile = (scopes.value as? ScopeState.Ready)?.profile ?: return emptyList()
        return synchronized(lock) { if (owner == profile) details.values + others.values else emptyList() }
    }

    /**
     * Changes every value held with [transform] at once (an optimistic
     * change); the undo, which puts back what wasn't changed since.
     */
    fun patchAll(transform: (Any) -> Any?): Patched {
        val undo = mutableListOf<() -> Unit>()
        val keys = mutableListOf<String>()
        for (node in nodes()) {
            node.patch(transform)?.let {
                undo += it
                keys += node.key
            }
        }
        return Patched(keys, undo)
    }

    /** Changes the value under [key] only. */
    fun patchKey(key: String, transform: (Any) -> Any?): Patched {
        val node = nodes().firstOrNull { it.key == key } ?: return Patched(emptyList(), emptyList())
        val undo = node.patch(transform) ?: return Patched(emptyList(), emptyList())
        return Patched(listOf(key), listOf(undo))
    }

    /**
     * Asks the server again for what [match]es (by key and value) and is
     * older than [olderThanMs]: now for what is collected, at the next
     * collection for the rest.
     */
    fun revalidate(olderThanMs: Long = 0, match: (key: String, value: Any?) -> Boolean) {
        val now = clock()
        for (node in nodes()) {
            if (!match(node.key, node.current)) continue
            val age = if (node.fetchedAt == NEVER) Long.MAX_VALUE else now - node.fetchedAt
            if (age < olderThanMs) continue
            if (node.active) node.refresh() else node.markStale()
        }
    }

    /** Drops what is held in memory: another profile, or signed out. */
    fun reset() = synchronized(lock) {
        details.clear()
        others.clear()
        owner = null
    }

    /** What [match]es is asked for again the next time it is collected, not now. */
    fun markStale(match: (key: String, value: Any?) -> Boolean) {
        for (node in nodes()) if (match(node.key, node.current)) node.markStale()
    }

    /** What an optimistic change touched, and how to undo it. */
    class Patched(val keys: List<String>, private val undo: List<() -> Unit>) {
        fun rollback() = undo.asReversed().forEach { it() }
    }

    companion object {
        /** Item pages and the like held in memory. */
        const val DETAILS = 50

        /** An answer collected again is asked for again when older than this. */
        const val REVALIDATE_MS = 30_000L

        const val NEVER = -1L
    }
}

/** One answer of one profile: its state, its request, its snapshot. */
internal class Entry<T>(
    private val profile: ProfileScope,
    private val resource: Resource<T>,
    private val cache: ResourceCache,
) : CacheNode {
    val state = MutableStateFlow<Loadable<T>>(Loadable.Loading)
    val subscribers = AtomicInteger()

    @Volatile
    override var fetchedAt: Long = ResourceCache.NEVER
        private set

    private var snapshotJob: Job? = null
    private var inFlight: Deferred<Result<T>>? = null

    /** The snapshot on disk, as a hash of its bytes: an unchanged answer isn't written again. */
    @Volatile
    private var snapshotHash: Int? = null

    /** What [seed] put in [state], while it is still what shows: the snapshot, an answer once, says more. */
    @Volatile
    private var seeded: Loadable<T>? = null

    override val key: String get() = resource.key
    override val active: Boolean get() = subscribers.get() > 0
    override val current: Any? get() = state.value.valueOrNull

    fun stale(): Boolean = fetchedAt == ResourceCache.NEVER || cache.clock() - fetchedAt >= ResourceCache.REVALIDATE_MS

    /** Reads the snapshot (once) and asks the server when what is held is stale. */
    fun start() {
        val serializer = resource.serializer
        synchronized(this) {
            if (snapshotJob == null && serializer != null) snapshotJob = cache.app.launch { readSnapshot(serializer) }
        }
        if (stale()) {
            // What is held is shown while the server is asked again: no
            // longer the server's latest (a player waits a moment for that).
            state.update { if (it is Loadable.Ready && it.fresh) Loadable.Ready(it.value, fresh = false) else it }
            load()
        }
    }

    /**
     * The request for this answer: the one running, or a new one. A
     * failure is its result, not the job's: it shows in [state], and the
     * app's scope never hears of it.
     */
    fun load(): Deferred<Result<T>> = synchronized(this) {
        inFlight?.takeIf { it.isActive }?.let { return it }
        cache.app.async {
            try {
                val value = resource.fetch(cache.api(profile), state.value.valueOrNull)
                put(value)
                Result.success(value)
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                state.update { Loadable.Failed(e, it.valueOrNull) }
                Result.failure(e)
            }
        }.also { inFlight = it }
    }

    fun put(value: T) {
        fetchedAt = cache.clock()
        state.value = Loadable.Ready(value, fresh = true)
        persist(value)
    }

    /** Shows [value] while nothing else is: [fetchedAt] stays never, so the server is still asked. */
    fun seed(value: T) {
        val stand = Loadable.Ready(value, fresh = false)
        // Named before it shows, so a snapshot read meanwhile knows it; one that never shows matches nothing.
        seeded = stand
        state.compareAndSet(Loadable.Loading, stand)
    }

    override fun refresh() {
        load()
    }

    override fun markStale() {
        fetchedAt = ResourceCache.NEVER
    }

    override fun patch(transform: (Any) -> Any?): (() -> Unit)? {
        val before = state.value
        val value = before.valueOrNull ?: return null
        val after = transform(value) ?: return null
        if (after === value) return null
        @Suppress("UNCHECKED_CAST")
        val patched: Loadable<T> = when (before) {
            is Loadable.Ready -> Loadable.Ready(after as T, before.fresh)
            is Loadable.Failed -> Loadable.Failed(before.error, after as T)
            Loadable.Loading -> return null
        }
        state.value = patched
        return { state.compareAndSet(patched, before) }
    }

    private suspend fun readSnapshot(serializer: KSerializer<T>) {
        val bytes = cache.snapshots.read(profile, resource.key) ?: return
        val value = withContext(cache.compute) {
            try {
                PleyJson.decodeFromString(serializer, bytes.decodeToString())
            } catch (_: SerializationException) {
                null
            } catch (_: IllegalArgumentException) {
                null
            }
        } ?: return
        snapshotHash = bytes.contentHashCode()
        state.update { shown ->
            when (shown) {
                Loadable.Loading -> Loadable.Ready(value, fresh = false)
                is Loadable.Failed -> if (shown.stale == null) Loadable.Failed(shown.error, value) else shown
                is Loadable.Ready -> if (shown === seeded) Loadable.Ready(value, fresh = false) else shown
            }
        }
    }

    private fun persist(value: T) {
        val serializer = resource.serializer ?: return
        cache.app.launch {
            val bytes = withContext(cache.compute) {
                PleyJson.encodeToString(serializer, resource.trim(value)).encodeToByteArray()
            }
            val hash = bytes.contentHashCode()
            if (hash == snapshotHash) return@launch
            snapshotHash = hash
            cache.snapshots.write(profile, resource.key, bytes)
        }
    }
}
