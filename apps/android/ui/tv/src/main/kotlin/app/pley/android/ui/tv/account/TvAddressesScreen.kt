package app.pley.android.ui.tv.account

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.focusGroup
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.text.BasicText
import androidx.compose.foundation.text.input.rememberTextFieldState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import app.pley.android.auth.AddressStatus
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyTv
import app.pley.android.designsystem.PleyType
import app.pley.android.designsystem.TvSafeArea
import app.pley.android.designsystem.icon.PleyIcon
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.screens.account.AddressProblem
import app.pley.android.screens.account.AddressRow
import app.pley.android.screens.account.AddressesUiState
import app.pley.android.screens.account.AddressesViewModel
import app.pley.android.ui.tv.R
import app.pley.android.ui.tv.kit.FocusMemory
import app.pley.android.ui.tv.kit.HeldEnds
import app.pley.android.ui.tv.kit.TvPillButton
import app.pley.android.ui.tv.kit.TvTextButton
import app.pley.android.ui.tv.kit.heldKeyStaysIn
import app.pley.android.ui.tv.kit.initialFocus
import app.pley.android.ui.tv.kit.rememberFocusMemory
import app.pley.android.ui.tv.kit.rememberedFocus
import app.pley.android.ui.tv.onboarding.TvAddressEntry
import app.pley.android.ui.tv.onboarding.TvDialog
import app.pley.android.ui.tv.settings.TvPaneHeading
import app.pley.android.ui.tv.settings.TvSettingRow
import app.pley.android.ui.tv.settings.hostPort

/**
 * Addresses on TV (docs/ANDROID.md, "Home and away"; Settings → Server):
 * where the server answers, at home and away, which address Pley uses and
 * what it knows of the others. Opening it has them all prove themselves
 * again.
 */
@Composable
internal fun TvAddressesScreen() {
    val model: AddressesViewModel = viewModel(factory = AddressesViewModel.factory)
    val state by model.state.collectAsStateWithLifecycle()
    LaunchedEffect(state.server) { if (state.server != null) model.check() }
    TvAddressesContent(state, onAdd = model::add, onEdited = model::edited, onRemove = model::remove)
}

/**
 * The addresses, At home then Away, each with what Pley knows of it in a
 * few words; focus starts on the first. Select on one added on this TV
 * asks before removing it (the server's own are its administrator's), and
 * focus goes on to the address next to it. "Add an address" types one as
 * choosing a server does ([TvAddressEntry]): the TV's keyboard opens, Add
 * adds it and comes back to the list, Back comes back without.
 */
@Composable
internal fun TvAddressesContent(
    state: AddressesUiState,
    onAdd: (typed: String, added: () -> Unit) -> Unit,
    onEdited: () -> Unit,
    onRemove: (url: String) -> Unit,
) {
    // Kept while the address is typed, so coming back focuses Add an address again.
    val memory = rememberFocusMemory()
    var typing by rememberSaveable { mutableStateOf(false) }
    if (typing) {
        BackHandler {
            typing = false
            onEdited()
        }
        val field = rememberTextFieldState()
        TvAddressEntry(
            title = stringResource(R.string.tv_addresses_add),
            action = stringResource(R.string.tv_addresses_add_confirm),
            onSubmit = { typed -> if (!state.adding) onAdd(typed) { typing = false } },
            onEdited = onEdited,
            problem = state.problem?.let { problemText(it) },
            field = field,
        )
    } else {
        AddressList(state, memory, onAdd = { typing = true }, onRemove = onRemove)
    }
}

@Composable
private fun AddressList(state: AddressesUiState, memory: FocusMemory, onAdd: () -> Unit, onRemove: (String) -> Unit) {
    val rows = state.home + state.away
    var removing by rememberSaveable { mutableStateOf<String?>(null) }
    // Each shown address's focus, and Add an address's, for where focus goes once the dialog has gone.
    val focus = remember { HashMap<String, FocusRequester>() }
    val add = remember { FocusRequester() }
    // The address again, or the one next to it once it is removed: asked once the dialog's buttons have let go.
    var after by remember { mutableStateOf<FocusRequester?>(null) }
    LaunchedEffect(removing) {
        val to = after
        if (removing == null && to != null) {
            after = null
            withFrameNanos { }
            runCatching { to.requestFocus() }
        }
    }
    val close: (FocusRequester?) -> Unit = { to ->
        after = to
        removing = null
    }
    Box(Modifier.fillMaxSize().testTag(TV_ADDRESSES_PAGE)) {
        Column(Modifier.fillMaxSize().padding(top = TvSafeArea.Vertical)) {
            BasicText(
                stringResource(R.string.tv_settings_addresses),
                style = PleyType.Tv.PageTitle.copy(color = PleyColors.Fg),
                modifier = Modifier.padding(start = PleyTv.Gutter),
            )
            Spacer(Modifier.height(8.dp))
            BasicText(
                stringResource(if (state.canMove) R.string.tv_addresses_hint else R.string.tv_addresses_too_old),
                style = PleyType.Tv.Body.copy(color = PleyColors.Muted),
                modifier = Modifier.padding(start = PleyTv.Gutter).width(ROW_WIDTH),
            )
            Spacer(Modifier.height(12.dp))
            LazyColumn(
                Modifier.fillMaxSize().heldKeyStaysIn(HeldEnds.UpDown).focusGroup(),
                contentPadding = PaddingValues(start = PleyTv.Gutter, end = TvSafeArea.Horizontal, bottom = 120.dp),
                verticalArrangement = Arrangement.spacedBy(2.dp),
            ) {
                val line: @Composable (AddressRow) -> Unit = { row ->
                    val requester = remember { FocusRequester() }
                    DisposableEffect(row.url) {
                        focus[row.url] = requester
                        onDispose { if (focus[row.url] === requester) focus.remove(row.url) }
                    }
                    AddressLine(
                        row,
                        state.checking,
                        icon = if (row in state.home) PleyIcons.House else PleyIcons.Globe,
                        onRemove = { removing = row.url },
                        modifier = Modifier
                            .rememberedFocus(memory, "address-${row.url}", initial = row.url == rows.first().url)
                            .focusRequester(requester)
                            .testTag(TV_ADDRESS_ROW + row.url),
                    )
                }
                group("home", R.string.tv_addresses_group_home, state.home, line)
                group("away", R.string.tv_addresses_group_away, state.away, line)
                item(key = "add", contentType = "add") {
                    Column(Modifier.width(ROW_WIDTH).padding(top = 20.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                        if (!state.several && state.canMove) {
                            BasicText(
                                stringResource(R.string.tv_addresses_one),
                                style = PleyType.Tv.Caption.copy(color = PleyColors.Subtle),
                                modifier = Modifier.padding(start = 20.dp),
                            )
                        }
                        TvPillButton(
                            stringResource(R.string.tv_addresses_add),
                            onClick = onAdd,
                            icon = PleyIcons.Plus,
                            enabled = state.canMove && state.server != null,
                            modifier = Modifier.rememberedFocus(memory, "add").focusRequester(add).testTag(TV_ADDRESSES_ADD),
                        )
                    }
                }
            }
        }
        removing?.let { url ->
            val back = { close(focus[url] ?: add) }
            TvDialog(
                title = stringResource(R.string.tv_addresses_remove_title, rows.firstOrNull { it.url == url }?.shown ?: hostPort(url)),
                body = null,
                onDismiss = back,
            ) {
                // The safe choice has focus.
                TvTextButton(stringResource(R.string.tv_account_cancel), onClick = back, modifier = Modifier.initialFocus())
                TvTextButton(
                    stringResource(R.string.tv_addresses_remove),
                    onClick = {
                        // Focus goes on to the address after it, else the one before, before it goes.
                        val at = rows.indexOfFirst { it.url == url }
                        close(listOfNotNull(rows.getOrNull(at + 1), rows.getOrNull(at - 1)).firstNotNullOfOrNull { focus[it.url] } ?: add)
                        onRemove(url)
                    },
                    color = PleyColors.DangerText,
                )
            }
        }
    }
}

/** A group's heading, then its addresses; nothing for a group without any. */
private fun LazyListScope.group(key: String, title: Int, rows: List<AddressRow>, line: @Composable (AddressRow) -> Unit) {
    if (rows.isEmpty()) return
    item(key = key, contentType = "heading") { TvPaneHeading(stringResource(title)) }
    items(rows, key = { it.url }, contentType = { "address" }) { line(it) }
}

/**
 * An address and what Pley knows of it, with [icon] (a house at home, a
 * globe away), as the Settings rows have theirs. One added on this TV says
 * Remove at its end, which Select asks about; the others take focus too,
 * so the D-pad reads down the list.
 */
@Composable
private fun AddressLine(row: AddressRow, checking: Boolean, icon: PleyIcon, onRemove: () -> Unit, modifier: Modifier = Modifier) {
    TvSettingRow(
        row.shown,
        value = statusText(row.status, checking),
        icon = icon,
        onClick = { if (row.removable) onRemove() },
        modifier = modifier.width(ROW_WIDTH),
        trailing = if (row.removable) {
            { focused ->
                BasicText(
                    stringResource(R.string.tv_addresses_remove),
                    style = PleyType.Tv.Label.copy(color = if (focused) PleyColors.OnAccent else PleyColors.Muted),
                )
            }
        } else {
            null
        },
    )
}

/** What Pley knows of an address, in a few words; none for one not tried while nothing is being checked. */
@Composable
private fun statusText(status: AddressStatus, checking: Boolean): String? = when (status) {
    AddressStatus.IN_USE -> stringResource(R.string.tv_addresses_in_use)
    AddressStatus.READY -> stringResource(R.string.tv_addresses_ready)
    AddressStatus.UNTRIED -> if (checking) stringResource(R.string.tv_onboarding_checking) else null
    AddressStatus.UNREACHABLE -> stringResource(R.string.tv_onboarding_unreachable)
    AddressStatus.OTHER_DEVICE -> stringResource(R.string.tv_addresses_other_device)
    // A TV is as often on Ethernet as on Wi-Fi, and either reaches the home network.
    AddressStatus.NEEDS_HOME_NETWORK -> stringResource(R.string.tv_addresses_needs_network)
    AddressStatus.UNENCRYPTED -> stringResource(R.string.tv_onboarding_unencrypted)
}

/** Why what was [typed] wasn't added. */
@Composable
private fun problemText(problem: AddressProblem): String = when (problem) {
    AddressProblem.NOT_AN_ADDRESS -> stringResource(R.string.tv_onboarding_problem_address)
    AddressProblem.INTERNET -> stringResource(R.string.tv_addresses_problem_internet)
    AddressProblem.KNOWN -> stringResource(R.string.tv_addresses_problem_known)
}

/** The rows, the hint and Add an address: x 96 to 656. */
private val ROW_WIDTH = 560.dp

internal const val TV_ADDRESSES_PAGE = "pley.tv.addresses"
internal const val TV_ADDRESSES_ADD = "pley.tv.addresses.add"
internal const val TV_ADDRESS_ROW = "pley.tv.addresses.row."
