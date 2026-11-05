@file:OptIn(ExperimentalMaterial3Api::class)

package app.pley.android.ui.phone.home

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.detectDragGestures
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.pulltorefresh.PullToRefreshBox
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.CustomAccessibilityAction
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.customActions
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.zIndex
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import app.pley.android.data.Loadable
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.icon.Icon
import app.pley.android.designsystem.icon.PleyIcon
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.designsystem.text.asString
import app.pley.android.designsystem.tile.rememberSkeleton
import app.pley.android.designsystem.tile.skeleton
import app.pley.android.model.LibraryKind
import app.pley.android.screens.PleyGraphOwner
import app.pley.android.screens.PleyKey
import app.pley.android.screens.home.LibrariesEvent
import app.pley.android.screens.home.LibrariesState
import app.pley.android.screens.home.LibrariesViewModel
import app.pley.android.screens.home.LibraryEntry
import app.pley.android.screens.livetv.LiveTvEntryViewModel
import app.pley.android.screens.playlists.Playlists
import app.pley.android.ui.phone.R
import app.pley.android.ui.phone.kit.EmptyState
import app.pley.android.ui.phone.kit.ErrorState
import app.pley.android.ui.phone.kit.PleyIconButton
import app.pley.android.ui.phone.kit.PleyTopAppBar
import app.pley.android.ui.phone.kit.RowDrag
import app.pley.android.ui.phone.shell.LocalPhoneNavigator
import app.pley.android.ui.phone.shell.LocalPhoneShell
import app.pley.android.ui.phone.shell.ReportLoad
import app.pley.android.ui.phone.theme.LocalPhoneMetrics
import kotlinx.coroutines.delay

/**
 * The Libraries tab: every library this profile sees, in the server's
 * order, each with its kind's icon, its name, how much it holds and a
 * spinner while the server scans it. A library opens its page. Pull to
 * refresh asks the server again.
 *
 * Sort (in the top bar, with two libraries or more) puts them in the
 * profile's own order: each row has a handle to drag it by (and Move up
 * and Move down for TalkBack), every move is saved at once, and "Use the
 * server's order" goes back. Done or Back ends it.
 */
@Composable
internal fun PhoneLibrariesScreen(vm: LibrariesViewModel = viewModel(factory = LibrariesViewModel.Factory)) {
    val state by vm.state.collectAsStateWithLifecycle()
    val live = liveChannels()
    val navigator = LocalPhoneNavigator.current
    val shell = LocalPhoneShell.current
    val resources = LocalResources.current
    val list = rememberLazyListState()
    var sorting by rememberSaveable { mutableStateOf(false) }
    val sortable = (state as? LibrariesState.Ready)?.sortable == true
    val inSort = sorting && sortable
    // Nothing left to sort (a library went, or this sign-in may not): the list as it is.
    LaunchedEffect(state) { if ((state as? LibrariesState.Ready)?.sortable == false) sorting = false }
    BackHandler(enabled = inSort) { sorting = false }
    LaunchedEffect(vm) {
        vm.events.collect { event ->
            when (event) {
                is LibrariesEvent.Failed -> shell.report(event.error)
                LibrariesEvent.NeedsNewerServer -> shell.toasts.show(resources.getString(R.string.home_libraries_sort_needs_newer_server))
            }
        }
    }
    // The offline banner: up while the list is what was saved, down once the server answers.
    ReportLoad(
        when (val s = state) {
            is LibrariesState.Ready -> s.error?.let { Loadable.Failed(it, Unit) } ?: Loadable.Ready(Unit, s.fresh)
            is LibrariesState.Failed -> Loadable.Failed(s.error, Unit)
            LibrariesState.Loading -> null
        },
    )
    Column(Modifier.fillMaxSize()) {
        PleyTopAppBar(
            title = stringResource(if (inSort) R.string.home_libraries_sort else R.string.home_libraries_title),
            scrolled = { list.canScrollBackward },
            actions = {
                when {
                    inSort -> TextButton(
                        onClick = { sorting = false },
                        colors = ButtonDefaults.textButtonColors(contentColor = PleyColors.AccentBright),
                    ) { Text(stringResource(R.string.home_libraries_sort_done), style = MaterialTheme.typography.labelLarge) }
                    sortable -> PleyIconButton(PleyIcons.ArrowUpDown, stringResource(R.string.home_libraries_sort), onClick = { sorting = true })
                }
            },
        )
        PullToRefreshBox(
            isRefreshing = (state as? LibrariesState.Ready)?.refreshing == true,
            onRefresh = vm::refresh,
            modifier = Modifier.weight(1f).fillMaxWidth(),
        ) {
            when (val s = state) {
                LibrariesState.Loading -> LibrariesSkeleton()
                is LibrariesState.Failed -> Box(Modifier.fillMaxSize().verticalScroll(rememberScrollState())) {
                    ErrorState(stringResource(R.string.home_libraries_failed), s.error, onRetry = vm::refresh)
                }
                is LibrariesState.Ready -> when {
                    s.libraries.isEmpty() && live == null -> Box(Modifier.fillMaxSize().verticalScroll(rememberScrollState())) {
                        EmptyState(
                            stringResource(R.string.home_no_libraries),
                            stringResource(R.string.home_no_libraries_body),
                            icon = PleyIcons.FolderPlus,
                        )
                    }
                    inSort -> SortList(s.libraries, list, vm)
                    else -> LazyColumn(state = list, modifier = Modifier.fillMaxSize(), contentPadding = PaddingValues(vertical = 8.dp)) {
                        if (live != null) {
                            item(key = "live", contentType = "library") { LiveTvRow(live, onClick = { navigator.open(PleyKey.LiveTv) }) }
                        }
                        items(s.libraries, key = { it.library.id }, contentType = { "library" }) { entry ->
                            LibraryRow(entry, onClick = { navigator.open(PleyKey.Library(entry.library.id)) })
                        }
                    }
                }
            }
        }
    }
}

/** One library: a 40 dp tile with its kind's icon, the name and count, a spinner while it is scanned, else a chevron. */
@Composable
private fun LibraryRow(entry: LibraryEntry, onClick: () -> Unit) {
    val metrics = LocalPhoneMetrics.current
    val scanning = stringResource(R.string.home_libraries_scanning)
    Row(
        Modifier
            .fillMaxWidth()
            .heightIn(min = 72.dp)
            .clickable(role = Role.Button, onClick = onClick)
            .padding(start = metrics.marginStart, end = metrics.marginEnd, top = 12.dp, bottom = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        KindAndName(entry)
        Spacer(Modifier.width(12.dp))
        if (entry.scanning) {
            CircularProgressIndicator(
                modifier = Modifier.size(20.dp).semantics { contentDescription = scanning },
                color = PleyColors.Accent,
                trackColor = PleyColors.Surface4,
                strokeWidth = 2.dp,
            )
        } else {
            Icon(PleyIcons.ChevronRight, contentDescription = null, size = 20.dp, tint = PleyColors.Muted)
        }
    }
}

/**
 * How many channels Live TV has, when it shows (the web's sidebar entry:
 * open to this profile and with channels); `null` otherwise, or while
 * unknown. Graphs without Live TV (the shell's tests) have none.
 */
@Composable
private fun liveChannels(): Int? {
    if (LocalContext.current.applicationContext !is PleyGraphOwner) return null
    val vm: LiveTvEntryViewModel = viewModel(factory = LiveTvEntryViewModel.Factory)
    val channels by vm.channels.collectAsStateWithLifecycle()
    return channels?.takeIf { it > 0 }
}

/** Live TV, first in the list: the TV icon's tile, then "Live TV" over how many channels it has. */
@Composable
private fun LiveTvRow(channels: Int, onClick: () -> Unit) {
    val metrics = LocalPhoneMetrics.current
    Row(
        Modifier
            .fillMaxWidth()
            .heightIn(min = 72.dp)
            .clickable(role = Role.Button, onClick = onClick)
            .padding(start = metrics.marginStart, end = metrics.marginEnd, top = 12.dp, bottom = 12.dp)
            .testTag(LIBRARIES_LIVE_TV),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(Modifier.size(40.dp).background(PleyColors.Surface3, IconShape), contentAlignment = Alignment.Center) {
            Icon(PleyIcons.TvMinimal, contentDescription = null, size = 22.dp, tint = PleyColors.Fg)
        }
        Spacer(Modifier.width(16.dp))
        Column(Modifier.weight(1f)) {
            Text(stringResource(R.string.livetv_title), style = MaterialTheme.typography.titleMedium, color = PleyColors.Fg, maxLines = 1)
            Text(
                pluralStringResource(R.plurals.livetv_entry_channels, channels, channels),
                style = MaterialTheme.typography.bodyMedium,
                color = PleyColors.Muted,
                maxLines = 1,
            )
        }
        Spacer(Modifier.width(12.dp))
        Icon(PleyIcons.ChevronRight, contentDescription = null, size = 20.dp, tint = PleyColors.Muted)
    }
}

/** The kind's tile, then the name over the count. */
@Composable
private fun RowScope.KindAndName(entry: LibraryEntry) {
    Box(Modifier.size(40.dp).background(PleyColors.Surface3, IconShape), contentAlignment = Alignment.Center) {
        Icon(kindIcon(entry.library.kind), contentDescription = null, size = 22.dp, tint = PleyColors.Fg)
    }
    Spacer(Modifier.width(16.dp))
    Column(Modifier.weight(1f)) {
        Text(entry.library.name, style = MaterialTheme.typography.titleMedium, color = PleyColors.Fg, maxLines = 1, overflow = TextOverflow.Ellipsis)
        Text(entry.countText.asString(), style = MaterialTheme.typography.bodyMedium, color = PleyColors.Muted, maxLines = 1)
    }
}

/**
 * The libraries while they are sorted: what to do, a row per library with
 * its handle, and "Use the server's order". A dragged row keeps the order
 * it is dragged into until the list has it.
 */
@Composable
private fun SortList(entries: List<LibraryEntry>, list: LazyListState, vm: LibrariesViewModel) {
    var local by remember { mutableStateOf<List<Long>?>(null) }
    val scope = rememberCoroutineScope()
    val drag = remember(list) { RowDrag(list, scope) }
    val shown = remember(entries, local) {
        val ids = local ?: return@remember entries
        val byId = entries.associateBy { it.library.id }
        ids.mapNotNull(byId::get) + entries.filter { it.library.id !in ids }
    }
    // The list's own order came: what was dragged is the list's now (or was refused).
    LaunchedEffect(entries) { if (drag.key == null) local = null }
    drag.onMove = { from, to ->
        val ids = shown.map { it.library.id }
        val f = from - FIRST_ROW
        val t = to - FIRST_ROW
        if (f in ids.indices && t in ids.indices) local = Playlists.moved(ids, f, t)
    }
    drag.onDrop = { key ->
        val from = entries.indexOfFirst { it.library.id == key }
        val to = local?.indexOf(key) ?: -1
        if (from >= 0 && to >= 0 && from != to) vm.move(from, to) else local = null
    }
    val metrics = LocalPhoneMetrics.current
    LazyColumn(state = list, modifier = Modifier.fillMaxSize().testTag(LIBRARIES_SORT), contentPadding = PaddingValues(bottom = 24.dp)) {
        item(key = "hint", contentType = "hint") {
            Text(
                stringResource(R.string.home_libraries_sort_hint),
                style = MaterialTheme.typography.bodyMedium,
                color = PleyColors.Muted,
                modifier = Modifier.padding(start = metrics.marginStart, end = metrics.marginEnd, top = 4.dp, bottom = 12.dp),
            )
        }
        itemsIndexed(shown, key = { _, entry -> entry.library.id }, contentType = { _, _ -> "library" }) { index, entry ->
            val lifted = drag.key == entry.library.id
            Box(
                Modifier
                    .then(if (lifted) Modifier.zIndex(1f) else Modifier.animateItem())
                    .graphicsLayer { translationY = if (drag.key == entry.library.id) drag.offset else 0f },
            ) {
                SortRow(
                    entry = entry,
                    lifted = lifted,
                    first = index == 0,
                    last = index == shown.lastIndex,
                    drag = drag,
                    onMoveBy = { by -> vm.moveBy(entry.library.id, by) },
                )
            }
        }
        item(key = "server-order", contentType = "action") {
            TextButton(
                onClick = vm::useServerOrder,
                colors = ButtonDefaults.textButtonColors(contentColor = PleyColors.AccentBright),
                modifier = Modifier.padding(start = (metrics.marginStart - 12.dp).coerceAtLeast(0.dp), top = 12.dp),
            ) { Text(stringResource(R.string.home_libraries_server_order), style = MaterialTheme.typography.labelLarge) }
        }
    }
}

/** A library being sorted: its handle, then its tile, name and count; lifted while dragged. */
@Composable
private fun SortRow(entry: LibraryEntry, lifted: Boolean, first: Boolean, last: Boolean, drag: RowDrag, onMoveBy: (Int) -> Unit) {
    val metrics = LocalPhoneMetrics.current
    val moveUp = stringResource(R.string.home_libraries_move_up)
    val moveDown = stringResource(R.string.home_libraries_move_down)
    Row(
        Modifier
            .fillMaxWidth()
            .background(if (lifted) PleyColors.Surface3 else PleyColors.Bg)
            .heightIn(min = 72.dp)
            .padding(end = metrics.marginEnd, top = 12.dp, bottom = 12.dp)
            .semantics(mergeDescendants = true) {
                customActions = buildList {
                    if (!first) add(CustomAccessibilityAction(moveUp) { onMoveBy(-1); true })
                    if (!last) add(CustomAccessibilityAction(moveDown) { onMoveBy(1); true })
                }
            }
            .testTag(LIBRARY_SORT_ROW + entry.library.id),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        val handle = Modifier.pointerInput(entry.library.id) {
            detectDragGestures(
                onDragStart = { drag.start(entry.library.id) },
                onDragEnd = drag::end,
                onDragCancel = drag::end,
                onDrag = { change, amount ->
                    change.consume()
                    drag.by(amount.y)
                },
            )
        }
        // The handle's 48 dp reaches the margin's edge.
        Box(
            handle.padding(start = (metrics.marginStart - 12.dp).coerceAtLeast(0.dp)).size(48.dp).testTag(LIBRARY_HANDLE + entry.library.id),
            contentAlignment = Alignment.Center,
        ) {
            Icon(PleyIcons.GripVertical, contentDescription = null, size = 20.dp, tint = PleyColors.Subtle)
        }
        Spacer(Modifier.width(4.dp))
        KindAndName(entry)
    }
}

/** Rows in the list's shape while nothing was saved, after a moment. */
@Composable
private fun LibrariesSkeleton() {
    var shown by remember { mutableStateOf(false) }
    LaunchedEffect(Unit) {
        delay(400)
        shown = true
    }
    val loading = stringResource(R.string.home_libraries_loading)
    Column(Modifier.fillMaxSize().padding(vertical = 8.dp).semantics { contentDescription = loading }) {
        if (!shown) return@Column
        val state = rememberSkeleton(animated = true)
        val metrics = LocalPhoneMetrics.current
        repeat(4) {
            Row(Modifier.fillMaxWidth().height(72.dp).padding(start = metrics.marginStart, end = metrics.marginEnd), verticalAlignment = Alignment.CenterVertically) {
                Box(Modifier.size(40.dp).skeleton(state, IconShape))
                Spacer(Modifier.width(16.dp))
                Column {
                    Box(Modifier.size(140.dp, 16.dp).skeleton(state, BarShape))
                    Spacer(Modifier.height(8.dp))
                    Box(Modifier.size(80.dp, 12.dp).skeleton(state, BarShape))
                }
            }
        }
    }
}

/** A library's icon by kind, as the TV's rail draws it. */
internal fun kindIcon(kind: LibraryKind): PleyIcon = when (kind) {
    LibraryKind.Movies -> PleyIcons.Clapperboard
    LibraryKind.Shows -> PleyIcons.Tv
    LibraryKind.Music -> PleyIcons.Music
    LibraryKind.Unknown -> PleyIcons.Film
}

/** The sorted list, and a library's row and handle in it (+ its id). */
internal const val LIBRARIES_SORT = "libraries_sort"
internal const val LIBRARIES_LIVE_TV = "libraries_live_tv"
internal const val LIBRARY_SORT_ROW = "library_sort_row_"
internal const val LIBRARY_HANDLE = "library_handle_"

/** The lazy position of the first library while sorting: the hint comes first. */
private const val FIRST_ROW = 1

private val IconShape = RoundedCornerShape(10.dp)
private val BarShape = RoundedCornerShape(4.dp)
