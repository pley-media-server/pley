package app.pley.android.ui.tv.onboarding

import androidx.compose.runtime.Composable
import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import app.pley.android.common.Language
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.designsystem.text.asString
import app.pley.android.screens.settings.DeviceLanguageUiState
import app.pley.android.ui.tv.kit.TvTextButton
import app.pley.android.ui.tv.settings.TvChoice
import app.pley.android.ui.tv.settings.TvChoicePanel
import app.pley.android.screens.R as ScreensR

/**
 * The globe of the screens shown before signing in ("Choose your server",
 * the code screen): [TvLanguageButton], a text button with the language
 * shown ("Français"), opens this device's choices in [TvLanguagePanel], a
 * side panel as Settings' choices open in: Automatic, English, Français,
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

@Composable
internal fun TvLanguageButton(picker: TvLanguagePicker, state: DeviceLanguageUiState, modifier: Modifier = Modifier) {
    val label = stringResource(ScreensR.string.language_choose)
    TvTextButton(
        label = state.value.asString(),
        onClick = { picker.open = true },
        icon = PleyIcons.Globe,
        modifier = modifier.focusRequester(picker.button).semantics { contentDescription = label }.testTag(LANGUAGE_TAG),
    )
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
