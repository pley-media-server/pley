package app.pley.android.ui.tv.home

import androidx.compose.foundation.background
import androidx.compose.foundation.focusGroup
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.wrapContentHeight
import androidx.compose.foundation.shape.CircleShape
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
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.focusRestorer
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.pley.android.common.errorSentence
import app.pley.android.common.perf.StartupSignals
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.PleyTv
import app.pley.android.designsystem.PleyType
import app.pley.android.designsystem.TvSafeArea
import app.pley.android.designsystem.icon.Icon
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.designsystem.image.LocalArtworkImageLoader
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
import coil3.SingletonImageLoader
import coil3.compose.LocalPlatformContext
import coil3.request.ImageRequest
import kotlinx.coroutines.async
import kotlinx.coroutines.awaitAll
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.withTimeoutOrNull

/**
 * The hero's stack at x = 96 (design spec §2.2): for a featured title why
 * it is there ([label], "New in Movies"), the title (logo art once the
 * server has it), an episode's "S2 · E5 · The Long Night", the metadata
 * line, progress, three lines of summary, then Resume (or Play) and the
 * round buttons From start, Info, and My List for movies and shows (Watched
 * for the rest). A missing piece closes its gap. Resume ([resume]) is the
 * page's initial focus.
 *
 * With several titles ([turns]), dashes beside the buttons show which is
 * on, and Right from the last button goes on to the next ([onNext]).
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
    onNext: (() -> Unit)? = null,
) {
    val column = LocalTvRowColumn.current
    val group = remember { FocusRequester() }
    if (column != null) {
        DisposableEffect(column) {
            column.rows[HERO_KEY] = group
            onDispose { if (column.rows[HERO_KEY] === group) column.rows.remove(HERO_KEY) }
        }
    }
    Column(
        modifier
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
        Box(
            Modifier.width(TextColumn).height(TitleBox).wrapContentHeight(Alignment.Bottom, unbounded = true),
            contentAlignment = Alignment.BottomStart,
        ) {
            Column {
                if (label != null) {
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        Box(Modifier.size(width = 4.dp, height = 16.dp).background(PleyColors.Accent, CircleShape))
                        Spacer(Modifier.width(10.dp))
                        BasicText(
                            label.uppercase(),
                            style = PleyType.Tv.Label.copy(color = PleyColors.Fg.copy(alpha = 0.85f), letterSpacing = 3.sp),
                            maxLines = 1,
                        )
                    }
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
            // Taking turns, the summary keeps its three lines' room: the buttons stay where they are.
            BasicText(
                hero.summary,
                style = PleyType.Tv.BodyLarge.copy(color = SummaryColor),
                maxLines = 3,
                minLines = if (turns > 1) 3 else 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.width(TextColumn),
            )
        }
        Spacer(Modifier.height(20.dp))
        Row(horizontalArrangement = Arrangement.spacedBy(PleyTv.ButtonGap)) {
            TvPillButton(
                label = stringResource(if (hero.inProgress) R.string.tv_home_resume else R.string.tv_home_play),
                onClick = { onPlay(false) },
                icon = PleyIcons.Play,
                modifier = Modifier.focusRequester(resume).initialFocus(memory),
            )
            if (hero.inProgress) {
                TvRoundButton(stringResource(R.string.tv_home_from_start), PleyIcons.RotateCcw, onClick = { onPlay(true) })
            }
            TvRoundButton(stringResource(R.string.tv_home_info), PleyIcons.Info, onClick = onInfo)
            // Right from the last button: the next title, as along a row.
            val last = if (onNext == null) {
                Modifier
            } else {
                Modifier.onPreviewKeyEvent {
                    if (it.type == KeyEventType.KeyDown && it.key == Key.DirectionRight) {
                        onNext()
                        true
                    } else {
                        false
                    }
                }
            }
            if (hero.listable) {
                TvRoundButton(
                    stringResource(if (hero.inList) R.string.tv_item_remove_from_my_list else R.string.tv_item_add_to_my_list),
                    if (hero.inList) PleyIcons.Check else PleyIcons.Plus,
                    onClick = onList,
                    modifier = last,
                )
            } else {
                TvRoundButton(
                    stringResource(if (hero.watched) R.string.tv_home_mark_unwatched else R.string.tv_home_mark_watched),
                    PleyIcons.CircleCheck,
                    onClick = onWatched,
                    modifier = last,
                )
            }
            if (turns > 1) {
                Spacer(Modifier.width(24.dp))
                HeroTurns(turns, turn, Modifier.align(Alignment.CenterVertically))
            }
        }
    }
}

/** The hero's dashes: one per title, the one on longer and white. Not focusable: Right and the clock turn them. */
@Composable
private fun HeroTurns(count: Int, current: Int, modifier: Modifier = Modifier) {
    Row(modifier.clearAndSetSemantics { }, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        repeat(count) { i ->
            Box(
                Modifier
                    .size(width = if (i == current) 36.dp else 14.dp, height = 4.dp)
                    .background(if (i == current) PleyColors.Fg else PleyColors.Fg.copy(alpha = 0.3f), CircleShape),
            )
        }
    }
}

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

private val HeroTop = 52.dp
private val TitleBox = 72.dp
private val SummaryColor = PleyColors.Fg.copy(alpha = 0.85f)

private const val SKELETON_DELAY_MS = 300L
private const val TOAST_MS = 3_500L
private const val ERROR_TOAST_MS = 6_000L
private const val FIRST_ROW_WAIT_MS = 2_000L
private const val FIRST_ROW_TILES = 4
