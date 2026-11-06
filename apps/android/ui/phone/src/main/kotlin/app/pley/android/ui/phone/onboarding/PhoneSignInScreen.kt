package app.pley.android.ui.phone.onboarding

import android.content.Context
import android.content.Intent
import androidx.activity.compose.LocalActivity
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
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
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.CustomAccessibilityAction
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.customActions
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.core.net.toUri
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.createSavedStateHandle
import androidx.lifecycle.compose.LifecycleEventEffect
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import app.pley.android.common.Language
import app.pley.android.common.UiText
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.icon.Icon
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.designsystem.text.asString
import app.pley.android.screens.PleyKey
import app.pley.android.screens.onboarding.BrowserSignIn
import app.pley.android.screens.onboarding.LocalAccess
import app.pley.android.screens.onboarding.ServerRow
import app.pley.android.screens.onboarding.ServersProblem
import app.pley.android.screens.onboarding.ServersUiState
import app.pley.android.screens.onboarding.ServersViewModel
import app.pley.android.screens.onboarding.SignInProblem
import app.pley.android.screens.onboarding.SignInViewModel
import app.pley.android.screens.onboarding.go
import app.pley.android.screens.onboarding.onboardingSource
import app.pley.android.screens.settings.DeviceLanguageUiState
import app.pley.android.screens.settings.DeviceLanguageViewModel
import app.pley.android.ui.phone.R
import app.pley.android.ui.phone.kit.BusyLine
import app.pley.android.ui.phone.kit.ConfirmDialog
import app.pley.android.ui.phone.kit.PleyTextButton
import app.pley.android.ui.phone.kit.PrimaryButton
import app.pley.android.ui.phone.kit.TonalButton
import app.pley.android.ui.phone.shell.LocalPhoneNavigator

/**
 * "Sign in to pley" (design spec §4.7, mockup phone-signin): the servers
 * this phone knows and those that answer on the network, an address to
 * type, and "Continue in browser", which opens the chosen server's own
 * sign-in page in a private Custom Tab. Only when no browser on the phone
 * opens that page does the button sign in with a code from another device
 * instead. Over the button, a chosen server that requires two-step sign-in
 * says so: its page sets it up. [preselect] is the server the app was
 * signed out of (PleyKey.SignIn). The globe in the top bar picks this
 * device's language.
 */
@Composable
internal fun PhoneSignInScreen(preselect: String?) {
    val navigator = LocalPhoneNavigator.current
    val activity = LocalActivity.current
    val context = LocalContext.current
    val given = LocalOnboardingSource.current
    val servers = viewModel(key = "servers:$preselect") { ServersViewModel(given ?: onboardingSource(), explainFirst = false, preselect = preselect) }
    val signIn = viewModel(key = "sign-in") { SignInViewModel(given ?: onboardingSource(), createSavedStateHandle()) }
    val state by servers.state.collectAsStateWithLifecycle()
    val signInState by signIn.state.collectAsStateWithLifecycle()
    val languageModel: DeviceLanguageViewModel = viewModel(factory = DeviceLanguageViewModel.factory)
    val language by languageModel.state.collectAsStateWithLifecycle()
    val withoutBrowser = remember(state.selected) { noBrowserFor(context, state.selected) }
    var viaCode by rememberSaveable { mutableStateOf(false) }

    LaunchedEffect(servers) {
        servers.chosen.collect { chosen ->
            when {
                chosen.signedIn -> navigator.restart(PleyKey.Home)
                viaCode || noBrowserFor(context, chosen.url) -> navigator.open(PleyKey.Link(chosen.url))
                else -> signIn.signIn(activity, chosen.url)
            }
        }
    }
    LaunchedEffect(signIn) { signIn.navigation.collect { navigator.go(it) } }
    val ask = rememberLauncherForActivityResult(ActivityResultContracts.RequestMultiplePermissions()) { servers.permissionsAnswered() }
    LaunchedEffect(state.ask) {
        if (state.ask.isNotEmpty()) {
            servers.asked()
            ask.launch(state.ask.toTypedArray())
        }
    }
    LifecycleEventEffect(Lifecycle.Event.ON_PAUSE) { signIn.paused() }
    LifecycleEventEffect(Lifecycle.Event.ON_RESUME) {
        signIn.resumed()
        servers.resumed()
    }

    PhoneSignInContent(
        state = state,
        signIn = signInState,
        withoutBrowser = withoutBrowser,
        onSelect = servers::select,
        onForget = servers::forget,
        onConnect = { servers.connect(it, open = false) },
        onConnectAndContinue = { address ->
            viaCode = false
            servers.connect(address, open = true)
        },
        onAddressEdited = servers::addressEdited,
        onSearchAgain = servers::search,
        onDemo = servers::useDemo,
        onOpenSettings = { openAppSettings(context) },
        onContinue = {
            viaCode = false
            state.selected?.let(servers::choose)
        },
        onUseCode = {
            viaCode = true
            state.selected?.let(servers::choose)
        },
        onRetry = {
            signIn.dismiss()
            viaCode = false
            state.selected?.let(servers::choose)
        },
        onOpenInBrowser = { server -> openInBrowser(context, server) },
        language = language,
        onLanguage = languageModel::choose,
    )
}

/**
 * Whether nothing on this phone opens [server]'s sign-in page: no app
 * takes a browsable view of its address. Browsers are visible to the app
 * from Android 11 through the manifest's queries.
 */
private fun noBrowserFor(context: Context, server: String?): Boolean {
    val page = server?.toUri()?.takeIf { it.scheme == "http" || it.scheme == "https" } ?: ANY_PAGE.toUri()
    val view = Intent(Intent.ACTION_VIEW, page).addCategory(Intent.CATEGORY_BROWSABLE)
    return view.resolveActivity(context.packageManager) == null
}

/** A page to ask about before a server is chosen. */
private const val ANY_PAGE = "https://example.com"

@OptIn(ExperimentalFoundationApi::class)
@Composable
internal fun PhoneSignInContent(
    state: ServersUiState,
    signIn: BrowserSignIn,
    withoutBrowser: Boolean,
    onSelect: (String) -> Unit,
    onForget: (String) -> Unit,
    onConnect: (String) -> Unit,
    onAddressEdited: () -> Unit,
    onSearchAgain: () -> Unit,
    onDemo: () -> Unit,
    onOpenSettings: () -> Unit,
    onContinue: () -> Unit,
    onUseCode: () -> Unit,
    onRetry: () -> Unit,
    onOpenInBrowser: (String) -> Unit,
    /** Continue with the address typed, once it answers: the button under the field says Continue, so it goes there. */
    onConnectAndContinue: (String) -> Unit = onConnect,
    /** This device's language, for the globe; `null`: no globe. */
    language: DeviceLanguageUiState? = null,
    onLanguage: (Language?) -> Unit = {},
) {
    var forgetting by remember { mutableStateOf<ServerRow?>(null) }
    var addressOpen by rememberSaveable { mutableStateOf(false) }
    var address by rememberSaveable { mutableStateOf("") }
    var submitted by remember { mutableStateOf(false) }
    // A typed address that answered joins the list, chosen: the field closes.
    // One that didn't stays to be put right (typing clears its problem).
    LaunchedEffect(state.checkingAddress, state.addressProblem) {
        if (submitted && !state.checkingAddress) {
            submitted = false
            if (state.addressProblem == null) {
                addressOpen = false
                address = ""
            }
        }
    }
    val busy = signIn == BrowserSignIn.Opening || signIn == BrowserSignIn.Waiting || signIn == BrowserSignIn.Finishing ||
        signIn == BrowserSignIn.SignedIn || state.busy != null
    Box(
        Modifier
            .fillMaxSize()
            .brandCorner()
            .imePadding()
            .testTag(SIGN_IN_TAG),
    ) {
        Column(Modifier.fillMaxSize()) {
            OnboardingTopBar(language, onLanguage, enabled = !busy)
            Column(
                Modifier
                    .weight(1f)
                    .fillMaxWidth()
                    .verticalScroll(rememberScrollState())
                    .padding(horizontal = 24.dp)
                    .widthIn(max = 560.dp),
            ) {
                // The wordmark where it was without the bar, 96 dp under the status bar.
                Spacer(Modifier.height(32.dp))
                PleyWordmark()
                Spacer(Modifier.height(56.dp))
                Text(stringResource(R.string.onboarding_title), style = MaterialTheme.typography.headlineMedium, color = PleyColors.Fg)
                Spacer(Modifier.height(10.dp))
                Text(
                    stringResource(if (state.rows.any { it.found }) R.string.onboarding_found else R.string.onboarding_choose),
                    style = MaterialTheme.typography.bodyLarge,
                    color = PleyColors.Muted,
                )
                Spacer(Modifier.height(20.dp))
                Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                    for (row in state.rows) {
                        ServerCard(
                            row = row,
                            selected = row.url == state.selected,
                            enabled = !busy,
                            onClick = { onSelect(row.url) },
                            onForget = if (row.saved) {
                                { forgetting = row }
                            } else {
                                null
                            },
                        )
                    }
                }
                ListStatus(state, onSearchAgain, onOpenSettings)
                Spacer(Modifier.height(16.dp))
                if (addressOpen) {
                    AddressField(
                        value = address,
                        onValueChange = {
                            address = it
                            onAddressEdited()
                        },
                        checking = state.checkingAddress,
                        problem = state.addressProblem,
                        onConnect = {
                            submitted = true
                            onConnect(address)
                        },
                    )
                } else {
                    TonalButton(
                        stringResource(R.string.onboarding_enter_address),
                        onClick = { addressOpen = true },
                        icon = PleyIcons.Plus,
                        enabled = !busy,
                        modifier = Modifier.fillMaxWidth().heightIn(min = 48.dp).testTag(ENTER_ADDRESS_TAG),
                    )
                }
                if (state.demo) {
                    PleyTextButton(stringResource(R.string.onboarding_demo), onClick = onDemo, enabled = !busy, modifier = Modifier.testTag(DEMO_TAG))
                }
                Spacer(Modifier.height(24.dp))
            }
            Column(
                Modifier
                    .fillMaxWidth()
                    .navigationBarsPadding()
                    .padding(start = 24.dp, end = 24.dp, bottom = 16.dp),
                verticalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                // An address typed and not connected yet is what Continue means: the
                // field's own Connect may be scrolled away under the keyboard.
                val typed = addressOpen && address.isNotBlank()
                // It's about the chosen card: hidden while Continue means a typed address.
                if (!typed && state.rows.any { it.url == state.selected && it.twoStepRequired }) {
                    Text(stringResource(R.string.onboarding_two_step_required), style = MaterialTheme.typography.bodyMedium, color = PleyColors.Muted)
                }
                SignInStatus(signIn, state, onRetry, onOpenInBrowser)
                PrimaryButton(
                    text = stringResource(if (withoutBrowser) R.string.onboarding_code_primary else R.string.onboarding_continue),
                    onClick = when {
                        typed -> {
                            {
                                submitted = true
                                onConnectAndContinue(address)
                            }
                        }
                        withoutBrowser -> onUseCode
                        else -> onContinue
                    },
                    trailingIcon = if (withoutBrowser) null else BrowserIcon,
                    enabled = (state.selected != null || typed) && !busy && !state.checkingAddress,
                    modifier = Modifier.fillMaxWidth().testTag(CONTINUE_TAG),
                )
            }
        }
    }
    forgetting?.let { row ->
        ConfirmDialog(
            title = stringResource(R.string.onboarding_forget_title, row.name),
            body = stringResource(R.string.onboarding_forget_body),
            confirmLabel = stringResource(R.string.onboarding_forget),
            onConfirm = {
                forgetting = null
                onForget(row.url)
            },
            onDismiss = { forgetting = null },
            destructive = true,
        )
    }
}

/** A server as a card (radius 12): its name and address, and where it was found. Chosen, a 2 dp accent edge. */
@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun ServerCard(row: ServerRow, selected: Boolean, enabled: Boolean, onClick: () -> Unit, onForget: (() -> Unit)?) {
    val forgetLabel = stringResource(R.string.onboarding_forget)
    Row(
        Modifier
            .fillMaxWidth()
            .heightIn(min = 68.dp)
            .background(PleyColors.Surface2, PleyShape.Card)
            .border(if (selected) 2.dp else 1.dp, if (selected) PleyColors.Accent else PleyColors.Surface4, PleyShape.Card)
            .combinedClickable(enabled = enabled, onLongClick = onForget, onClick = onClick)
            .semantics {
                role = Role.RadioButton
                this.selected = selected
                if (onForget != null) customActions = listOf(CustomAccessibilityAction(forgetLabel) { onForget(); true })
            }
            .testTag(SERVER_CARD_TAG + row.url)
            .padding(horizontal = 16.dp, vertical = 14.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(Modifier.size(40.dp).background(PleyColors.Surface3, PleyShape.Field), contentAlignment = Alignment.Center) {
            Icon(PleyIcons.Monitor, contentDescription = null, size = 22.dp, tint = PleyColors.Fg)
        }
        Spacer(Modifier.size(16.dp))
        Column(Modifier.weight(1f)) {
            Text(row.name, style = MaterialTheme.typography.titleMedium, color = PleyColors.Fg, maxLines = 1, overflow = TextOverflow.Ellipsis)
            if (row.unreachable) {
                Text(stringResource(R.string.onboarding_unreachable), style = MaterialTheme.typography.bodyMedium, color = PleyColors.DangerText)
            } else {
                Text(row.address, style = MaterialTheme.typography.bodyMedium, color = PleyColors.Muted, maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
        }
        val (tag, warning) = when {
            row.lookalike -> stringResource(R.string.onboarding_lookalike) to true
            row.unencrypted -> stringResource(R.string.onboarding_unencrypted) to true
            row.found -> stringResource(R.string.onboarding_on_network) to false
            else -> null to false
        }
        if (tag != null) {
            Spacer(Modifier.size(8.dp))
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                if (!warning) Box(Modifier.size(8.dp).background(PleyColors.Success, PleyShape.Circle))
                Text(tag, style = MaterialTheme.typography.bodyMedium, color = if (warning) PleyColors.DangerText else PleyColors.Muted)
            }
        }
    }
}

/** Under the cards: searching, nothing found, or no access to the home network. */
@Composable
private fun ListStatus(state: ServersUiState, onSearchAgain: () -> Unit, onOpenSettings: () -> Unit) {
    Column(Modifier.padding(top = 12.dp).semantics { liveRegion = LiveRegionMode.Polite }) {
        when {
            state.searching -> BusyLine(stringResource(R.string.onboarding_searching))
            state.access == LocalAccess.Denied -> {
                Text(stringResource(R.string.onboarding_access_denied), style = MaterialTheme.typography.bodyMedium, color = PleyColors.Muted)
                PleyTextButton(stringResource(R.string.onboarding_open_settings), onClick = onOpenSettings)
            }
            state.searched && state.rows.isEmpty() -> {
                Text(stringResource(R.string.onboarding_none), style = MaterialTheme.typography.bodyMedium, color = PleyColors.Muted)
                PleyTextButton(stringResource(R.string.onboarding_search_again), onClick = onSearchAgain)
            }
        }
        state.problem?.let { Text(problemText(it), style = MaterialTheme.typography.bodyMedium, color = PleyColors.DangerText) }
    }
}

/** Typing an address: a field and Connect; what was wrong under it. */
@Composable
private fun AddressField(value: String, onValueChange: (String) -> Unit, checking: Boolean, problem: ServersProblem?, onConnect: () -> Unit) {
    val focus = remember { FocusRequester() }
    LaunchedEffect(Unit) { runCatching { focus.requestFocus() } }
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        OutlinedTextField(
            value = value,
            onValueChange = onValueChange,
            singleLine = true,
            label = { Text(stringResource(R.string.onboarding_address_label)) },
            placeholder = { Text(stringResource(R.string.onboarding_address_placeholder)) },
            keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Uri, imeAction = ImeAction.Go, autoCorrectEnabled = false),
            keyboardActions = KeyboardActions(onGo = { onConnect() }),
            isError = problem != null,
            shape = PleyShape.Field,
            colors = OutlinedTextFieldDefaults.colors(
                focusedContainerColor = PleyColors.Surface2,
                unfocusedContainerColor = PleyColors.Surface2,
                errorContainerColor = PleyColors.Surface2,
                errorBorderColor = PleyColors.DangerText,
                errorLabelColor = PleyColors.DangerText,
            ),
            modifier = Modifier.fillMaxWidth().focusRequester(focus).testTag(ADDRESS_FIELD_TAG),
        )
        if (problem != null) {
            Text(
                problemText(problem),
                style = MaterialTheme.typography.bodyMedium,
                color = PleyColors.DangerText,
                modifier = Modifier.semantics { liveRegion = LiveRegionMode.Polite }.testTag(ADDRESS_PROBLEM_TAG),
            )
        }
        TonalButton(
            stringResource(if (checking) R.string.onboarding_checking else R.string.onboarding_connect),
            onClick = onConnect,
            enabled = !checking && value.isNotBlank(),
            modifier = Modifier.fillMaxWidth().heightIn(min = 48.dp).testTag(CONNECT_TAG),
        )
    }
}

/** Over the button: opening, waiting and finishing with a spinner after 400 ms; what went wrong, with its way out. */
@Composable
private fun SignInStatus(signIn: BrowserSignIn, state: ServersUiState, onRetry: () -> Unit, onOpenInBrowser: (String) -> Unit) {
    val live = Modifier.semantics { liveRegion = LiveRegionMode.Polite }.testTag(SIGN_IN_STATUS_TAG)
    when (signIn) {
        BrowserSignIn.Opening -> BusyLine(stringResource(R.string.onboarding_opening), live)
        BrowserSignIn.Waiting -> BusyLine(stringResource(R.string.onboarding_waiting), live)
        BrowserSignIn.Finishing, BrowserSignIn.SignedIn -> BusyLine(stringResource(R.string.onboarding_finishing), live)
        is BrowserSignIn.Failed -> Column(live, verticalArrangement = Arrangement.spacedBy(8.dp)) {
            val name = state.rows.firstOrNull { it.url == signIn.server }?.name ?: ServersViewModel.addressOf(signIn.server)
            val text = when (signIn.problem) {
                SignInProblem.TooOld -> stringResource(R.string.onboarding_too_old, name)
                else -> UiText.Sentence(signIn.text).asString()
            }
            Text(text, style = MaterialTheme.typography.bodyMedium, color = PleyColors.DangerText)
            Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                if (signIn.problem == SignInProblem.TooOld) {
                    TonalButton(stringResource(R.string.onboarding_open_in_browser), onClick = { onOpenInBrowser(signIn.server) }, trailingIcon = BrowserIcon)
                }
                TonalButton(stringResource(R.string.onboarding_retry), onClick = onRetry, icon = PleyIcons.RefreshCw)
            }
        }
        BrowserSignIn.Idle -> if (state.busy != null) BusyLine(stringResource(R.string.onboarding_checking), live)
    }
}

/** What went wrong with a server, in words. */
@Composable
internal fun problemText(problem: ServersProblem): String = when (problem) {
    is ServersProblem.Unreachable -> stringResource(R.string.onboarding_problem_unreachable, problem.address, problem.reason.asString())
    is ServersProblem.NotPley -> stringResource(R.string.onboarding_problem_not_pley, problem.address)
    is ServersProblem.Unencrypted -> stringResource(R.string.onboarding_problem_unencrypted, problem.address)
    ServersProblem.BadAddress -> stringResource(R.string.onboarding_problem_address)
    is ServersProblem.Failed -> UiText.Sentence(problem.text).asString()
}

internal const val SIGN_IN_TAG = "pley.signin"
internal const val SERVER_CARD_TAG = "pley.signin.server."
internal const val ENTER_ADDRESS_TAG = "pley.signin.enter_address"
internal const val ADDRESS_FIELD_TAG = "pley.signin.address"
internal const val ADDRESS_PROBLEM_TAG = "pley.signin.address.problem"
internal const val CONNECT_TAG = "pley.signin.connect"
internal const val CONTINUE_TAG = "pley.signin.continue"
internal const val DEMO_TAG = "pley.signin.demo"
internal const val SIGN_IN_STATUS_TAG = "pley.signin.status"
