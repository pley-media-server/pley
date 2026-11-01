package app.pley.android.testing.data

import app.pley.android.data.Loadable
import app.pley.android.data.RequestChange
import app.pley.android.data.RequestNotice
import app.pley.android.data.RequestsRepository
import app.pley.android.data.RequestsState
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
import app.pley.android.model.RequestSummary
import app.pley.android.model.TitleKind
import app.pley.android.network.ApiException
import app.pley.android.testing.Fixtures
import app.pley.android.testing.network.FakePleyApi
import java.util.concurrent.ConcurrentHashMap
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asSharedFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.update

/**
 * Requests as a test drives them: [summary] says what shows, answers come
 * from [api] (the fixtures', or what a test sets there) unless [pages] or
 * [searches] say otherwise, [change] is a request event heard, [notify] a
 * notice. What was asked shows in [api]'s calls.
 */
class FakeRequests(
    summary: RequestSummary? = Fixtures.decode("requests/summary.json"),
    val api: FakePleyApi = FakePleyApi(TestData.ISSUER),
) : RequestsRepository {
    private val _state = MutableStateFlow(RequestsState.of(summary))
    override val state: StateFlow<RequestsState> = _state.asStateFlow()

    /** The summary the server answers; setting it is a new answer. */
    var summary: RequestSummary? = summary
        set(value) {
            field = value
            _state.value = RequestsState.of(value)
        }

    /** Page [page] of a list: the fixtures' trending titles unless a test says otherwise. */
    var pages: suspend (DiscoverList, Int) -> DiscoverPage = { list, page -> api.discover(list, page) }

    /** Search: the fixtures' titles unless a test says otherwise. */
    var searches: suspend (String, Int, Boolean) -> DiscoverPage = { query, page, missing -> api.discoverSearch(query, page, missing) }

    var started = 0
        private set
    var refreshed = 0
        private set

    private val _changes = MutableSharedFlow<RequestChange>(extraBufferCapacity = 64)
    override val changes: Flow<RequestChange> = _changes.asSharedFlow()

    /** A request changed, as an event says. */
    fun change(change: RequestChange) {
        _changes.tryEmit(change)
    }

    private val _notices = MutableStateFlow<List<RequestNotice>>(emptyList())
    override val notices: StateFlow<List<RequestNotice>> = _notices.asStateFlow()

    fun notify(notice: RequestNotice) {
        _notices.update { (it + notice).takeLast(RequestsRepository.KEPT) }
    }

    override fun dismiss(notice: RequestNotice) {
        _notices.update { notices -> notices.filterNot { it.key == notice.key } }
    }

    override fun start() {
        started++
    }

    override fun refresh() {
        refreshed++
    }

    override fun row(list: DiscoverList): Flow<Loadable<DiscoverPage>> = flow {
        emit(
            try {
                Loadable.Ready(pages(list, 1), fresh = true)
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                Loadable.Failed(e)
            },
        )
    }

    override suspend fun page(list: DiscoverList, page: Int): DiscoverPage = pages(list, page)

    override suspend fun search(query: String, page: Int, missing: Boolean): DiscoverPage = searches(query, page, missing)

    /** Each title's reads: a refresh reads it again. */
    private val revisions = ConcurrentHashMap<String, MutableStateFlow<Int>>()

    private fun revision(kind: TitleKind, tmdbId: Long) = revisions.getOrPut("${kind.wire}:$tmdbId") { MutableStateFlow(0) }

    override fun title(kind: TitleKind, tmdbId: Long): Flow<Loadable<DiscoverDetail>> = revision(kind, tmdbId).map {
        try {
            Loadable.Ready(api.discoverTitle(kind, tmdbId), fresh = true)
        } catch (e: CancellationException) {
            throw e
        } catch (e: Exception) {
            Loadable.Failed(e)
        }
    }

    override fun refreshTitle(kind: TitleKind, tmdbId: Long) {
        revision(kind, tmdbId).update { it + 1 }
    }

    override suspend fun readTitle(kind: TitleKind, tmdbId: Long): DiscoverDetail = api.discoverTitle(kind, tmdbId)

    override suspend fun itemRequest(itemId: Long): ItemRequest? = orNull { api.itemRequest(itemId) }

    override suspend fun list(query: RequestQuery): RequestList = api.requests(query)

    override suspend fun request(id: Long): MediaRequest? = orNull { api.request(id) }

    override suspend fun ask(request: NewRequest): Asked = api.ask(request)

    override suspend fun delete(request: MediaRequest) = api.deleteRequest(request.id)

    override suspend fun unwatch(request: MediaRequest) = api.unwatchRequest(request.id)

    override suspend fun approve(id: Long, approval: Approval): MediaRequest = api.approveRequest(id, approval)

    override suspend fun decline(id: Long, reason: String?): MediaRequest = api.declineRequest(id, Decline(reason))

    override suspend fun handled(id: Long): MediaRequest = api.requestHandled(id)

    override suspend fun options(): RequestOptions = api.requestOptions()

    override suspend fun seen() {
        api.requestsSeen()
        summary = summary?.copy(unseen = 0)
    }

    /** [block]'s answer, `null` for a 404, as the app's repository does. */
    private inline fun <T> orNull(block: () -> T): T? = try {
        block()
    } catch (e: ApiException) {
        if (e.status == 404) null else throw e
    }
}
