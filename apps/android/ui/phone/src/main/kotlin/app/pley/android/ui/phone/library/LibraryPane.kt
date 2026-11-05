package app.pley.android.ui.phone.library

import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.rememberLazyGridState
import androidx.compose.foundation.lazy.layout.LazyLayoutCacheWindow
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.rememberScrollState
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import app.pley.android.common.TestTags
import app.pley.android.data.GridDensity
import app.pley.android.data.Loadable
import app.pley.android.data.valueOrNull
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyPhone
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.designsystem.image.ArtworkImage
import app.pley.android.designsystem.image.rememberImagePrefetcher
import app.pley.android.designsystem.text.asString
import app.pley.android.designsystem.tile.ArtworkTileModel
import app.pley.android.designsystem.tile.SkeletonState
import app.pley.android.designsystem.tile.SkeletonTile
import app.pley.android.designsystem.tile.TileKind
import app.pley.android.designsystem.tile.TileStyle
import app.pley.android.model.LibraryKind
import app.pley.android.screens.PleyKey
import app.pley.android.screens.library.GridCells as Cells
import app.pley.android.screens.library.LibraryUiState
import app.pley.android.screens.library.LibraryViewModel
import app.pley.android.screens.library.TileLook
import app.pley.android.screens.library.labelText
import app.pley.android.ui.phone.R
import app.pley.android.ui.phone.kit.EmptyState
import app.pley.android.ui.phone.kit.ErrorState
import app.pley.android.ui.phone.kit.LocalPhoneDeviceTier
import app.pley.android.ui.phone.kit.MediaCard
import app.pley.android.ui.phone.kit.PleyChip
import app.pley.android.ui.phone.kit.TonalButton
import app.pley.android.ui.phone.shell.LocalPhoneNavigator
import app.pley.android.ui.phone.theme.LocalPhoneMetrics
import kotlinx.coroutines.flow.distinctUntilChanged

/**
 * The Library tab (design spec §4.2): a toolbar of chips that stays put
 * (sort, the watched filters, genre, year, resolution and dynamic range
 * where the library has them to choose, then Clear filters and Save as…
 * while filtered), and under it the grid of every title, sized from the
 * listing's total so the scroll bar is right from the first frame. Cells
 * not loaded yet are skeletons in the same shape. On the right edge, a
 * fast-scroll handle shows the letter it is at when sorted by title.
 */
@Composable
internal fun LibraryPane(state: LibraryUiState<ArtworkTileModel>, vm: LibraryViewModel<ArtworkTileModel>, skeleton: SkeletonState) {
    var sheet by rememberSaveable { mutableStateOf<LibrarySheet?>(null) }
    val grid = state.grid
    val cells = grid.cells.valueOrNull
    Column(Modifier.fillMaxSize()) {
        Toolbar(state, vm, cells?.total, onSheet = { sheet = it })
        Box(Modifier.weight(1f).fillMaxWidth()) {
            val failed = grid.cells as? Loadable.Failed
            when {
                failed != null && cells == null ->
                    Scrolling { ErrorState(stringResource(R.string.library_failed), failed.error, onRetry = vm::retry) }
                cells != null && cells.total == 0 -> Scrolling {
                    if (state.filtered) {
                        EmptyState(
                            title = stringResource(R.string.library_no_matches_title),
                            body = stringResource(R.string.library_no_matches_body),
                            action = { TonalButton(stringResource(R.string.library_clear_filters), onClick = vm::clearFilters) },
                        )
                    } else {
                        EmptyState(
                            title = stringResource(R.string.library_empty_title),
                            body = stringResource(R.string.library_empty_body),
                            icon = PleyIcons.FolderSearch,
                        )
                    }
                }
                else -> LibraryGrid(state, cells, vm, skeleton)
            }
        }
    }
    sheet?.let { open ->
        LibrarySheetHost(open, state, vm, onDismiss = { sheet = null })
    }
}

/** A short page that still scrolls (and so takes the bar's scroll states), for empty and failed tabs. */
@Composable
internal fun Scrolling(content: @Composable () -> Unit) {
    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()), horizontalAlignment = Alignment.CenterHorizontally) {
        content()
    }
}

/**
 * The chips over the grid, and the count of what matches: at the end of
 * the chips, or filtered ("23 of 342 movies") on a line of its own under
 * them, which then run past the screen's edge.
 */
@Composable
private fun Toolbar(state: LibraryUiState<ArtworkTileModel>, vm: LibraryViewModel<ArtworkTileModel>, total: Int?, onSheet: (LibrarySheet) -> Unit) {
    val metrics = LocalPhoneMetrics.current
    val params = state.params
    val music = state.kind == LibraryKind.Music
    val resources = LocalResources.current
    val counted = total?.let { n ->
        val of = state.library?.itemCount?.takeIf { params.filtered && it >= n }
        if (of != null) countOf(resources, itemsOfPlural(state.kind), n, of) else count(resources, itemsPlural(state.kind), n)
    }
    Column {
        LazyRow(
            modifier = Modifier.fillMaxWidth(),
            contentPadding = PaddingValues(start = metrics.marginStart, end = metrics.marginEnd, top = 8.dp, bottom = 8.dp),
            horizontalArrangement = Arrangement.spacedBy(8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            item(key = "sort") {
                PleyChip(
                    label = stringResource(sortLabel(params.sort)),
                    selected = false,
                    onClick = { onSheet(LibrarySheet.Sort) },
                    icon = if (params.effectiveOrder == app.pley.android.model.SortOrder.Ascending) PleyIcons.ArrowUpNarrowWide else PleyIcons.ArrowDownWideNarrow,
                )
            }
            if (!music) {
                item(key = "unwatched") {
                    PleyChip(
                        label = stringResource(R.string.library_unwatched),
                        selected = params.watched == false,
                        onClick = { vm.watched(if (params.watched == false) null else false) },
                    )
                }
                item(key = "watched") {
                    PleyChip(
                        label = stringResource(R.string.library_watched),
                        selected = params.watched == true,
                        onClick = { vm.watched(if (params.watched == true) null else true) },
                    )
                }
                if (state.filters.genres.isNotEmpty() || params.genre != null) {
                    item(key = "genre") {
                        PleyChip(
                            label = params.genre ?: stringResource(R.string.library_genre),
                            selected = params.genre != null,
                            onClick = { onSheet(LibrarySheet.Genre) },
                            dropdown = true,
                        )
                    }
                }
                if (state.filters.years.isNotEmpty() || params.year != null) {
                    item(key = "year") {
                        PleyChip(
                            label = params.year?.toString() ?: stringResource(R.string.library_year),
                            selected = params.year != null,
                            onClick = { onSheet(LibrarySheet.Year) },
                            dropdown = true,
                        )
                    }
                }
                if (state.resolutions.isNotEmpty()) {
                    item(key = "resolution") {
                        PleyChip(
                            label = params.resolution?.label ?: stringResource(R.string.library_resolution),
                            selected = params.resolution != null,
                            onClick = { onSheet(LibrarySheet.Resolution) },
                            dropdown = true,
                        )
                    }
                }
                if (state.dynamicRanges.isNotEmpty()) {
                    item(key = "dynamic_range") {
                        PleyChip(
                            label = params.dynamicRange?.labelText?.asString() ?: stringResource(R.string.library_dynamic_range),
                            selected = params.dynamicRange != null,
                            onClick = { onSheet(LibrarySheet.DynamicRange) },
                            dropdown = true,
                        )
                    }
                }
                if (params.filtered) {
                    item(key = "clear") {
                        PleyChip(label = stringResource(R.string.library_clear_filters), selected = false, onClick = vm::clearFilters, icon = PleyIcons.X)
                    }
                    item(key = "save") {
                        val navigator = LocalPhoneNavigator.current
                        PleyChip(label = stringResource(R.string.library_save_as), selected = false, onClick = { navigator.open(vm.saveAs()) }, icon = PleyIcons.Sparkles)
                    }
                }
            }
            if (counted != null && !params.filtered) {
                item(key = "count") {
                    Text(
                        counted,
                        style = MaterialTheme.typography.labelMedium,
                        color = PleyColors.Muted,
                        modifier = Modifier.padding(start = 4.dp),
                    )
                }
            }
        }
        if (counted != null && params.filtered) {
            Text(
                counted,
                style = MaterialTheme.typography.labelMedium,
                color = PleyColors.Muted,
                modifier = Modifier.padding(start = metrics.marginStart, end = metrics.marginEnd, bottom = 8.dp),
            )
        }
    }
}

/**
 * The grid: [density]'s columns, each cell keyed by its item (or its
 * place while loading), its picture asked for 1.5 screens ahead, and
 * about a screen composed ahead of the scroll. As it scrolls, the
 * ViewModel hears where it is, so a big library's window follows.
 */
@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun LibraryGrid(
    state: LibraryUiState<ArtworkTileModel>,
    cells: Cells<ArtworkTileModel>?,
    vm: LibraryViewModel<ArtworkTileModel>,
    skeleton: SkeletonState,
) {
    val navigator = LocalPhoneNavigator.current
    val metrics = LocalPhoneMetrics.current
    val density = LocalDensity.current
    val kind = if (state.grid.look == TileLook.Square) TileKind.Square else TileKind.Poster
    val gap = PleyPhone.TileGap
    val available = metrics.contentWidth - metrics.margin * 2
    val columns = libraryColumns(state.density, kind, available, gap)
    val width = cellWidth(columns, available, gap)
    val grid = rememberLazyGridState(cacheWindow = remember { LazyLayoutCacheWindow(aheadFraction = 1f, behindFraction = 0.25f) })
    // A tap reads the page as it is then: cards keep their handlers, and don't recompose, as the listing's pages arrive.
    val current = rememberUpdatedState(state)

    // A new sort or filter starts at the top.
    val query = state.params.query()
    var shownQuery by rememberSaveable { mutableStateOf(query.toString()) }
    LaunchedEffect(query) {
        if (shownQuery != query.toString()) {
            shownQuery = query.toString()
            grid.scrollToItem(0)
        }
    }
    LaunchedEffect(grid, vm) {
        snapshotFlow { grid.firstVisibleItemIndex }.distinctUntilChanged().collect(vm::showing)
    }
    val images = remember(cells) { cells?.let(::ImagesOf) ?: emptyList() }
    rememberImagePrefetcher(
        state = grid,
        images = images,
        widthPx = with(density) { width.roundToPx() },
        heightPx = with(density) { (width / kind.aspectRatio).roundToPx() },
        tier = LocalPhoneDeviceTier.current,
    )
    Box(Modifier.fillMaxSize()) {
        LazyVerticalGrid(
            columns = remember(columns) { EqualCells(columns) },
            state = grid,
            modifier = Modifier.fillMaxSize().testTag(TestTags.LIBRARY_GRID),
            contentPadding = PaddingValues(start = metrics.marginStart, end = metrics.marginEnd, top = 6.dp, bottom = 24.dp),
            horizontalArrangement = Arrangement.spacedBy(gap),
            verticalArrangement = Arrangement.spacedBy(ROW_GAP),
            userScrollEnabled = cells != null,
        ) {
            if (cells == null) {
                items(SKELETONS, contentType = { SKELETON }) {
                    SkeletonTile(skeleton, kind, TileStyle.Phone, width = width)
                }
            } else {
                items(
                    count = cells.total,
                    key = { cells.key(it) },
                    contentType = { if (cells[it] == null) SKELETON else kind },
                ) { position ->
                    val model = cells[position]
                    if (model == null) {
                        SkeletonTile(skeleton, kind, TileStyle.Phone, width = width)
                    } else {
                        MediaCard(
                            model = model,
                            kind = kind,
                            onClick = { navigator.open(current.value.opens(model.id)) },
                            onLongClick = { navigator.open(PleyKey.ItemActions(model.id)) },
                            modifier = Modifier.fillMaxWidth(),
                        )
                    }
                }
            }
        }
        if (cells != null && cells.total > FAST_SCROLL_FROM) {
            FastScroller(
                grid = grid,
                total = cells.total,
                columns = columns,
                index = state.grid.index,
                modifier = Modifier.align(Alignment.TopEnd).padding(end = metrics.insetEnd),
            )
        }
    }
}

/** A listing's pictures by position, for the prefetcher: `null` where nothing is loaded. */
@Stable
private class ImagesOf(private val cells: Cells<ArtworkTileModel>) : AbstractList<ArtworkImage?>() {
    override val size: Int get() = cells.total

    override fun get(index: Int): ArtworkImage? = cells[index]?.image
}

/** Between a card's caption and the next row's art. */
private val ROW_GAP = 16.dp

/** Skeleton cards while nothing is known. */
private const val SKELETONS = 18

/** Grids this long get the fast-scroll handle. */
private const val FAST_SCROLL_FROM = 60

private const val SKELETON = "skeleton"

/** The sheets the toolbar opens. */
internal enum class LibrarySheet { Sort, Genre, Year, Resolution, DynamicRange }

/** [GridDensity]'s label. */
internal fun densityLabel(density: GridDensity): Int = when (density) {
    GridDensity.Small -> R.string.library_density_small
    GridDensity.Medium -> R.string.library_density_medium
    GridDensity.Large -> R.string.library_density_large
}
