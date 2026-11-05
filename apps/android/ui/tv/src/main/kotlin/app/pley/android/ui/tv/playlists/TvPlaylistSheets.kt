package app.pley.android.ui.tv.playlists

import android.content.res.Resources
import androidx.compose.foundation.focusGroup
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.text.BasicText
import androidx.compose.foundation.text.input.TextFieldState
import androidx.compose.foundation.text.input.rememberTextFieldState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusProperties
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import app.pley.android.common.errorSentence
import app.pley.android.data.Loadable
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.PleyTv
import app.pley.android.designsystem.PleyType
import app.pley.android.designsystem.icon.Icon
import app.pley.android.designsystem.icon.PleyIcon
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.designsystem.image.ArtworkCandidates
import app.pley.android.designsystem.text.asString
import app.pley.android.designsystem.tile.rememberSkeleton
import app.pley.android.designsystem.tile.skeleton
import app.pley.android.model.PlaylistKind
import app.pley.android.screens.PleyKey
import app.pley.android.screens.playlists.AddEvent
import app.pley.android.screens.playlists.AddToPlaylistUiState
import app.pley.android.screens.playlists.AddToPlaylistViewModel
import app.pley.android.screens.playlists.CreateEvent
import app.pley.android.screens.playlists.NewPlaylistMode
import app.pley.android.screens.playlists.NewPlaylistViewModel
import app.pley.android.screens.playlists.PlaylistCard
import app.pley.android.screens.playlists.Playlists
import app.pley.android.ui.tv.R
import app.pley.android.ui.tv.item.TvToastHost
import app.pley.android.ui.tv.item.TvToastState
import app.pley.android.ui.tv.kit.BackdropScrim
import app.pley.android.ui.tv.kit.TvBackdrop
import app.pley.android.ui.tv.kit.TvSidePanel
import app.pley.android.ui.tv.kit.TvTextField
import app.pley.android.ui.tv.shell.LocalTvNavigator
import app.pley.android.ui.tv.theme.LocalTvTier
import kotlinx.coroutines.delay

/**
 * Add to playlist on TV (PleyKey.AddToPlaylist; design spec: a side
 * panel): over the item's backdrop and name, a panel with a new playlist
 * to name (Select types; the keyboard's Done creates it), then the
 * playlists that take it. Focus starts on the first of those, else on
 * the name. Once added, the panel says so ("Added to Road trip") for a
 * moment and closes; Back closes it.
 */
@Composable
internal fun TvAddToPlaylistScreen(itemIds: List<Long>) {
    val vm: AddToPlaylistViewModel = viewModel(factory = AddToPlaylistViewModel.factory(itemIds))
    val state by vm.state.collectAsStateWithLifecycle()
    val navigator = LocalTvNavigator.current
    val resources = LocalResources.current
    val toasts = remember { TvToastState() }
    var done by remember { mutableStateOf<String?>(null) }
    LaunchedEffect(vm) {
        vm.events.collect { event ->
            when (event) {
                is AddEvent.Added -> done = addedMessage(resources, event)
                is AddEvent.Failed -> toasts.report(event.error)
            }
        }
    }
    LaunchedEffect(done) {
        if (done != null) {
            delay(DONE_MS)
            navigator.back()
        }
    }
    TvAddToPlaylistContent(state, done, onCreate = vm::create, onAdd = vm::add, onClose = navigator::back)
    TvToastHost(toasts)
}

/** The toast's words for [event], as the web says them. */
internal fun addedMessage(resources: Resources, event: AddEvent.Added): String = when (val n = event.entries) {
    0 -> resources.getString(R.string.tv_add_nothing)
    null, 1 -> resources.getString(R.string.tv_add_added, event.playlist)
    else -> resources.getString(R.string.tv_add_added_many, n.toString(), event.playlist)
}

@Composable
internal fun TvAddToPlaylistContent(
    state: AddToPlaylistUiState,
    done: String?,
    onCreate: (String) -> Boolean,
    onAdd: (PlaylistCard) -> Unit,
    onClose: () -> Unit,
) {
    val tier = LocalTvTier.current
    val music = state.kind == PlaylistKind.Audio
    Box(Modifier.fillMaxSize().testTag(TV_ADD)) {
        val backdrop = remember(state.lead, state.origin, tier) { state.lead?.let { ArtworkCandidates.backdrop(it, state.origin, tier) } }
        TvBackdrop(backdrop, state.title.orEmpty(), scrim = BackdropScrim.Item)
        Behind(PleyIcons.ListPlus, stringResource(R.string.tv_add_to_playlist), state.title)
        TvSidePanel(visible = true, onDismiss = onClose) {
            if (done != null) {
                Done(done)
                return@TvSidePanel
            }
            val name = rememberTextFieldState()
            val first = remember { FocusRequester() }
            val field = remember { FocusRequester() }
            val cards = (state.playlists as? Loadable.Ready)?.value ?: (state.playlists as? Loadable.Failed)?.stale
            LaunchedEffect(cards != null, state.kind != null) {
                if (state.kind == null || state.playlists == Loadable.Loading) return@LaunchedEffect
                withFrameNanos { }
                withFrameNanos { }
                runCatching { if (!cards.isNullOrEmpty()) first.requestFocus() else field.requestFocus() }
            }
            LazyColumn(
                Modifier
                    .fillMaxSize()
                    .focusProperties { onExit = { cancelFocusChange() } }
                    .focusGroup(),
            ) {
                item(key = "title") { TvPanelTitle(stringResource(R.string.tv_add_to_playlist), state.title) }
                item(key = "new") {
                    NewField(
                        name = name,
                        placeholder = stringResource(if (music) R.string.tv_add_new_music else R.string.tv_add_new_video),
                        focus = field,
                        enabled = state.kind != null && !state.busy,
                        onCreate = { onCreate(name.text.toString()) },
                    )
                }
                item(key = "create") {
                    TvPanelRow(
                        stringResource(R.string.tv_add_create),
                        onClick = { onCreate(name.text.toString()) },
                        icon = PleyIcons.Plus,
                        enabled = state.kind != null && !state.busy && Playlists.check(name.text.toString()) == null,
                    )
                }
                item(key = "yours") { TvPanelHeading(stringResource(R.string.tv_add_yours)) }
                when (val playlists = state.playlists) {
                    Loadable.Loading -> item(key = "loading") { SkeletonRows() }
                    is Loadable.Failed -> if (playlists.stale == null) {
                        item(key = "failed") {
                            BasicText(
                                errorSentence(playlists.error).asString(),
                                style = PleyType.Tv.Body.copy(color = PleyColors.DangerText),
                                modifier = Modifier.padding(horizontal = 16.dp, vertical = 8.dp),
                            )
                        }
                    }
                    is Loadable.Ready -> if (playlists.value.isEmpty() && state.kind != null) {
                        item(key = "none") {
                            BasicText(
                                stringResource(if (music) R.string.tv_add_none_music else R.string.tv_add_none_video),
                                style = PleyType.Tv.Body.copy(color = PleyColors.Muted),
                                modifier = Modifier.padding(horizontal = 16.dp, vertical = 8.dp),
                            )
                        }
                    }
                }
                cards?.let { list ->
                    items(list, key = { it.id }, contentType = { "playlist" }) { card ->
                        TvPanelRow(
                            label = card.title,
                            supporting = card.meta,
                            onClick = { onAdd(card) },
                            enabled = !state.busy,
                            focusRequester = if (card == list.first()) first else null,
                            leading = { TvPlaylistArt(card.artwork, card.origin, card.video, card.title, Modifier.size(40.dp), glyph = 18.dp) },
                            modifier = Modifier.testTag(TV_ADD_ROW + card.id),
                        )
                    }
                }
            }
        }
    }
}

/** The name of a new playlist: the kit's field (Select types), the keyboard's Done making it. */
@Composable
private fun NewField(name: TextFieldState, placeholder: String, focus: FocusRequester, enabled: Boolean, onCreate: () -> Unit) {
    Box(Modifier.padding(horizontal = 4.dp, vertical = 6.dp).focusProperties { canFocus = enabled }) {
        TvTextField(
            state = name,
            placeholder = placeholder,
            imeAction = ImeAction.Done,
            onAction = onCreate,
            focusModifier = Modifier.focusRequester(focus),
            modifier = Modifier.testTag(TV_ADD_FIELD),
        )
    }
}

/** "Added to Road trip": the accent check and the words, for a moment before the panel closes. */
@Composable
private fun Done(message: String) {
    Column(
        Modifier.fillMaxSize().semantics { liveRegion = LiveRegionMode.Polite },
        verticalArrangement = Arrangement.Center,
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Icon(PleyIcons.CircleCheck, contentDescription = null, size = 48.dp, tint = PleyColors.Accent)
        Spacer(Modifier.height(16.dp))
        BasicText(message, style = PleyType.Tv.Title.copy(color = PleyColors.Fg), maxLines = 3, overflow = TextOverflow.Ellipsis)
    }
}

/** What the panel is for, at the gutter beside it: a glyph, the action and what it acts on. */
@Composable
private fun Behind(icon: PleyIcon, title: String, subtitle: String?) {
    Column(Modifier.padding(start = PleyTv.Gutter, top = 58.dp).width(TextColumn)) {
        Icon(icon, contentDescription = null, size = 32.dp, tint = PleyColors.Muted)
        Spacer(Modifier.height(16.dp))
        BasicText(title, style = PleyType.Tv.Label.copy(color = PleyColors.Muted))
        Spacer(Modifier.height(6.dp))
        if (subtitle != null) {
            BasicText(subtitle, style = PleyType.Tv.PageTitle.copy(color = PleyColors.Fg), maxLines = 3, overflow = TextOverflow.Ellipsis)
        }
    }
}

@Composable
private fun SkeletonRows() {
    val skeleton = rememberSkeleton(animated = false)
    Column(Modifier.padding(horizontal = 16.dp, vertical = 8.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
        repeat(2) { Box(Modifier.fillMaxWidth(0.8f).height(40.dp).skeleton(skeleton, PleyShape.Pill)) }
    }
}

/**
 * New playlist and Save these filters on TV (PleyKey.NewPlaylist,
 * PleyKey.SaveSmartPlaylist): a side panel with the title (Select types)
 * and, for an empty playlist, what it plays; Create opens the playlist
 * made. Focus starts on the title.
 */
@Composable
internal fun TvNewPlaylistScreen(mode: NewPlaylistMode) {
    val vm: NewPlaylistViewModel = viewModel(factory = NewPlaylistViewModel.factory(mode))
    val busy by vm.busy.collectAsStateWithLifecycle()
    val navigator = LocalTvNavigator.current
    val toasts = remember { TvToastState() }
    LaunchedEffect(vm) {
        vm.events.collect { event ->
            when (event) {
                is CreateEvent.Created -> {
                    navigator.back()
                    navigator.open(PleyKey.Playlist(event.playlistId))
                }
                is CreateEvent.Failed -> toasts.report(event.error)
            }
        }
    }
    TvNewPlaylistContent(
        smart = mode is NewPlaylistMode.Smart,
        filters = vm.filters,
        asksKind = vm.asksKind,
        busy = busy,
        onCreate = vm::create,
        onClose = navigator::back,
    )
    TvToastHost(toasts)
}

@Composable
internal fun TvNewPlaylistContent(
    smart: Boolean,
    filters: List<String>,
    asksKind: Boolean,
    busy: Boolean,
    onCreate: (String, PlaylistKind) -> Boolean,
    onClose: () -> Unit,
) {
    val title = stringResource(if (smart) R.string.tv_playlist_smart_title else R.string.tv_playlists_new)
    val body = when {
        !smart -> stringResource(R.string.tv_playlist_new_body)
        filters.isEmpty() -> stringResource(R.string.tv_playlist_smart_body_plain)
        else -> stringResource(R.string.tv_playlist_smart_body, filters.joinToString(" · "))
    }
    Box(Modifier.fillMaxSize().testTag(TV_NEW)) {
        Column(Modifier.padding(start = PleyTv.Gutter, top = 58.dp).width(TextColumn)) {
            Icon(PleyIcons.ListPlus, contentDescription = null, size = 32.dp, tint = PleyColors.Muted)
            Spacer(Modifier.height(16.dp))
            BasicText(title, style = PleyType.Tv.PageTitle.copy(color = PleyColors.Fg))
            Spacer(Modifier.height(12.dp))
            BasicText(body, style = PleyType.Tv.BodyLarge.copy(color = PleyColors.Muted))
        }
        TvSidePanel(visible = true, onDismiss = onClose) {
            val name = rememberTextFieldState()
            var kind by remember { mutableStateOf(PlaylistKind.Video) }
            val field = remember { FocusRequester() }
            LaunchedEffect(Unit) {
                withFrameNanos { }
                withFrameNanos { }
                runCatching { field.requestFocus() }
            }
            val ok = Playlists.check(name.text.toString()) == null && !busy
            Column(Modifier.fillMaxSize().focusProperties { onExit = { cancelFocusChange() } }.focusGroup()) {
                TvPanelTitle(title)
                Box(Modifier.padding(horizontal = 4.dp, vertical = 6.dp)) {
                    TvTextField(
                        state = name,
                        placeholder = stringResource(R.string.tv_playlist_form_title),
                        imeAction = ImeAction.Done,
                        onAction = { if (ok) onCreate(name.text.toString(), kind) },
                        focusModifier = Modifier.focusRequester(field),
                        modifier = Modifier.testTag(TV_NEW_FIELD),
                    )
                }
                BasicText(
                    stringResource(R.string.tv_playlist_form_hint),
                    style = PleyType.Tv.Caption.copy(color = PleyColors.Muted),
                    modifier = Modifier.padding(start = 8.dp, top = 4.dp),
                )
                if (asksKind) {
                    TvPanelHeading(stringResource(R.string.tv_playlist_form_for))
                    TvPanelRow(stringResource(R.string.tv_playlist_form_video), onClick = { kind = PlaylistKind.Video }, icon = PleyIcons.ListVideo, checked = kind == PlaylistKind.Video)
                    TvPanelRow(stringResource(R.string.tv_playlist_form_music), onClick = { kind = PlaylistKind.Audio }, icon = PleyIcons.ListMusic, checked = kind == PlaylistKind.Audio)
                }
                Spacer(Modifier.height(16.dp))
                TvPanelRow(
                    stringResource(if (smart) R.string.tv_playlist_save else R.string.tv_playlist_form_create),
                    onClick = { onCreate(name.text.toString(), kind) },
                    icon = if (smart) PleyIcons.Save else PleyIcons.Plus,
                    enabled = ok,
                    modifier = Modifier.testTag(TV_NEW_CREATE),
                )
            }
        }
    }
}

/** How long "Added to …" shows before the panel closes. */
private const val DONE_MS = 1_400L

/** Text stays in the backdrop's 85 % scrim: x 96 to 560. */
private val TextColumn = PleyTv.TextColumnEnd - PleyTv.Gutter

internal const val TV_ADD = "pley.tv.playlists.add"
internal const val TV_ADD_FIELD = "pley.tv.playlists.add.new"
internal const val TV_ADD_ROW = "pley.tv.playlists.add.row."
internal const val TV_NEW = "pley.tv.playlists.new"
internal const val TV_NEW_FIELD = "pley.tv.playlists.new.title"
internal const val TV_NEW_CREATE = "pley.tv.playlists.new.create"
