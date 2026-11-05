package app.pley.android.ui.phone.kit

import android.view.KeyEvent as AndroidKeyEvent
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.focusable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.ripple
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.onKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.PleyType
import app.pley.android.designsystem.icon.Icon
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.designsystem.reducedMotion
import app.pley.android.ui.phone.R

/**
 * The PIN pad for touch (design spec §4.7), wherever a PIN is asked: a
 * [picture] (a profile's avatar, a device's icon), the [title] ("Enter
 * Sam's PIN"), what it is for ([body]), up to 8 dots, and a 3 × 4 grid of
 * 72 dp keys (1-9, ⌫, 0, OK), which a keyboard's digits, Backspace, Enter
 * and Escape drive too. [digits] typed are never shown; OK takes 4 at
 * least. A wrong PIN shakes ([shakes] counts them) and clears the dots,
 * with [error] under them. Locked out, the keys are off and [lockedOutLabel]
 * leads away ([onCancel]).
 */
@Composable
internal fun PinPad(
    title: String,
    digits: Int,
    onDigit: (Int) -> Unit,
    onDelete: () -> Unit,
    onSubmit: () -> Unit,
    onCancel: () -> Unit,
    lockedOutLabel: String,
    modifier: Modifier = Modifier,
    body: String? = null,
    error: String? = null,
    shakes: Int = 0,
    lockedOut: Boolean = false,
    busy: Boolean = false,
    picture: @Composable () -> Unit = {},
) {
    val reduced = reducedMotion()
    val shake = remember { Animatable(0f) }
    LaunchedEffect(shakes) {
        if (shakes == 0 || reduced) return@LaunchedEffect
        for (x in floatArrayOf(-6f, 6f, -6f, 6f, -6f, 0f)) shake.animateTo(x, tween(40, easing = LinearEasing))
    }
    val keys = remember { FocusRequester() }
    LaunchedEffect(Unit) { runCatching { keys.requestFocus() } }
    Column(
        modifier
            .focusRequester(keys)
            .onKeyEvent { event ->
                if (event.type != KeyEventType.KeyDown) return@onKeyEvent false
                when (val code = event.nativeKeyEvent.keyCode) {
                    in AndroidKeyEvent.KEYCODE_0..AndroidKeyEvent.KEYCODE_9 -> onDigit(code - AndroidKeyEvent.KEYCODE_0)
                    in AndroidKeyEvent.KEYCODE_NUMPAD_0..AndroidKeyEvent.KEYCODE_NUMPAD_9 -> onDigit(code - AndroidKeyEvent.KEYCODE_NUMPAD_0)
                    AndroidKeyEvent.KEYCODE_DEL -> onDelete()
                    AndroidKeyEvent.KEYCODE_ENTER, AndroidKeyEvent.KEYCODE_NUMPAD_ENTER -> onSubmit()
                    AndroidKeyEvent.KEYCODE_ESCAPE -> onCancel()
                    else -> return@onKeyEvent false
                }
                true
            }
            .focusable()
            .verticalScroll(rememberScrollState())
            .navigationBarsPadding()
            .padding(horizontal = 24.dp)
            .testTag(PIN_PAD_TAG),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Spacer(Modifier.height(16.dp))
        picture()
        Spacer(Modifier.height(16.dp))
        Text(title, style = MaterialTheme.typography.titleLarge, color = PleyColors.Fg, textAlign = TextAlign.Center)
        if (body != null) {
            Spacer(Modifier.height(6.dp))
            Text(body, style = MaterialTheme.typography.bodyMedium, color = PleyColors.Muted, textAlign = TextAlign.Center)
        }
        Spacer(Modifier.height(20.dp))
        val entered = pluralStringResource(R.plurals.onboarding_pin_entered, digits, digits)
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
            repeat(maxOf(4, digits)) { i -> Dot(filled = i < digits) }
        }
        Box(Modifier.fillMaxWidth().height(44.dp), contentAlignment = Alignment.Center) {
            error?.let {
                Text(
                    it,
                    style = MaterialTheme.typography.bodyMedium,
                    color = PleyColors.DangerText,
                    textAlign = TextAlign.Center,
                    modifier = Modifier.semantics { liveRegion = LiveRegionMode.Assertive }.testTag(PIN_ERROR_TAG),
                )
            }
        }
        val resources = LocalResources.current
        Column(
            Modifier.graphicsLayer { alpha = if (lockedOut) 0.4f else 1f },
            verticalArrangement = Arrangement.spacedBy(16.dp),
        ) {
            val rows = listOf(listOf("1", "2", "3"), listOf("4", "5", "6"), listOf("7", "8", "9"), listOf(DELETE, "0", OK))
            for (row in rows) {
                Row(horizontalArrangement = Arrangement.spacedBy(24.dp)) {
                    for (key in row) {
                        val label = when (key) {
                            DELETE -> resources.getString(R.string.onboarding_delete)
                            OK -> resources.getString(R.string.onboarding_ok)
                            else -> resources.getString(R.string.onboarding_digit, key.toInt())
                        }
                        PadKey(
                            key = key,
                            label = label,
                            enabled = !lockedOut && !busy && (key != OK || digits >= 4),
                            onClick = {
                                when (key) {
                                    DELETE -> onDelete()
                                    OK -> onSubmit()
                                    else -> onDigit(key.toInt())
                                }
                            },
                        )
                    }
                }
            }
        }
        Spacer(Modifier.height(16.dp))
        if (lockedOut) {
            TonalButton(lockedOutLabel, onClick = onCancel)
        }
        Spacer(Modifier.height(16.dp))
    }
}

@Composable
private fun PadKey(key: String, label: String, enabled: Boolean, onClick: () -> Unit) {
    Box(
        Modifier
            .size(72.dp)
            .background(PleyColors.Surface3, PleyShape.Circle)
            .clickable(enabled = enabled, role = Role.Button, interactionSource = null, indication = ripple(bounded = true), onClick = onClick)
            .testTag("pley.pad.$key")
            .clearAndSetSemantics { contentDescription = label }
            .graphicsLayer { alpha = if (enabled || key != OK) 1f else 0.38f },
        contentAlignment = Alignment.Center,
    ) {
        when (key) {
            DELETE -> Icon(PleyIcons.Delete, contentDescription = null, size = 28.dp, tint = PleyColors.Fg)
            OK -> Icon(PleyIcons.Check, contentDescription = null, size = 28.dp, tint = PleyColors.Fg)
            else -> Text(key, style = PleyType.Tv.PinDigit, color = PleyColors.Fg)
        }
    }
}

@Composable
private fun Dot(filled: Boolean) {
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

private const val DELETE = "delete"
private const val OK = "ok"

internal const val PIN_PAD_TAG = "pley.pin"
internal const val PIN_DOTS_TAG = "pley.pin.dots"
internal const val PIN_ERROR_TAG = "pley.pin.error"
