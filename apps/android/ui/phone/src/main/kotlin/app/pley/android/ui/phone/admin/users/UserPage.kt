package app.pley.android.ui.phone.admin.users

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import app.pley.android.common.errorSentence
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.icon.Icon
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.designsystem.text.asString
import app.pley.android.designsystem.text.formatWords
import app.pley.android.model.RequestAccess
import app.pley.android.model.admin.UserAccount
import app.pley.android.model.admin.UserApp
import app.pley.android.model.admin.UserDevice
import app.pley.android.model.format.FormatWords
import app.pley.android.model.format.formatRelative
import app.pley.android.screens.admin.AdminLoad
import app.pley.android.screens.admin.valueOrNull
import app.pley.android.screens.admin.users.AccessChoice
import app.pley.android.screens.admin.users.UserPage
import app.pley.android.screens.admin.users.UserViewModel
import app.pley.android.screens.admin.users.asAdmin
import app.pley.android.screens.admin.users.passwordProblem
import app.pley.android.screens.admin.users.pinOk
import app.pley.android.screens.admin.users.requestsShown
import app.pley.android.ui.phone.R
import app.pley.android.ui.phone.account.Choice
import app.pley.android.ui.phone.account.ChoiceSheet
import app.pley.android.ui.phone.account.RowDivider
import app.pley.android.ui.phone.account.SettingsGroup
import app.pley.android.ui.phone.account.SettingsRow
import app.pley.android.ui.phone.account.SwitchRow
import app.pley.android.ui.phone.admin.AdminEvents
import app.pley.android.ui.phone.admin.AdminPage
import app.pley.android.ui.phone.admin.Entry
import app.pley.android.ui.phone.admin.TextEntryDialog
import app.pley.android.ui.phone.admin.adminWidth
import app.pley.android.ui.phone.kit.Avatar
import app.pley.android.ui.phone.kit.ConfirmDialog
import app.pley.android.ui.phone.kit.PleyTextButton
import app.pley.android.ui.phone.shell.LocalPhoneNavigator
import app.pley.android.ui.phone.shell.LocalPhoneShell

/** What a user's page has open over it. */
private sealed interface UserDialog {
    data object Rename : UserDialog
    data object Password : UserDialog
    data object Pin : UserDialog
    data object Requests : UserDialog
    data object TwoStep : UserDialog
    data object SignOutEverywhere : UserDialog
    data class SignOutApp(val app: UserApp) : UserDialog
    data object Delete : UserDialog
}

/**
 * One account (the web's user dialog), each change sent as it is made.
 * One's own account can't change its password, PIN or two-step sign-in
 * here (Account does, asking for the password), nor be disabled, signed
 * out everywhere or deleted: those aren't offered, as on the web.
 */
@Composable
internal fun UserAccountPage(id: Long) {
    val model: UserViewModel = viewModel(key = "user-$id", factory = UserViewModel.factory(id))
    val load by model.page.state.collectAsStateWithLifecycle()
    val libraries by model.libraries.state.collectAsStateWithLifecycle()
    val server by model.server.state.collectAsStateWithLifecycle()
    val devices by model.devices.state.collectAsStateWithLifecycle()
    val apps by model.apps.state.collectAsStateWithLifecycle()
    val busy by model.busy.collectAsStateWithLifecycle()
    val deleted by model.deleted.collectAsStateWithLifecycle()
    val navigator = LocalPhoneNavigator.current
    val shell = LocalPhoneShell.current
    val resources = LocalResources.current
    var dialog by remember { mutableStateOf<UserDialog?>(null) }

    LaunchedEffect(deleted) {
        val name = deleted ?: return@LaunchedEffect
        shell.toasts.show(resources.getString(R.string.admin_users_deleted, name))
        navigator.back()
    }
    AdminEvents(model) { done ->
        val name = done.args.firstOrNull()?.toString().orEmpty()
        when (done.what) {
            UserViewModel.SAVED -> resources.getString(R.string.admin_users_saved, name)
            UserViewModel.TWO_STEP_OFF -> resources.getString(R.string.admin_users_two_step_off, name)
            UserViewModel.TWO_STEP_SIGNED_OUT -> resources.getString(R.string.admin_users_two_step_signed_out, name)
            UserViewModel.SIGNED_OUT_EVERYWHERE -> resources.getString(R.string.admin_users_devices_signed_out, name)
            UserViewModel.APP_SIGNED_OUT -> resources.getString(R.string.admin_users_apps_signed_out, name)
            else -> null
        }
    }

    val self by model.self.collectAsStateWithLifecycle()
    val twoStepRequired = server.valueOrNull?.twoStepRequired == true
    AdminPage(
        title = load.valueOrNull?.user?.name ?: stringResource(R.string.admin_section_users),
        load = load,
        onRetry = model.page::reload,
        tag = USER_PAGE,
    ) { page ->
        val user = page.user
        val idle = busy == 0
        item(key = "header") { Header(user, model.avatar(user)) }
        item(key = "profile") { ProfileGroup(user, self, idle, onRename = { dialog = UserDialog.Rename }, onPassword = { dialog = UserDialog.Password }) }
        item(key = "roles") { RolesGroup(page, self, idle, model) }
        val requests = requestsShown(user)
        if (requests != null) {
            item(key = "requests") {
                SettingsGroup(null, Modifier.adminWidth, footer = requestsHint(user)) {
                    SettingsRow(
                        stringResource(R.string.admin_users_requests),
                        value = requestLabel(requests),
                        icon = PleyIcons.Compass,
                        enabled = !user.asAdmin && idle,
                        onClick = { dialog = UserDialog.Requests },
                    )
                }
            }
        }
        item(key = "access") {
            AccessGroup(
                access = AccessChoice(user.allLibraries, user.libraryIds, user.maxAge),
                libraries = libraries.valueOrNull,
                onLibraries = model::setLibraries,
                onAgeLimit = model::setAgeLimit,
                pickLibrary = stringResource(R.string.admin_users_pick_library),
                modifier = Modifier.adminWidth,
                enabled = idle,
            )
        }
        if (!self) {
            item(key = "pin") {
                SettingsGroup(
                    null,
                    Modifier.adminWidth,
                    footer = stringResource(if (user.hasPin) R.string.admin_users_pin_set else R.string.admin_users_pin_hint),
                ) {
                    SettingsRow(stringResource(R.string.admin_users_pin), icon = PleyIcons.Lock, enabled = idle, onClick = { dialog = UserDialog.Pin })
                    if (user.hasPin) {
                        RowDivider()
                        SettingsRow(stringResource(R.string.admin_users_remove_pin), icon = PleyIcons.LockOpen, enabled = idle, onClick = model::removePin)
                    }
                }
            }
        }
        if (user.twoStep && !self) {
            item(key = "two-step") {
                SettingsGroup(
                    stringResource(R.string.admin_users_two_step_title),
                    Modifier.adminWidth,
                    footer = if (twoStepRequired) stringResource(R.string.admin_users_two_step_hint_required, user.name) else stringResource(R.string.admin_users_two_step_hint),
                ) {
                    SettingsRow(
                        stringResource(R.string.admin_users_two_step_turn_off),
                        value = stringResource(R.string.admin_users_two_step_turn_off_title),
                        icon = PleyIcons.ShieldCheck,
                        danger = true,
                        enabled = idle,
                        onClick = { dialog = UserDialog.TwoStep },
                    )
                }
            }
        }
        if (!self) {
            item(key = "devices") {
                val signedIn = devices.valueOrNull.orEmpty().isNotEmpty() || apps.valueOrNull.orEmpty().isNotEmpty()
                DevicesGroup(devices, showSignOut = signedIn, onSignOut = { dialog = UserDialog.SignOutEverywhere })
            }
            item(key = "apps") { AppsGroup(apps, user, onSignOut = { dialog = UserDialog.SignOutApp(it) }) }
        }
        if (!self) {
            item(key = "delete") {
                SettingsGroup(null, Modifier.adminWidth) {
                    SettingsRow(
                        stringResource(R.string.admin_users_delete),
                        icon = PleyIcons.Trash2,
                        danger = true,
                        enabled = idle,
                        onClick = { dialog = UserDialog.Delete },
                        modifier = Modifier.testTag(USER_DELETE),
                    )
                }
            }
        }
    }

    val user = load.valueOrNull?.user ?: return
    val close = { dialog = null }
    when (val open = dialog) {
        UserDialog.Rename -> TextEntryDialog(
            title = stringResource(R.string.admin_users_display_name),
            initial = user.displayName.orEmpty(),
            hint = stringResource(R.string.admin_users_display_name_hint),
            onSave = {
                close()
                if (it != user.displayName.orEmpty()) model.rename(it)
            },
            onDismiss = close,
        )
        UserDialog.Password -> {
            val words = passwordWords()
            TextEntryDialog(
                title = stringResource(if (user.managed) R.string.admin_users_password else R.string.admin_users_new_password),
                initial = "",
                entry = Entry.Secret,
                hint = if (user.managed) stringResource(R.string.admin_users_managed_password_hint) else null,
                check = { typed -> passwordProblem(typed, user.username)?.let(words) },
                onSave = {
                    close()
                    model.setPassword(it)
                },
                onDismiss = close,
            )
        }
        UserDialog.Pin -> {
            val digits = stringResource(R.string.admin_users_pin_digits)
            TextEntryDialog(
                title = stringResource(R.string.admin_users_pin),
                initial = "",
                entry = Entry.Number,
                hint = stringResource(if (user.hasPin) R.string.admin_users_pin_set else R.string.admin_users_pin_hint),
                check = { typed -> if (pinOk(typed)) null else digits },
                onSave = {
                    close()
                    model.setPin(it)
                },
                onDismiss = close,
            )
        }
        UserDialog.Requests -> ChoiceSheet(
            title = stringResource(R.string.admin_users_requests),
            choices = REQUEST_CHOICES.map { Choice(it, requestLabel(it)) },
            selected = user.requestAccess?.takeIf { it != RequestAccess.Unknown } ?: RequestAccess.Ask,
            onPick = { if (it != user.requestAccess) model.setRequests(it) },
            onDismiss = close,
        )
        UserDialog.TwoStep -> ConfirmDialog(
            title = stringResource(R.string.admin_users_two_step_title),
            body = if (twoStepRequired) stringResource(R.string.admin_users_two_step_hint_required, user.name) else stringResource(R.string.admin_users_two_step_turn_off_title),
            confirmLabel = stringResource(R.string.admin_users_two_step_turn_off),
            dismissLabel = stringResource(R.string.admin_users_two_step_keep),
            destructive = true,
            onConfirm = {
                close()
                model.turnOffTwoStep()
            },
            onDismiss = close,
        )
        UserDialog.SignOutEverywhere -> ConfirmDialog(
            title = stringResource(R.string.admin_users_devices_confirm, user.name),
            body = null,
            confirmLabel = stringResource(R.string.admin_users_devices_sign_out_everywhere),
            destructive = true,
            onConfirm = {
                close()
                model.signOutEverywhere()
            },
            onDismiss = close,
        )
        is UserDialog.SignOutApp -> ConfirmDialog(
            title = stringResource(R.string.admin_users_apps_sign_out, open.app.label),
            body = null,
            confirmLabel = stringResource(R.string.admin_users_sign_out),
            destructive = true,
            onConfirm = {
                close()
                model.signOutApp(open.app)
            },
            onDismiss = close,
        )
        UserDialog.Delete -> ConfirmDialog(
            title = stringResource(R.string.admin_users_delete_title),
            body = stringResource(R.string.admin_users_delete_message, user.name),
            confirmLabel = stringResource(R.string.admin_users_delete),
            destructive = true,
            onConfirm = {
                close()
                model.delete()
            },
            onDismiss = close,
        )
        null -> Unit
    }
}

/** The order the web's Requests choice offers. */
private val REQUEST_CHOICES = listOf(RequestAccess.Ask, RequestAccess.Auto, RequestAccess.Off)

/** The picture, the name, and "@username · role", as the web's dialog opens. */
@Composable
private fun Header(user: UserAccount, picture: String?) {
    Row(Modifier.adminWidth.padding(horizontal = 4.dp), verticalAlignment = Alignment.CenterVertically) {
        Avatar(user.name, color = user.avatarColor, imageUrl = picture, size = 56.dp)
        Spacer(Modifier.width(16.dp))
        Column(Modifier.weight(1f)) {
            Text(user.name, style = MaterialTheme.typography.titleLarge, color = PleyColors.Fg, maxLines = 1, overflow = TextOverflow.Ellipsis)
            val role = stringResource(
                when {
                    user.managed -> R.string.admin_users_managed
                    user.isAdmin -> R.string.admin_users_administrator
                    else -> R.string.admin_users_regular
                },
            )
            Text("@${user.username} · $role", style = MaterialTheme.typography.bodyMedium, color = PleyColors.Muted, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
    }
}

/** The display name, and a new password (not one's own: Account changes it). */
@Composable
private fun ProfileGroup(user: UserAccount, self: Boolean, idle: Boolean, onRename: () -> Unit, onPassword: () -> Unit) {
    SettingsGroup(null, Modifier.adminWidth, footer = if (self) stringResource(R.string.admin_users_own_password) else null) {
        SettingsRow(
            stringResource(R.string.admin_users_display_name),
            value = user.displayName?.takeIf { it.isNotBlank() },
            icon = PleyIcons.Pencil,
            enabled = !self && idle,
            onClick = onRename,
        )
        if (!self) {
            RowDivider()
            SettingsRow(
                stringResource(if (user.managed) R.string.admin_users_password else R.string.admin_users_new_password),
                value = when {
                    user.managed -> stringResource(R.string.admin_users_managed_password_hint)
                    !user.hasPassword -> stringResource(R.string.admin_users_no_password_hint)
                    else -> null
                },
                icon = PleyIcons.Lock,
                enabled = idle,
                onClick = onPassword,
                modifier = Modifier.testTag(USER_PASSWORD),
            )
        }
    }
}

/**
 * Administrator, home member, disabled. Off limits where the server
 * would refuse: a managed profile as administrator, one's own role or
 * account, the last administrator's.
 */
@Composable
private fun RolesGroup(page: UserPage, self: Boolean, idle: Boolean, model: UserViewModel) {
    val user = page.user
    SettingsGroup(null, Modifier.adminWidth) {
        SwitchRow(
            stringResource(R.string.admin_users_administrator),
            checked = user.asAdmin,
            onChange = model::setAdmin,
            subtitle = stringResource(if (user.managed) R.string.admin_users_managed_not_admin else R.string.admin_users_admin_hint),
            enabled = idle && !user.managed && !(self && user.isAdmin) && !(page.lastAdmin && user.isAdmin),
            modifier = Modifier.testTag(USER_ADMIN),
        )
        RowDivider(inset = false)
        SwitchRow(
            stringResource(R.string.admin_users_home),
            checked = user.home,
            onChange = model::setHome,
            subtitle = stringResource(R.string.admin_users_home_hint),
            enabled = idle,
        )
        RowDivider(inset = false)
        SwitchRow(
            stringResource(R.string.admin_users_disabled),
            checked = user.disabled,
            onChange = model::setDisabled,
            subtitle = stringResource(R.string.admin_users_disabled_hint),
            enabled = idle && !self && !(page.lastAdmin && !user.disabled),
            modifier = Modifier.testTag(USER_DISABLED),
        )
    }
}

/** Where the account is signed in, and signing it out everywhere (not one's own). */
@Composable
private fun DevicesGroup(devices: AdminLoad<List<UserDevice>>, showSignOut: Boolean, onSignOut: () -> Unit) {
    val words = formatWords()
    val now = remember { System.currentTimeMillis() / 1000 }
    SettingsGroup(stringResource(R.string.admin_users_devices_title), Modifier.adminWidth) {
        val list = devices.valueOrNull
        when {
            list == null && devices is AdminLoad.Failed -> GroupLine(errorSentence(devices.error).asString(), PleyColors.DangerText)
            list == null -> GroupLine(stringResource(R.string.admin_loading))
            list.isEmpty() -> GroupLine(stringResource(R.string.admin_users_devices_none))
            else -> {
                for ((i, device) in list.withIndex()) {
                    if (i > 0) RowDivider()
                    DeviceRow(device, now, words)
                }
            }
        }
        if (showSignOut) {
            RowDivider(inset = false)
            SettingsRow(
                stringResource(R.string.admin_users_devices_sign_out_everywhere),
                icon = PleyIcons.LogOut,
                danger = true,
                onClick = onSignOut,
                modifier = Modifier.testTag(USER_SIGN_OUT_EVERYWHERE),
            )
        }
    }
}

@Composable
private fun DeviceRow(device: UserDevice, now: Long, words: FormatWords) {
    val phone = PHONE.containsMatchIn(device.name)
    Row(
        Modifier.fillMaxWidth().heightIn(min = 56.dp).padding(horizontal = 16.dp, vertical = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(if (phone) PleyIcons.Smartphone else PleyIcons.Laptop, contentDescription = null, size = 24.dp, tint = PleyColors.Muted)
        Spacer(Modifier.width(16.dp))
        Column(Modifier.weight(1f)) {
            Text(device.name, style = MaterialTheme.typography.bodyLarge, color = PleyColors.Fg, maxLines = 1, overflow = TextOverflow.Ellipsis)
            Text(
                listOfNotNull(device.ip, formatRelative(device.lastSeenAt, now, words)).joinToString(" · "),
                style = MaterialTheme.typography.bodyMedium,
                color = PleyColors.Muted,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
    }
}

/** A phone or tablet, by its name, as the web picks the icon. */
private val PHONE = Regex("iPhone|iPad|Android")

/** The apps using the account or allowed by it, each signed out on its own. */
@Composable
private fun AppsGroup(apps: AdminLoad<List<UserApp>>, user: UserAccount, onSignOut: (UserApp) -> Unit) {
    val words = formatWords()
    val now = remember { System.currentTimeMillis() / 1000 }
    SettingsGroup(stringResource(R.string.admin_users_apps_title), Modifier.adminWidth) {
        val list = apps.valueOrNull
        when {
            list == null && apps is AdminLoad.Failed -> GroupLine(errorSentence(apps.error).asString(), PleyColors.DangerText)
            list == null -> GroupLine(stringResource(R.string.admin_loading))
            list.isEmpty() -> GroupLine(stringResource(R.string.admin_users_apps_none))
            else -> {
                for ((i, app) in list.withIndex()) {
                    if (i > 0) RowDivider()
                    AppRow(app, user, now, words, onSignOut)
                }
            }
        }
    }
}

@Composable
private fun AppRow(app: UserApp, user: UserAccount, now: Long, words: FormatWords, onSignOut: (UserApp) -> Unit) {
    Row(
        Modifier.fillMaxWidth().heightIn(min = 56.dp).padding(start = 16.dp, end = 4.dp, top = 10.dp, bottom = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(PleyIcons.AppWindow, contentDescription = null, size = 24.dp, tint = PleyColors.Muted)
        Spacer(Modifier.width(16.dp))
        Column(Modifier.weight(1f)) {
            FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(4.dp), itemVerticalAlignment = Alignment.CenterVertically) {
                Text(app.label, style = MaterialTheme.typography.bodyLarge, color = PleyColors.Fg, maxLines = 1, overflow = TextOverflow.Ellipsis)
                when {
                    app.firstParty -> Tag(stringResource(R.string.admin_users_apps_pley_app), TagTone.Accent)
                    !app.name.isNullOrEmpty() -> Text(
                        stringResource(R.string.admin_users_apps_calls_itself, app.name.orEmpty()),
                        style = MaterialTheme.typography.bodySmall,
                        color = PleyColors.Subtle,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                }
            }
            val who = listOfNotNull(
                if (app.profile.id != user.id) stringResource(R.string.admin_users_apps_using, app.profile.name) else null,
                if (app.approvedBy.id != user.id) stringResource(R.string.admin_users_apps_allowed_by, app.approvedBy.name) else null,
                scopesLabel(app.scopes).ifEmpty { null },
            )
            if (who.isNotEmpty()) {
                Text(who.joinToString(" · "), style = MaterialTheme.typography.bodyMedium, color = PleyColors.Muted, maxLines = 2, overflow = TextOverflow.Ellipsis)
            }
            Text(
                listOfNotNull(app.device, app.ip, stringResource(R.string.admin_users_apps_last_active, formatRelative(app.lastUsedAt, now, words))).joinToString(" · "),
                style = MaterialTheme.typography.bodySmall,
                color = PleyColors.Subtle,
                maxLines = 2,
                overflow = TextOverflow.Ellipsis,
            )
        }
        PleyTextButton(stringResource(R.string.admin_users_sign_out), onClick = { onSignOut(app) })
    }
}

/** What an app may do, in a few words: "Libraries · Play · Playlists"; scopes Pley doesn't name stay as they are. */
@Composable
private fun scopesLabel(scopes: List<String>): String = scopes.map { scope ->
    when (scope) {
        "library" -> stringResource(R.string.admin_users_scope_library)
        "play" -> stringResource(R.string.admin_users_scope_play)
        "playlists" -> stringResource(R.string.admin_users_scope_playlists)
        "profiles" -> stringResource(R.string.admin_users_scope_profiles)
        "requests" -> stringResource(R.string.admin_users_scope_requests)
        "approvals" -> stringResource(R.string.admin_users_scope_approvals)
        "activity" -> stringResource(R.string.admin_users_scope_activity)
        "server" -> stringResource(R.string.admin_users_scope_server)
        else -> scope
    }
}.joinToString(" · ")

@Composable
internal fun requestLabel(access: RequestAccess): String = stringResource(
    when (access) {
        RequestAccess.Auto -> R.string.admin_users_request_auto
        RequestAccess.Off -> R.string.admin_users_request_off
        RequestAccess.Ask, RequestAccess.Unknown -> R.string.admin_users_request_ask
    },
)

/** The line under Requests: approved as made for an administrator; Discover's age limit; else what asking is. */
@Composable
private fun requestsHint(user: UserAccount): String = stringResource(
    when {
        user.asAdmin -> R.string.admin_users_requests_admin
        user.maxAge != null -> R.string.admin_users_requests_age_limit
        else -> R.string.admin_users_requests_hint
    },
)

internal const val USER_PAGE = "pley.admin.users.user"
internal const val USER_PASSWORD = "pley.admin.users.user.password"
internal const val USER_ADMIN = "pley.admin.users.user.admin"
internal const val USER_DISABLED = "pley.admin.users.user.disabled"
internal const val USER_SIGN_OUT_EVERYWHERE = "pley.admin.users.user.signout"
internal const val USER_DELETE = "pley.admin.users.user.delete"
