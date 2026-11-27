package app.pley.android.ui.phone.account

import androidx.compose.runtime.Composable
import androidx.compose.ui.platform.LocalContext
import androidx.navigation3.runtime.EntryProviderScope
import androidx.navigation3.runtime.NavKey
import app.pley.android.screens.PleyGraphOwner
import app.pley.android.screens.PleyKey
import app.pley.android.ui.phone.diagnostics.DiagnosticsScreen
import app.pley.android.ui.phone.downloads.DownloadsScreen
import app.pley.android.ui.phone.settings.SettingsScreen
import app.pley.android.ui.phone.shell.Placeholder

/**
 * The phone's screens for your profile, history, downloads, settings and
 * diagnostics (Me, History, Downloads, Settings, Diagnostics), added to the
 * app's back stacks. Me is the Me tab's first screen; the others open
 * above it.
 */
fun EntryProviderScope<NavKey>.accountEntries() {
    entry<PleyKey.Me> { key -> WithGraph(key) { MeScreen() } }
    entry<PleyKey.History> { key -> WithGraph(key) { HistoryScreen() } }
    entry<PleyKey.Downloads> { key -> WithGraph(key) { DownloadsScreen() } }
    entry<PleyKey.Settings> { key -> WithGraph(key) { SettingsScreen() } }
    entry<PleyKey.Diagnostics> { key -> WithGraph(key) { DiagnosticsScreen() } }
    entry<PleyKey.Addresses> { key -> WithGraph(key) { AddressesScreen() } }
}

/**
 * [content], whose ViewModels come from the app's graph; where the
 * Application holds none (the shell's own tests), the shell's placeholder.
 */
@Composable
private fun WithGraph(key: NavKey, content: @Composable () -> Unit) {
    if (LocalContext.current.applicationContext is PleyGraphOwner) content() else Placeholder(key)
}
