package app.pley.android.ui.tv.home

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.background
import androidx.compose.foundation.focusGroup
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.itemsIndexed
import androidx.compose.foundation.lazy.grid.rememberLazyGridState
import androidx.compose.foundation.relocation.BringIntoViewRequester
import androidx.compose.foundation.relocation.bringIntoViewRequester
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusProperties
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import androidx.tv.material3.ClickableSurfaceDefaults
import androidx.tv.material3.LocalContentColor
import androidx.tv.material3.Surface
import app.pley.android.data.Loadable
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyFocus
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.PleyTv
import app.pley.android.designsystem.PleyType
import app.pley.android.designsystem.TvSafeArea
import app.pley.android.designsystem.icon.Icon
import app.pley.android.designsystem.icon.PleyIcon
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.model.LibraryKind
import app.pley.android.screens.PleyKey
import app.pley.android.screens.home.LibrariesEvent
import app.pley.android.screens.home.LibrariesState
import app.pley.android.screens.home.LibrariesViewModel
import app.pley.android.screens.home.LibraryEntry
import app.pley.android.ui.tv.R
import app.pley.android.ui.tv.item.TvToastHost
import app.pley.android.ui.tv.item.TvToastState
import app.pley.android.ui.tv.kit.OverlayShowing
import app.pley.android.ui.tv.kit.TvPillButton
import app.pley.android.ui.tv.kit.TvSidePanel
import app.pley.android.ui.tv.kit.menuKey
import app.pley.android.ui.tv.kit.rememberFocusMemory
import app.pley.android.ui.tv.kit.rememberedFocus
import app.pley.android.ui.tv.playlists.TvPanelRow
import app.pley.android.ui.tv.playlists.TvPanelTitle
import app.pley.android.ui.tv.shell.LocalTvNavigator
import app.pley.android.ui.tv.shell.LocalTvRailItems
import app.pley.android.ui.tv.shell.ReportOffline
import app.pley.android.ui.tv.shell.failedBody
import app.pley.android.ui.tv.shell.failedTitle
import app.pley.android.ui.tv.shell.playlistsOnLibrariesPage
import kotlinx.coroutines.launch

/**
 * Every library (design spec §2.1): the rail shows the first three, and
 * "Libraries" opens this page. The page title, then a card per library in
 * the server's order: its kind's icon, its name, how much it holds, and
 * "Scanning…" while the server scans it. After them, while the rail has
 * no room for Playlists ([playlistsOnLibrariesPage]), a card like theirs
 * opens Playlists. Initial focus on the first card, or on the one opened
 * last when coming back.
 *
 * The profile may put them in its own order (with two or more): Sort, by
 * the title, or Move in a card's actions (holding Select). While sorting,
 * Select lifts a card; the arrows move it through the grid, Select drops
 * it there (and saves the order), Back puts it back. Done or Back ends
 * sorting; "Use the server's order" goes back to the order everyone sees.
 */
@Composable
internal fun TvLibrariesScreen(vm: LibrariesViewModel = viewModel(factory = LibrariesViewModel.Factory)) {
    val state by vm.state.collectAsStateWithLifecycle()
    val navigator = LocalTvNavigator.current
    val playlists = playlistsOnLibrariesPage(LocalTvRailItems.current)
    val resources = LocalResources.current
    val toasts = remember { TvToastState() }
    LaunchedEffect(vm) {
        vm.events.collect { event ->
            when (event) {
                is LibrariesEvent.Failed -> toasts.report(event.error)
                LibrariesEvent.NeedsNewerServer -> toasts.show(resources.getString(R.string.tv_libraries_sort_needs_newer_server), error = true)
            }
        }
    }
    ReportOffline(
        when (val s = state) {
            is LibrariesState.Ready -> s.error?.let { Loadable.Failed(it, Unit) } ?: Loadable.Ready(Unit, s.fresh)
            is LibrariesState.Failed -> Loadable.Failed(s.error, Unit)
            LibrariesState.Loading -> null
        },
    )
    Box(Modifier.fillMaxSize()) {
        when (val s = state) {
            LibrariesState.Loading -> Unit
            is LibrariesState.Failed -> TvHomeNotice(failedTitle(s.error, stringResource(R.string.tv_libraries_failed)), failedBody(s.error), retry = vm::refresh)
            is LibrariesState.Ready -> if (s.libraries.isEmpty()) {
                TvHomeNotice(stringResource(R.string.tv_home_no_libraries), stringResource(R.string.tv_home_no_libraries_body))
            } else {
                LibrariesGrid(s, vm, playlists, open = navigator::open)
            }
        }
        TvToastHost(toasts)
    }
}

@Composable
private fun LibrariesGrid(state: LibrariesState.Ready, vm: LibrariesViewModel, playlists: Boolean, open: (PleyKey) -> Unit) {
    val memory = rememberFocusMemory()
    val scope = rememberCoroutineScope()
    var sorting by remember { mutableStateOf(false) }
    var moving by remember { mutableStateOf<Long?>(null) }
    var actionsFor by remember { mutableStateOf<LibraryEntry?>(null) }
    val cardFocus = remember { mutableStateMapOf<Long, FocusRequester>() }
    val inSort = sorting && state.sortable
    // Nothing left to sort (a library went, or this sign-in may not): the page as it is.
    LaunchedEffect(state.sortable) {
        if (!state.sortable) {
            sorting = false
            moving = null
        }
    }
    val focusCard = { id: Long -> scope.launch { withFrameNanos { }; runCatching { cardFocus[id]?.requestFocus() } } }
    // Back: a lifted card goes back where it was, then sorting ends.
    BackHandler(enabled = moving != null) {
        vm.cancelMoves()
        moving = null
    }
    BackHandler(enabled = inSort && moving == null) { sorting = false }
    // While sorting, the keys are the cards': requests' news waits.
    OverlayShowing(inSort)

    Column(Modifier.fillMaxSize().padding(top = TvSafeArea.Vertical)) {
        Row(
            Modifier.fillMaxWidth().padding(start = PleyTv.Gutter, end = TvSafeArea.Horizontal).focusGroup(),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            BasicText(
                stringResource(if (inSort) R.string.tv_libraries_sort else R.string.tv_libraries_title),
                style = PleyType.Tv.PageTitle.copy(color = PleyColors.Fg),
                modifier = Modifier.weight(1f).semantics { heading() },
            )
            when {
                inSort -> {
                    TvPillButton(
                        stringResource(R.string.tv_libraries_server_order),
                        onClick = {
                            moving = null
                            vm.useServerOrder()
                        },
                        icon = PleyIcons.RotateCcw,
                        enabled = moving == null,
                    )
                    Spacer(Modifier.width(12.dp))
                    TvPillButton(
                        stringResource(R.string.tv_libraries_sort_done),
                        onClick = { sorting = false },
                        icon = PleyIcons.Check,
                        enabled = moving == null,
                    )
                }
                state.sortable -> TvPillButton(
                    stringResource(R.string.tv_libraries_sort),
                    onClick = {
                        sorting = true
                        // The button goes: the first library takes the focus, ready to lift.
                        state.libraries.firstOrNull()?.let { focusCard(it.library.id) }
                    },
                    icon = PleyIcons.ArrowUpDown,
                    modifier = Modifier.testTag(TV_LIBRARIES_SORT),
                )
            }
        }
        Spacer(Modifier.height(20.dp))
        LazyVerticalGrid(
            columns = GridCells.Fixed(COLUMNS),
            state = rememberLazyGridState(),
            contentPadding = PaddingValues(start = PleyTv.Gutter, end = TvSafeArea.Horizontal, top = 8.dp, bottom = TvSafeArea.Vertical),
            horizontalArrangement = Arrangement.spacedBy(16.dp),
            verticalArrangement = Arrangement.spacedBy(16.dp),
            modifier = Modifier.fillMaxWidth(),
        ) {
            itemsIndexed(state.libraries, key = { _, entry -> entry.library.id }, contentType = { _, _ -> "library" }) { index, entry ->
                val id = entry.library.id
                val requester = remember { FocusRequester() }
                cardFocus[id] = requester
                LibraryCard(
                    entry,
                    index = index,
                    sorting = inSort,
                    moving = moving == id,
                    onClick = {
                        when {
                            moving != null -> Unit
                            inSort -> moving = id
                            else -> open(PleyKey.Library(id))
                        }
                    },
                    onLongClick = { if (moving == null) actionsFor = entry },
                    onMove = { by ->
                        // Down onto a shorter last row lands at its end.
                        val last = state.libraries.lastIndex
                        val to = index + by
                        val clamped = if (to > last && index / COLUMNS < last / COLUMNS) last else to
                        vm.moveBy(id, clamped - index, save = false)
                    },
                    onDrop = {
                        vm.saveOrder()
                        moving = null
                    },
                    modifier = Modifier
                        .focusRequester(requester)
                        .rememberedFocus(memory, "library/$id", initial = index == 0),
                )
            }
            // Not while sorting, which moves the libraries alone.
            if (playlists && !inSort) {
                item(key = PLAYLISTS_KEY, contentType = "library") {
                    PlaylistsCard(onClick = { open(PleyKey.Playlists) }, modifier = Modifier.rememberedFocus(memory, PLAYLISTS_KEY))
                }
            }
        }
    }
    if (inSort) SortHint(if (moving != null) R.string.tv_libraries_moving else R.string.tv_libraries_sorting)

    // A card's actions: Open, and while they can be sorted, Move and the server's order.
    val shown = actionsFor
    TvSidePanel(visible = shown != null, onDismiss = {
        actionsFor = null
        shown?.let { focusCard(it.library.id) }
    }) {
        if (shown != null) {
            CardActions(
                shown,
                sortable = state.sortable,
                onOpen = {
                    actionsFor = null
                    open(PleyKey.Library(shown.library.id))
                },
                onMove = {
                    actionsFor = null
                    sorting = true
                    moving = shown.library.id
                    focusCard(shown.library.id)
                },
                onServerOrder = {
                    actionsFor = null
                    vm.useServerOrder()
                    focusCard(shown.library.id)
                },
            )
        }
    }
}

/**
 * One library: a [surface2][PleyColors.Surface2] card; focused, the accent
 * fill with [onAccent][PleyColors.OnAccent] content at 1.05 (design spec
 * §5.2 rule 1). While [sorting], a handle at its end; lifted ([moving]),
 * a ring around it, and the arrows move it [by][onMove] a place left or
 * right, a row up or down, until Select drops it.
 */
@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun LibraryCard(
    entry: LibraryEntry,
    index: Int,
    sorting: Boolean,
    moving: Boolean,
    onClick: () -> Unit,
    onLongClick: () -> Unit,
    onMove: (Int) -> Unit,
    onDrop: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val shape = RoundedCornerShape(PleyShape.CardRadius)
    val bring = remember { BringIntoViewRequester() }
    LaunchedEffect(index, moving) { if (moving) runCatching { bring.bringIntoView() } }
    Surface(
        onClick = onClick,
        onLongClick = onLongClick,
        modifier = modifier
            .menuKey(onLongClick)
            .fillMaxWidth()
            .height(CardHeight)
            .bringIntoViewRequester(bring)
            .onPreviewKeyEvent { event ->
                if (!moving) return@onPreviewKeyEvent false
                val by = when (event.key) {
                    Key.DirectionLeft -> -1
                    Key.DirectionRight -> 1
                    Key.DirectionUp -> -COLUMNS
                    Key.DirectionDown -> COLUMNS
                    Key.DirectionCenter, Key.Enter, Key.NumPadEnter -> {
                        if (event.type == KeyEventType.KeyUp) onDrop()
                        return@onPreviewKeyEvent true
                    }
                    else -> return@onPreviewKeyEvent false
                }
                if (event.type == KeyEventType.KeyDown) onMove(by)
                true
            }
            .drawWithContent {
                drawContent()
                if (moving) {
                    val stroke = PleyFocus.RingStroke.toPx()
                    val r = PleyShape.CardRadius.toPx()
                    drawRoundRect(
                        PleyColors.AccentBright,
                        topLeft = Offset(stroke / 2, stroke / 2),
                        size = Size(size.width - stroke, size.height - stroke),
                        cornerRadius = CornerRadius(r, r),
                        style = Stroke(stroke),
                    )
                }
            }
            .testTag(TV_LIBRARY_CARD + entry.library.id),
        shape = ClickableSurfaceDefaults.shape(shape),
        colors = if (moving) {
            ClickableSurfaceDefaults.colors(
                containerColor = PleyColors.Surface3,
                contentColor = PleyColors.Fg,
                focusedContainerColor = PleyColors.Surface3,
                focusedContentColor = PleyColors.Fg,
                pressedContainerColor = PleyColors.Surface3,
                pressedContentColor = PleyColors.Fg,
            )
        } else {
            cardColors()
        },
        // Lifted, it keeps its size: the ring is drawn around it as it is.
        scale = ClickableSurfaceDefaults.scale(focusedScale = if (moving) 1f else 1.05f, pressedScale = 1f),
    ) {
        val content = LocalContentColor.current
        val line = if (entry.scanning) "${entry.count} · ${stringResource(R.string.tv_libraries_scanning)}" else entry.count
        CardBody(
            kindIcon(entry.library.kind),
            entry.library.name,
            line,
            grip = if (!sorting) null else if (moving) PleyColors.AccentBright else content.copy(alpha = 0.6f),
        )
    }
}

/** Playlists, after the libraries while the rail has no room for them: a library's card, with no line under the name. */
@Composable
private fun PlaylistsCard(onClick: () -> Unit, modifier: Modifier = Modifier) {
    Surface(
        onClick = onClick,
        modifier = modifier.fillMaxWidth().height(CardHeight).testTag(TV_LIBRARIES_PLAYLISTS),
        shape = ClickableSurfaceDefaults.shape(RoundedCornerShape(PleyShape.CardRadius)),
        colors = cardColors(),
        scale = ClickableSurfaceDefaults.scale(focusedScale = 1.05f, pressedScale = 1f),
    ) {
        CardBody(PleyIcons.ListMusic, stringResource(R.string.tv_playlists_title), line = null)
    }
}

/** A card's colours: focused, the accent fill with [onAccent][PleyColors.OnAccent] content. */
@Composable
private fun cardColors() = ClickableSurfaceDefaults.colors(
    containerColor = PleyColors.Surface2,
    contentColor = PleyColors.Fg,
    focusedContainerColor = PleyColors.Accent,
    focusedContentColor = PleyColors.OnAccent,
    pressedContainerColor = PleyColors.AccentBright,
    pressedContentColor = PleyColors.OnAccent,
)

/**
 * A card's inside, in its content colour: the icon on a tint, the name,
 * the [line] under it, and while sorting a handle in [grip]'s colour.
 */
@Composable
private fun CardBody(icon: PleyIcon, name: String, line: String?, grip: Color? = null) {
    val content = LocalContentColor.current
    Row(Modifier.fillMaxSize().padding(horizontal = 18.dp), verticalAlignment = Alignment.CenterVertically) {
        Box(
            Modifier.size(PleyTv.ButtonHeight).drawBehind { drawRoundRect(content.copy(alpha = 0.12f), cornerRadius = CornerRadius(PleyShape.ControlRadius.toPx())) },
            contentAlignment = Alignment.Center,
        ) {
            Icon(icon, contentDescription = null, size = PleyTv.IconSize, tint = content)
        }
        Spacer(Modifier.width(14.dp))
        Column(Modifier.weight(1f)) {
            BasicText(
                name,
                style = PleyType.Tv.Title.copy(color = content),
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            if (line != null) {
                Spacer(Modifier.height(2.dp))
                BasicText(line, style = PleyType.Tv.Caption.copy(color = content.copy(alpha = 0.75f)), maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
        }
        if (grip != null) {
            Spacer(Modifier.width(12.dp))
            Icon(PleyIcons.GripVertical, contentDescription = null, size = PleyTv.IconSize, tint = grip)
        }
    }
}

/** A card's actions in the side panel: Open, and Move and the server's order while they can be sorted. */
@Composable
private fun CardActions(entry: LibraryEntry, sortable: Boolean, onOpen: () -> Unit, onMove: () -> Unit, onServerOrder: () -> Unit) {
    val first = remember(entry.library.id) { FocusRequester() }
    LaunchedEffect(entry.library.id) {
        withFrameNanos { }
        withFrameNanos { }
        runCatching { first.requestFocus() }
    }
    Column(Modifier.focusProperties { onExit = { cancelFocusChange() } }.focusGroup()) {
        TvPanelTitle(entry.library.name, entry.count)
        TvPanelRow(stringResource(R.string.tv_libraries_open), onClick = onOpen, icon = PleyIcons.ArrowRight, focusRequester = first)
        if (sortable) {
            TvPanelRow(stringResource(R.string.tv_libraries_move), onClick = onMove, icon = PleyIcons.ArrowUpDown)
            TvPanelRow(stringResource(R.string.tv_libraries_server_order), onClick = onServerOrder, icon = PleyIcons.RotateCcw)
        }
    }
}

/** What the remote does while sorting, at the bottom of the screen. */
@Composable
private fun SortHint(text: Int) {
    Box(Modifier.fillMaxSize().padding(bottom = TvSafeArea.Vertical), contentAlignment = Alignment.BottomCenter) {
        BasicText(
            stringResource(text),
            style = PleyType.Tv.Label.copy(color = PleyColors.Fg),
            modifier = Modifier.background(PleyColors.Surface3, PleyShape.Pill).padding(horizontal = 20.dp, vertical = 12.dp),
        )
    }
}

/** A library's icon by kind, as the rail draws it. */
internal fun kindIcon(kind: LibraryKind): PleyIcon = when (kind) {
    LibraryKind.Movies -> PleyIcons.Clapperboard
    LibraryKind.Shows -> PleyIcons.Tv
    LibraryKind.Music -> PleyIcons.Music
    LibraryKind.Unknown -> PleyIcons.Film
}

/** Sort, a library's card (+ its id), and Playlists' card. */
internal const val TV_LIBRARIES_SORT = "tv_libraries_sort"
internal const val TV_LIBRARY_CARD = "tv_library_card_"
internal const val TV_LIBRARIES_PLAYLISTS = "tv_libraries_playlists"

/** Playlists' card: its key in the grid and in the page's focus memory. */
private const val PLAYLISTS_KEY = "playlists"

private const val COLUMNS = 3
private val CardHeight = 80.dp
