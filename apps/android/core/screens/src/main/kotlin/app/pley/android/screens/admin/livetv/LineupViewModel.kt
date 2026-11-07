package app.pley.android.screens.admin.livetv

import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.viewModelScope
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import app.pley.android.data.ServerAdmin
import app.pley.android.model.PleyJson
import app.pley.android.model.admin.LineupChannel
import app.pley.android.model.admin.LineupFilter
import app.pley.android.model.admin.LineupGroup
import app.pley.android.model.admin.LineupPage
import app.pley.android.model.admin.LineupPatch
import app.pley.android.model.admin.LineupState
import app.pley.android.screens.admin.AdminDone
import app.pley.android.screens.admin.AdminViewModel
import app.pley.android.screens.screensGraph
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.FlowPreview
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.debounce
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.drop
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.serialization.builtins.ListSerializer

/** The lineup [filter] keeps: [total] channels (`null` until the first page says), read a page at a time. */
data class LineupList(
    val filter: LineupFilter = LineupFilter(),
    val total: Int? = null,
    val pages: Map<Int, List<LineupChannel>> = emptyMap(),
    /** Why the first page couldn't be read. */
    val error: Throwable? = null,
) {
    /** The channel at [index], once its page is read. */
    fun at(index: Int): LineupChannel? = pages[index / LINEUP_PAGE]?.getOrNull(index % LINEUP_PAGE)
}

/** What is chosen: some channels by id, or every one the filter keeps. */
data class LineupChoice(val all: Boolean = false, val ids: Set<Long> = emptySet()) {
    fun count(total: Int): Int = if (all) total else ids.size

    operator fun contains(id: Long): Boolean = all || id in ids
}

/** Channels asked for at once, as the web asks. */
const val LINEUP_PAGE = 100

/** GET /api/live/lineup/page's query for page [page] of what [filter] keeps. */
fun lineupQuery(filter: LineupFilter, page: Int, limit: Int = LINEUP_PAGE): List<Pair<String, String?>> = listOf(
    "group" to filter.group,
    "state" to filter.state.wire,
    "q" to filter.q?.trim()?.ifEmpty { null },
    "offset" to (page * limit).toString(),
    "limit" to limit.toString(),
)

/**
 * Live TV's Channels, as the web's tab: the lineup, hidden channels too,
 * filtered on the server (group, state, a name or a number's
 * beginning) and read a page at a time as it scrolls ([visible]), so a
 * lineup of tens of thousands opens at once. Channels are hidden and
 * shown one at a time, chosen, or every one the filter keeps; a channel's
 * name, number and guide channel are set in its editor ([editing]).
 */
@OptIn(FlowPreview::class)
class LineupViewModel(admin: ServerAdmin, val origin: String) : AdminViewModel(admin) {
    /** The whole lineup's counts, whatever the filter: what the page's line says. */
    val summary = resource { page(LineupFilter(), 0, limit = 1) }

    /** The groups the filter offers. */
    val groups = resource { get(ListSerializer(LineupGroup.serializer()), listOf("api", "live", "lineup", "groups")) }

    private val listNow = MutableStateFlow(LineupList())
    val list: StateFlow<LineupList> = listNow.asStateFlow()

    private val typed = MutableStateFlow("")

    /** What is typed to find a channel; the filter takes it once typing stops. */
    val query: StateFlow<String> = typed.asStateFlow()

    private val choiceNow = MutableStateFlow(LineupChoice())
    val choice: StateFlow<LineupChoice> = choiceNow.asStateFlow()

    private val editingNow = MutableStateFlow<LineupChannel?>(null)

    /** The channel whose name, number and guide channel are being set. */
    val editing: StateFlow<LineupChannel?> = editingNow.asStateFlow()

    /** The rows on screen, as [visible] last said. */
    private var window = 0..0

    /** The pages being read for the filter listed, and their reads, which another filter cancels. */
    private var reading = mutableSetOf<Int>()
    private var loads: CoroutineScope? = null

    init {
        start(LineupFilter())
        viewModelScope.launch {
            typed.drop(1).debounce(SEARCH_WAIT).distinctUntilChanged().collect { q ->
                filter(listNow.value.filter.copy(q = q.trim().ifEmpty { null }))
            }
        }
    }

    fun find(query: String) {
        typed.value = query
    }

    fun setGroup(name: String?) = filter(listNow.value.filter.copy(group = name))

    fun setState(state: LineupState) = filter(listNow.value.filter.copy(state = state))

    /** Rows [first] to [last] are on screen: their pages, and the next one, are read if they aren't yet. */
    fun visible(first: Int, last: Int) {
        window = first..last
        val total = listNow.value.total ?: return
        val lastPage = ((total - 1) / LINEUP_PAGE).coerceAtLeast(0)
        val from = first / LINEUP_PAGE
        val to = minOf(lastPage, (last + LINEUP_PAGE / 2) / LINEUP_PAGE)
        for (p in from..to) if (p !in listNow.value.pages) read(p)
        // Pages far from what is shown go, so a long scroll through a large lineup holds a few.
        listNow.update { l -> l.copy(pages = l.pages.filterKeys { it in (from - KEPT)..(to + KEPT) }) }
    }

    /** Reads the first page again, after it couldn't be. */
    fun retry() = start(listNow.value.filter)

    fun choose(id: Long) = choiceNow.update { c ->
        val ids = if (c.all) emptySet() else c.ids
        LineupChoice(ids = if (id in ids) ids - id else ids + id)
    }

    /** Every channel the filter keeps, or none. */
    fun chooseAll() = choiceNow.update { if (it.all) LineupChoice() else LineupChoice(all = true) }

    /** Hides or shows [channel]. */
    fun setHidden(channel: LineupChannel, hidden: Boolean) = setHidden(hidden, listOf(channel.id))

    /** Hides or shows what is chosen. */
    fun setChosenHidden(hidden: Boolean) {
        val c = choiceNow.value
        setHidden(hidden, if (c.all) null else c.ids.toList())
    }

    /** [ids], or every channel the filter keeps (`null`). */
    private fun setHidden(hidden: Boolean, ids: List<Long>?) {
        val l = listNow.value
        val count = ids?.size ?: l.total ?: 0
        val patch = if (ids != null) LineupPatch(ids = ids, hidden = hidden) else LineupPatch(filter = l.filter, hidden = hidden)
        // Shown at once; what the server has is read after.
        val marked = ids?.toSet()
        listNow.update { now ->
            now.copy(pages = now.pages.mapValues { (_, cs) -> cs.map { if (marked == null || it.id in marked) it.copy(hidden = hidden) else it } })
        }
        val said = if (count > 1) AdminDone(if (hidden) HIDDEN else SHOWN, listOf(count)) else null
        change(said = said, after = ::refresh) {
            call("PATCH", listOf("api", "live", "lineup"), PleyJson.encodeToJsonElement(LineupPatch.serializer(), patch))
            choiceNow.value = LineupChoice()
        }
    }

    fun edit(channel: LineupChannel?) {
        editingNow.value = channel
    }

    /** Sets [channel]'s name, number and guide channel; `""` gives back the source's. */
    fun save(channel: LineupChannel, name: String, number: String, guideId: String) {
        val patch = LineupPatch(ids = listOf(channel.id), name = name.trim(), number = number.trim(), guideId = guideId.trim())
        change(said = AdminDone(SAVED), after = ::refresh) {
            call("PATCH", listOf("api", "live", "lineup"), PleyJson.encodeToJsonElement(LineupPatch.serializer(), patch))
            if (editingNow.value?.id == channel.id) editingNow.value = null
        }
    }

    private fun filter(filter: LineupFilter) {
        if (filter == listNow.value.filter) return
        start(filter)
    }

    /** Lists what [filter] keeps from its first page, from the top; another filter, another choice. */
    private fun start(filter: LineupFilter) {
        loads?.cancel()
        loads = CoroutineScope(viewModelScope.coroutineContext + SupervisorJob(viewModelScope.coroutineContext[Job]))
        reading = mutableSetOf()
        window = 0..0
        choiceNow.value = LineupChoice()
        listNow.value = LineupList(filter)
        read(0)
    }

    /** Reads the counts, and the pages held, again: after a change. */
    private fun refresh() {
        summary.reload()
        groups.reload()
        for (p in listNow.value.pages.keys + 0) read(p, again = true)
    }

    /** Reads [page] of the filter listed, unless it is being read already ([again]: read it anyway). */
    private fun read(page: Int, again: Boolean = false) {
        val reading = reading
        if (!reading.add(page) && !again) return
        val filter = listNow.value.filter
        val scope = loads ?: return
        scope.launch {
            try {
                val answer = page(filter, page)
                listNow.update { l ->
                    if (l.filter != filter) l else l.copy(total = answer.total, pages = l.pages + (page to answer.channels), error = null)
                }
                if (page == 0) visible(window.first, window.last)
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                if (page == 0 && listNow.value.total == null) listNow.update { it.copy(error = e) } else failed(e)
            } finally {
                reading.remove(page)
            }
        }
    }

    private suspend fun page(filter: LineupFilter, page: Int, limit: Int = LINEUP_PAGE): LineupPage =
        admin.api().get(LineupPage.serializer(), listOf("api", "live", "lineup", "page"), lineupQuery(filter, page, limit))

    companion object {
        const val HIDDEN = "livetv.lineup.hidden"
        const val SHOWN = "livetv.lineup.shown"
        const val SAVED = "livetv.lineup.saved"
        const val SEARCH_WAIT = 300L

        /** Pages kept either side of those on screen. */
        private const val KEPT = 2

        val factory: ViewModelProvider.Factory = viewModelFactory {
            initializer {
                val graph = screensGraph()
                LineupViewModel(graph.data.admin, graph.data.session.session.value?.issuer.orEmpty())
            }
        }
    }
}
