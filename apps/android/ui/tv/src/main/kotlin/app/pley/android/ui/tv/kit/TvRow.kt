@file:OptIn(ExperimentalFoundationApi::class)

package app.pley.android.ui.tv.kit

import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.focusGroup
import androidx.compose.foundation.gestures.BringIntoViewSpec
import androidx.compose.foundation.gestures.LocalBringIntoViewSpec
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.lazy.layout.LazyLayoutCacheWindow
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.runtime.withFrameNanos
import androidx.compose.runtime.snapshots.Snapshot
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.CacheDrawScope
import androidx.compose.ui.draw.DrawResult
import androidx.compose.ui.draw.drawWithCache
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.focus.FocusDirection
import androidx.compose.ui.focus.FocusEventModifierNode
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.FocusState
import androidx.compose.ui.focus.focusProperties
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.focusRestorer
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.drawscope.clipRect
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEvent
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.layout.LayoutCoordinates
import androidx.compose.ui.layout.layout
import androidx.compose.ui.layout.onPlaced
import androidx.compose.ui.node.CompositionLocalConsumerModifierNode
import androidx.compose.ui.node.ModifierNodeElement
import androidx.compose.ui.node.ObserverModifierNode
import androidx.compose.ui.node.currentValueOf
import androidx.compose.ui.node.observeReads
import androidx.compose.ui.platform.InspectorInfo
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyFocus
import app.pley.android.designsystem.image.GradientBitmaps
import app.pley.android.designsystem.image.drawStretched
import app.pley.android.designsystem.image.rememberImagePrefetcher
import app.pley.android.designsystem.PleyTv
import app.pley.android.designsystem.PleyType
import app.pley.android.designsystem.TvSafeArea
import app.pley.android.designsystem.reducedMotion
import app.pley.android.designsystem.tile.ArtworkTileModel
import app.pley.android.designsystem.tile.TileKind
import app.pley.android.ui.tv.theme.LocalTvTier
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.launch
import kotlin.math.abs
import kotlin.math.ceil
import kotlin.math.floor
import kotlin.math.roundToInt

/**
 * A row of tiles (design spec §2.4, D-pad rules 2 to 5): a header, then a
 * lazy row whose focused tile stays at the gutter (x = 96) while the row
 * moves under it.
 *
 * - The row is one focus group that comes back to the tile focused last
 *   (focusRestorer), the first time to [enterOn] when given; [memory]
 *   keeps that tile across leaving the page, so Back lands on it.
 * - Left from the first tile enters the rail, on a press of its own: a
 *   held Left stops there. Right stops at the end.
 * - Keys are the tiles' ids, and the content type their kind.
 * - The row moves itself ([RowMotion]): to a pressed tile over 150 ms,
 *   eased out. Under a held Left or Right from a tile at the gutter it
 *   slides on, a tile per held move, beneath a ring that stays still at the
 *   gutter, its tiles keeping their resting look ([TileQuiet]); when the
 *   key comes up focus lands on the tile under the ring and the row
 *   settles there.
 * - Its pictures load a screen and a half ahead of the scroll; under a
 *   held key they wait, and where it comes up loads them.
 * - A focused tile that leaves the row (Remove from Continue Watching)
 *   hands focus to the tile that takes its place; a row that goes with
 *   its last tile hands it to the row that takes its place ([TvRowColumn]).
 *
 * [rowKey] names the row on its page; in a [TvRowColumn] it is also the
 * row's item key. [initialFocus] makes the first tile the page's initial
 * focus. [seeAll] adds the "See all" tile at the end. [onFocus] hears
 * each tile that takes focus (an info block follows it once it rests,
 * [rememberRested]); [onActions] is the long press of Select.
 */
@Composable
fun TvRow(
    rowKey: String,
    title: String?,
    tiles: List<ArtworkTileModel>,
    kind: TileKind,
    onOpen: (ArtworkTileModel) -> Unit,
    modifier: Modifier = Modifier,
    onActions: ((ArtworkTileModel) -> Unit)? = null,
    onFocus: ((ArtworkTileModel) -> Unit)? = null,
    memory: FocusMemory? = null,
    initialFocus: Boolean = false,
    seeAll: SeeAll? = null,
    enterOn: Long? = null,
    state: LazyListState = rememberTvRowState(),
) {
    val density = LocalDensity.current
    val gate = LocalKeyRepeatGate.current
    val rail = LocalTvRailFocus.current
    val column = LocalTvRowColumn.current
    val reduced = reducedMotion()
    val scope = rememberCoroutineScope()
    val row = remember { RowFocus() }
    val motion = remember(state, gate) { RowMotion(state, gate, scope) }
    motion.measure(density, kind, tiles.size + if (seeAll != null) 1 else 0, reduced)
    // Pictures ahead of the scroll, not only as each tile composes: every page's rows, as on Samsung.
    val images = remember(tiles) { tiles.map { it.image } }
    rememberImagePrefetcher(
        state = state,
        images = images,
        widthPx = with(density) { kind.tvWidth.roundToPx() },
        heightPx = with(density) { kind.tvHeight.roundToPx() },
        tier = LocalTvTier.current,
    )
    val group = remember { FocusRequester() }
    if (column != null) {
        DisposableEffect(column, rowKey, motion) {
            column.rows[rowKey] = group
            column.motions[rowKey] = motion
            onDispose {
                if (column.rows[rowKey] === group) column.rows.remove(rowKey)
                if (column.motions[rowKey] === motion) column.motions.remove(rowKey)
                // Its focus is cleared by then: whether it had it is what the column last saw.
                if (column.focusedRow == rowKey && column.hadFocus) column.lost(rowKey)
            }
        }
    }
    // The focused tile left the row: the one now in its place takes focus.
    val heir = remember(tiles) { row.heirOf(tiles) }
    val heirFocus = remember { FocusRequester() }
    if (heir != null) {
        LaunchedEffect(heir) {
            withFrameNanos { }
            runCatching { heirFocus.requestFocus() }
        }
    }
    val enter = remember { FocusRequester() }
    Column(
        modifier
            .fillMaxWidth()
            .focusRequester(group)
            .onFocusChanged {
                row.hasFocus = it.hasFocus
                motion.hasFocus = it.hasFocus
                if (it.hasFocus) column?.focusedRow = rowKey else column?.left(rowKey)
            }
            .focusedRowOnTop(row)
            .focusGroup(),
    ) {
        if (title != null) {
            BasicText(
                title,
                style = PleyType.Tv.Section.copy(color = PleyColors.Fg),
                maxLines = 1,
                modifier = Modifier.padding(start = PleyTv.Gutter, end = TvSafeArea.Horizontal),
            )
            Spacer(Modifier.height(PleyTv.HeaderToTiles))
        }
        CompositionLocalProvider(LocalBringIntoViewSpec provides TvScrolling.itself, LocalTileQuiet provides motion.quiet) {
            LazyRow(
                state = state,
                modifier = Modifier
                    .fillMaxWidth()
                    .bleed(TileBleed)
                    .testTag("$ROW_TAG$rowKey")
                    .railOnLeftStopOnRight(rail)
                    .focusRestorer(if (enterOn != null) enter else FocusRequester.Default)
                    .onPreviewKeyEvent(motion::onKey)
                    .drawWithCache { motion.drawRing(this, kind) },
                contentPadding = PaddingValues(
                    start = PleyTv.Gutter,
                    end = TvSafeArea.Horizontal + kind.tvWidth,
                    top = TileBleed,
                    bottom = TileBleed,
                ),
                horizontalArrangement = Arrangement.spacedBy(kind.tvGap),
            ) {
                itemsIndexed(tiles, key = { _, tile -> tile.id }, contentType = { _, _ -> kind }) { index, tile ->
                    val first = initialFocus && index == 0
                    val remembered = if (memory != null) Modifier.rememberedFocus(memory, "$rowKey/${tile.id}", first) else Modifier.initialFocus(enabled = first)
                    TvTile(
                        model = tile,
                        kind = kind,
                        onClick = { onOpen(tile) },
                        onLongClick = onActions?.let { { it(tile) } },
                        modifier = Modifier.onFocusChanged {
                            if (it.hasFocus) {
                                row.focusedIndex = index
                                row.focusedId = tile.id
                                motion.focused(index)
                                column?.follow(rowKey, motion.slots.coordinates(index))
                                onFocus?.invoke(tile)
                            }
                        },
                        focusModifier = remembered
                            .focusSlot(motion.slots, index)
                            .then(if (tile.id == heir) Modifier.focusRequester(heirFocus) else Modifier)
                            .then(if (tile.id == enterOn) Modifier.focusRequester(enter) else Modifier),
                    )
                }
                if (seeAll != null) {
                    item(key = SEE_ALL_KEY, contentType = SEE_ALL_KEY) {
                        TvSeeAllTile(
                            kind = kind,
                            label = seeAll.label,
                            onClick = seeAll.onClick,
                            modifier = Modifier.onFocusChanged {
                                if (it.hasFocus) {
                                    row.focusedIndex = tiles.size
                                    motion.focused(tiles.size)
                                    column?.follow(rowKey, motion.slots.coordinates(tiles.size))
                                }
                            },
                            focusModifier = (if (memory != null) Modifier.rememberedFocus(memory, "$rowKey/$SEE_ALL_KEY") else Modifier)
                                .focusSlot(motion.slots, tiles.size),
                        )
                    }
                }
            }
        }
    }
}

/** A row's "See all" tile: its [label] and what it opens. */
@Stable
class SeeAll(val label: String, val onClick: () -> Unit)

/**
 * A row of [items] drawn by [tile] rather than as [TvTile]s (search's Live
 * TV channels), each [kind]'s size, without a header: it moves as a
 * [TvRow] does ([RowMotion]), its focused tile gliding to the gutter and a
 * held Left or Right sliding it under a still ring, and in a [TvRowColumn]
 * the page goes to it as to a row. [tile] puts the slot it is given on its
 * focusable part (a [TvTileFrame]'s focusModifier), after any focus
 * memory. [rowKey] is its item key in the column.
 */
@Composable
internal fun <T> TvTileRow(
    rowKey: String,
    items: List<T>,
    key: (T) -> Any,
    kind: TileKind,
    modifier: Modifier = Modifier,
    state: LazyListState = rememberTvRowState(),
    tile: @Composable (item: T, slot: Modifier) -> Unit,
) {
    val density = LocalDensity.current
    val gate = LocalKeyRepeatGate.current
    val rail = LocalTvRailFocus.current
    val column = LocalTvRowColumn.current
    val scope = rememberCoroutineScope()
    val motion = remember(state, gate) { RowMotion(state, gate, scope) }
    motion.measure(density, kind, items.size, reducedMotion())
    val group = remember { FocusRequester() }
    if (column != null) {
        DisposableEffect(column, rowKey, motion) {
            column.rows[rowKey] = group
            column.motions[rowKey] = motion
            onDispose {
                if (column.rows[rowKey] === group) column.rows.remove(rowKey)
                if (column.motions[rowKey] === motion) column.motions.remove(rowKey)
                // As a TvRow: a row that goes with focus in it hands it to the row now in its place.
                if (column.focusedRow == rowKey && column.hadFocus) column.lost(rowKey)
            }
        }
    }
    CompositionLocalProvider(LocalBringIntoViewSpec provides TvScrolling.itself, LocalTileQuiet provides motion.quiet) {
        LazyRow(
            state = state,
            modifier = modifier
                .fillMaxWidth()
                .focusRequester(group)
                .onFocusChanged {
                    motion.hasFocus = it.hasFocus
                    if (it.hasFocus) column?.focusedRow = rowKey else column?.left(rowKey)
                }
                .bleed(TileBleed)
                .railOnLeftStopOnRight(rail)
                .focusRestorer()
                .onPreviewKeyEvent(motion::onKey)
                .drawWithCache { motion.drawRing(this, kind) },
            contentPadding = PaddingValues(start = PleyTv.Gutter, end = TvSafeArea.Horizontal + kind.tvWidth, top = TileBleed, bottom = TileBleed),
            horizontalArrangement = Arrangement.spacedBy(kind.tvGap),
        ) {
            itemsIndexed(items, key = { _, item -> key(item) }, contentType = { _, _ -> kind }) { index, item ->
                val slot = Modifier
                    .onFocusChanged {
                        if (it.hasFocus) {
                            motion.focused(index)
                            column?.follow(rowKey, motion.slots.coordinates(index))
                        }
                    }
                    .focusSlot(motion.slots, index)
                tile(item, slot)
            }
        }
    }
}

/**
 * A row's scroll state, composing half a screen past its edge: the tiles a
 * slide brings in are made before they show, rather than in the frame they
 * do. [initialFirstVisibleItemIndex] is the tile it opens at.
 */
@Composable
fun rememberTvRowState(initialFirstVisibleItemIndex: Int = 0): LazyListState = rememberLazyListState(
    cacheWindow = remember { LazyLayoutCacheWindow(aheadFraction = ROW_AHEAD, behindFraction = 0f) },
    initialFirstVisibleItemIndex = initialFirstVisibleItemIndex,
)

/** How far past its edge a row composes, in screens. */
private const val ROW_AHEAD = 0.5f

/** The keys that act on the focused tile: Select (and Enter) opens it, Menu its actions. */
private val TileKeys = setOf(Key.DirectionCenter, Key.Enter, Key.NumPadEnter, Key.Menu)

/**
 * How a row moves (design spec §2.4, D-pad rules 4 and 5), as the native
 * Samsung app moves it. Focus on a tile brings the tile's left edge to the
 * gutter, or the row as far as it goes; a row whose tiles all fit stays
 * where it starts. A held Left or Right from a tile resting at the gutter
 * slides the row on ([slide]), and the key coming up lands focus where it
 * stopped ([land]). Its tiles all have the one width, so a tile's place is
 * its index times the pitch; sizes are in pixels, from [measure].
 */
@Stable
internal class RowMotion(val state: LazyListState, private val gate: KeyRepeatGate, private val scope: CoroutineScope) {
    val slots = FocusSlots()
    val quiet = TileQuiet()

    /** The tile that has focus, or had it last; -1 before any. */
    val focusedIndex: Int get() = focused

    /**
     * Whether focus is in the row: a slide that ends elsewhere lands without
     * taking it back. A key swallowed ([onKey]) whose key-up went elsewhere
     * is forgotten, so the row's next press of it acts.
     */
    var hasFocus = false
        set(value) {
            field = value
            if (!value) swallowed = null
        }

    /** Which way the row slides under a held key: 1, -1, or 0 when it doesn't. */
    private var sliding = 0
    private var focused = -1
    private val glide = TvGlide(state, scope)
    private var looks: Job? = null
    private val landing: () -> Unit = { land() }
    private var pitch = 1
    private var gap = 0
    private var start = 0
    private var end = 0
    private var safe = 0
    private var count = 0
    private var reduced = false

    /** The row's sizes: [count] tiles (See all with them) of [kind]. */
    fun measure(density: Density, kind: TileKind, count: Int, reduced: Boolean) {
        with(density) {
            gap = kind.tvGap.roundToPx()
            pitch = kind.tvWidth.roundToPx() + gap
            start = PleyTv.Gutter.roundToPx()
            end = (TvSafeArea.Horizontal + kind.tvWidth).roundToPx()
            safe = TvSafeArea.Horizontal.roundToPx()
        }
        this.count = count
        this.reduced = reduced
    }

    /** Tile [index] took focus: the row moves to it, unless a slide is moving it. */
    fun focused(index: Int) {
        focused = index
        if (sliding == 0) moveTo(restFor(index))
    }

    /** Select or Menu gone down while the row slid: its key-up, should it come after the landing, opens nothing either. */
    private var swallowed: Key? = null

    /**
     * A key the row's tiles get first. Under a held key, Left or Right from
     * a tile resting at the gutter, where the row has room that way, starts
     * a slide; while it runs, its keys go to it and focus waits. So does
     * Select or Menu pressed meanwhile, its key-up too when it comes after
     * the landing: focus is still on the tile the slide left behind, which
     * they would open, and Menu opens a tile's actions as it comes up.
     */
    fun onKey(event: KeyEvent): Boolean {
        if (event.key in TileKeys && (sliding != 0 || event.key == swallowed)) {
            swallowed = if (event.type == KeyEventType.KeyUp) null else event.key
            return true
        }
        if (event.type != KeyEventType.KeyDown) return false
        val step = when (event.key) {
            Key.DirectionLeft -> -1
            Key.DirectionRight -> 1
            else -> return false
        }
        if (sliding != 0) return true
        if (!gate.holding || reduced || focused < 0 || !canSlide(focused, step)) return false
        slide(step)
        return true
    }

    /** The ring that stays still at the gutter while the row slides; the tiles' wells' top is [TileBleed] down. */
    fun drawRing(cache: CacheDrawScope, kind: TileKind): DrawResult = with(cache) {
        val stroke = Stroke(PleyFocus.RingStroke.toPx())
        val at = Offset(PleyTv.Gutter.roundToPx().toFloat(), TileBleed.roundToPx().toFloat())
        onDrawWithContent {
            drawContent()
            if (quiet.on) drawStillRing(kind, at, stroke)
        }
    }

    /** How far the row has scrolled. */
    private fun scrolled(): Float = state.firstVisibleItemIndex * pitch.toFloat() + state.firstVisibleItemScrollOffset

    private fun width(): Int = state.layoutInfo.viewportSize.width

    /** The tiles' span, from the first's left edge to the last's right. */
    private fun content(): Int = count * pitch - gap

    /** Whether every tile fits before the safe area's edge unscrolled. */
    private fun fits(): Boolean = start + content() <= width() - safe

    /** The furthest the row scrolls: its last tile in from the edge, its end room after it. */
    private fun most(): Float = (start + content() + end - width()).coerceAtLeast(0).toFloat()

    /** Where the row rests with focus on tile [index]. */
    private fun restFor(index: Int): Float = if (fits()) 0f else minOf(index * pitch.toFloat(), most())

    /**
     * Whether the row can slide [step] with focus at [index]: the tile rests
     * at the gutter, where the slide's ring stays, and the row has room that
     * way. Near the row's end, where tiles don't come to the gutter, focus
     * steps across them instead, so the ring never leaps from tile to gutter.
     */
    private fun canSlide(index: Int, step: Int): Boolean {
        if (fits()) return false
        val rest = restFor(index)
        if (abs(rest - index * pitch) >= 0.5f) return false
        return if (step > 0) rest < most() - 0.5f else index > 0
    }

    private fun moveTo(target: Float) = glide.move(target - scrolled(), gate.holding, reduced)

    /**
     * The PSP's XMB way: the row slides to its end as one movement, a tile
     * per held move, under a ring that stays still at the gutter; the tiles
     * keep their resting look meanwhile, and focus waits where it was. The
     * key coming up, or the row's end, lands it ([land]).
     */
    private fun slide(step: Int) {
        sliding = step
        quiet.start()
        looks?.cancel()
        gate.onRelease(landing)
        val from = scrolled()
        val to = if (step > 0) most() else 0f
        val ms = (abs(to - from) / pitch * KeyRepeatGate.MOVE_INTERVAL_MS).roundToInt()
        // At the row's end focus steps on from where the tiles stop coming to the gutter.
        if (ms > 0) glide.slide(to - from, ms, landing) else land()
    }

    /**
     * The slide ends, as the key comes up or the row reaches its end: focus
     * lands on the tile under the ring, or the next one the way it was going,
     * so the row only ever settles forward; the row settles on that tile, and
     * the tiles take their looks back. Focus moves while they are still quiet,
     * so the tile it leaves shows nothing of it. The settling takes over from
     * the slide where it got to.
     */
    private fun land() {
        val step = sliding
        if (step == 0) return
        sliding = 0
        val under = scrolled() / pitch
        // The row's tiles may have changed meanwhile, even to none.
        val target = (if (step > 0) ceil(under - 0.01f) else floor(under + 0.01f)).toInt().coerceIn(0, (count - 1).coerceAtLeast(0))
        if (!hasFocus || target == focused || !slots.focus(target)) moveTo(restFor(target))
        looks = scope.launch { quiet.end(reduced) }
    }
}

/**
 * A page of rows (design spec §2.4, §4.1): a lazy column that brings the
 * focused row's header to y = 172 ([headerY]) and hides tiles scrolled
 * under the rail ([railFade]). Give each [TvRow] its rowKey as its item
 * key.
 *
 * The column moves itself to a row's tile as it takes focus, as the
 * native Samsung app's RowColumnView does: over 150 ms, eased out, for a
 * press; evenly over [KeyRepeatGate.GLIDE_MS] for a held Up or Down's step,
 * a little longer than the time between steps, so the glides run into
 * each other and the page moves on without stopping (D-pad rule 5). What
 * else isn't a row is scrolled to as Compose scrolls to focus.
 *
 * A page with a hero (Home, item pages) makes it the first item, marks it
 * ([tvRowColumnHero]) and says how tall it is ([heroHeight]): focus in the
 * hero takes the page back to its top, gliding as to a row, and Down from
 * the hero's buttons finds the first row, since both are in the one
 * column.
 *
 * Once the page has scrolled, nothing is drawn above the focused row's
 * header ([clearAbove]): the rows pass beneath the page's info block
 * (design spec §4.1), leaving the backdrop clear there.
 */
@Composable
fun TvRowColumn(
    modifier: Modifier = Modifier,
    state: LazyListState = rememberTvRowColumnState(),
    contentPadding: PaddingValues = PaddingValues(bottom = LastRowRoom),
    headerY: Dp = PleyTv.FocusedRowHeaderY,
    heroHeight: Dp = 0.dp,
    clearAbove: Boolean = true,
    content: LazyListScope.() -> Unit,
) {
    val density = LocalDensity.current
    val tileTop = headerY + PleyTv.RowHeaderHeight + PleyTv.HeaderToTiles
    val holder = rememberTvRowColumnFocus(state, with(density) { tileTop.toPx() })
    val spec = remember(density, tileTop, state, heroHeight, holder) { holder.spec(TvScrolling.page(density, tileTop, state, heroHeight)) }
    CompositionLocalProvider(LocalBringIntoViewSpec provides spec, LocalTvRowColumn provides holder) {
        LazyColumn(
            state = state,
            contentPadding = contentPadding,
            verticalArrangement = Arrangement.spacedBy(PleyTv.RowGap),
            modifier = modifier
                .railFade()
                .clearAboveHeader(state, headerY, clearAbove)
                .tvRowColumnFocus(holder)
                .heldKeyStaysIn(HeldEnds.UpDown)
                // Coming back from the rail: the row focus was in, then (its own restorer) the tile.
                // focusRestorer here would pick by composition key, which lazy rows share.
                .focusProperties {
                    onEnter = {
                        if (requestedFocusDirection == FocusDirection.Enter) {
                            holder.focusedRow?.let(holder.rows::get)?.let { runCatching { it.requestFocus() } }
                        }
                    }
                },
            content = content,
        )
    }
}

/**
 * A page of rows' scroll state, composing about a row of posters below the
 * screen: the next row and its pictures are on their way before focus
 * gets there, and a held Down glides onto rows already made.
 */
@Composable
fun rememberTvRowColumnState(): LazyListState =
    rememberLazyListState(cacheWindow = remember { LazyLayoutCacheWindow(ahead = ColumnAhead, behind = 0.dp) })

/** About one row of posters, with its header and caption. */
private val ColumnAhead = 300.dp

/**
 * How a lazy column of rows on [state] moves to its rows' tiles, which
 * come to [tileTop] (pixels from its top): [TvRowColumn]'s, and the item
 * page's, whose rows sit among sections of its own. The column provides
 * it as [LocalTvRowColumn], carries [tvRowColumnFocus], and scrolls with
 * [TvRowColumnFocus.spec].
 */
@Composable
internal fun rememberTvRowColumnFocus(state: LazyListState, tileTop: Float): TvRowColumnFocus {
    val scope = rememberCoroutineScope()
    val holder = remember { TvRowColumnFocus() }
    holder.state = state
    holder.scope = scope
    holder.glide = remember(state, scope) { TvGlide(state, scope) }
    holder.gate = LocalKeyRepeatGate.current
    holder.reduced = reducedMotion()
    holder.hadFocus = holder.hasFocus
    holder.tileTop = tileTop
    return holder
}

/** Tells [column] whether focus is in this column and where it is laid out, and gives it held Up and Down first. */
internal fun Modifier.tvRowColumnFocus(column: TvRowColumnFocus): Modifier =
    onFocusChanged { column.hasFocus = it.hasFocus }.onPlaced { column.coordinates = it }.onPreviewKeyEvent(column::onKey)

/**
 * The page's own spec for what isn't a row; nothing for a row's tiles or
 * the hero, which the column moves to itself ([TvRowColumnFocus.follow],
 * [TvRowColumnFocus.heroFocused]).
 */
@Stable
private class ColumnSpec(private val page: BringIntoViewSpec, private val column: TvRowColumnFocus) : BringIntoViewSpec {
    override fun calculateScrollDistance(offset: Float, size: Float, containerSize: Float): Float =
        if (column.following != null || column.inHero) 0f else page.calculateScrollDistance(offset, size, containerSize)
}

/**
 * Marks a [TvRowColumn]'s hero, its first item: focus coming into it
 * takes the page back to its top as the page goes to a row, over 150 ms,
 * eased out, as the native Samsung app's page does, where Compose's own
 * scrolling would spring there. A held Up stops on the first row
 * ([heldKeyStaysOut]); a press of its own comes into the hero. The hero is
 * a focus group, after this. Nothing outside a page of rows (a
 * [TvRowColumn], the item page's column).
 */
fun Modifier.tvRowColumnHero(): Modifier = heldKeyStaysOut() then TvRowColumnHeroElement

private data object TvRowColumnHeroElement : ModifierNodeElement<TvRowColumnHeroNode>() {
    override fun create() = TvRowColumnHeroNode()

    override fun update(node: TvRowColumnHeroNode) = Unit

    override fun InspectorInfo.inspectableProperties() {
        name = "tvRowColumnHero"
    }
}

private class TvRowColumnHeroNode :
    Modifier.Node(),
    FocusEventModifierNode,
    CompositionLocalConsumerModifierNode,
    ObserverModifierNode {
    private var hadFocus = false

    /** The column this hero is counted in ([TvRowColumnFocus.heroes]). */
    private var column: TvRowColumnFocus? = null

    override fun onAttach() = onObservedReadsChanged()

    override fun onObservedReadsChanged() = observeReads { countIn(currentValueOf(LocalTvRowColumn)) }

    override fun onDetach() = countIn(null)

    private fun countIn(next: TvRowColumnFocus?) {
        if (next === column) return
        column?.let { it.heroes-- }
        next?.let { it.heroes++ }
        column = next
    }

    override fun onFocusEvent(focusState: FocusState) {
        if (focusState.hasFocus == hadFocus) return
        hadFocus = focusState.hasFocus
        val column = currentValueOf(LocalTvRowColumn) ?: return
        if (hadFocus) column.heroFocused() else column.heroLeft()
    }
}

/**
 * Draws nothing above [headerY] (less a little room) once [state] has
 * scrolled from its top; at the top, everything (a hero) draws.
 */
private fun Modifier.clearAboveHeader(state: LazyListState, headerY: Dp, enabled: Boolean): Modifier =
    if (!enabled) {
        this
    } else {
        drawWithContent {
            val atTop = state.firstVisibleItemIndex == 0 && state.firstVisibleItemScrollOffset == 0
            if (atTop) {
                drawContent()
            } else {
                clipRect(top = (headerY - HeaderClearance).toPx()) { this@drawWithContent.drawContent() }
            }
        }
    }

/** Room kept above the focused row's header when the rows above it are cleared. */
private val HeaderClearance = 12.dp

/** Which row of a [TvRowColumn] has focus, by its key, and how the column moves to it. */
@Stable
internal class TvRowColumnFocus {
    var focusedRow: Any? = null

    /** Each row's focus group, by its key, while it is composed. */
    val rows = HashMap<Any, FocusRequester>()

    /** Each row's tiles and how it moves, by its key, while it is composed: where held Up and Down go ([onKey]). */
    val motions = HashMap<Any, RowMotion>()

    /** Whether focus is in the column. */
    var hasFocus = false

    /** Whether it was when the column last composed: before rows went. */
    var hadFocus = false
    var state: LazyListState? = null
    var scope: CoroutineScope? = null
    var gate: KeyRepeatGate? = null
    var reduced = false

    /** Where a focused tile's top comes, in pixels from the column's top: under its row's header at the line. */
    var tileTop = 0f

    /** Where a row's focused tile comes instead of [tileTop], by the row's key: the item page's episodes, under their tabs. */
    val tileTops = HashMap<Any, Float>()

    /** Where the column is laid out, for where its rows' tiles are. */
    var coordinates: LayoutCoordinates? = null

    /** The row whose tile has focus, which the column moves to itself; null while focus is elsewhere (a hero). */
    var following: Any? = null
        private set
    /** How the column moves, on [state]. */
    var glide: TvGlide? = null

    /** [page] for what the column doesn't move to itself: nothing for a row's tile or the hero ([follow], [heroFocused]). */
    fun spec(page: BringIntoViewSpec): BringIntoViewSpec = ColumnSpec(page, this)

    /**
     * A tile of row [rowKey] took focus, laid out at [tile]: the column
     * brings the tile's top to [tileTop] (or the row's own, [tileTops]).
     * Within the row the column goes on as it was; a tile not laid out yet
     * leaves it to Compose.
     */
    fun follow(rowKey: Any, tile: LayoutCoordinates?) {
        val glide = glide
        val column = coordinates
        if (glide == null || column == null || tile == null || !column.isAttached || !tile.isAttached) {
            following = null
            return
        }
        val same = following == rowKey
        following = rowKey
        if (same && glide.moving) return
        glide.move(column.localPositionOf(tile, Offset.Zero).y - (tileTops[rowKey] ?: tileTop), gate?.holding == true, reduced)
    }

    /** Focus left row [rowKey]: what takes it next, if not a row's tile or the hero, Compose scrolls to. */
    fun left(rowKey: Any) {
        if (following == rowKey) following = null
    }

    /**
     * A key the column gets before its rows: a held Up or Down from a row's
     * tile goes to the next row's tile nearest across ([heldStep]).
     */
    fun onKey(event: KeyEvent): Boolean {
        if (event.type != KeyEventType.KeyDown || gate?.holding != true) return false
        return when (event.key) {
            Key.DirectionDown -> heldStep(down = true)
            Key.DirectionUp -> heldStep(down = false)
            else -> false
        }
    }

    /**
     * A held step from the focused row's tile to the tile of the row next
     * to it, [down] or up, whose middle is nearest the tile's: where
     * Compose's own search goes, without it. That search lays the focused
     * row out again at every step, and the whole column at its ends, to
     * look past them for more (on a Google TV Streamer, a seventh of the
     * main thread's time under a held key). At the column's ends the key
     * stays, as it would ([heldKeyStaysIn]), and so it does below a hero
     * ([heldKeyStaysOut]). What else isn't a row's tile on either side (an
     * item page's section) is left to the search. Whether the key is used.
     */
    private fun heldStep(down: Boolean): Boolean {
        val list = state ?: return false
        val key = following ?: return false
        val from = motions[key] ?: return false
        val info = list.layoutInfo
        val items = info.visibleItemsInfo
        val at = items.indexOfFirst { it.key == key }
        if (at < 0) return false
        val nextIndex = items[at].index + if (down) 1 else -1
        if (nextIndex !in 0 until info.totalItemsCount || (nextIndex == 0 && heroes > 0)) return true
        val next = items.getOrNull(if (down) at + 1 else at - 1)?.takeIf { it.index == nextIndex } ?: return false
        val to = motions[next.key] ?: return false
        val tile = from.state.layoutInfo.visibleItemsInfo.firstOrNull { it.index == from.focusedIndex } ?: return false
        val middle = tile.offset + tile.size / 2
        val target = to.state.layoutInfo.visibleItemsInfo.minByOrNull { abs(it.offset + it.size / 2 - middle) } ?: return false
        return to.slots.focus(target.index)
    }

    /** Whether focus is in the hero ([tvRowColumnHero]), which the column takes back to its top itself. */
    var inHero = false
        private set

    /** How many heroes ([tvRowColumnHero]) are composed: the column's first item is one while this isn't 0. */
    var heroes = 0

    /**
     * Focus came into the hero: the column glides back to its top. From a
     * row the hero still shows, so how far that is is known; otherwise
     * Compose scrolls there.
     */
    fun heroFocused() {
        val list = state
        val glide = glide
        if (list == null || glide == null || list.firstVisibleItemIndex != 0) return
        inHero = true
        glide.move(-list.firstVisibleItemScrollOffset.toFloat(), gate?.holding == true, reduced)
    }

    /** Focus left the hero. */
    fun heroLeft() {
        inHero = false
    }

    /**
     * Row [rowKey] went with focus in it (its last tile left): once the
     * column has settled, the row now in its place takes focus, or the
     * one above when it was the last. Not when focus went somewhere else
     * meanwhile, nor once the column is gone (its scope with it).
     */
    fun lost(rowKey: Any) {
        val list = state ?: return
        val index = list.layoutInfo.visibleItemsInfo.firstOrNull { it.key == rowKey }?.index ?: return
        scope?.launch {
            withFrameNanos { }
            withFrameNanos { }
            if (hasFocus) return@launch
            val shown = list.layoutInfo.visibleItemsInfo.filter { it.key in rows }
            val next = shown.firstOrNull { it.index >= index } ?: shown.lastOrNull { it.index < index } ?: return@launch
            runCatching { rows[next.key]?.requestFocus() }
        }
    }
}

internal val LocalTvRowColumn = staticCompositionLocalOf<TvRowColumnFocus?> { null }

/** One row's focus: whether it has it (for drawing over its neighbours) and which tile. */
@Stable
private class RowFocus {
    var hasFocus by mutableStateOf(false)
    var focusedIndex = -1
    var focusedId: Long? = null
    private var tiles: List<ArtworkTileModel> = emptyList()

    /**
     * The row's tiles are now [next]: when the focused tile left them, the
     * tile now at its place (or the last) that takes focus from it.
     */
    fun heirOf(next: List<ArtworkTileModel>): Long? {
        val before = tiles
        tiles = next
        val focused = Snapshot.withoutReadObservation { hasFocus }
        val gone = focusedId?.takeIf { id -> focused && next.none { it.id == id } } ?: return null
        val at = before.indexOfFirst { it.id == gone }.takeIf { it >= 0 } ?: return null
        return (next.getOrNull(at) ?: next.lastOrNull())?.id
    }
}

/** The focused row draws over the rows around it, so its tile's ring and glow cross their edges (decided while placing). */
private fun Modifier.focusedRowOnTop(row: RowFocus): Modifier = layout { measurable, constraints ->
    val placeable = measurable.measure(constraints)
    layout(placeable.width, placeable.height) { placeable.place(0, 0, zIndex = if (row.hasFocus) 1f else 0f) }
}

/**
 * Hides what scrolls under the collapsed rail (design spec §2.4): [Bg][PleyColors.Bg]
 * up to x = 56, clear from x = 72, drawn once over the content, while
 * [shown] (read as it draws: a row that has scrolled). The focused tile's
 * ring sits right of it.
 */
fun Modifier.railFade(shown: () -> Boolean = { true }): Modifier = drawWithContent {
    drawContent()
    // A stretched picture rather than a gradient shader (GradientBitmaps): it is
    // drawn every frame, over the whole height.
    if (shown()) drawStretched(RailFade, Offset.Zero, Size(PleyTv.RailFadeClearFrom.toPx(), size.height))
}

/** Bg to x = 56 of its 72 dp, then clear at 72. */
private val RailFade by lazy {
    val solid = PleyTv.RailFadeSolidUntil.value / PleyTv.RailFadeClearFrom.value
    GradientBitmaps.horizontal(0f to PleyColors.Bg, solid to PleyColors.Bg, 1f to Color.Transparent)
}

/**
 * Lays this out [vertical] taller above and below than the room it takes,
 * and draws it that much higher: a lazy row clips 15 dp past its edges, and
 * a focused tile's growth, ring and glow reach up to 29 dp.
 */
internal fun Modifier.bleed(vertical: Dp): Modifier = layout { measurable, constraints ->
    val extra = vertical.roundToPx()
    val placeable = measurable.measure(
        constraints.copy(maxHeight = if (constraints.hasBoundedHeight) constraints.maxHeight + 2 * extra else constraints.maxHeight),
    )
    layout(placeable.width, (placeable.height - 2 * extra).coerceAtLeast(0)) { placeable.place(0, -extra) }
}

/** Room above and below a row's tiles for the focus look, beyond the 15 dp a lazy row draws past its edges. */
private val TileBleed = 16.dp

/**
 * Room under a page's last row, so it too can come up to the header line
 * (y = 172): the screen, less the line and the shortest row (landscape
 * tiles with their header and caption).
 */
val LastRowRoom: Dp = PleyTv.CanvasHeight - PleyTv.FocusedRowHeaderY -
    (PleyTv.RowHeaderHeight + PleyTv.HeaderToTiles + TileKind.Landscape.tvHeight + PleyTv.TilesToCaption + PleyTv.CaptionHeight)

private const val SEE_ALL_KEY = "pley.seeAll"

/** The test tag of the lazy row of [TvRow] `rowKey`: this, then the key. */
const val ROW_TAG = "pley.tv.row."
