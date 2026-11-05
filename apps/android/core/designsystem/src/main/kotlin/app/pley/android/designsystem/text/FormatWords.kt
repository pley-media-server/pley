package app.pley.android.designsystem.text

import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalResources
import app.pley.android.common.formatWords
import app.pley.android.model.format.FormatWords

/**
 * The words of :core:model's format functions in the language shown,
 * their numbers in the locale shown ([shownLocale]): the same until the
 * configuration changes. Pass them last: `formatRuntime(ms, words)`.
 */
@Composable
fun formatWords(): FormatWords {
    val resources = LocalResources.current
    val configuration = LocalConfiguration.current
    return remember(resources, configuration) { formatWords(resources) }
}
