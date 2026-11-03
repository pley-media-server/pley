package app.pley.android.ui.tv.livetv

import android.view.KeyEvent
import androidx.activity.compose.BackHandler
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.focusable
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
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.listSaver
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.clipToBounds
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.focus.FocusDirection
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.nativeKeyCode
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.layout.layout
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalFocusManager
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
import kotlinx.coroutines.flow.distinctUntilChanged
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
    LaunchedEffect(landing) {
        val l = landing ?: return@LaunchedEffect
        if (l.category != category || placed.value == l) return@LaunchedEffect
        placed.value = l
        focus = focus.place(l.index ?: 0, total)
    }

    // Not on a row still loading: there is nothing to come back to there.
    BackHandler(enabled = focused && channel != null && cursor.awayFromNow(programs[channel.id], now)) { focus = focus.backToNow(now) }

    val pxPerSecond = with(density) { DP_PER_MINUTE.toPx() } / 60f
    val x by animateFloatAsState(
        targetValue = (left - start) * pxPerSecond,
        animationSpec = tween(if (reduced) 0 else PleyMotion.OVERLAY_MS),
        label = "guide",
    )
    val rows = rememberLazyListState()
    LaunchedEffect(row) {
        val visible = rows.layoutInfo.visibleItemsInfo
        val first = visible.firstOrNull()?.index ?: 0
        val last = visible.lastOrNull()?.index ?: 0
        if (row <= first || row >= last) rows.animateScrollToItem((row - 1).coerceAtLeast(0))
    }
    LaunchedEffect(rows) {
        snapshotFlow { rows.layoutInfo.visibleItemsInfo.let { (it.firstOrNull()?.index ?: 0) to (it.lastOrNull()?.index ?: 0) } }
            .distinctUntilChanged()
            .collect { (first, last) -> vm.showingRows(first, last) }
    }
    val skeleton = rememberSkeleton(animated = false)

    Column(modifier.testTag(TV_LIVE_GUIDE)) {
        Focused(channel, channel?.let { LiveTv.channelName(it, category) }, current, origin, now, clock, Modifier.padding(end = TvSafeArea.Horizontal).height(INFO_HEIGHT))
        // The times across the top, the day at the left.
        Row(Modifier.fillMaxWidth().height(28.dp)) {
            Box(Modifier.width(CHANNEL_WIDTH).fillMaxHeight().padding(start = CHANNEL_INSET), contentAlignment = Alignment.CenterStart) {
                BasicText(dayOf(maxOf(left, start), now), style = PleyType.Tv.Caption.copy(color = PleyColors.Fg), maxLines = 1)
            }
            Box(Modifier.weight(1f).fillMaxHeight().padding(end = TvSafeArea.Horizontal).clipToBounds()) {
                var t = GuideCursor.slot(left - VIEW_S)
                while (t < left + 2 * VIEW_S) {
                    val slotStart = t
                    // The time now, in its pill, takes the place of a time under it (the web's).
                    val underNow = now > slotStart - NOW_PILL_S / 2 && now < slotStart + SLOT_LABEL_S + NOW_PILL_S / 2
                    if (slotStart >= start && !underNow) {
                        val midnight = Calendar.getInstance().apply { timeInMillis = slotStart * 1000 }.let { it.get(Calendar.HOUR_OF_DAY) == 0 && it.get(Calendar.MINUTE) == 0 }
                        BasicText(
                            if (midnight) dayOf(slotStart, now) else clock(slotStart),
                            style = PleyType.Tv.Caption.copy(color = if (midnight) PleyColors.Fg else PleyColors.Muted),
                            maxLines = 1,
                            modifier = Modifier
                                .align(Alignment.CenterStart)
                                .offset { IntOffset(((slotStart - start) * pxPerSecond - x).roundToInt() + 6.dp.roundToPx(), 0) },
                        )
                    }
                    t += LiveTv.SLOT_S
                }
                // The time now, in a pill over the now line.
                BasicText(
                    clock(now),
                    style = PleyType.Tv.Badge.copy(color = PleyColors.OnAccent),
                    maxLines = 1,
                    modifier = Modifier
                        .align(Alignment.CenterStart)
                        .layout { measurable, constraints ->
                            val pill = measurable.measure(constraints.copy(minWidth = 0))
                            layout(pill.width, pill.height) { pill.place(((now - start) * pxPerSecond - x).roundToInt() - pill.width / 2, 0) }
                        }
                        .background(PleyColors.Accent, PleyShape.Badge)
                        .padding(horizontal = 6.dp, vertical = 2.dp),
                )
            }
        }
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
                            if (down) focus = at.pageUp()
                            true
                        }
                        KeyEvent.KEYCODE_CHANNEL_DOWN, KeyEvent.KEYCODE_PAGE_DOWN -> {
                            if (down) focus = at.pageDown(total)
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
                    GuideRow(
                        channel = c,
                        name = LiveTv.channelName(c, category),
                        programs = programs[c.id],
                        loaded = guide.loaded,
                        origin = origin,
                        start = start,
                        left = left,
                        now = now,
                        pxPerSecond = pxPerSecond,
                        x = { x },
                        focusedStart = if (index == row && focused) current?.start ?: -1L else null,
                        rowFocused = index == row && focused,
                    )
                }
            }
            // Now, down the grid.
            val nowX = (now - start) * pxPerSecond
            Box(
                Modifier
                    .padding(start = CHANNEL_WIDTH, end = TvSafeArea.Horizontal)
                    .fillMaxSize()
                    .clipToBounds(),
            ) {
                Box(
                    Modifier
                        .offset { IntOffset((nowX - x).roundToInt() - 1.dp.roundToPx(), 0) }
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

/** One channel's row: its cell and its programmes, the one with focus filled with the accent. */
@Composable
private fun GuideRow(
    channel: LiveChannel,
    name: String,
    programs: List<LiveProgram>?,
    loaded: Boolean,
    origin: String,
    start: Long,
    left: Long,
    now: Long,
    pxPerSecond: Float,
    x: () -> Float,
    focusedStart: Long?,
    rowFocused: Boolean,
) {
    val density = LocalDensity.current
    val clock = rememberClock()
    // What may come into view as the grid moves a screen either way.
    val visible = remember(programs, left, start) { LiveTv.between(programs, maxOf(left - VIEW_S, start), left + 2 * VIEW_S) }
    Row(
        Modifier
            .fillMaxWidth()
            .height(ROW_HEIGHT)
            .drawBehind { drawLine(PleyColors.Surface4.copy(alpha = 0.6f), Offset(0f, size.height - 0.5f), Offset(size.width, size.height - 0.5f), 1f) },
    ) {
        Row(
            Modifier
                .width(CHANNEL_WIDTH)
                .fillMaxHeight()
                .padding(vertical = 4.dp)
                .clip(PleyShape.Control)
                .background(if (rowFocused) PleyColors.Surface3 else androidx.compose.ui.graphics.Color.Transparent)
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
                    style = PleyType.Tv.Label.copy(color = if (rowFocused) PleyColors.Fg else PleyColors.Muted),
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }
        Box(Modifier.weight(1f).fillMaxHeight().padding(end = TvSafeArea.Horizontal).clipToBounds()) {
            if (visible.isEmpty() && loaded) {
                val label = if (channel.hasGuide) stringResource(R.string.tv_livetv_no_info) else name
                Box(
                    Modifier
                        .fillMaxSize()
                        .padding(vertical = 4.dp, horizontal = CELL_INSET_X)
                        .clip(PleyShape.Control)
                        .background(if (focusedStart == -1L) PleyColors.Accent else PleyColors.Surface.copy(alpha = 0.6f))
                        .padding(start = 12.dp),
                    contentAlignment = Alignment.CenterStart,
                ) {
                    BasicText(label, style = PleyType.Tv.Caption.copy(color = if (focusedStart == -1L) PleyColors.OnAccent else PleyColors.Subtle))
                }
            }
            for (program in visible) {
                val from = maxOf(program.start, start)
                val leftPx = (from - start) * pxPerSecond
                val widthPx = (program.stop - from) * pxPerSecond
                val isFocused = focusedStart == program.start
                val airing = program.airingAt(now)
                val span = spanOf(program, clock)
                Box(
                    Modifier
                        .offset { IntOffset((leftPx - x()).roundToInt(), 0) }
                        .width(with(density) { widthPx.toDp() })
                        .fillMaxHeight()
                        .padding(vertical = 4.dp, horizontal = CELL_INSET_X)
                        .alpha(if (program.stop <= now && !isFocused) 0.55f else 1f)
                        .clip(PleyShape.Control)
                        .background(
                            when {
                                isFocused -> PleyColors.Accent
                                airing -> PleyColors.PrimaryContainer
                                else -> PleyColors.Surface2
                            },
                        )
                        .then(if (airing && !isFocused) Modifier.border(1.dp, PleyColors.Accent.copy(alpha = 0.35f), PleyShape.Control) else Modifier)
                        .semantics { contentDescription = "${program.title}, $span" },
                ) {
                    if (widthPx > MIN_TEXT_PX) {
                        Column(
                            Modifier
                                .fillMaxHeight()
                                .offset {
                                    // A programme begun before the left edge keeps its title in view.
                                    val room = (widthPx - 90.dp.toPx()).coerceAtLeast(0f)
                                    IntOffset((x() - leftPx).coerceIn(0f, room).roundToInt(), 0)
                                }
                                .padding(horizontal = 10.dp),
                            verticalArrangement = Arrangement.Center,
                        ) {
                            BasicText(
                                program.title,
                                style = PleyType.Tv.Label.copy(
                                    color = if (isFocused) PleyColors.OnAccent else PleyColors.Fg,
                                    fontWeight = if (airing || isFocused) FontWeight.Bold else FontWeight.Medium,
                                ),
                                maxLines = 1,
                                overflow = TextOverflow.Ellipsis,
                            )
                            BasicText(
                                span,
                                style = PleyType.Tv.Caption.copy(color = if (isFocused) PleyColors.OnAccent.copy(alpha = 0.8f) else PleyColors.Subtle),
                                maxLines = 1,
                            )
                        }
                    }
                    if (airing) {
                        // From the cell's left edge to the now line (Samsung's BarWidth): as the share played of
                        // the whole programme, one begun before the guide (its cell from the guide's edge) ran past now.
                        val inset = with(density) { CELL_INSET_X.toPx() }
                        val bar = ((now - start) * pxPerSecond - leftPx - inset).coerceIn(0f, (widthPx - 2 * inset).coerceAtLeast(0f))
                        Box(
                            Modifier
                                .align(Alignment.BottomStart)
                                .width(with(density) { bar.toDp() })
                                .height(2.dp)
                                .background(if (isFocused) PleyColors.OnAccent else PleyColors.Accent),
                        )
                    }
                }
            }
        }
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

internal const val TV_LIVE_GUIDE = "pley.tv.live.guide"
