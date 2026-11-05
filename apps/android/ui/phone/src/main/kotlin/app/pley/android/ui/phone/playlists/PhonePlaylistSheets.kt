package app.pley.android.ui.phone.playlists

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import app.pley.android.common.UiText
import app.pley.android.common.errorSentence
import app.pley.android.data.Loadable
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.icon.Icon
import app.pley.android.designsystem.icon.PleyIcon
import app.pley.android.designsystem.icon.PleyIcons
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
import app.pley.android.ui.phone.R
import app.pley.android.ui.phone.kit.PleyTextButton
import app.pley.android.ui.phone.kit.PrimaryButton
import app.pley.android.ui.phone.library.joined
import app.pley.android.ui.phone.shell.LocalPhoneNavigator
import app.pley.android.ui.phone.shell.LocalPhoneShell

/**
 * Add to playlist, as a bottom sheet (PleyKey.AddToPlaylist; the web's
 * AddToPlaylistModal): the full title of what is added, a new playlist
 * named on the spot, then the playlists that take it. Choosing one adds
 * and closes the sheet, leaving "Added to Road trip" (or "Added 8 items
 * to Road trip", or "Nothing to add") in a toast.
 */
@Composable
internal fun AddToPlaylistSheet(itemIds: List<Long>) {
    val vm: AddToPlaylistViewModel = viewModel(factory = AddToPlaylistViewModel.factory(itemIds))
    val state by vm.state.collectAsStateWithLifecycle()
    val navigator = LocalPhoneNavigator.current
    val shell = LocalPhoneShell.current
    val resources = LocalResources.current
    LaunchedEffect(vm) {
        vm.events.collect { event ->
            when (event) {
                is AddEvent.Added -> {
                    shell.toasts.show(addedMessage(resources, event))
                    navigator.back()
                }
                is AddEvent.Failed -> shell.report(event.error)
            }
        }
    }
    AddToPlaylistContent(state, onCreate = vm::create, onAdd = vm::add)
}

/** The toast's words for [event], as the web says them. */
internal fun addedMessage(resources: android.content.res.Resources, event: AddEvent.Added): String = when (val n = event.entries) {
    0 -> resources.getString(R.string.add_nothing)
    null, 1 -> resources.getString(R.string.add_added, event.playlist)
    else -> resources.getQuantityString(R.plurals.add_added_many, n, n, event.playlist)
}

@Composable
internal fun AddToPlaylistContent(state: AddToPlaylistUiState, onCreate: (String) -> Boolean, onAdd: (PlaylistCard) -> Unit) {
    var name by rememberSaveable { mutableStateOf("") }
    val music = state.kind == PlaylistKind.Audio
    Column(Modifier.fillMaxWidth().imePadding().testTag(ADD_SHEET)) {
        SheetTitle(stringResource(R.string.add_to_playlist), state.titleText?.asString())
        Row(
            Modifier.fillMaxWidth().padding(horizontal = 24.dp, vertical = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            TitleField(
                value = name,
                onValueChange = { name = it },
                onDone = { if (onCreate(name)) name = "" },
                label = null,
                placeholder = stringResource(if (music) R.string.add_new_music else R.string.add_new_video),
                enabled = state.kind != null && !state.busy,
                modifier = Modifier.weight(1f).testTag(ADD_NEW_FIELD),
            )
            PrimaryButton(
                stringResource(R.string.add_create),
                onClick = { onCreate(name) },
                icon = PleyIcons.Plus,
                enabled = state.kind != null && !state.busy && Playlists.check(name) == null,
            )
        }
        Spacer(Modifier.height(8.dp))
        when (val playlists = state.playlists) {
            Loadable.Loading -> SkeletonRows()
            is Loadable.Failed -> if (playlists.stale == null) {
                Text(
                    errorSentence(playlists.error).asString(),
                    style = MaterialTheme.typography.bodyMedium,
                    color = PleyColors.DangerText,
                    modifier = Modifier.padding(horizontal = 24.dp, vertical = 8.dp),
                )
            } else {
                PlaylistRows(playlists.stale.orEmpty(), state.busy, onAdd)
            }
            is Loadable.Ready -> if (playlists.value.isEmpty() && state.kind != null) {
                Text(
                    stringResource(if (music) R.string.add_none_music else R.string.add_none_video),
                    style = MaterialTheme.typography.bodyMedium,
                    color = PleyColors.Muted,
                    modifier = Modifier.padding(horizontal = 24.dp, vertical = 12.dp),
                )
            } else {
                PlaylistRows(playlists.value, state.busy, onAdd)
            }
        }
        Spacer(Modifier.height(8.dp))
    }
}

/** The playlists to add to: each a 48 dp cover, its title and "12 items · 3h 12m". */
@Composable
private fun PlaylistRows(list: List<PlaylistCard>, busy: Boolean, onAdd: (PlaylistCard) -> Unit) {
    LazyColumn(Modifier.fillMaxWidth().heightIn(max = 360.dp)) {
        items(list, key = { it.id }, contentType = { "playlist" }) { card ->
            val meta = card.metaTexts.joined(LocalResources.current)
            Row(
                Modifier
                    .fillMaxWidth()
                    .heightIn(min = 64.dp)
                    .alpha(if (busy) 0.5f else 1f)
                    .clickable(enabled = !busy, role = Role.Button) { onAdd(card) }
                    .padding(horizontal = 24.dp, vertical = 8.dp)
                    .testTag(ADD_ROW + card.id),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                PlaylistArt(card.artwork, card.origin, card.video, card.title, Modifier.size(48.dp), glyph = 20.dp)
                Spacer(Modifier.width(16.dp))
                Column(Modifier.weight(1f)) {
                    Text(card.title, style = MaterialTheme.typography.titleMedium, color = PleyColors.Fg, maxLines = 1, overflow = TextOverflow.Ellipsis)
                    if (meta.isNotEmpty()) {
                        Text(meta, style = MaterialTheme.typography.bodySmall, color = PleyColors.Muted, maxLines = 1)
                    }
                }
            }
        }
    }
}

@Composable
private fun SkeletonRows() {
    val skeleton = rememberSkeleton(animated = true)
    Column(Modifier.padding(horizontal = 24.dp, vertical = 8.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
        repeat(2) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Box(Modifier.size(48.dp).skeleton(skeleton, PleyShape.TilePhone))
                Spacer(Modifier.width(16.dp))
                Box(Modifier.size(160.dp, 14.dp).skeleton(skeleton, PleyShape.Badge))
            }
        }
    }
}

/**
 * New playlist and Save these filters, as a bottom sheet
 * (PleyKey.NewPlaylist, PleyKey.SaveSmartPlaylist): a title of at most
 * 200 characters and, for an empty playlist, what it plays (Movies & TV
 * or Music). Creating closes the sheet and opens the playlist.
 */
@Composable
internal fun NewPlaylistSheet(mode: NewPlaylistMode) {
    val vm: NewPlaylistViewModel = viewModel(factory = NewPlaylistViewModel.factory(mode))
    val busy by vm.busy.collectAsStateWithLifecycle()
    val navigator = LocalPhoneNavigator.current
    val shell = LocalPhoneShell.current
    LaunchedEffect(vm) {
        vm.events.collect { event ->
            when (event) {
                is CreateEvent.Created -> {
                    navigator.back()
                    navigator.open(PleyKey.Playlist(event.playlistId))
                }
                is CreateEvent.Failed -> shell.report(event.error)
            }
        }
    }
    NewPlaylistContent(
        smart = mode is NewPlaylistMode.Smart,
        filters = vm.filterTexts,
        asksKind = vm.asksKind,
        busy = busy,
        onCreate = vm::create,
        onCancel = navigator::back,
    )
}

@Composable
internal fun NewPlaylistContent(
    smart: Boolean,
    filters: List<UiText>,
    asksKind: Boolean,
    busy: Boolean,
    onCreate: (String, PlaylistKind) -> Boolean,
    onCancel: () -> Unit,
) {
    var title by rememberSaveable { mutableStateOf("") }
    var kind by rememberSaveable { mutableStateOf(PlaylistKind.Video) }
    val focus = remember { FocusRequester() }
    LaunchedEffect(Unit) { runCatching { focus.requestFocus() } }
    val ok = Playlists.check(title) == null && !busy
    Column(Modifier.fillMaxWidth().imePadding().padding(horizontal = 24.dp).testTag(NEW_SHEET)) {
        Text(
            stringResource(if (smart) R.string.playlist_smart_title else R.string.playlists_new),
            style = MaterialTheme.typography.titleLarge,
            color = PleyColors.Fg,
            modifier = Modifier.padding(top = 4.dp, bottom = 12.dp),
        )
        if (smart) {
            Text(
                if (filters.isEmpty()) {
                    stringResource(R.string.playlist_smart_body_plain)
                } else {
                    stringResource(R.string.playlist_smart_body, filters.joined(LocalResources.current))
                },
                style = MaterialTheme.typography.bodyMedium,
                color = PleyColors.Muted,
                modifier = Modifier.padding(bottom = 16.dp),
            )
        }
        TitleField(
            value = title,
            onValueChange = { title = it },
            onDone = { if (ok) onCreate(title, kind) },
            enabled = !busy,
            modifier = Modifier.focusRequester(focus).testTag(NEW_FIELD),
        )
        if (asksKind) {
            Spacer(Modifier.height(20.dp))
            Text(stringResource(R.string.playlist_form_for), style = MaterialTheme.typography.titleSmall, color = PleyColors.Fg)
            Spacer(Modifier.height(8.dp))
            Row(Modifier.fillMaxWidth().selectableGroup(), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                KindChoice(PleyIcons.ListVideo, stringResource(R.string.playlist_form_video), kind == PlaylistKind.Video, Modifier.weight(1f)) { kind = PlaylistKind.Video }
                KindChoice(PleyIcons.ListMusic, stringResource(R.string.playlist_form_music), kind == PlaylistKind.Audio, Modifier.weight(1f)) { kind = PlaylistKind.Audio }
            }
        }
        Spacer(Modifier.height(20.dp))
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(8.dp, Alignment.End), verticalAlignment = Alignment.CenterVertically) {
            PleyTextButton(stringResource(R.string.kit_cancel), onClick = onCancel)
            PrimaryButton(
                stringResource(if (smart) R.string.playlist_form_save else R.string.playlist_form_create),
                onClick = { onCreate(title, kind) },
                enabled = ok,
            )
        }
        Spacer(Modifier.height(8.dp))
    }
}

/** One of the two kinds: a 48 dp box with its glyph and name, accent-edged when chosen. */
@Composable
private fun KindChoice(icon: PleyIcon, label: String, selected: Boolean, modifier: Modifier, onClick: () -> Unit) {
    Row(
        modifier
            .height(48.dp)
            .background(if (selected) PleyColors.PrimaryContainer else PleyColors.Surface2, PleyShape.Field)
            .border(if (selected) 2.dp else 1.dp, if (selected) PleyColors.Accent else PleyColors.Control, PleyShape.Field)
            .selectable(selected = selected, role = Role.RadioButton, onClick = onClick),
        horizontalArrangement = Arrangement.Center,
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(icon, contentDescription = null, size = 18.dp, tint = if (selected) PleyColors.AccentBright else PleyColors.Muted)
        Spacer(Modifier.width(8.dp))
        Text(label, style = MaterialTheme.typography.labelLarge, color = if (selected) PleyColors.Fg else PleyColors.Muted)
    }
}

/** The sheet's title in titleLarge, and what it is for under it. */
@Composable
private fun SheetTitle(title: String, subtitle: String?) {
    Column(Modifier.padding(start = 24.dp, end = 24.dp, top = 4.dp, bottom = 4.dp)) {
        Text(title, style = MaterialTheme.typography.titleLarge, color = PleyColors.Fg)
        if (subtitle == null) {
            Box(Modifier.padding(top = 6.dp).size(200.dp, 14.dp).skeleton(rememberSkeleton(animated = true), PleyShape.Badge))
        } else if (subtitle.isNotEmpty()) {
            Text(subtitle, style = MaterialTheme.typography.bodyMedium, color = PleyColors.Muted, maxLines = 2, overflow = TextOverflow.Ellipsis)
        }
    }
}

internal const val ADD_SHEET = "pley.playlists.add"
internal const val ADD_NEW_FIELD = "pley.playlists.add.new"
internal const val ADD_ROW = "pley.playlists.add.row."
internal const val NEW_SHEET = "pley.playlists.new"
internal const val NEW_FIELD = "pley.playlists.new.title"
