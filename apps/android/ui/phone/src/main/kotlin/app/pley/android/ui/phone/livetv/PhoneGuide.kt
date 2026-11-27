package app.pley.android.ui.phone.livetv

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.Orientation
import androidx.compose.foundation.gestures.ScrollableState
import androidx.compose.foundation.gestures.animateScrollBy
import androidx.compose.foundation.gestures.scrollable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
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
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.Stable
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.clipToBounds
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.layout.layout
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.icon.Icon
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.designsystem.tile.rememberSkeleton
import app.pley.android.designsystem.tile.skeleton
import app.pley.android.model.LiveCategory
import app.pley.android.model.LiveChannel
import app.pley.android.model.LiveNames
import app.pley.android.model.LiveProgram
import app.pley.android.screens.livetv.ChannelPages
import app.pley.android.screens.livetv.LiveTv
import app.pley.android.screens.livetv.LiveTvViewModel
import app.pley.android.ui.phone.R
import app.pley.android.ui.phone.theme.LocalPhoneMetrics
import app.pley.android.ui.phone.theme.WidthClass
import java.util.Calendar
import kotlin.math.floor
import kotlin.math.roundToInt
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.launch

/** The guide's measures: a narrow one for phones held upright, a wide one beyond. */
private class GuideLayout(val dpPerMinute: Dp, val rowHeight: Dp, val channelWidth: Dp, val headerHeight: Dp, val names: Boolean) {
    companion object {
        val Narrow = GuideLayout(4.dp, 64.dp, 92.dp, 40.dp, names = false)
        val Wide = GuideLayout(5.5.dp, 68.dp, 200.dp, 44.dp, names = true)
    }
}

/**
 * Where the guide is scrolled to across time, in px from its start. One
 * for every row and the header, read where things are placed, so
 * scrolling never recomposes the rows; a row only composes again when
 * the time in view moves on by half a screen.
 */
@Stable
private class GuideScroll(initial: Float) {
    var x by mutableFloatStateOf(initial)
    var max by mutableFloatStateOf(Float.MAX_VALUE)

    val state = ScrollableState { delta ->
        val before = x
        x = (x + delta).coerceIn(0f, max)
        x - before
    }

    suspend fun animateTo(target: Float) {
        state.animateScrollBy(target.coerceIn(0f, max) - x)
    }
}

/**
 * The program guide (the web's GuideGrid): channels down, time across
 * from half an hour before now for two days, now marked by a line. Rows
 * scroll up and down, time sideways; programs are fetched by chunk as
 * they come into view. A channel plays at a tap, a program opens its
 * details; Now brings the time back.
 */
@Composable
internal fun PhoneGuide(
    vm: LiveTvViewModel,
    channels: ChannelPages,
    category: LiveCategory,
    origin: String,
    now: Long,
    onWatch: (LiveChannel) -> Unit,
    onDetails: (LiveChannel, LiveProgram) -> Unit,
) {
    val palette = PleyColors.palette
    val metrics = LocalPhoneMetrics.current
    val layout = if (metrics.widthClass == WidthClass.Compact) GuideLayout.Narrow else GuideLayout.Wide
    val density = LocalDensity.current
    val start = vm.guideStart
    val pxPerSecond = with(density) { layout.dpPerMinute.toPx() } / 60f
    val guide by vm.guide.collectAsStateWithLifecycle()
    val scope = rememberCoroutineScope()
    val clock = rememberClock()
    val scroll = remember(pxPerSecond) { GuideScroll(((now - start - LiveTv.SLOT_S) * pxPerSecond).coerceAtLeast(0f)) }
    BoxWithConstraints(Modifier.fillMaxSize().testTag(LIVE_TV_GUIDE)) {
        val channelPx = with(density) { layout.channelWidth.toPx() }
        val viewport = (constraints.maxWidth - channelPx).coerceAtLeast(1f)
        scroll.max = (LiveTv.GUIDE_SPAN_S * pxPerSecond - viewport).coerceAtLeast(0f)
        val half = viewport / 2
        val bucket by remember(half) { derivedStateOf { floor(scroll.x / half).toInt() } }
        // What is composed: from half a screen before the left edge to half a screen past the right.
        val viewFrom = start + ((bucket - 1) * half / pxPerSecond).toLong()
        val viewTo = start + ((bucket + 4) * half / pxPerSecond).toLong()
        LaunchedEffect(viewFrom, viewTo) { vm.showingTimes(viewFrom, viewTo) }
        val nowX = (now - start) * pxPerSecond
        val nowInView by remember(nowX, viewport) { derivedStateOf { nowX >= scroll.x && nowX <= scroll.x + viewport } }
        val leftEdge = start + (scroll.x / pxPerSecond).toLong()

        Column(Modifier.fillMaxSize().scrollable(scroll.state, Orientation.Horizontal, reverseDirection = true)) {
            // The times across the top, the day at the left.
            Row(
                Modifier
                    .fillMaxWidth()
                    .height(layout.headerHeight)
                    .background(PleyColors.Bg)
                    .drawBehind { drawLine(palette.border, Offset(0f, size.height - 0.5f), Offset(size.width, size.height - 0.5f), 1f) },
            ) {
                Box(Modifier.width(layout.channelWidth).fillMaxHeight().padding(start = 12.dp), contentAlignment = Alignment.CenterStart) {
                    Text(dayOf(maxOf(leftEdge, start), now), style = MaterialTheme.typography.labelMedium, color = PleyColors.Fg, maxLines = 1)
                }
                Box(Modifier.weight(1f).fillMaxHeight().clipToBounds()) {
                    var t = Math.floorDiv(maxOf(viewFrom, start), LiveTv.SLOT_S) * LiveTv.SLOT_S
                    while (t < minOf(viewTo, start + LiveTv.GUIDE_SPAN_S)) {
                        val slot = t
                        val x = (slot - start) * pxPerSecond
                        Box(
                            Modifier
                                .offset { IntOffset((x - scroll.x).roundToInt(), 0) }
                                .fillMaxHeight()
                                .drawBehind { drawLine(palette.border, Offset(0f, size.height * 0.3f), Offset(0f, size.height), 1f) }
                                .padding(start = 8.dp),
                            contentAlignment = Alignment.CenterStart,
                        ) {
                            val midnight = Calendar.getInstance().apply { timeInMillis = slot * 1000 }.let { it.get(Calendar.HOUR_OF_DAY) == 0 && it.get(Calendar.MINUTE) == 0 }
                            Text(
                                if (midnight) dayOf(slot, now) else clock(slot),
                                style = MaterialTheme.typography.bodySmall,
                                color = if (midnight) PleyColors.Fg else PleyColors.Muted,
                                fontWeight = if (midnight) FontWeight.SemiBold else null,
                                maxLines = 1,
                            )
                        }
                        t += LiveTv.SLOT_S
                    }
                    if (nowX > 0) {
                        Text(
                            clock(now),
                            style = MaterialTheme.typography.labelSmall,
                            color = PleyColors.OnAccent,
                            maxLines = 1,
                            modifier = Modifier
                                .align(Alignment.BottomStart)
                                .layout { measurable, constraints ->
                                    // Centered on the line.
                                    val chip = measurable.measure(constraints)
                                    layout(chip.width, chip.height) { chip.place((nowX - scroll.x).roundToInt() - chip.width / 2, 0) }
                                }
                                .clip(PleyShape.Badge)
                                .background(PleyColors.Accent)
                                .padding(horizontal = 5.dp, vertical = 1.dp),
                        )
                    }
                }
            }
            Box(Modifier.weight(1f).fillMaxWidth()) {
                val rows = rememberLazyListState()
                val page by vm.state.collectAsStateWithLifecycle()
                val landing by vm.landing.collectAsStateWithLifecycle()
                Landing(landing, page) { index -> rows.scrollToItem(((index ?: 0) - 2).coerceAtLeast(0)) }
                LaunchedEffect(rows) {
                    snapshotFlow { rows.layoutInfo.visibleItemsInfo.let { (it.firstOrNull()?.index ?: 0) to (it.lastOrNull()?.index ?: 0) } }
                        .distinctUntilChanged()
                        .collect { (first, last) -> vm.showingRows(first, last) }
                }
                val skeleton = rememberSkeleton(animated = true)
                LazyColumn(state = rows, modifier = Modifier.fillMaxSize(), contentPadding = PaddingValues(bottom = 72.dp)) {
                    items(count = channels.total ?: 0, key = { index -> channels[index]?.id ?: -index - 1L }, contentType = { "row" }) { index ->
                        val channel = channels[index]
                        if (channel == null) {
                            LoadingRow(layout, skeleton)
                            return@items
                        }
                        GuideRow(
                            channel = channel,
                            name = LiveNames.channelName(channel, category),
                            programs = guide.byChannel[channel.id],
                            loaded = guide.loaded,
                            origin = origin,
                            layout = layout,
                            start = start,
                            viewFrom = viewFrom,
                            viewTo = viewTo,
                            now = now,
                            pxPerSecond = pxPerSecond,
                            scroll = scroll,
                            onWatch = { onWatch(channel) },
                            onDetails = { onDetails(channel, it) },
                        )
                    }
                }
                // Now, down the grid.
                Box(Modifier.padding(start = layout.channelWidth).fillMaxSize().clipToBounds()) {
                    if (nowX > 0) {
                        Box(
                            Modifier
                                .offset { IntOffset((nowX - scroll.x).roundToInt() - 1.dp.roundToPx(), 0) }
                                .width(2.dp)
                                .fillMaxHeight()
                                .background(PleyColors.Accent.copy(alpha = 0.85f)),
                        )
                    }
                }
                if (!nowInView) {
                    Row(
                        Modifier
                            .align(Alignment.BottomEnd)
                            .padding(end = metrics.marginEnd, bottom = 16.dp)
                            .clip(PleyShape.Circle)
                            .background(PleyColors.Accent)
                            .clickable(role = Role.Button) { scope.launch { scroll.animateTo((now - start - LiveTv.SLOT_S) * pxPerSecond) } }
                            .padding(horizontal = 16.dp, vertical = 10.dp)
                            .testTag(LIVE_TV_NOW),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Icon(PleyIcons.Clock, contentDescription = null, size = 18.dp, tint = PleyColors.OnAccent)
                        Spacer(Modifier.width(6.dp))
                        Text(stringResource(R.string.livetv_now), style = MaterialTheme.typography.labelLarge, color = PleyColors.OnAccent)
                    }
                }
            }
        }
    }
}

/** A row whose channel is still on its way. */
@Composable
private fun LoadingRow(layout: GuideLayout, skeleton: app.pley.android.designsystem.tile.SkeletonState) {
    Row(Modifier.fillMaxWidth().height(layout.rowHeight).padding(vertical = 4.dp, horizontal = 10.dp), verticalAlignment = Alignment.CenterVertically) {
        Box(Modifier.size(width = 64.dp, height = 34.dp).skeleton(skeleton, PleyShape.Field))
        Spacer(Modifier.width(16.dp))
        Box(Modifier.weight(1f).fillMaxHeight().skeleton(skeleton, PleyShape.Field))
    }
}

/** One channel's row: its cell (logo, number, name) and its programs, placed by time. */
@Composable
private fun GuideRow(
    channel: LiveChannel,
    name: String,
    programs: List<LiveProgram>?,
    loaded: Boolean,
    origin: String,
    layout: GuideLayout,
    start: Long,
    viewFrom: Long,
    viewTo: Long,
    now: Long,
    pxPerSecond: Float,
    scroll: GuideScroll,
    onWatch: () -> Unit,
    onDetails: (LiveProgram) -> Unit,
) {
    val palette = PleyColors.palette
    val density = LocalDensity.current
    val visible = remember(programs, viewFrom, viewTo, start) { LiveTv.between(programs, maxOf(viewFrom, start), viewTo) }
    Row(
        Modifier
            .fillMaxWidth()
            .height(layout.rowHeight)
            .drawBehind { drawLine(palette.border.copy(alpha = 0.7f), Offset(0f, size.height - 0.5f), Offset(size.width, size.height - 0.5f), 1f) },
    ) {
        val watch = stringResource(R.string.livetv_watch, listOfNotNull(channel.number, name).joinToString(" "))
        Row(
            Modifier
                .width(layout.channelWidth)
                .fillMaxHeight()
                .clickable(role = Role.Button, onClickLabel = watch, onClick = onWatch)
                .semantics(mergeDescendants = true) { contentDescription = watch }
                .padding(horizontal = if (layout.names) 10.dp else 6.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            if (!layout.names) {
                // Narrow: the logo over the number, as many logos look alike.
                Column(Modifier.fillMaxWidth(), horizontalAlignment = Alignment.CenterHorizontally) {
                    ChannelTile(channel, origin, Modifier.size(width = 64.dp, height = 34.dp))
                    Text(
                        channel.number ?: name,
                        style = MaterialTheme.typography.labelSmall,
                        color = if (channel.favorite) PleyColors.AccentBright else PleyColors.Muted,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                        modifier = Modifier.padding(top = 3.dp),
                    )
                }
            } else {
                ChannelTile(channel, origin, Modifier.size(width = 60.dp, height = 38.dp))
            }
            if (layout.names) {
                Spacer(Modifier.width(10.dp))
                Column(Modifier.weight(1f)) {
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        Text(channel.number.orEmpty(), style = MaterialTheme.typography.bodySmall, color = PleyColors.Subtle, maxLines = 1)
                        if (channel.favorite) Icon(PleyIcons.Star, contentDescription = null, size = 12.dp, tint = PleyColors.Accent, modifier = Modifier.padding(start = 4.dp))
                    }
                    Text(name, style = MaterialTheme.typography.titleSmall, color = PleyColors.Fg, maxLines = 1, overflow = TextOverflow.Ellipsis)
                }
            }
        }
        Box(Modifier.weight(1f).fillMaxHeight().clipToBounds()) {
            if (visible.isEmpty() && loaded) {
                Box(
                    Modifier
                        .fillMaxSize()
                        .padding(horizontal = 1.dp, vertical = 4.dp)
                        .clip(PleyShape.Field)
                        .background(PleyColors.Surface.copy(alpha = 0.7f))
                        .padding(start = 12.dp),
                    contentAlignment = Alignment.CenterStart,
                ) {
                    if (channel.hasGuide) {
                        Text(stringResource(R.string.livetv_no_info), style = MaterialTheme.typography.bodySmall, color = PleyColors.Subtle, maxLines = 1)
                    } else {
                        Text(name, style = MaterialTheme.typography.bodyMedium, color = PleyColors.Muted, maxLines = 1, overflow = TextOverflow.Ellipsis)
                    }
                }
            }
            for (program in visible) {
                val from = maxOf(program.start, start)
                val left = (from - start) * pxPerSecond
                val widthPx = (program.stop - from) * pxPerSecond
                ProgramCell(
                    program = program,
                    now = now,
                    leftPx = left,
                    widthPx = widthPx,
                    scroll = scroll,
                    modifier = Modifier
                        .offset { IntOffset((left - scroll.x).roundToInt(), 0) }
                        .width(with(density) { widthPx.toDp() })
                        .fillMaxHeight()
                        .padding(horizontal = 1.dp, vertical = 4.dp),
                    onClick = { onDetails(program) },
                )
            }
        }
    }
}

/**
 * A program: its title and times, the one on now edged in the accent
 * with how far it is along its foot, those over dimmed. A program begun
 * before the left edge keeps its title in view.
 */
@Composable
private fun ProgramCell(
    program: LiveProgram,
    now: Long,
    leftPx: Float,
    widthPx: Float,
    scroll: GuideScroll,
    modifier: Modifier,
    onClick: () -> Unit,
) {
    val clock = rememberClock()
    val airing = program.airingAt(now)
    val past = program.stop <= now
    val span = spanOf(program, clock)
    Box(
        modifier
            .alpha(if (past) 0.55f else 1f)
            .clip(PleyShape.Field)
            .background(if (airing) PleyColors.PrimaryContainer else PleyColors.Surface2)
            .then(if (airing) Modifier.border(1.dp, PleyColors.Accent.copy(alpha = 0.4f), PleyShape.Field) else Modifier)
            .clickable(role = Role.Button, onClick = onClick)
            .semantics(mergeDescendants = true) { contentDescription = "${program.title}, $span" },
    ) {
        if (widthPx > MIN_TEXT_PX) {
            Column(
                Modifier
                    .fillMaxHeight()
                    .offset {
                        val room = (widthPx - 80.dp.toPx()).coerceAtLeast(0f)
                        IntOffset((scroll.x - leftPx).coerceIn(0f, room).roundToInt(), 0)
                    }
                    .padding(horizontal = 8.dp),
                verticalArrangement = Arrangement.Center,
            ) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    BadgeOf(program, Modifier.padding(end = 5.dp))
                    Text(
                        program.title,
                        style = MaterialTheme.typography.bodyMedium,
                        fontWeight = if (airing) FontWeight.SemiBold else FontWeight.Medium,
                        color = PleyColors.Fg,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                }
                Text(
                    listOfNotNull(span, program.subtitle).joinToString(" · "),
                    style = MaterialTheme.typography.bodySmall,
                    color = PleyColors.Subtle,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }
        if (airing) {
            Box(
                Modifier
                    .align(Alignment.BottomStart)
                    .fillMaxWidth(program.progressAt(now))
                    .height(2.dp)
                    .background(PleyColors.Accent),
            )
        }
    }
}

/** Narrower than this, a program shows no text. */
private const val MIN_TEXT_PX = 60f

internal const val LIVE_TV_GUIDE = "pley.live.guide"
internal const val LIVE_TV_NOW = "pley.live.now"
