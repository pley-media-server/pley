package app.pley.android.ui.phone.admin.libraries

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.detectDragGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.CustomAccessibilityAction
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.customActions
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.zIndex
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.icon.Icon
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.model.Library
import app.pley.android.model.admin.LibraryScan
import app.pley.android.screens.PleyKey
import app.pley.android.screens.admin.AdminLoad
import app.pley.android.screens.admin.AdminSection
import app.pley.android.screens.admin.libraries.LibrariesAdminViewModel
import app.pley.android.screens.admin.libraries.LibraryAdmin
import app.pley.android.screens.admin.valueOrNull
import app.pley.android.screens.playlists.Playlists
import app.pley.android.ui.phone.R
import app.pley.android.ui.phone.admin.AdminEvents
import app.pley.android.ui.phone.admin.WhileShown
import app.pley.android.ui.phone.admin.adminWidth
import app.pley.android.ui.phone.home.kindIcon
import app.pley.android.ui.phone.kit.DelayedSpinner
import app.pley.android.ui.phone.kit.EmptyState
import app.pley.android.ui.phone.kit.ErrorState
import app.pley.android.ui.phone.kit.PleyIconButton
import app.pley.android.ui.phone.kit.PleyTopAppBar
import app.pley.android.ui.phone.kit.PrimaryButton
import app.pley.android.ui.phone.kit.RowDrag
import app.pley.android.ui.phone.kit.TonalButton
import app.pley.android.ui.phone.shell.LocalPhoneNavigator
import app.pley.android.ui.phone.theme.LocalPhoneMetrics

/**
 * Libraries, of managing the server: [id] `null` is the list (Settings →
 * Libraries); a library's id, or [PleyKey.Manage.NEW], its page
 * ([LibraryAdminScreen]).
 */
@Composable
internal fun LibrariesScreen(id: String?) {
    if (id == null) LibrariesList() else LibraryAdminScreen(id)
}

/**
 * The libraries in the order everyone sees them in, each with its kind,
 * count, folders and last scan, a scan under way as it goes, and a Scan
 * button; dragged by their handles to reorder them; Scan all, and Add
 * library in the bar. A library opens its page.
 */
@Composable
private fun LibrariesList() {
    val vm: LibrariesAdminViewModel = viewModel(factory = LibrariesAdminViewModel.factory)
    val load by vm.libraries.state.collectAsStateWithLifecycle()
    val scans by vm.scans.state.collectAsStateWithLifecycle()
    val navigator = LocalPhoneNavigator.current
    val resources = LocalResources.current
    val list = rememberLazyListState()
    AdminEvents(vm) { librariesSaid(resources, it) }
    WhileShown(vm) { vm.watchScans() }
    val add = { navigator.open(PleyKey.Manage(AdminSection.Libraries, PleyKey.Manage.NEW)) }
    Column(Modifier.fillMaxSize().testTag(LIBRARIES_PAGE)) {
        PleyTopAppBar(
            title = stringResource(R.string.admin_libraries_title),
            onBack = navigator::back,
            scrolled = { list.canScrollBackward },
            actions = { PleyIconButton(PleyIcons.Plus, stringResource(R.string.admin_libraries_add), onClick = add) },
        )
        val libraries = load.valueOrNull
        when {
            libraries != null -> LibraryList(libraries, scans.valueOrNull.orEmpty(), list, vm, add)
            load is AdminLoad.Failed -> ErrorState(stringResource(R.string.admin_libraries_couldnt_load), (load as AdminLoad.Failed).error, vm.libraries::reload)
            else -> Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
                DelayedSpinner(stringResource(R.string.admin_loading))
            }
        }
    }
}

@Composable
private fun LibraryList(libraries: List<Library>, scans: List<LibraryScan>, list: LazyListState, vm: LibrariesAdminViewModel, add: () -> Unit) {
    val navigator = LocalPhoneNavigator.current
    val metrics = LocalPhoneMetrics.current
    val reorderable = libraries.size > 1
    // While a row is dragged, the order it is dragged into; the list's own once it has it.
    var local by remember { mutableStateOf<List<Long>?>(null) }
    val scope = rememberCoroutineScope()
    val drag = remember(list) { RowDrag(list, scope) }
    val shown = remember(libraries, local) {
        val ids = local ?: return@remember libraries
        val byId = libraries.associateBy { it.id }
        ids.mapNotNull(byId::get) + libraries.filter { it.id !in ids }
    }
    LaunchedEffect(libraries) { if (drag.key == null) local = null }
    drag.onMove = { from, to ->
        val ids = shown.map { it.id }
        val f = from - FIRST_ROW
        val t = to - FIRST_ROW
        if (f in ids.indices && t in ids.indices) local = Playlists.moved(ids, f, t)
    }
    drag.onDrop = { key ->
        val from = libraries.indexOfFirst { it.id == key }
        val to = local?.indexOf(key) ?: -1
        if (from >= 0 && to >= 0 && from != to) vm.move(from, to) else local = null
    }
    LazyColumn(
        state = list,
        modifier = Modifier.fillMaxSize(),
        contentPadding = PaddingValues(start = metrics.marginStart, end = metrics.marginEnd, top = 8.dp, bottom = 32.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        item(key = "intro", contentType = "intro") {
            val intro = stringResource(R.string.admin_libraries_description) +
                if (reorderable) " " + stringResource(R.string.admin_libraries_shared_order) else ""
            Text(intro, style = MaterialTheme.typography.bodyMedium, color = PleyColors.Muted, modifier = Modifier.adminWidth.padding(start = 4.dp, bottom = 16.dp))
        }
        if (libraries.isEmpty()) {
            item(key = "empty", contentType = "empty") {
                EmptyState(
                    title = stringResource(R.string.admin_libraries_empty_title),
                    body = stringResource(R.string.admin_libraries_empty_description),
                    icon = PleyIcons.FolderPlus,
                    action = { PrimaryButton(stringResource(R.string.admin_libraries_add), onClick = add) },
                )
            }
            return@LazyColumn
        }
        item(key = "scan-all", contentType = "action") {
            Box(Modifier.adminWidth.padding(bottom = 16.dp)) {
                TonalButton(stringResource(R.string.admin_libraries_scan_all), onClick = { vm.scanAll() }, icon = PleyIcons.RefreshCw, modifier = Modifier.testTag(LIBRARIES_SCAN_ALL))
            }
        }
        itemsIndexed(shown, key = { _, library -> library.id }, contentType = { _, _ -> "library" }) { index, library ->
            val lifted = drag.key == library.id
            Box(
                Modifier
                    .adminWidth
                    .padding(bottom = 12.dp)
                    .then(if (lifted) Modifier.zIndex(1f) else Modifier.animateItem())
                    .graphicsLayer { translationY = if (drag.key == library.id) drag.offset else 0f },
            ) {
                LibraryCard(
                    library = library,
                    scan = LibraryAdmin.scanOf(library.id, scans),
                    lifted = lifted,
                    drag = drag.takeIf { reorderable },
                    first = index == 0,
                    last = index == shown.lastIndex,
                    onOpen = { navigator.open(PleyKey.Manage(AdminSection.Libraries, library.id.toString())) },
                    onScan = { vm.scan(library, force = false) },
                    onMoveBy = { by -> vm.moveBy(library.id, by) },
                )
            }
        }
    }
}

/**
 * A library: its handle (with [drag], when there are others to order),
 * its kind's tile, its name over its count and last scan, its folders, a
 * scan under way, and Scan. Lifted while dragged; a screen reader moves it
 * with actions instead.
 */
@Composable
private fun LibraryCard(
    library: Library,
    scan: LibraryScan?,
    lifted: Boolean,
    drag: RowDrag?,
    first: Boolean,
    last: Boolean,
    onOpen: () -> Unit,
    onScan: () -> Unit,
    onMoveBy: (Int) -> Unit,
) {
    val running = library.scanning || scan?.running == true
    val moveUp = stringResource(R.string.admin_libraries_move_up, library.name)
    val moveDown = stringResource(R.string.admin_libraries_move_down, library.name)
    Row(
        Modifier
            .clip(PleyShape.Card)
            .background(if (lifted) PleyColors.Surface3 else PleyColors.Surface)
            .clickable(role = Role.Button, onClick = onOpen)
            .semantics {
                if (drag != null) {
                    customActions = buildList {
                        if (!first) add(CustomAccessibilityAction(moveUp) { onMoveBy(-1); true })
                        if (!last) add(CustomAccessibilityAction(moveDown) { onMoveBy(1); true })
                    }
                }
            }
            .padding(start = if (drag != null) 0.dp else 16.dp, end = 4.dp, top = 12.dp, bottom = 12.dp)
            .testTag(LIBRARY_CARD + library.id),
        verticalAlignment = Alignment.Top,
    ) {
        if (drag != null) {
            Box(
                Modifier
                    .pointerInput(library.id) {
                        detectDragGestures(
                            onDragStart = { drag.start(library.id) },
                            onDragEnd = drag::end,
                            onDragCancel = drag::end,
                            onDrag = { change, amount ->
                                change.consume()
                                drag.by(amount.y)
                            },
                        )
                    }
                    .size(40.dp, 48.dp)
                    .testTag(LIBRARY_HANDLE + library.id),
                contentAlignment = Alignment.Center,
            ) {
                Icon(PleyIcons.GripVertical, contentDescription = null, size = 20.dp, tint = PleyColors.Subtle)
            }
        }
        Box(Modifier.padding(top = 4.dp).size(40.dp).background(PleyColors.Surface3, TileShape), contentAlignment = Alignment.Center) {
            Icon(kindIcon(library.kind), contentDescription = null, size = 22.dp, tint = PleyColors.AccentBright)
        }
        Spacer(Modifier.width(16.dp))
        Column(Modifier.weight(1f).padding(top = 2.dp), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(library.name, style = MaterialTheme.typography.titleMedium, color = PleyColors.Fg, maxLines = 1, overflow = TextOverflow.Ellipsis)
            Text(libraryLine(library), style = MaterialTheme.typography.bodySmall, color = PleyColors.Muted)
            if (library.paths.isNotEmpty()) Spacer(Modifier.height(2.dp))
            for (path in library.paths) {
                Text(
                    path,
                    style = MaterialTheme.typography.bodySmall,
                    fontFamily = FontFamily.Monospace,
                    color = PleyColors.Subtle,
                    maxLines = 1,
                    overflow = TextOverflow.StartEllipsis,
                )
            }
            if (running) ScanProgress(scan, Modifier.padding(top = 10.dp, end = 8.dp))
        }
        Box(Modifier.heightIn(min = 48.dp), contentAlignment = Alignment.Center) {
            PleyIconButton(
                PleyIcons.ScanSearch,
                stringResource(R.string.admin_libraries_scan),
                onClick = onScan,
                enabled = !running,
                modifier = Modifier.testTag(LIBRARY_SCAN + library.id),
            )
        }
    }
}

/** The lazy position of the first library: the intro and Scan all come first. */
private const val FIRST_ROW = 2

private val TileShape = RoundedCornerShape(10.dp)

internal const val LIBRARIES_PAGE = "pley.admin.libraries"
internal const val LIBRARIES_SCAN_ALL = "pley.admin.libraries.scanAll"
internal const val LIBRARY_CARD = "pley.admin.libraries.card."
internal const val LIBRARY_HANDLE = "pley.admin.libraries.handle."
internal const val LIBRARY_SCAN = "pley.admin.libraries.scan."
