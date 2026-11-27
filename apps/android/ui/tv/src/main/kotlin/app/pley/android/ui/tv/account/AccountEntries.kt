package app.pley.android.ui.tv.account

import androidx.compose.runtime.Composable
import androidx.compose.ui.platform.LocalContext
import androidx.navigation3.runtime.EntryProviderScope
import androidx.navigation3.runtime.NavKey
import app.pley.android.screens.PleyGraphOwner
import app.pley.android.screens.PleyKey
import app.pley.android.ui.tv.diagnostics.TvDiagnosticsScreen
import app.pley.android.ui.tv.settings.Category
import app.pley.android.ui.tv.settings.TvAppearanceScreen
import app.pley.android.ui.tv.settings.TvSettingsScreen
import app.pley.android.ui.tv.shell.Placeholder

/**
 * The TV's screens for your profile, history, settings, appearance and
 * diagnostics (Me, History, Settings, Appearance, Diagnostics), added to the app's back stack.
 * The TV has no Me page of its own: Me is Settings opened on Profile.
 */
fun EntryProviderScope<NavKey>.accountEntries() {
    entry<PleyKey.Settings> { key -> WithGraph(key) { TvSettingsScreen() } }
    entry<PleyKey.Me> { key -> WithGraph(key) { TvSettingsScreen(start = Category.Profile) } }
    entry<PleyKey.History> { key -> WithGraph(key) { TvHistoryScreen() } }
    entry<PleyKey.Diagnostics> { key -> WithGraph(key) { TvDiagnosticsScreen() } }
    entry<PleyKey.Appearance> { TvAppearanceScreen() }
}

/**
 * [content], whose ViewModels come from the app's graph; where the
 * Application holds none (the shell's own tests), the shell's placeholder.
 */
@Composable
private fun WithGraph(key: NavKey, content: @Composable () -> Unit) {
    if (LocalContext.current.applicationContext is PleyGraphOwner) content() else Placeholder(key)
}
