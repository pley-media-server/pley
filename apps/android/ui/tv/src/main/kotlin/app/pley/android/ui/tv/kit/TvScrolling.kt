package app.pley.android.ui.tv.kit

import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.gestures.BringIntoViewSpec
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.runtime.Stable
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import app.pley.android.designsystem.PleyTv
import app.pley.android.designsystem.TvSafeArea

/**
 * Where scrolling puts what has focus (D-pad rule 4, with the design
 * spec's change, §2.4). Compose's own default depends on the leanback
 * feature, which Fire TV sticks may not declare, so the shell sets these
 * explicitly:
 *
 * - [root] at the TV's root: the focused element's leading edge at 0.3 of
 *   the viewport, Google's TV value, for grids and lists.
 * - [itself] inside each row: the row moves itself, its focused tile's
 *   left edge to the gutter (x = 96), so focus stays put and the row moves,
 *   as on Plex and Google TV ([TvRow]).
 * - [page] around a page of rows: the focused tile's top where its row's
 *   header lands at y = 172, just under the info block.
 * - [within] a row of chips: nothing moves while the focused chip shows
 *   between the gutter and the safe area's edge.
 *
 * Each only says where; a scroll that can't go that far stops at the end.
 */
@OptIn(ExperimentalFoundationApi::class)
object TvScrolling {
    /** The focused element's leading edge at [fraction] of the viewport. */
    fun root(fraction: Float = 0.3f): BringIntoViewSpec = FractionSpec(fraction)

    /**
     * For what the kit moves itself, to keep to its durations ([TvMotion]):
     * Compose's own scrolling to the focused element leaves it be.
     */
    val itself: BringIntoViewSpec = ItselfSpec

    /**
     * Leaves the focused element where it is while it shows between
     * [start] from the leading edge and [end] from the trailing one, and
     * otherwise brings it just inside: recent searches' chips, as the
     * native Samsung app moves them, only when one would pass the gutter
     * or the safe area's edge.
     */
    fun within(density: Density, start: Dp = PleyTv.Gutter, end: Dp = TvSafeArea.Horizontal): BringIntoViewSpec =
        with(density) { WithinSpec(start.toPx(), end.toPx()) }

    /** Whether every tile of [state]'s row fits on screen unscrolled, short of [endRoom]; null before its first layout. */
    fun fits(state: LazyListState, endRoom: Float): Boolean? {
        val info = state.layoutInfo
        val items = info.visibleItemsInfo
        if (info.totalItemsCount == 0 || items.isEmpty()) return null
        if (items.size < info.totalItemsCount || items.first().index != 0) return false
        val span = items.last().let { it.offset + it.size } - items.first().offset
        return span <= info.viewportEndOffset - endRoom
    }

    /**
     * The focused tile's top at [tileTop] from the page's top: by default
     * the row header's line (y = 172) plus the header and its gap to the
     * tiles.
     *
     * A page that starts with a hero ([heroHeight] tall, the first item of
     * [state]) scrolls back to its top whenever focus is in the hero, so
     * the hero's buttons sit where the page drew them.
     */
    fun page(
        density: Density,
        tileTop: Dp = PleyTv.FocusedRowHeaderY + PleyTv.RowHeaderHeight + PleyTv.HeaderToTiles,
        state: LazyListState? = null,
        heroHeight: Dp = 0.dp,
    ): BringIntoViewSpec = with(density) {
        if (state != null && heroHeight > 0.dp) {
            HeroPageSpec(tileTop.toPx(), heroHeight.toPx(), state)
        } else {
            AnchorSpec(tileTop.toPx())
        }
    }

    @Stable
    private class FractionSpec(private val fraction: Float) : BringIntoViewSpec {
        override fun calculateScrollDistance(offset: Float, size: Float, containerSize: Float): Float {
            // Google's pivot (developer.android.com/training/tv/playback/compose):
            // the leading edge at the fraction, unless the element would then
            // run past the end; then its trailing edge at the end.
            var target = containerSize * fraction
            if (size <= containerSize && target + size > containerSize) target = containerSize - size
            return offset - target
        }
    }

    @Stable
    private object ItselfSpec : BringIntoViewSpec {
        override fun calculateScrollDistance(offset: Float, size: Float, containerSize: Float): Float = 0f
    }

    @Stable
    private class WithinSpec(private val start: Float, private val end: Float) : BringIntoViewSpec {
        override fun calculateScrollDistance(offset: Float, size: Float, containerSize: Float): Float {
            val room = containerSize - end
            return when {
                offset < start -> offset - start
                // Past the edge: its end just inside it, unless it is too wide to fit, when its start.
                offset + size > room -> minOf(offset + size - room, offset - start)
                else -> 0f
            }
        }
    }

    @Stable
    private class AnchorSpec(private val anchor: Float) : BringIntoViewSpec {
        override fun calculateScrollDistance(offset: Float, size: Float, containerSize: Float): Float = offset - anchor
    }

    /**
     * [AnchorSpec] below a hero; inside the hero, back to the page's top.
     * Asked again on every frame of a scroll, so it follows the list as it moves.
     */
    @Stable
    private class HeroPageSpec(
        private val anchor: Float,
        private val hero: Float,
        private val state: LazyListState,
    ) : BringIntoViewSpec {
        override fun calculateScrollDistance(offset: Float, size: Float, containerSize: Float): Float {
            if (state.firstVisibleItemIndex == 0) {
                val scrolled = state.firstVisibleItemScrollOffset.toFloat()
                val inHero = offset + scrolled - state.layoutInfo.beforeContentPadding
                if (inHero + size <= hero) return -scrolled
            }
            return offset - anchor
        }
    }
}
