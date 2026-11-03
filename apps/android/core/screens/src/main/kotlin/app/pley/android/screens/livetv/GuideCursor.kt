package app.pley.android.screens.livetv

import app.pley.android.model.LiveChannel
import app.pley.android.model.LiveProgram

/** What OK on the guide does. */
enum class GuideOk {
    /** Nothing has focus (a row still loading). */
    None,

    /** A programme on now, or a channel without guide: the live player. */
    Watch,

    /** Another programme: its details in the side panel. */
    Details,
}

/**
 * Focus in the TV's programme guide (Samsung's GuideCursor), moved by the
 * guide itself rather than by geometry: a [row] (the channel), a [cursor]
 * time (the programme on then has focus) and the grid's [left] edge, which
 * shows [VIEW_S] from there. The guide starts at [start] and goes on for
 * [LiveTv.GUIDE_SPAN_S]. Each move gives the cursor after it.
 *
 * - Left and Right go to the programme before and after, the grid
 *   following by half hours. Left on a programme that began with the guide
 *   or before it, the grid scrolled on, goes back through it a screen at a
 *   time; where nothing is in view (a channel without a guide), Left and
 *   Right move the grid and the cursor a half hour. Left leaves for the
 *   categories only from the guide's first half hour ([moveLeft] is `null`).
 * - Up and Down keep the time and change channel; Up from the first row
 *   leaves for the tabs. Channel up and down move [PAGE] rows.
 * - Rewind and fast forward move the grid and the cursor [JUMP_S] ([jump]).
 * - OK on a programme on now (or a channel without guide) watches the
 *   channel, on another opens its details, as does holding OK or Menu.
 * - Back away from now comes back to now first ([awayFromNow], [backToNow]).
 */
data class GuideCursor(val start: Long, val row: Int, val cursor: Long, val left: Long) {
    /** The guide opened at [now]: the first row, the grid from the half hour before now's. */
    constructor(start: Long, now: Long) : this(start, 0, now, leftFor(now, start))

    /** Where the guide ends: two days on. */
    val end: Long get() = start + LiveTv.GUIDE_SPAN_S

    /** The programme with focus, of the focused row's [programs]. */
    fun current(programs: List<LiveProgram>?): LiveProgram? = on(programs, cursor)

    /** The list opens at [row] (the landing) within [total] rows. */
    fun place(row: Int, total: Int): GuideCursor = copy(row = row.coerceAtMost(total - 1).coerceAtLeast(0))

    /** The row kept within [total] rows, as the list's length changes. */
    fun fit(total: Int): GuideCursor = if (row in 0 until total || row == 0) this else place(row, total)

    /** The cursor at [time] (a programme's start, or a half hour), the grid following it. */
    fun moveTo(time: Long): GuideCursor {
        val at = time.coerceAtMost(end - 1).coerceAtLeast(start)
        val edge = when {
            at < left -> maxOf(slot(at), start)
            at >= left + VIEW_S - LiveTv.SLOT_S -> (slot(at) - LiveTv.SLOT_S).coerceAtMost(end - VIEW_S).coerceAtLeast(start)
            else -> left
        }
        return copy(cursor = at, left = edge)
    }

    /** Left on the focused row's [programs]: `null` when focus leaves the guide for the categories. */
    fun moveLeft(programs: List<LiveProgram>?): GuideCursor? {
        val current = current(programs)
        if (current != null && current.start <= start && left > start) {
            // The programme began with the guide or before it, and the grid is scrolled on: the grid
            // goes back through it first, a screen at a time. Gone by the programme alone, Left took
            // focus to the categories with the grid still scrolled on, the time before it unseen.
            val edge = maxOf(start, slot(current.start), left - (VIEW_S - LiveTv.SLOT_S))
            return copy(left = edge, cursor = maxOf(current.start, start, edge))
        }
        val target = current?.start?.minus(1) ?: (cursor - LiveTv.SLOT_S)
        // Only from the guide's first half hour. Where a channel has no programmes it goes back a half
        // hour at a time, as Right goes on: refused there, Left took focus to the categories from
        // wherever Right had gone, with no way back through the time.
        if (target < start) return null
        if (current == null && blank(programs) && left > start) {
            val edge = maxOf(start, left - LiveTv.SLOT_S)
            return copy(left = edge, cursor = maxOf(edge, slot(target)))
        }
        val before = on(programs, target)
        return moveTo(if (before != null) maxOf(before.start, slot(target)) else slot(target))
    }

    /** Right on the focused row's [programs]: the next programme, or the next half hour; never past the guide. */
    fun moveRight(programs: List<LiveProgram>?): GuideCursor {
        val current = current(programs)
        val target = current?.stop ?: (slot(cursor) + LiveTv.SLOT_S)
        if (target >= end) return this
        val lastLeft = maxOf(start, end - VIEW_S)
        if (current == null && blank(programs) && left < lastLeft) {
            // Nothing in view (a channel without a guide): the grid moves on a half hour with the
            // cursor at each press. Moved alone, the cursor went unseen until it neared the edge.
            val edge = minOf(lastLeft, left + LiveTv.SLOT_S)
            return copy(left = edge, cursor = maxOf(edge, target))
        }
        return moveTo(target)
    }

    /** Up: `null` from the first row, where focus leaves for the tabs above. */
    fun moveUp(): GuideCursor? = if (row == 0) null else copy(row = row - 1)

    fun moveDown(total: Int): GuideCursor = if (row < total - 1) copy(row = row + 1) else this

    /** Channel up: [PAGE] rows up. */
    fun pageUp(): GuideCursor = copy(row = maxOf(0, row - PAGE))

    /** Channel down: [PAGE] rows down. */
    fun pageDown(total: Int): GuideCursor = copy(row = minOf(maxOf(0, total - 1), row + PAGE))

    /**
     * Rewind and fast forward ([by] seconds back or on, the web's): the grid
     * moves, and the cursor with it, within the guide; at its ends, nothing.
     */
    fun jump(by: Long): GuideCursor {
        val edge = (left + by).coerceAtMost(maxOf(start, end - VIEW_S)).coerceAtLeast(start)
        if (edge == left) return this
        return copy(left = edge, cursor = (cursor + by).coerceAtMost(edge + VIEW_S - 1).coerceAtLeast(edge))
    }

    /** What OK (released, not held) does on the focused row: its [channel] and [programs]. */
    fun ok(channel: LiveChannel?, programs: List<LiveProgram>?, now: Long): GuideOk {
        if (channel == null) return GuideOk.None
        val current = current(programs)
        return if (current == null || current.airingAt(now)) GuideOk.Watch else GuideOk.Details
    }

    /**
     * Whether Back first brings the guide back to now: focus is on something
     * not airing, or the grid moved.
     */
    fun awayFromNow(programs: List<LiveProgram>?, now: Long): Boolean =
        left != leftFor(now, start) ||
            // Where there is no programme (a channel without a guide), now's half hour: taken as away
            // there always, Back went on coming back to now and never left the guide.
            (current(programs)?.let { !it.airingAt(now) } ?: (slot(cursor) != slot(now)))

    fun backToNow(now: Long): GuideCursor = copy(cursor = now, left = leftFor(now, start))

    /** Whether the row has nothing that may come into view (a channel without a guide): the cursor would move unseen in it. */
    private fun blank(programs: List<LiveProgram>?): Boolean {
        val from = maxOf(left - VIEW_S, start)
        val to = left + 2 * VIEW_S
        return programs.orEmpty().none { it.stop > from && it.start < to }
    }

    companion object {
        /** The grid shows this much time: two hours, at 3.6 dp a minute in its 432 dp beside the categories. */
        const val VIEW_S = 120 * 60L

        /** Channel up and down move this many rows. */
        const val PAGE = 5

        /** Rewind and fast forward move the grid this far (the web's). */
        const val JUMP_S = 2 * 3600L

        /** The half hour [time] is in. */
        fun slot(time: Long): Long = Math.floorDiv(time, LiveTv.SLOT_S) * LiveTv.SLOT_S

        /** Where the grid starts for [now]: the half hour before the one it is in, never before the guide. */
        fun leftFor(now: Long, start: Long): Long = maxOf(slot(now) - LiveTv.SLOT_S, start)

        /** The programme on at [time] of [programs]. */
        fun on(programs: List<LiveProgram>?, time: Long): LiveProgram? = programs?.firstOrNull { it.start <= time && it.stop > time }
    }
}
