package app.pley.android.ui.phone.onboarding

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.produceState
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.compose.LifecycleStartEffect
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import app.pley.android.common.UiText
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.PleyType
import app.pley.android.designsystem.icon.Icon
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.designsystem.text.asString
import app.pley.android.screens.PleyKey
import app.pley.android.screens.onboarding.LinkStatus
import app.pley.android.screens.onboarding.LinkUiState
import app.pley.android.screens.onboarding.LinkViewModel
import app.pley.android.screens.onboarding.go
import app.pley.android.screens.onboarding.onboardingSource
import app.pley.android.ui.phone.R
import app.pley.android.ui.phone.kit.BusyLine
import app.pley.android.ui.phone.kit.PleyTextButton
import app.pley.android.ui.phone.kit.TonalButton
import app.pley.android.ui.phone.shell.LocalPhoneNavigator
import kotlinx.coroutines.delay

/**
 * Signing a phone in with a code from another device (design spec §4.7:
 * the link screen stacked in one column), for phones without a browser,
 * or when the person asks for a code.
 */
@Composable
internal fun PhoneLinkScreen(server: String) {
    val navigator = LocalPhoneNavigator.current
    val given = LocalOnboardingSource.current
    val vm = viewModel(key = "link:$server") { LinkViewModel(given ?: onboardingSource(), server) }
    LaunchedEffect(vm) { vm.navigation.collect { navigator.go(it) } }
    LifecycleStartEffect(vm) {
        vm.started()
        onStopOrDispose { vm.stopped() }
    }
    val view = LocalView.current
    DisposableEffect(view) {
        view.keepScreenOn = true
        onDispose { view.keepScreenOn = false }
    }
    val state by vm.state.collectAsStateWithLifecycle()
    PhoneLinkContent(state, onNewCode = vm::newCode, onChangeServer = { navigator.restart(PleyKey.Servers) })
}

@Composable
internal fun PhoneLinkContent(state: LinkUiState, onNewCode: () -> Unit, onChangeServer: () -> Unit) {
    val status = state.status
    val code = state.code
    Box(Modifier.fillMaxSize().brandCorner().testTag(LINK_TAG)) {
        Column(
            Modifier
                .fillMaxSize()
                .verticalScroll(rememberScrollState())
                .statusBarsPadding()
                .navigationBarsPadding()
                .padding(horizontal = 24.dp),
        ) {
            Spacer(Modifier.height(72.dp))
            PleyWordmark()
            Spacer(Modifier.height(40.dp))
            if (status is LinkStatus.SignedIn) {
                Box(Modifier.size(64.dp).background(PleyColors.Success, PleyShape.Circle), contentAlignment = Alignment.Center) {
                    Icon(PleyIcons.Check, contentDescription = null, size = 32.dp, tint = PleyColors.Bg)
                }
                Spacer(Modifier.height(20.dp))
                Text(
                    status.name?.let { stringResource(R.string.onboarding_link_signed_in, it) } ?: stringResource(R.string.onboarding_link_signed_in_plain),
                    style = MaterialTheme.typography.headlineMedium,
                    color = PleyColors.Fg,
                    modifier = Modifier.semantics { liveRegion = LiveRegionMode.Polite },
                )
                return@Column
            }
            Text(stringResource(R.string.onboarding_link_title), style = MaterialTheme.typography.headlineMedium, color = PleyColors.Fg)
            Spacer(Modifier.height(20.dp))
            Step(1) {
                Column {
                    Text(stringResource(R.string.onboarding_link_go_to), style = MaterialTheme.typography.bodyLarge, color = PleyColors.Fg)
                    Text(
                        code?.address ?: "…",
                        style = MaterialTheme.typography.titleLarge,
                        color = PleyColors.Fg,
                        modifier = Modifier.semantics { code?.address?.let { contentDescription = it.toList().joinToString(" ") } },
                    )
                }
            }
            Spacer(Modifier.height(16.dp))
            Step(2) { Text(stringResource(R.string.onboarding_link_enter), style = MaterialTheme.typography.bodyLarge, color = PleyColors.Fg) }
            Spacer(Modifier.height(16.dp))
            val halves = code?.halves ?: listOf("····", "····")
            val label = code?.let { stringResource(R.string.onboarding_link_code_label, it.halves.joinToString(", ") { h -> h.toList().joinToString(" ") }) }
            Row(
                Modifier
                    .alpha(if (code == null || status == LinkStatus.Expired || status == LinkStatus.Starting) 0.4f else 1f)
                    .testTag(CODE_TAG)
                    .clearAndSetSemantics { if (label != null) contentDescription = label },
                verticalAlignment = Alignment.CenterVertically,
            ) {
                halves.forEachIndexed { i, half ->
                    if (i > 0) {
                        Box(Modifier.width(32.dp), contentAlignment = Alignment.Center) {
                            Box(Modifier.size(14.dp, 3.dp).background(PleyColors.Subtle, PleyShape.Control))
                        }
                    }
                    Box(
                        Modifier.height(64.dp).background(PleyColors.Surface2, PleyShape.Card).padding(horizontal = 14.dp),
                        contentAlignment = Alignment.Center,
                    ) {
                        Text(
                            half,
                            style = PleyType.Phone.DisplaySmall.copy(fontFamily = FontFamily.Monospace, fontSize = 34.sp, letterSpacing = 3.sp),
                            color = PleyColors.Fg,
                            maxLines = 1,
                        )
                    }
                }
            }
            Spacer(Modifier.height(16.dp))
            Text(stringResource(R.string.onboarding_link_other_device), style = MaterialTheme.typography.bodyMedium, color = PleyColors.Muted)
            if (state.twoStepRequired) {
                Spacer(Modifier.height(8.dp))
                Text(stringResource(R.string.onboarding_link_two_step_required), style = MaterialTheme.typography.bodyMedium, color = PleyColors.Muted)
            }
            Spacer(Modifier.height(24.dp))
            Status(status)
            if (status == LinkStatus.Declined || status == LinkStatus.Expired) {
                Spacer(Modifier.height(12.dp))
                TonalButton(stringResource(R.string.onboarding_link_new_code), onClick = onNewCode, icon = PleyIcons.RefreshCw, modifier = Modifier.testTag(NEW_CODE_TAG))
            }
            Spacer(Modifier.height(12.dp))
            PleyTextButton(stringResource(R.string.onboarding_change_server), onClick = onChangeServer)
        }
    }
}

@Composable
private fun Step(number: Int, content: @Composable () -> Unit) {
    Row(verticalAlignment = Alignment.Top) {
        Box(Modifier.size(28.dp).background(PleyColors.Surface3, PleyShape.Field).clearAndSetSemantics {}, contentAlignment = Alignment.Center) {
            Text(number.toString(), style = MaterialTheme.typography.labelLarge, color = PleyColors.Fg)
        }
        Spacer(Modifier.width(14.dp))
        Box(Modifier.padding(top = 3.dp)) { content() }
    }
}

@Composable
private fun Status(status: LinkStatus) {
    val live = Modifier.fillMaxWidth().semantics { liveRegion = LiveRegionMode.Polite }.testTag(STATUS_TAG)
    when (status) {
        LinkStatus.Starting -> BusyLine(stringResource(R.string.onboarding_link_getting), live)
        LinkStatus.Waiting -> BusyLine(stringResource(R.string.onboarding_link_waiting), live)
        is LinkStatus.Unreachable -> {
            val seconds by produceState(0, status) {
                while (true) {
                    value = ((status.retryAt - System.currentTimeMillis() + 999) / 1_000).toInt().coerceAtLeast(0)
                    delay(1_000)
                }
            }
            Text(stringResource(R.string.onboarding_link_retrying, seconds), style = MaterialTheme.typography.bodyMedium, color = PleyColors.DangerText, modifier = live)
        }
        LinkStatus.Declined -> Text(stringResource(R.string.onboarding_link_declined), style = MaterialTheme.typography.bodyMedium, color = PleyColors.DangerText, modifier = live)
        LinkStatus.Expired -> Text(stringResource(R.string.onboarding_link_expired), style = MaterialTheme.typography.bodyMedium, color = PleyColors.Muted, modifier = live)
        is LinkStatus.Failed -> Text(UiText.Sentence(status.text).asString(), style = MaterialTheme.typography.bodyMedium, color = PleyColors.DangerText, modifier = live)
        is LinkStatus.SignedIn -> Unit
    }
}

internal const val LINK_TAG = "pley.link"
internal const val CODE_TAG = "pley.link.code"
internal const val STATUS_TAG = "pley.link.status"
internal const val NEW_CODE_TAG = "pley.link.new_code"
