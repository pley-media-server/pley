package app.pley.android.ui.tv.onboarding

import androidx.compose.animation.core.Animatable
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusProperties
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyFocus
import app.pley.android.designsystem.PleyMotion
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.PleyType
import app.pley.android.designsystem.icon.Icon
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.designsystem.reducedMotion
import app.pley.android.ui.tv.R
import app.pley.android.ui.tv.kit.initialFocus
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

/** A key of a TV pad. */
@Immutable
internal sealed interface PadKey {
    data class Digit(val value: Int) : PadKey

    data object Delete : PadKey

    data object Ok : PadKey
}

/** The PIN pad's keys (design spec §2.9): 1-9, then ⌫, 0, OK. */
internal val PinKeys: List<List<PadKey>> = listOf(
    listOf(PadKey.Digit(1), PadKey.Digit(2), PadKey.Digit(3)),
    listOf(PadKey.Digit(4), PadKey.Digit(5), PadKey.Digit(6)),
    listOf(PadKey.Digit(7), PadKey.Digit(8), PadKey.Digit(9)),
    listOf(PadKey.Delete, PadKey.Digit(0), PadKey.Ok),
)

/** Where a pad's key flashes from: the remote's digit keys press keys too. */
internal class PadFlash {
    var key: PadKey? by mutableStateOf(null)

    /** Flashes [key] for 100 ms. */
    fun flash(key: PadKey) {
        this.key = key
    }
}

/**
 * A 3 × 4 grid of 64 dp round keys, 14 apart (design spec §2.9): [surface3][PleyColors.Surface3]
 * with tvPinDigit; focused, [accent][PleyColors.Accent] and 1.08. Each
 * press flashes its key with [surface4][PleyColors.Surface4] for 100 ms.
 * Initial focus is on 5. Disabled ([enabled] false), the keys are dimmed
 * and can't take focus.
 */
@Composable
internal fun TvKeypad(
    rows: List<List<PadKey>>,
    onKey: (PadKey) -> Unit,
    labels: PadLabels,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    flash: PadFlash = remember { PadFlash() },
    initial: PadKey? = PadKey.Digit(5),
    /** Asks for the initial key's focus again (after a crossfade took it elsewhere). */
    initialRequester: FocusRequester = remember { FocusRequester() },
    /** OK's, for a pad that gives it focus. */
    okRequester: FocusRequester? = null,
) {
    Column(
        modifier.graphicsLayer { alpha = if (enabled) 1f else 0.4f },
        verticalArrangement = Arrangement.spacedBy(KeyGap),
    ) {
        for (row in rows) {
            Row(horizontalArrangement = Arrangement.spacedBy(KeyGap)) {
                for (key in row) {
                    TvPadKey(
                        key = key,
                        labels = labels,
                        enabled = enabled,
                        flashing = flash.key == key,
                        initial = enabled && key == initial,
                        modifier = when {
                            key == initial -> Modifier.focusRequester(initialRequester)
                            key == PadKey.Ok && okRequester != null -> Modifier.focusRequester(okRequester)
                            else -> Modifier
                        },
                        onClick = {
                            flash.flash(key)
                            onKey(key)
                        },
                    )
                }
            }
        }
    }
    LaunchedEffect(flash.key) {
        if (flash.key != null) {
            delay(PleyMotion.PRESS_MS.toLong())
            flash.key = null
        }
    }
}

/** What screen readers hear for the keys that aren't digits. */
@Immutable
internal data class PadLabels(val digit: (Int) -> String, val delete: String, val ok: String)

/** What screen readers hear for the pads' keys. */
@Composable
internal fun padLabels(): PadLabels {
    val resources = LocalResources.current
    val delete = stringResource(R.string.tv_onboarding_delete)
    val ok = stringResource(R.string.tv_onboarding_ok)
    return remember(resources, delete, ok) { PadLabels({ n -> resources.getString(R.string.tv_onboarding_digit, n) }, delete, ok) }
}

/** One round key of a pad: 64 dp. */
@Composable
internal fun TvPadKey(
    key: PadKey,
    labels: PadLabels,
    enabled: Boolean,
    flashing: Boolean,
    initial: Boolean,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val reduced = reducedMotion()
    val scope = rememberCoroutineScope()
    val scale = remember { Animatable(1f) }
    var focused by remember { mutableStateOf(false) }
    val interactions = remember { MutableInteractionSource() }
    val description = when (key) {
        is PadKey.Digit -> labels.digit(key.value)
        PadKey.Delete -> labels.delete
        PadKey.Ok -> labels.ok
    }
    Box(
        modifier
            .size(KeySize)
            .testTag(padKeyTag(key))
            .semantics { contentDescription = description }
            .then(if (initial) Modifier.initialFocus() else Modifier)
            .focusProperties { canFocus = enabled }
            .onFocusChanged {
                focused = it.isFocused
                scope.launch {
                    scale.animateTo(if (it.isFocused) PleyFocus.ROUND_SCALE else 1f, if (it.isFocused) PleyMotion.focus(reduced) else PleyMotion.unfocus(reduced))
                }
            }
            .clickable(interactionSource = interactions, indication = null, enabled = enabled, role = Role.Button, onClick = onClick)
            .graphicsLayer {
                scaleX = scale.value
                scaleY = scale.value
            }
            .drawBehind {
                val fill = when {
                    flashing -> PleyColors.Surface4
                    focused -> PleyColors.Accent
                    else -> PleyColors.Surface3
                }
                drawRoundRect(fill, cornerRadius = CornerRadius(PleyShape.ControlRadius.toPx()))
            },
        contentAlignment = Alignment.Center,
    ) {
        val ink = if (focused && !flashing) PleyColors.OnAccent else PleyColors.Fg
        // The key says what it is (contentDescription): the glyph adds nothing to hear.
        Box(Modifier.clearAndSetSemantics {}, contentAlignment = Alignment.Center) {
            when (key) {
                is PadKey.Digit -> BasicText(key.value.toString(), style = PleyType.Tv.PinDigit.copy(color = ink))
                PadKey.Delete -> Icon(PleyIcons.Delete, contentDescription = null, size = 28.dp, tint = ink)
                PadKey.Ok -> Icon(PleyIcons.Check, contentDescription = null, size = 28.dp, tint = ink)
            }
        }
    }
}

/** The test tag of a pad key: "pley.pad.5", "pley.pad.delete", "pley.pad.ok". */
internal fun padKeyTag(key: PadKey): String = "pley.pad." + when (key) {
    is PadKey.Digit -> key.value.toString()
    PadKey.Delete -> "delete"
    PadKey.Ok -> "ok"
}

internal val KeySize = 64.dp
internal val KeyGap = 14.dp
