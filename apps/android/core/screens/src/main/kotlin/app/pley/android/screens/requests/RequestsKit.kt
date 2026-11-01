package app.pley.android.screens.requests

import app.pley.android.data.RequestChange
import app.pley.android.data.RequestsRepository
import app.pley.android.model.Approval
import app.pley.android.model.ArrOptions
import app.pley.android.model.Asked
import app.pley.android.model.DiscoverPage
import app.pley.android.model.DiscoverTitle
import app.pley.android.model.ItemRequest
import app.pley.android.model.MediaRequest
import app.pley.android.model.TitleKind
import app.pley.android.model.TitleStatus
import app.pley.android.screens.PleyKey
import app.pley.android.screens.ScreensGraph
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.buffer
import kotlinx.coroutines.flow.channelFlow
import kotlinx.coroutines.launch

/** What the request screens read: the repository, the server's address for pictures, and whether events come. */
class RequestsDeps(
    val requests: RequestsRepository,
    val origin: Flow<String?>,
    /** The event stream is connected: without it, open lists are read again every 30 s. */
    val streaming: StateFlow<Boolean>,
    val compute: CoroutineDispatcher = Dispatchers.Default,
) {
    companion object {
        fun of(graph: ScreensGraph): RequestsDeps =
            RequestsDeps(graph.data.requests, graph.data.servers.active, graph.data.liveUpdates.streaming)
    }
}

/** Something a request screen tells the person once (a toast), or a place to go. */
sealed interface RequestEvent {
    /** Asking for [title] did what [asked] says: "Asked for “X”: an administrator decides." */
    data class Asked(val asked: app.pley.android.model.Asked, val title: String) : RequestEvent

    /** It didn't work: [problem] says why. */
    data class Failed(val problem: RequestProblem) : RequestEvent

    /** Go to [key]: a show's picker, a library title. */
    data class Open(val key: PleyKey) : RequestEvent
}

/** What a request action left. */
sealed interface Outcome {
    /** The request as it is now. */
    data class Changed(val request: MediaRequest) : Outcome

    /** It's no longer this account's: withdrawn, removed, deleted, or no longer followed. */
    data object Gone : Outcome

    /** Asked for again: a new request (or one now followed). */
    data class AskedAgain(val asked: Asked) : Outcome
}

/** Does [action] to [request] ([approval] for Approve…, [reason] for Decline). */
suspend fun RequestsRepository.perform(
    request: MediaRequest,
    action: CardAction,
    approval: Approval? = null,
    reason: String? = null,
): Outcome = when (action) {
    CardAction.Withdraw, CardAction.Remove, CardAction.Delete -> {
        delete(request)
        Outcome.Gone
    }
    CardAction.StopFollowing -> {
        unwatch(request)
        Outcome.Gone
    }
    CardAction.Approve, CardAction.TryAgain -> Outcome.Changed(approve(request.id))
    CardAction.ApproveWith -> Outcome.Changed(approve(request.id, approval ?: Approval()))
    CardAction.Decline, CardAction.DeclineWithReason -> Outcome.Changed(decline(request.id, reason))
    CardAction.Handled -> Outcome.Changed(handled(request.id))
    CardAction.AskAgain -> Outcome.AskedAgain(ask(askAgain(request)))
}

/** [block]'s value, or `null` once its failure was handed to [failed]. */
internal suspend fun <T> attempt(failed: suspend (RequestProblem) -> Unit, block: suspend () -> T): T? = try {
    block()
} catch (e: CancellationException) {
    throw e
} catch (e: Exception) {
    failed(RequestProblem.of(e))
    null
}

// ---------------------------------------------------------------- library titles

/**
 * What requests say of a library's movie or show (GET /api/items/{id}/request):
 * the stage pill of its newest active request, and Request more seasons
 * ([more]) when some can be asked for and asking works.
 */
data class ItemRequestState(
    val kind: TitleKind,
    val tmdbId: Long,
    val status: TitleStatus,
    val request: MediaRequest?,
    val more: Boolean,
    val manage: Boolean,
) {
    /** Where Request more seasons goes: the Discover page, its picker open. */
    val picker: PleyKey get() = PleyKey.DiscoverTitle(kind, tmdbId, pick = true)

    companion object {
        fun of(answer: ItemRequest, mayAsk: Boolean, manage: Boolean): ItemRequestState = ItemRequestState(
            kind = answer.kind,
            tmdbId = answer.tmdbId,
            status = answer.status,
            request = answer.request,
            more = mayAsk && !answer.askable.isNullOrEmpty(),
            manage = manage,
        )
    }
}

// ---------------------------------------------------------------- titles shown

/** Whether [change] may be about [title]: its request, or the title itself. */
fun RequestChange.touches(title: DiscoverTitle): Boolean =
    (requestId != null && title.requestId == requestId) || (tmdbId == title.tmdbId && (kind == null || kind == title.kind))

/**
 * How titles already shown change with [change] (requests_changed): their
 * badges follow its request, read once (GET /api/requests/{id}). One this
 * account can no longer read says nothing of its title: the title's page
 * is read instead, for its status (the request let go of, should that
 * fail too). `null` when none of [shown] is touched, many changed at once
 * (rows are never read again for that), or the request couldn't be read.
 */
suspend fun RequestsRepository.patchFor(change: RequestChange, shown: Sequence<DiscoverTitle>): ((DiscoverTitle) -> DiscoverTitle)? {
    val id = change.requestId ?: return null
    val touched = shown.filter { change.touches(it) }.map { it.kind to it.tmdbId }.toSet()
    if (touched.isEmpty()) return null
    val request = try {
        request(id)
    } catch (e: CancellationException) {
        throw e
    } catch (_: Exception) {
        return null
    }
    if (request != null) return { title -> if (change.touches(title)) patched(title, request) else title }
    val pages = touched.associateWith { (kind, tmdbId) ->
        try {
            readTitle(kind, tmdbId)
        } catch (e: CancellationException) {
            throw e
        } catch (_: Exception) {
            null
        }
    }
    return { title ->
        val page = pages[title.kind to title.tmdbId]
        when {
            !change.touches(title) -> title
            page != null -> title.copy(status = page.status, itemId = page.itemId, requestId = page.requestId)
            else -> patched(title, null)
        }
    }
}

/**
 * Changes for a screen that reads the server for them: those heard while
 * it reads come next, together and each once, so it neither holds up the
 * other screens nor loses any.
 */
fun Flow<RequestChange>.gathered(): Flow<List<RequestChange>> = channelFlow {
    val heard = LinkedHashSet<RequestChange>()
    val ready = Channel<Unit>(Channel.CONFLATED)
    launch {
        collect { change ->
            synchronized(heard) { heard += change }
            ready.trySend(Unit)
        }
        ready.close()
    }
    for (signal in ready) {
        val batch = synchronized(heard) { heard.toList().also { heard.clear() } }
        if (batch.isNotEmpty()) send(batch)
    }
}.buffer(Channel.RENDEZVOUS)

/** The key telling titles apart in lists: a movie and a show may share a TMDB id. */
val DiscoverTitle.key: String get() = "${kind.wire}:$tmdbId"

/**
 * Titles of a list or of search as they're read page by page: each title
 * once, more to read while [page] < [totalPages] (a short page, even an
 * empty one, isn't the end).
 */
data class Pages(
    val titles: List<DiscoverTitle> = emptyList(),
    /** The last page read; 0 before the first. */
    val page: Int = 0,
    val totalPages: Int = 1,
    val loading: Boolean = false,
    val problem: RequestProblem? = null,
) {
    val more: Boolean get() = page < totalPages

    /** [answer] added: the first page replaces what was there; later ones add what's new. */
    fun with(answer: DiscoverPage): Pages {
        val titles = if (answer.page <= 1) answer.results.distinctBy { it.key } else (titles + answer.results).distinctBy { it.key }
        return Pages(titles, answer.page.coerceAtLeast(1), answer.totalPages, loading = false, problem = null)
    }

    /** Another page is wanted without scrolling: fewer than a [screenful], and more to read. */
    fun short(screenful: Int): Boolean = more && !loading && problem == null && titles.size < screenful

    fun map(transform: (DiscoverTitle) -> DiscoverTitle): Pages = copy(titles = titles.map(transform))
}

// ---------------------------------------------------------------- approving

/**
 * The Approve… dialog: [request], with the service's [options] once read
 * (`null` while loading, or when it isn't connected: then only "Approve
 * with the settings' choices").
 */
data class ApproveDialog(
    val request: MediaRequest,
    val options: ArrOptions?,
    val loading: Boolean,
) {
    /** The profile the dialog starts on: anime's for anime, else the settings'. */
    val profile: Long? get() = options?.let { if (request.anime) it.animeQualityProfile ?: it.qualityProfile else it.qualityProfile }

    /** The folder the dialog starts on. */
    val folder: String? get() = options?.let { if (request.anime) it.animeRootFolder ?: it.rootFolder else it.rootFolder }

    /** Something to choose: the service answered with lists. */
    val choices: Boolean get() = options != null && (options.qualityProfiles.isNotEmpty() || options.rootFolders.isNotEmpty())

    /** Shows: the seasons asked for, all ticked to start with. */
    val seasons: List<Int> get() = if (request.kind == TitleKind.Show) request.seasons.orEmpty().sorted() else emptyList()

    /**
     * What approving sends: only what differs from the settings' choices,
     * and the seasons when some were left out.
     */
    fun approval(profile: Long?, folder: String?, seasons: Set<Int>?): Approval = Approval(
        qualityProfile = profile?.takeIf { it != this.profile },
        rootFolder = folder?.takeIf { it != this.folder },
        seasons = seasons?.sorted()?.takeIf { request.kind == TitleKind.Show && it.isNotEmpty() && it.size < this.seasons.size },
    )
}
