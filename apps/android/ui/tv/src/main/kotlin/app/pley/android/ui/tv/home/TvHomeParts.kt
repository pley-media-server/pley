package app.pley.android.ui.tv.home

import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.focusGroup
import androidx.compose.foundation.focusable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsFocusedAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.wrapContentHeight
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.ReadOnlyComposable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusProperties
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.focusRestorer
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.CompositingStrategy
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.layout.layout
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.rememberTextMeasurer
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.pley.android.common.Tier
import app.pley.android.common.errorSentence
import app.pley.android.common.perf.StartupSignals
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyFocus
import app.pley.android.designsystem.PleyMotion
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.PleyTv
import app.pley.android.designsystem.PleyType
import app.pley.android.designsystem.TvSafeArea
import app.pley.android.designsystem.icon.Icon
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.designsystem.image.LocalArtworkImageLoader
import app.pley.android.designsystem.reducedMotion
import app.pley.android.designsystem.text.asString
import app.pley.android.designsystem.tile.QualityBadge
import app.pley.android.designsystem.tile.TileKind
import app.pley.android.designsystem.tile.TileStyle
import app.pley.android.designsystem.tile.rememberSkeleton
import app.pley.android.designsystem.tile.skeleton
import app.pley.android.screens.home.HomeInfo
import app.pley.android.screens.home.HomeMessage
import app.pley.android.screens.home.HomeMeta
import app.pley.android.ui.tv.R
import app.pley.android.ui.tv.kit.FocusMemory
import app.pley.android.ui.tv.kit.LocalTvRailFocus
import app.pley.android.ui.tv.kit.LocalTvRowColumn
import app.pley.android.ui.tv.kit.TvPillButton
import app.pley.android.ui.tv.kit.TvRoundButton
import app.pley.android.ui.tv.kit.initialFocus
import app.pley.android.ui.tv.theme.LocalTvTier
import coil3.SingletonImageLoader
import coil3.compose.LocalPlatformContext
import coil3.request.ImageRequest
import kotlinx.coroutines.async
import kotlinx.coroutines.awaitAll
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.withTimeoutOrNull
import kotlin.math.abs
import kotlin.math.min
import kotlin.math.roundToInt

/**
 * The hero's stack at x = 96 (design spec §2.2): for a featured title why
 * it is there ([label], "New in Movies"), the title (logo art once the
 * server has it), an episode's "S2 · E5 · The Long Night", the metadata
 * line, progress, three lines of summary, then Resume (or Play) and the
 * round buttons From start, Info, and My List for movies and shows (Watched
 * for the rest). A missing piece closes its gap. Resume ([resume]) is the
 * page's initial focus.
 *
 * The label is level with the dashes at the top right. The title and
 * the summary have more room than the text column
 * ([HeroColumn], [SummaryColumn]), so few titles take two lines. The label stays on its
 * line: a title of two lines grows down into the summary's room, which
 * keeps two lines for it, so the buttons stay where they are. The
 * buttons are the kit's compact ones, smaller than a page's.
 *
 * With several titles ([turns]), dashes at the top right show which is on,
 * the one on filling as its time runs ([progress]). They take focus, Up
 * from the buttons ([onDashesFocus] hears it): Left and Right there turn
 * to the title before or after ([onTurn]), and Down goes back to Resume.
 * Right from the last button stays on it: only the dashes, and the time,
 * turn the titles. A title turned to fades its words in; the buttons stay
 * as they are.
 *
 * It is one focus group that the rows' column comes back to by its key
 * ([HERO_KEY]), always on Resume once the page went back to its top.
 */
@Composable
internal fun TvHomeHero(
    hero: HomeInfo,
    resume: FocusRequester,
    memory: FocusMemory,
    onFocused: () -> Unit,
    onPlay: (fromStart: Boolean) -> Unit,
    onInfo: () -> Unit,
    onWatched: () -> Unit,
    onList: () -> Unit,
    modifier: Modifier = Modifier,
    label: String? = null,
    turns: Int = 1,
    turn: Int = 0,
    progress: () -> Float = { 1f },
    onTurn: (Int) -> Unit = {},
    onDashesFocus: (Boolean) -> Unit = {},
) {
    // The title's lines decide the summary's: measured here, so the page lays out once.
    val measurer = rememberTextMeasurer()
    val density = LocalDensity.current
    val titleLines = remember(hero.title, measurer, density) {
        val width = with(density) { HeroColumn.roundToPx() }
        measurer.measure(hero.title, PleyType.Tv.HeroTitle, maxLines = 2, constraints = Constraints(maxWidth = width)).lineCount
    }
    val stack = (if (label != null) LabelLine else 0.dp) + HeroTitleLine * titleLines
    // How far the label and title reach below their box: the summary gives that room up.
    val below = (stack - TitleBox - TitleRise).coerceAtLeast(0.dp)
    // An episode's line and the time left stand between the title and the summary, and take of its room too.
    val between = (if (hero.context.isNotEmpty()) ContextRoom else 0.dp) +
        (if (hero.inProgress && hero.timeLeft.isNotEmpty()) TimeLeftRoom else 0.dp)
    // Taking turns, the summary has what is left of its room, so the buttons stay where they are;
    // a hero that stays has what its height leaves, down to the rows.
    val summaryRoom = ((if (turns > 1) SummaryRoom else SummaryMost) - below - between).coerceAtLeast(SummaryLine)
    val summaryLines = (summaryRoom / SummaryLine).toInt().coerceIn(1, SUMMARY_LINES)
    val column = LocalTvRowColumn.current
    val group = remember { FocusRequester() }
    if (column != null) {
        DisposableEffect(column) {
            column.rows[HERO_KEY] = group
            onDispose { if (column.rows[HERO_KEY] === group) column.rows.remove(HERO_KEY) }
        }
    }
    // A title turned to fades its words in (the first is there at once, as is each on the slowest boxes).
    // Made with the title, at nothing: reset in an effect, the words would show whole for a frame first.
    val plain = LocalTvTier.current == Tier.T0
    val opened = remember { booleanArrayOf(false) }
    val words = remember(hero.item.id) {
        Animatable(if (opened[0] && !plain) 0f else 1f).also { opened[0] = true }
    }
    LaunchedEffect(words) { words.animateTo(1f, PleyMotion.reveal()) }
    Column(
        modifier
            // As wide as the page: its dashes are at the far end, and a layer fading it cuts at its edge.
            .fillMaxWidth()
            .height(HeroHeight)
            .wrapContentHeight(Alignment.Top, unbounded = true)
            .focusRequester(group)
            .onFocusChanged {
                if (it.hasFocus) {
                    onFocused()
                    column?.focusedRow = HERO_KEY
                }
            }
            .focusRestorer(resume)
            .focusGroup()
            .padding(start = PleyTv.Gutter, top = HeroTop),
    ) {
        if (turns > 1) {
            // At the page's top right, taking no room in the stack: with focus its chevron ends at the safe area.
            HeroPager(
                turns,
                turn,
                progress,
                onTurn,
                onDashesFocus,
                resume,
                Modifier.layout { measurable, constraints ->
                    val pager = measurable.measure(constraints.copy(minWidth = 0, minHeight = 0))
                    val safe = TvSafeArea.Horizontal.roundToPx()
                    // From the stack's start: read right to left, that is the page's far side, and the dashes are at its left.
                    val x = if (layoutDirection == LayoutDirection.Ltr) constraints.maxWidth - safe - pager.width else safe - constraints.maxWidth
                    layout(0, 0) { pager.place(x, (PagerTop - HeroTop).roundToPx()) }
                },
            )
        }
        // Text alone, nothing over anything else: faded as it is drawn, with no layer of its own to fill.
        Column(
            Modifier.graphicsLayer {
                alpha = words.value
                compositingStrategy = CompositingStrategy.ModulateAlpha
            },
        ) {
            Box(
                Modifier.width(HeroColumn).height(TitleBox + below).wrapContentHeight(Alignment.Bottom, unbounded = true),
                contentAlignment = Alignment.BottomStart,
            ) {
                Column {
                    if (label != null) {
                        BasicText(
                            label.uppercase(),
                            style = PleyType.Tv.Label.copy(color = PleyColors.Fg.copy(alpha = 0.85f), letterSpacing = 3.sp),
                            maxLines = 1,
                        )
                        Spacer(Modifier.height(12.dp))
                    }
                    BasicText(
                        hero.title,
                        style = PleyType.Tv.HeroTitle.copy(color = PleyColors.Fg),
                        maxLines = 2,
                        overflow = TextOverflow.Ellipsis,
                        modifier = Modifier.semantics { heading() },
                    )
                }
            }
            if (hero.context.isNotEmpty()) {
                Spacer(Modifier.height(10.dp))
                BasicText(hero.context, style = PleyType.Tv.Title.copy(color = PleyColors.Fg), maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.width(TextColumn))
            }
            if (!hero.meta.isEmpty) {
                Spacer(Modifier.height(8.dp))
                TvMetaLine(hero.meta)
            }
            if (hero.inProgress && hero.timeLeft.isNotEmpty()) {
                Spacer(Modifier.height(10.dp))
                BasicText(hero.timeLeft, style = PleyType.Tv.Caption.copy(color = PleyColors.Fg), maxLines = 1, modifier = Modifier.heightIn(min = 17.dp))
            }
            if (hero.summary.isNotEmpty() || turns > 1) {
                Spacer(Modifier.height(10.dp))
                // Taking turns, the summary keeps its room: the buttons stay where they are.
                BasicText(
                    hero.summary,
                    style = PleyType.Tv.BodyLarge.copy(color = SummaryColor),
                    maxLines = summaryLines,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.width(SummaryColumn).then(if (turns > 1) Modifier.height(summaryRoom) else Modifier),
                )
            }
        }
        Spacer(Modifier.height(20.dp))
        Row(horizontalArrangement = Arrangement.spacedBy(PleyTv.ButtonGap)) {
            TvPillButton(
                label = stringResource(if (hero.inProgress) R.string.tv_home_resume else R.string.tv_home_play),
                onClick = { onPlay(false) },
                icon = PleyIcons.Play,
                modifier = Modifier.focusRequester(resume).initialFocus(memory),
                compact = true,
            )
            if (hero.inProgress) {
                TvRoundButton(stringResource(R.string.tv_home_from_start), PleyIcons.RotateCcw, onClick = { onPlay(true) }, compact = true)
            }
            TvRoundButton(stringResource(R.string.tv_home_info), PleyIcons.Info, onClick = onInfo, compact = true)
            // Right from the last button stays on it: it neither turns to the next title nor finds the dashes above.
            val last = Modifier.focusProperties { end = FocusRequester.Cancel }
            if (hero.listable) {
                TvRoundButton(
                    stringResource(if (hero.inList) R.string.tv_item_remove_from_my_list else R.string.tv_item_add_to_my_list),
                    if (hero.inList) PleyIcons.Check else PleyIcons.Plus,
                    onClick = onList,
                    modifier = last,
                    compact = true,
                )
            } else {
                TvRoundButton(
                    stringResource(if (hero.watched) R.string.tv_home_mark_unwatched else R.string.tv_home_mark_watched),
                    PleyIcons.CircleCheck,
                    onClick = onWatched,
                    modifier = last,
                    compact = true,
                )
            }
        }
    }
}

/**
 * The hero's dashes, one per title, at the page's top right: plain white
 * dashes with nothing round them, faint but for the one on, which is
 * longer and fills as its time runs. On a turn the long dash glides to
 * the title on, the one left giving up its fill as it shortens.
 *
 * Focused, they brighten and grow a little, a chevron at each end, and
 * that is all: no box, and the accent stays the buttons'. Left and Right
 * turn to the title before or after, Select to the next ([onTurn]), a held
 * key once. Down goes to [down], the hero's first button, not the one
 * nearest under it.
 *
 * It is one box that draws: [progress], the glide and the focus are read as
 * it draws, so none of them composes or lays anything out.
 */
@Composable
private fun HeroPager(
    count: Int,
    current: Int,
    progress: () -> Float,
    onTurn: (Int) -> Unit,
    onFocus: (Boolean) -> Unit,
    down: FocusRequester,
    modifier: Modifier = Modifier,
) {
    val calm = reducedMotion()
    // The slowest boxes go without the glide, as they do without the words' fade.
    val still = calm || LocalTvTier.current == Tier.T0
    val interactions = remember { MutableInteractionSource() }
    val focused by interactions.collectIsFocusedAsState()
    val focus = animateFloatAsState(if (focused) 1f else 0f, if (focused) PleyMotion.focus(calm) else PleyMotion.unfocus(calm), label = "dashes")
    // Where the long dash is, in titles: it goes the short way round, so the last title to the first is one step.
    val at = remember { Animatable(current.toFloat()) }
    LaunchedEffect(current, count, still) {
        val by = shortWay(current - at.value, count)
        // As long as the backdrop takes to change: the dash lands with the picture.
        if (!still && abs(by) > 0.001f) at.animateTo(at.value + by, tween(PleyMotion.BACKDROP_MS, easing = PleyMotion.FocusEasing))
        at.snapTo(current.toFloat())
    }
    val trail = remember { PagerTrail(current) }
    DisposableEffect(Unit) { onDispose { onFocus(false) } }
    val description = stringResource(R.string.tv_home_turns, current + 1, count)
    Box(
        modifier
            .graphicsLayer {
                val scale = 1f + (PleyFocus.PILL_SCALE - 1f) * focus.value
                scaleX = scale
                scaleY = scale
            }
            .onFocusChanged { onFocus(it.isFocused) }
            .focusProperties { this.down = down }
            .onKeyEvent {
                val by = when (it.key) {
                    Key.DirectionLeft -> -1
                    Key.DirectionRight, Key.DirectionCenter, Key.Enter, Key.NumPadEnter -> 1
                    else -> return@onKeyEvent false
                }
                if (it.type == KeyEventType.KeyDown && it.nativeKeyEvent.repeatCount == 0) onTurn(by)
                true
            }
            .focusable(interactionSource = interactions)
            .clearAndSetSemantics { contentDescription = description }
            .size(DashLong + (DashShort + DashGap) * (count - 1) + (PagerPadding + ChevronRoom) * 2, PagerHeight)
            .drawBehind {
                val lit = focus.value
                val on = if (still) current.toFloat() else at.value
                trail.follow(current)
                val time = progress().coerceIn(0f, 1f)

                // White, short of full at rest: they are there to glance at, not to draw the eye from the title.
                val ink = PleyColors.Fg.copy(alpha = DASH_REST + (1f - DASH_REST) * lit)
                val track = PleyColors.Fg.copy(alpha = TRACK_REST + (TRACK_LIT - TRACK_REST) * lit)
                val high = DashHeight.toPx()
                val round = CornerRadius(high / 2)
                val top = (size.height - high) / 2
                var x = (ChevronRoom + PagerPadding).toPx()
                repeat(count) { i ->
                    // How much of the long dash this one is: 1 on the title on, less as the glide passes.
                    val away = (on - i).mod(count.toFloat())
                    val weight = (1f - min(away, count - away)).coerceAtLeast(0f)
                    val wide = DashShort.toPx() + (DashLong - DashShort).toPx() * weight
                    drawRoundRect(track, Offset(x, top), Size(wide, high), round)
                    if (i == current) {
                        // Never less than a dot: the dash on shows as on from its first moment.
                        drawRoundRect(ink, Offset(x, top), Size(high + (wide - high) * time, high), round)
                    } else if (i == trail.left && weight > 0f) {
                        drawRoundRect(ink.copy(alpha = weight), Offset(x, top), Size(high + (wide - high) * trail.leftAt, high), round)
                    }
                    x += wide + DashGap.toPx()
                }
                trail.shown = time

                if (lit > 0f) {
                    // They come in from the dashes' side as focus does.
                    val middle = ChevronRoom.toPx() / 2 + PagerPadding.toPx() * (1f - lit)
                    chevron(middle, -1f, ink.copy(alpha = lit))
                    chevron(size.width - middle, 1f, ink.copy(alpha = lit))
                }
            },
    )
}

/** A chevron about [x] at the box's middle, pointing left ([way] -1) or right (1). */
private fun DrawScope.chevron(x: Float, way: Float, color: Color) {
    val half = ChevronSize.toPx()
    val y = size.height / 2
    val tip = Offset(x + way * half / 2, y)
    val stroke = 1.5.dp.toPx()
    drawLine(color, Offset(x - way * half / 2, y - half), tip, stroke, StrokeCap.Round)
    drawLine(color, Offset(x - way * half / 2, y + half), tip, stroke, StrokeCap.Round)
}

/**
 * What the dashes last drew, kept between draws and not as state: the
 * title on, the one it turned from ([left]) and how far that one's time
 * had run ([leftAt]), which its dash keeps while it shortens.
 */
private class PagerTrail(var on: Int) {
    var left = -1
    var leftAt = 0f
    var shown = 0f

    fun follow(current: Int) {
        if (current == on) return
        left = on
        leftAt = shown
        on = current
    }
}

/** [by] titles on, as the shorter way round [count] of them: from the last to the first is one on, not all of them back. */
private fun shortWay(by: Float, count: Int): Float = by - count * (by / count).roundToInt()

/**
 * The compact block about the focused tile over the rows (design spec
 * §4.1), from y = 28: the title in a 52 dp box, the metadata line 10
 * below (an episode's number first), two lines of summary 8 below.
 */
@Composable
internal fun TvInfoBlock(info: HomeInfo, modifier: Modifier = Modifier) {
    Column(modifier.padding(start = PleyTv.Gutter, top = 28.dp).width(TextColumn)) {
        Box(Modifier.height(52.dp).wrapContentHeight(Alignment.Bottom, unbounded = true), contentAlignment = Alignment.BottomStart) {
            BasicText(info.title, style = PleyType.Tv.PageTitle.copy(color = PleyColors.Fg), maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
        Spacer(Modifier.height(10.dp))
        TvMetaLine(info.meta, lead = info.context)
        if (info.summary.isNotEmpty()) {
            Spacer(Modifier.height(8.dp))
            BasicText(info.summary, style = PleyType.Tv.Body.copy(color = SummaryColor), maxLines = 2, overflow = TextOverflow.Ellipsis)
        }
    }
}

/**
 * The metadata line (design spec §2.2, §2.6): year · content rating (a
 * badge) · runtime · ★ rating · quality badges, in tvLabel
 * [muted][PleyColors.Muted] with [subtle][PleyColors.Subtle] dots. [lead]
 * goes first ("S1 · E3 · Glasshouse") and gives way when space runs out.
 */
@Composable
internal fun TvMetaLine(meta: HomeMeta, modifier: Modifier = Modifier, lead: String = "") {
    val label = PleyType.Tv.Label.copy(color = PleyColors.Muted)
    val dot = PleyType.Tv.Label.copy(color = PleyColors.Subtle)
    val score = meta.rating
    val rating = score?.let { stringResource(R.string.tv_home_rating, it) }
    Row(
        modifier.widthIn(max = TextColumn).height(18.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        var first = true
        val separate: @Composable () -> Unit = {
            if (!first) BasicText("·", style = dot)
            first = false
        }
        if (lead.isNotEmpty()) {
            separate()
            BasicText(lead, style = label.copy(color = PleyColors.Fg), maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f, fill = false))
        }
        if (meta.year.isNotEmpty()) {
            separate()
            BasicText(meta.year, style = label, maxLines = 1)
        }
        meta.contentRating?.let {
            separate()
            QualityBadge(it, TileStyle.Tv)
        }
        if (meta.runtime.isNotEmpty()) {
            separate()
            BasicText(meta.runtime, style = label, maxLines = 1, softWrap = false)
        }
        if (score != null) {
            separate()
            Row(
                Modifier.semantics(mergeDescendants = true) { contentDescription = rating.orEmpty() },
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Icon(PleyIcons.Star, contentDescription = null, size = 14.dp, tint = PleyColors.Accent)
                Spacer(Modifier.width(4.dp))
                BasicText(score, style = label.copy(color = PleyColors.Fg), maxLines = 1)
            }
        }
        if (meta.badges.isNotEmpty()) {
            separate()
            Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) { meta.badges.forEach { QualityBadge(it, TileStyle.Tv) } }
        }
    }
}

private val HomeMeta.isEmpty: Boolean
    get() = year.isEmpty() && contentRating == null && runtime.isEmpty() && rating == null && badges.isEmpty()

/**
 * Home with nothing saved yet (design spec §8): static skeletons in the
 * hero's and the first row's shapes, after a moment, so a Home read from
 * what was saved never flashes one. No spinner.
 */
@Composable
internal fun TvHomeSkeleton() {
    var shown by remember { mutableStateOf(false) }
    LaunchedEffect(Unit) {
        delay(SKELETON_DELAY_MS)
        shown = true
    }
    val loading = stringResource(R.string.tv_home_loading)
    Box(Modifier.fillMaxSize().semantics { contentDescription = loading }) {
        if (!shown) return@Box
        val state = rememberSkeleton(animated = false)
        val bar = RoundedCornerShape(PleyShape.BadgeRadius)
        @Composable
        fun block(x: Dp, y: Dp, w: Dp, h: Dp, shape: androidx.compose.ui.graphics.Shape = bar) =
            Box(Modifier.offset(x, y).size(w, h).skeleton(state, shape))
        block(PleyTv.Gutter, 84.dp, 260.dp, 38.dp)
        block(PleyTv.Gutter, 132.dp, 210.dp, 18.dp)
        block(PleyTv.Gutter, 168.dp, 440.dp, 15.dp)
        block(PleyTv.Gutter, 190.dp, 400.dp, 15.dp)
        block(PleyTv.Gutter, 212.dp, 260.dp, 15.dp)
        block(PleyTv.Gutter, 250.dp, 128.dp, PleyTv.ButtonHeight, PleyShape.Control)
        block(PleyTv.Gutter, 387.dp, 180.dp, 17.dp)
        val tile = TileKind.Landscape
        repeat(4) { i ->
            block(PleyTv.Gutter + (tile.tvWidth + tile.tvGap) * i, 422.dp, tile.tvWidth, tile.tvHeight, TileStyle.Tv.shape(tile))
        }
    }
}

/**
 * A state with a sentence instead of rows (design spec §8): [title] in
 * tvPageTitle and [body] in tvBody [muted][PleyColors.Muted], centred, and
 * a Retry pill with focus when [retry] is given. Without one there is
 * nothing to focus on the page, so focus goes to the rail.
 */
@Composable
internal fun TvHomeNotice(title: String, body: String, retry: (() -> Unit)? = null) {
    val rail = LocalTvRailFocus.current
    Column(
        Modifier.fillMaxSize().padding(start = PleyTv.Gutter, end = TvSafeArea.Horizontal).semantics { liveRegion = LiveRegionMode.Polite },
        verticalArrangement = Arrangement.Center,
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        BasicText(
            title,
            style = PleyType.Tv.PageTitle.copy(color = PleyColors.Fg, textAlign = TextAlign.Center),
            modifier = Modifier.semantics { heading() },
        )
        Spacer(Modifier.height(12.dp))
        BasicText(body, style = PleyType.Tv.Body.copy(color = PleyColors.Muted, textAlign = TextAlign.Center), modifier = Modifier.widthIn(max = TextColumn))
        if (retry != null) {
            Spacer(Modifier.height(24.dp))
            TvPillButton(stringResource(R.string.tv_home_retry), onClick = retry, icon = PleyIcons.RefreshCw, modifier = Modifier.initialFocus())
        }
    }
    if (retry == null) {
        LaunchedEffect(Unit) {
            withFrameNanos { }
            rail?.focusCurrent()
        }
    }
}

/** A short message at the foot of the page, gone after 3.5 s (6 s for errors). */
@Composable
internal fun BoxScope.TvHomeToast(message: HomeMessage?, onShown: () -> Unit) {
    if (message == null) return
    val text = when (message) {
        HomeMessage.NothingToPlay -> stringResource(R.string.tv_home_nothing_to_play)
        is HomeMessage.Failed -> sentence(message.error)
    }
    LaunchedEffect(message) {
        delay(if (message is HomeMessage.Failed) ERROR_TOAST_MS else TOAST_MS)
        onShown()
    }
    Box(
        Modifier
            .align(Alignment.BottomStart)
            .padding(start = PleyTv.Gutter, bottom = TvSafeArea.Vertical)
            .background(PleyColors.Surface3, PleyShape.Control)
            .padding(horizontal = 20.dp, vertical = 10.dp)
            .semantics { liveRegion = LiveRegionMode.Polite },
    ) {
        BasicText(text, style = PleyType.Tv.Label.copy(color = PleyColors.Fg))
    }
}

/**
 * Tells the shells Home is drawn (StartupSignals.homeDrawn, which the
 * benchmarks' time to full display waits for): its first frame with
 * content, once the first row's first pictures are in (or after 2 s).
 */
@Composable
internal fun MarkHomeDrawn(model: TvHomeModel) {
    val context = LocalPlatformContext.current
    val loader = LocalArtworkImageLoader.current ?: SingletonImageLoader.get(context)
    val density = LocalDensity.current
    LaunchedEffect(Unit) {
        if (StartupSignals.homeDrawn.value) return@LaunchedEffect
        withFrameNanos { }
        val row = model.rows.firstOrNull()
        if (row != null) {
            val w = with(density) { row.kind.tvWidth.roundToPx() }
            val h = with(density) { row.kind.tvHeight.roundToPx() }
            withTimeoutOrNull(FIRST_ROW_WAIT_MS) {
                coroutineScope {
                    row.tiles.take(FIRST_ROW_TILES).mapNotNull { it.image }
                        .map { image -> async { loader.execute(ImageRequest.Builder(context).data(image).size(w, h).build()) } }
                        .awaitAll()
                }
            }
        }
        StartupSignals.markHomeDrawn()
    }
}

/** [error]'s sentence, capitalised, with a full stop, in the language shown. */
@Composable
@ReadOnlyComposable
internal fun sentence(error: Throwable): String = errorSentence(error).asString()

/** The text column over backdrops (design spec §1.3): x 96 to 560. */
internal val TextColumn = 464.dp

/**
 * The stack's top: the label over a title of one line starts 2 dp above
 * it, so its words are level with the dashes at the top right, their
 * middles at y = 39.
 */
private val HeroTop = 32.dp
private val TitleBox = 72.dp

/** The label's line and its gap to the title, and a line of the title. */
private val LabelLine = 30.dp
private val HeroTitleLine = 44.dp

/** How far above its box the label may sit: a title of one line under it rises just this far. */
private val TitleRise = 2.dp

/**
 * The room the title has over the backdrop's darkened side, x 96 to 736,
 * and the summary's, x 96 to 656: wider than the text column, as on a
 * title's page.
 */
private val HeroColumn = 640.dp
private val SummaryColumn = 560.dp

/**
 * The summary's three lines of BodyLarge. Taking turns it has
 * [SummaryRoom], a line more than they need: three lines under a title of
 * one line, two under a label and a title of two. A hero that stays
 * (Continue Watching's first) has up to [SummaryMost], what the hero's
 * height leaves short of the rows.
 */
private const val SUMMARY_LINES = 3
private val SummaryLine = 22.dp
private val SummaryRoom = SummaryLine * (SUMMARY_LINES + 1)
private val SummaryMost = 128.dp

/** An episode's line and the time left, each with the gap over it. */
private val ContextRoom = 32.dp
private val TimeLeftRoom = 27.dp

/**
 * The dashes' box: its top (the safe area's, its middle level with the
 * rail's picture) and height, and at each end the room for a chevron and
 * the gap from it to the dashes.
 */
private val PagerTop = TvSafeArea.Vertical
private val PagerHeight = 24.dp
private val PagerPadding = 6.dp
private val ChevronRoom = 12.dp

/** Half a chevron's height, and its whole width. */
private val ChevronSize = 4.dp

/** A dash: its height, its length (the one on, the rest) and the gap between two. */
private val DashHeight = 3.dp
private val DashLong = 28.dp
private val DashShort = 10.dp
private val DashGap = 6.dp

/** How white the dashes are: the one on and the track under them at rest, the track with focus (the one on is then full). */
private const val DASH_REST = 0.8f
private const val TRACK_REST = 0.3f
private const val TRACK_LIT = 0.5f
private val SummaryColor = PleyColors.Fg.copy(alpha = 0.85f)

private const val SKELETON_DELAY_MS = 300L
private const val TOAST_MS = 3_500L
private const val ERROR_TOAST_MS = 6_000L
private const val FIRST_ROW_WAIT_MS = 2_000L
private const val FIRST_ROW_TILES = 4
