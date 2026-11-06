package app.pley.android.ui.tv.onboarding

import androidx.compose.foundation.focusGroup
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.runtime.Composable
import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusDirection
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusProperties
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import androidx.tv.material3.Button
import androidx.tv.material3.ButtonDefaults
import androidx.tv.material3.Glow
import androidx.tv.material3.LocalContentColor
import androidx.tv.material3.Text
import app.pley.android.common.Language
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyFocus
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.PleyType
import app.pley.android.designsystem.icon.Icon
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.designsystem.text.asString
import app.pley.android.screens.settings.DeviceLanguageUiState
import app.pley.android.ui.tv.settings.TvChoice
import app.pley.android.ui.tv.settings.TvChoicePanel
import app.pley.android.screens.R as ScreensR

/**
 * The globe of the screens shown before signing in ("Choose your server",
 * the code screen), at the bottom left in [TvOnboardingFoot]:
 * [TvLanguageButton], a small plain globe with the language shown by its
 * two letters ("FR"), opens this device's choices in [TvLanguagePanel], a side
 * panel as Settings' choices open in: Automatic, English, Français,
 * Español, the current one checked. Picking one, or Back, closes it, and
 * focus comes back to the button.
 */
@Stable
internal class TvLanguagePicker {
    var open by mutableStateOf(false)
    val button = FocusRequester()
}

@Composable
internal fun rememberTvLanguagePicker(): TvLanguagePicker = remember { TvLanguagePicker() }

/**
 * The globe: smaller than the screen's buttons, 32 dp tall with a 16 dp
 * icon and Caption, no fill until it has focus; focused, the accent fill
 * and scale of every TV button.
 */
@Composable
internal fun TvLanguageButton(picker: TvLanguagePicker, state: DeviceLanguageUiState, modifier: Modifier = Modifier) {
    val label = stringResource(ScreensR.string.language_choose)
    Button(
        onClick = { picker.open = true },
        modifier = modifier.height(32.dp).focusRequester(picker.button).semantics { contentDescription = label }.testTag(LANGUAGE_TAG),
        shape = ButtonDefaults.shape(PleyShape.Control),
        colors = ButtonDefaults.colors(
            containerColor = Color.Transparent,
            contentColor = PleyColors.Fg,
            focusedContainerColor = PleyColors.Accent,
            focusedContentColor = PleyColors.OnAccent,
            pressedContainerColor = PleyColors.AccentBright,
            pressedContentColor = PleyColors.OnAccent,
        ),
        scale = ButtonDefaults.scale(focusedScale = PleyFocus.PILL_SCALE, pressedScale = 1f),
        glow = ButtonDefaults.glow(Glow.None, Glow.None, Glow.None),
        contentPadding = PaddingValues(horizontal = GLOBE_PADDING),
    ) {
        Icon(PleyIcons.Globe, contentDescription = null, size = 16.dp, tint = LocalContentColor.current)
        Spacer(Modifier.width(6.dp))
        Text(state.short, style = PleyType.Tv.Caption)
    }
}

/** The globe's room either side, which it hangs out by so its icon lines up with the screen's edge. */
private val GLOBE_PADDING = 10.dp

/**
 * The foot of the screens before signing in, at the bottom left of the
 * screen's root box: the globe, its icon in line with the screen's left
 * edge (the button's own room hangs out), then [content] beside it (a
 * status, a hint). The remote's Down reaches it, not Left or Right: Left
 * from a screen's first button stays put, as no rail is there to go to.
 */
@Composable
internal fun BoxScope.TvOnboardingFoot(
    picker: TvLanguagePicker,
    state: DeviceLanguageUiState,
    content: @Composable () -> Unit = {},
) {
    Row(
        Modifier
            .align(Alignment.BottomStart)
            .padding(start = 96.dp - GLOBE_PADDING, bottom = 24.dp)
            .widthIn(max = 600.dp - 96.dp)
            .focusProperties {
                onEnter = {
                    if (requestedFocusDirection == FocusDirection.Left || requestedFocusDirection == FocusDirection.Right) cancelFocusChange()
                }
            }
            .focusGroup(),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        TvLanguageButton(picker, state)
        Spacer(Modifier.width(6.dp))
        content()
    }
}

/** The globe's side panel, over the whole screen: last in the screen's root box. */
@Composable
internal fun TvLanguagePanel(picker: TvLanguagePicker, state: DeviceLanguageUiState, onPick: (Language?) -> Unit) {
    TvChoicePanel(
        open = picker.open,
        title = stringResource(ScreensR.string.language_choose),
        choices = state.choices.map { TvChoice(it, state.label(it).asString()) },
        selected = state.picked,
        onPick = onPick,
        onDismiss = { picker.open = false },
        returnTo = picker.button,
    )
}

internal const val LANGUAGE_TAG = "pley.onboarding.language"
