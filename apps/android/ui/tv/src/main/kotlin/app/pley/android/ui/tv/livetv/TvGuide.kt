package app.pley.android.ui.tv.livetv

import android.view.KeyEvent
import androidx.activity.compose.BackHandler
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.focusable
import androidx.compose.foundation.gestures.animateScrollBy
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.requiredWidth
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.wrapContentWidth
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.saveable.listSaver
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clipToBounds
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.draw.drawWithCache
import androidx.compose.ui.focus.FocusDirection
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.addOutline
import androidx.compose.ui.graphics.drawscope.clipPath
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.nativeKeyCode
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.layout.layout
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyMotion
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.PleyTv
import app.pley.android.designsystem.TvSafeArea
import app.pley.android.designsystem.PleyType
import app.pley.android.designsystem.icon.Icon
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.designsystem.reducedMotion
import app.pley.android.designsystem.tile.rememberSkeleton
import app.pley.android.designsystem.tile.skeleton
import app.pley.android.model.LiveCategory
import app.pley.android.model.LiveChannel
import app.pley.android.model.LiveProgram
import app.pley.android.screens.livetv.ChannelPages
import app.pley.android.screens.livetv.GuideCursor
import app.pley.android.screens.livetv.GuideOk
import app.pley.android.screens.livetv.Landing
import app.pley.android.screens.livetv.LiveTv
import app.pley.android.screens.livetv.LiveTvViewModel
import app.pley.android.ui.tv.R
import app.pley.android.ui.tv.kit.LocalKeyRepeatGate
import app.pley.android.ui.tv.kit.initialFocus
import java.util.Calendar
import kotlin.coroutines.resume
import kotlin.math.abs
import kotlinx.coroutines.CancellableContinuation
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlin.math.roundToInt

/**
 * The programme guide on TV (the web's GuideGrid, for a remote): what has
 * focus is described above the grid, channels down and time across, now
 * marked by a line with the time in a pill over it. Focus is a programme,
 * moved by the guide itself rather than by geometry ([GuideCursor]; rows
 * only compose what is in view):
 *
 * - Left and Right go to the programme before and after, the grid
 *   following by half hours, back through a long programme a screen at a
 *   time, and by half hours on a channel without a guide; Left from the
 *   guide's first half hour goes to the categories ([onLeave]) on a press
 *   of its own, not held nor within [EDGE_PAUSE_MS] of Left moving;
 * - Up and Down keep the time and change channel, the category's channels
 *   loaded a page at a time as they come; Up from the first goes to the
 *   tabs above on a press, not held; Channel up and down page through the
 *   lineup;
 * - rewind and fast forward (or Previous and Next) move the grid two hours;
 * - OK on a programme on now (or a channel without guide) watches the
 *   channel, on another opens its details, as does holding OK or Menu;
 * - Back away from now comes back to now first.
 *
 * It opens on the channel last watched, when it is in the category.
 */
@Composable
internal fun TvGuide(
    vm: LiveTvViewModel,
    pages: ChannelPages,
    category: LiveCategory,
    origin: String,
    now: Long,
    onWatch: (LiveChannel) -> Unit,
    onDetails: (LiveChannel, LiveProgram) -> Unit,
    onLeave: () -> Unit,
    modifier: Modifier = Modifier,
    requester: FocusRequester = remember { FocusRequester() },
    /** Takes focus as it shows (not while a search is typed beside it). */
    takeFocus: Boolean = true,
) {
    val guide by vm.guide.collectAsStateWithLifecycle()
    val start = vm.guideStart
    val gate = LocalKeyRepeatGate.current
    val focusManager = LocalFocusManager.current
    val density = LocalDensity.current
    val reduced = reducedMotion()
    val clock = rememberClock()
    val saver = remember(start) { cursorSaver(start) }
    var focus by rememberSaveable(stateSaver = saver) { mutableStateOf(GuideCursor(start, now)) }
    var focused by remember { mutableStateOf(false) }
    var held by remember { mutableStateOf(false) }
    // When Left last moved within the guide (the key's uptime): taps in a row stop at its start.
    val leftMovedAt = remember { longArrayOf(Long.MIN_VALUE / 2) }
    val total = pages.total ?: 0
    // Kept within the rows as the list's length changes: put back after the frame, not while it is drawn.
    val cursor = focus.fit(total)
    if (cursor != focus) SideEffect { focus = cursor }
    val row = cursor.row
    val left = cursor.left
    val channel = pages[row]
    val programs = guide.byChannel
    val current = channel?.let { cursor.current(programs[it.id]) }

    LaunchedEffect(left) { vm.showingTimes(left, left + VIEW_S) }
    // Opens on the channel last watched, once per category and return from the player.
    val landing by vm.landing.collectAsStateWithLifecycle()
    val placed = remember { mutableStateOf<Landing?>(null) }
    // The rows' next scroll goes at once: the row was placed by a landing (not gliding as the guide
    // opens), or moved by a held Channel up or down, which the gate, holding the arrows only, lets repeat.
    val snapRows = remember { booleanArrayOf(false) }
    LaunchedEffect(landing) {
        val l = landing ?: return@LaunchedEffect
        if (l.category != category || placed.value == l) return@LaunchedEffect
        placed.value = l
        val at = focus.place(l.index ?: 0, total)
        snapRows[0] = at.row != focus.row
        focus = at
    }

    // Not on a row still loading: there is nothing to come back to there.
    BackHandler(enabled = focused && channel != null && cursor.awayFromNow(programs[channel.id], now)) { focus = focus.backToNow(now) }

    val pxPerSecond = with(density) { DP_PER_MINUTE.toPx() } / 60f
    val slide = animateFloatAsState(
        targetValue = (left - start) * pxPerSecond,
        animationSpec = tween(if (reduced) 0 else PleyMotion.OVERLAY_MS),
        label = "guide",
    )
    // The grid's scroll and the time, read while drawing: as the grid slides only layers move, and
    // as the time goes on only the bars grow; a row is composed again when a programme begins or ends.
    val x = remember(slide) { { slide.value } }
    val nowState = rememberUpdatedState(now)
    val nowAt = remember(nowState) { { nowState.value } }
    val rows = rememberLazyListState()
    val rowPx = with(density) { ROW_HEIGHT.roundToPx() }
    // The rows scroll when the focused one reaches the first or the last in view, bringing it second
    // (Samsung's): in 150 ms for a press; at once for a landing, and a row at a time with the gate's
    // moves under a held key (TvRow's), where a glide begun again at each repeat never got there.
    LaunchedEffect(row) {
        val visible = rows.layoutInfo.visibleItemsInfo
        val first = visible.firstOrNull()
        val snap = snapRows[0]
        snapRows[0] = false
        if (first != null && row > first.index && row < visible.last().index) return@LaunchedEffect
        val target = (row - 1).coerceAtLeast(0)
        if (first == null || snap || gate.holding || reduced) {
            rows.scrollToItem(target)
        } else {
            // The rows are all as tall: where the target is, in view or not.
            rows.animateScrollBy(((target - first.index) * rowPx + first.offset).toFloat(), PleyMotion.focus())
        }
    }
    LaunchedEffect(rows) {
        snapshotFlow { rows.layoutInfo.visibleItemsInfo.let { (it.firstOrNull()?.index ?: 0) to (it.lastOrNull()?.index ?: 0) } }
            .distinctUntilChanged()
            .collect { (first, last) -> vm.showingRows(first, last) }
    }
    val skeleton = rememberSkeleton(animated = false)
    val fill = remember { RowFill() }
    LaunchedEffect(fill) { fill.run { focus.row } }

    Column(modifier.testTag(TV_LIVE_GUIDE)) {
        val name = remember(channel, category) { channel?.let { LiveTv.channelName(it, category) } }
        Focused(channel, name, current, origin, now, clock, Modifier.padding(end = TvSafeArea.Horizontal).height(INFO_HEIGHT))
        GuideTimes(left, start, now, clock, pxPerSecond, x)
        Box(
            Modifier
                .weight(1f)
                .fillMaxWidth()
                .initialFocus(enabled = takeFocus)
                .focusRequester(requester)
                .onFocusChanged { focused = it.isFocused }
                .onPreviewKeyEvent { event ->
                    val code = event.key.nativeKeyCode
                    val down = event.type == KeyEventType.KeyDown
                    val native = event.nativeKeyEvent
                    // The focus as it is now, not as this frame drew it: two keys may come between frames.
                    val at = focus.fit(total)
                    val c = pages[at.row]
                    val list = c?.let { programs[it.id] }
                    val on = at.current(list)
                    when (code) {
                        KeyEvent.KEYCODE_DPAD_LEFT -> {
                            if (!down) return@onPreviewKeyEvent true
                            val moved = at.moveLeft(list)
                            if (moved != null) {
                                focus = moved
                                leftMovedAt[0] = native.eventTime
                            } else if (!gate.holding && native.eventTime - leftMovedAt[0] >= EDGE_PAUSE_MS) {
                                // Out to the categories on a press of its own: not under a held key, nor
                                // straight after Left went back through the programmes (Samsung's).
                                onLeave()
                            }
                            true
                        }
                        KeyEvent.KEYCODE_DPAD_RIGHT -> {
                            if (down) focus = at.moveRight(list)
                            true
                        }
                        KeyEvent.KEYCODE_DPAD_UP -> {
                            if (!down) return@onPreviewKeyEvent true
                            val moved = at.moveUp()
                            // To the tabs on a press: a held Up stops at the first row.
                            if (moved != null) focus = moved else if (!gate.holding) focusManager.moveFocus(FocusDirection.Up)
                            true
                        }
                        KeyEvent.KEYCODE_DPAD_DOWN -> {
                            if (down) focus = at.moveDown(total)
                            true
                        }
                        KeyEvent.KEYCODE_CHANNEL_UP, KeyEvent.KEYCODE_PAGE_UP -> {
                            if (down) {
                                val moved = at.pageUp()
                                snapRows[0] = native.repeatCount > 0 && moved.row != at.row
                                focus = moved
                            }
                            true
                        }
                        KeyEvent.KEYCODE_CHANNEL_DOWN, KeyEvent.KEYCODE_PAGE_DOWN -> {
                            if (down) {
                                val moved = at.pageDown(total)
                                snapRows[0] = native.repeatCount > 0 && moved.row != at.row
                                focus = moved
                            }
                            true
                        }
                        // Two hours back or on, a press at a time: held, they would run through the two days in a second.
                        KeyEvent.KEYCODE_MEDIA_REWIND, KeyEvent.KEYCODE_MEDIA_PREVIOUS -> {
                            if (down && native.repeatCount == 0) focus = at.jump(-GuideCursor.JUMP_S)
                            true
                        }
                        KeyEvent.KEYCODE_MEDIA_FAST_FORWARD, KeyEvent.KEYCODE_MEDIA_NEXT -> {
                            if (down && native.repeatCount == 0) focus = at.jump(GuideCursor.JUMP_S)
                            true
                        }
                        KeyEvent.KEYCODE_DPAD_CENTER, KeyEvent.KEYCODE_ENTER, KeyEvent.KEYCODE_NUMPAD_ENTER -> {
                            if (c == null) return@onPreviewKeyEvent true
                            if (down) {
                                // A press of its own: a hold whose key-up went to the details it opened is over.
                                if (native.repeatCount == 0) held = false
                                // Held: the details, once.
                                if (native.repeatCount > 0 && !held && on != null) {
                                    held = true
                                    onDetails(c, on)
                                }
                            } else {
                                if (!held) {
                                    when (at.ok(c, list, now)) {
                                        GuideOk.Watch -> onWatch(c)
                                        GuideOk.Details -> on?.let { onDetails(c, it) }
                                        GuideOk.None -> Unit
                                    }
                                }
                                held = false
                            }
                            true
                        }
                        KeyEvent.KEYCODE_MENU, KeyEvent.KEYCODE_INFO -> {
                            if (!down && c != null && on != null) onDetails(c, on)
                            true
                        }
                        else -> false
                    }
                }
                .focusable(),
        ) {
            LazyColumn(
                state = rows,
                userScrollEnabled = false,
                modifier = Modifier.fillMaxSize(),
                contentPadding = PaddingValues(bottom = TvSafeAreaBottom),
            ) {
                items(count = total, key = { index -> pages[index]?.id ?: -index - 1L }, contentType = { index -> if (pages[index] == null) LOADING else "row" }) { index ->
                    val c = pages[index]
                    if (c == null) {
                        Row(Modifier.fillMaxWidth().height(ROW_HEIGHT).padding(vertical = 4.dp)) {
                            Box(Modifier.width(CHANNEL_WIDTH - 8.dp).fillMaxHeight().skeleton(skeleton, PleyShape.Control))
                            Spacer(Modifier.width(10.dp))
                            Box(Modifier.weight(1f).fillMaxHeight().padding(end = TvSafeArea.Horizontal).skeleton(skeleton, PleyShape.Control))
                        }
                        return@items
                    }
                    val list = programs[c.id]
                    val rowFocused = index == row && focused
                    val focusAt = if (rowFocused) current?.start ?: -1L else null
                    // What the row draws, worked out again only when one of these changes; the row is
                    // drawn again only when that does (Samsung's): not at every tick, chunk or move.
                    val cells = remember(list, left, now, focusAt, guide.loaded) { RowCells.of(list, start, left, now, focusAt, guide.loaded) }
                    GuideRow(
                        channel = c,
                        name = remember(c, category) { LiveTv.channelName(c, category) },
                        cells = cells,
                        origin = origin,
                        start = start,
                        pxPerSecond = pxPerSecond,
                        x = x,
                        now = nowAt,
                        clock = clock,
                        rowFocused = rowFocused,
                        index = index,
                        fill = fill,
                    )
                }
            }
            // Now, down the grid, on a layer of its own that slides with it.
            val nowX = (now - start) * pxPerSecond
            Box(
                Modifier
                    .padding(start = CHANNEL_WIDTH, end = TvSafeArea.Horizontal)
                    .fillMaxSize()
                    .clipToBounds(),
            ) {
                Box(
                    Modifier
                        .graphicsLayer { translationX = nowX - x() - 1.dp.toPx() }
                        .width(2.dp)
                        .fillMaxHeight()
                        .background(PleyColors.Accent.copy(alpha = 0.85f)),
                )
            }
        }
    }
}

/** What has focus, above the grid: its title, channel and times, and what the guide says of it. */
@Composable
private fun Focused(channel: LiveChannel?, name: String?, program: LiveProgram?, origin: String, now: Long, clock: (Long) -> String, modifier: Modifier) {
    Row(modifier.fillMaxWidth().padding(top = 4.dp), verticalAlignment = Alignment.Top) {
        Column(Modifier.weight(1f)) {
            if (channel == null) return@Column
            Row(verticalAlignment = Alignment.CenterVertically) {
                if (program != null) BadgeOf(program, Modifier.padding(end = 8.dp))
                BasicText(
                    program?.title ?: name.orEmpty(),
                    style = PleyType.Tv.Section.copy(color = PleyColors.Fg),
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
            val meta = buildList {
                // Without a programme the title is the channel's name already.
                if (program != null) add(listOfNotNull(channel.number, name).joinToString(" ")) else channel.number?.let(::add)
                if (program != null) {
                    add("${dayOf(program.start, now)} ${spanOf(program, clock)}")
                    if (program.airingAt(now)) add(timeLeft(program, now))
                    program.episodeLabel?.let(::add)
                } else {
                    add(stringResource(if (channel.hasGuide) R.string.tv_livetv_no_info else R.string.tv_livetv_no_guide))
                }
            }
            BasicText(meta.joinToString("  ·  "), style = PleyType.Tv.Label.copy(color = PleyColors.Muted), maxLines = 1, modifier = Modifier.padding(top = 2.dp))
            // Cut to what the two lines can show: laid out whole at every move, a long one held each.
            val text = remember(program) { program?.let { p -> listOfNotNull(p.subtitle, p.description).joinToString(" — ").ifEmpty { null }?.let(::shortened) } }
            if (text != null) {
                BasicText(
                    text,
                    style = PleyType.Tv.Body.copy(color = PleyColors.Muted),
                    maxLines = 2,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.padding(top = 6.dp).width(560.dp),
                )
            }
        }
        if (channel != null) {
            ChannelTile(channel, origin, Modifier.padding(start = 24.dp).size(width = 120.dp, height = 68.dp), name = true)
        }
    }
}

/**
 * The times across the top, the day at the left, and the time now in a
 * pill over the now line: drawn again as the grid's edge or the time
 * moves, not at every key (Samsung's header); the times on a layer that
 * slides with the grid ([strip]).
 */
@Composable
private fun GuideTimes(left: Long, start: Long, now: Long, clock: (Long) -> String, pxPerSecond: Float, x: () -> Float) {
    // The half hours a screen either way of the grid's edge, as the programmes are, and which begin a day.
    val slots = remember(left, start, clock) {
        buildList {
            var t = GuideCursor.slot(left - VIEW_S)
            while (t < left + 2 * VIEW_S) {
                if (t >= start) add(SlotTime(t, clock(t), midnight(t)))
                t += LiveTv.SLOT_S
            }
        }
    }
    Row(Modifier.fillMaxWidth().height(28.dp)) {
        Box(Modifier.width(CHANNEL_WIDTH).fillMaxHeight().padding(start = CHANNEL_INSET), contentAlignment = Alignment.CenterStart) {
            BasicText(dayOf(maxOf(left, start), now), style = PleyType.Tv.Caption.copy(color = PleyColors.Fg), maxLines = 1)
        }
        Box(Modifier.weight(1f).fillMaxHeight().padding(end = TvSafeArea.Horizontal).clipToBounds()) {
            Box(Modifier.strip(x).fillMaxHeight()) {
                for (slot in slots) {
                    // The time now, in its pill, takes the place of a time under it (the web's).
                    val underNow = now > slot.time - NOW_PILL_S / 2 && now < slot.time + SLOT_LABEL_S + NOW_PILL_S / 2
                    if (!underNow) {
                        key(slot.time) {
                            BasicText(
                                if (slot.midnight) dayOf(slot.time, now) else slot.label,
                                style = PleyType.Tv.Caption.copy(color = if (slot.midnight) PleyColors.Fg else PleyColors.Muted),
                                maxLines = 1,
                                modifier = Modifier
                                    .align(Alignment.CenterStart)
                                    .offset { IntOffset(((slot.time - start) * pxPerSecond).roundToInt() + 6.dp.roundToPx(), 0) },
                            )
                        }
                    }
                }
                BasicText(
                    clock(now),
                    style = PleyType.Tv.Badge.copy(color = PleyColors.OnAccent),
                    maxLines = 1,
                    modifier = Modifier
                        .align(Alignment.CenterStart)
                        .layout { measurable, constraints ->
                            val pill = measurable.measure(constraints.copy(minWidth = 0))
                            layout(pill.width, pill.height) { pill.place(((now - start) * pxPerSecond).roundToInt() - pill.width / 2, 0) }
                        }
                        .background(PleyColors.Accent, PleyShape.Badge)
                        .padding(horizontal = 6.dp, vertical = 2.dp),
                )
            }
        }
    }
}

/** A half hour across the top of the guide: its time as the device writes it, and whether a day begins at it. */
private class SlotTime(val time: Long, val label: String, val midnight: Boolean)

/** Whether [time] (unix seconds) is midnight where the device is. */
private fun midnight(time: Long): Boolean =
    Calendar.getInstance().apply { timeInMillis = time * 1000 }.let { it.get(Calendar.HOUR_OF_DAY) == 0 && it.get(Calendar.MINUTE) == 0 }

/**
 * The guide's time, two days of it from its start, on a layer the grid's
 * scroll ([x]) moves: as the grid slides, only the layer moves, where each
 * programme's cell was placed again at every frame. In view through its
 * parent's clip.
 */
private fun Modifier.strip(x: () -> Float): Modifier =
    wrapContentWidth(Alignment.Start, unbounded = true)
        .requiredWidth(STRIP_WIDTH)
        .graphicsLayer { translationX = -x() }

/**
 * What a programme's cell draws ([ProgramCell] is drawn again only when it
 * changes): its times and title, whether it airs or has ended, whether it
 * has focus, and how far its title is moved in to stay in view at the
 * grid's left edge ([shift], in seconds of the guide's time).
 */
internal data class CellLook(
    val start: Long,
    val stop: Long,
    val title: String,
    val airing: Boolean,
    val ended: Boolean,
    val focused: Boolean,
    val shift: Long,
)

/**
 * What a guide row draws of its programmes (Samsung's ProgramsLook): the
 * cells of those that may come into view as the grid moves a screen either
 * way; with none, whether the guide has answered for them ([blank]: the
 * channel says why across the row) and whether focus is there. Equal when
 * the row would look the same, whatever the time or the grid's edge: each
 * guide answer brings new lists of the same programmes, and the time ticks
 * every 15 s, which drew every row again.
 */
internal data class RowCells(val cells: List<CellLook>, val blank: Boolean, val blankFocused: Boolean) {
    /** Focus is in the row: drawn again at once as it leaves, never two rows showing it. */
    val hasFocus: Boolean get() = blankFocused || cells.any { it.focused }

    companion object {
        /**
         * The row of [programs] with the grid at [left] (the guide starting
         * at [start]) at [now]; [focus] is the start of the programme with
         * focus (-1 where there is none under the cursor), `null` on a row
         * without focus. [loaded]: the guide has answered for the time in view.
         */
        fun of(programs: List<LiveProgram>?, start: Long, left: Long, now: Long, focus: Long?, loaded: Boolean): RowCells {
            val cells = LiveTv.between(programs, maxOf(left - VIEW_S, start), left + 2 * VIEW_S).map { p ->
                val from = maxOf(p.start, start)
                CellLook(
                    start = p.start,
                    stop = p.stop,
                    title = p.title,
                    airing = p.airingAt(now),
                    ended = p.stop <= now,
                    focused = focus == p.start,
                    // A programme begun before the grid's edge keeps its title in view, with room for it.
                    shift = (left - from).coerceIn(0L, maxOf(0L, p.stop - from - TITLE_ROOM_S)),
                )
            }
            return RowCells(cells, blank = cells.isEmpty() && loaded, blankFocused = cells.isEmpty() && focus == -1L)
        }
    }
}

/**
 * One channel's row ([index] of the list): its cell, and its programmes on
 * a layer that slides with the grid, the one with focus filled with the
 * accent. Drawn again only when [cells] or the focus change, and then in
 * its turn ([fill]) but for focus coming to the row or leaving it; coming
 * into view, it shows its channel first. The bars of what airs follow the
 * time ([now]) as they are drawn.
 */
@Composable
private fun GuideRow(
    channel: LiveChannel,
    name: String,
    cells: RowCells,
    origin: String,
    start: Long,
    pxPerSecond: Float,
    x: () -> Float,
    now: () -> Long,
    clock: (Long) -> String,
    rowFocused: Boolean,
    index: Int,
    fill: RowFill,
) {
    // What the row last drew (not state: kept as it is drawn, it composes nothing), and what its turn let it draw.
    val last = remember { arrayOfNulls<RowCells>(1) }
    var turned by remember { mutableStateOf<RowCells?>(null) }
    val drawn = if (rowFocused || last[0]?.hasFocus == true || turned == cells) cells else last[0]
    if (drawn != cells) {
        LaunchedEffect(cells) {
            fill.turn(index)
            turned = cells
        }
    }
    SideEffect { last[0] = drawn }
    Row(
        Modifier
            .fillMaxWidth()
            .height(ROW_HEIGHT)
            .drawBehind { drawLine(PleyColors.Surface4.copy(alpha = 0.6f), Offset(0f, size.height - 0.5f), Offset(size.width, size.height - 0.5f), 1f) },
    ) {
        ChannelCell(channel, name, origin, rowFocused)
        Box(Modifier.weight(1f).fillMaxHeight().padding(end = TvSafeArea.Horizontal).clipToBounds()) {
            if (drawn?.blank == true) {
                val label = if (channel.hasGuide) stringResource(R.string.tv_livetv_no_info) else name
                Box(
                    Modifier
                        .fillMaxSize()
                        .padding(vertical = 4.dp, horizontal = CELL_INSET_X)
                        .background(if (drawn.blankFocused) PleyColors.Accent else PleyColors.Surface.copy(alpha = 0.6f), PleyShape.Control)
                        .padding(start = 12.dp),
                    contentAlignment = Alignment.CenterStart,
                ) {
                    BasicText(label, style = PleyType.Tv.Caption.copy(color = if (drawn.blankFocused) PleyColors.OnAccent else PleyColors.Subtle))
                }
            }
            if (drawn != null) {
                Box(Modifier.strip(x).fillMaxHeight()) {
                    for (cell in drawn.cells) {
                        key(cell.start) { ProgramCell(cell, start, pxPerSecond, now, clock) }
                    }
                }
            }
        }
    }
}

/**
 * The rows' turns to draw what changed in them (Samsung's catch-up): a
 * guide answer filling a screen of rows drew them all, cells and texts, in
 * one frame, which dropped frames on weak TVs. A row waits its [turn];
 * each frame [ROWS_PER_FRAME] of those waiting draw, the nearest the
 * focused row first ([run]). Made and used on the main thread.
 */
@Stable
internal class RowFill {
    private val waiting = HashMap<Int, CancellableContinuation<Unit>>()
    private val asked = Channel<Unit>(Channel.CONFLATED)

    /**
     * Returns once row [index] may draw. A row waiting under the same index
     * (the list reordered under it: Recently watched after a watch, a
     * favourite gone) draws now: cancelled, it would keep what it last drew
     * until its programmes changed again.
     */
    suspend fun turn(index: Int) = suspendCancellableCoroutine { turn ->
        waiting.put(index, turn)?.resume(Unit)
        turn.invokeOnCancellation { if (waiting[index] === turn) waiting.remove(index) }
        asked.trySend(Unit)
    }

    /** Gives the rows waiting their turns, a frame at a time, nearest [focus] first, for as long as the guide shows. */
    suspend fun run(focus: () -> Int) {
        for (ignored in asked) {
            while (waiting.isNotEmpty()) {
                withFrameNanos { }
                val row = focus()
                waiting.keys.sortedBy { abs(it - row) }.take(ROWS_PER_FRAME).forEach { waiting.remove(it)?.resume(Unit) }
            }
        }
    }
}

/** A row's channel: its logo, number (a star when a favourite) and name; the focused row's filled. */
@Composable
private fun ChannelCell(channel: LiveChannel, name: String, origin: String, focused: Boolean) {
    Row(
        Modifier
            .width(CHANNEL_WIDTH)
            .fillMaxHeight()
            .padding(vertical = 4.dp)
            // Drawn in its shape: clipped, each row's cell was a graphics layer.
            .background(if (focused) PleyColors.Surface3 else Color.Transparent, PleyShape.Control)
            .padding(start = CHANNEL_INSET, end = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        ChannelTile(channel, origin, Modifier.size(width = 56.dp, height = 34.dp))
        Spacer(Modifier.width(10.dp))
        Column(Modifier.weight(1f)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                BasicText(channel.number.orEmpty(), style = PleyType.Tv.Caption.copy(color = PleyColors.Subtle), maxLines = 1)
                if (channel.favorite) Icon(PleyIcons.Star, contentDescription = null, size = 12.dp, tint = PleyColors.Accent, modifier = Modifier.padding(start = 4.dp))
            }
            BasicText(
                name,
                style = PleyType.Tv.Label.copy(color = if (focused) PleyColors.Fg else PleyColors.Muted),
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
    }
}

/**
 * A programme's cell at its time on its row's layer: focused in the
 * accent, airing marked with an edge and a bar to the now line, ended
 * dimmed. Drawn in its shape and dimmed by its colours, without graphics
 * layers: clipped and faded, each cell was a layer, and each ended one
 * drawn off screen first.
 */
@Composable
private fun ProgramCell(cell: CellLook, start: Long, pxPerSecond: Float, now: () -> Long, clock: (Long) -> String) {
    val density = LocalDensity.current
    val resources = LocalResources.current
    val from = maxOf(cell.start, start)
    val leftPx = (from - start) * pxPerSecond
    val widthPx = (cell.stop - from) * pxPerSecond
    // Made once for the programme, not each time its cell is drawn again.
    val span = remember(cell.start, cell.stop, clock, resources) { resources.getString(R.string.tv_livetv_span, clock(cell.start), clock(cell.stop)) }
    val fade = if (cell.ended && !cell.focused) PAST_ALPHA else 1f
    val fill = when {
        cell.focused -> PleyColors.Accent
        cell.airing -> PleyColors.PrimaryContainer
        else -> PleyColors.Surface2
    }
    Box(
        Modifier
            .offset { IntOffset(leftPx.roundToInt(), 0) }
            .width(with(density) { widthPx.toDp() })
            .fillMaxHeight()
            .padding(vertical = 4.dp, horizontal = CELL_INSET_X)
            .background(fill.copy(alpha = fill.alpha * fade), PleyShape.Control)
            .then(if (cell.airing && !cell.focused) Modifier.border(1.dp, PleyColors.Accent.copy(alpha = 0.35f), PleyShape.Control) else Modifier)
            .then(
                if (cell.airing) {
                    val inset = with(density) { CELL_INSET_X.toPx() }
                    Modifier.nowBar(if (cell.focused) PleyColors.OnAccent else PleyColors.Accent, leftPx + inset, start, pxPerSecond, now)
                } else {
                    Modifier
                },
            )
            .semantics { contentDescription = "${cell.title}, $span" },
    ) {
        if (widthPx > MIN_TEXT_PX) {
            // Moved in by as much as the title is, narrower by as much: in the cell, which doesn't clip.
            val shift = with(density) { (cell.shift * pxPerSecond).toDp() }
            Column(
                Modifier.fillMaxHeight().padding(start = 10.dp + shift, end = 10.dp),
                verticalArrangement = Arrangement.Center,
            ) {
                BasicText(
                    cell.title,
                    style = PleyType.Tv.Label.copy(
                        color = (if (cell.focused) PleyColors.OnAccent else PleyColors.Fg).let { it.copy(alpha = it.alpha * fade) },
                        fontWeight = if (cell.airing || cell.focused) FontWeight.Bold else FontWeight.Medium,
                    ),
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                BasicText(
                    span,
                    style = PleyType.Tv.Caption.copy(color = if (cell.focused) PleyColors.OnAccent.copy(alpha = 0.8f) else PleyColors.Subtle.copy(alpha = PleyColors.Subtle.alpha * fade)),
                    maxLines = 1,
                )
            }
        }
    }
}

/**
 * An airing programme's bar along the foot of its cell, from the cell's
 * left edge ([fromPx], on the guide's time line) to the now line
 * (Samsung's BarWidth), in the cell's shape so it meets its rounded corner.
 * [now] is read as it is drawn: the time moves it without composing.
 */
private fun Modifier.nowBar(color: Color, fromPx: Float, start: Long, pxPerSecond: Float, now: () -> Long): Modifier = drawWithCache {
    val shape = Path().apply { addOutline(PleyShape.Control.createOutline(size, layoutDirection, this@drawWithCache)) }
    val height = BAR_HEIGHT.toPx()
    onDrawBehind {
        val width = ((now() - start) * pxPerSecond - fromPx).coerceIn(0f, size.width)
        if (width > 0f) clipPath(shape) { drawRect(color, Offset(0f, size.height - height), Size(width, height)) }
    }
}

/**
 * The guide's focus, kept with the page: its row, time and the grid's
 * edge. None when the guide starts later than it did (the app was let go
 * meanwhile): it opens at now again.
 */
private fun cursorSaver(start: Long) = listSaver<GuideCursor, Long>(
    save = { listOf(it.row.toLong(), it.cursor, it.left) },
    restore = { (row, cursor, left) -> GuideCursor(start, row.toInt(), cursor, left).takeIf { cursor >= start && left >= start } },
)

/** The grid shows this much time: two hours at 3.6 dp a minute in its 432 dp, beside the categories. */
private const val VIEW_S = GuideCursor.VIEW_S
private val DP_PER_MINUTE = 3.6.dp
private val ROW_HEIGHT = 52.dp
private val CHANNEL_WIDTH = 160.dp

/** The channel cell's filling starts this far left of the page's gutter, so its text lines up with it. */
private val CHANNEL_INSET = 10.dp
private val INFO_HEIGHT = 92.dp
private val TvSafeAreaBottom = 27.dp

/** How much of the guide's time the time-now pill and a half hour's time take, about (the web's 16 and 18 minutes). */
private const val NOW_PILL_S = 16 * 60L
private const val SLOT_LABEL_S = 18 * 60L

/** How long after Left last moved within the guide a Left at its start still stays: taps in a row. */
internal const val EDGE_PAUSE_MS = 500L

/** A programme's cell is inset this far at either side, 4 dp above and below. */
private val CELL_INSET_X = 2.dp

/** Narrower than this, a programme shows no text. */
private const val MIN_TEXT_PX = 70f

/** The room a title keeps in its cell as it is moved in to stay in view: 90 dp, 25 minutes of the guide. */
private const val TITLE_ROOM_S = 25 * 60L

/** An ended programme's cell, text and fill, at this much of its colours. */
private const val PAST_ALPHA = 0.55f

/** Rows drawing what changed in them each frame, the rest waiting their turn ([RowFill]). */
private const val ROWS_PER_FRAME = 2

/** How tall an airing programme's bar is, along its cell's foot. */
private val BAR_HEIGHT = 2.dp

/** The guide's two days, as wide as the grid draws them: the layer the programmes and the times are on. */
private val STRIP_WIDTH = DP_PER_MINUTE * (LiveTv.GUIDE_SPAN_S / 60f)

internal const val TV_LIVE_GUIDE = "pley.tv.live.guide"
