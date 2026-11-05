package app.pley.android.ui.phone.requests

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.text.input.InputTransformation
import androidx.compose.foundation.text.input.TextFieldBuffer
import androidx.compose.foundation.text.input.TextFieldLineLimits
import androidx.compose.foundation.text.input.rememberTextFieldState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.icon.Icon
import app.pley.android.designsystem.icon.PleyIcon
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.designsystem.image.Artwork
import app.pley.android.designsystem.image.ArtworkImage
import app.pley.android.model.Approval
import app.pley.android.model.MediaRequest
import app.pley.android.model.RequestStatus
import app.pley.android.model.format.formatBytes
import app.pley.android.screens.R
import app.pley.android.screens.requests.ApproveDialog
import app.pley.android.screens.requests.CardAction
import app.pley.android.screens.requests.DECLINE_REASON_MAX
import app.pley.android.screens.requests.RequestTexts
import app.pley.android.screens.requests.cardActions
import app.pley.android.screens.requests.confirms
import app.pley.android.screens.requests.declineReasonEnd
import app.pley.android.screens.requests.readyElsewhere
import app.pley.android.ui.phone.kit.ConfirmDialog
import app.pley.android.ui.phone.kit.DelayedSpinner
import app.pley.android.ui.phone.kit.PleyIconButton
import app.pley.android.ui.phone.kit.PrimaryButton
import app.pley.android.ui.phone.kit.TonalButton

/**
 * One request: its poster ([poster]), title, year and
 * seasons, its stage pill (with the download's bar), who asked and when,
 * the reason or the note, where it went (managers), and its actions:
 * the main one or two as buttons, the rest under More actions. Tapping
 * the poster or title calls [onOpen]; Watch, shown once it's ready,
 * [onWatch]. [busy] holds the actions while one is on its way.
 */
@Composable
internal fun RequestCard(
    request: MediaRequest,
    origin: String,
    manage: Boolean,
    busy: Boolean,
    texts: RequestTexts,
    onOpen: (() -> Unit)?,
    onAction: (CardAction) -> Unit,
    modifier: Modifier = Modifier,
    poster: Boolean = true,
    onWatch: (() -> Unit)? = null,
) {
    val open = onOpen?.let { Modifier.clickable(role = Role.Button, onClick = it) } ?: Modifier
    Row(
        modifier
            .fillMaxWidth()
            .clip(PleyShape.Card)
            .background(PleyColors.Surface)
            .padding(12.dp)
            .testTag(REQUEST_CARD + request.id),
    ) {
        if (poster) {
            Artwork(
                image = remember(request.posterUrl, origin) { ArtworkImage.of(request.posterUrl, origin) },
                title = request.title,
                titleStyle = MaterialTheme.typography.labelSmall,
                modifier = Modifier
                    .width(POSTER_WIDTH)
                    .aspectRatio(2f / 3f)
                    .clip(PleyShape.TilePhone)
                    .then(open),
            )
            Spacer(Modifier.width(12.dp))
        }
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(6.dp)) {
            Column(open) {
                Text(
                    request.title,
                    style = MaterialTheme.typography.titleMedium,
                    color = PleyColors.Fg,
                    maxLines = 2,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.semantics { heading() },
                )
                val seasons = request.seasons?.takeIf { it.isNotEmpty() }?.let(texts::seasons)
                Text(
                    listOfNotNull(titleLine(request.year, request.kind, texts), seasons).joinToString(" · "),
                    style = MaterialTheme.typography.bodySmall,
                    color = PleyColors.Muted,
                )
            }
            RequestStage(request, manage, texts)
            if (readyElsewhere(request)) {
                Text(stringResource(R.string.requests_ready_elsewhere), style = MaterialTheme.typography.bodySmall, color = PleyColors.Muted)
            }
            Text(
                listOfNotNull(texts.asked(request, manage), request.watchers.takeIf { manage && it > 0 }?.let(texts::watchers)).joinToString(" · "),
                style = MaterialTheme.typography.bodySmall,
                color = PleyColors.Subtle,
            )
            val reason = request.reason?.trim()?.takeIf { it.isNotEmpty() && request.status == RequestStatus.Declined }
            if (reason != null) {
                Text(stringResource(R.string.requests_reason_quoted, reason), style = MaterialTheme.typography.bodyMedium.copy(fontStyle = FontStyle.Italic), color = PleyColors.Muted)
            }
            request.note?.takeIf { manage && it.isNotBlank() }?.let { note ->
                Text(note, style = MaterialTheme.typography.bodySmall, color = PleyColors.Muted)
            }
            if (manage) {
                texts.sentTo(request)?.let { Text(it, style = MaterialTheme.typography.bodySmall, color = PleyColors.Subtle) }
            }
            CardActions(cardActions(request, dialogs = true), busy, texts, onAction, onWatch)
        }
    }
}

/**
 * A card's actions: Watch (once it's ready where the person can open it),
 * Approve, Try again and Request again first; Decline, Withdraw, Remove and
 * Stop following beside them; the rest under More actions.
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun CardActions(actions: List<CardAction>, busy: Boolean, texts: RequestTexts, onAction: (CardAction) -> Unit, onWatch: (() -> Unit)?) {
    if (actions.isEmpty() && onWatch == null) return
    val (menu, shown) = actions.partition { it in MENU }
    FlowRow(
        Modifier.padding(top = 4.dp),
        horizontalArrangement = Arrangement.spacedBy(8.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
        itemVerticalAlignment = Alignment.CenterVertically,
    ) {
        if (onWatch != null) PrimaryButton(stringResource(R.string.requests_watch), onClick = onWatch, icon = PleyIcons.Play)
        shown.forEach { action ->
            if (action in FIRST) {
                PrimaryButton(texts.action(action), onClick = { onAction(action) }, icon = iconOf(action), enabled = !busy)
            } else {
                TonalButton(texts.action(action), onClick = { onAction(action) }, icon = iconOf(action), enabled = !busy)
            }
        }
        if (menu.isNotEmpty()) {
            Box {
                var open by remember { mutableStateOf(false) }
                PleyIconButton(PleyIcons.EllipsisVertical, stringResource(R.string.requests_more_actions), onClick = { open = true }, enabled = !busy, tint = PleyColors.Muted)
                DropdownMenu(expanded = open, onDismissRequest = { open = false }, containerColor = PleyColors.Surface3) {
                    menu.forEach { action ->
                        DropdownMenuItem(
                            text = { Text(texts.action(action), color = if (action == CardAction.Delete) PleyColors.DangerText else PleyColors.Fg) },
                            leadingIcon = { Icon(iconOf(action), contentDescription = null, size = 20.dp, tint = if (action == CardAction.Delete) PleyColors.DangerText else PleyColors.Fg) },
                            onClick = {
                                open = false
                                onAction(action)
                            },
                        )
                    }
                }
            }
        }
    }
}

private val FIRST = setOf(CardAction.Approve, CardAction.TryAgain, CardAction.AskAgain)

private val MENU = setOf(CardAction.ApproveWith, CardAction.Handled, CardAction.Delete, CardAction.DeclineWithReason)

private fun iconOf(action: CardAction): PleyIcon = when (action) {
    CardAction.Approve -> PleyIcons.Check
    CardAction.ApproveWith -> PleyIcons.SlidersHorizontal
    CardAction.TryAgain -> PleyIcons.RotateCw
    CardAction.Decline, CardAction.DeclineWithReason -> PleyIcons.X
    CardAction.Withdraw -> PleyIcons.X
    CardAction.Remove, CardAction.Delete -> PleyIcons.Trash2
    CardAction.StopFollowing -> PleyIcons.BookmarkMinus
    CardAction.Handled -> PleyIcons.CircleCheck
    CardAction.AskAgain -> PleyIcons.RefreshCw
}

/**
 * The way a screen does a card's actions: Withdraw, Remove and Delete ask
 * first; Decline asks for an optional reason; the rest go to [act] at
 * once (Approve… opens the ViewModel's dialog there).
 */
@Composable
internal fun rememberRequestActions(texts: RequestTexts, act: (MediaRequest, CardAction, String?) -> Unit): (MediaRequest, CardAction) -> Unit {
    var confirming by remember { mutableStateOf<Pair<MediaRequest, CardAction>?>(null) }
    var declining by remember { mutableStateOf<MediaRequest?>(null) }
    confirming?.let { (request, action) ->
        ConfirmDialog(
            title = texts.confirm(action, request.title).orEmpty(),
            body = null,
            confirmLabel = texts.action(action),
            onConfirm = {
                confirming = null
                act(request, action, null)
            },
            onDismiss = { confirming = null },
            destructive = true,
            dismissLabel = stringResource(R.string.requests_cancel),
        )
    }
    declining?.let { request ->
        DeclineDialog(
            title = request.title,
            onDecline = { reason ->
                declining = null
                act(request, CardAction.Decline, reason)
            },
            onDismiss = { declining = null },
        )
    }
    return remember(texts, act) {
        { request, action ->
            when {
                action.confirms -> confirming = request to action
                action == CardAction.Decline || action == CardAction.DeclineWithReason -> declining = request
                else -> act(request, action, null)
            }
        }
    }
}

/** Decline: an optional reason, which the person who asked sees. */
@Composable
internal fun DeclineDialog(title: String, onDecline: (String?) -> Unit, onDismiss: () -> Unit) {
    // Kept as the screen turns; a reason pasted past the server's limit keeps its first characters.
    val reason = rememberTextFieldState()
    AlertDialog(
        onDismissRequest = onDismiss,
        confirmButton = {
            TextButton(
                onClick = { onDecline(reason.text.toString().trim().takeIf { it.isNotEmpty() }) },
                colors = ButtonDefaults.textButtonColors(contentColor = PleyColors.DangerText),
            ) { Text(stringResource(R.string.requests_decline), style = MaterialTheme.typography.labelLarge) }
        },
        dismissButton = {
            TextButton(onClick = onDismiss, colors = ButtonDefaults.textButtonColors(contentColor = PleyColors.Fg)) {
                Text(stringResource(R.string.requests_cancel), style = MaterialTheme.typography.labelLarge)
            }
        },
        title = { Text(stringResource(R.string.requests_decline_title, title), style = MaterialTheme.typography.headlineSmall) },
        text = {
            OutlinedTextField(
                state = reason,
                modifier = Modifier.fillMaxWidth().testTag(DECLINE_REASON),
                label = { Text(stringResource(R.string.requests_decline_reason)) },
                supportingText = { Text(stringResource(R.string.requests_decline_reason_hint)) },
                inputTransformation = ReasonLength,
                lineLimits = TextFieldLineLimits.MultiLine(maxHeightInLines = 4),
                keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.Sentences),
                shape = PleyShape.Field,
                colors = OutlinedTextFieldDefaults.colors(
                    focusedBorderColor = PleyColors.AccentBright,
                    unfocusedBorderColor = PleyColors.Control,
                    focusedLabelColor = PleyColors.AccentBright,
                    unfocusedLabelColor = PleyColors.Muted,
                    cursorColor = PleyColors.AccentBright,
                    focusedSupportingTextColor = PleyColors.Muted,
                    unfocusedSupportingTextColor = PleyColors.Muted,
                ),
            )
        },
        shape = PleyShape.Dialog,
        containerColor = PleyColors.Surface2,
        titleContentColor = PleyColors.Fg,
        textContentColor = PleyColors.Muted,
        tonalElevation = 0.dp,
    )
}

/**
 * Approve… (phones): the quality profile and the folder
 * (starting on the settings' choices, anime's for anime), and for shows
 * the seasons asked for, all ticked. Without the service's lists (not
 * connected, or still reading them) it approves with the settings'
 * choices.
 */
@Composable
internal fun ApproveDialogView(dialog: ApproveDialog, texts: RequestTexts, onApprove: (Approval) -> Unit, onDismiss: () -> Unit) {
    val options = dialog.options
    var profile by remember(dialog.request.id, options) { mutableStateOf(dialog.profile) }
    var folder by remember(dialog.request.id, options) { mutableStateOf(dialog.folder) }
    var seasons by remember(dialog.request.id) { mutableStateOf(dialog.seasons.toSet()) }
    val ready = !dialog.loading && (dialog.seasons.isEmpty() || seasons.isNotEmpty())
    AlertDialog(
        onDismissRequest = onDismiss,
        confirmButton = {
            TextButton(
                onClick = { onApprove(dialog.approval(profile, folder, seasons.takeIf { dialog.seasons.isNotEmpty() })) },
                enabled = ready,
                colors = ButtonDefaults.textButtonColors(contentColor = PleyColors.AccentBright),
            ) { Text(stringResource(R.string.requests_approve), style = MaterialTheme.typography.labelLarge) }
        },
        dismissButton = {
            TextButton(onClick = onDismiss, colors = ButtonDefaults.textButtonColors(contentColor = PleyColors.Fg)) {
                Text(stringResource(R.string.requests_cancel), style = MaterialTheme.typography.labelLarge)
            }
        },
        title = { Text(stringResource(R.string.requests_approve_title, dialog.request.title), style = MaterialTheme.typography.headlineSmall) },
        text = {
            Column(Modifier.verticalScroll(rememberScrollState()).testTag(APPROVE_DIALOG)) {
                when {
                    dialog.loading -> Box(Modifier.fillMaxWidth().padding(vertical = 16.dp), contentAlignment = Alignment.Center) {
                        DelayedSpinner(stringResource(R.string.requests_approve_with))
                    }
                    options == null || !dialog.choices -> Text(stringResource(R.string.requests_approve_settings), style = MaterialTheme.typography.bodyMedium)
                    else -> {
                        if (options.qualityProfiles.isNotEmpty()) {
                            Heading(stringResource(R.string.requests_approve_profile))
                            if (dialog.profile == null) ChoiceRow(stringResource(R.string.requests_approve_default), null, profile == null) { profile = null }
                            options.qualityProfiles.forEach { p -> ChoiceRow(p.name, null, profile == p.id) { profile = p.id } }
                        }
                        if (options.rootFolders.isNotEmpty()) {
                            Heading(stringResource(R.string.requests_approve_folder))
                            if (dialog.folder == null) ChoiceRow(stringResource(R.string.requests_approve_default), null, folder == null) { folder = null }
                            options.rootFolders.forEach { f ->
                                val free = f.freeBytes?.let { stringResource(R.string.requests_folder_free, formatBytes(it)) }
                                ChoiceRow(f.path, free, folder == f.path) { folder = f.path }
                            }
                        }
                    }
                }
                options?.error?.takeIf { it.isNotBlank() }?.let { error ->
                    Text(error, style = MaterialTheme.typography.bodySmall, color = PleyColors.DangerText, modifier = Modifier.padding(top = 8.dp))
                }
                if (dialog.seasons.isNotEmpty()) {
                    Heading(stringResource(R.string.requests_approve_seasons))
                    dialog.seasons.forEach { number ->
                        CheckRow(texts.seasons(listOf(number)), null, checked = number in seasons, enabled = true) {
                            seasons = if (number in seasons) seasons - number else seasons + number
                        }
                    }
                }
            }
        },
        shape = PleyShape.Dialog,
        containerColor = PleyColors.Surface2,
        titleContentColor = PleyColors.Fg,
        textContentColor = PleyColors.Fg,
        tonalElevation = 0.dp,
    )
}

@Composable
private fun Heading(text: String) {
    Text(
        text,
        style = MaterialTheme.typography.labelLarge,
        color = PleyColors.Muted,
        modifier = Modifier.padding(top = 12.dp, bottom = 4.dp).semantics { heading() },
    )
}

/** One of a few, the chosen one with a check (as a library's choices). Screen readers hear a radio button. */
@Composable
private fun ChoiceRow(label: String, hint: String?, selected: Boolean, onClick: () -> Unit) {
    Row(
        Modifier
            .fillMaxWidth()
            .heightIn(min = 48.dp)
            .selectable(selected = selected, role = Role.RadioButton, onClick = onClick)
            .padding(vertical = 6.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(Modifier.weight(1f)) {
            Text(label, style = MaterialTheme.typography.bodyLarge, color = if (selected) PleyColors.Fg else PleyColors.Muted)
            if (hint != null) Text(hint, style = MaterialTheme.typography.bodySmall, color = PleyColors.Subtle)
        }
        Box(Modifier.size(24.dp), contentAlignment = Alignment.Center) {
            if (selected) Icon(PleyIcons.Check, contentDescription = null, size = 22.dp, tint = PleyColors.Accent)
        }
    }
}

/**
 * A box to tick before [label] (with [hint] under it): filled in
 * [accent][PleyColors.Accent] with a check when [checked]; a disabled row
 * shows, dimmed, and does nothing.
 */
@Composable
internal fun CheckRow(label: String, hint: String?, checked: Boolean, enabled: Boolean, modifier: Modifier = Modifier, onToggle: () -> Unit) {
    Row(
        modifier
            .fillMaxWidth()
            .heightIn(min = 48.dp)
            .toggleable(value = checked, enabled = enabled, role = Role.Checkbox, onValueChange = { onToggle() })
            .padding(vertical = 6.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(
            Modifier
                .size(22.dp)
                .then(
                    if (checked) {
                        Modifier.background(if (enabled) PleyColors.Accent else PleyColors.Disabled, PleyShape.Control)
                    } else {
                        Modifier.border(2.dp, if (enabled) PleyColors.Control else PleyColors.Surface4, PleyShape.Control)
                    },
                ),
            contentAlignment = Alignment.Center,
        ) {
            if (checked) Icon(PleyIcons.Check, contentDescription = null, size = 16.dp, tint = PleyColors.OnAccent)
        }
        Spacer(Modifier.width(16.dp))
        Column(Modifier.weight(1f)) {
            Text(label, style = MaterialTheme.typography.bodyLarge, color = if (enabled) PleyColors.Fg else PleyColors.Disabled)
            if (hint != null) Text(hint, style = MaterialTheme.typography.bodySmall, color = if (enabled) PleyColors.Muted else PleyColors.Disabled)
        }
    }
}

/**
 * The number on the way into requests: [accent][PleyColors.Accent], or
 * [danger][PleyColors.Danger] while failed requests wait for a manager
 * ([alarm]). Screen readers say [description] instead: the colour and a
 * capped number say too little.
 */
@Composable
internal fun CountBadge(count: Int, alarm: Boolean, description: String, modifier: Modifier = Modifier) {
    Box(
        modifier
            .clearAndSetSemantics { contentDescription = description }
            .heightIn(min = 20.dp)
            .widthIn(min = 20.dp)
            .background(if (alarm) PleyColors.Danger else PleyColors.Accent, PleyShape.Circle)
            .padding(horizontal = 6.dp),
        contentAlignment = Alignment.Center,
    ) {
        Text(
            if (count > MAX_COUNT) "$MAX_COUNT+" else count.toString(),
            style = MaterialTheme.typography.labelSmall,
            color = if (alarm) PleyColors.Bg else PleyColors.OnAccent,
        )
    }
}

/** What a badge counts to before it says "99+". */
private const val MAX_COUNT = 99

/** A reason past [DECLINE_REASON_MAX] characters (pasted, say) keeps its first ones ([declineReasonEnd]). */
private object ReasonLength : InputTransformation {
    override fun TextFieldBuffer.transformInput() {
        val end = declineReasonEnd(asCharSequence())
        if (end < length) replace(end, length, "")
    }
}

private val POSTER_WIDTH = 64.dp

/** A card's widest, on tablets: its words and its buttons stay together. */
internal val REQUEST_CARD_MAX = 720.dp

internal const val REQUEST_CARD = "pley.requests.card."
internal const val DECLINE_REASON = "pley.requests.declineReason"
internal const val APPROVE_DIALOG = "pley.requests.approve"
