package app.pley.android.ui.phone.onboarding

import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import app.pley.android.common.Language
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.icon.Icon
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.designsystem.text.asString
import app.pley.android.screens.settings.DeviceLanguageUiState
import app.pley.android.ui.phone.account.Choice
import app.pley.android.ui.phone.account.ChoiceSheet
import app.pley.android.screens.R as ScreensR

/**
 * The top bar of the screens shown before signing in: no title (the
 * screen's wordmark is under it), over the screen's own background, and
 * the globe at its start, which picks this device's [language], its icon
 * in line with the screen's 24 dp edge.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
internal fun OnboardingTopBar(
    language: DeviceLanguageUiState?,
    onLanguage: (Language?) -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
) {
    TopAppBar(
        title = {},
        // The bar keeps 4 dp at its start and the button 12 around its content: 8 more lines the icon up at 24.
        navigationIcon = { if (language != null) LanguageButton(language, onLanguage, enabled, Modifier.padding(start = 8.dp)) },
        colors = TopAppBarDefaults.topAppBarColors(containerColor = Color.Transparent, scrolledContainerColor = Color.Transparent),
        modifier = modifier,
    )
}

/**
 * The globe: the language shown by its two letters ("FR"), heard as
 * "Choose a language". It opens this device's choices in a sheet, as Settings'
 * choices open: Automatic, English, Français, Español, the current one
 * checked. A pick shows at once.
 */
@Composable
private fun LanguageButton(state: DeviceLanguageUiState, onPick: (Language?) -> Unit, enabled: Boolean, modifier: Modifier = Modifier) {
    var open by rememberSaveable { mutableStateOf(false) }
    val label = stringResource(ScreensR.string.language_choose)
    TextButton(
        onClick = { open = true },
        enabled = enabled,
        modifier = modifier.heightIn(min = 48.dp).semantics { contentDescription = label }.testTag(LANGUAGE_TAG),
        colors = ButtonDefaults.textButtonColors(contentColor = PleyColors.Fg),
    ) {
        Icon(PleyIcons.Globe, contentDescription = null, size = 20.dp, tint = if (enabled) PleyColors.Fg else PleyColors.Disabled)
        Spacer(Modifier.width(8.dp))
        Text(state.short, style = MaterialTheme.typography.labelLarge)
    }
    if (open) {
        ChoiceSheet(
            title = label,
            choices = state.choices.map { Choice(it, state.label(it).asString()) },
            selected = state.picked,
            onPick = onPick,
            onDismiss = { open = false },
        )
    }
}

internal const val LANGUAGE_TAG = "pley.onboarding.language"
