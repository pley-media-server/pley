package app.pley.android.designsystem.text

import androidx.compose.runtime.Composable
import androidx.compose.runtime.ReadOnlyComposable
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalResources
import app.pley.android.common.UiText
import app.pley.android.common.resolve
import java.util.Locale

/** These words in the language shown, as stringResource gives a resource's. */
@Composable
@ReadOnlyComposable
fun UiText.asString(): String = resolve(LocalResources.current)

/** These words one after the other, in the language shown: "4 albums · 40 tracks". */
@Composable
@ReadOnlyComposable
fun List<UiText>.asString(separator: String = " · "): String {
    val resources = LocalResources.current
    return joinToString(separator) { it.resolve(resources) }
}

/**
 * The locale the screen is shown in, for the dates, times and numbers it
 * writes: the language shown, in the device's region where the device is
 * in that language. Never Locale.getDefault(), which Android sets again as
 * the configuration changes.
 */
@Composable
@ReadOnlyComposable
fun shownLocale(): Locale = LocalConfiguration.current.locales[0]
