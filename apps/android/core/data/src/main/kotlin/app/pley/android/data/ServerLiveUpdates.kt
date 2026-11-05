@file:OptIn(ExperimentalCoroutinesApi::class)

package app.pley.android.data

import app.pley.android.model.CollectionDetail
import app.pley.android.model.Hubs
import app.pley.android.model.ItemDetail
import app.pley.android.model.ItemList
import app.pley.android.model.Library
import app.pley.android.model.MaybeItem
import app.pley.android.model.PlaylistDetail
import app.pley.android.model.CastKind
import app.pley.android.model.ServerEvent
import app.pley.android.network.EventReceiver
import app.pley.android.network.LiveEvent
import app.pley.android.network.ServerEvents
import java.io.IOException
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicReference
import kotlin.coroutines.cancellation.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.Job
import kotlinx.coroutines.awaitCancellation
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.asSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.catch
import kotlinx.coroutines.flow.emitAll
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.transformLatest
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

/**
 * The server's events, while the app is in the [foreground] and signed in,
 * or signed in while a player has a session ([playing]: a remote's
 * commands reach it in the background too):
 *
 * - `library_changed`, held [DEBOUNCE_MS] per library (a scan sends many),
 *   asks again for what shows that library: its rows, listings and
 *   collections, Home, the libraries, and the pages of its items;
 * - `libraries_changed` (a library added, renamed or removed, their order,
 *   or what an administrator lets this profile see: its libraries, its age
 *   limit) asks again for everything held, and for who the profile is: a
 *   kid's Home fills once their age limit is lifted;
 * - `watchlist_changed` (My List changed on another device) asks again
 *   for My List, Home and the pages of movies and shows;
 * - `live_channels_changed` (Live TV's sources, channels or guide) asks
 *   again for Live TV's summary, channels and guide;
 * - `user_state_changed` (something watched, started or taken out of
 *   Continue Watching, here or on another device), gathered for
 *   [STATE_MS], asks again for Home, the libraries' rows and what shows
 *   those items ([touches]), but not for what was fetched since it was
 *   heard (a change made here, asked for already); a library change
 *   waiting then is asked for in the same round (a play that ends sends
 *   both);
 * - a new connection (the server replays nothing) and every return to the
 *   foreground, the stream open or not, ask again for what is older than
 *   [STALE_MS];
 * - scans fill [scans] (GET /api/activity refuses apps' tokens).
 *
 * In the background (the stream open for a player), what the events touch
 * is only marked to be asked for again when next shown: no screen shows it
 * meanwhile, and a phone playing music on a weak connection would fetch it
 * all again at each reconnection.
 *
 * The stream ends for good when the server refuses the app (401, 403):
 * until the next return to the foreground or sign-in, or, in the
 * background, until the next playback.
 *
 * In the foreground the stream names this device as a receiver
 * ([receiverId], the app's install id), so the profile's remotes cast to
 * it: titles always, Live TV's channels while Live TV is open to the
 * profile ([liveTv], `null` while not known yet: the stream waits for it
 * [LIVE_WAIT_MS] at most, so it doesn't open twice); and it says it
 * switches to the profile of a remote on another profile of the household
 * that casts to it (`switch`). In the background,
 * where nothing cast would play, it names none: going there and back
 * opens it again.
 */
internal class ServerLiveUpdates(
    private val session: SessionManager,
    private val foreground: Flow<Boolean>,
    private val eventsOf: (issuer: String) -> ServerEvents,
    private val cache: ResourceCache,
    private val app: CoroutineScope,
    private val receiverId: suspend () -> String? = { null },
    private val liveTv: () -> Flow<Boolean?> = { flowOf(false) },
) : LiveUpdates {
    private val _scans = MutableStateFlow<List<ScanState>>(emptyList())
    private val readScans = _scans.asStateFlow()

    // Nothing else starts it in the app: reading the scans does (LiveUpdates.scans).
    override val scans: StateFlow<List<ScanState>>
        get() = readScans.also { start() }

    private val started = AtomicBoolean(false)

    private val heard = MutableSharedFlow<ServerEvent>(extraBufferCapacity = HEARD_BUFFER)

    override val events: Flow<ServerEvent>
        get() = heard.asSharedFlow().also { start() }

    private val connected = MutableSharedFlow<Unit>(extraBufferCapacity = 1)

    override val answered: Flow<Unit>
        get() = connected.asSharedFlow().also { start() }

    private val _streaming = MutableStateFlow(false)

    override val streaming: StateFlow<Boolean> get() = _streaming.asStateFlow()

    private val _receiver = MutableStateFlow<String?>(null)

    override val receiver: StateFlow<String?> get() = _receiver.asStateFlow()

    /** A stream to open: the server's, naming this device as [receiver] or as none. */
    private data class Stream(val issuer: String, val receiver: EventReceiver?)

    /** The players with a session on the server now ([playing]). */
    private val players = MutableStateFlow<Set<Any>>(emptySet())

    override fun start() {
        if (!started.compareAndSet(false, true)) return
        app.launch {
            val id = ownId()
            val signedIn = session.state.map { (it as? SessionState.SignedIn)?.session?.issuer }.distinctUntilChanged()
            val anyPlaying = players.map { it.isNotEmpty() }.distinctUntilChanged()
            combine(foreground.distinctUntilChanged(), anyPlaying, signedIn, liveTvOpen()) { inFront, playing, issuer, live ->
                val casts = listOf(CastKind.Item) + (if (live) listOf(CastKind.Live) else emptyList()) + CastKind.Switch
                issuer?.takeIf { inFront || playing }?.let { Stream(it, id?.takeIf { inFront }?.let { r -> EventReceiver(r, casts) }) }
            }
                .distinctUntilChanged()
                .collectLatest { stream ->
                    if (stream == null) {
                        _scans.value = emptyList()
                        return@collectLatest
                    }
                    while (true) {
                        follow(stream.issuer, stream.receiver)
                        // Refused: tried again at the next return to the foreground. A stream that names this
                        // device in front changes with it, and the combination above opens it again; one that
                        // names none either way (no id to name), held open in the background by a player,
                        // doesn't.
                        if (id != null) awaitCancellation()
                        foreground.first { !it }
                        foreground.first { it }
                    }
                }
        }
    }

    override fun playing(player: Any, playing: Boolean) {
        players.update { if (playing) it + player else it - player }
    }

    /** This device's receiver id; `null` when it can't be read (then the stream names none). */
    private suspend fun ownId(): String? = try {
        receiverId()
    } catch (e: CancellationException) {
        throw e
    } catch (_: Exception) {
        null
    }

    /**
     * Whether Live TV is open to the profile, as its summary says: not
     * known yet, it is waited for [LIVE_WAIT_MS] at most, and taken as no.
     */
    private fun liveTvOpen(): Flow<Boolean> = flow { emitAll(liveTv()) }
        .catch { emit(false) }
        .distinctUntilChanged()
        .transformLatest { open ->
            if (open == null) delay(LIVE_WAIT_MS)
            emit(open == true)
        }
        .distinctUntilChanged()

    /** A fresh list of libraries: which are scanning now. */
    fun heard(libraries: List<Library>) {
        val scanning = libraries.filter { it.scanning }.mapTo(HashSet()) { it.id }
        _scans.update { scans ->
            val kept = scans.filter { it.libraryId == null || it.libraryId in scanning }
            kept + scanning.filter { id -> kept.none { it.libraryId == id } }.map { ScanState(it, null) }
        }
    }

    /**
     * Follows [issuer]'s events, naming this device as [receiver] (or as
     * none), until the server refuses them; returns then (the stream ends
     * for good until the next return).
     */
    private suspend fun follow(issuer: String, receiver: EventReceiver?) = coroutineScope {
        val inFront = AtomicBoolean(false)
        // Back in the foreground (or just signed in), also while a player kept the stream open: what is old is asked again.
        // Undispatched, so whether the app is in front is known before the first event.
        val returns = launch(start = CoroutineStart.UNDISPATCHED) {
            foreground.distinctUntilChanged().collect { front ->
                inFront.set(front)
                if (!front) return@collect
                cache.revalidate(STALE_MS) { _, _ -> true }
                // A stream kept open in the background connects no more: who the profile is is asked here instead.
                if (_streaming.value && session.meState.value?.fresh != true) launch { session.refresh() }
            }
        }
        val pending = ConcurrentHashMap<Long, Job>()
        val watchState = AtomicReference<WatchStateHeard?>(null)
        val refetch = Refetch(cache, inFront)
        try {
            eventsOf(issuer).events(receiver).collect { live ->
                when (live) {
                    LiveEvent.Reconnected -> {
                        _streaming.value = true
                        _receiver.value = receiver?.id
                        connected.tryEmit(Unit)
                        // In the background, the next return to the foreground asks for what is old.
                        if (inFront.get()) {
                            cache.revalidate(STALE_MS) { _, _ -> true }
                            if (session.meState.value?.fresh != true) launch { session.refresh() }
                        }
                    }
                    LiveEvent.Dropped -> {
                        _streaming.value = false
                        _receiver.value = null
                    }
                    is LiveEvent.Event -> handle(live.event, pending, watchState, refetch)
                }
            }
        } catch (e: CancellationException) {
            throw e
        } catch (_: IOException) {
            // Refused (signed out, not allowed): stopped until the next start.
        } finally {
            returns.cancel()
            _streaming.value = false
            _receiver.value = null
            // Changes heard but not yet acted on: asked again when next shown.
            for ((libraryId, job) in pending) {
                job.cancel()
                cache.markStale { key, value -> affects(libraryId, key, value) }
            }
            watchState.getAndSet(null)?.let { heard -> cache.markStale { key, value -> touches(heard.ids, heard.under, key, value) } }
            _scans.value = emptyList()
        }
    }

    private fun CoroutineScope.handle(
        event: ServerEvent,
        pending: ConcurrentHashMap<Long, Job>,
        watchState: AtomicReference<WatchStateHeard?>,
        refetch: Refetch,
    ) {
        heard.tryEmit(event)
        when (event) {
            is ServerEvent.ScanStarted -> scan(event.libraryId, null)
            is ServerEvent.ScanProgress ->
                scan(event.libraryId, if (event.total > 0) (event.processed.toFloat() / event.total).coerceIn(0f, 1f) else null)
            is ServerEvent.ScanFinished -> _scans.update { scans -> scans.filterNot { it.libraryId == event.libraryId } }
            is ServerEvent.LibraryChanged -> {
                val libraryId = event.libraryId
                pending.remove(libraryId)?.cancel()
                pending[libraryId] = launch {
                    delay(DEBOUNCE_MS)
                    pending.remove(libraryId)
                    refetch { key, value -> affects(libraryId, key, value) }
                }
            }
            ServerEvent.LibrariesChanged -> {
                // Which titles this profile may see may have changed with it: whatever shows them.
                refetch { _, _ -> true }
                launch { session.refresh() }
            }
            ServerEvent.WatchlistChanged -> refetch { key, value ->
                key == SnapshotKeys.HOME || key == SnapshotKeys.WATCHLIST || (value is ItemDetail && value.kind.listable)
            }
            is ServerEvent.UserStateChanged -> {
                val now = cache.clock()
                val first = watchState.getAndUpdate { it?.with(event, now) ?: WatchStateHeard.of(event, now) } == null
                if (first) {
                    launch {
                        delay(STATE_MS)
                        watchStateChanged(watchState.getAndSet(null) ?: return@launch, pending, refetch)
                    }
                }
            }
            ServerEvent.LiveChannelsChanged -> refetch { key, _ -> SnapshotKeys.isLive(key) }
            // Requests, the players and what plays casts follow these themselves (RequestsRepository, the players, the
            // cast receiver), from [events].
            is ServerEvent.RequestsChanged, is ServerEvent.RequestAvailable, is ServerEvent.RequestDecided,
            is ServerEvent.RequestCreated, is ServerEvent.RequestFailed, ServerEvent.RequestsSettingsChanged,
            ServerEvent.SessionsChanged, is ServerEvent.SessionStopped, is ServerEvent.PlayerCommand, is ServerEvent.Cast,
            ServerEvent.LiveTunersChanged, is ServerEvent.LiveTakenOver, is ServerEvent.Unknown -> Unit
        }
    }

    /**
     * What shows the watch state [heard] names is asked again, but not what
     * was fetched since it was heard: a change made here asked for it
     * already. Library changes waiting are asked for now with it, so a play
     * that ends (which sends both) costs one round of requests.
     */
    private fun watchStateChanged(heard: WatchStateHeard, pending: ConcurrentHashMap<Long, Job>, refetch: Refetch) {
        for ((libraryId, job) in pending) {
            if (!pending.remove(libraryId, job)) continue
            job.cancel()
            refetch { key, value -> affects(libraryId, key, value) }
        }
        refetch(olderThanMs = cache.clock() - heard.at) { key, value -> touches(heard.ids, heard.under, key, value) }
    }

    /** What an event touches is asked for again: now in the foreground, when next shown in the background. */
    private class Refetch(private val cache: ResourceCache, private val inFront: AtomicBoolean) {
        operator fun invoke(olderThanMs: Long = 0, match: (key: String, value: Any?) -> Boolean) {
            if (inFront.get()) cache.revalidate(olderThanMs, match) else cache.markStale(match)
        }
    }

    private fun scan(libraryId: Long, progress: Float?) {
        _scans.update { scans ->
            val others = scans.filterNot { it.libraryId == libraryId }
            others + ScanState(libraryId, progress)
        }
    }

    /**
     * `user_state_changed` events heard and not acted on yet: the items they
     * name, those everything under which changed, and when the last one
     * came (on the cache's clock).
     */
    private class WatchStateHeard(val ids: Set<Long>, val under: Set<Long>, val at: Long) {
        fun with(event: ServerEvent.UserStateChanged, now: Long): WatchStateHeard {
            val next = of(event, now)
            return WatchStateHeard(ids + next.ids, under + next.under, now)
        }

        companion object {
            fun of(event: ServerEvent.UserStateChanged, now: Long) = WatchStateHeard(
                ids = event.itemIds.toSet(),
                under = if (event.children) setOfNotNull(event.itemIds.firstOrNull()) else emptySet(),
                at = now,
            )
        }
    }

    companion object {
        /** How long a library's changes are gathered before asking again. */
        const val DEBOUNCE_MS = 1_500L

        /** How long watch state changes are gathered before asking again, from the first. */
        const val STATE_MS = 300L

        /** What is older than this is asked again on reconnecting or coming back. */
        const val STALE_MS = 30_000L

        /** Events kept for slow listeners of [events]; more are dropped. */
        private const val HEARD_BUFFER = 16

        /** How long a stream waits to know whether Live TV is open before it names what this device plays. */
        const val LIVE_WAIT_MS = 2_000L

        /** Whether the answer under [key], holding [value], shows something of [libraryId]. */
        fun affects(libraryId: Long, key: String, value: Any?): Boolean {
            if (SnapshotKeys.libraryOf(key) == libraryId) return true
            if (key == SnapshotKeys.HOME || key == SnapshotKeys.LIBRARIES) return true
            return when (value) {
                is ItemDetail -> value.libraryId == libraryId
                is ItemList -> value.items.any { it.libraryId == libraryId }
                is MaybeItem -> value.item?.libraryId == libraryId
                is CollectionDetail -> value.libraryId == libraryId
                is PlaylistDetail -> value.entries.any { it.item.libraryId == libraryId }
                is Hubs -> value.hubs.any { h -> h.libraryId == libraryId || h.items.any { it.libraryId == libraryId } }
                else -> false
            }
        }

        /**
         * Whether the answer under [key], holding [value], shows the watch
         * state of [ids] (items, with their seasons and shows) or of what is
         * under [under] (a show or season marked as a whole): Home and the
         * libraries' rows always (Continue Watching), the items' pages,
         * children and on-deck, and any page or list holding one of them.
         */
        fun touches(ids: Set<Long>, under: Set<Long>, key: String, value: Any?): Boolean {
            if (key == SnapshotKeys.HOME || key.startsWith("libhubs:")) return true
            val about = key.substringAfter(':', "").toLongOrNull()
            if (about in ids && (key.startsWith("item:") || key.startsWith("children:") || key.startsWith("ondeck:"))) return true
            if (value == null) return false
            fun changed(id: Long, parentId: Long?, grandparentId: Long?) = id in ids || parentId in under || grandparentId in under
            var found = false
            ItemPatch(
                summary = { s -> s.also { if (changed(s.id, s.parentId, s.grandparentId)) found = true } },
                detail = { d -> d.also { if (changed(d.id, d.parentId, d.grandparentId)) found = true } },
            ).apply(value)
            return found
        }
    }
}
