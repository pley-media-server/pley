@file:OptIn(ExperimentalFoundationApi::class)

package app.pley.android.ui.tv.library

import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.focusGroup
import androidx.compose.foundation.gestures.BringIntoViewSpec
import androidx.compose.foundation.gestures.LocalBringIntoViewSpec
import androidx.compose.foundation.gestures.scrollBy
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
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusDirection
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
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import app.pley.android.common.TestTags
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.PleyTv
import app.pley.android.designsystem.TvSafeArea
import app.pley.android.designsystem.image.ArtworkImage
import app.pley.android.designsystem.image.rememberImagePrefetcher
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
import app.pley.android.ui.tv.kit.LocalKeyRepeatGate
import app.pley.android.ui.tv.kit.TvTileFrame
import app.pley.android.ui.tv.kit.rememberedFocus
import app.pley.android.ui.tv.theme.LocalTvTier
import kotlinx.coroutines.launch

/**
 * Where a TV grid's focus is, and each composed cell's focus, by position:
 * how a jump (A to Z, a letter) lands on a tile it scrolled to.
 */
@Stable
internal class TvGridFocus {
    /** The focused cell's position; -1 before any. Read by what follows it (the jump control's letter). */
    var focused by mutableIntStateOf(-1)

    internal val cells = HashMap<Int, FocusRequester>()

    /** Focuses the cell at [position], if it is composed. */
    fun focus(position: Int): Boolean = cells[position]?.let { runCatching { it.requestFocus() }.isSuccess } ?: false
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
 * - It is one focus group that comes back to the tile it had; each tile
 *   is remembered in [memory] (as `"<memoryPrefix>/<id>"`), so Back from
 *   a title lands on it.
 * - While a key repeats, Up and Down move a row at a time without
 *   animating (rule 5).
 * - Right from the last column calls [onRight] (the jump control).
 * - Pictures load about a row ahead; [badge] puts a count on a tile's
 *   top right (collections).
 * - [footer] adds cells after the [total] (a grid's Show more).
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
    footer: (LazyGridScope.() -> Unit)? = null,
) {
    val density = LocalDensity.current
    val gate = LocalKeyRepeatGate.current
    val focusManager = LocalFocusManager.current
    val scope = rememberCoroutineScope()
    val spec = remember(density, contentTop, heroHeight, state) {
        GridSpec(with(density) { contentTop.toPx() }, with(density) { heroHeight.toPx() }, state)
    }
    val pitch = with(density) { (kind.tvHeight + PleyTv.TilesToCaption + PleyTv.CaptionHeight + RowGap).toPx() }
    val images = remember(total, tileAt) { TvImages(total, tileAt) }
    rememberImagePrefetcher(
        state = state,
        images = images,
        widthPx = with(density) { kind.tvWidth.roundToPx() },
        heightPx = with(density) { kind.tvHeight.roundToPx() },
        tier = LocalTvTier.current,
    )
    CompositionLocalProvider(LocalBringIntoViewSpec provides spec) {
        LazyVerticalGrid(
            columns = GridCells.Fixed(columns),
            state = state,
            modifier = modifier
                .testTag(TestTags.LIBRARY_GRID)
                .focusRestorer()
                .focusGroup()
                .onPreviewKeyEvent { event ->
                    if (event.type != KeyEventType.KeyDown) return@onPreviewKeyEvent false
                    val from = focus.focused
                    if (from < 0) return@onPreviewKeyEvent false
                    when (event.key) {
                        Key.DirectionRight -> {
                            val last = from % columns == columns - 1 || from == total - 1
                            last && onRight?.invoke() == true
                        }
                        Key.DirectionUp, Key.DirectionDown -> {
                            if (!gate.holding) return@onPreviewKeyEvent false
                            val down = event.key == Key.DirectionDown
                            val target = from + if (down) columns else -columns
                            if (target !in 0 until total) return@onPreviewKeyEvent false
                            scope.launch {
                                state.scrollBy(if (down) pitch else -pitch)
                                withFrameNanos { }
                                focusManager.moveFocus(if (down) FocusDirection.Down else FocusDirection.Up)
                            }
                            true
                        }
                        else -> false
                    }
                },
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
                val requester = remember { FocusRequester() }
                DisposableEffect(position, requester) {
                    focus.cells[position] = requester
                    onDispose { if (focus.cells[position] === requester) focus.cells.remove(position) }
                }
                val tile = tileAt(position)
                GridCell(
                    tile = tile,
                    kind = kind,
                    skeleton = skeleton,
                    badge = if (tile != null) badge?.invoke(position) else null,
                    onClick = { if (tile != null) onOpen(position, tile) },
                    onLongClick = if (tile != null) onActions?.let { { it(tile) } } else null,
                    modifier = Modifier.onFocusChanged { if (it.hasFocus) focus.focused = position },
                    focusModifier = Modifier
                        .focusRequester(requester)
                        .rememberedFocus(memory, "$memoryPrefix/${tile?.id ?: "cell$position"}", initial = initialFocus && position == 0),
                )
            }
            footer?.invoke(this)
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

