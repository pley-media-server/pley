package app.pley.android.ui.tv.onboarding

import androidx.activity.compose.BackHandler
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.focusGroup
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyFocus
import app.pley.android.designsystem.PleyMotion
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.PleyType
import app.pley.android.designsystem.icon.Icon
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.designsystem.image.Artwork
import app.pley.android.designsystem.image.ArtworkImage
import app.pley.android.designsystem.reducedMotion
import app.pley.android.screens.onboarding.PinEntry
import app.pley.android.screens.onboarding.ProfileGate
import app.pley.android.screens.onboarding.ProfileTile
import app.pley.android.screens.onboarding.ProfilesStep
import app.pley.android.screens.onboarding.ProfilesUiState
import app.pley.android.screens.onboarding.ProfilesViewModel
import app.pley.android.screens.onboarding.go
import app.pley.android.screens.onboarding.onboardingSource
import app.pley.android.ui.tv.R
import app.pley.android.ui.tv.kit.TvPillButton
import app.pley.android.ui.tv.kit.TvTextButton
import app.pley.android.ui.tv.kit.digitKeys
import app.pley.android.ui.tv.kit.HeldEnds
import app.pley.android.ui.tv.kit.heldKeyStaysIn
import app.pley.android.ui.tv.kit.initialFocus
import app.pley.android.ui.tv.shell.LocalTvNavigator
import androidx.compose.ui.input.key.onPreviewKeyEvent
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

/**
 * "Who's watching?" (design spec §2.9): one row of large avatars, focus
 * on the last profile used so one OK continues; the PIN pad for a
 * profile with a PIN; a new code for a profile that needs its password.
 * [pin]: a start as the profile in use, whose pad opens first.
 */
@Composable
internal fun TvProfilesScreen(pin: Boolean = false) {
    val navigator = LocalTvNavigator.current
    val given = LocalOnboardingSource.current
    val vm = viewModel { ProfilesViewModel(given ?: onboardingSource(), tv = true, gate = !ProfileGate.passed, pin = pin) }
    LaunchedEffect(vm) { vm.navigation.collect { navigator.go(it) } }
    val state by vm.state.collectAsStateWithLifecycle()
    TvProfilesContent(
        state = state,
        onPick = vm::pick,
        onDigit = vm::digit,
        onDelete = vm::delete,
        onSubmit = vm::submit,
        onBack = vm::backToPick,
        onRetry = vm::reload,
        onAnotherProfile = vm::signInAgain,
    )
}

@Composable
internal fun TvProfilesContent(
    state: ProfilesUiState,
    onPick: (Long) -> Unit,
    onDigit: (Int) -> Unit,
    onDelete: () -> Unit,
    onSubmit: () -> Unit,
    onBack: () -> Unit,
    onRetry: () -> Unit,
    onAnotherProfile: () -> Unit,
) {
    val reduced = reducedMotion()
    val step = state.step
    if (step !is ProfilesStep.Pick) BackHandler(onBack = onBack)
    // The pad replaces the picker with a 200 ms crossfade.
    AnimatedContent(
        targetState = step::class,
        transitionSpec = { fadeIn(tween(if (reduced) 0 else PleyMotion.OVERLAY_MS)) togetherWith fadeOut(tween(if (reduced) 0 else PleyMotion.OVERLAY_MS)) },
        label = "profiles",
    ) { kind ->
        val current = state.step
        when {
            kind == ProfilesStep.Pin::class && current is ProfilesStep.Pin ->
                TvPinPad(current, onDigit = onDigit, onDelete = onDelete, onSubmit = onSubmit, onAnother = onBack)
            kind == ProfilesStep.Approve::class && current is ProfilesStep.Approve -> {
                val server = state.server
                if (server != null) {
                    val profile = current.profile
                    TvApproveContent(
                        server = server,
                        profileId = profile.id,
                        title = stringResource(if (profile.id == 0L) R.string.tv_onboarding_another_profile else R.string.tv_onboarding_approve_title),
                        body = if (profile.id == 0L) {
                            stringResource(R.string.tv_onboarding_another_body)
                        } else {
                            stringResource(R.string.tv_onboarding_approve_body, profile.name)
                        },
                        onBack = onBack,
                    )
                }
            }
            else -> Picker(state, onPick, onRetry, onAnotherProfile)
        }
    }
}

/** The household in one centred row; the title at y = 110, the avatars from y = 204. */
@Composable
private fun Picker(state: ProfilesUiState, onPick: (Long) -> Unit, onRetry: () -> Unit, onAnotherProfile: () -> Unit) {
    Box(Modifier.fillMaxSize().testTag(PROFILES_TAG)) {
        BasicText(
            stringResource(R.string.tv_onboarding_whos_watching),
            style = PleyType.Tv.PageTitle.copy(color = PleyColors.Fg, textAlign = TextAlign.Center),
            modifier = Modifier.fillMaxWidth().padding(top = 110.dp),
        )
        val profiles = state.profiles
        if (profiles.isNotEmpty()) {
            LazyRow(
                modifier = Modifier.fillMaxWidth().padding(top = 204.dp - RowBleed).heldKeyStaysIn(HeldEnds.LeftRight).focusGroup(),
                horizontalArrangement = Arrangement.spacedBy(40.dp, Alignment.CenterHorizontally),
                contentPadding = PaddingValues(horizontal = 48.dp, vertical = RowBleed),
            ) {
                items(profiles, key = { it.id }, contentType = { "profile" }) { profile ->
                    ProfileChoice(
                        profile = profile,
                        busy = state.busy == profile.id,
                        initial = profile.id == state.focus,
                        onClick = { onPick(profile.id) },
                    )
                }
            }
        }
        Column(
            Modifier.fillMaxWidth().padding(top = 400.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            val error = state.error
            if (error != null) {
                BasicText(error, style = PleyType.Tv.Body.copy(color = PleyColors.DangerText, textAlign = TextAlign.Center), modifier = Modifier.width(560.dp))
            }
            if (profiles.isEmpty() && !state.loading) {
                TvPillButton(stringResource(R.string.tv_onboarding_retry), onClick = onRetry, icon = PleyIcons.RefreshCw, modifier = Modifier.initialFocus())
            }
            if (!state.canSwitch) {
                TvTextButton(
                    stringResource(R.string.tv_onboarding_another_profile),
                    onClick = onAnotherProfile,
                    modifier = Modifier.initialFocus(enabled = profiles.isEmpty() && state.loading),
                )
            }
        }
    }
}

/**
 * A profile: its 112 dp avatar and name. Focused, the avatar grows to 1.1
 * with the focus ring (a 3 dp gap); the name
 * turns bold, on the same baseline. A PIN profile wears a lock.
 */
@Composable
private fun ProfileChoice(profile: ProfileTile, busy: Boolean, initial: Boolean, onClick: () -> Unit) {
    val reduced = reducedMotion()
    val scope = rememberCoroutineScope()
    val progress = remember { Animatable(0f) }
    var focused by remember { mutableStateOf(false) }
    val hasPin = stringResource(R.string.tv_onboarding_has_pin, profile.name)
    Column(
        Modifier
            .width(AvatarSize)
            .testTag(PROFILE_TAG + profile.id)
            .semantics(mergeDescendants = true) {
                if (profile.locked) contentDescription = hasPin
            }
            .then(if (initial) Modifier.initialFocus() else Modifier)
            .onFocusChanged {
                focused = it.isFocused
                scope.launch { progress.animateTo(if (it.isFocused) 1f else 0f, if (it.isFocused) PleyMotion.focus(reduced) else PleyMotion.unfocus(reduced)) }
            }
            .clickable(interactionSource = remember { MutableInteractionSource() }, indication = null, role = Role.Button, onClick = onClick),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Box(
            Modifier
                .size(AvatarSize)
                .graphicsLayer {
                    val s = 1f + (PleyFocus.AVATAR_SCALE - 1f) * progress.value
                    scaleX = s
                    scaleY = s
                    alpha = if (busy) 0.6f else 1f
                }
                .drawWithContent {
                    drawContent()
                    val a = progress.value
                    if (a > 0f) {
                        val s = 1f + (PleyFocus.AVATAR_SCALE - 1f) * a
                        val offset = PleyFocus.RingOffset.toPx() / s
                        drawCircle(
                            PleyColors.AccentBright.copy(alpha = a),
                            radius = size.minDimension / 2f + offset,
                            style = Stroke(PleyFocus.RingStroke.toPx() / s),
                        )
                    }
                },
        ) {
            ProfileAvatar(profile, AvatarSize)
            if (profile.locked) {
                Box(
                    Modifier
                        .offset(AvatarSize * 0.86f - 14.dp, AvatarSize * 0.86f - 14.dp)
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
        Spacer(Modifier.height(12.dp))
        BasicText(
            profile.name,
            style = PleyType.Tv.Label.copy(
                color = PleyColors.Fg,
                fontWeight = if (focused) PleyType.Bold else PleyType.Medium,
                textAlign = TextAlign.Center,
            ),
            maxLines = 1,
        )
    }
}

/** A profile's picture, or its initial on its colour (the web's Avatar.tsx). Decoration: the name is said beside it. */
@Composable
internal fun ProfileAvatar(profile: ProfileTile, size: Dp, modifier: Modifier = Modifier) {
    val fill = remember(profile.name, profile.color) { ProfileColors.of(profile.name, profile.color) }
    Box(
        modifier.size(size).clip(PleyShape.Circle).background(fill).clearAndSetSemantics {},
        contentAlignment = Alignment.Center,
    ) {
        val url = profile.avatarUrl
        if (url != null) {
            Artwork(ArtworkImage(listOf(url)), profile.name, Modifier.size(size))
        } else {
            BasicText(
                profile.initial,
                style = PleyType.Tv.PageTitle.copy(
                    color = ProfileColors.textOn(fill),
                    fontSize = (size.value * 0.44f).sp,
                    lineHeight = (size.value * 0.5f).sp,
                ),
            )
        }
    }
}

/**
 * The PIN pad (design spec §2.9): the avatar, "Enter Anna's PIN", up to 8
 * dots, the 3 × 4 keypad with focus on 5, and a hint about the remote's
 * digit keys. A wrong PIN shakes the dots, clears them and shows the
 * server's sentence. A locked PIN dims the pad, and "Use another profile"
 * takes focus.
 */
@Composable
internal fun TvPinPad(
    step: ProfilesStep.Pin,
    onDigit: (Int) -> Unit,
    onDelete: () -> Unit,
    onSubmit: () -> Unit,
    onAnother: () -> Unit,
) {
    val reduced = reducedMotion()
    val flash = remember { PadFlash() }
    val shake = remember { Animatable(0f) }
    LaunchedEffect(step.shakes) {
        if (step.shakes == 0 || reduced) return@LaunchedEffect
        // 3 × 6 dp over 240 ms.
        for (x in floatArrayOf(-6f, 6f, -6f, 6f, -6f, 0f)) shake.animateTo(x, tween(40, easing = LinearEasing))
    }
    val another = remember { FocusRequester() }
    LaunchedEffect(step.lockedOut) { if (step.lockedOut) runCatching { another.requestFocus() } }
    // The picker leaves during the crossfade, and focus with it: 5 takes it back once it has gone.
    val five = remember { FocusRequester() }
    var touched by remember { mutableStateOf(false) }
    LaunchedEffect(Unit) {
        delay(if (reduced) 0L else PleyMotion.OVERLAY_MS + 50L)
        if (!touched && !step.lockedOut) runCatching { five.requestFocus() }
    }
    // A PIN typed with the remote's number keys is sent with OK: OK takes focus once it could be sent, rather than 5 typing a 5.
    val ok = remember { FocusRequester() }
    val digits by rememberUpdatedState(step.digits)
    val resources = LocalResources.current
    Box(
        Modifier
            .fillMaxSize()
            .onPreviewKeyEvent {
                touched = true
                false
            }
            .digitKeys(
                onDigit = { d ->
                    flash.flash(PadKey.Digit(d))
                    onDigit(d)
                    if (digits + 1 >= PinEntry.MIN && !step.lockedOut) runCatching { ok.requestFocus() }
                },
                onDelete = {
                    flash.flash(PadKey.Delete)
                    onDelete()
                },
            )
            .testTag(PIN_PAD_TAG),
    ) {
        Column(Modifier.fillMaxWidth().padding(top = 30.dp), horizontalAlignment = Alignment.CenterHorizontally) {
            ProfileAvatar(step.profile, 56.dp)
            Spacer(Modifier.height(14.dp))
            BasicText(stringResource(R.string.tv_onboarding_enter_pin, step.profile.name), style = PleyType.Tv.Title.copy(color = PleyColors.Fg))
            Spacer(Modifier.height(16.dp))
            val entered = pluralStringResource(R.plurals.tv_onboarding_pin_entered, step.digits, step.digits)
            Row(
                Modifier
                    .height(14.dp)
                    .graphicsLayer { translationX = shake.value * density }
                    .semantics {
                        contentDescription = entered
                        liveRegion = LiveRegionMode.Polite
                    }
                    .testTag(PIN_DOTS_TAG),
                horizontalArrangement = Arrangement.spacedBy(16.dp),
            ) {
                repeat(maxOf(4, step.digits)) { i -> PinDot(filled = i < step.digits) }
            }
            Box(Modifier.height(22.dp).width(560.dp), contentAlignment = Alignment.Center) {
                val error = step.error
                if (error != null) {
                    BasicText(
                        error,
                        style = PleyType.Tv.Body.copy(color = PleyColors.DangerText, textAlign = TextAlign.Center),
                        maxLines = 1,
                        modifier = Modifier.semantics { liveRegion = LiveRegionMode.Assertive }.testTag(PIN_ERROR_TAG),
                    )
                }
            }
            TvKeypad(
                rows = PinKeys,
                labels = remember(resources) {
                    PadLabels(
                        digit = { n -> resources.getString(R.string.tv_onboarding_digit, n) },
                        delete = resources.getString(R.string.tv_onboarding_delete),
                        ok = resources.getString(R.string.tv_onboarding_ok),
                    )
                },
                flash = flash,
                initialRequester = five,
                okRequester = ok,
                enabled = !step.lockedOut,
                onKey = { key ->
                    when (key) {
                        is PadKey.Digit -> onDigit(key.value)
                        PadKey.Delete -> onDelete()
                        PadKey.Ok -> onSubmit()
                    }
                },
            )
            Spacer(Modifier.height(16.dp))
            if (step.lockedOut) {
                TvTextButton(stringResource(R.string.tv_onboarding_use_another), onClick = onAnother, modifier = Modifier.focusRequester(another))
            } else {
                BasicText(stringResource(R.string.tv_onboarding_pin_hint), style = PleyType.Tv.Caption.copy(color = PleyColors.Muted))
            }
        }
    }
}

/** A 14 dp dot: an empty 2 dp [control][PleyColors.Control] ring, or filled with [fg][PleyColors.Fg]. */
@Composable
private fun PinDot(filled: Boolean) {
    Box(
        Modifier.size(14.dp).drawWithContent {
            if (filled) {
                drawCircle(PleyColors.Fg)
            } else {
                val w = 2.dp.toPx()
                drawCircle(PleyColors.Control, radius = size.minDimension / 2f - w / 2f, center = Offset(size.width / 2f, size.height / 2f), style = Stroke(w))
            }
        },
    )
}

/**
 * The web's profile colours (apps/web/src/components/Avatar.tsx): the
 * profile's own, or one picked from its name; dark text on light ones.
 */
internal object ProfileColors {
    private val palette = listOf(
        Color(0xFF38BDF8), Color(0xFFA78BFA), Color(0xFFF472B6), Color(0xFFFB923C),
        Color(0xFFFACC15), Color(0xFF4ADE80), Color(0xFF2DD4BF), Color(0xFFF87171),
    )

    fun of(name: String, chosen: String?): Color {
        chosen?.let(::parse)?.let { return it }
        var hash = 0L
        var i = 0
        while (i < name.length) {
            hash = (hash * 31 + name[i].code) and 0xFFFFFFFFL
            i += Character.charCount(name.codePointAt(i))
        }
        return palette[(hash % palette.size).toInt()]
    }

    /** The web's luminance test, on unconverted channels. */
    fun textOn(color: Color): Color =
        if (0.2126f * color.red + 0.7152f * color.green + 0.0722f * color.blue > 0.45f) PleyColors.Bg else Color.White

    private fun parse(hex: String): Color? {
        val digits = hex.removePrefix("#")
        if (digits.length != 3 && digits.length != 6 || digits.any { Character.digit(it, 16) < 0 }) return null
        val full = if (digits.length == 3) digits.map { "$it$it" }.joinToString("") else digits
        return Color(0xFF000000 or full.toLong(16))
    }
}

private val AvatarSize = 100.dp

/** Room above and below the row for a focused avatar's growth and ring. */
private val RowBleed = 32.dp

internal const val PROFILES_TAG = "pley.profiles"
internal const val PROFILE_TAG = "pley.profiles.profile."
internal const val PIN_PAD_TAG = "pley.profiles.pin"
internal const val PIN_DOTS_TAG = "pley.profiles.pin.dots"
internal const val PIN_ERROR_TAG = "pley.profiles.pin.error"
