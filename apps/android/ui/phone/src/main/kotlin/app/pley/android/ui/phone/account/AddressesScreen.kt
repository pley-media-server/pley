package app.pley.android.ui.phone.account

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.ButtonDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import app.pley.android.auth.AddressStatus
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.icon.PleyIcon
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.screens.account.AddressProblem
import app.pley.android.screens.account.AddressRow
import app.pley.android.screens.account.AddressesUiState
import app.pley.android.screens.account.AddressesViewModel
import app.pley.android.ui.phone.R
import app.pley.android.ui.phone.kit.ConfirmDialog
import app.pley.android.ui.phone.kit.PleyAlertDialog
import app.pley.android.ui.phone.kit.PleyTopAppBar
import app.pley.android.ui.phone.kit.TonalButton
import app.pley.android.ui.phone.shell.LocalPhoneNavigator

/**
 * Addresses (docs/ANDROID.md, "Home and away"): where the server answers,
 * at home and away, which one Pley uses and what it knows of the others.
 * Opening it has them all prove themselves again. An address added here
 * can be removed here; the server's own are its administrator's.
 */
@Composable
internal fun AddressesScreen() {
    val model: AddressesViewModel = viewModel(factory = AddressesViewModel.factory)
    val state by model.state.collectAsStateWithLifecycle()
    var adding by rememberSaveable { mutableStateOf(false) }
    var removing by rememberSaveable { mutableStateOf<String?>(null) }
    val navigator = LocalPhoneNavigator.current

    LaunchedEffect(state.server) { if (state.server != null) model.check() }

    val list = rememberLazyListState()
    Column(Modifier.fillMaxSize().testTag(ADDRESSES_PAGE)) {
        PleyTopAppBar(title = stringResource(R.string.account_addresses), onBack = navigator::back, scrolled = { list.canScrollBackward })
        LazyColumn(
            state = list,
            modifier = Modifier.fillMaxSize(),
            contentPadding = PaddingValues(start = 16.dp, end = 16.dp, bottom = 32.dp),
            verticalArrangement = Arrangement.spacedBy(28.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            item(key = "hint", contentType = "hint") {
                Text(
                    stringResource(if (state.canMove) R.string.addresses_hint else R.string.addresses_too_old),
                    style = MaterialTheme.typography.bodyMedium,
                    color = PleyColors.Muted,
                    modifier = Modifier.widthIn(max = MAX_WIDTH).fillMaxWidth().padding(horizontal = 4.dp),
                )
            }
            if (state.home.isNotEmpty()) {
                item(key = "home", contentType = "group") {
                    AddressGroup(stringResource(R.string.addresses_group_home), PleyIcons.House, state.home, state, onRemove = { removing = it })
                }
            }
            if (state.away.isNotEmpty()) {
                item(key = "away", contentType = "group") {
                    AddressGroup(stringResource(R.string.addresses_group_away), PleyIcons.Globe, state.away, state, onRemove = { removing = it })
                }
            }
            item(key = "add", contentType = "add") {
                Column(Modifier.widthIn(max = MAX_WIDTH).fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                    if (!state.several && state.canMove) {
                        Text(
                            stringResource(R.string.addresses_one),
                            style = MaterialTheme.typography.bodySmall,
                            color = PleyColors.Subtle,
                            modifier = Modifier.padding(horizontal = 4.dp),
                        )
                    }
                    TonalButton(
                        stringResource(R.string.addresses_add),
                        onClick = { adding = true },
                        icon = PleyIcons.Plus,
                        enabled = state.canMove && state.server != null,
                        modifier = Modifier.testTag(ADDRESSES_ADD),
                    )
                }
            }
        }
    }

    if (adding) {
        AddAddressDialog(
            state = state,
            onEdit = model::edited,
            onAdd = { typed -> model.add(typed) { adding = false } },
            onDismiss = {
                adding = false
                model.edited()
            },
        )
    }
    removing?.let { url ->
        ConfirmDialog(
            title = stringResource(R.string.addresses_remove_title, displayOf(url, state)),
            body = null,
            confirmLabel = stringResource(R.string.addresses_remove),
            onConfirm = {
                removing = null
                model.remove(url)
            },
            onDismiss = { removing = null },
            destructive = true,
        )
    }
}

/** A group of addresses, each with [icon] (a house at home, a globe away), as Me's rows have theirs. */
@Composable
private fun AddressGroup(title: String, icon: PleyIcon, rows: List<AddressRow>, state: AddressesUiState, onRemove: (String) -> Unit) {
    SettingsGroup(title, Modifier.widthIn(max = MAX_WIDTH)) {
        rows.forEachIndexed { i, row ->
            if (i > 0) RowDivider()
            SettingsRow(
                title = row.shown,
                value = statusText(row.status, state.checking),
                icon = icon,
                chevron = false,
                enabled = true,
                onClick = { if (row.removable) onRemove(row.url) },
                modifier = Modifier.testTag(ADDRESS_ROW_PREFIX + row.url),
                trailing = if (row.removable) {
                    {
                        TextButton(
                            onClick = { onRemove(row.url) },
                            colors = ButtonDefaults.textButtonColors(contentColor = PleyColors.Muted),
                        ) { Text(stringResource(R.string.addresses_remove), style = MaterialTheme.typography.labelLarge) }
                    }
                } else {
                    null
                },
            )
        }
    }
}

/** What Pley knows of an address, in a few words; none for one not tried while nothing is being checked. */
@Composable
private fun statusText(status: AddressStatus, checking: Boolean): String? = when (status) {
    AddressStatus.IN_USE -> stringResource(R.string.addresses_in_use)
    AddressStatus.READY -> stringResource(R.string.addresses_ready)
    AddressStatus.UNTRIED -> if (checking) stringResource(R.string.onboarding_checking) else null
    AddressStatus.UNREACHABLE -> stringResource(R.string.onboarding_unreachable)
    AddressStatus.OTHER_DEVICE -> stringResource(R.string.addresses_other_device)
    AddressStatus.NEEDS_HOME_NETWORK -> stringResource(R.string.addresses_needs_wifi)
    AddressStatus.UNENCRYPTED -> stringResource(R.string.onboarding_unencrypted)
}

/** Adding an address: the field the sign-in screen has, Add and Cancel, and what was wrong under it. */
@Composable
private fun AddAddressDialog(state: AddressesUiState, onEdit: () -> Unit, onAdd: (String) -> Unit, onDismiss: () -> Unit) {
    var typed by rememberSaveable { mutableStateOf("") }
    val focus = remember { FocusRequester() }
    LaunchedEffect(Unit) { runCatching { focus.requestFocus() } }
    PleyAlertDialog(
        onDismissRequest = onDismiss,
        confirmButton = {
            TextButton(
                onClick = { onAdd(typed) },
                enabled = typed.isNotBlank() && !state.adding,
                colors = ButtonDefaults.textButtonColors(contentColor = PleyColors.AccentBright),
                modifier = Modifier.testTag(ADDRESSES_ADD_CONFIRM),
            ) { Text(stringResource(R.string.addresses_add_confirm), style = MaterialTheme.typography.labelLarge) }
        },
        dismissButton = {
            TextButton(onClick = onDismiss, colors = ButtonDefaults.textButtonColors(contentColor = PleyColors.Fg)) {
                Text(stringResource(R.string.kit_cancel), style = MaterialTheme.typography.labelLarge)
            }
        },
        title = { Text(stringResource(R.string.addresses_add), style = MaterialTheme.typography.headlineSmall) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                OutlinedTextField(
                    value = typed,
                    onValueChange = {
                        typed = it
                        onEdit()
                    },
                    singleLine = true,
                    label = { Text(stringResource(R.string.onboarding_address_label)) },
                    placeholder = { Text(stringResource(R.string.onboarding_address_placeholder)) },
                    keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Uri, imeAction = ImeAction.Done, autoCorrectEnabled = false),
                    keyboardActions = KeyboardActions(onDone = { if (typed.isNotBlank()) onAdd(typed) }),
                    isError = state.problem != null,
                    shape = PleyShape.Field,
                    colors = OutlinedTextFieldDefaults.colors(
                        focusedContainerColor = PleyColors.Surface,
                        unfocusedContainerColor = PleyColors.Surface,
                        errorContainerColor = PleyColors.Surface,
                        errorBorderColor = PleyColors.DangerText,
                        errorLabelColor = PleyColors.DangerText,
                    ),
                    modifier = Modifier.fillMaxWidth().focusRequester(focus).testTag(ADDRESSES_FIELD),
                )
                state.problem?.let { problem ->
                    Text(
                        problemText(problem),
                        style = MaterialTheme.typography.bodyMedium,
                        color = PleyColors.DangerText,
                        modifier = Modifier.semantics { liveRegion = LiveRegionMode.Polite },
                    )
                }
            }
        },
    )
}

@Composable
private fun problemText(problem: AddressProblem): String = when (problem) {
    AddressProblem.NOT_AN_ADDRESS -> stringResource(R.string.onboarding_problem_address)
    AddressProblem.INTERNET -> stringResource(R.string.addresses_problem_internet)
    AddressProblem.KNOWN -> stringResource(R.string.addresses_problem_known)
}

private fun displayOf(url: String, state: AddressesUiState): String =
    (state.home + state.away).firstOrNull { it.url == url }?.shown ?: hostPort(url)

/** Rows stop growing at this width (tablets), as on Me. */
private val MAX_WIDTH = 640.dp

internal const val ADDRESSES_PAGE = "pley.addresses.page"
internal const val ADDRESSES_ADD = "pley.addresses.add"
internal const val ADDRESSES_ADD_CONFIRM = "pley.addresses.add-confirm"
internal const val ADDRESSES_FIELD = "pley.addresses.field"
internal const val ADDRESS_ROW_PREFIX = "pley.addresses.row:"
