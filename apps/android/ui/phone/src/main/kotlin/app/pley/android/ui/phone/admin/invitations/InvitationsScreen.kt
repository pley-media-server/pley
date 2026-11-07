package app.pley.android.ui.phone.admin.invitations

import android.content.ClipData
import android.content.Context
import android.content.Intent
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.ClipEntry
import androidx.compose.ui.platform.LocalClipboard
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.icon.Icon
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.designsystem.text.formatWords
import app.pley.android.model.Library
import app.pley.android.model.admin.Invitation
import app.pley.android.model.format.FormatWords
import app.pley.android.model.format.formatRelative
import app.pley.android.screens.PleyKey
import app.pley.android.screens.admin.AdminSection
import app.pley.android.screens.admin.invitations.EXPIRY_DAYS
import app.pley.android.screens.admin.invitations.InvitationState
import app.pley.android.screens.admin.invitations.InvitationsViewModel
import app.pley.android.screens.admin.invitations.MAX_USES
import app.pley.android.screens.admin.invitations.NewInvitationProblem
import app.pley.android.screens.admin.invitations.NewInvitationViewModel
import app.pley.android.screens.admin.invitations.stateOf
import app.pley.android.screens.admin.valueOrNull
import app.pley.android.ui.phone.R
import app.pley.android.ui.phone.account.Choice
import app.pley.android.ui.phone.account.ChoiceSheet
import app.pley.android.ui.phone.account.RowDivider
import app.pley.android.ui.phone.account.SettingsGroup
import app.pley.android.ui.phone.account.SettingsRow
import app.pley.android.ui.phone.admin.AdminEvents
import app.pley.android.ui.phone.admin.AdminPage
import app.pley.android.ui.phone.admin.Entry
import app.pley.android.ui.phone.admin.TextEntryDialog
import app.pley.android.ui.phone.admin.adminWidth
import app.pley.android.ui.phone.admin.numberIn
import app.pley.android.ui.phone.admin.numberProblem
import app.pley.android.ui.phone.admin.users.AccessGroup
import app.pley.android.ui.phone.admin.users.ShownAgain
import app.pley.android.ui.phone.admin.users.Tag
import app.pley.android.ui.phone.admin.users.TagTone
import app.pley.android.ui.phone.admin.users.ageTag
import app.pley.android.ui.phone.admin.users.librariesLabel
import app.pley.android.ui.phone.kit.ConfirmDialog
import app.pley.android.ui.phone.kit.EmptyState
import app.pley.android.ui.phone.kit.PleyIconButton
import app.pley.android.ui.phone.kit.PleyTextButton
import app.pley.android.ui.phone.kit.PrimaryButton
import app.pley.android.ui.phone.shell.LocalPhoneNavigator
import app.pley.android.ui.phone.shell.LocalPhoneShell
import kotlinx.coroutines.launch

/**
 * Invitations (the web's Settings → Invitations): the links that make an
 * account, with their state; [PleyKey.Manage.NEW] makes one.
 */
@Composable
internal fun InvitationsScreen(id: String?) {
    if (id == PleyKey.Manage.NEW) NewInvitationPage() else InvitationsList()
}

/** Every link, newest first: its code, state and access, its link to copy, and deleting it (asked first). */
@Composable
private fun InvitationsList() {
    val model: InvitationsViewModel = viewModel(factory = InvitationsViewModel.factory)
    val load by model.invitations.state.collectAsStateWithLifecycle()
    val libraries by model.libraries.state.collectAsStateWithLifecycle()
    val navigator = LocalPhoneNavigator.current
    val resources = LocalResources.current
    val copy = rememberCopy()
    var deleting by remember { mutableStateOf<Invitation?>(null) }
    val add = { navigator.open(PleyKey.Manage(AdminSection.Invitations, PleyKey.Manage.NEW)) }
    AdminEvents(model) { done -> if (done.what == InvitationsViewModel.DELETED) resources.getString(R.string.admin_invitations_deleted) else null }
    ShownAgain(model::shownAgain)
    val now by model.now.collectAsStateWithLifecycle()
    val words = formatWords()
    AdminPage(
        title = stringResource(R.string.admin_section_invitations),
        load = load,
        onRetry = model.invitations::reload,
        tag = INVITATIONS_PAGE,
        actions = { PleyIconButton(PleyIcons.MailPlus, stringResource(R.string.admin_invitations_new), onClick = add, modifier = Modifier.testTag(INVITATIONS_ADD)) },
    ) { invitations ->
        item(key = "intro") {
            Text(
                stringResource(R.string.admin_invitations_description),
                style = MaterialTheme.typography.bodyMedium,
                color = PleyColors.Muted,
                modifier = Modifier.adminWidth.padding(horizontal = 4.dp),
            )
        }
        if (invitations.isEmpty()) {
            item(key = "empty") {
                EmptyState(
                    title = stringResource(R.string.admin_invitations_empty_title),
                    body = stringResource(R.string.admin_invitations_empty_description),
                    icon = PleyIcons.MailPlus,
                    action = { PrimaryButton(stringResource(R.string.admin_invitations_new), onClick = add, icon = PleyIcons.Plus) },
                )
            }
        } else {
            // One group, as the web's list: a server's links are a handful, drawn at once.
            item(key = "invitations") {
                SettingsGroup(null, Modifier.adminWidth) {
                    for ((i, invitation) in invitations.withIndex()) {
                        if (i > 0) RowDivider(inset = false)
                        InvitationRow(
                            invitation = invitation,
                            state = stateOf(invitation, now),
                            now = now,
                            words = words,
                            libraries = libraries.valueOrNull,
                            onCopy = model.link(invitation)?.let { link -> { copy(link) } },
                            onDelete = { deleting = invitation },
                        )
                    }
                }
            }
        }
    }
    deleting?.let { invitation ->
        ConfirmDialog(
            title = stringResource(R.string.admin_invitations_delete_title),
            body = stringResource(R.string.admin_invitations_delete_message),
            confirmLabel = stringResource(R.string.admin_invitations_delete_action),
            destructive = true,
            onConfirm = {
                deleting = null
                model.delete(invitation)
            },
            onDismiss = { deleting = null },
        )
    }
}

/**
 * One link: its code and tags (state, libraries, age limit), then its
 * note, sign-ups left, when it expires and when it was made. An active
 * one's link copies ([onCopy]; `null` while the server's address isn't known).
 */
@Composable
private fun InvitationRow(
    invitation: Invitation,
    state: InvitationState,
    now: Long,
    words: FormatWords,
    libraries: List<Library>?,
    onCopy: (() -> Unit)?,
    onDelete: () -> Unit,
) {
    Row(
        Modifier.fillMaxWidth().heightIn(min = 64.dp).padding(start = 16.dp, end = 4.dp, top = 12.dp, bottom = 12.dp).testTag(invitationTag(invitation.code)),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(Modifier.weight(1f)) {
            FlowRow(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalArrangement = Arrangement.spacedBy(6.dp), itemVerticalAlignment = Alignment.CenterVertically) {
                Text(invitation.code, style = MaterialTheme.typography.bodyMedium.copy(fontFamily = FontFamily.Monospace), color = PleyColors.Fg)
                when (state) {
                    InvitationState.Active -> Tag(stringResource(R.string.admin_invitations_status_active), TagTone.Success)
                    InvitationState.Used -> Tag(stringResource(R.string.admin_invitations_status_used))
                    InvitationState.Expired -> Tag(stringResource(R.string.admin_invitations_status_expired), TagTone.Danger)
                }
                Tag(librariesLabel(invitation.allLibraries, invitation.libraryIds, libraries))
                ageTag(invitation.maxAge)?.let { Tag(it) }
            }
            val expires = invitation.expiresAt
            val line = listOfNotNull(
                invitation.note?.takeIf { it.isNotBlank() },
                if (state == InvitationState.Active) pluralStringResource(R.plurals.admin_invitations_sign_ups_left, invitation.usesLeft, invitation.usesLeft) else null,
                when {
                    expires == null -> stringResource(R.string.admin_invitations_never_expires)
                    expires > now -> stringResource(R.string.admin_invitations_expires, formatRelative(expires, now, words))
                    else -> stringResource(R.string.admin_invitations_expired, formatRelative(expires, now, words))
                },
                stringResource(R.string.admin_invitations_created, formatRelative(invitation.createdAt, now, words)),
            )
            Text(
                line.joinToString(" · "),
                style = MaterialTheme.typography.bodySmall,
                color = PleyColors.Subtle,
                maxLines = 2,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.padding(top = 6.dp),
            )
        }
        if (state == InvitationState.Active && onCopy != null) {
            PleyIconButton(PleyIcons.Copy, stringResource(R.string.admin_invitations_copy_link), onClick = onCopy, tint = PleyColors.Muted)
        }
        PleyIconButton(PleyIcons.Trash2, stringResource(R.string.admin_invitations_delete), onClick = onDelete, tint = PleyColors.Muted)
    }
}

private enum class NewInvitationDialog { Expiry, Uses, Note }

/**
 * Making a link (the web's New invitation dialog): how long it lasts, how
 * many accounts it makes, a note, and what they see, sent together by
 * Create link; then its link to copy or share.
 */
@Composable
private fun NewInvitationPage() {
    val model: NewInvitationViewModel = viewModel(factory = NewInvitationViewModel.factory)
    val load by model.libraries.state.collectAsStateWithLifecycle()
    val form by model.form.collectAsStateWithLifecycle()
    val busy by model.busy.collectAsStateWithLifecycle()
    val created by model.created.collectAsStateWithLifecycle()
    val navigator = LocalPhoneNavigator.current
    var dialog by remember { mutableStateOf<NewInvitationDialog?>(null) }
    var problem by remember { mutableStateOf<NewInvitationProblem?>(null) }
    AdminEvents(model)

    AdminPage(title = stringResource(R.string.admin_invitations_new), load = load, onRetry = model.libraries::reload, tag = NEW_INVITATION_PAGE) { libraries ->
        item(key = "intro") {
            Text(
                stringResource(R.string.admin_invitations_intro),
                style = MaterialTheme.typography.bodyMedium,
                color = PleyColors.Muted,
                modifier = Modifier.adminWidth.padding(horizontal = 4.dp),
            )
        }
        item(key = "link") {
            SettingsGroup(null, Modifier.adminWidth) {
                SettingsRow(
                    stringResource(R.string.admin_invitations_expires_label),
                    value = expiryLabel(form.days),
                    icon = PleyIcons.Clock,
                    onClick = { dialog = NewInvitationDialog.Expiry },
                )
                RowDivider()
                SettingsRow(
                    stringResource(R.string.admin_invitations_uses),
                    value = form.uses.toString(),
                    icon = PleyIcons.Users,
                    onClick = { dialog = NewInvitationDialog.Uses },
                    modifier = Modifier.testTag(NEW_INVITATION_USES),
                )
            }
        }
        item(key = "note") {
            SettingsGroup(null, Modifier.adminWidth, footer = stringResource(R.string.admin_invitations_note_hint)) {
                SettingsRow(
                    stringResource(R.string.admin_invitations_note),
                    value = form.note.ifEmpty { null },
                    icon = PleyIcons.Pencil,
                    onClick = { dialog = NewInvitationDialog.Note },
                )
            }
        }
        item(key = "access") {
            AccessGroup(
                access = form.access,
                libraries = libraries,
                onLibraries = { all, ids -> model.edit { it.copy(access = it.access.copy(allLibraries = all, libraryIds = ids)) } },
                onAgeLimit = { age -> model.edit { it.copy(access = it.access.copy(maxAge = age)) } },
                pickLibrary = stringResource(R.string.admin_invitations_pick_library),
                modifier = Modifier.adminWidth,
            )
        }
        item(key = "create") {
            Column(Modifier.adminWidth, verticalArrangement = Arrangement.spacedBy(12.dp)) {
                problem?.let {
                    val words = when (it) {
                        NewInvitationProblem.PickLibrary -> R.string.admin_invitations_pick_library
                        NewInvitationProblem.AtLeastOne -> R.string.admin_invitations_at_least_one
                    }
                    Text(stringResource(words), style = MaterialTheme.typography.bodyMedium, color = PleyColors.DangerText)
                }
                PrimaryButton(
                    stringResource(R.string.admin_invitations_create_link),
                    onClick = { problem = model.create() },
                    enabled = busy == 0 && created == null,
                    icon = PleyIcons.Link2,
                    modifier = Modifier.fillMaxWidth().testTag(NEW_INVITATION_CREATE),
                )
            }
        }
    }

    val close = { dialog = null }
    when (dialog) {
        NewInvitationDialog.Expiry -> ChoiceSheet(
            title = stringResource(R.string.admin_invitations_expires_label),
            choices = EXPIRY_DAYS.map { Choice(it, expiryLabel(it)) },
            selected = form.days,
            onPick = { days -> model.edit { it.copy(days = days) } },
            onDismiss = close,
        )
        NewInvitationDialog.Uses -> TextEntryDialog(
            title = stringResource(R.string.admin_invitations_uses),
            initial = form.uses.toString(),
            entry = Entry.Number,
            check = numberProblem(USES),
            onSave = { typed ->
                close()
                numberIn(typed, USES)?.let { uses -> model.edit { it.copy(uses = uses.toInt()) } }
            },
            onDismiss = close,
        )
        NewInvitationDialog.Note -> TextEntryDialog(
            title = stringResource(R.string.admin_invitations_note),
            initial = form.note,
            hint = stringResource(R.string.admin_invitations_note_hint),
            onSave = { typed ->
                close()
                // As the web's field, which takes no more than the server keeps.
                model.edit { it.copy(note = typed.take(NOTE_MAX)) }
            },
            onDismiss = close,
        )
        null -> Unit
    }

    val made = created
    val link = model.link()
    if (made != null && link != null) {
        ReadyDialog(made, link, onDone = navigator::back)
    }
}

/**
 * The link made, to send to whoever is invited: copy it, or share it
 * through an app; Done goes back to the list, which reads it.
 */
@Composable
private fun ReadyDialog(invitation: Invitation, link: String, onDone: () -> Unit) {
    val context = LocalContext.current
    val copy = rememberCopy()
    val share = stringResource(R.string.admin_invitations_share)
    val words = formatWords()
    val now = remember { System.currentTimeMillis() / 1000 }
    AlertDialog(
        onDismissRequest = onDone,
        title = { Text(stringResource(R.string.admin_invitations_ready_title), style = MaterialTheme.typography.headlineSmall) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                Text(stringResource(R.string.admin_invitations_ready_send), style = MaterialTheme.typography.bodyMedium)
                Row(
                    Modifier.fillMaxWidth().background(PleyColors.Surface3, PleyShape.Field).padding(start = 12.dp, top = 4.dp, bottom = 4.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Icon(PleyIcons.Link2, contentDescription = null, size = 18.dp, tint = PleyColors.AccentBright)
                    Spacer(Modifier.width(8.dp))
                    Text(link, style = MaterialTheme.typography.bodyMedium, color = PleyColors.Fg, modifier = Modifier.weight(1f).testTag(INVITATION_LINK))
                    PleyIconButton(PleyIcons.Copy, stringResource(R.string.admin_invitations_copy_link), onClick = { copy(link) }, tint = PleyColors.Muted)
                }
                invitation.expiresAt?.let { expires ->
                    Text(
                        stringResource(R.string.admin_invitations_ready_expires, formatRelative(expires, now, words)),
                        style = MaterialTheme.typography.bodySmall,
                        color = PleyColors.Subtle,
                    )
                }
            }
        },
        confirmButton = { PleyTextButton(stringResource(R.string.admin_invitations_done), onClick = onDone) },
        dismissButton = { PleyTextButton(share, onClick = { shareLink(context, link, share) }) },
        shape = PleyShape.Dialog,
        containerColor = PleyColors.Surface2,
        titleContentColor = PleyColors.Fg,
        textContentColor = PleyColors.Muted,
        tonalElevation = 0.dp,
    )
}

/** Copies a link, and says so. */
@Composable
private fun rememberCopy(): (String) -> Unit {
    val clipboard = LocalClipboard.current
    val shell = LocalPhoneShell.current
    val resources = LocalResources.current
    val scope = rememberCoroutineScope()
    return { link ->
        scope.launch {
            clipboard.setClipEntry(ClipEntry(ClipData.newPlainText(resources.getString(R.string.admin_invitations_ready_link), link)))
            shell.toasts.show(resources.getString(R.string.admin_invitations_copied))
        }
    }
}

/** Offers [link] to the apps that send text (messages, mail), in the system's chooser. */
private fun shareLink(context: Context, link: String, title: String) {
    val send = Intent(Intent.ACTION_SEND).setType("text/plain").putExtra(Intent.EXTRA_TEXT, link)
    context.startActivity(Intent.createChooser(send, title))
}

/** "7 days", "Never". */
@Composable
private fun expiryLabel(days: Int): String =
    if (days == 0) stringResource(R.string.admin_invitations_never) else pluralStringResource(R.plurals.admin_invitations_days, days, days)

/** How many accounts a link makes, as the server takes it. */
private val USES = 1L..MAX_USES.toLong()

/** The note's length the server keeps. */
private const val NOTE_MAX = 200

internal const val INVITATIONS_PAGE = "pley.admin.invitations"
internal const val INVITATIONS_ADD = "pley.admin.invitations.add"
internal const val NEW_INVITATION_PAGE = "pley.admin.invitations.new"
internal const val NEW_INVITATION_USES = "pley.admin.invitations.new.uses"
internal const val NEW_INVITATION_CREATE = "pley.admin.invitations.new.create"
internal const val INVITATION_LINK = "pley.admin.invitations.link"

internal fun invitationTag(code: String): String = "pley.admin.invitations.invitation.$code"
