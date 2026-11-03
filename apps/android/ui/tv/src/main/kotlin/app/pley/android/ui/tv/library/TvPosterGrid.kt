@file:OptIn(ExperimentalFoundationApi::class)

package app.pley.android.ui.tv.library

import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.focusGroup
import androidx.compose.foundation.gestures.BringIntoViewSpec
import androidx.compose.foundation.gestures.LocalBringIntoViewSpec
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.LazyGridScope
import androidx.compose.foundation.lazy.grid.LazyGridState
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.rememberLazyGridState
import androidx.compose.foundation.lazy.layout.LazyLayoutCacheWindow
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.CacheDrawScope
import androidx.compose.ui.draw.DrawResult
import androidx.compose.ui.draw.drawWithCache
import androidx.compose.ui.focus.focusRestorer
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.layout.LayoutCoordinates
import androidx.compose.ui.layout.onPlaced
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import app.pley.android.common.TestTags
import app.pley.android.designsystem.PleyFocus
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.PleyTv
import app.pley.android.designsystem.TvSafeArea
import app.pley.android.designsystem.image.ArtworkImage
import app.pley.android.designsystem.image.rememberImagePrefetcher
import app.pley.android.designsystem.reducedMotion
import app.pley.android.designsystem.tile.ArtworkTile
import app.pley.android.designsystem.tile.ArtworkTileModel
import app.pley.android.designsystem.tile.QualityBadge
import app.pley.android.designsystem.tile.SkeletonState
import app.pley.android.designsystem.tile.TileCaption
import app.pley.android.designsystem.tile.TileKind
import app.pley.android.designsystem.tile.TileStyle
import app.pley.android.designsystem.tile.skeleton
import app.pley.android.designsystem.tile.tvTileSize
import app.pley.android.ui.tv.kit.FocusMemory
import app.pley.android.ui.tv.kit.FocusSlots
import app.pley.android.ui.tv.kit.KeyRepeatGate
import app.pley.android.ui.tv.kit.LocalKeyRepeatGate
import app.pley.android.ui.tv.kit.LocalTileQuiet
import app.pley.android.ui.tv.kit.TileQuiet
import app.pley.android.ui.tv.kit.TvTileFrame
import app.pley.android.ui.tv.kit.drawStillRing
import app.pley.android.ui.tv.kit.focusSlot
import app.pley.android.ui.tv.kit.TvGlide
import app.pley.android.ui.tv.kit.rememberedFocus
import app.pley.android.ui.tv.theme.LocalTvTier
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.launch

/**
 * Where a TV grid's focus is, and each composed cell's focus, by position:
 * how a jump (A to Z, a letter) lands on a tile it scrolled to.
 */
@Stable
internal class TvGridFocus {
    /** The focused cell's position; -1 before any. Read by what follows it (the jump control's letter). */
    var focused by mutableIntStateOf(-1)

    internal val slots = FocusSlots()

    /** Focuses the cell at [position], if it is composed. */
    fun focus(position: Int): Boolean = slots.focus(position)
}

/**
 * How a TV grid moves (design spec §4.2, D-pad rules 4 and 5), as the
 * native Samsung app's GridView moves it. Focus on a cell brings its top
 * to [anchor] from the grid's top, or as far as the grid goes: over 150 ms,
 * eased out, for a press; evenly over [KeyRepeatGate.GLIDE_MS] for a held
 * key's step, so the steps run into each other. Under a held Up or Down
 * the rows glide under one ring that stays still where focus is going, the
 * tiles keeping their resting look ([TileQuiet]): a tile growing and
 * shrinking at each row, its ring with it, would make the page stutter.
 * The key coming up gives them their looks back.
 */
@Stable
internal class GridMotion(private val state: LazyGridState, private val gate: KeyRepeatGate, private val scope: CoroutineScope) {
    val quiet = TileQuiet()

    /** Where the grid is laid out, for where its cells are. */
    var coordinates: LayoutCoordinates? = null

    /** The focused cell's position while a cell has focus; -1 while focus is elsewhere (a header), which Compose scrolls to. */
    var inCell = -1
        private set

    var anchor = 0f
    var columns = 1
    var reduced = false

    /** The still ring's top left, in the grid: where the focused cell's well comes to rest. */
    private var ringX by mutableFloatStateOf(0f)
    private var ringY by mutableFloatStateOf(0f)
    private var row = -1
    private val glide = TvGlide(state, scope)
    private var looks: Job? = null
    private val landing: () -> Unit = { land() }

    /** The cell at [position] took focus, laid out at [cell]. */
    fun focused(position: Int, cell: LayoutCoordinates?) {
        inCell = position
        val newRow = position / columns != row
        row = position / columns
        val grid = coordinates
        if (grid == null || cell == null || !grid.isAttached || !cell.isAttached) return
        // Along a row the grid goes on as it was.
        if (!newRow && glide.moving) return
        val at = grid.localPositionOf(cell, Offset.Zero)
        var by = minOf(at.y - anchor, forwardRoom())
        if (by < 0f && state.firstVisibleItemIndex == 0) by = maxOf(by, -state.firstVisibleItemScrollOffset.toFloat())
        val held = gate.holding
        if (held && newRow && !reduced) {
            ringX = at.x
            ringY = at.y - by
            quieten()
        }
        glide.move(by, held, reduced)
    }

    /** The cell at [position] lost focus. */
    fun lost(position: Int) {
        if (inCell == position) inCell = -1
    }

    /** The ring that stays still while the rows glide under a held key. */
    fun drawRing(cache: CacheDrawScope, kind: TileKind): DrawResult = with(cache) {
        val stroke = Stroke(PleyFocus.RingStroke.toPx())
        onDrawWithContent {
            drawContent()
            if (quiet.on) drawStillRing(kind, Offset(ringX, ringY), stroke)
        }
    }

    /** How much further the grid scrolls forward: up to its end, once its last row is laid out. */
    private fun forwardRoom(): Float {
        val info = state.layoutInfo
        val items = info.visibleItemsInfo
        if (items.isEmpty() || items.last().index < info.totalItemsCount - 1) return Float.MAX_VALUE
        val end = items.maxOf { it.offset.y + it.size.height } + info.afterContentPadding
        return (end - info.viewportEndOffset).coerceAtLeast(0).toFloat()
    }

    private fun quieten() {
        if (quiet.on) return
        looks?.cancel()
        quiet.start()
        gate.onRelease(landing)
    }

    /** The key came up: the tiles take their looks back as the still ring goes. */
    private fun land() {
        looks?.cancel()
        looks = scope.launch { quiet.end(reduced) }
    }
}

/** The grid's own spec for what isn't a cell (a page's header); nothing for a cell, which the grid moves to itself. */
@Stable
private class CellSpec(private val page: BringIntoViewSpec, private val motion: GridMotion) : BringIntoViewSpec {
    override fun calculateScrollDistance(offset: Float, size: Float, containerSize: Float): Float =
        if (motion.inCell >= 0) 0f else page.calculateScrollDistance(offset, size, containerSize)
}

/**
 * A TV grid of tiles (design spec §4.2): [columns] wells of [kind] from
 * the gutter (x = 96), 16 dp apart, 22 dp between a caption and the next
 * row. It holds [total] cells; [tileAt] is `null` for one not loaded yet,
 * drawn as a still skeleton that takes focus like a tile, so the D-pad
 * runs through a big library's unloaded stretches too.
 *
 * - Scrolling keeps the focused row's top [contentTop] below the grid's
 *   top, so the next row always peeks in; focus in the [header] (a page's
 *   top) scrolls back to the start.
 * - The grid moves itself ([GridMotion]): over 150 ms, eased out, for a
 *   press; under a held Up or Down it glides a row per step, the tiles at
 *   rest under one still ring (rule 5).
 * - It is one focus group that comes back to the tile it had; each tile
 *   is remembered in [memory] (as `"<memoryPrefix>/<id>"`), so Back from
 *   a title lands on it.
 * - Right from the last column calls [onRight] (the jump control). A
 *   held Right stops there, and a held Up at the first row: the jump, and
 *   the chips or a page's header above, take a press of their own.
 * - Pictures load about a row ahead; [badge] puts a count on a tile's
 *   top right (collections).
 */
@Composable
internal fun TvPosterGrid(
    total: Int,
    tileAt: (Int) -> ArtworkTileModel?,
    keyAt: (Int) -> Any,
    kind: TileKind,
    onOpen: (Int, ArtworkTileModel) -> Unit,
    memory: FocusMemory,
    memoryPrefix: String,
    skeleton: SkeletonState,
    modifier: Modifier = Modifier,
    state: LazyGridState = rememberTvGridState(),
    focus: TvGridFocus = remember { TvGridFocus() },
    onActions: ((ArtworkTileModel) -> Unit)? = null,
    initialFocus: Boolean = true,
    columns: Int = tvGridColumns(kind),
    contentTop: Dp = TileRoom,
    heroHeight: Dp = 0.dp,
    header: (LazyGridScope.() -> Unit)? = null,
    badge: ((Int) -> String?)? = null,
    onRight: (() -> Boolean)? = null,
) {
    val density = LocalDensity.current
    val gate = LocalKeyRepeatGate.current
    val scope = rememberCoroutineScope()
    val motion = remember(state, gate) { GridMotion(state, gate, scope) }
    motion.anchor = with(density) { contentTop.toPx() }
    motion.columns = columns
    motion.reduced = reducedMotion()
    val spec = remember(density, contentTop, heroHeight, state, motion) {
        CellSpec(GridSpec(with(density) { contentTop.toPx() }, with(density) { heroHeight.toPx() }, state), motion)
    }
    val images = remember(total, tileAt) { TvImages(total, tileAt) }
    rememberImagePrefetcher(
        state = state,
        images = images,
        widthPx = with(density) { kind.tvWidth.roundToPx() },
        heightPx = with(density) { kind.tvHeight.roundToPx() },
        tier = LocalTvTier.current,
    )
    CompositionLocalProvider(LocalBringIntoViewSpec provides spec, LocalTileQuiet provides motion.quiet) {
        LazyVerticalGrid(
            columns = GridCells.Fixed(columns),
            state = state,
            modifier = modifier
                .testTag(TestTags.LIBRARY_GRID)
                .focusRestorer()
                .focusGroup()
                .onPlaced { motion.coordinates = it }
                .onPreviewKeyEvent { event ->
                    if (event.type != KeyEventType.KeyDown) return@onPreviewKeyEvent false
                    val from = motion.inCell
                    if (from < 0) return@onPreviewKeyEvent false
                    when (event.key) {
                        // A held key stays in the grid (rule 5): the A to Z jump and what is above take a press of their own.
                        Key.DirectionRight -> (from % columns == columns - 1 || from == total - 1) && (gate.holding || onRight?.invoke() == true)
                        Key.DirectionUp -> gate.holding && from < columns
                        else -> false
                    }
                }
                .drawWithCache { motion.drawRing(this, kind) },
            contentPadding = PaddingValues(
                start = PleyTv.Gutter,
                end = PleyTv.CanvasWidth - PleyTv.Gutter - (kind.tvWidth + PleyTv.TileGap) * columns + PleyTv.TileGap,
                top = if (header != null) 0.dp else contentTop,
                bottom = TvSafeArea.Vertical + TileRoom,
            ),
            horizontalArrangement = Arrangement.spacedBy(PleyTv.TileGap),
            verticalArrangement = Arrangement.spacedBy(RowGap),
        ) {
            header?.invoke(this)
            // One cell per position, whether its page has loaded or not: the same
            // frame turns from a skeleton into the tile, so a focused cell keeps
            // its focus (and the grid its scroll anchor) when its page arrives.
            items(count = total, key = keyAt, contentType = { kind }) { position ->
                val tile = tileAt(position)
                GridCell(
                    tile = tile,
                    kind = kind,
                    skeleton = skeleton,
                    badge = if (tile != null) badge?.invoke(position) else null,
                    onClick = { if (tile != null) onOpen(position, tile) },
                    onLongClick = if (tile != null) onActions?.let { { it(tile) } } else null,
                    modifier = Modifier.onFocusChanged {
                        if (it.hasFocus) {
                            focus.focused = position
                            motion.focused(position, focus.slots.coordinates(position))
                        } else {
                            motion.lost(position)
                        }
                    },
                    focusModifier = Modifier
                        .focusSlot(focus.slots, position)
                        .rememberedFocus(memory, "$memoryPrefix/${tile?.id ?: "cell$position"}", initial = initialFocus && position == 0),
                )
            }
        }
    }
}

/** A grid's scroll state, composing the next row or two ahead of focus (a screen ahead). */
@Composable
internal fun rememberTvGridState(): LazyGridState =
    rememberLazyGridState(cacheWindow = remember { LazyLayoutCacheWindow(aheadFraction = 1f, behindFraction = 0.25f) })

/**
 * A grid's cell: [tile], with [badge] (a count) on its art's top right
 * when given; while [tile] is `null` (its page not loaded yet), a still
 * well and two caption bars, focusable like the tile it stands for.
 */
@Composable
private fun GridCell(
    tile: ArtworkTileModel?,
    kind: TileKind,
    skeleton: SkeletonState,
    badge: String?,
    onClick: () -> Unit,
    onLongClick: (() -> Unit)?,
    modifier: Modifier,
    focusModifier: Modifier,
) {
    TvTileFrame(
        kind = kind,
        onClick = onClick,
        onLongClick = onLongClick,
        modifier = modifier,
        focusModifier = focusModifier,
        caption = { focused ->
            if (tile != null) {
                TileCaption(tile.title, tile.subtitle, TileStyle.Tv, emphasized = focused)
            } else {
                Column(Modifier.fillMaxWidth().clearAndSetSemantics { }) {
                    Box(Modifier.padding(top = 3.dp).fillMaxWidth(0.6f).height(12.dp).skeleton(skeleton, PleyShape.Badge))
                    Box(Modifier.padding(top = 8.dp).fillMaxWidth(0.35f).height(12.dp).skeleton(skeleton, PleyShape.Badge))
                }
            }
        },
    ) {
        if (tile == null) {
            Box(Modifier.tvTileSize(kind).skeleton(skeleton, TileStyle.Tv.shape(kind)))
        } else {
            Box(Modifier.tvTileSize(kind)) {
                ArtworkTile(tile, kind, TileStyle.Tv, Modifier.tvTileSize(kind))
                if (badge != null) {
                    QualityBadge(
                        badge,
                        TileStyle.Tv,
                        onArt = true,
                        modifier = Modifier.align(Alignment.TopEnd).padding(TileStyle.Tv.inset).clearAndSetSemantics { },
                    )
                }
            }
        }
    }
}

/** A grid's pictures by position, for the prefetcher. */
private class TvImages(private val total: Int, private val tileAt: (Int) -> ArtworkTileModel?) : AbstractList<ArtworkImage?>() {
    override val size: Int get() = total

    override fun get(index: Int): ArtworkImage? = tileAt(index)?.image
}

/**
 * Keeps the focused row's top [anchor] from the grid's top; while focus
 * is in a header [hero] tall (the grid's first item), the grid's start.
 */
@Stable
private class GridSpec(private val anchor: Float, private val hero: Float, private val state: LazyGridState) : BringIntoViewSpec {
    override fun calculateScrollDistance(offset: Float, size: Float, containerSize: Float): Float {
        if (hero > 0f && state.firstVisibleItemIndex == 0) {
            val scrolled = state.firstVisibleItemScrollOffset.toFloat()
            val inHero = offset + scrolled - state.layoutInfo.beforeContentPadding
            if (inHero + size <= hero) return -scrolled
        }
        return offset - anchor
    }
}

/** Between a caption and the next row's wells. */
internal val RowGap = 20.dp

/** Room above the first row for the focused tile's growth and ring. */
internal val TileRoom = 14.dp

