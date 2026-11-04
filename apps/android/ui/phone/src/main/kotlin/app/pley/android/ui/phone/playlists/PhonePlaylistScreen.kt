package app.pley.android.ui.phone.playlists

import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.gestures.detectDragGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.SwipeToDismissBox
import androidx.compose.material3.SwipeToDismissBoxState
import androidx.compose.material3.SwipeToDismissBoxValue
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.CustomAccessibilityAction
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.customActions
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.TextRange
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.input.TextFieldValue
import androidx.compose.ui.text.style.LineBreak
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.zIndex
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import app.pley.android.data.Loadable
import app.pley.android.data.valueOrNull
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.icon.Icon
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.designsystem.tile.ArtworkTile
import app.pley.android.designsystem.tile.ArtworkTileModel
import app.pley.android.designsystem.tile.TileKind
import app.pley.android.designsystem.tile.TileStyle
import app.pley.android.designsystem.tile.rememberSkeleton
import app.pley.android.designsystem.tile.skeleton
import app.pley.android.network.ApiException
import app.pley.android.screens.PleyKey
import app.pley.android.screens.playlists.PlaylistDeps
import app.pley.android.screens.playlists.PlaylistEvent
import app.pley.android.screens.playlists.PlaylistPage
import app.pley.android.screens.playlists.PlaylistRow
import app.pley.android.screens.playlists.PlaylistViewModel
import app.pley.android.screens.playlists.Playlists
import app.pley.android.screens.screensGraph
import app.pley.android.ui.phone.R
import app.pley.android.ui.phone.kit.ConfirmDialog
import app.pley.android.ui.phone.kit.EmptyState
import app.pley.android.ui.phone.kit.ErrorState
import app.pley.android.ui.phone.kit.PleyIconButton
import app.pley.android.ui.phone.kit.PleyTopAppBar
import app.pley.android.ui.phone.kit.PrimaryButton
import app.pley.android.ui.phone.remote.CastTarget
import app.pley.android.ui.phone.remote.PlayOnButton
import app.pley.android.ui.phone.kit.RowDrag
import app.pley.android.ui.phone.kit.TonalButton
import app.pley.android.ui.phone.library.PhoneTiles
import app.pley.android.ui.phone.shell.LocalPhoneNavigator
import app.pley.android.ui.phone.shell.LocalPhoneShell
import app.pley.android.ui.phone.shell.ReportLoad
import app.pley.android.ui.phone.theme.LocalPhoneMetrics

/**
 * A playlist's page on the phone (design spec §4.9; the web's
 * PlaylistPage): the cover, "Video playlist" with its Smart badge, the
 * title, "12 items · 3h 12m" with a smart playlist's filter in words,
 * Play, Shuffle, Rename and Delete; then its entries, each with its
 * number, thumbnail (with what was played), two lines and length.
 *
 * Tapping a thumbnail plays from it; tapping its words opens the item.
 * An entry is dragged by its handle to a new place, and swiped away to
 * remove it, with Undo in the toast. Screen readers get Move up, Move
 * down and Remove as actions. Smart playlists are read-only.
 */
@Composable
internal fun PhonePlaylistScreen(playlistId: Long) {
    val resources = LocalResources.current
    val vm: PlaylistViewModel<ArtworkTileModel> = viewModel {
        val graph = screensGraph()
        PlaylistViewModel(playlistId, PlaylistDeps.of(graph), graph.playerLauncher, PhoneTiles(resources))
    }
    val state by vm.state.collectAsStateWithLifecycle()
    val navigator = LocalPhoneNavigator.current
    val shell = LocalPhoneShell.current
    LaunchedEffect(vm) {
        vm.events.collect { event ->
            when (event) {
                is PlaylistEvent.Removed -> shell.toasts.show(
                    resources.getString(R.string.playlist_removed, event.title),
                    actionLabel = resources.getString(R.string.playlist_undo),
                    onAction = { vm.undo(event.entryId) },
                )
                PlaylistEvent.Deleted -> {
                    shell.toasts.show(resources.getString(R.string.playlist_deleted))
                    navigator.back()
                }
                is PlaylistEvent.Open -> navigator.open(event.key)
                is PlaylistEvent.Failed -> shell.report(event.error)
            }
        }
    }
    ReportLoad(state)
    PlaylistContent(
        state = state,
        actions = PlaylistActions(
            play = { from, shuffle -> vm.play(from, shuffle) },
            open = { row -> navigator.open(vm.opens(row)) },
            more = { row -> navigator.open(PleyKey.ItemActions(row.item.id)) },
            move = { from, to -> vm.move(from, to) },
            remove = { row -> vm.remove(row.entryId, undoable = true) },
            rename = vm::rename,
            delete = vm::delete,
            retry = vm::retry,
            back = navigator::back,
        ),
    )
}

/** What the page's controls do. */
internal class PlaylistActions(
    val play: (from: Int, shuffle: Boolean) -> Unit,
    val open: (PlaylistRow<ArtworkTileModel>) -> Unit,
    val more: (PlaylistRow<ArtworkTileModel>) -> Unit,
    val move: (from: Int, to: Int) -> Unit,
    val remove: (PlaylistRow<ArtworkTileModel>) -> Unit,
    val rename: (String) -> Boolean,
    val delete: () -> Unit,
    val retry: () -> Unit,
    val back: () -> Unit,
)

@Composable
internal fun PlaylistContent(state: Loadable<PlaylistPage<ArtworkTileModel>>, actions: PlaylistActions) {
    val page = state.valueOrNull
    val list = rememberLazyListState()
    var renaming by rememberSaveable { mutableStateOf(false) }
    var deleting by rememberSaveable { mutableStateOf(false) }
    // The top bar shows the title once the header has scrolled away.
    val pastHeader by remember(list) { derivedStateOf { list.firstVisibleItemIndex > 0 } }
    Column(Modifier.fillMaxSize().testTag(PLAYLIST_PAGE)) {
        PleyTopAppBar(
            title = if (page != null && pastHeader) page.title else "",
            onBack = actions.back,
            scrolled = { list.canScrollBackward },
        )
        Box(Modifier.weight(1f).fillMaxWidth()) {
            when {
                page == null && state is Loadable.Failed -> Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState())) {
                    if ((state.error as? ApiException)?.status == 404) {
                        EmptyState(
                            title = stringResource(R.string.playlist_not_found),
                            body = stringResource(R.string.playlist_not_found_body),
                            icon = PleyIcons.ListVideo,
                        )
                    } else {
                        ErrorState(stringResource(R.string.playlist_failed), state.error, onRetry = actions.retry)
                    }
                }
                page == null -> Skeleton()
                else -> Entries(page, list, actions, onRename = { renaming = true }, onDelete = { deleting = true })
            }
        }
    }
    if (page != null && renaming) {
        RenameDialog(page.title, onSave = { if (actions.rename(it)) renaming = false }, onDismiss = { renaming = false })
    }
    if (page != null && deleting) {
        ConfirmDialog(
            title = stringResource(R.string.playlist_delete_title),
            body = stringResource(R.string.playlist_delete_body, page.title),
            confirmLabel = stringResource(R.string.playlist_delete_confirm),
            onConfirm = {
                deleting = false
                actions.delete()
            },
            onDismiss = { deleting = false },
            destructive = true,
        )
    }
}

/** The header and the entries, which keep the order being dragged until the page has it. */
@Composable
private fun Entries(
    page: PlaylistPage<ArtworkTileModel>,
    list: LazyListState,
    actions: PlaylistActions,
    onRename: () -> Unit,
    onDelete: () -> Unit,
) {
    val rows = page.rows
    var local by remember { mutableStateOf<List<Long>?>(null) }
    val scope = rememberCoroutineScope()
    val drag = remember(list) { RowDrag(list, scope) }
    val shown = remember(rows, local) {
        val ids = local ?: return@remember rows
        val byId = rows.associateBy { it.entryId }
        ids.mapNotNull(byId::get) + rows.filter { it.entryId !in ids }
    }
    // The page's own order came: what was dragged is the page's now (or was refused).
    LaunchedEffect(rows) { if (drag.key == null) local = null }
    drag.onMove = { from, to ->
        val ids = shown.map { it.entryId }
        val f = from - FIRST_ROW
        val t = to - FIRST_ROW
        if (f in ids.indices && t in ids.indices) local = Playlists.moved(ids, f, t)
    }
    drag.onDrop = { key ->
        val from = rows.indexOfFirst { it.entryId == key }
        val to = local?.indexOf(key) ?: -1
        if (from >= 0 && to >= 0 && from != to) actions.move(from, to) else local = null
    }
    val metrics = LocalPhoneMetrics.current
    LazyColumn(
        state = list,
        modifier = Modifier.fillMaxSize().testTag(PLAYLIST_ENTRIES),
        contentPadding = PaddingValues(bottom = 32.dp),
    ) {
        item(key = "header", contentType = "header") {
            Header(page, actions, onRename, onDelete)
        }
        if (shown.isEmpty()) {
            item(key = "empty", contentType = "empty") {
                Text(
                    stringResource(
                        when {
                            page.smart -> R.string.playlist_empty_smart
                            page.video -> R.string.playlist_empty_video
                            else -> R.string.playlist_empty_music
                        },
                    ),
                    style = MaterialTheme.typography.bodyMedium,
                    color = PleyColors.Muted,
                    modifier = Modifier.padding(start = metrics.marginStart, end = metrics.marginEnd, top = 8.dp, bottom = 8.dp),
                )
            }
        }
        itemsIndexed(shown, key = { _, row -> row.entryId }, contentType = { _, _ -> "entry" }) { index, row ->
            val dragged = drag.key == row.entryId
            Box(
                Modifier
                    .then(if (dragged) Modifier.zIndex(1f) else Modifier.animateItem())
                    .graphicsLayer { translationY = if (drag.key == row.entryId) drag.offset else 0f },
            ) {
                EntryRow(
                    row = row,
                    number = index + 1,
                    video = page.video,
                    editable = page.editable,
                    lifted = dragged,
                    last = index == shown.lastIndex,
                    actions = actions,
                    onPlay = { actions.play(rows.indexOfFirst { it.entryId == row.entryId }.coerceAtLeast(0), false) },
                    onMoveBy = { by ->
                        val from = rows.indexOfFirst { it.entryId == row.entryId }
                        if (from >= 0) actions.move(from, from + by)
                    },
                    drag = drag,
                )
            }
        }
    }
}

/** The cover, what it plays, the title, its facts, and the buttons. */
@Composable
private fun Header(page: PlaylistPage<ArtworkTileModel>, actions: PlaylistActions, onRename: () -> Unit, onDelete: () -> Unit) {
    val metrics = LocalPhoneMetrics.current
    Column(Modifier.fillMaxWidth().padding(start = metrics.marginStart, end = metrics.marginEnd, top = 8.dp, bottom = 20.dp)) {
        PlaylistArt(
            artwork = page.artwork,
            origin = page.origin,
            video = page.video,
            title = page.title,
            modifier = Modifier.size(COVER),
            shape = PleyShape.Card,
            glyph = 56.dp,
        )
        Spacer(Modifier.height(20.dp))
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Icon(if (page.video) PleyIcons.ListVideo else PleyIcons.ListMusic, contentDescription = null, size = 16.dp, tint = PleyColors.Muted)
            Text(
                stringResource(if (page.video) R.string.playlist_video else R.string.playlist_music),
                style = MaterialTheme.typography.labelMedium,
                color = PleyColors.Muted,
            )
            if (page.smart) SmartBadge(stringResource(R.string.playlists_smart))
        }
        Spacer(Modifier.height(6.dp))
        Text(
            page.title,
            style = MaterialTheme.typography.headlineMedium.copy(lineBreak = LineBreak.Heading),
            color = PleyColors.Fg,
            modifier = Modifier.semantics { heading() },
        )
        val facts = (listOf(page.meta) + page.notes).filter { it.isNotEmpty() }
        if (facts.isNotEmpty()) {
            Spacer(Modifier.height(6.dp))
            Text(facts.joinToString(" · "), style = MaterialTheme.typography.labelMedium, color = PleyColors.Muted)
        }
        Spacer(Modifier.height(20.dp))
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            PrimaryButton(
                stringResource(R.string.playlist_play),
                onClick = { actions.play(0, false) },
                icon = PleyIcons.Play,
                enabled = page.canPlay,
                modifier = Modifier.weight(1f),
            )
            TonalButton(
                stringResource(R.string.playlist_shuffle),
                onClick = { actions.play(0, true) },
                icon = PleyIcons.Shuffle,
                enabled = page.canShuffle,
                modifier = Modifier.height(48.dp),
            )
            if (page.editable) {
                RoundAction(PleyIcons.Pencil, stringResource(R.string.playlist_rename), onRename)
            }
            RoundAction(TrashIcon, stringResource(R.string.playlist_delete), onDelete)
        }
        // Under Play, with its words: the row beside it has no room left on a phone.
        if (page.canPlay) {
            PlayOnButton(CastTarget(playlistId = page.id, shuffles = page.canShuffle), Modifier.padding(top = 8.dp), labelled = true)
        }
    }
}

/** A 48 dp round button on [surface3][PleyColors.Surface3]: Rename, Delete. */
@Composable
private fun RoundAction(icon: app.pley.android.designsystem.icon.PleyIcon, label: String, onClick: () -> Unit) {
    Box(Modifier.size(48.dp).background(PleyColors.Surface3, PleyShape.Circle), contentAlignment = Alignment.Center) {
        PleyIconButton(icon, label, onClick = onClick)
    }
}

/**
 * One entry: its drag handle, number, thumbnail, two lines and length.
 * The thumbnail plays, the words open the item, a long press opens its
 * actions; editable entries swipe away to the left.
 */
@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun EntryRow(
    row: PlaylistRow<ArtworkTileModel>,
    number: Int,
    video: Boolean,
    editable: Boolean,
    lifted: Boolean,
    last: Boolean,
    actions: PlaylistActions,
    onPlay: () -> Unit,
    onMoveBy: (Int) -> Unit,
    drag: RowDrag,
) {
    val metrics = LocalPhoneMetrics.current
    val moveUp = stringResource(R.string.playlist_move_up)
    val moveDown = stringResource(R.string.playlist_move_down)
    val remove = stringResource(R.string.playlist_remove)
    val playLabel = stringResource(R.string.playlist_play_entry, row.title)
    val content: @Composable () -> Unit = {
        Row(
            Modifier
                .fillMaxWidth()
                .background(if (lifted) PleyColors.Surface3 else PleyColors.Bg)
                .heightIn(min = 72.dp)
                .padding(start = if (editable) 0.dp else metrics.marginStart, end = metrics.marginEnd)
                .semantics {
                    if (editable) {
                        customActions = buildList {
                            if (number > 1) add(CustomAccessibilityAction(moveUp) { onMoveBy(-1); true })
                            if (!last) add(CustomAccessibilityAction(moveDown) { onMoveBy(1); true })
                            add(CustomAccessibilityAction(remove) { actions.remove(row); true })
                        }
                    }
                }
                .testTag(ENTRY_TAG + row.entryId),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            if (editable) {
                val handle = Modifier.pointerInput(row.entryId) {
                    detectDragGestures(
                        onDragStart = { drag.start(row.entryId) },
                        onDragEnd = drag::end,
                        onDragCancel = drag::end,
                        onDrag = { change, amount ->
                            change.consume()
                            drag.by(amount.y)
                        },
                    )
                }
                Box(handle.size(48.dp).testTag(HANDLE_TAG + row.entryId), contentAlignment = Alignment.Center) {
                    Icon(PleyIcons.GripVertical, contentDescription = null, size = 20.dp, tint = PleyColors.Subtle)
                }
            }
            Text(
                number.toString(),
                style = MaterialTheme.typography.bodyMedium,
                color = PleyColors.Subtle,
                textAlign = androidx.compose.ui.text.style.TextAlign.End,
                modifier = Modifier.width(24.dp),
            )
            Spacer(Modifier.width(12.dp))
            val kind = if (video) TileKind.Landscape else TileKind.Square
            ArtworkTile(
                row.tile,
                kind,
                TileStyle.Phone,
                Modifier
                    .width(if (video) 112.dp else 56.dp)
                    .aspectRatio(kind.aspectRatio)
                    .semantics { contentDescription = playLabel }
                    .combinedClickable(role = Role.Button, onClick = onPlay, onLongClick = { actions.more(row) }),
            )
            Spacer(Modifier.width(12.dp))
            Column(
                Modifier
                    .weight(1f)
                    .combinedClickable(role = Role.Button, onClick = { actions.open(row) }, onLongClick = { actions.more(row) })
                    .padding(vertical = 8.dp),
            ) {
                Text(row.title, style = MaterialTheme.typography.titleMedium, color = PleyColors.Fg, maxLines = 1, overflow = TextOverflow.Ellipsis)
                if (row.subtitle.isNotEmpty()) {
                    Text(row.subtitle, style = MaterialTheme.typography.bodySmall, color = PleyColors.Muted, maxLines = 1, overflow = TextOverflow.Ellipsis)
                }
            }
            if (row.duration.isNotEmpty()) {
                Spacer(Modifier.width(8.dp))
                Text(row.duration, style = MaterialTheme.typography.bodySmall, color = PleyColors.Muted)
            }
        }
    }
    if (!editable) {
        content()
        return
    }
    // Not saved with the row: a row put back by Undo comes back settled, not swiped away again.
    val swipe = remember(row.entryId) { SwipeToDismissBoxState(SwipeToDismissBoxValue.Settled, positionalThreshold = { it * SWIPE_THRESHOLD }) }
    SwipeToDismissBox(
        state = swipe,
        enableDismissFromStartToEnd = false,
        gesturesEnabled = !lifted,
        onDismiss = { value -> if (value == SwipeToDismissBoxValue.EndToStart) actions.remove(row) },
        backgroundContent = {
            Box(
                Modifier.fillMaxSize().background(PleyColors.Danger).padding(horizontal = 24.dp),
                contentAlignment = Alignment.CenterEnd,
            ) {
                Icon(TrashIcon, contentDescription = null, size = 22.dp, tint = PleyColors.Bg)
            }
        },
    ) { content() }
}

/** "Rename playlist": the title as it is, Save once it will do. */
@Composable
private fun RenameDialog(current: String, onSave: (String) -> Unit, onDismiss: () -> Unit) {
    var text by rememberSaveable { mutableStateOf(current) }
    val focus = remember { FocusRequester() }
    LaunchedEffect(Unit) { runCatching { focus.requestFocus() } }
    val ok = Playlists.check(text) == null
    AlertDialog(
        onDismissRequest = onDismiss,
        confirmButton = {
            TextButton(
                onClick = { onSave(text) },
                enabled = ok,
                colors = ButtonDefaults.textButtonColors(contentColor = PleyColors.AccentBright),
            ) { Text(stringResource(R.string.playlist_rename_save), style = MaterialTheme.typography.labelLarge) }
        },
        dismissButton = {
            TextButton(onClick = onDismiss, colors = ButtonDefaults.textButtonColors(contentColor = PleyColors.Fg)) {
                Text(stringResource(R.string.kit_cancel), style = MaterialTheme.typography.labelLarge)
            }
        },
        title = { Text(stringResource(R.string.playlist_rename), style = MaterialTheme.typography.headlineSmall) },
        text = {
            TitleField(
                value = text,
                onValueChange = { text = it },
                onDone = { if (ok) onSave(text) },
                modifier = Modifier.focusRequester(focus),
            )
        },
        shape = PleyShape.Dialog,
        containerColor = PleyColors.Surface2,
        titleContentColor = PleyColors.Fg,
        textContentColor = PleyColors.Muted,
        tonalElevation = 0.dp,
    )
}

/** A playlist's title: one line, at most 200 characters. */
@Composable
internal fun TitleField(
    value: String,
    onValueChange: (String) -> Unit,
    onDone: () -> Unit,
    modifier: Modifier = Modifier,
    placeholder: String? = null,
    label: String? = stringResource(R.string.playlist_form_title),
    enabled: Boolean = true,
) {
    // The text with its cursor, which starts at the end of what was there.
    var field by remember { mutableStateOf(TextFieldValue(value, TextRange(value.length))) }
    if (field.text != value) field = TextFieldValue(value, TextRange(value.length))
    OutlinedTextField(
        value = field,
        onValueChange = {
            if (it.text.length <= Playlists.TITLE_MAX) {
                field = it
                onValueChange(it.text)
            }
        },
        modifier = modifier.fillMaxWidth(),
        enabled = enabled,
        singleLine = true,
        label = label?.let { { Text(it) } },
        placeholder = placeholder?.let { { Text(it, maxLines = 1, overflow = TextOverflow.Ellipsis) } },
        textStyle = MaterialTheme.typography.bodyLarge,
        keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.Sentences, imeAction = ImeAction.Done),
        keyboardActions = androidx.compose.foundation.text.KeyboardActions(onDone = { onDone() }),
        shape = PleyShape.Field,
        colors = OutlinedTextFieldDefaults.colors(
            focusedBorderColor = PleyColors.AccentBright,
            unfocusedBorderColor = PleyColors.Control,
            focusedLabelColor = PleyColors.AccentBright,
            unfocusedLabelColor = PleyColors.Muted,
            cursorColor = PleyColors.AccentBright,
            focusedTextColor = PleyColors.Fg,
            unfocusedTextColor = PleyColors.Fg,
            focusedPlaceholderColor = PleyColors.Muted,
            unfocusedPlaceholderColor = PleyColors.Muted,
        ),
    )
}

/** "Smart": an accent-edged badge, the web's accent tone. */
@Composable
internal fun SmartBadge(text: String) {
    Box(
        Modifier
            .border(1.dp, PleyColors.Accent.copy(alpha = 0.6f), PleyShape.Badge)
            .background(PleyColors.PrimaryContainer, PleyShape.Badge)
            .padding(horizontal = 6.dp, vertical = 1.dp),
    ) {
        Text(text, style = MaterialTheme.typography.labelSmall, color = PleyColors.AccentBright)
    }
}

/** Still blocks in the header's and rows' shapes. */
@Composable
private fun Skeleton() {
    val skeleton = rememberSkeleton(animated = true)
    val metrics = LocalPhoneMetrics.current
    Column(Modifier.fillMaxSize().padding(start = metrics.marginStart, end = metrics.marginEnd, top = 8.dp, bottom = 8.dp)) {
        Box(Modifier.size(COVER).skeleton(skeleton, PleyShape.Card))
        Spacer(Modifier.height(20.dp))
        Box(Modifier.size(100.dp, 14.dp).skeleton(skeleton, PleyShape.Badge))
        Spacer(Modifier.height(10.dp))
        Box(Modifier.size(220.dp, 28.dp).skeleton(skeleton, PleyShape.Badge))
        Spacer(Modifier.height(10.dp))
        Box(Modifier.size(140.dp, 14.dp).skeleton(skeleton, PleyShape.Badge))
        Spacer(Modifier.height(24.dp))
        repeat(4) {
            Row(Modifier.padding(vertical = 8.dp), verticalAlignment = Alignment.CenterVertically) {
                Box(Modifier.size(112.dp, 63.dp).skeleton(skeleton, PleyShape.TilePhone))
                Spacer(Modifier.width(12.dp))
                Column {
                    Box(Modifier.size(160.dp, 14.dp).skeleton(skeleton, PleyShape.Badge))
                    Spacer(Modifier.height(8.dp))
                    Box(Modifier.size(100.dp, 12.dp).skeleton(skeleton, PleyShape.Badge))
                }
            }
        }
    }
}

/** How far a row is swiped, of its width, before letting go removes it. */
private const val SWIPE_THRESHOLD = 0.4f

/** The lazy position of the first entry: the header comes before it. */
private const val FIRST_ROW = 1

private val COVER = 168.dp

internal const val PLAYLIST_PAGE = "pley.playlist"
internal const val PLAYLIST_ENTRIES = "pley.playlist.entries"
internal const val ENTRY_TAG = "pley.playlist.entry."
internal const val HANDLE_TAG = "pley.playlist.handle."
