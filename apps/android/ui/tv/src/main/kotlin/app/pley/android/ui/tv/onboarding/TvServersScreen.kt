package app.pley.android.ui.tv.onboarding

import android.content.ActivityNotFoundException
import android.content.Intent
import android.provider.Settings
import androidx.activity.compose.BackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.tween
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.background
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.focusGroup
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.platform.LocalWindowInfo
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.core.net.toUri
import androidx.lifecycle.compose.LifecycleResumeEffect
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import app.pley.android.common.Language
import app.pley.android.common.UiText
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.PleyType
import app.pley.android.designsystem.icon.Icon
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.designsystem.text.asString
import app.pley.android.screens.PleyKey
import app.pley.android.screens.onboarding.LocalAccess
import app.pley.android.screens.onboarding.ServerRow
import app.pley.android.screens.onboarding.ServersProblem
import app.pley.android.screens.onboarding.ServersUiState
import app.pley.android.screens.onboarding.ServersViewModel
import app.pley.android.screens.onboarding.onboardingSource
import app.pley.android.screens.settings.DeviceLanguageUiState
import app.pley.android.screens.settings.DeviceLanguageViewModel
import app.pley.android.ui.tv.R
import app.pley.android.ui.tv.kit.TvTextButton
import app.pley.android.ui.tv.kit.HeldEnds
import app.pley.android.ui.tv.kit.heldKeyStaysIn
import app.pley.android.ui.tv.kit.initialFocus
import app.pley.android.ui.tv.kit.menuKey
import app.pley.android.ui.tv.shell.LocalTvNavigator

/**
 * "Choose your server" on TV (design spec §4.7): saved servers first, then
 * those that answer as they come; "Enter an address" (on the TV's keyboard),
 * "Search this network", and the demo server in debug builds. Holding OK
 * on a saved server forgets it. Beside the title, the globe picks this
 * device's language.
 */
@Composable
internal fun TvServersScreen() {
    val navigator = LocalTvNavigator.current
    val given = LocalOnboardingSource.current
    val vm = viewModel { ServersViewModel(given ?: onboardingSource(), explainFirst = true) }
    val state by vm.state.collectAsStateWithLifecycle()
    val languageModel: DeviceLanguageViewModel = viewModel(factory = DeviceLanguageViewModel.factory)
    val language by languageModel.state.collectAsStateWithLifecycle()
    LaunchedEffect(vm) {
        vm.chosen.collect { chosen ->
            navigator.restart(
                when {
                    !chosen.signedIn -> PleyKey.Link(chosen.url)
                    chosen.profiles > 1 -> PleyKey.Profiles()
                    else -> PleyKey.Home
                },
            )
        }
    }
    val ask = rememberLauncherForActivityResult(ActivityResultContracts.RequestMultiplePermissions()) { vm.permissionsAnswered() }
    LaunchedEffect(state.ask) {
        if (state.ask.isNotEmpty()) {
            vm.asked()
            ask.launch(state.ask.toTypedArray())
        }
    }
    LifecycleResumeEffect(vm) {
        vm.resumed()
        onPauseOrDispose { }
    }
    var typing by rememberSaveable { mutableStateOf(false) }
    if (typing) {
        BackHandler { typing = false }
        TvAddressEntry(
            state = state,
            onConnect = { vm.connect(it, open = true) },
            onEdited = vm::addressEdited,
        )
    } else {
        val context = LocalContext.current
        TvServersContent(
            state = state,
            onChoose = vm::choose,
            onForget = vm::forget,
            onEnterAddress = { typing = true },
            onSearchNetwork = vm::searchNetwork,
            onDemo = vm::useDemo,
            onOpenSettings = { openAppSettings(context) },
            language = language,
            onLanguage = languageModel::choose,
        )
    }
    if (state.access == LocalAccess.Explain) {
        TvDialog(
            title = stringResource(R.string.tv_onboarding_access_title),
            body = stringResource(R.string.tv_onboarding_access_body),
            onDismiss = vm::permissionsAnswered,
        ) {
            TvTextButton(stringResource(R.string.tv_onboarding_continue), onClick = vm::explained, modifier = Modifier.initialFocus())
        }
    }
}

/** Opens this app's page in the system's settings (absent on some TVs). */
private fun openAppSettings(context: android.content.Context) {
    try {
        context.startActivity(
            Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS, "package:${context.packageName}".toUri()).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK),
        )
    } catch (_: ActivityNotFoundException) {
        // No settings screen for apps here: nothing to open.
    }
}

@OptIn(ExperimentalFoundationApi::class)
@Composable
internal fun TvServersContent(
    state: ServersUiState,
    onChoose: (String) -> Unit,
    onForget: (String) -> Unit,
    onEnterAddress: () -> Unit,
    onSearchNetwork: () -> Unit,
    onDemo: () -> Unit,
    onOpenSettings: () -> Unit = {},
    language: DeviceLanguageUiState = DeviceLanguageUiState(),
    onLanguage: (Language?) -> Unit = {},
) {
    var forgetting by remember { mutableStateOf<ServerRow?>(null) }
    val picker = rememberTvLanguagePicker()
    val firstRow = remember { FocusRequester() }
    // Each row's focus, for the row a cancelled Forget goes back to.
    val rowFocus = remember { HashMap<String, FocusRequester>() }
    val enterAddress = remember { FocusRequester() }
    var touched by remember { mutableStateOf(false) }
    val rows = state.rows
    // D-pad rule 1: focus on the first server; "Enter an address" until one shows, or when none does.
    // Asked again once the window has focus: asked before, on a cold start, it is lost, and the
    // window's own first focus goes to the left-most thing on the screen, the globe.
    val windowFocused = LocalWindowInfo.current.isWindowFocused
    LaunchedEffect(rows.isNotEmpty(), state.searched, windowFocused) {
        if (touched) return@LaunchedEffect
        if (rows.isNotEmpty()) runCatching { firstRow.requestFocus() } else runCatching { enterAddress.requestFocus() }
    }
    Box(
        Modifier
            .fillMaxSize()
            .brandGlow()
            .onPreviewKeyEvent {
                touched = true
                false
            }
            .testTag(SERVERS_TAG),
    ) {
        Column(Modifier.padding(start = 96.dp, top = 27.dp, end = 48.dp, bottom = 27.dp).fillMaxSize()) {
            Row(Modifier.fillMaxWidth().height(40.dp), verticalAlignment = Alignment.CenterVertically) {
                BasicText(
                    stringResource(R.string.tv_onboarding_servers_title),
                    style = PleyType.Tv.PageTitle.copy(color = PleyColors.Fg),
                    modifier = Modifier.weight(1f),
                )
                PleyLogo(height = 32.dp)
            }
            Spacer(Modifier.height(25.dp))
            LazyColumn(
                Modifier.width(ROW_WIDTH + 16.dp).heightIn(max = 300.dp).heldKeyStaysIn(HeldEnds.UpDown).focusGroup(),
                verticalArrangement = Arrangement.spacedBy(4.dp),
            ) {
                items(rows, key = { it.url }, contentType = { "server" }) { row ->
                    val first = row.url == rows.first().url
                    val requester = remember { FocusRequester() }
                    DisposableEffect(row.url) {
                        rowFocus[row.url] = requester
                        onDispose { if (rowFocus[row.url] === requester) rowFocus.remove(row.url) }
                    }
                    TvServerRow(
                        row = row,
                        connecting = state.busy == row.url,
                        onClick = { onChoose(row.url) },
                        onLongClick = if (row.saved) {
                            { forgetting = row }
                        } else {
                            null
                        },
                        modifier = (if (first) Modifier.focusRequester(firstRow) else Modifier)
                            .focusRequester(requester),
                    )
                }
            }
            Spacer(Modifier.height(if (rows.isEmpty()) 0.dp else 16.dp))
            Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                TvTextButton(
                    stringResource(R.string.tv_onboarding_enter_address),
                    onClick = onEnterAddress,
                    modifier = Modifier.focusRequester(enterAddress).testTag(ENTER_ADDRESS_TAG),
                )
                TvTextButton(
                    stringResource(R.string.tv_onboarding_search_network),
                    onClick = onSearchNetwork,
                    enabled = !state.sweeping,
                    modifier = Modifier.testTag(SEARCH_NETWORK_TAG),
                )
                if (state.demo) {
                    TvTextButton(
                        stringResource(R.string.tv_onboarding_demo),
                        // Kept enabled while it starts, so focus stays on it.
                        onClick = onDemo,
                        modifier = Modifier.testTag(DEMO_TAG),
                    )
                }
            }
            Spacer(Modifier.height(16.dp))
            ServersStatus(state, onOpenSettings, Modifier.width(ROW_WIDTH))
        }
        TvOnboardingFoot(picker, language) {
            if (rows.any { it.saved }) {
                BasicText(stringResource(R.string.tv_onboarding_forget_hint), style = PleyType.Tv.Caption.copy(color = PleyColors.Subtle), maxLines = 1)
            }
        }
        TvLanguagePanel(picker, language, onLanguage)
    }
    forgetting?.let { row ->
        TvDialog(
            title = stringResource(R.string.tv_onboarding_forget_title, row.name),
            body = stringResource(R.string.tv_onboarding_forget_body),
            onDismiss = { forgetting = null },
            returnTo = rowFocus[row.url],
        ) {
            // The safe choice has focus.
            TvTextButton(stringResource(R.string.tv_onboarding_cancel), onClick = { forgetting = null }, modifier = Modifier.initialFocus())
            TvTextButton(
                stringResource(R.string.tv_onboarding_forget),
                onClick = {
                    forgetting = null
                    onForget(row.url)
                },
                color = PleyColors.DangerText,
            )
        }
    }
}

/** One server: a 64 dp row, 520 wide; focused, an accent fill without scale (rows in lists don't scale). */
@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun TvServerRow(
    row: ServerRow,
    connecting: Boolean,
    onClick: () -> Unit,
    onLongClick: (() -> Unit)?,
    modifier: Modifier = Modifier,
) {
    var focused by remember { mutableStateOf(false) }
    val interactions = remember { MutableInteractionSource() }
    val ink = if (focused) PleyColors.OnAccent else PleyColors.Fg
    val soft = if (focused) PleyColors.OnAccent else PleyColors.Muted
    Row(
        modifier
            .width(ROW_WIDTH)
            .height(64.dp)
            .testTag(SERVER_ROW_TAG + row.url)
            .onFocusChanged { focused = it.isFocused }
            .menuKey(onLongClick)
            .combinedClickable(interactionSource = interactions, indication = null, onLongClick = onLongClick, onClick = onClick)
            .drawBehind {
                if (focused) drawRoundRect(PleyColors.Accent, size = Size(size.width, size.height), cornerRadius = CornerRadius(PleyShape.CardRadius.toPx()))
            }
            .padding(horizontal = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(
            Modifier.size(40.dp).background(if (focused) PleyColors.OnAccent.copy(alpha = 0.12f) else PleyColors.Surface3, PleyShape.Field),
            contentAlignment = Alignment.Center,
        ) {
            Icon(PleyIcons.Monitor, contentDescription = null, size = 22.dp, tint = ink)
        }
        Spacer(Modifier.width(16.dp))
        Column(Modifier.weight(1f)) {
            BasicText(row.name, style = PleyType.Tv.Title.copy(color = ink), maxLines = 1, overflow = TextOverflow.Ellipsis)
            val (line, danger) = when {
                connecting -> stringResource(R.string.tv_onboarding_connecting, row.address) to false
                row.unreachable -> stringResource(R.string.tv_onboarding_unreachable) to true
                else -> row.address to false
            }
            BasicText(
                line,
                style = PleyType.Tv.Caption.copy(color = if (danger && !focused) PleyColors.DangerText else soft),
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
        val (tag, warning) = when {
            row.lookalike -> stringResource(R.string.tv_onboarding_lookalike) to true
            row.unencrypted -> stringResource(R.string.tv_onboarding_unencrypted) to true
            row.found -> stringResource(R.string.tv_onboarding_on_network) to false
            else -> null to false
        }
        if (tag != null) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                if (!warning) Box(Modifier.size(8.dp).background(PleyColors.Success, PleyShape.Circle))
                BasicText(tag, style = PleyType.Tv.Caption.copy(color = if (warning && !focused) PleyColors.DangerText else soft))
            }
        }
    }
}

/** Under the buttons: searching, none found, the sweep's progress, what went wrong, or the Settings shortcut. */
@Composable
private fun ServersStatus(state: ServersUiState, onOpenSettings: () -> Unit, modifier: Modifier) {
    val live = modifier.semantics { liveRegion = LiveRegionMode.Polite }
    val danger = PleyType.Tv.Body.copy(color = PleyColors.DangerText)
    Column(live, verticalArrangement = Arrangement.spacedBy(8.dp)) {
        when {
            state.busy == ServersViewModel.DEMO -> StatusLine(stringResource(R.string.tv_onboarding_demo_starting), waiting = true)
            state.sweeping -> {
                val range = state.sweepRange
                StatusLine(
                    if (range != null) stringResource(R.string.tv_onboarding_sweeping, range) else stringResource(R.string.tv_onboarding_sweeping_network),
                    style = PleyType.Tv.Caption.copy(color = PleyColors.Muted),
                )
                SweepBar()
            }
            state.searching -> StatusLine(stringResource(R.string.tv_onboarding_searching), waiting = true)
            state.access == LocalAccess.Denied -> {
                BasicText(stringResource(R.string.tv_onboarding_access_denied), style = PleyType.Tv.Body.copy(color = PleyColors.Muted))
                TvTextButton(stringResource(R.string.tv_onboarding_open_settings), onClick = onOpenSettings)
            }
            state.searched && state.rows.isEmpty() && state.problem == null ->
                BasicText(stringResource(R.string.tv_onboarding_none_found), style = PleyType.Tv.Body.copy(color = PleyColors.Muted))
        }
        state.problem?.let { BasicText(problemText(it), style = danger) }
    }
}

/** The sweep's 160 × 4 bar, filling over the 5 s it takes at most. */
@Composable
private fun SweepBar() {
    val progress = remember { Animatable(0f) }
    LaunchedEffect(Unit) { progress.animateTo(1f, tween(SWEEP_MS, easing = LinearEasing)) }
    Box(
        Modifier.size(160.dp, 4.dp).drawBehind {
            val r = CornerRadius(size.height / 2f)
            drawRoundRect(PleyColors.Surface4, cornerRadius = r)
            drawRoundRect(PleyColors.Accent, size = Size(size.width * progress.value, size.height), cornerRadius = r)
        },
    )
}

/** What went wrong, in words. */
@Composable
internal fun problemText(problem: ServersProblem): String = when (problem) {
    is ServersProblem.Unreachable -> stringResource(R.string.tv_onboarding_problem_unreachable, problem.address, problem.reason.asString())
    is ServersProblem.NotPley -> stringResource(R.string.tv_onboarding_problem_not_pley, problem.address)
    is ServersProblem.Unencrypted -> stringResource(R.string.tv_onboarding_problem_unencrypted, problem.address)
    ServersProblem.BadAddress -> stringResource(R.string.tv_onboarding_problem_address)
    is ServersProblem.Failed -> UiText.Sentence(problem.text).asString()
}

private const val SWEEP_MS = 5_000
private val ROW_WIDTH = 520.dp

internal const val SERVERS_TAG = "pley.servers"
internal const val SERVER_ROW_TAG = "pley.servers.row."
internal const val ENTER_ADDRESS_TAG = "pley.servers.enter_address"
internal const val SEARCH_NETWORK_TAG = "pley.servers.search_network"
internal const val DEMO_TAG = "pley.servers.demo"
