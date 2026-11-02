package app.pley.android.screens.livetv

import app.pley.android.data.LiveTvRepository
import app.pley.android.data.Loadable
import app.pley.android.data.valueOrNull
import app.pley.android.model.LiveCategory
import app.pley.android.model.LiveChannel
import app.pley.android.model.LiveChannelPage
import app.pley.android.model.LiveGroup
import app.pley.android.model.LiveGuide
import app.pley.android.model.LiveProgram
import java.time.Instant
import java.time.ZoneId
import java.time.temporal.ChronoUnit
import java.util.concurrent.atomic.AtomicReference
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn

/** How a Live TV page shows the channels: the guide (channels by time) or cards. */
enum class LiveView { Guide, Channels }

/** A region ("UK") gathering its groups ("UK| Sport", "UK| News"…), with their channels' [count]. */
data class GroupRegion(val region: String, val groups: List<LiveGroup>, val count: Int)

/**
 * The groups as the categories list them (the web's CategoryRail
 * `groupSections`): [regions] in alphabetical order, each opening to its
 * groups, then [others] (no region, or a region of one group) in the
 * provider's order. With fewer than two regions, every group is in
 * [others], in the provider's order.
 */
data class GroupSections(val regions: List<GroupRegion>, val others: List<LiveGroup>)

/**
 * A category's channels as far as they are loaded: [total] in the list
 * (`null` until its first page is in), and the pages in, by number.
 */
data class ChannelPages(
    val total: Int? = null,
    val pages: Map<Int, List<LiveChannel>> = emptyMap(),
    /** The first page failed, with nothing to show. */
    val error: Throwable? = null,
) {
    /** The channel at [index], once its page is in. */
    operator fun get(index: Int): LiveChannel? = pages[index / LiveTvRepository.PAGE]?.getOrNull(index % LiveTvRepository.PAGE)

    /** Every channel loaded, in order. */
    val loaded: List<LiveChannel> get() = pages.toSortedMap().values.flatten()
}

/** Live TV's rules, shared by its pages and its player, the web's (apps/web/src/pages/livetv). */
object LiveTv {
    /** The guide's columns: half an hour each. */
    const val SLOT_S = 30 * 60L

    /** How far ahead the guide goes from where it starts. */
    const val GUIDE_SPAN_S = 48 * 3600L

    /** The regions of two or more groups, alphabetically, then the rest in the provider's order; one list with fewer than two regions. */
    fun groupSections(groups: List<LiveGroup>): GroupSections {
        val regions = groups.filter { it.region != null }.groupBy { it.region!! }
            .filterValues { it.size > 1 }
            .map { (region, gs) -> GroupRegion(region, gs, gs.sumOf { it.count }) }
            .sortedWith(compareBy(String.CASE_INSENSITIVE_ORDER) { it.region })
        if (regions.size < 2) return GroupSections(emptyList(), groups)
        val gathered = regions.mapTo(HashSet()) { it.region }
        return GroupSections(regions, groups.filter { it.region == null || it.region !in gathered })
    }

    /** The groups a search names, as quick links beside its channels (the web's, a dozen at most). */
    fun foundGroups(groups: List<LiveGroup>, query: String): List<LiveGroup> {
        val q = query.trim().lowercase()
        if (q.isEmpty()) return emptyList()
        return groups.filter { it.name.lowercase().contains(q) }.take(FOUND_GROUPS)
    }

    /** At most this many groups show for a search. */
    const val FOUND_GROUPS = 12

    /**
     * A name's region and the rest, when it begins with one the way
     * providers write them ("UK| Sport", "[UK] Sport", "UK: Sport"), as the
     * server splits group names.
     */
    fun splitRegion(name: String): Pair<String, String>? {
        val n = name.trim().replace(LEADING_BARS, "")
        val m = BRACKETED.matchEntire(n) ?: BARRED.matchEntire(n) ?: COLON.matchEntire(n) ?: return null
        val region = m.groupValues[1].trim()
        val rest = m.groupValues[2].replace(LEADING_MARKS, "").trim()
        return if (region.isNotEmpty() && rest.isNotEmpty()) region to rest else null
    }

    /**
     * A channel's name as a list shows it: in a group, without the region
     * the group already says ("Sky Sports 1" in UK · Sport, not "UK| Sky
     * Sports 1").
     */
    fun channelName(channel: LiveChannel, category: LiveCategory?): String {
        if (category !is LiveCategory.Group) return channel.name
        val name = splitRegion(channel.name) ?: return channel.name
        val group = splitRegion(channel.group) ?: return channel.name
        return if (name.first.equals(group.first, ignoreCase = true)) name.second else channel.name
    }

    /** "UK · Sport" for a group of a region; its name otherwise. */
    fun groupName(group: LiveGroup?, name: String): String =
        if (group?.region != null) "${group.region} · ${group.label}" else group?.label ?: name

    /** Where the guide starts for [now] (unix seconds): the start of the half hour before the one it is in. */
    fun guideStart(now: Long): Long = Math.floorDiv(now, SLOT_S) * SLOT_S - SLOT_S

    /**
     * The guide's chunks that cover [from] to [to], and one either side, so
     * what comes into view next is there already; none before [start] or
     * past its span.
     */
    fun chunks(start: Long, from: Long, to: Long): List<Long> {
        val size = LiveTvRepository.GUIDE_CHUNK_S
        val first = Math.floorDiv(maxOf(from, start), size) * size - size
        val last = Math.floorDiv(minOf(to, start + GUIDE_SPAN_S), size) * size + size
        val floor = Math.floorDiv(start, size) * size
        return (first..last step size).filter { it >= floor && it < start + GUIDE_SPAN_S }
    }

    /** The programmes of [guides] by channel, each list by start, one programme per start. */
    fun merge(guides: List<LiveGuide>): Map<Long, List<LiveProgram>> {
        val out = HashMap<Long, MutableMap<Long, LiveProgram>>()
        for (guide in guides) {
            for ((channel, programs) in guide.programs) {
                val known = out.getOrPut(channel) { HashMap() }
                for (p in programs) known.putIfAbsent(p.start, p)
            }
        }
        return out.mapValues { (_, byStart) -> byStart.values.sortedBy { it.start } }
    }

    /** The programmes that show between [from] and [to] (unix seconds). */
    fun between(programs: List<LiveProgram>?, from: Long, to: Long): List<LiveProgram> =
        programs.orEmpty().filter { it.stop > from && it.start < to }

    /** A programme's rest at [now], as hours and minutes ("1 h 5 min left"). */
    fun left(program: LiveProgram, now: Long): Pair<Int, Int> {
        val minutes = maxOf(1, Math.round((program.stop - now) / 60.0).toInt())
        return minutes / 60 to minutes % 60
    }

    /**
     * How many days [seconds]'s date is from [now]'s (unix seconds) in
     * [zone]: 1 tomorrow, -1 yesterday. Dates, not 24-hour steps: a day is
     * 23 or 25 hours when daylight saving starts or ends.
     */
    fun daysApart(seconds: Long, now: Long, zone: ZoneId = ZoneId.systemDefault()): Long =
        ChronoUnit.DAYS.between(Instant.ofEpochSecond(now).atZone(zone).toLocalDate(), Instant.ofEpochSecond(seconds).atZone(zone).toLocalDate())

    private val LEADING_BARS = Regex("^\\|+\\s*")
    private val BRACKETED = Regex("^\\[([^\\]]{1,16})]\\s*(.+)$")
    private val BARRED = Regex("^([^|]{1,16})\\|\\s*(.+)$")
    private val COLON = Regex("^([A-Z]{1,4}):\\s*(.+)$")
    private val LEADING_MARKS = Regex("^[|:\\-\\s]+")
}

/**
 * The channel last watched: a list opens back at it, coming back from the
 * player. Kept for the profile too ([app.pley.android.data.LiveTvViews]),
 * which sets it again after a restart.
 */
object LastWatched {
    val channel = MutableStateFlow<Long?>(null)
}

/**
 * The channel a page opens the live player on, as the page has it: the
 * player shows it at once (its tuning card's logo, number, name and
 * what's on) rather than black until the server says what the channel
 * is. Handed over in memory, as [LastWatched] is; the player of that
 * channel takes it once.
 */
object ChannelHandoff {
    private val given = AtomicReference<LiveChannel?>(null)

    /** [channel] is about to be watched. */
    fun give(channel: LiveChannel) = given.set(channel)

    /** The channel given for [id], once; `null` when none was, or another. */
    fun take(id: Long): LiveChannel? = given.getAndSet(null)?.takeIf { it.id == id }
}

/**
 * A category's channels a page at a time, as a list scrolls ([showing]):
 * its first page (which says how many there are) and the pages covering
 * the rows in view, so a lineup of tens of thousands costs what's on
 * screen.
 */
@OptIn(ExperimentalCoroutinesApi::class)
class ChannelPaging(
    private val live: LiveTvRepository,
    category: Flow<LiveCategory>,
    query: Flow<String>,
    scope: CoroutineScope,
) {
    private val rows = MutableStateFlow(0..0)

    val pages: StateFlow<ChannelPages> = combine(category.distinctUntilChanged(), query.map { it.trim() }.distinctUntilChanged(), ::Pair)
        .flatMapLatest { (c, q) ->
            rows.map { pagesFor(it) }.distinctUntilChanged().flatMapLatest { wanted ->
                combine(wanted.map { p -> live.page(c, q, p).map { p to it } }) { loads -> pagesOf(loads.toList()) }
            }
        }
        .stateIn(scope, SharingStarted.WhileSubscribed(LiveTvViewModel.STOP_AFTER_MS), ChannelPages())

    /** Rows [first] to [last] are in view (a few either side are asked for with them). Cheap: call it as the list scrolls. */
    fun showing(first: Int, last: Int) {
        rows.value = first.coerceAtLeast(0)..last.coerceAtLeast(first)
    }

    private fun pagesFor(range: IntRange): List<Int> {
        val from = (range.first - MARGIN).coerceAtLeast(0) / LiveTvRepository.PAGE
        val to = (range.last + MARGIN) / LiveTvRepository.PAGE
        return (setOf(0) + (from..to)).sorted()
    }

    private fun pagesOf(loads: List<Pair<Int, Loadable<LiveChannelPage>>>): ChannelPages {
        val first = loads.firstOrNull { it.first == 0 }?.second
        val total = first?.valueOrNull?.total
        // A page past the end (the list got shorter) shows nothing.
        val pages = loads.mapNotNull { (p, load) -> load.valueOrNull?.let { page -> p to page.channels } }.toMap()
        return ChannelPages(total, pages, (first as? Loadable.Failed)?.takeIf { total == null }?.error)
    }

    private companion object {
        /** Rows asked for either side of what shows. */
        const val MARGIN = 20
    }
}

/**
 * Where Live TV's changes run (a favourite set): to their end on the main
 * thread's loop, whatever happens to the screen that asked.
 */
internal val LiveWork: CoroutineScope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
