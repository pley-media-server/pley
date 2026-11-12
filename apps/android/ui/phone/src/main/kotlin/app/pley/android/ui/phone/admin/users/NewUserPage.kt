package app.pley.android.ui.phone.admin.users

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.model.RequestAccess
import app.pley.android.screens.admin.users.AccountKind
import app.pley.android.screens.admin.users.NewUserProblem
import app.pley.android.screens.admin.users.NewUserViewModel
import app.pley.android.screens.admin.users.passwordProblem
import app.pley.android.screens.admin.users.pinOk
import app.pley.android.screens.admin.valueOrNull
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
import app.pley.android.ui.phone.kit.PrimaryButton
import app.pley.android.ui.phone.kit.SegmentedControl
import app.pley.android.ui.phone.shell.LocalPhoneNavigator
import app.pley.android.ui.phone.shell.LocalPhoneShell

private enum class NewUserDialog { Username, DisplayName, Password, Pin, Requests }

/**
 * Adding a user (the web's Add user dialog): a regular account or a
 * managed profile, its names and password, role, requests, what it sees
 * and a PIN, sent together by Add user. What the web checks first is said
 * under the button; the server's refusals are toasts.
 */
@Composable
internal fun NewUserPage() {
    val model: NewUserViewModel = viewModel(factory = NewUserViewModel.factory)
    val load by model.libraries.state.collectAsStateWithLifecycle()
    val ratingSystems by model.ratingSystems.state.collectAsStateWithLifecycle()
    val labels by model.labels.state.collectAsStateWithLifecycle()
    val metadataKeys by model.metadataKeys.state.collectAsStateWithLifecycle()
    val form by model.form.collectAsStateWithLifecycle()
    val busy by model.busy.collectAsStateWithLifecycle()
    val created by model.created.collectAsStateWithLifecycle()
    val navigator = LocalPhoneNavigator.current
    val shell = LocalPhoneShell.current
    val resources = LocalResources.current
    var dialog by remember { mutableStateOf<NewUserDialog?>(null) }
    var problem by remember { mutableStateOf<NewUserProblem?>(null) }

    LaunchedEffect(created) {
        val user = created ?: return@LaunchedEffect
        shell.toasts.show(resources.getString(R.string.admin_users_added, user.name))
        navigator.back()
    }
    AdminEvents(model)

    val managed = form.managed
    AdminPage(title = stringResource(R.string.admin_users_add), load = load, onRetry = model.libraries::reload, tag = NEW_USER_PAGE) { libraries ->
        item(key = "kind") {
            Column(Modifier.adminWidth, verticalArrangement = Arrangement.spacedBy(8.dp)) {
                SegmentedControl(
                    options = listOf(stringResource(R.string.admin_users_regular), stringResource(R.string.admin_users_managed)),
                    selected = if (managed) 1 else 0,
                    onSelect = { i -> model.edit { it.copy(kind = if (i == 1) AccountKind.Managed else AccountKind.Regular) } },
                    modifier = Modifier.fillMaxWidth(),
                )
                Text(
                    stringResource(if (managed) R.string.admin_users_managed_text else R.string.admin_users_regular_text),
                    style = MaterialTheme.typography.bodyMedium,
                    color = PleyColors.Muted,
                    modifier = Modifier.padding(horizontal = 4.dp),
                )
            }
        }
        item(key = "names") {
            SettingsGroup(null, Modifier.adminWidth) {
                SettingsRow(
                    stringResource(R.string.admin_users_username),
                    value = form.username.ifEmpty { null },
                    icon = PleyIcons.UserRound,
                    onClick = { dialog = NewUserDialog.Username },
                    modifier = Modifier.testTag(NEW_USER_USERNAME),
                )
                RowDivider()
                SettingsRow(
                    stringResource(R.string.admin_users_display_name),
                    value = form.displayName.ifEmpty { null },
                    icon = PleyIcons.Pencil,
                    onClick = { dialog = NewUserDialog.DisplayName },
                )
                if (!managed) {
                    RowDivider()
                    SettingsRow(
                        stringResource(R.string.admin_users_password),
                        value = if (form.password.isEmpty()) null else HIDDEN,
                        icon = PleyIcons.Lock,
                        onClick = { dialog = NewUserDialog.Password },
                        modifier = Modifier.testTag(NEW_USER_PASSWORD),
                    )
                }
            }
        }
        item(key = "roles") {
            // An administrator is made in the web app, which the server leaves it to.
            SettingsGroup(null, Modifier.adminWidth, footer = stringResource(R.string.admin_users_admin_in_web)) {
                SwitchRow(
                    stringResource(R.string.admin_users_home),
                    checked = form.home,
                    onChange = { on -> model.edit { it.copy(home = on) } },
                    subtitle = stringResource(R.string.admin_users_home_hint),
                )
            }
        }
        item(key = "requests") {
            val hint = when {
                form.access.maxAge != null -> R.string.admin_users_requests_age_limit
                else -> R.string.admin_users_requests_hint
            }
            SettingsGroup(null, Modifier.adminWidth, footer = stringResource(hint)) {
                SettingsRow(
                    stringResource(R.string.admin_users_requests),
                    value = requestLabel(form.requests),
                    icon = PleyIcons.Compass,
                    onClick = { dialog = NewUserDialog.Requests },
                )
            }
        }
        item(key = "access") {
            AccessGroup(
                access = form.access,
                libraries = libraries,
                sources = AccessSources(ratingSystems.valueOrNull, labels.valueOrNull, metadataKeys.valueOrNull?.tmdbKey),
                onLibraries = { all, ids -> model.edit { it.copy(access = it.access.copy(allLibraries = all, libraryIds = ids)) } },
                onAgeLimit = { access -> model.edit { it.copy(access = access) } },
                onLabels = { show, hide -> model.edit { it.copy(access = it.access.copy(showLabels = show, hideLabels = hide)) } },
                pickLibrary = stringResource(R.string.admin_users_pick_library),
                modifier = Modifier.adminWidth,
                birthday = true,
            )
        }
        item(key = "pin") {
            SettingsGroup(null, Modifier.adminWidth, footer = stringResource(R.string.admin_users_pin_hint)) {
                SettingsRow(
                    stringResource(R.string.admin_users_pin),
                    value = if (form.pin.isEmpty()) null else HIDDEN_PIN,
                    icon = PleyIcons.Lock,
                    onClick = { dialog = NewUserDialog.Pin },
                )
            }
        }
        item(key = "create") {
            Column(Modifier.adminWidth, verticalArrangement = Arrangement.spacedBy(12.dp)) {
                problem?.let { Text(problemWords(it), style = MaterialTheme.typography.bodyMedium, color = PleyColors.DangerText) }
                PrimaryButton(
                    stringResource(R.string.admin_users_add),
                    onClick = { problem = model.create() },
                    enabled = busy == 0 && created == null,
                    icon = PleyIcons.UserPlus,
                    modifier = Modifier.fillMaxWidth().testTag(NEW_USER_CREATE),
                )
            }
        }
    }

    val close = { dialog = null }
    when (dialog) {
        NewUserDialog.Username -> TextEntryDialog(
            title = stringResource(R.string.admin_users_username),
            initial = form.username,
            hint = if (managed) stringResource(R.string.admin_users_username_managed_hint) else null,
            onSave = { typed ->
                close()
                model.edit { it.copy(username = typed) }
            },
            onDismiss = close,
        )
        NewUserDialog.DisplayName -> TextEntryDialog(
            title = stringResource(R.string.admin_users_display_name),
            initial = form.displayName,
            hint = stringResource(R.string.admin_users_display_name_hint),
            onSave = { typed ->
                close()
                model.edit { it.copy(displayName = typed) }
            },
            onDismiss = close,
        )
        NewUserDialog.Password -> {
            val words = passwordWords()
            TextEntryDialog(
                title = stringResource(R.string.admin_users_password),
                initial = form.password,
                entry = Entry.Secret,
                check = { typed -> passwordProblem(typed, form.username)?.let(words) },
                onSave = { typed ->
                    close()
                    model.edit { it.copy(password = typed) }
                },
                onDismiss = close,
            )
        }
        NewUserDialog.Pin -> {
            val digits = stringResource(R.string.admin_users_pin_digits)
            TextEntryDialog(
                title = stringResource(R.string.admin_users_pin),
                initial = form.pin,
                entry = Entry.Number,
                hint = stringResource(R.string.admin_users_pin_hint),
                // Empty takes the PIN away again: it is optional.
                check = { typed -> if (typed.isEmpty() || pinOk(typed)) null else digits },
                onSave = { typed ->
                    close()
                    model.edit { it.copy(pin = typed) }
                },
                onDismiss = close,
            )
        }
        NewUserDialog.Requests -> ChoiceSheet(
            title = stringResource(R.string.admin_users_requests),
            choices = listOf(RequestAccess.Ask, RequestAccess.Auto, RequestAccess.Off).map { Choice(it, requestLabel(it)) },
            selected = form.requests,
            onPick = { picked -> model.edit { it.copy(requestChoice = picked) } },
            onDismiss = close,
        )
        null -> Unit
    }
}

/** What keeps the form from being sent, in the web's words. */
@Composable
private fun problemWords(problem: NewUserProblem): String = when (problem) {
    NewUserProblem.EnterUsername -> stringResource(R.string.admin_users_enter_username)
    is NewUserProblem.Password -> passwordWords()(problem.problem)
    NewUserProblem.PinDigits -> stringResource(R.string.admin_users_pin_digits)
    NewUserProblem.PickLibrary -> stringResource(R.string.admin_users_pick_library)
    is NewUserProblem.LabelsInBoth -> stringResource(R.string.admin_users_labels_in_both, problem.labels.joinToString(", "))
}

/** A password or PIN typed, shown hidden on its row. */
private const val HIDDEN = "••••••••"
private const val HIDDEN_PIN = "••••"

internal const val NEW_USER_PAGE = "pley.admin.users.new"
internal const val NEW_USER_USERNAME = "pley.admin.users.new.username"
internal const val NEW_USER_PASSWORD = "pley.admin.users.new.password"
internal const val NEW_USER_CREATE = "pley.admin.users.new.create"
