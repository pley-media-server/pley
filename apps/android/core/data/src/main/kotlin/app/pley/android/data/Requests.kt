package app.pley.android.data

import androidx.datastore.core.DataStore
import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.core.longPreferencesKey
import app.pley.android.model.Approval
import app.pley.android.model.Asked
import app.pley.android.model.Decline
import app.pley.android.model.DiscoverDetail
import app.pley.android.model.DiscoverList
import app.pley.android.model.DiscoverPage
import app.pley.android.model.ItemRequest
import app.pley.android.model.MediaRequest
import app.pley.android.model.NewRequest
import app.pley.android.model.RequestList
import app.pley.android.model.RequestOptions
import app.pley.android.model.RequestQuery
import app.pley.android.model.RequestScope
import app.pley.android.model.RequestStage
import app.pley.android.model.RequestStatus
import app.pley.android.model.RequestSummary
import app.pley.android.model.ServerEvent
import app.pley.android.model.TitleKind
import app.pley.android.network.ApiException
import app.pley.android.network.PleyApi
import java.io.IOException
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicInteger
import java.util.concurrent.atomic.AtomicLong
import java.util.concurrent.atomic.AtomicReference
import kotlin.coroutines.cancellation.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.Job
import kotlinx.coroutines.channels.BufferOverflow
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asSharedFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.drop
import kotlinx.coroutines.flow.emptyFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

/**
 * What requests are for the profile in use, from GET /api/requests/summary:
 * whether any of it shows, and what.
 */
sealed interface RequestsState {
    /** Not known yet: starting, or the server never answered. */
    data object Unknown : RequestsState

    /** None of it shows: requests are off, or the server is older. */
    data object Hidden : RequestsState

    /** None of it shows either: requests are on, but this account may not ask, and has no requests. */
    data object NoAccess : RequestsState

    /** Discover shows only "Sign in again to ask for movies and shows": this sign-in came before requests. */
    data class SignInAgain(val summary: RequestSummary) : RequestsState

    /** Requests work: [summary] says what shows (Discover, Your requests, the managers' Requests). */
    data class Open(val summary: RequestSummary) : RequestsState

    companion object {
        /** The state [summary] gives; `null`: not known. */
        fun of(summary: RequestSummary?): RequestsState = when {
            summary == null -> Unknown
            !summary.enabled -> Hidden
            summary.signInAgain == RequestScope.Requests && summary.discover -> SignInAgain(summary)
            summary.discover || summary.requestsEntry -> Open(summary)
            else -> NoAccess
        }
    }
}

/** The summary behind the state, where there is one. */
val RequestsState.summary: RequestSummary?
    get() = when (this) {
        is RequestsState.Open -> summary
        is RequestsState.SignInAgain -> summary
        RequestsState.Hidden, RequestsState.NoAccess, RequestsState.Unknown -> null
    }

/** Asking works from here now: Discover's rows, search's Discover row, title pages' Request. */
val RequestsState.mayAsk: Boolean get() = this is RequestsState.Open && summary.mayAsk

/**
 * A request changed, here or for the server ([requestId] with its title),
 * or many did (all `null`): screens read again what they show of it.
 */
data class RequestChange(val requestId: Long?, val kind: TitleKind?, val tmdbId: Long?) {
    /** Many changed: whatever shows requests is read again. */
    val all: Boolean get() = requestId == null && tmdbId == null

    /** Whether it may be about the title [kind] [tmdbId]. */
    fun about(kind: TitleKind, tmdbId: Long): Boolean = all || (this.tmdbId == tmdbId && (this.kind == null || this.kind == kind))

    companion object {
        val All = RequestChange(null, null, null)
    }
}

/**
 * Something to tell the person about their requests, once. The shells show
 * them when they can (never over playing video), oldest first; the last
 * [RequestsRepository.KEPT] are kept meanwhile. [key] tells two apart,
 * however alike.
 */
sealed interface RequestNotice {
    val key: Long

    /**
     * Ready to watch where [itemId] is: the whole of it ([complete]), or
     * [seasons] of a show. [posterUrl] is the title's poster on the server,
     * which a TV's pop-up shows.
     */
    data class Ready(
        override val key: Long,
        val requestId: Long,
        val title: String,
        val kind: TitleKind,
        val itemId: Long?,
        val seasons: List<Int>?,
        val complete: Boolean,
        val posterUrl: String? = null,
    ) : RequestNotice

    data class Approved(override val key: Long, val requestId: Long, val title: String) : RequestNotice

    /** [reason]: the administrator's, `null` without one (and for followers). */
    data class Declined(override val key: Long, val requestId: Long, val title: String, val reason: String?) : RequestNotice

    /** For managers: [by] asked for it, waiting for a decision. */
    data class Asked(override val key: Long, val requestId: Long, val title: String, val by: String?, val seasons: List<Int>?) : RequestNotice

    /** For managers: it couldn't be sent to Radarr ([kind] movie) or Sonarr, and waits for one. */
    data class Failed(override val key: Long, val requestId: Long, val title: String, val kind: TitleKind, val reason: String?) : RequestNotice

    /** Several of the person's requests have news. */
    data class News(override val key: Long, val count: Int) : RequestNotice
}

/**
 * Requests and Discover for the profile in use (docs/API.md "Requests").
 *
 * [state] says what shows; it follows the summary, which is read only
 * while something follows [state] or [start] ran: when first followed,
 * after a profile switch, when the event stream connects, after request
 * events (gathered for a second), and every minute in the foreground
 * while the stream is down. An older server (404, or a 403
 * `insufficient_scope` naming no scope) is taken as requests being off,
 * and asked again only when the stream connects or the app starts.
 */
interface RequestsRepository {
    val state: StateFlow<RequestsState>

    /**
     * Starts following the server's request events, polling while they
     * can't come, and catching up on news: once, after the first frame.
     */
    fun start()

    /** Reads the summary again now: a Retry, a sign-in done. */
    fun refresh()

    /** Requests that changed, as they are heard; nothing is replayed. */
    val changes: Flow<RequestChange>

    /** The first page of [list], from what was saved first. */
    fun row(list: DiscoverList): Flow<Loadable<DiscoverPage>>

    /** Page [page] of [list], from the server. */
    suspend fun page(list: DiscoverList, page: Int): DiscoverPage

    /** TMDB's titles matching [query] (as [searchText] makes it); [missing] leaves out those a library has. */
    suspend fun search(query: String, page: Int = 1, missing: Boolean = false): DiscoverPage

    /** A title page, held in memory while it is used. */
    fun title(kind: TitleKind, tmdbId: Long): Flow<Loadable<DiscoverDetail>>

    /** Reads [kind] [tmdbId]'s page again: now if it shows, else when it next does. */
    fun refreshTitle(kind: TitleKind, tmdbId: Long)

    /** [kind] [tmdbId]'s page from the server now; where the page shows, it shows this. */
    suspend fun readTitle(kind: TitleKind, tmdbId: Long): DiscoverDetail

    /** A library title as Discover sees it; `null` when it has no TMDB match. */
    suspend fun itemRequest(itemId: Long): ItemRequest?

    suspend fun list(query: RequestQuery): RequestList

    /** One request; `null` when it is gone (or never was this account's to see). */
    suspend fun request(id: Long): MediaRequest?

    /** Asks for [request] (or follows the requests that ask for it already). */
    suspend fun ask(request: NewRequest): Asked

    /** Withdraws this account's [request] (Remove, on a declined one); managers delete anyone's. */
    suspend fun delete(request: MediaRequest)

    /** Stops following someone else's [request]. */
    suspend fun unwatch(request: MediaRequest)

    suspend fun approve(id: Long, approval: Approval = Approval()): MediaRequest

    suspend fun decline(id: Long, reason: String? = null): MediaRequest

    /** Approves without sending: added by hand. */
    suspend fun handled(id: Long): MediaRequest

    /** Radarr's and Sonarr's choices for approving. */
    suspend fun options(): RequestOptions

    /** The person has looked at their requests: their news is seen, and the badge clears. */
    suspend fun seen()

    /** What to tell the person, oldest first; at most [KEPT]. */
    val notices: StateFlow<List<RequestNotice>>

    /** [notice] was shown (or is no longer wanted). */
    fun dismiss(notice: RequestNotice)

    companion object {
        /** Notices kept while they can't show. */
        const val KEPT = 3

        /** The longest search the server takes, in characters: it refuses longer ones. */
        const val SEARCH_MAX = 200

        /** [query] as the server takes it: trimmed, and cut to [SEARCH_MAX] characters, never halfway through one. */
        fun searchText(query: String): String {
            val trimmed = query.trim()
            if (trimmed.codePointCount(0, trimmed.length) <= SEARCH_MAX) return trimmed
            return trimmed.substring(0, trimmed.offsetByCodePoints(0, SEARCH_MAX)).trim()
        }

        /** Requests that never show: graphs made without them (tests of other screens, previews). */
        val None: RequestsRepository = object : RequestsRepository {
            override val state: StateFlow<RequestsState> = MutableStateFlow(RequestsState.Hidden).asStateFlow()
            override val changes: Flow<RequestChange> = emptyFlow()
            override val notices: StateFlow<List<RequestNotice>> = MutableStateFlow(emptyList<RequestNotice>()).asStateFlow()

            override fun start() = Unit
            override fun refresh() = Unit
            override fun row(list: DiscoverList): Flow<Loadable<DiscoverPage>> = flowOf(Loadable.Failed(off()))
            override suspend fun page(list: DiscoverList, page: Int): DiscoverPage = throw off()
            override suspend fun search(query: String, page: Int, missing: Boolean): DiscoverPage = throw off()
            override fun title(kind: TitleKind, tmdbId: Long): Flow<Loadable<DiscoverDetail>> = flowOf(Loadable.Failed(off()))
            override fun refreshTitle(kind: TitleKind, tmdbId: Long) = Unit
            override suspend fun readTitle(kind: TitleKind, tmdbId: Long): DiscoverDetail = throw off()
            override suspend fun itemRequest(itemId: Long): ItemRequest? = null
            override suspend fun list(query: RequestQuery): RequestList = throw off()
            override suspend fun request(id: Long): MediaRequest? = null
            override suspend fun ask(request: NewRequest): Asked = throw off()
            override suspend fun delete(request: MediaRequest) = throw off()
            override suspend fun unwatch(request: MediaRequest) = throw off()
            override suspend fun approve(id: Long, approval: Approval): MediaRequest = throw off()
            override suspend fun decline(id: Long, reason: String?): MediaRequest = throw off()
            override suspend fun handled(id: Long): MediaRequest = throw off()
            override suspend fun options(): RequestOptions = RequestOptions()
            override suspend fun seen() = Unit
            override fun dismiss(notice: RequestNotice) = Unit

            /** What a server with requests off answers. */
            private fun off() = ApiException(503, "unavailable", "Requests are off")
        }
    }
}

/**
 * [RequestsRepository] over [cache] (the summary and Discover's first
 * pages kept as snapshots, title pages in memory), the server's events
 * from [live], and [store], which keeps per server and profile the newest
 * `updated_at` announced, by catching up or by an event, so no request is
 * announced twice, across starts too.
 *
 * On a [tv], only what became ready to watch is told, as the Samsung and
 * LG apps do: the TV's pop-up takes focus from the page, worth it for a
 * title to open. The rest (approved, declined; for managers, new requests
 * and those that couldn't be sent) shows as the number and in the lists.
 */
internal class ServerRequests(
    private val cache: ResourceCache,
    private val live: LiveUpdates,
    private val foreground: Flow<Boolean>,
    /** The session's scopes: a sign-in again changes them, and the summary is read again. */
    private val scopes: Flow<Set<String>?>,
    private val store: DataStore<Preferences>,
    private val app: CoroutineScope,
    private val tv: Boolean = false,
) : RequestsRepository {
    /** Servers that answered as older ones do: asked again when the stream connects, or at the next start. */
    private val older: MutableSet<String> = ConcurrentHashMap.newKeySet()

    private val summaryResource = Resource(SnapshotKeys.REQUESTS, RequestSummary.serializer()) { api, _ -> readSummary(api) }

    // Read while something follows it: the shells, from just after their first frame.
    private val summaries: StateFlow<Loadable<RequestSummary>> =
        cache.observe(summaryResource).stateIn(app, SharingStarted.WhileSubscribed(), Loadable.Loading)

    override val state: StateFlow<RequestsState> = summaries
        .map { RequestsState.of(it.valueOrNull) }
        .distinctUntilChanged()
        .stateIn(app, SharingStarted.WhileSubscribed(), RequestsState.Unknown)

    // A screen that falls behind loses its oldest change rather than every screen the newest.
    private val _changes = MutableSharedFlow<RequestChange>(extraBufferCapacity = HEARD_BUFFER, onBufferOverflow = BufferOverflow.DROP_OLDEST)
    override val changes: Flow<RequestChange> get() = _changes.asSharedFlow()

    private val _notices = MutableStateFlow<List<RequestNotice>>(emptyList())
    override val notices: StateFlow<List<RequestNotice>> = _notices.asStateFlow()

    private val started = AtomicBoolean(false)
    private val keys = AtomicLong()
    private val soon = AtomicReference<Job?>(null)

    /** The profile whose notices these are, and the last fresh summary it had. */
    @Volatile private var owner: ProfileScope? = null
    @Volatile private var last: RequestSummary? = null

    /** News events heard since the last fresh summary: they explain its `unseen` rising. */
    private val explained = AtomicInteger()

    /** Requests announced in this process, for the profile in use. */
    private val announced: MutableSet<Long> = ConcurrentHashMap.newKeySet()

    /**
     * Events told news the saved mark doesn't cover yet: an event doesn't
     * say when its request changed, so the next fresh summary catches up,
     * which raises the mark past them.
     */
    private val unmarked = AtomicBoolean(false)

    private suspend fun readSummary(api: PleyApi): RequestSummary {
        val server = (cache.scopes.value as? ScopeState.Ready)?.profile?.server
        if (server != null && server in older) return OFF
        return try {
            api.requestSummary()
        } catch (e: ApiException) {
            if (!e.olderServer) throw e
            if (server != null) older += server
            OFF
        }
    }

    override fun start() {
        if (!started.compareAndSet(false, true)) return
        app.launch { cache.scopes.collect { switched((it as? ScopeState.Ready)?.profile) } }
        app.launch { live.events.collect { heard(it) } }
        app.launch { live.answered.collect { connected() } }
        app.launch { scopes.distinctUntilChanged().drop(1).collect { refreshNow() } }
        app.launch { summaries.collect { read(it) } }
        app.launch {
            combine(foreground, live.streaming) { inFront, streaming -> inFront && !streaming }
                .distinctUntilChanged()
                .collectLatest { polling ->
                    while (polling) {
                        delay(POLL_MS)
                        refreshNow()
                    }
                }
        }
    }

    override fun refresh() {
        app.launch { refreshNow() }
    }

    private suspend fun refreshNow() {
        try {
            cache.fresh(summaryResource)
        } catch (e: CancellationException) {
            throw e
        } catch (_: Exception) {
            // Shown where the summary is (it keeps what it had); asked again later.
        }
    }

    /** The summary read again in a second, however many changes come meanwhile. */
    private fun refreshSoon() {
        val job = app.launch(start = CoroutineStart.LAZY) {
            delay(DEBOUNCE_MS)
            refreshNow()
        }
        soon.getAndSet(job)?.cancel()
        job.start()
    }

    private fun switched(profile: ProfileScope?) {
        if (profile == owner) return
        owner = profile
        last = null
        explained.set(0)
        announced.clear()
        unmarked.set(false)
        _notices.value = emptyList()
    }

    /** The stream connected: nothing is replayed, and the server may have been updated. */
    private fun connected() {
        older.clear()
        refresh()
        _changes.tryEmit(RequestChange.All)
    }

    /** A fresh summary: what it says changed is caught up on. */
    private suspend fun read(load: Loadable<RequestSummary>) {
        val summary = (load as? Loadable.Ready)?.takeIf { it.fresh }?.value ?: return
        val profile = (cache.scopes.value as? ScopeState.Ready)?.profile ?: return
        if (profile != owner) switched(profile)
        val before = last
        last = summary
        val news = explained.getAndSet(0)
        // Without events, changed_at says when what shows is to be read again.
        if (before != null && before.changedAt != summary.changedAt && !live.streaming.value) _changes.tryEmit(RequestChange.All)
        val rose = if (before == null) summary.unseen > 0 else summary.unseen > before.unseen && news == 0
        if ((rose || unmarked.get()) && summary.enabled && summary.signInAgain != RequestScope.Requests) catchUp(profile)
    }

    /**
     * GET /api/requests?unseen=1: one notice for what wasn't announced yet
     * (on a TV, for what of it is ready to watch). Then every request
     * listed has been announced (now, by an event, or before), and the
     * saved mark is raised past them all.
     */
    private suspend fun catchUp(profile: ProfileScope) {
        // Events heard from here on set it again: their requests may come after this list.
        val pending = unmarked.getAndSet(false)
        val unseen = try {
            cache.api(profile).requests(RequestQuery(unseen = true, limit = CATCH_UP))
        } catch (e: CancellationException) {
            throw e
        } catch (_: Exception) {
            if (pending) unmarked.set(true)
            return
        }
        val key = longPreferencesKey("${profile.server}/${profile.userId}/announced")
        val mark = try {
            store.data.first()[key] ?: 0L
        } catch (_: IOException) {
            0L
        }
        if (owner != profile) return
        val fresh = unseen.requests.filter { it.updatedAt > mark && it.id !in announced }
        if (fresh.isNotEmpty()) {
            val told = if (tv) fresh.filter { it.stage == RequestStage.Available || it.stage == RequestStage.Partial } else fresh
            val notice = when (told.size) {
                0 -> null
                1 -> noticeOf(told.single())
                else -> RequestNotice.News(keys.incrementAndGet(), told.size)
            }
            notice?.let(::post)
            announced += fresh.map { it.id }
        }
        val newest = unseen.requests.maxOfOrNull { it.updatedAt } ?: return
        if (newest <= mark) return
        try {
            store.edit { it[key] = maxOf(it[key] ?: 0L, newest) }
        } catch (_: IOException) {
            // Raised at the next fresh summary instead.
            unmarked.set(true)
        }
    }

    private fun noticeOf(request: MediaRequest): RequestNotice? = when (request.stage) {
        RequestStage.Available -> RequestNotice.Ready(
            keys.incrementAndGet(), request.id, request.title, request.kind, request.itemId, null, complete = true, posterUrl = request.posterUrl,
        )
        RequestStage.Partial -> RequestNotice.Ready(
            keys.incrementAndGet(), request.id, request.title, request.kind, request.itemId, request.availableSeasons, complete = false,
            posterUrl = request.posterUrl,
        )
        RequestStage.Declined -> RequestNotice.Declined(keys.incrementAndGet(), request.id, request.title, request.reason)
        else -> null
    }

    private fun heard(event: ServerEvent) {
        when (event) {
            is ServerEvent.RequestsChanged -> changed(RequestChange(event.requestId, event.kind, event.tmdbId))
            is ServerEvent.RequestAvailable -> {
                told(event.requestId)
                post(
                    RequestNotice.Ready(
                        keys.incrementAndGet(), event.requestId, event.title, event.kind, event.itemId, event.seasons, event.complete,
                        posterUrl = event.posterUrl,
                    ),
                )
                changed(RequestChange(event.requestId, event.kind, event.tmdbId))
            }
            is ServerEvent.RequestDecided -> {
                when (event.status) {
                    RequestStatus.Declined -> {
                        told(event.requestId)
                        if (!tv) post(RequestNotice.Declined(keys.incrementAndGet(), event.requestId, event.title, event.reason))
                    }
                    RequestStatus.Approved -> if (!tv) post(RequestNotice.Approved(keys.incrementAndGet(), event.requestId, event.title))
                    else -> Unit
                }
                changed(RequestChange(event.requestId, event.kind, event.tmdbId))
            }
            is ServerEvent.RequestCreated -> {
                // Managers hear of every request; only those waiting for them are told.
                if (event.status == RequestStatus.Pending && !tv) {
                    post(RequestNotice.Asked(keys.incrementAndGet(), event.requestId, event.title, event.requestedBy?.name, event.seasons))
                }
                changed(RequestChange(event.requestId, event.kind, event.tmdbId))
            }
            is ServerEvent.RequestFailed -> {
                if (!tv) post(RequestNotice.Failed(keys.incrementAndGet(), event.requestId, event.title, event.kind, event.reason))
                changed(RequestChange(event.requestId, event.kind, null))
            }
            ServerEvent.RequestsSettingsChanged -> {
                refreshSoon()
                _changes.tryEmit(RequestChange.All)
            }
            else -> Unit
        }
    }

    /** An event told [requestId]'s news: it explains `unseen` rising, and the saved mark is to be raised past it. */
    private fun told(requestId: Long) {
        explained.incrementAndGet()
        announced += requestId
        unmarked.set(true)
    }

    /** The summary is read again soon, the title's page now if it shows, and the screens hear of it. */
    private fun changed(change: RequestChange) {
        refreshSoon()
        if (change.kind != null && change.tmdbId != null) refreshTitle(change.kind, change.tmdbId)
        _changes.tryEmit(change)
    }

    private fun post(notice: RequestNotice) {
        _notices.update { (it + notice).takeLast(RequestsRepository.KEPT) }
    }

    override fun dismiss(notice: RequestNotice) {
        _notices.update { notices -> notices.filterNot { it.key == notice.key } }
    }

    // ---------------------------------------------------------------- Discover

    private fun rowResource(list: DiscoverList) =
        Resource(SnapshotKeys.discover(list.wire), DiscoverPage.serializer()) { api, _ -> api.discover(list, 1) }

    private fun titleResource(kind: TitleKind, tmdbId: Long) =
        Resource<DiscoverDetail>(SnapshotKeys.discoverTitle(kind.wire, tmdbId), null, detail = true) { api, _ -> api.discoverTitle(kind, tmdbId) }

    override fun row(list: DiscoverList): Flow<Loadable<DiscoverPage>> = cache.observe(rowResource(list))

    override suspend fun page(list: DiscoverList, page: Int): DiscoverPage = api().discover(list, page)

    override suspend fun search(query: String, page: Int, missing: Boolean): DiscoverPage =
        api().discoverSearch(RequestsRepository.searchText(query), page, missing)

    override fun title(kind: TitleKind, tmdbId: Long): Flow<Loadable<DiscoverDetail>> = cache.observe(titleResource(kind, tmdbId))

    override fun refreshTitle(kind: TitleKind, tmdbId: Long) {
        val key = SnapshotKeys.discoverTitle(kind.wire, tmdbId)
        cache.revalidate { k, _ -> k == key }
    }

    override suspend fun readTitle(kind: TitleKind, tmdbId: Long): DiscoverDetail = cache.fresh(titleResource(kind, tmdbId))

    override suspend fun itemRequest(itemId: Long): ItemRequest? = try {
        api().itemRequest(itemId)
    } catch (e: ApiException) {
        if (e.status == 404) null else throw e
    }

    // ---------------------------------------------------------------- requests

    override suspend fun list(query: RequestQuery): RequestList = api().requests(query)

    override suspend fun request(id: Long): MediaRequest? = try {
        api().request(id)
    } catch (e: ApiException) {
        if (e.status == 404) null else throw e
    }

    override suspend fun ask(request: NewRequest): Asked {
        val asked = try {
            api().ask(request)
        } catch (e: ApiException) {
            // Someone else asked meanwhile, or it arrived: the page shows it as it is now.
            if (e.status == 409) refreshTitle(request.kind, request.tmdbId)
            throw e
        }
        acted(asked.request)
        return asked
    }

    override suspend fun delete(request: MediaRequest) {
        api().deleteRequest(request.id)
        acted(request)
    }

    override suspend fun unwatch(request: MediaRequest) {
        api().unwatchRequest(request.id)
        acted(request)
    }

    override suspend fun approve(id: Long, approval: Approval): MediaRequest = api().approveRequest(id, approval).also(::acted)

    override suspend fun decline(id: Long, reason: String?): MediaRequest =
        api().declineRequest(id, Decline(reason?.trim()?.takeIf { it.isNotEmpty() })).also(::acted)

    override suspend fun handled(id: Long): MediaRequest = api().requestHandled(id).also(::acted)

    override suspend fun options(): RequestOptions = api().requestOptions()

    override suspend fun seen() {
        // The badge clears at once; the server's summary follows.
        cache.patchKey(SnapshotKeys.REQUESTS) { value ->
            (value as? RequestSummary)?.takeIf { it.unseen > 0 }?.copy(unseen = 0)
        }
        try {
            api().requestsSeen()
        } finally {
            refreshSoon()
        }
    }

    /**
     * [request] changed here: the summary is read now (counts, what's
     * left), the title's page too, and the screens hear of it at once,
     * before the server's event comes.
     */
    private fun acted(request: MediaRequest) {
        refresh()
        refreshTitle(request.kind, request.tmdbId)
        _changes.tryEmit(RequestChange(request.id, request.kind, request.tmdbId))
    }

    private suspend fun api(): PleyApi = cache.api(cache.profile())

    companion object {
        /** Requests off: what an older server's answer is taken as. */
        private val OFF = RequestSummary(enabled = false)

        /** How long request events are gathered before reading the summary again. */
        const val DEBOUNCE_MS = 1_000L

        /** How often the summary is read while the event stream is down, in the foreground. */
        const val POLL_MS = 60_000L

        /** The most news read when catching up. */
        const val CATCH_UP = 20

        private const val HEARD_BUFFER = 16

        /** The DataStore's file, in filesDir. */
        const val FILE = "datastore/requests.preferences_pb"
    }
}

/** An answer older servers give to requests: 404, or (before scopes for them) a 403 naming no scope. */
private val ApiException.olderServer: Boolean get() = status == 404 || (needsScope && scope == null)
