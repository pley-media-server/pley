package app.pley.android.ui.tv.onboarding

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.LifecycleStartEffect
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.PleyType
import app.pley.android.designsystem.icon.Icon
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.screens.PleyKey
import app.pley.android.screens.onboarding.LinkCode
import app.pley.android.screens.onboarding.LinkPurpose
import app.pley.android.screens.onboarding.LinkStatus
import app.pley.android.screens.onboarding.LinkUiState
import app.pley.android.screens.onboarding.LinkViewModel
import app.pley.android.screens.onboarding.go
import app.pley.android.screens.onboarding.onboardingSource
import app.pley.android.screens.onboarding.sentence
import app.pley.android.ui.tv.R
import app.pley.android.ui.tv.kit.TvPillButton
import app.pley.android.ui.tv.kit.TvTextButton
import app.pley.android.ui.tv.kit.initialFocus
import app.pley.android.ui.tv.shell.LocalTvNavigator
import kotlinx.coroutines.delay

/** The TV's way in (design spec §2.8): the code for [server], approved on a phone at /link. */
@Composable
internal fun TvLinkScreen(server: String) {
    val navigator = LocalTvNavigator.current
    val given = LocalOnboardingSource.current
    val vm = viewModel(key = "link:$server") { LinkViewModel(given ?: onboardingSource(), server) }
    LaunchedEffect(vm) { vm.navigation.collect { navigator.go(it) } }
    TvLinkRunner(vm)
    val state by vm.state.collectAsStateWithLifecycle()
    TvLinkContent(
        state = state,
        onNewCode = vm::newCode,
        backLabel = stringResource(R.string.tv_onboarding_change_server),
        onBack = { navigator.restart(PleyKey.Servers) },
    )
}

/**
 * A code that approves [profileId] on a phone (design spec §2.9: a profile
 * that needs its password), in the link screen's layout.
 */
@Composable
internal fun TvApproveContent(server: String, profileId: Long, title: String, body: String, onBack: () -> Unit) {
    val navigator = LocalTvNavigator.current
    val given = LocalOnboardingSource.current
    val vm = viewModel(key = "approve:$server:$profileId") { LinkViewModel(given ?: onboardingSource(), server, LinkPurpose.Profile(profileId)) }
    LaunchedEffect(vm) { vm.navigation.collect { navigator.go(it) } }
    TvLinkRunner(vm)
    val state by vm.state.collectAsStateWithLifecycle()
    TvLinkContent(
        state = state,
        onNewCode = vm::newCode,
        backLabel = stringResource(R.string.tv_onboarding_use_another),
        onBack = onBack,
        title = title,
        intro = body,
    )
}

/** Asks about the code while the screen is started only, and keeps the screen on while a code shows. */
@Composable
private fun TvLinkRunner(vm: LinkViewModel) {
    LifecycleStartEffect(vm) {
        vm.started()
        onStopOrDispose { vm.stopped() }
    }
    val view = LocalView.current
    DisposableEffect(view) {
        view.keepScreenOn = true
        onDispose { view.keepScreenOn = false }
    }
}

/**
 * The link screen's layout on the 960 × 540 canvas (design spec §2.8):
 * the steps and the code on the left from x = 96, the QR code and
 * [backLabel] on the right from x = 600. [backLabel] has focus, except
 * after a decline or once the code expired, when "Get a new code" has it.
 */
@Composable
internal fun TvLinkContent(
    state: LinkUiState,
    onNewCode: () -> Unit,
    backLabel: String,
    onBack: () -> Unit,
    title: String = stringResource(R.string.tv_onboarding_link_title),
    /** Under the title. Signing in, it says when the server requires two-step sign-in. */
    intro: String? = if (state.twoStepRequired) stringResource(R.string.tv_onboarding_two_step_required) else null,
) {
    val status = state.status
    val code = state.code
    val newCode = remember { FocusRequester() }
    val back = remember { FocusRequester() }
    val needsNewCode = status == LinkStatus.Declined || status == LinkStatus.Expired
    LaunchedEffect(needsNewCode) {
        if (needsNewCode) runCatching { newCode.requestFocus() } else runCatching { back.requestFocus() }
    }
    Box(Modifier.fillMaxSize().brandGlow().testTag(LINK_TAG)) {
        if (status is LinkStatus.SignedIn) {
            SignedIn(status.name)
        } else {
            Column(Modifier.padding(start = 96.dp, top = 56.dp).width(440.dp)) {
                PleyLogo()
                Spacer(Modifier.height(28.dp))
                BasicText(title, style = PleyType.Tv.PageTitle.copy(color = PleyColors.Fg), maxLines = 1)
                if (intro != null) {
                    Spacer(Modifier.height(6.dp))
                    BasicText(intro, style = PleyType.Tv.Body.copy(color = PleyColors.Muted), maxLines = 2)
                }
                Spacer(Modifier.height(if (intro != null) 10.dp else 18.dp))
                Step(1, stringResource(R.string.tv_onboarding_link_scan_step))
                Spacer(Modifier.height(12.dp))
                val goTo = stringResource(R.string.tv_onboarding_link_go_to)
                Step(2) {
                    val address = code?.address
                    BasicText(
                        buildAnnotatedString {
                            append(goTo)
                            append(' ')
                            withStyle(SpanStyle(fontSize = PleyType.Tv.Section.fontSize, fontWeight = PleyType.Bold, color = PleyColors.Fg)) {
                                append(address ?: "…")
                            }
                        },
                        style = PleyType.Tv.BodyLarge.copy(color = PleyColors.Fg),
                        maxLines = 1,
                        modifier = Modifier.semantics {
                            if (address != null) contentDescription = "$goTo ${spelled(address)}"
                        },
                    )
                }
                Spacer(Modifier.height(12.dp))
                Step(3, stringResource(R.string.tv_onboarding_link_enter_step))
                Spacer(Modifier.height(12.dp))
                CodeFields(code, dimmed = status == LinkStatus.Expired || (status == LinkStatus.Starting && code != null))
                Spacer(Modifier.height(16.dp))
                if (needsNewCode) {
                    TvPillButton(
                        label = stringResource(R.string.tv_onboarding_new_code),
                        onClick = onNewCode,
                        icon = PleyIcons.RefreshCw,
                        modifier = Modifier.focusRequester(newCode).testTag(NEW_CODE_TAG),
                    )
                } else {
                    Notes()
                }
            }
            Status(status, Modifier.padding(start = 96.dp, top = 478.dp))
        }
        // Signed in, the code and the way back go: only the success shows.
        if (status !is LinkStatus.SignedIn) Column(
            Modifier.padding(start = 600.dp, top = 120.dp).width(268.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            // The white tile is the code's quiet zone: its modules as big as 4 modules of margin allow.
            Box(
                Modifier
                    .size(268.dp)
                    .alpha(if (status == LinkStatus.Expired) 0.4f else 1f)
                    .clip(PleyShape.Dialog)
                    .background(if (code != null) PleyColors.QrLight else PleyColors.Surface2)
                    .testTag(QR_TAG),
                contentAlignment = Alignment.Center,
            ) {
                if (code != null) QrCodeImage(code.qr, 268.dp, Modifier.clearAndSetSemantics {})
            }
            Spacer(Modifier.height(12.dp))
            BasicText(
                stringResource(R.string.tv_onboarding_link_scan),
                style = PleyType.Tv.Caption.copy(color = PleyColors.Muted, textAlign = TextAlign.Center),
            )
            Spacer(Modifier.height(18.dp))
            TvTextButton(
                label = backLabel,
                onClick = onBack,
                modifier = Modifier.focusRequester(back).initialFocus(enabled = !needsNewCode).testTag(BACK_TAG),
            )
        }
    }
}

/** A numbered step: a 28 dp [surface3][PleyColors.Surface3] circle with the number, then its words. */
@Composable
private fun Step(number: Int, text: String) = Step(number) {
    BasicText(text, style = PleyType.Tv.BodyLarge.copy(color = PleyColors.Fg), maxLines = 1)
}

@Composable
private fun Step(number: Int, content: @Composable () -> Unit) {
    Row(Modifier.height(28.dp), verticalAlignment = Alignment.CenterVertically) {
        Box(Modifier.size(28.dp).background(PleyColors.Surface3, PleyShape.Field).clearAndSetSemantics {}, contentAlignment = Alignment.Center) {
            BasicText(number.toString(), style = PleyType.Tv.Label.copy(color = PleyColors.Fg))
        }
        Spacer(Modifier.width(14.dp))
        content()
    }
}

/** The code in two 76 dp fields, "BCDF – GHJK" in tvCode; read out letter by letter. */
@Composable
private fun CodeFields(code: LinkCode?, dimmed: Boolean) {
    val halves = code?.halves ?: listOf("····", "····")
    val label = code?.let { stringResource(R.string.tv_onboarding_code_label, it.halves.joinToString(", ") { half -> spelled(half) }) }
    Row(
        Modifier
            .alpha(if (dimmed || code == null) 0.4f else 1f)
            .testTag(CODE_TAG)
            .clearAndSetSemantics { if (label != null) contentDescription = label },
        verticalAlignment = Alignment.CenterVertically,
    ) {
        halves.forEachIndexed { i, half ->
            if (i > 0) {
                Box(Modifier.width(52.dp), contentAlignment = Alignment.Center) {
                    Box(Modifier.size(20.dp, 4.dp).background(PleyColors.Subtle, PleyShape.Control))
                }
            }
            Box(
                Modifier.height(76.dp).background(PleyColors.Surface2, PleyShape.Card).padding(horizontal = 20.dp),
                contentAlignment = Alignment.Center,
            ) {
                BasicText(half, style = PleyType.Tv.Code.copy(color = PleyColors.Fg), maxLines = 1, softWrap = false)
            }
        }
    }
}

@Composable
private fun Notes() {
    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        val note = PleyType.Tv.Body.copy(color = PleyColors.Muted)
        BasicText(stringResource(R.string.tv_onboarding_link_same_wifi), style = note, maxLines = 1)
        BasicText(stringResource(R.string.tv_onboarding_link_other_device), style = note, maxLines = 1)
    }
}

/** The status line at y = 478: waiting (the breathing dot, no spinner), declined, expired, retrying, or what failed. */
@Composable
private fun Status(status: LinkStatus, modifier: Modifier) {
    val muted = PleyType.Tv.Label.copy(color = PleyColors.Muted)
    val danger = PleyType.Tv.Label.copy(color = PleyColors.DangerText)
    val live = modifier.semantics { liveRegion = LiveRegionMode.Polite }.testTag(STATUS_TAG)
    when (status) {
        LinkStatus.Starting -> StatusLine(stringResource(R.string.tv_onboarding_getting_code), live, waiting = true)
        LinkStatus.Waiting -> StatusLine(stringResource(R.string.tv_onboarding_waiting), live, waiting = true)
        is LinkStatus.Unreachable -> {
            val seconds by produceState(secondsUntil(status.retryAt), status) {
                while (true) {
                    value = secondsUntil(status.retryAt)
                    delay(1_000)
                }
            }
            StatusLine(stringResource(R.string.tv_onboarding_retrying, seconds), live, style = danger)
        }
        LinkStatus.Declined -> StatusLine(stringResource(R.string.tv_onboarding_declined), live, style = danger)
        LinkStatus.Expired -> StatusLine(stringResource(R.string.tv_onboarding_expired), live, style = muted)
        is LinkStatus.Failed -> StatusLine(sentence(status.message), live.width(440.dp), style = danger)
        is LinkStatus.SignedIn -> Unit
    }
}

/** For about half a second: a 64 dp success circle with a check, and "Signed in as Anna". */
@Composable
private fun SignedIn(name: String?) {
    Column(
        Modifier.padding(start = 96.dp, top = 200.dp).width(440.dp).semantics { liveRegion = LiveRegionMode.Polite },
        verticalArrangement = Arrangement.spacedBy(20.dp),
    ) {
        Box(Modifier.size(64.dp).background(PleyColors.Success, PleyShape.Dialog), contentAlignment = Alignment.Center) {
            Icon(PleyIcons.Check, contentDescription = null, size = 32.dp, tint = PleyColors.Bg)
        }
        BasicText(
            if (name != null) stringResource(R.string.tv_onboarding_signed_in, name) else stringResource(R.string.tv_onboarding_signed_in_plain),
            style = PleyType.Tv.PageTitle.copy(color = PleyColors.Fg),
            maxLines = 2,
        )
    }
}

private fun secondsUntil(at: Long): Int = ((at - System.currentTimeMillis() + 999) / 1_000).toInt().coerceAtLeast(0)

/** "BCDF" as "B C D F", so screen readers spell it. */
private fun spelled(text: String): String = text.toList().joinToString(" ")

internal const val LINK_TAG = "pley.link"
internal const val CODE_TAG = "pley.link.code"
internal const val STATUS_TAG = "pley.link.status"
internal const val NEW_CODE_TAG = "pley.link.new_code"
internal const val BACK_TAG = "pley.link.back"
internal const val QR_TAG = "pley.link.qr"
