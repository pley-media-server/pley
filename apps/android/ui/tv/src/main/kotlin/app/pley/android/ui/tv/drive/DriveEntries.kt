package app.pley.android.ui.tv.drive

import androidx.compose.runtime.Composable
import androidx.compose.ui.platform.LocalContext
import androidx.navigation3.runtime.EntryProviderScope
import androidx.navigation3.runtime.NavKey
import app.pley.android.screens.PleyGraphOwner
import app.pley.android.screens.PleyKey
import app.pley.android.ui.tv.shell.Placeholder

/**
 * The USB drive's pages (Drive), added to the app's back stack: its top,
 * and each folder opened over the one before.
 */
fun EntryProviderScope<NavKey>.driveEntries() {
    entry<PleyKey.Drive> { key -> WithGraph(key) { TvDriveScreen(key.folder) } }
}

/**
 * [content], whose ViewModels come from the app's graph; where the
 * Application holds none (the shell's own tests), the shell's placeholder.
 */
@Composable
private fun WithGraph(key: NavKey, content: @Composable () -> Unit) {
    if (LocalContext.current.applicationContext is PleyGraphOwner) content() else Placeholder(key)
}
