package app.pley.android.ui.tv.library

import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.focusGroup
import androidx.compose.foundation.gestures.LocalBringIntoViewSpec
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.saveable.rememberSaveableStateHolder
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusDirection
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusProperties
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import app.pley.android.data.Loadable
import app.pley.android.data.valueOrNull
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyTv
import app.pley.android.designsystem.PleyType
import app.pley.android.designsystem.TvSafeArea
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.designsystem.tile.ArtworkTileModel
import app.pley.android.designsystem.tile.SkeletonState
import app.pley.android.designsystem.tile.TileKind
import app.pley.android.designsystem.tile.rememberSkeleton
import app.pley.android.model.LibraryKind
import app.pley.android.model.SortOrder
import app.pley.android.screens.LibraryTab
import app.pley.android.screens.PleyKey
import app.pley.android.screens.library.LibraryDeps
import app.pley.android.screens.library.LibraryUiState
import app.pley.android.screens.library.LibraryViewModel
import app.pley.android.screens.library.TileLook
import app.pley.android.screens.screensGraph
import app.pley.android.ui.tv.R
import app.pley.android.ui.tv.item.tileActions
import app.pley.android.ui.tv.kit.FocusMemory
import app.pley.android.ui.tv.kit.TvScrolling
import app.pley.android.ui.tv.kit.railFade
import app.pley.android.ui.tv.kit.rememberFocusMemory
import app.pley.android.ui.tv.kit.rememberedFocus
import app.pley.android.ui.tv.shell.LocalTvNavigator
import app.pley.android.ui.tv.shell.ReportOffline
import app.pley.android.ui.tv.shell.failedBody
import app.pley.android.ui.tv.shell.failedTitle
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.launch

/**
 * A library's page on TV (design spec §4.2, mockup tv-library): the name
 * at the top left and the tabs on the same line at the right, then the
 * tab: the Library tab's chips (Sort, Filter, Unwatched, Genre, Year, the
 * resolution and dynamic range while one is chosen, and the count) over a
 * grid of six posters a row with the A to Z jump beside it; the
 * Recommended tab's rows; the Collections tab's grid. Sort and filters
 * open in a side panel. The first tile has focus, or the one focused last
 * when coming back.
 */
@Composable
internal fun TvLibraryScreen(key: PleyKey.Library) {
    var saved by rememberSaveable { mutableStateOf<String?>(null) }
    val context = LocalContext.current
    val vm: LibraryViewModel<ArtworkTileModel> = viewModel {
        LibraryViewModel(key.libraryId, key.tab, saved, LibraryDeps.of(screensGraph()), TvTiles(context), key.view)
    }
    LaunchedEffect(vm) {
        vm.state.map { it.params.toQueryString() }.distinctUntilChanged().collect { saved = it }
    }
    val state by vm.state.collectAsStateWithLifecycle()
    TvLibraryPage(state, vm)
}

@Composable
private fun TvLibraryPage(state: LibraryUiState<ArtworkTileModel>, vm: LibraryViewModel<ArtworkTileModel>) {
    val memory = rememberFocusMemory()
    val skeleton = rememberSkeleton(animated = false)
    val tabs = rememberSaveableStateHolder()
    var panel by rememberSaveable { mutableStateOf<TvPanel?>(null) }
    // The chip a panel was opened from takes focus back when it closes.
    val chips = remember { TvPanel.entries.associateWith { FocusRequester() } }
    // Up from the tab's content reaches the tabs, however far right they are.
    val tabFocus = remember { FocusRequester() }
    // Down from the tabs enters the tab's content (the header sits apart from it on screen).
    val contentFocus = remember { FocusRequester() }
    var returnTo by remember { mutableStateOf<TvPanel?>(null) }
    // The grids' focus, kept here while the page is covered: Back to the page lands on the tile it had.
    val libraryGrid = remember { TvGridFocus() }
    val collectionsGrid = remember { TvGridFocus() }
    val params by rememberUpdatedState(state.params)
    LaunchedEffect(panel) {
        val chip = returnTo
        if (panel == null && chip != null) {
            withFrameNanos { }
            // A resolution's or dynamic range's chip goes with its choice: focus goes to Filter, where it is chosen.
            val gone = (chip == TvPanel.Resolution && params.resolution == null) || (chip == TvPanel.DynamicRange && params.dynamicRange == null)
            runCatching { chips.getValue(if (gone) TvPanel.Filter else chip).requestFocus() }
            returnTo = null
        }
    }
    val openPanel: (TvPanel?) -> Unit = { next ->
        if (panel == null && next != null) returnTo = next
        panel = next
    }
    // Content takes focus on its own only on the tab the page opened on (where it was left), before another was chosen.
    var switched by rememberSaveable { mutableStateOf(false) }
    val openedOn = state.openedOn
    val tab = state.params.tab
    Box(Modifier.fillMaxSize().backToGrid(memory, mapOf("grid/" to libraryGrid, "collections/" to collectionsGrid))) {
        when {
            state.missing -> TvMessage(
                title = stringResource(R.string.tv_library_missing_title),
                body = stringResource(R.string.tv_library_missing_body),
            )
            state.libraryError != null -> TvMessage(
                title = failedTitle(state.libraryError!!, stringResource(R.string.tv_library_failed)),
                body = failedBody(state.libraryError!!),
                action = stringResource(R.string.tv_library_retry),
                onAction = vm::retry,
            )
            // The tab the library was left on is read first: no other flashes before it.
            openedOn == null -> Unit
            else -> tabs.SaveableStateProvider(tab) {
                val initial = !switched && tab == openedOn
                when (tab) {
                    LibraryTab.Library -> TvLibraryTab(state, vm, memory, skeleton, initial, libraryGrid, chips, tabFocus, contentFocus, onPanel = openPanel)
                    LibraryTab.Recommended -> TvRecommendedTab(state, vm, memory, initial, tabFocus, Modifier.focusRequester(contentFocus).upTo(tabFocus))
                    LibraryTab.Collections -> TvCollectionsTab(state, vm, memory, skeleton, initial, collectionsGrid, tabFocus, contentFocus)
                }
            }
        }
        Header(
            name = state.library?.name.orEmpty(),
            tabs = if (state.missing || state.libraryError != null) emptyList() else state.tabs,
            selected = tab,
            memory = memory,
            focus = tabFocus,
            down = contentFocus,
            onSelect = {
                if (it != tab) switched = true
                vm.select(it)
            },
        )
        TvLibraryPanel(panel, state, vm, onPanel = openPanel)
    }
}

/** The library's name at (96, 27) in tvPageTitle, and the tabs on its line at the right safe edge. */
@Composable
private fun Header(
    name: String,
    tabs: List<LibraryTab>,
    selected: LibraryTab,
    memory: FocusMemory,
    focus: FocusRequester,
    down: FocusRequester,
    onSelect: (LibraryTab) -> Unit,
) {
    Row(
        Modifier
            .fillMaxWidth()
            .padding(start = PleyTv.Gutter, end = TvSafeArea.Horizontal, top = TvSafeArea.Vertical)
            .height(HeaderHeight),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        BasicText(
            name,
            style = PleyType.Tv.PageTitle.copy(color = PleyColors.Fg),
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.weight(1f).semantics { heading() },
        )
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
            tabs.forEach { tab ->
                TvTab(
                    label = stringResource(tvTabLabel(tab)),
                    selected = tab == selected,
                    onClick = { onSelect(tab) },
                    modifier = (if (tab == selected) Modifier.focusRequester(focus) else Modifier)
                        .focusProperties { this.down = down }
                        .rememberedFocus(memory, "tab/${tab.name}"),
                )
            }
        }
    }
}

/**
 * The Library tab: the chips at y = 80, the grid from y = 136, and the
 * jump control at (872, 136) when the listing has its A to Z.
 */
@Composable
private fun TvLibraryTab(
    state: LibraryUiState<ArtworkTileModel>,
    vm: LibraryViewModel<ArtworkTileModel>,
    memory: FocusMemory,
    skeleton: SkeletonState,
    initial: Boolean,
    focus: TvGridFocus,
    chips: Map<TvPanel, FocusRequester>,
    tabFocus: FocusRequester,
    contentFocus: FocusRequester,
    onPanel: (TvPanel) -> Unit,
) {
    val navigator = LocalTvNavigator.current
    val actions = tileActions()
    val resources = LocalResources.current
    val params = state.params
    val music = state.kind == LibraryKind.Music
    val cells = state.grid.cells.valueOrNull
    val grid = rememberTvGridState()
    val jump = remember { FocusRequester() }
    val scope = rememberCoroutineScope()
    val kind = if (state.grid.look == TileLook.Square) TileKind.Square else TileKind.LibraryGrid
    val columns = tvGridColumns(kind)

    // A new sort or filter starts at the top.
    val query = remember(params) { params.query().toString() }
    var shownQuery by rememberSaveable { mutableStateOf(query) }
    LaunchedEffect(query) {
        if (shownQuery != query) {
            shownQuery = query
            grid.scrollToItem(0)
        }
    }
    LaunchedEffect(grid, vm) {
        snapshotFlow { grid.firstVisibleItemIndex }.distinctUntilChanged().collect(vm::showing)
    }

    Column(Modifier.fillMaxSize()) {
        Spacer(Modifier.height(ChipsTop))
        ChipRow(Modifier.focusRequester(contentFocus).upTo(tabFocus)) {
            TvChip(
                label = stringResource(tvSortLabel(params.sort)),
                icon = if (params.effectiveOrder == SortOrder.Ascending) PleyIcons.ArrowUpNarrowWide else PleyIcons.ArrowDownWideNarrow,
                onClick = { onPanel(TvPanel.Sort) },
                modifier = Modifier.focusRequester(chips.getValue(TvPanel.Sort)).rememberedFocus(memory, "chip/sort"),
            )
            if (!music) {
                TvChip(
                    label = stringResource(R.string.tv_library_filter),
                    icon = PleyIcons.SlidersHorizontal,
                    onClick = { onPanel(TvPanel.Filter) },
                    modifier = Modifier.focusRequester(chips.getValue(TvPanel.Filter)).rememberedFocus(memory, "chip/filter"),
                )
                TvChip(
                    label = stringResource(R.string.tv_library_unwatched),
                    selected = params.watched == false,
                    onClick = { vm.watched(if (params.watched == false) null else false) },
                    modifier = Modifier.rememberedFocus(memory, "chip/unwatched"),
                )
                if (state.filters.genres.isNotEmpty() || params.genre != null) {
                    TvChip(
                        label = params.genre ?: stringResource(R.string.tv_library_genre),
                        selected = params.genre != null,
                        onClick = { onPanel(TvPanel.Genre) },
                        modifier = Modifier.widthIn(max = 220.dp).focusRequester(chips.getValue(TvPanel.Genre)).rememberedFocus(memory, "chip/genre"),
                    )
                }
                if (state.filters.years.isNotEmpty() || params.year != null) {
                    TvChip(
                        label = params.year?.toString() ?: stringResource(R.string.tv_library_year),
                        selected = params.year != null,
                        onClick = { onPanel(TvPanel.Year) },
                        modifier = Modifier.focusRequester(chips.getValue(TvPanel.Year)).rememberedFocus(memory, "chip/year"),
                    )
                }
                // Resolution and dynamic range are chosen in the Filter panel; their chips show only what is chosen.
                params.resolution?.let { resolution ->
                    TvChip(
                        label = resolution.label,
                        selected = true,
                        onClick = { onPanel(TvPanel.Resolution) },
                        modifier = Modifier.focusRequester(chips.getValue(TvPanel.Resolution)).rememberedFocus(memory, "chip/resolution"),
                    )
                }
                params.dynamicRange?.let { range ->
                    TvChip(
                        label = tvRangeLabel(range),
                        selected = true,
                        onClick = { onPanel(TvPanel.DynamicRange) },
                        modifier = Modifier.focusRequester(chips.getValue(TvPanel.DynamicRange)).rememberedFocus(memory, "chip/dynamic_range"),
                    )
                }
            }
            if (cells != null) {
                // "23 of 342 movies" when filtered.
                val of = state.library?.itemCount?.takeIf { params.filtered && it >= cells.total }
                BasicText(
                    if (of != null) {
                        tvCountOf(resources, tvItemsOfPlural(state.kind), cells.total, of)
                    } else {
                        tvCount(resources, tvItemsPlural(state.kind), cells.total)
                    },
                    style = PleyType.Tv.Caption.copy(color = PleyColors.Muted),
                    maxLines = 1,
                    modifier = Modifier.padding(start = 8.dp),
                )
            }
        }
        Spacer(Modifier.height(GridTop - ChipsTop - 40.dp - TileRoom))
        Box(Modifier.weight(1f).fillMaxWidth()) {
            val failed = state.grid.cells as? Loadable.Failed
            ReportOffline(state.grid.cells)
            when {
                failed != null && cells == null -> TvMessage(
                    title = failedTitle(failed.error, stringResource(R.string.tv_library_failed)),
                    body = failedBody(failed.error),
                    action = stringResource(R.string.tv_library_retry),
                    onAction = vm::retry,
                    top = 24.dp,
                )
                cells != null && cells.total == 0 -> if (state.filtered) {
                    TvMessage(
                        title = stringResource(R.string.tv_library_no_matches_title),
                        body = stringResource(R.string.tv_library_no_matches_body),
                        action = stringResource(R.string.tv_library_clear_filters),
                        onAction = vm::clearFilters,
                        top = 24.dp,
                    )
                } else {
                    TvMessage(
                        title = stringResource(R.string.tv_library_empty_title),
                        body = stringResource(R.string.tv_library_empty_body),
                        top = 24.dp,
                        takeFocus = initial,
                        focusInstead = tabFocus,
                    )
                }
                else -> {
                    // A tile its actions took out of the grid (watched, with Unwatched on): focus comes back to the one in its place.
                    memory.settle { key -> gridTileId(key)?.let { id -> cells != null && cells.positionOf(id) >= 0 } == true }
                    TvPosterGrid(
                        total = cells?.total ?: SKELETONS,
                        tileAt = { cells?.get(it) },
                        // By position: a cell keeps its key (and focus) when its page loads.
                        keyAt = { it },
                        kind = kind,
                        onOpen = { _, tile -> navigator.open(state.opens(tile.id)) },
                        onActions = { tile ->
                            val at = cells?.positionOf(tile.id) ?: -1
                            val near = cells?.let { c -> listOf(c.idAt(at + 1), c.idAt(at - 1)).firstOrNull { at >= 0 && it >= 0 } }
                            memory.mayLeave("grid/${tile.id}", near?.let { "grid/$it" })
                            actions.open(tile.id, continueWatching = false)
                        },
                        memory = memory,
                        memoryPrefix = "grid",
                        skeleton = skeleton,
                        state = grid,
                        focus = focus,
                        initialFocus = initial,
                        columns = columns,
                        onRight = if (state.grid.index != null) ({ runCatching { jump.requestFocus() }.isSuccess }) else null,
                        modifier = Modifier.fillMaxSize(),
                    )
                    val index = state.grid.index
                    if (index != null) {
                        JumpControl(
                            index = index,
                            focus = focus,
                            requester = jump,
                            onJump = { position ->
                                scope.launch {
                                    grid.scrollToItem(position - position % columns)
                                    withFrameNanos { }
                                    focus.focus(position)
                                }
                            },
                            modifier = Modifier.offset(x = PleyTv.CanvasWidth - TvSafeArea.Horizontal - JumpSize, y = TileRoom - NeighbourRoom),
                        )
                    }
                }
            }
        }
    }
}

/**
 * The Library tab's chips at y = 80, from the gutter. The row is as wide as
 * the page, so a chip's focus growth is never cut. When the chips and the
 * count don't all fit short of the safe area's edge (every filter chosen,
 * a long genre), it scrolls to its end as focus nears it, and back to its
 * start ([TvScrolling.ends]).
 */
@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun ChipRow(modifier: Modifier, content: @Composable RowScope.() -> Unit) {
    val density = LocalDensity.current
    val scroll = rememberScrollState()
    val ends = remember(density, scroll) { with(density) { TvScrolling.ends(scroll, PleyTv.Gutter.toPx(), TvSafeArea.Horizontal.toPx()) } }
    CompositionLocalProvider(LocalBringIntoViewSpec provides ends) {
        Row(
            Modifier
                .fillMaxWidth()
                // Scrolled, the first chips pass under the closed rail, which the page fades out as its rows do.
                .railFade { scroll.value > 0 }
                .horizontalScroll(scroll)
                .padding(start = PleyTv.Gutter, end = TvSafeArea.Horizontal)
                .height(40.dp)
                .then(modifier),
            horizontalArrangement = Arrangement.spacedBy(10.dp),
            verticalAlignment = Alignment.CenterVertically,
            content = content,
        )
    }
}

/** The Collections tab: the count at y = 80, then the collections' posters with their counts, from y = 136. */
@Composable
private fun TvCollectionsTab(
    state: LibraryUiState<ArtworkTileModel>,
    vm: LibraryViewModel<ArtworkTileModel>,
    memory: FocusMemory,
    skeleton: SkeletonState,
    initial: Boolean,
    focus: TvGridFocus,
    tabFocus: FocusRequester,
    contentFocus: FocusRequester,
) {
    val navigator = LocalTvNavigator.current
    val resources = LocalResources.current
    val collections = state.collections
    val list = collections.valueOrNull
    Column(Modifier.fillMaxSize()) {
        Spacer(Modifier.height(ChipsTop))
        Box(Modifier.padding(start = PleyTv.Gutter).height(40.dp), contentAlignment = Alignment.CenterStart) {
            if (list != null) {
                BasicText(
                    tvCount(resources, R.plurals.tv_library_count_collections, list.size),
                    style = PleyType.Tv.Caption.copy(color = PleyColors.Muted),
                )
            }
        }
        Spacer(Modifier.height(GridTop - ChipsTop - 40.dp - TileRoom))
        Box(Modifier.weight(1f).fillMaxWidth()) {
            when {
                list == null && collections is Loadable.Failed -> TvMessage(
                    title = failedTitle(collections.error, stringResource(R.string.tv_library_collections_failed)),
                    body = failedBody(collections.error),
                    action = stringResource(R.string.tv_library_retry),
                    onAction = vm::retry,
                    top = 24.dp,
                )
                list != null && list.isEmpty() -> TvMessage(
                    title = stringResource(R.string.tv_library_collections_empty_title),
                    body = stringResource(
                        if (state.kind == LibraryKind.Shows) R.string.tv_library_collections_empty_shows else R.string.tv_library_collections_empty_movies,
                    ),
                    top = 24.dp,
                    takeFocus = initial,
                    focusInstead = tabFocus,
                )
                else -> TvPosterGrid(
                    total = list?.size ?: SKELETONS,
                    tileAt = { list?.getOrNull(it)?.tile },
                    keyAt = { it },
                    kind = TileKind.LibraryGrid,
                    onOpen = { position, _ -> list?.getOrNull(position)?.let { navigator.open(PleyKey.Collection(it.id)) } },
                    memory = memory,
                    memoryPrefix = "collections",
                    skeleton = skeleton,
                    focus = focus,
                    initialFocus = initial,
                    badge = { list?.getOrNull(it)?.count?.toString() },
                    modifier = Modifier.fillMaxSize().focusRequester(contentFocus).upTo(tabFocus),
                )
            }
        }
    }
}

/** The item id of a grid tile's focus key ("grid/<id>"); `null` for any other key. */
private fun gridTileId(key: String): Long? = key.removePrefix("grid/").takeIf { it.length < key.length }?.toLongOrNull()

/** Up out of this group goes to [target] (the tabs), not to whatever is geometrically nearest. */
internal fun Modifier.upTo(target: FocusRequester): Modifier = focusProperties {
    val inner = onExit
    onExit = {
        inner()
        if (requestedFocusDirection == FocusDirection.Up) {
            cancelFocusChange()
            runCatching { target.requestFocus() }
        }
    }
}.focusGroup()

/**
 * Focus coming back to the page (Back from a title, Right from the rail)
 * lands on the tile it left when that was one of [grids]' (by its key's
 * prefix in [memory]), as the Samsung app's grid comes back to its last
 * child. The page's own restore stops at the grid's focus group, which
 * keeps no child of its own, and the page's first element took it: the
 * Title chip, above a grid at its top.
 */
private fun Modifier.backToGrid(memory: FocusMemory, grids: Map<String, TvGridFocus>): Modifier = focusProperties {
    onEnter = {
        val key = memory.key
        val grid = grids.entries.firstOrNull { key != null && key.startsWith(it.key) }?.value
        val at = grid?.focused ?: -1
        if (requestedFocusDirection == FocusDirection.Enter && at >= 0 && grid?.slots?.has(at) == true) {
            cancelFocusChange()
            grid.focus(at)
        }
    }
}.focusGroup()

/** Skeleton tiles while nothing is known: three rows. */
private const val SKELETONS = 18

/** The header's line: tvPageTitle's 34 dp, room for the tabs' 40 and their focus scale. */
internal val HeaderHeight = 44.dp

/** The chips' top, and the grid's tiles' (design spec §4.2). */
internal val ChipsTop = 80.dp
internal val GridTop = 136.dp
