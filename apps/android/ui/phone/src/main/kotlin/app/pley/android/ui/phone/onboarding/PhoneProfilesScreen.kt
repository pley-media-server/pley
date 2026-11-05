package app.pley.android.ui.phone.onboarding

import androidx.activity.compose.BackHandler
import androidx.activity.compose.LocalActivity
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.ripple
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.createSavedStateHandle
import androidx.lifecycle.viewmodel.compose.viewModel
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.icon.Icon
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.designsystem.reducedMotion
import app.pley.android.screens.onboarding.ProfileTile
import app.pley.android.screens.onboarding.ProfilesStep
import app.pley.android.screens.onboarding.ProfilesUiState
import app.pley.android.screens.onboarding.ProfilesViewModel
import app.pley.android.screens.onboarding.SignInViewModel
import app.pley.android.screens.onboarding.go
import app.pley.android.screens.onboarding.onboardingSource
import app.pley.android.ui.phone.R
import app.pley.android.ui.phone.kit.Avatar
import app.pley.android.ui.phone.kit.ConfirmDialog
import app.pley.android.ui.phone.kit.EmptyState
import app.pley.android.ui.phone.kit.PinPad
import app.pley.android.ui.phone.kit.PleyTextButton
import app.pley.android.ui.phone.kit.PleyTopAppBar
import app.pley.android.ui.phone.kit.TonalButton
import app.pley.android.ui.phone.shell.LocalPhoneNavigator

/**
 * Switching profiles on a phone (design spec §4.7, opened from Me): the
 * household as a grid of 88 dp avatars, the current one ringed, PIN
 * profiles with a lock; the TV's PIN pad sized for touch (72 dp keys),
 * which a keyboard's digits, Backspace, Enter and Escape drive too. A
 * profile that needs its password is signed in to again in the browser.
 */
@Composable
internal fun PhoneProfilesScreen() {
    val navigator = LocalPhoneNavigator.current
    val activity = LocalActivity.current
    val given = LocalOnboardingSource.current
    val vm = viewModel(key = "profiles") { ProfilesViewModel(given ?: onboardingSource(), tv = false, gate = false) }
    val signIn = viewModel(key = "profiles-sign-in") { SignInViewModel(given ?: onboardingSource(), createSavedStateHandle()) }
    LaunchedEffect(vm) { vm.navigation.collect { navigator.go(it) } }
    LaunchedEffect(signIn) { signIn.navigation.collect { navigator.go(it) } }
    val state by vm.state.collectAsStateWithLifecycle()
    PhoneProfilesContent(
        state = state,
        onBack = navigator::back,
        onPick = vm::pick,
        onDigit = vm::digit,
        onDelete = vm::delete,
        onSubmit = vm::submit,
        onCancelPin = vm::backToPick,
        onRetry = vm::reload,
        onAnotherProfile = vm::signInAgain,
        onBrowser = {
            vm.backToPick()
            val server = state.server
            if (server != null) signIn.signIn(activity, server)
        },
    )
}

@Composable
internal fun PhoneProfilesContent(
    state: ProfilesUiState,
    onBack: () -> Unit,
    onPick: (Long) -> Unit,
    onDigit: (Int) -> Unit,
    onDelete: () -> Unit,
    onSubmit: () -> Unit,
    onCancelPin: () -> Unit,
    onRetry: () -> Unit,
    onAnotherProfile: () -> Unit,
    onBrowser: () -> Unit,
) {
    val reduced = reducedMotion()
    val step = state.step
    if (step is ProfilesStep.Pin) BackHandler(onBack = onCancelPin)
    Column(Modifier.fillMaxSize().testTag(PROFILES_TAG)) {
        PleyTopAppBar(
            title = stringResource(R.string.onboarding_profiles_title),
            onBack = if (step is ProfilesStep.Pin) onCancelPin else onBack,
        )
        AnimatedContent(
            targetState = step is ProfilesStep.Pin,
            transitionSpec = { fadeIn(tween(if (reduced) 0 else 200)) togetherWith fadeOut(tween(if (reduced) 0 else 200)) },
            label = "profiles",
            modifier = Modifier.weight(1f),
        ) { pin ->
            val current = state.step
            if (pin && current is ProfilesStep.Pin) {
                PhonePinPad(current, onDigit = onDigit, onDelete = onDelete, onSubmit = onSubmit, onCancel = onCancelPin)
            } else {
                ProfileGrid(state, onPick, onRetry, onAnotherProfile)
            }
        }
    }
    val browser = state.step as? ProfilesStep.Browser
    if (browser != null) {
        val profile = browser.profile
        ConfirmDialog(
            title = if (profile.id == 0L) stringResource(R.string.onboarding_another_profile) else stringResource(R.string.onboarding_password_title, profile.name),
            body = if (profile.id == 0L) stringResource(R.string.onboarding_another_body) else stringResource(R.string.onboarding_password_body, profile.name),
            confirmLabel = stringResource(R.string.onboarding_continue),
            onConfirm = onBrowser,
            onDismiss = onCancelPin,
        )
    }
}

@Composable
private fun ProfileGrid(state: ProfilesUiState, onPick: (Long) -> Unit, onRetry: () -> Unit, onAnotherProfile: () -> Unit) {
    if (state.profiles.isEmpty() && !state.loading) {
        Column(Modifier.fillMaxSize(), verticalArrangement = Arrangement.Center) {
            EmptyState(
                title = stringResource(R.string.onboarding_profiles_failed),
                body = state.error.orEmpty(),
                action = { TonalButton(stringResource(R.string.onboarding_retry), onClick = onRetry, icon = PleyIcons.RefreshCw) },
            )
            if (!state.canSwitch) {
                PleyTextButton(stringResource(R.string.onboarding_another_profile), onClick = onAnotherProfile, modifier = Modifier.align(Alignment.CenterHorizontally))
            }
        }
        return
    }
    LazyVerticalGrid(
        columns = GridCells.Adaptive(112.dp),
        contentPadding = PaddingValues(start = 16.dp, end = 16.dp, top = 24.dp, bottom = 24.dp),
        verticalArrangement = Arrangement.spacedBy(24.dp),
        horizontalArrangement = Arrangement.spacedBy(8.dp),
        modifier = Modifier.fillMaxSize(),
    ) {
        items(state.profiles, key = { it.id }, contentType = { "profile" }) { profile ->
            ProfileCell(profile, busy = state.busy == profile.id, enabled = state.canSwitch || profile.current, onClick = { onPick(profile.id) })
        }
        val error = state.error
        if (error != null) {
            item(key = "error", span = { androidx.compose.foundation.lazy.grid.GridItemSpan(maxLineSpan) }, contentType = "error") {
                Text(
                    error,
                    style = MaterialTheme.typography.bodyMedium,
                    color = PleyColors.DangerText,
                    textAlign = TextAlign.Center,
                    modifier = Modifier.semantics { liveRegion = LiveRegionMode.Polite },
                )
            }
        }
        if (!state.canSwitch) {
            item(key = "another", span = { androidx.compose.foundation.lazy.grid.GridItemSpan(maxLineSpan) }, contentType = "another") {
                Box(contentAlignment = Alignment.Center) {
                    TonalButton(stringResource(R.string.onboarding_another_profile), onClick = onAnotherProfile)
                }
            }
        }
    }
}

/** A profile: its 88 dp avatar (ringed in accent when it's the one in use, a lock when it has a PIN) and its name. */
@Composable
private fun ProfileCell(profile: ProfileTile, busy: Boolean, enabled: Boolean, onClick: () -> Unit) {
    val description = when {
        profile.current -> stringResource(R.string.onboarding_profile_current, profile.name)
        profile.locked -> stringResource(R.string.onboarding_profile_pin, profile.name)
        else -> profile.name
    }
    Column(
        Modifier
            .clickable(enabled = enabled, role = Role.Button, interactionSource = null, indication = ripple(bounded = false, radius = 56.dp), onClick = onClick)
            .testTag(PROFILE_TAG + profile.id)
            .clearAndSetSemantics { contentDescription = description }
            .padding(vertical = 4.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Box(
            Modifier
                .size(100.dp)
                .graphicsLayer { alpha = if (busy) 0.6f else 1f }
                .drawBehind {
                    if (profile.current) drawCircle(PleyColors.Accent, radius = size.minDimension / 2f - 1.dp.toPx(), style = Stroke(2.dp.toPx()))
                },
            contentAlignment = Alignment.Center,
        ) {
            Avatar(profile.name, color = profile.color, imageUrl = profile.avatarUrl, size = 88.dp)
            if (profile.locked) {
                Box(
                    Modifier
                        .align(Alignment.BottomEnd)
                        .offset((-4).dp, (-4).dp)
                        .size(28.dp)
                        .background(PleyColors.Bg, PleyShape.Circle)
                        .padding(2.dp)
                        .background(PleyColors.Surface3, PleyShape.Circle),
                    contentAlignment = Alignment.Center,
                ) {
                    Icon(PleyIcons.Lock, contentDescription = null, size = 14.dp, tint = PleyColors.Fg)
                }
            }
        }
        Spacer(Modifier.height(8.dp))
        Text(profile.name, style = MaterialTheme.typography.titleSmall, color = PleyColors.Fg, maxLines = 1, overflow = TextOverflow.Ellipsis)
    }
}

/**
 * "Who's watching?"'s PIN pad ([PinPad]): the profile's avatar and "Enter
 * Sam's PIN", the server's sentence under the dots after a refusal; locked
 * out, "Use another profile".
 */
@Composable
internal fun PhonePinPad(
    step: ProfilesStep.Pin,
    onDigit: (Int) -> Unit,
    onDelete: () -> Unit,
    onSubmit: () -> Unit,
    onCancel: () -> Unit,
) {
    PinPad(
        title = stringResource(R.string.onboarding_enter_pin, step.profile.name),
        digits = step.digits,
        onDigit = onDigit,
        onDelete = onDelete,
        onSubmit = onSubmit,
        onCancel = onCancel,
        lockedOutLabel = stringResource(R.string.onboarding_use_another),
        modifier = Modifier.fillMaxSize(),
        error = step.error,
        shakes = step.shakes,
        lockedOut = step.lockedOut,
        busy = step.busy,
    ) {
        Avatar(step.profile.name, color = step.profile.color, imageUrl = step.profile.avatarUrl, size = 64.dp)
    }
}

internal const val PROFILES_TAG = "pley.profiles"
internal const val PROFILE_TAG = "pley.profiles.profile."
