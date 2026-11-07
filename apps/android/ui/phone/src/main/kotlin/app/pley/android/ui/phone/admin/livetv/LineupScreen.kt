package app.pley.android.ui.phone.admin.livetv

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.input.rememberTextFieldState
import androidx.compose.material3.Checkbox
import androidx.compose.material3.CheckboxDefaults
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.designsystem.image.ArtworkImage
import app.pley.android.designsystem.tile.ChannelLogo
import app.pley.android.model.admin.LineupChannel
import app.pley.android.model.admin.LineupState
import app.pley.android.screens.admin.livetv.LineupList
import app.pley.android.screens.admin.livetv.LineupViewModel
import app.pley.android.screens.admin.valueOrNull
import app.pley.android.ui.phone.R
import app.pley.android.ui.phone.account.Choice
import app.pley.android.ui.phone.account.ChoiceSheet
import app.pley.android.ui.phone.account.RowDivider
import app.pley.android.ui.phone.account.SettingsGroup
import app.pley.android.ui.phone.account.SettingsRow
import app.pley.android.ui.phone.admin.AdminEvents
import app.pley.android.ui.phone.admin.Entry
import app.pley.android.ui.phone.admin.TextEntryDialog
import app.pley.android.ui.phone.admin.adminWidth
import app.pley.android.ui.phone.kit.EmptyState
import app.pley.android.ui.phone.kit.ErrorState
import app.pley.android.ui.phone.kit.PleyChip
import app.pley.android.ui.phone.kit.PleyIconButton
import app.pley.android.ui.phone.kit.PleySheet
import app.pley.android.ui.phone.kit.PleyTextButton
import app.pley.android.ui.phone.kit.PrimaryButton
import app.pley.android.ui.phone.search.SearchField
import app.pley.android.ui.phone.theme.LocalPhoneMetrics
import kotlinx.coroutines.flow.distinctUntilChanged

/** The filters chosen in a sheet. */
private enum class LineupPick { Group, State }

/**
 * Live TV's Channels, as the web's tab: the lineup's counts, a field to
 * find a channel, the group and state it lists, then the
 * channels, read a page at a time as the list scrolls. Each channel's eye
 * hides or shows it; chosen ones (or every one listed) are hidden or
 * shown together; a channel opens its editor (name, number, guide
 * channel), saved with one button.
 */
@Composable
internal fun LineupScreen() {
    val model: LineupViewModel = viewModel(factory = LineupViewModel.factory)
    val summary by model.summary.state.collectAsStateWithLifecycle()
    val list by model.list.collectAsStateWithLifecycle()
    val choice by model.choice.collectAsStateWithLifecycle()
    val editing by model.editing.collectAsStateWithLifecycle()
    val groups by model.groups.state.collectAsStateWithLifecycle()
    val busy by model.busy.collectAsStateWithLifecycle()
    val query by model.query.collectAsStateWithLifecycle()
    val metrics = LocalPhoneMetrics.current
    val field = rememberTextFieldState(query)
    var picking by rememberSaveable { mutableStateOf<LineupPick?>(null) }
    val rows = rememberLazyListState()
    LaunchedEffect(field) { snapshotFlow { field.text.toString() }.collect(model::find) }
    LaunchedEffect(rows) {
        snapshotFlow { rows.layoutInfo.visibleItemsInfo.let { (it.firstOrNull()?.index ?: 0) to (it.lastOrNull()?.index ?: 0) } }
            .distinctUntilChanged()
            .collect { (first, last) -> model.visible(first, last) }
    }
    // Another filter lists from the top.
    LaunchedEffect(list.filter) { rows.scrollToItem(0) }
    AdminEvents(model, liveTvSaid())
    val groupNames = remember(groups) { groups.valueOrNull.orEmpty().map { it.name }.filter { it.isNotEmpty() } }
    LiveListPage(
        title = stringResource(R.string.admin_livetv_tab_channels),
        load = summary,
        onRetry = model.summary::reload,
        tag = LIVETV_LINEUP_PAGE,
        list = rows,
        header = { all ->
            if (all.total > 0) {
                val margins = Modifier.padding(start = metrics.marginStart, end = metrics.marginEnd)
                val shownCount = all.total - all.hidden
                PageLine(
                    pluralStringResource(R.plurals.admin_livetv_lineup_shown, shownCount, shownCount) +
                        (if (all.hidden > 0) pluralStringResource(R.plurals.admin_livetv_lineup_also_hidden, all.hidden, all.hidden) else "") +
                        ". " + stringResource(R.string.admin_livetv_lineup_hint),
                    margins,
                )
                SearchField(field, stringResource(R.string.admin_livetv_lineup_find), onSubmit = {}, autoFocus = false, tag = LIVETV_LINEUP_FIND)
                Row(
                    Modifier.horizontalScroll(rememberScrollState()).then(margins).padding(bottom = 8.dp),
                    horizontalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    val filter = list.filter
                    if (groupNames.size > 1) {
                        PleyChip(
                            filter.group ?: stringResource(R.string.admin_livetv_lineup_every_group),
                            selected = filter.group != null,
                            onClick = { picking = LineupPick.Group },
                            dropdown = true,
                        )
                    }
                    PleyChip(stateName(filter.state), selected = filter.state != LineupState.All, onClick = { picking = LineupPick.State }, dropdown = true)
                }
                ChoiceBar(list, choice.count(list.total ?: 0), choice.all, busy == 0, onAll = model::chooseAll, onHidden = model::setChosenHidden)
            }
        },
    ) { all ->
        val total = list.total
        val error = list.error
        when {
            all.total == 0 -> item(key = "none") {
                EmptyState(stringResource(R.string.admin_livetv_lineup_none_title), stringResource(R.string.admin_livetv_lineup_none_body), icon = PleyIcons.ListVideo)
            }
            error != null && total == null -> item(key = "error") { ErrorState(stringResource(R.string.admin_load_failed), error, model::retry) }
            total == 0 -> item(key = "nomatch") {
                Text(
                    stringResource(R.string.admin_livetv_lineup_no_match),
                    style = MaterialTheme.typography.bodyMedium,
                    color = PleyColors.Muted,
                    modifier = Modifier.padding(24.dp),
                )
            }
            total != null -> items(count = total, key = { it }, contentType = { "channel" }) { index ->
                // Rows go by their place: a page read again after a change may move a channel to another.
                val channel = list.at(index)
                if (channel == null) {
                    PendingRow()
                } else {
                    ChannelRow(
                        channel,
                        origin = model.origin,
                        chosen = channel.id in choice,
                        onChoose = { model.choose(channel.id) },
                        onHidden = { model.setHidden(channel, !channel.hidden) },
                        onEdit = { model.edit(channel) },
                    )
                }
            }
        }
    }
    when (picking) {
        LineupPick.Group -> ChoiceSheet(
            title = stringResource(R.string.admin_livetv_lineup_group),
            choices = listOf(Choice<String?>(null, stringResource(R.string.admin_livetv_lineup_every_group))) + groupNames.map { Choice<String?>(it, it) },
            selected = list.filter.group,
            onPick = model::setGroup,
            onDismiss = { picking = null },
        )
        LineupPick.State -> ChoiceSheet(
            title = stringResource(R.string.admin_livetv_lineup_which),
            choices = STATES.map { Choice(it, stateName(it)) },
            selected = list.filter.state,
            onPick = model::setState,
            onDismiss = { picking = null },
        )
        null -> Unit
    }
    editing?.let { channel -> ChannelEditor(channel, busy = busy > 0, onSave = model::save, onDismiss = { model.edit(null) }) }
}

/** The states the lineup lists, in the web's order. */
private val STATES = listOf(LineupState.All, LineupState.Shown, LineupState.Hidden, LineupState.NoGuide, LineupState.Offline)

@Composable
private fun stateName(state: LineupState): String = stringResource(
    when (state) {
        LineupState.Shown -> R.string.admin_livetv_lineup_shown_only
        LineupState.Hidden -> R.string.admin_livetv_lineup_hidden_only
        LineupState.NoGuide -> R.string.admin_livetv_lineup_without_guide
        LineupState.Offline -> R.string.admin_livetv_lineup_offline
        else -> R.string.admin_livetv_lineup_shown_and_hidden
    },
)

/** Over the list: choosing every channel listed, and how many are listed, or how many are chosen with Hide and Show. */
@Composable
private fun ChoiceBar(list: LineupList, chosen: Int, all: Boolean, enabled: Boolean, onAll: () -> Unit, onHidden: (Boolean) -> Unit) {
    val total = list.total
    val chooseAll = stringResource(R.string.admin_livetv_lineup_choose_all)
    Column(Modifier.fillMaxWidth().background(PleyColors.Surface2)) {
        Row(Modifier.fillMaxWidth().heightIn(min = 48.dp).padding(start = 4.dp, end = 8.dp), verticalAlignment = Alignment.CenterVertically) {
            Checkbox(
                checked = all && (total ?: 0) > 0,
                onCheckedChange = { onAll() },
                enabled = (total ?: 0) > 0,
                colors = checkboxColors(),
                modifier = Modifier.semantics { contentDescription = chooseAll },
            )
            if (chosen > 0) {
                Text(
                    pluralStringResource(R.plurals.admin_livetv_lineup_chosen, chosen, chosen),
                    style = MaterialTheme.typography.bodyMedium,
                    color = PleyColors.Fg,
                    modifier = Modifier.weight(1f),
                )
                PleyTextButton(stringResource(R.string.admin_livetv_lineup_hide), onClick = { onHidden(true) }, enabled = enabled)
                PleyTextButton(stringResource(R.string.admin_livetv_lineup_show), onClick = { onHidden(false) }, enabled = enabled)
            } else {
                Text(
                    total?.let { pluralStringResource(R.plurals.admin_livetv_channels_count, it, it) } ?: "…",
                    style = MaterialTheme.typography.bodyMedium,
                    color = PleyColors.Subtle,
                    modifier = Modifier.weight(1f),
                )
            }
        }
        HorizontalDivider(thickness = 1.dp, color = PleyColors.Surface4)
    }
}

/** A channel: chosen or not, its logo, name, number and group, offline, its guide, and the eye that hides or shows it. */
@Composable
private fun ChannelRow(channel: LineupChannel, origin: String, chosen: Boolean, onChoose: () -> Unit, onHidden: () -> Unit, onEdit: () -> Unit) {
    val about = listOfNotNull(
        channel.number,
        channel.group.ifEmpty { null },
        channel.customName?.let { stringResource(R.string.admin_livetv_lineup_at_source, channel.sourceName) },
    ).joinToString(" · ")
    Column(Modifier.adminWidth) {
        Row(
            Modifier
                .fillMaxWidth()
                .height(ROW)
                .clickable(role = Role.Button, onClick = onEdit)
                .padding(start = 4.dp, end = 4.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Checkbox(
                checked = chosen,
                onCheckedChange = { onChoose() },
                colors = checkboxColors(),
                modifier = Modifier.semantics { contentDescription = channel.name },
            )
            Row(Modifier.weight(1f).alpha(if (channel.hidden) 0.5f else 1f), verticalAlignment = Alignment.CenterVertically) {
                ChannelLogo(
                    image = ArtworkImage.of(channel.logo, origin),
                    label = channel.number ?: channel.name.take(3),
                    textStyle = MaterialTheme.typography.labelSmall,
                    modifier = Modifier.size(width = 48.dp, height = 32.dp).clip(PleyShape.Field).background(PleyColors.Surface3),
                )
                Spacer(Modifier.width(12.dp))
                Column(Modifier.weight(1f)) {
                    Text(channel.name, style = MaterialTheme.typography.bodyLarge, color = PleyColors.Fg, maxLines = 1, overflow = TextOverflow.Ellipsis)
                    Row {
                        if (channel.offline) {
                            Text(stringResource(R.string.admin_livetv_lineup_offline), style = MaterialTheme.typography.bodySmall, color = PleyColors.DangerText, maxLines = 1)
                            Text(" · ", style = MaterialTheme.typography.bodySmall, color = PleyColors.Subtle)
                        }
                        Text(
                            stringResource(if (channel.hasGuide) R.string.admin_livetv_lineup_guide else R.string.admin_livetv_no_guide),
                            style = MaterialTheme.typography.bodySmall,
                            color = if (channel.hasGuide) PleyColors.Success else PleyColors.Subtle,
                            maxLines = 1,
                        )
                        if (about.isNotEmpty()) {
                            Text(" · $about", style = MaterialTheme.typography.bodySmall, color = PleyColors.Subtle, maxLines = 1, overflow = TextOverflow.Ellipsis)
                        }
                    }
                }
            }
            PleyIconButton(
                if (channel.hidden) PleyIcons.EyeOff else PleyIcons.Eye,
                stringResource(if (channel.hidden) R.string.admin_livetv_show_name else R.string.admin_livetv_hide_name, channel.name),
                onClick = onHidden,
                tint = if (channel.hidden) PleyColors.Subtle else PleyColors.Muted,
            )
        }
        HorizontalDivider(thickness = 1.dp, color = PleyColors.Surface4)
    }
}

/** A channel whose page is being read. */
@Composable
private fun PendingRow() {
    Column(Modifier.adminWidth) {
        Row(Modifier.fillMaxWidth().height(ROW).padding(start = 52.dp), verticalAlignment = Alignment.CenterVertically) {
            Box(Modifier.size(width = 48.dp, height = 32.dp).background(PleyColors.Surface3, PleyShape.Field))
            Spacer(Modifier.width(12.dp))
            Box(Modifier.size(width = 140.dp, height = 14.dp).background(PleyColors.Surface3, PleyShape.Badge))
        }
        HorizontalDivider(thickness = 1.dp, color = PleyColors.Surface4)
    }
}

/**
 * A channel's name, number and guide channel, as set here (empty: the
 * source's), each typed in its dialog and saved together.
 */
@Composable
private fun ChannelEditor(channel: LineupChannel, busy: Boolean, onSave: (LineupChannel, String, String, String) -> Unit, onDismiss: () -> Unit) {
    var name by rememberSaveable(channel.id) { mutableStateOf(channel.customName.orEmpty()) }
    var number by rememberSaveable(channel.id) { mutableStateOf(channel.customNumber.orEmpty()) }
    var guide by rememberSaveable(channel.id) { mutableStateOf(channel.customGuideId.orEmpty()) }
    var typing by rememberSaveable(channel.id) { mutableStateOf<Int?>(null) }
    val metrics = LocalPhoneMetrics.current
    val nameHint = stringResource(R.string.admin_livetv_editor_name_hint)
    val numberHint = channel.sourceNumber?.let { stringResource(R.string.admin_livetv_editor_number_hint, it) }
        ?: stringResource(R.string.admin_livetv_editor_number_hint_none)
    val guideHint = when {
        channel.sourceGuideId.isNotEmpty() -> stringResource(R.string.admin_livetv_editor_guide_hint_source, channel.sourceGuideId)
        channel.matchedGuideId != null -> stringResource(R.string.admin_livetv_editor_guide_hint_matched, channel.matchedGuideId.orEmpty())
        else -> stringResource(R.string.admin_livetv_editor_guide_hint_none)
    }
    PleySheet(onDismiss = onDismiss, title = channel.sourceName) {
        SettingsGroup(null, Modifier.padding(horizontal = metrics.margin), footer = stringResource(R.string.admin_livetv_editor_note)) {
            SettingsRow(stringResource(R.string.admin_livetv_editor_name), value = name.ifEmpty { nameHint }, onClick = { typing = 0 })
            RowDivider(inset = false)
            SettingsRow(stringResource(R.string.admin_livetv_editor_number), value = number.ifEmpty { numberHint }, onClick = { typing = 1 })
            RowDivider(inset = false)
            SettingsRow(stringResource(R.string.admin_livetv_editor_guide), value = guide.ifEmpty { guideHint }, onClick = { typing = 2 })
        }
        Row(Modifier.fillMaxWidth().padding(horizontal = metrics.margin, vertical = 16.dp), horizontalArrangement = Arrangement.End) {
            PrimaryButton(
                stringResource(R.string.admin_save),
                onClick = { onSave(channel, name, number, guide) },
                enabled = !busy,
                modifier = Modifier.testTag(LIVETV_EDITOR_SAVE),
            )
        }
    }
    when (typing) {
        0 -> TextEntryDialog(
            title = stringResource(R.string.admin_livetv_editor_name),
            initial = name,
            onSave = {
                name = it
                typing = null
            },
            onDismiss = { typing = null },
            hint = nameHint,
        )
        1 -> TextEntryDialog(
            title = stringResource(R.string.admin_livetv_editor_number),
            initial = number,
            onSave = {
                number = it
                typing = null
            },
            onDismiss = { typing = null },
            hint = numberHint,
        )
        2 -> TextEntryDialog(
            title = stringResource(R.string.admin_livetv_editor_guide),
            initial = guide,
            onSave = {
                guide = it
                typing = null
            },
            onDismiss = { typing = null },
            hint = guideHint,
            entry = Entry.Text,
        )
    }
}

@Composable
private fun checkboxColors() = CheckboxDefaults.colors(
    checkedColor = PleyColors.Accent,
    uncheckedColor = PleyColors.Control,
    checkmarkColor = PleyColors.OnAccent,
)

private val ROW = 64.dp

internal const val LIVETV_LINEUP_PAGE = "pley.admin.livetv.channels"
internal const val LIVETV_LINEUP_FIND = "pley.admin.livetv.channels.find"
internal const val LIVETV_EDITOR_SAVE = "pley.admin.livetv.editor.save"
