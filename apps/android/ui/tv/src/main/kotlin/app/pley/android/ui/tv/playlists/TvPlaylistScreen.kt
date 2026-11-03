package app.pley.android.ui.tv.playlists

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.background
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.focusGroup
import androidx.compose.foundation.gestures.LocalBringIntoViewSpec
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsFocusedAsState
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
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.relocation.BringIntoViewRequester
import androidx.compose.foundation.relocation.bringIntoViewRequester
import androidx.compose.foundation.text.BasicText
import androidx.compose.foundation.text.input.rememberTextFieldState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusProperties
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import app.pley.android.data.Loadable
import app.pley.android.data.valueOrNull
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyFocus
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.PleyTv
import app.pley.android.designsystem.PleyType
import app.pley.android.designsystem.TvSafeArea
import app.pley.android.designsystem.icon.Icon
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.designsystem.image.ArtworkCandidates
import app.pley.android.designsystem.tile.ArtworkTile
import app.pley.android.designsystem.tile.ArtworkTileModel
import app.pley.android.designsystem.tile.QualityBadge
import app.pley.android.designsystem.tile.TileKind
import app.pley.android.designsystem.tile.TileStyle
import app.pley.android.designsystem.tile.rememberSkeleton
import app.pley.android.designsystem.tile.skeleton
import app.pley.android.network.ApiException
import app.pley.android.screens.playlists.PlaylistDeps
import app.pley.android.screens.playlists.PlaylistEvent
import app.pley.android.screens.playlists.PlaylistPage
import app.pley.android.screens.playlists.PlaylistRow
import app.pley.android.screens.playlists.PlaylistViewModel
import app.pley.android.screens.playlists.Playlists
import app.pley.android.screens.screensGraph
import app.pley.android.ui.tv.R
import app.pley.android.ui.tv.item.TvToastHost
import app.pley.android.ui.tv.item.TvToastState
import app.pley.android.ui.tv.kit.BackdropScrim
import app.pley.android.ui.tv.kit.LocalTvRailFocus
import app.pley.android.ui.tv.kit.OverlayShowing
import app.pley.android.ui.tv.kit.TvBackdrop
import app.pley.android.ui.tv.kit.TvPillButton
import app.pley.android.ui.tv.kit.TvRoundButton
import app.pley.android.ui.tv.kit.TvScrolling
import app.pley.android.ui.tv.kit.TvSidePanel
import app.pley.android.ui.tv.kit.TvTextButton
import app.pley.android.ui.tv.kit.TvTextField
import app.pley.android.ui.tv.kit.initialFocus
import app.pley.android.ui.tv.kit.menuKey
import app.pley.android.ui.tv.kit.railOnLeftStopOnRight
import app.pley.android.ui.tv.kit.rememberFocusMemory
import app.pley.android.ui.tv.kit.rememberedFocus
import app.pley.android.ui.tv.library.TvMessage
import app.pley.android.ui.tv.onboarding.TvDialog
import app.pley.android.ui.tv.shell.LocalTvNavigator
import app.pley.android.ui.tv.shell.failedBody
import app.pley.android.ui.tv.shell.failedTitle
import app.pley.android.ui.tv.theme.LocalTvTier
import kotlinx.coroutines.launch

/**
 * A playlist's page on TV (design spec §4.9): the item page's top (the
 * first entry's backdrop; "Video playlist" or "Smart playlist", the
 * title, "12 items · 3h 12m" with a smart playlist's filter in words;
 * Play with focus, Shuffle, Rename, Delete), then its entries as a list
 * of landscape rows: number, 160 × 90 still, title and length, and More.
 *
 * Select on an entry plays from it; holding Select, or More, opens its
 * actions: Play from here, Go to it, Move, Remove. Move lifts the entry:
 * Up and Down move it, Select drops it there (and sends the order), Back
 * puts it back. Smart playlists offer no edits.
 */
@Composable
internal fun TvPlaylistScreen(playlistId: Long) {
    val vm: PlaylistViewModel<ArtworkTileModel> = viewModel {
        val graph = screensGraph()
        PlaylistViewModel(playlistId, PlaylistDeps.of(graph), graph.playerLauncher, TvPlaylistTiles)
    }
    TvPlaylistScreen(vm)
}

/** The page of [vm]'s playlist. */
@Composable
internal fun TvPlaylistScreen(vm: PlaylistViewModel<ArtworkTileModel>) {
    val resources = LocalResources.current
    val state by vm.state.collectAsStateWithLifecycle()
    val navigator = LocalTvNavigator.current
    val toasts = remember { TvToastState() }
    LaunchedEffect(vm) {
        vm.events.collect { event ->
            when (event) {
                is PlaylistEvent.Removed -> toasts.show(resources.getString(R.string.tv_playlist_removed, event.title))
                PlaylistEvent.Deleted -> navigator.back()
                is PlaylistEvent.Open -> navigator.open(event.key)
                is PlaylistEvent.Failed -> toasts.report(event.error)
            }
        }
    }
    val page = state.valueOrNull
    Box(Modifier.fillMaxSize().testTag(TV_PLAYLIST)) {
        when {
            page != null -> PlaylistContent(page, vm, toasts)
            state is Loadable.Failed -> {
                val error = (state as Loadable.Failed).error
                if ((error as? ApiException)?.status == 404) {
                    TvMessage(
                        title = stringResource(R.string.tv_playlist_not_found),
                        body = stringResource(R.string.tv_playlist_not_found_body),
                        action = stringResource(R.string.tv_playlist_back),
                        onAction = navigator::back,
                        top = 58.dp,
                    )
                } else {
                    TvMessage(
                        title = failedTitle(error, stringResource(R.string.tv_playlist_failed)),
                        body = failedBody(error),
                        action = stringResource(R.string.tv_playlists_retry),
                        onAction = vm::retry,
                        top = 58.dp,
                    )
                }
            }
            else -> Loading()
        }
        TvToastHost(toasts)
    }
}

/** The TV's thumbnails: 16:9 stills for video entries, squares for music. */
internal object TvPlaylistTiles : app.pley.android.screens.library.TileFactory<ArtworkTileModel> {
    override fun item(item: app.pley.android.model.ItemSummary, look: app.pley.android.screens.library.TileLook, origin: String) =
        ArtworkTileModel.of(item, if (look == app.pley.android.screens.library.TileLook.Square) TileKind.Square else TileKind.Landscape, origin)

    override fun collection(collection: app.pley.android.model.CollectionSummary, kind: app.pley.android.model.LibraryKind?, origin: String) =
        ArtworkTileModel(id = collection.id, title = collection.title)
}

/** What can be open over the page: a row's actions, Rename, or Delete's question. */
private sealed interface Overlay {
    data class Actions(val row: PlaylistRow<ArtworkTileModel>) : Overlay

    data object Rename : Overlay

    data object Delete : Overlay
}

@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun PlaylistContent(page: PlaylistPage<ArtworkTileModel>, vm: PlaylistViewModel<ArtworkTileModel>, toasts: TvToastState) {
    val navigator = LocalTvNavigator.current
    val resources = LocalResources.current
    val rail = LocalTvRailFocus.current
    val tier = LocalTvTier.current
    val density = LocalDensity.current
    val memory = rememberFocusMemory()
    val list = rememberLazyListState()
    val scope = androidx.compose.runtime.rememberCoroutineScope()
    var overlay by remember { mutableStateOf<Overlay?>(null) }
    var moving by remember { mutableStateOf<Long?>(null) }
    /** Each shown row's focus, so focus can go back to the row a panel was opened from, or to a neighbour. */
    val rowFocus = remember { HashMap<Long, FocusRequester>() }
    var focusNext by remember { mutableStateOf<Long?>(null) }
    /** Delete's, which Cancel goes back to, as does the last entry removed. */
    val deleteFocus = remember { FocusRequester() }
    val heroPx = with(density) { HeroHeight.toPx() }

    // A removed entry's neighbour takes focus once the entry has gone; Delete, when it was the last.
    LaunchedEffect(page.rows, focusNext) {
        val next = focusNext ?: return@LaunchedEffect
        if (next == NO_ENTRY && page.rows.isEmpty()) {
            withFrameNanos { }
            runCatching { deleteFocus.requestFocus() }
            focusNext = null
        } else if (page.rows.any { it.entryId == next }) {
            withFrameNanos { }
            runCatching { rowFocus[next]?.requestFocus() }
            focusNext = null
        }
    }
    // While moving: Back puts the entry back where it was. The keys are the entry's: requests' news waits.
    BackHandler(enabled = moving != null) {
        vm.cancelMoves()
        moving = null
    }
    OverlayShowing(moving != null)

    Box(Modifier.fillMaxSize()) {
        val backdrop = remember(page.lead, page.origin, tier) { page.lead?.let { ArtworkCandidates.backdrop(it, page.origin, tier) } }
        TvBackdrop(
            image = backdrop,
            title = page.title,
            scrim = BackdropScrim.Item,
            modifier = Modifier.drawWithContent {
                drawContent()
                // The list passes over the backdrop: it darkens as the top scrolls away (one rect, drawn).
                val scrolled = if (list.firstVisibleItemIndex > 0) 1f else (list.firstVisibleItemScrollOffset / heroPx).coerceIn(0f, 1f)
                if (scrolled > 0f) drawRect(PleyColors.Bg.copy(alpha = scrolled * 0.92f))
            },
        )
        val spec = remember(density, list) { TvScrolling.page(density, RowAnchor, list, HeroHeight) }
        CompositionLocalProvider(LocalBringIntoViewSpec provides spec) {
            LazyColumn(
                state = list,
                modifier = Modifier
                    .fillMaxSize()
                    .railOnLeftStopOnRight(rail, stopRight = false)
                    .testTag(TV_PLAYLIST_ENTRIES),
                contentPadding = PaddingValues(bottom = 200.dp),
            ) {
                hero(page, vm, memory, deleteFocus, onRename = { overlay = Overlay.Rename }, onDelete = { overlay = Overlay.Delete })
                itemsIndexed(page.rows, key = { _, row -> row.entryId }, contentType = { _, _ -> "entry" }) { index, row ->
                    val requester = remember { FocusRequester() }
                    DisposableEffect(row.entryId) {
                        rowFocus[row.entryId] = requester
                        onDispose { if (rowFocus[row.entryId] === requester) rowFocus.remove(row.entryId) }
                    }
                    EntryRow(
                        row = row,
                        video = page.video,
                        editable = page.editable,
                        moving = moving == row.entryId,
                        index = index,
                        focus = requester,
                        memory = memory,
                        onPlay = { vm.play(from = index) },
                        onMore = { overlay = Overlay.Actions(row) },
                        onMove = { by -> vm.moveBy(row.entryId, by, send = false) },
                        onDrop = {
                            vm.sendOrder()
                            moving = null
                        },
                    )
                }
            }
        }
        if (moving != null) MovingHint()

        // A row's actions.
        val actionsFor = (overlay as? Overlay.Actions)?.row
        TvSidePanel(visible = actionsFor != null, onDismiss = {
            val back = actionsFor?.entryId
            overlay = null
            back?.let { id -> scope.launch { withFrameNanos { }; runCatching { rowFocus[id]?.requestFocus() } } }
        }, modifier = Modifier.testTag(TV_PLAYLIST_PANEL)) {
            val row = actionsFor ?: (overlay as? Overlay.Actions)?.row
            if (row != null) RowActions(row, page, onDone = { overlay = null }, actions = RowActionsHandler(
                play = { vm.play(from = page.rows.indexOfFirst { it.entryId == row.entryId }.coerceAtLeast(0)) },
                open = { navigator.open(vm.opens(row)) },
                move = {
                    moving = row.entryId
                    scope.launch { withFrameNanos { }; runCatching { rowFocus[row.entryId]?.requestFocus() } }
                },
                remove = {
                    val at = page.rows.indexOfFirst { it.entryId == row.entryId }
                    focusNext = (page.rows.getOrNull(at + 1) ?: page.rows.getOrNull(at - 1))?.entryId ?: NO_ENTRY
                    vm.remove(row.entryId, undoable = false)
                    toasts.show(resources.getString(R.string.tv_playlist_removed, row.title))
                },
            ))
        }
        // Rename.
        TvSidePanel(visible = overlay == Overlay.Rename, onDismiss = { overlay = null }, modifier = Modifier.testTag(TV_PLAYLIST_RENAME)) {
            if (overlay == Overlay.Rename) RenamePanel(page.title, onSave = { title -> if (vm.rename(title)) overlay = null })
        }
        if (overlay == Overlay.Delete) {
            val cancel = remember { FocusRequester() }
            LaunchedEffect(Unit) {
                withFrameNanos { }
                runCatching { cancel.requestFocus() }
            }
            TvDialog(
                title = stringResource(R.string.tv_playlist_delete_title),
                body = stringResource(R.string.tv_playlist_delete_body, page.title),
                onDismiss = { overlay = null },
                returnTo = deleteFocus,
            ) {
                TvTextButton(stringResource(R.string.tv_playlist_cancel), onClick = { overlay = null }, modifier = Modifier.focusRequester(cancel))
                TvTextButton(
                    stringResource(R.string.tv_playlist_delete),
                    onClick = {
                        overlay = null
                        vm.delete()
                    },
                    color = PleyColors.DangerText,
                )
            }
        }
    }
}

/** The text column of the page's top: what it plays, the title, its facts and the buttons. */
private fun LazyListScope.hero(
    page: PlaylistPage<ArtworkTileModel>,
    vm: PlaylistViewModel<ArtworkTileModel>,
    memory: app.pley.android.ui.tv.kit.FocusMemory,
    deleteFocus: FocusRequester,
    onRename: () -> Unit,
    onDelete: () -> Unit,
) {
    item(key = "hero", contentType = "hero") {
        Column(Modifier.height(HeroHeight).padding(start = PleyTv.Gutter, top = 58.dp).widthIn(max = TextColumn)) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Icon(if (page.video) PleyIcons.ListVideo else PleyIcons.ListMusic, contentDescription = null, size = 18.dp, tint = PleyColors.Muted)
                BasicText(
                    stringResource(
                        when {
                            page.smart -> R.string.tv_playlist_smart
                            page.video -> R.string.tv_playlist_video
                            else -> R.string.tv_playlist_music
                        },
                    ),
                    style = PleyType.Tv.Caption.copy(color = PleyColors.Muted),
                )
            }
            Spacer(Modifier.height(10.dp))
            BasicText(
                page.title,
                style = PleyType.Tv.HeroTitle.copy(color = PleyColors.Fg),
                maxLines = 2,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.semantics { heading() },
            )
            Spacer(Modifier.height(12.dp))
            val facts = (listOf(page.meta) + page.notes).filter { it.isNotEmpty() }
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                if (page.smart) QualityBadge(stringResource(R.string.tv_playlists_smart), TileStyle.Tv)
                BasicText(facts.joinToString("  ·  "), style = PleyType.Tv.Label.copy(color = PleyColors.Muted), maxLines = 2, overflow = TextOverflow.Ellipsis)
            }
            Spacer(Modifier.weight(1f))
            Row(horizontalArrangement = Arrangement.spacedBy(PleyTv.ButtonGap), verticalAlignment = Alignment.Top) {
                TvPillButton(
                    stringResource(R.string.tv_playlist_play),
                    onClick = { vm.play() },
                    icon = PleyIcons.Play,
                    enabled = page.canPlay,
                    modifier = if (page.canPlay) Modifier.rememberedFocus(memory, "play", initial = true) else Modifier,
                )
                TvRoundButton(
                    stringResource(R.string.tv_playlist_shuffle),
                    PleyIcons.Shuffle,
                    onClick = { vm.play(shuffle = true) },
                    enabled = page.canShuffle,
                    modifier = Modifier.rememberedFocus(memory, "shuffle"),
                )
                if (page.editable) {
                    TvRoundButton(
                        stringResource(R.string.tv_playlist_rename),
                        PleyIcons.Pencil,
                        onClick = onRename,
                        modifier = if (page.canPlay) Modifier.rememberedFocus(memory, "rename") else Modifier.rememberedFocus(memory, "rename", initial = true),
                    )
                }
                TvRoundButton(
                    stringResource(R.string.tv_playlist_delete),
                    TvTrashIcon,
                    onClick = onDelete,
                    modifier = (if (page.canPlay || page.editable) Modifier.rememberedFocus(memory, "delete") else Modifier.rememberedFocus(memory, "delete", initial = true))
                        .focusRequester(deleteFocus),
                )
            }
            Spacer(Modifier.height(PleyTv.ButtonLabelRoom + 12.dp))
            if (page.rows.isEmpty()) {
                BasicText(
                    stringResource(
                        when {
                            page.smart -> R.string.tv_playlist_empty_smart
                            page.video -> R.string.tv_playlist_empty_video
                            else -> R.string.tv_playlist_empty_music
                        },
                    ),
                    style = PleyType.Tv.Body.copy(color = PleyColors.Muted),
                )
            }
        }
    }
}

/**
 * One entry, as a row of a list (design spec §2.7: rows fill without
 * scaling): its number, 160 × 90 still (90 × 90 cover for music), its two
 * lines and length; focused, an accent pill. More sits at its end, a
 * Right away. While [moving] it is lifted (a ring, the arrows): Up and
 * Down move it, Select drops it.
 */
@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun EntryRow(
    row: PlaylistRow<ArtworkTileModel>,
    video: Boolean,
    editable: Boolean,
    moving: Boolean,
    index: Int,
    focus: FocusRequester,
    memory: app.pley.android.ui.tv.kit.FocusMemory,
    onPlay: () -> Unit,
    onMore: () -> Unit,
    onMove: (Int) -> Unit,
    onDrop: () -> Unit,
) {
    val interactions = remember { MutableInteractionSource() }
    val focused by interactions.collectIsFocusedAsState()
    val bring = remember { BringIntoViewRequester() }
    LaunchedEffect(index, moving) { if (moving) runCatching { bring.bringIntoView() } }
    val fill = when {
        moving -> PleyColors.Surface3
        focused -> PleyColors.Accent
        else -> androidx.compose.ui.graphics.Color.Transparent
    }
    val content = if (focused && !moving) PleyColors.OnAccent else PleyColors.Fg
    val secondary = if (focused && !moving) PleyColors.OnAccent else PleyColors.Muted
    val playLabel = stringResource(R.string.tv_playlist_play_from_here)
    Row(
        Modifier
            .fillMaxWidth()
            .padding(start = PleyTv.Gutter - 16.dp, end = TvSafeArea.Horizontal)
            .bringIntoViewRequester(bring)
            .testTag(TV_ENTRY + row.entryId),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Row(
            Modifier
                .weight(1f)
                .heightIn(min = 106.dp)
                .focusRequester(focus)
                .rememberedFocus(memory, "entry/${row.entryId}")
                .onPreviewKeyEvent { event ->
                    if (!moving) return@onPreviewKeyEvent false
                    when (event.key) {
                        Key.DirectionUp -> {
                            if (event.type == KeyEventType.KeyDown) onMove(-1)
                            true
                        }
                        Key.DirectionDown -> {
                            if (event.type == KeyEventType.KeyDown) onMove(1)
                            true
                        }
                        Key.DirectionCenter, Key.Enter, Key.NumPadEnter -> {
                            if (event.type == KeyEventType.KeyUp) onDrop()
                            true
                        }
                        Key.DirectionLeft, Key.DirectionRight -> true
                        else -> false
                    }
                }
                .background(fill, PleyShape.Card)
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
                .semantics { contentDescription = "${index + 1}. ${row.title}, ${row.subtitle}, ${row.duration}" }
                .menuKey(onMore)
                .combinedClickable(interactionSource = interactions, indication = null, onClick = onPlay, onLongClick = onMore, onClickLabel = playLabel)
                .padding(horizontal = 16.dp, vertical = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            BasicText(
                (index + 1).toString(),
                style = PleyType.Tv.Label.copy(color = if (focused && !moving) PleyColors.OnAccent else PleyColors.Subtle, textAlign = TextAlign.End),
                modifier = Modifier.width(28.dp),
            )
            Spacer(Modifier.width(16.dp))
            val kind = if (video) TileKind.Landscape else TileKind.Square
            ArtworkTile(row.tile, kind, TileStyle.Tv, Modifier.height(90.dp).aspectRatio(if (video) 16f / 9f else 1f))
            Spacer(Modifier.width(20.dp))
            Column(Modifier.weight(1f)) {
                BasicText(row.title, style = PleyType.Tv.Title.copy(color = content), maxLines = 1, overflow = TextOverflow.Ellipsis)
                if (row.subtitle.isNotEmpty()) {
                    BasicText(row.subtitle, style = PleyType.Tv.Caption.copy(color = secondary), maxLines = 1, overflow = TextOverflow.Ellipsis)
                }
            }
            if (moving) {
                Column(horizontalAlignment = Alignment.CenterHorizontally) {
                    Icon(PleyIcons.ChevronUp, contentDescription = null, size = 22.dp, tint = PleyColors.AccentBright)
                    Icon(PleyIcons.ChevronDown, contentDescription = null, size = 22.dp, tint = PleyColors.AccentBright)
                }
            } else if (row.duration.isNotEmpty()) {
                Spacer(Modifier.width(16.dp))
                BasicText(row.duration, style = PleyType.Tv.Caption.copy(color = secondary))
            }
        }
        Spacer(Modifier.width(12.dp))
        TvRoundButton(
            stringResource(R.string.tv_playlist_more),
            PleyIcons.Ellipsis,
            onClick = onMore,
            onArt = false,
            enabled = !moving,
            modifier = Modifier.padding(top = PleyTv.ButtonLabelRoom / 2),
        )
    }
}

/** The hint while an entry is lifted, at the bottom of the screen. */
@Composable
private fun MovingHint() {
    Box(Modifier.fillMaxSize().padding(bottom = TvSafeArea.Vertical), contentAlignment = Alignment.BottomCenter) {
        BasicText(
            stringResource(R.string.tv_playlist_moving),
            style = PleyType.Tv.Label.copy(color = PleyColors.Fg),
            modifier = Modifier.background(PleyColors.Surface3, PleyShape.Pill).padding(horizontal = 20.dp, vertical = 12.dp),
        )
    }
}

/** What a row's actions do. */
private class RowActionsHandler(val play: () -> Unit, val open: () -> Unit, val move: () -> Unit, val remove: () -> Unit)

/** A row's actions in the side panel: Play from here, Go to it, and for editable playlists Move and Remove. */
@Composable
private fun RowActions(row: PlaylistRow<ArtworkTileModel>, page: PlaylistPage<ArtworkTileModel>, onDone: () -> Unit, actions: RowActionsHandler) {
    val first = remember(row.entryId) { FocusRequester() }
    LaunchedEffect(row.entryId) {
        withFrameNanos { }
        withFrameNanos { }
        runCatching { first.requestFocus() }
    }
    Column(Modifier.focusProperties { onExit = { cancelFocusChange() } }.focusGroup()) {
        TvPanelTitle(row.title, row.subtitle)
        TvPanelRow(stringResource(R.string.tv_playlist_play_from_here), onClick = {
            onDone()
            actions.play()
        }, icon = PleyIcons.Play, focusRequester = first)
        TvPanelRow(stringResource(R.string.tv_playlist_go_to, row.title), onClick = {
            onDone()
            actions.open()
        }, icon = PleyIcons.ArrowRight)
        if (page.editable) {
            TvPanelRow(stringResource(R.string.tv_playlist_move), onClick = {
                onDone()
                actions.move()
            }, icon = PleyIcons.ArrowUpDown, enabled = page.rows.size > 1)
            TvPanelRow(stringResource(R.string.tv_playlist_remove), onClick = {
                onDone()
                actions.remove()
            }, icon = PleyIcons.X, danger = true)
        }
    }
}

/** Rename: the field with the title as it is (Select types), and Save once it will do. */
@Composable
private fun RenamePanel(current: String, onSave: (String) -> Unit) {
    val field = rememberTextFieldState(current)
    val ok = Playlists.check(field.text.toString()) == null
    Column(Modifier.focusProperties { onExit = { cancelFocusChange() } }.focusGroup(), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        TvPanelTitle(stringResource(R.string.tv_playlist_rename_title))
        TvTextField(
            state = field,
            placeholder = stringResource(R.string.tv_playlist_form_title),
            imeAction = androidx.compose.ui.text.input.ImeAction.Done,
            onAction = { if (ok) onSave(field.text.toString()) },
            focusModifier = Modifier.initialFocus(),
            modifier = Modifier.testTag(TV_RENAME_FIELD),
        )
        BasicText(stringResource(R.string.tv_playlist_form_hint), style = PleyType.Tv.Caption.copy(color = PleyColors.Muted), modifier = Modifier.padding(start = 4.dp))
        Spacer(Modifier.height(4.dp))
        TvPillButton(stringResource(R.string.tv_playlist_save), onClick = { onSave(field.text.toString()) }, enabled = ok)
    }
}

/** Still blocks in the top's shapes, while nothing was saved. */
@Composable
private fun Loading() {
    val skeleton = rememberSkeleton(animated = false)
    Column(Modifier.fillMaxSize().padding(start = PleyTv.Gutter, top = 58.dp), verticalArrangement = Arrangement.spacedBy(14.dp)) {
        Box(Modifier.size(120.dp, 18.dp).skeleton(skeleton, PleyShape.Badge))
        Box(Modifier.size(360.dp, 44.dp).skeleton(skeleton, PleyShape.Badge))
        Box(Modifier.size(200.dp, 18.dp).skeleton(skeleton, PleyShape.Badge))
        Spacer(Modifier.height(120.dp))
        Box(Modifier.size(128.dp, PleyTv.ButtonHeight).skeleton(skeleton, PleyShape.Pill))
    }
}

/** The page's top, down to just above the first entry (design spec §4.3: what is below starts at 452). */
private val HeroHeight = 436.dp

/** Where a focused entry's top settles once the page has scrolled. */
private val RowAnchor = 140.dp

/** The neighbour of the last entry removed: Delete takes focus. */
private const val NO_ENTRY = -1L

/** Text stays in the backdrop's 85 % scrim: x 96 to 560. */
private val TextColumn = PleyTv.TextColumnEnd - PleyTv.Gutter

internal const val TV_PLAYLIST = "pley.tv.playlist"
internal const val TV_PLAYLIST_ENTRIES = "pley.tv.playlist.entries"
internal const val TV_PLAYLIST_PANEL = "pley.tv.playlist.panel"
internal const val TV_PLAYLIST_RENAME = "pley.tv.playlist.rename"
internal const val TV_RENAME_FIELD = "pley.tv.playlist.rename.field"
internal const val TV_ENTRY = "pley.tv.playlist.entry."
