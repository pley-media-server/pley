package app.pley.android.screens.admin.dashboard

import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.viewModelScope
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import app.pley.android.common.AppLanguage
import app.pley.android.data.ServerAdmin
import app.pley.android.model.HistoryPage
import app.pley.android.model.HistoryUser
import app.pley.android.model.admin.DashboardActivity
import app.pley.android.model.admin.DashboardResources
import app.pley.android.model.admin.DashboardSession
import app.pley.android.model.admin.DashboardSystem
import app.pley.android.model.admin.DashboardTranscoderStatus
import app.pley.android.model.admin.StorageCleanReport
import app.pley.android.model.admin.StorageUsage
import app.pley.android.network.AdminApi
import app.pley.android.screens.account.HistoryRow
import app.pley.android.screens.account.historyRow
import app.pley.android.screens.admin.AdminDone
import app.pley.android.screens.admin.AdminLoad
import app.pley.android.screens.admin.AdminViewModel
import app.pley.android.screens.admin.valueOrNull
import app.pley.android.screens.screensGraph
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.awaitCancellation
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.flowOn
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.receiveAsFlow
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

/** A play of everyone's history, as its row shows it, with who played it. */
data class DashboardPlay(val row: HistoryRow, val user: HistoryUser)

/** Everyone's plays loaded so far, as rows, and whether there are more. */
data class DashboardHistory(val plays: List<DashboardPlay>, val total: Int) {
    val canLoadMore: Boolean get() = plays.size < total
}

/**
 * The dashboard (the web's Settings → Dashboard): what plays and the
 * libraries' work, the machine's load and the transcoder's jobs, read
 * again on the web's timers while the page shows ([keepFresh]); the
 * server, its storage and everyone's plays, read when it opens. An
 * administrator stops someone's playback (with a message for their
 * screen), removes a play and cleans up storage. Backups stay in the web
 * app, whose routes for them apps can't reach.
 */
@OptIn(kotlinx.coroutines.ExperimentalCoroutinesApi::class)
class DashboardViewModel(
    admin: ServerAdmin,
    origin: Flow<String>,
    private val nowSeconds: () -> Long = { System.currentTimeMillis() / 1000 },
    compute: CoroutineDispatcher = Dispatchers.Default,
) : AdminViewModel(admin) {
    /** The server's address, which its pictures' paths are made absolute on. */
    val origin: StateFlow<String> = origin.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), "")

    /** How many plays the history shows: what reading it again asks for, Show more's pages included. */
    private var playsShown = HISTORY_PAGE

    /** The page has shown before: showing again, what it read once is read again. */
    private var shown = false

    val system = resource { get(DashboardSystem.serializer(), SYSTEM) }
    val storage = resource { get(StorageUsage.serializer(), STORAGE) }
    val history = resource { get(HistoryPage.serializer(), HISTORY, historyQuery(0, playsShown)) }

    private val activityNow = MutableStateFlow<AdminLoad<DashboardActivity>>(AdminLoad.Loading)
    private val transcoderNow = MutableStateFlow<AdminLoad<DashboardTranscoderStatus>>(AdminLoad.Loading)
    private val resourcesNow = MutableStateFlow<AdminLoad<DashboardResources>>(AdminLoad.Loading)

    /** Everyone's sessions, the libraries' scans and the work that waits: every 5 s while the page shows. */
    val activity: StateFlow<AdminLoad<DashboardActivity>> = activityNow.asStateFlow()

    /** The transcoder's live numbers and jobs: every 2.5 s while the page shows. */
    val transcoder: StateFlow<AdminLoad<DashboardTranscoderStatus>> = transcoderNow.asStateFlow()

    /** The processors and memory over the last five minutes: every 2 s, only the samples since the last. */
    val resources: StateFlow<AdminLoad<DashboardResources>> = resourcesNow.asStateFlow()

    private val loadingMoreNow = MutableStateFlow(false)
    private val removingNow = MutableStateFlow<Set<Long>>(emptySet())
    private val reports = Channel<AdminDone>(Channel.BUFFERED)

    /** Whether Show more is reading the next plays. */
    val loadingMore: StateFlow<Boolean> = loadingMoreNow.asStateFlow()

    /** The plays being removed, whose buttons wait. */
    val removing: StateFlow<Set<Long>> = removingNow.asStateFlow()

    /** What cleaning up did, which the server's answer says: for a toast ([CLEANED], [NOTHING_TO_CLEAN]). */
    val results: Flow<AdminDone> = reports.receiveAsFlow()

    /** Everyone's plays as rows; made again when the language shown changes, whose words they are in. */
    val plays: StateFlow<DashboardHistory?> =
        combine(history.state, AppLanguage.shown) { load, _ ->
            load.valueOrNull?.let { page ->
                val words = AppLanguage.formatWords()
                val now = nowSeconds()
                DashboardHistory(page.entries.map { DashboardPlay(historyRow(it, now, words), it.user) }, page.total)
            }
        }.flowOn(compute).stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), null)

    /**
     * Reads what changes while the page shows on the web's timers, each
     * at once and then after its wait, until the caller is cancelled: the
     * screen runs it while it is started, so nothing is read while it
     * isn't on screen. A read that fails keeps what was shown. Showing
     * again (back from a title it opened, or from another app), it reads
     * the server, its storage and the plays again too, as the web does.
     */
    suspend fun keepFresh(): Nothing = coroutineScope {
        if (shown) {
            system.reload()
            storage.reload()
            history.reload()
        }
        shown = true
        launch { every(ACTIVITY_MS) { readActivity() } }
        launch { every(TRANSCODER_MS) { readTranscoder() } }
        launch { every(RESOURCES_MS) { readResources() } }
        awaitCancellation()
    }

    private suspend fun every(ms: Long, read: suspend () -> Unit): Nothing {
        while (true) {
            read()
            delay(ms)
        }
    }

    internal suspend fun readActivity() = read(activityNow) { get(DashboardActivity.serializer(), ACTIVITY) }

    internal suspend fun readTranscoder() = read(transcoderNow) { get(DashboardTranscoderStatus.serializer(), TRANSCODER) }

    internal suspend fun readResources() = read(resourcesNow) {
        val held = resourcesNow.value.valueOrNull
        val last = held?.samples?.lastOrNull()?.at
        val fresh = get(DashboardResources.serializer(), RESOURCES, listOf("since" to last?.toString()))
        withSamplesOf(held, fresh)
    }

    private suspend fun <T> read(into: MutableStateFlow<AdminLoad<T>>, read: suspend AdminApi.() -> T) {
        try {
            into.value = AdminLoad.Ready(admin.api().read())
        } catch (e: CancellationException) {
            throw e
        } catch (e: Exception) {
            into.value = AdminLoad.Failed(e, into.value.valueOrNull)
        }
    }

    /**
     * Stops [session] at once, with [message] (trimmed; none when blank)
     * shown on its player's screen. It leaves the list straight away, and
     * the list is read again either way.
     */
    fun stop(session: DashboardSession, message: String) =
        change(said = AdminDone(STOPPED), after = { viewModelScope.launch { readActivity() } }) {
            val words = message.trim().takeIf { it.isNotEmpty() }
            call("DELETE", listOf("api", "sessions", session.sessionId), words?.let { buildJsonObject { put("message", it) } })
            activityNow.update { load ->
                val shown = load.valueOrNull ?: return@update load
                AdminLoad.Ready(shown.copy(sessions = shown.sessions.filterNot { it.sessionId == session.sessionId }))
            }
        }

    /** The next page of everyone's plays, after those shown. */
    fun loadMore() {
        val page = history.state.value.valueOrNull ?: return
        if (loadingMoreNow.value || page.entries.size >= page.total) return
        loadingMoreNow.value = true
        viewModelScope.launch {
            try {
                val next = admin.api().get(HistoryPage.serializer(), HISTORY, historyQuery(page.entries.size))
                val held = history.state.value.valueOrNull ?: page
                val more = HistoryPage(next.total, (held.entries + next.entries).distinctBy { it.id })
                playsShown = maxOf(HISTORY_PAGE, more.entries.size)
                history.set(more)
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                failed(e)
            } finally {
                loadingMoreNow.value = false
            }
        }
    }

    /** Removes a play from the history; it leaves the list once the server has. */
    fun removePlay(entryId: Long) {
        if (entryId in removingNow.value) return
        removingNow.update { it + entryId }
        change(after = { removingNow.update { it - entryId } }) {
            call("DELETE", listOf("api", "history", entryId))
            history.state.value.valueOrNull?.let { page ->
                val left = page.entries.filterNot { it.id == entryId }
                history.set(HistoryPage(page.total - (page.entries.size - left.size), left))
            }
        }
    }

    /** Removes what nothing uses any more, and old cache files; [results] says what it freed. */
    fun cleanUp() = change(after = storage::reload) {
        val report = send("POST", StorageCleanReport.serializer(), CLEAN)
        reports.send(
            if (report.filesRemoved > 0 || report.allBytesFreed > 0) {
                AdminDone(CLEANED, listOf(report.filesRemoved, report.allBytesFreed))
            } else {
                AdminDone(NOTHING_TO_CLEAN)
            },
        )
    }

    companion object {
        /** A session stopped. */
        const val STOPPED = "stopped"

        /** Storage cleaned up: how many files went (an Int) and the bytes freed (a Long). */
        const val CLEANED = "cleaned"

        /** Storage cleaned up, with nothing to remove. */
        const val NOTHING_TO_CLEAN = "nothing_to_clean"

        /** The web's timers. */
        const val ACTIVITY_MS = 5_000L
        const val TRANSCODER_MS = 2_500L
        const val RESOURCES_MS = 2_000L

        /** Plays a page of the history shows, as the web's dashboard. */
        const val HISTORY_PAGE = 10

        /** Samples kept, as the server keeps them: five minutes of them. */
        const val RESOURCE_SAMPLES = 150

        private val ACTIVITY = listOf("api", "activity")
        private val SYSTEM = listOf("api", "system")
        private val RESOURCES = listOf("api", "system", "resources")
        private val STORAGE = listOf("api", "system", "storage")
        private val CLEAN = listOf("api", "system", "storage", "clean")
        private val TRANSCODER = listOf("api", "transcoder")
        private val HISTORY = listOf("api", "history")

        private fun historyQuery(offset: Int, limit: Int = HISTORY_PAGE) = listOf("all" to "1", "offset" to offset.toString(), "limit" to limit.toString())

        val factory: ViewModelProvider.Factory = viewModelFactory {
            initializer {
                val g = screensGraph()
                DashboardViewModel(g.data.admin, g.data.servers.active.map { it.orEmpty() }.distinctUntilChanged())
            }
        }
    }
}

/**
 * [fresh] samples after those [held] (the answer to `since` its last):
 * the last [DashboardViewModel.RESOURCE_SAMPLES] of both, with what
 * [fresh] says of the machine. Without samples held, [fresh] as it is.
 */
fun withSamplesOf(held: DashboardResources?, fresh: DashboardResources): DashboardResources {
    if (held == null || held.samples.isEmpty()) return fresh
    return fresh.copy(samples = (held.samples + fresh.samples).takeLast(DashboardViewModel.RESOURCE_SAMPLES))
}
